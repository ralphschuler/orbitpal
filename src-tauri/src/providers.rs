//! Provider-neutral model and audio adapters. Requests only run on explicit app actions.
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, Response, Url};
use serde_json::{json, Value};
use std::{collections::BTreeMap, future::Future, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::Emitter;

const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_AUDIO: usize = 25 * 1024 * 1024;

pub struct Completion { pub text: String, pub tool_calls: Vec<Value> }

fn client() -> Result<Client, String> {
    Client::builder().connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180)).build().map_err(|_| "HTTP-Client konnte nicht erstellt werden.".into())
}
fn field<'a>(v: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty()).unwrap_or(fallback)
}
fn endpoint(config: &Value, path: &str) -> Result<String, String> {
    let provider = field(config, "provider", "compatible");
    let default = match provider { "ollama" => "http://localhost:11434/v1", "lmstudio" => "http://localhost:1234/v1", "anthropic" => "https://api.anthropic.com/v1", "elevenlabs" => "https://api.elevenlabs.io/v1", _ => "https://api.openai.com/v1" };
    let mut base = Url::parse(field(config, "baseUrl", default)).map_err(|_| "Ungültige Anbieter-URL.".to_string())?;
    if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() || !base.username().is_empty() || base.password().is_some() || base.query().is_some() || base.fragment().is_some() {
        return Err("Anbieter-URL muss eine HTTP(S)-Basis-URL ohne Zugangsdaten oder Query sein.".into());
    }
    let prefix = if base.path().trim_matches('/').is_empty() { "/v1" } else { base.path().trim_end_matches('/') };
    let joined = format!("{}/{}", prefix, path.trim_start_matches('/'));
    base.set_path(&joined);
    Ok(base.to_string())
}
fn authorize(request: RequestBuilder, key: &Option<String>, anthropic: bool) -> RequestBuilder {
    if let Some(key) = key.as_ref().filter(|key| !key.trim().is_empty()) {
        if anthropic { request.header("x-api-key", key) } else { request.bearer_auth(key) }
    } else { request }
}
async fn checked(response: Response, label: &str) -> Result<Response, String> {
    if response.status().is_success() { return Ok(response); }
    // Do not relay provider bodies: proxies can echo credentials or private request context.
    let code = response.status().as_u16();
    let hint = match code { 401 | 403 => "Zugangsdaten und Berechtigungen prüfen", 404 => "Basis-URL und Modell prüfen", 429 => "Anbieterlimit erreicht; später erneut versuchen", 400 | 422 => "Modellparameter oder Werkzeugunterstützung prüfen", _ => "Anbieter oder Netzwerk prüfen" };
    Err(format!("{label}: HTTP {code}; {hint}."))
}
async fn bounded_bytes(response: Response, limit: usize) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > limit as u64) { return Err("Anbieterantwort überschreitet die Größenbegrenzung.".into()); }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Anbieterantwort konnte nicht vollständig gelesen werden.".to_string())?;
        if body.len() + chunk.len() > limit { return Err("Anbieterantwort überschreitet die Größenbegrenzung.".into()); }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
async fn cancellable<T>(cancel: &AtomicBool, future: impl Future<Output = T>) -> Result<T, String> {
    tokio::pin!(future);
    loop {
        if cancel.load(Ordering::Relaxed) { return Err("Antwort abgebrochen.".into()); }
        tokio::select! { result = &mut future => return Ok(result), _ = tokio::time::sleep(Duration::from_millis(75)) => {} }
    }
}
fn emit_delta(app: &tauri::AppHandle, turn: &str, text: &str) {
    let _ = app.emit("cohost://agent", json!({"kind":"delta","turnId":turn,"text":text}));
}

/// Convert the common OpenAI-shaped conversation into Anthropic content blocks.
fn anthropic_messages(messages: Vec<Value>) -> (String, Vec<Value>) {
    let mut system = Vec::new();
    let mut output: Vec<Value> = Vec::new();
    for message in messages {
        let role = message["role"].as_str().unwrap_or("user");
        let text = message["content"].as_str().unwrap_or("");
        if role == "system" || role == "developer" { system.push(text.to_string()); continue; }
        let mut blocks = Vec::new();
        let mapped_role = if role == "assistant" { "assistant" } else { "user" };
        if role == "tool" {
            blocks.push(json!({"type":"tool_result","tool_use_id":message["tool_call_id"],"content":text}));
        } else {
            if !text.is_empty() { blocks.push(json!({"type":"text","text":text})); }
            if let Some(calls) = message["tool_calls"].as_array() {
                for call in calls {
                    let input = call["function"]["arguments"].as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or_else(|| json!({}));
                    blocks.push(json!({"type":"tool_use","id":call["id"],"name":call["function"]["name"],"input":input}));
                }
            }
        }
        if blocks.is_empty() { continue; }
        // Multiple tool results must remain together in the immediate user turn.
        if let Some(last) = output.last_mut().filter(|last| last["role"] == mapped_role) {
            if let Some(content) = last["content"].as_array_mut() { content.extend(blocks); }
        } else { output.push(json!({"role":mapped_role,"content":blocks})); }
    }
    (system.join("\n\n"), output)
}

pub async fn complete(config: &Value, messages: Vec<Value>, tools: Vec<Value>, api_key: Option<String>, app: tauri::AppHandle, turn_id: String, cancel: Arc<AtomicBool>) -> Result<Completion, String> {
    if matches!(field(config,"provider","compatible"), "claude-code" | "codex") {
        return crate::cli_provider::complete(config,messages,tools,api_key,app,turn_id,cancel).await;
    }
    let model = field(config, "model", "");
    if model.is_empty() { return Err("Bitte zuerst ein Dialogmodell auswählen.".into()); }
    let anthropic = field(config, "provider", "compatible") == "anthropic";
    let max_tokens = config["maxTokens"].as_u64().unwrap_or(512).clamp(32, 8192);
    let temperature = config["temperature"].as_f64().unwrap_or(0.8).clamp(0.0, if anthropic { 1.0 } else { 2.0 });
    let mut body = if anthropic {
        let (system, messages) = anthropic_messages(messages);
        json!({"model":model,"messages":messages,"system":system,"max_tokens":max_tokens,"temperature":temperature,"stream":true})
    } else { json!({"model":model,"messages":messages,"max_tokens":max_tokens,"temperature":temperature,"stream":true}) };
    let openai = !anthropic && (field(config, "provider", "compatible") == "openai" || Url::parse(&endpoint(config, "chat/completions")?).ok().and_then(|url| url.host_str().map(str::to_owned)).as_deref() == Some("api.openai.com"));
    if openai {
        // OpenAI's current parameter also supports reasoning tokens; max_tokens rejects o-series models.
        body.as_object_mut().unwrap().remove("max_tokens");
        body["max_completion_tokens"] = json!(max_tokens);
        if model.starts_with("gpt-5") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
            body.as_object_mut().unwrap().remove("temperature");
        }
    }
    // Advanced compatible servers can opt out of a sampling parameter they do not implement.
    if config["temperatureEnabled"].as_bool() == Some(false) { body.as_object_mut().unwrap().remove("temperature"); }
    if !tools.is_empty() && config["toolsEnabled"].as_bool().unwrap_or(true) {
        body["tools"] = if anthropic { Value::Array(tools.iter().map(|tool| {
            let f = tool.get("function").unwrap_or(tool);
            json!({"name":f["name"],"description":f["description"].as_str().unwrap_or(""),"input_schema":f.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object","properties":{}}))})
        }).collect()) } else { Value::Array(tools) };
    }
    let _ = app.emit("cohost://agent", json!({"kind":"status","turnId":turn_id,"status":"thinking"}));
    let mut request = client()?.post(endpoint(config, if anthropic { "messages" } else { "chat/completions" })?).json(&body);
    if anthropic { request = request.header("anthropic-version", "2023-06-01"); }
    let response = cancellable(&cancel, authorize(request, &api_key, anthropic).send()).await?.map_err(|_| "Dialoganbieter ist nicht erreichbar oder hat das Zeitlimit überschritten.".to_string())?;
    let response = checked(response, "Dialogmodell").await?;
    let mut completion = Completion { text: String::new(), tool_calls: Vec::new() };
    let mut calls: BTreeMap<usize, Value> = BTreeMap::new();
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut total = 0usize;
    let mut finished = false;
    while let Some(chunk) = cancellable(&cancel, stream.next()).await? {
        let chunk = chunk.map_err(|_| "Verbindung zum Dialogmodell wurde unterbrochen.".to_string())?;
        total += chunk.len();
        if total > MAX_RESPONSE { return Err("Dialogantwort überschreitet die Größenbegrenzung.".into()); }
        buffer.extend_from_slice(&chunk);
        // SSE is framed as lines. Parse only complete UTF-8 lines, even when network chunks split characters.
        while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            let line = std::str::from_utf8(&line).map_err(|_| "Anbieter lieferte ungültiges UTF-8.".to_string())?.trim();
            let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue; };
            if data.is_empty() { continue; }
            if data == "[DONE]" { finished = true; break; }
            let event: Value = serde_json::from_str(data).map_err(|_| "Ungültige Streaming-Antwort des Dialoganbieters.".to_string())?;
            if anthropic {
                match event["type"].as_str().unwrap_or("") {
                    "content_block_start" => {
                        let block = &event["content_block"];
                        if block["type"] == "tool_use" {
                            calls.insert(event["index"].as_u64().unwrap_or(0) as usize, json!({"id":block["id"],"type":"function","function":{"name":block["name"],"arguments":""}}));
                        } else if let Some(text) = block["text"].as_str().filter(|s| !s.is_empty()) {
                            completion.text.push_str(text); emit_delta(&app, &turn_id, text);
                        }
                    }
                    "content_block_delta" => {
                        let delta = &event["delta"];
                        if let Some(text) = delta["text"].as_str() { completion.text.push_str(text); emit_delta(&app, &turn_id, text); }
                        if let Some(fragment) = delta["partial_json"].as_str() {
                            if let Some(call) = calls.get_mut(&(event["index"].as_u64().unwrap_or(0) as usize)) {
                                let args = call["function"]["arguments"].as_str().unwrap_or("").to_string() + fragment;
                                call["function"]["arguments"] = json!(args);
                            }
                        }
                    }
                    "message_stop" => { finished = true; }
                    "error" => { return Err("Dialoganbieter meldet einen Streaming-Fehler. Anbieterlimit und Konfiguration prüfen.".into()); }
                    _ => {}
                }
            } else {
                if event.get("error").is_some() { return Err("Dialoganbieter meldet einen Streaming-Fehler.".into()); }
                if let Some(choice) = event["choices"].as_array().and_then(|choices| choices.first()) {
                    if !choice["finish_reason"].is_null() { finished = true; }
                    let delta = &choice["delta"];
                    if let Some(text) = delta["content"].as_str() { completion.text.push_str(text); emit_delta(&app, &turn_id, text); }
                    if let Some(fragments) = delta["tool_calls"].as_array() {
                        for fragment in fragments {
                            let index = fragment["index"].as_u64().unwrap_or(0) as usize;
                            if index > 63 { return Err("Zu viele Werkzeugaufrufe in einer Antwort.".into()); }
                            let call = calls.entry(index).or_insert_with(|| json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
                            for key in ["id", "name", "arguments"] {
                                let fragment_value = if key == "id" { &fragment["id"] } else { &fragment["function"][key] };
                                if let Some(part) = fragment_value.as_str() {
                                    let old = if key == "id" { call["id"].as_str() } else { call["function"][key].as_str() }.unwrap_or("").to_string();
                                    if key == "id" { call["id"] = json!(old + part); } else { call["function"][key] = json!(old + part); }
                                }
                            }
                        }
                    }
                }
            }
        }
        if finished { break; }
        if buffer.len() > 256 * 1024 { return Err("Streaming-Ereignis ist zu groß.".into()); }
    }
    if cancel.load(Ordering::Relaxed) { return Err("Antwort abgebrochen.".into()); }
    if !finished { return Err("Dialogantwort wurde vor dem Abschluss unterbrochen.".into()); }
    for mut call in calls.into_values() {
        if call["id"].as_str().unwrap_or("").is_empty() { call["id"] = json!(uuid::Uuid::new_v4().to_string()); }
        if call["function"]["arguments"] == "" { call["function"]["arguments"] = json!("{}"); }
        if call["function"]["name"].as_str().unwrap_or("").is_empty() { return Err("Dialogmodell lieferte einen Werkzeugaufruf ohne Namen.".into()); }
        let arguments = call["function"]["arguments"].as_str().and_then(|args| serde_json::from_str::<Value>(args).ok());
        if !arguments.is_some_and(|args| args.is_object()) { return Err("Dialogmodell lieferte unvollständige oder ungültige Werkzeugargumente.".into()); }
        completion.tool_calls.push(call);
    }
    if completion.text.is_empty() && completion.tool_calls.is_empty() { return Err("Das Dialogmodell hat eine leere Antwort geliefert.".into()); }
    Ok(completion)
}

pub async fn models(config: &Value, api_key: Option<String>) -> Result<Vec<String>, String> {
    if matches!(field(config,"provider","compatible"), "claude-code" | "codex") {
        return Err("Bei CLI-Anbietern den Modellnamen oder default eintragen. Die CLI verwaltet den Modellzugang.".into());
    }
    let anthropic = field(config, "provider", "compatible") == "anthropic";
    let mut request = client()?.get(endpoint(config, "models")?);
    if anthropic { request = request.header("anthropic-version", "2023-06-01"); }
    let response = authorize(request, &api_key, anthropic).send().await.map_err(|_| "Modellliste ist nicht erreichbar.".to_string())?;
    let body = bounded_bytes(checked(response, "Modellliste").await?, MAX_RESPONSE).await?;
    let data: Value = serde_json::from_slice(&body).map_err(|_| "Ungültige Modellliste.".to_string())?;
    let mut ids: Vec<String> = data["data"].as_array().ok_or_else(|| "Anbieter lieferte keine kompatible Modellliste.".to_string())?.iter().filter_map(|item| item["id"].as_str().map(str::to_string)).collect();
    ids.sort(); ids.dedup(); Ok(ids)
}

pub async fn synthesize(config: &Value, text: &str, api_key: Option<String>) -> Result<Value, String> {
    if text.trim().is_empty() || text.len() > 20_000 { return Err("Sprachtext ist leer oder zu lang (maximal 20.000 Bytes).".into()); }
    let provider = field(config, "provider", "system");
    if provider == "system" { return Err("Systemstimme wird in der Oberfläche abgespielt.".into()); }
    let voice = field(config, "voiceId", "");
    if voice.is_empty() { return Err("Bitte eine Stimme konfigurieren.".into()); }
    let request = if provider == "elevenlabs" {
        if !voice.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { return Err("Ungültige ElevenLabs Voice-ID.".into()); }
        let key = api_key.as_ref().filter(|key| !key.trim().is_empty()).ok_or_else(|| "ElevenLabs benötigt einen API-Key.".to_string())?;
        client()?.post(endpoint(config, &format!("text-to-speech/{voice}"))?).header("xi-api-key", key)
            .query(&[("output_format", "mp3_44100_128")])
            .json(&json!({"text":text,"model_id":field(config,"model","eleven_flash_v2_5")}))
    } else {
        // SpeechPlayer owns playback speed for every provider; applying rate here too would square it.
        authorize(client()?.post(endpoint(config, "audio/speech")?).json(&json!({"model":field(config,"model","tts-1"),"input":text,"voice":voice,"response_format":"mp3","speed":1.0})), &api_key, false)
    };
    let response = checked(request.send().await.map_err(|_| "Sprachanbieter ist nicht erreichbar.".to_string())?, "Sprachausgabe").await?;
    let mime = response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or("audio/mpeg").split(';').next().unwrap_or("audio/mpeg").to_string();
    if !mime.starts_with("audio/") && mime != "application/octet-stream" { return Err("Sprachanbieter lieferte kein Audio.".into()); }
    let audio = bounded_bytes(response, MAX_AUDIO).await?;
    if audio.is_empty() { return Err("Sprachanbieter lieferte leeres Audio.".into()); }
    Ok(json!({"audioBase64":STANDARD.encode(audio),"mimeType":if mime == "application/octet-stream" { "audio/mpeg" } else { &mime }}))
}

pub async fn transcribe(config: &Value, audio_base64: &str, mime_type: &str, api_key: Option<String>) -> Result<String, String> {
    crate::transcription::transcribe(config,audio_base64,mime_type,api_key).await
}
