//! Provider-neutral model and audio adapters. Requests only run on explicit app actions.
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, Response, Url};
use serde_json::{json, Value};
use std::{collections::BTreeMap, future::Future, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::Emitter;

const USAGE_TRAILER_GRACE:Duration=Duration::from_secs(2);
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_AUDIO: usize = 25 * 1024 * 1024;

pub struct Completion { pub text: String, pub tool_calls: Vec<Value> }

fn client() -> Result<Client, String> {
    Client::builder().retry(reqwest::retry::never()).redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180)).build().map_err(|_| "HTTP-Client konnte nicht erstellt werden.".into())
}
/// Coding CLIs accept their own default model when the selector is blank.
pub fn dialog_model_ready(config:&Value)->bool{!config["model"].as_str().unwrap_or_default().trim().is_empty()||matches!(config["provider"].as_str(),Some("codex"|"claude-code"))}
fn field<'a>(v: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(fallback)
}
fn endpoint(config:&Value,path:&str)->Result<String,String>{crate::api_connections::provider_endpoint(config,path)}

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

#[cfg(test)]
pub(crate) const FIXTURE_IMAGE:&str="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+X0ZQAAAAASUVORK5CYII=";

#[cfg(test)]
mod vision_tests{
    use super::*;
    fn conversation()->Vec<Value>{vec![json!({"role":"system","content":"Character persona"}),json!({"role":"user","content":"Look at the tool screenshot"}),json!({"role":"assistant","content":Value::Null,"tool_calls":[{"id":"shot","type":"function","function":{"name":"screenshot","arguments":"{}"}},{"id":"info","type":"function","function":{"name":"info","arguments":"{}"}}]}),json!({"role":"tool","tool_call_id":"shot","content":"Image attached below"}),json!({"role":"tool","tool_call_id":"info","content":"Other host result"}),json!({"role":"user","content":[{"type":"text","text":"Untrusted screenshot evidence"},{"type":"image_url","image_url":{"url":FIXTURE_IMAGE,"detail":"auto"}}]})]}
    #[test]
    fn every_compatible_adapter_transports_real_image_arrays_and_tool_ids(){
        for provider in ["openai","compatible","ollama","lmstudio"]{
            let messages=conversation();let (body,anthropic,vision)=completion_body(&json!({"provider":provider,"model":"fixture-vision-model","visionEnabled":true}),messages.clone(),Vec::new()).unwrap();
            assert!(!anthropic);assert!(vision);assert_eq!(body["messages"],json!(messages));assert_eq!(body["messages"][5]["content"][1]["image_url"]["url"],FIXTURE_IMAGE);
        }
    }
    #[test]
    fn anthropic_keeps_parallel_tool_results_before_native_image_blocks(){
        let (body,anthropic,vision)=completion_body(&json!({"provider":"anthropic","model":"fixture-vision-model","visionEnabled":true}),conversation(),Vec::new()).unwrap();
        assert!(anthropic&&vision);assert_eq!(body["system"],"Character persona");assert_eq!(body["messages"][1]["content"][0]["type"],"tool_use");
        let content=body["messages"][2]["content"].as_array().unwrap();assert_eq!(content[0]["tool_use_id"],"shot");assert_eq!(content[1]["tool_use_id"],"info");assert_eq!(content[3]["type"],"image");assert_eq!(content[3]["source"]["media_type"],"image/png");assert_eq!(content[3]["source"]["data"],FIXTURE_IMAGE.split_once(',').unwrap().1);
    }
    #[test]
    fn disabled_text_only_and_oversized_visual_context_fail_without_dropping_images(){
        assert!(completion_body(&json!({"provider":"compatible","model":"fixture","visionEnabled":false}),conversation(),Vec::new()).unwrap_err().contains("deaktiviert"));
        assert!(completion_body(&json!({"provider":"compatible","model":"fixture","inputModalities":["text"]}),conversation(),Vec::new()).unwrap_err().contains("keine Bilder"));
        let mut messages=conversation();for _ in 0..4{messages.push(messages[5].clone());}assert!(validate_vision_messages(&json!({"visionEnabled":true}),&messages).unwrap_err().contains("4 Bilder"));
        let mut remote=conversation();remote[5]["content"][1]["image_url"]["url"]=json!("https://example.invalid/never-fetch.png");assert!(validate_vision_messages(&json!({}),&remote).is_err());
    }
}

pub(crate) fn validate_vision_messages(config:&Value,messages:&[Value])->Result<bool,String>{
    let mut count=0usize;let mut total=0usize;
    for message in messages{for block in message["content"].as_array().into_iter().flatten(){
        if block["type"]!="image_url"{continue;}
        if config["visionEnabled"].as_bool()==Some(false){return Err("Bildübergabe ist für diesen Bot deaktiviert. Ohne Bilddaten wird keine Bildanalyse behauptet.".into());}
        if message["role"]!="user"{return Err("Bilddaten müssen als eigene Benutzernachricht nach den Werkzeugantworten übergeben werden.".into());}
        if config["inputModalities"].as_array().is_some_and(|values|!values.iter().any(|value|value=="image")){return Err("Das ausgewählte Modell unterstützt laut seinen Metadaten keine Bilder. Bitte ein Vision-Modell auswählen.".into());}
        let image=crate::vision::parse_data_uri(block["image_url"]["url"].as_str().ok_or("Bildblock enthält keine gültige Daten-URL")?)?;
        total=total.checked_add(image.bytes).ok_or("Bildkontext ist zu groß")?;count+=1;
        if count>4||total>12*1024*1024{return Err("Bildkontext überschreitet über alle Werkzeugrunden die Grenze von 4 Bildern bzw. 12 MiB pro Antwort. Es werden keine Bildbelege still entfernt.".into());}
    }}
    Ok(count>0)
}
fn anthropic_content(content:&Value)->Result<Vec<Value>,String>{
    if let Some(text)=content.as_str(){return Ok(if text.is_empty(){Vec::new()}else{vec![json!({"type":"text","text":text})]});}
    if content.is_null(){return Ok(Vec::new());}
    let mut result=Vec::new();
    for block in content.as_array().ok_or("Nachrichteninhalt muss Text oder eine Blockliste sein")?{
        match block["type"].as_str().unwrap_or_default(){
            "text"=>{let text=block["text"].as_str().ok_or("Textblock ist ungültig")?;if !text.is_empty(){result.push(json!({"type":"text","text":text}));}},
            "image_url"=>{let image=crate::vision::parse_data_uri(block["image_url"]["url"].as_str().ok_or("Bild-Daten-URL fehlt")?)?;result.push(json!({"type":"image","source":{"type":"base64","media_type":image.mime_type,"data":image.base64}}));},
            _=>return Err("Der Anthropic-Adapter unterstützt in Nachrichten nur Text- und Bildblöcke.".into()),
        }
    }
    Ok(result)
}
/// Convert the common OpenAI-shaped conversation into Anthropic content blocks.
fn anthropic_messages(messages: Vec<Value>) -> Result<(String, Vec<Value>),String> {
    let mut system = Vec::new();
    let mut output: Vec<Value> = Vec::new();
    for message in messages {
        let role = message["role"].as_str().unwrap_or("user");
        let content=anthropic_content(&message["content"])?;
        if role == "system" || role == "developer" {
            if content.iter().any(|block|block["type"]!="text"){return Err("Systemanweisungen dürfen keine Bildblöcke enthalten".into());}
            system.extend(content.iter().filter_map(|block|block["text"].as_str().map(str::to_owned)));continue;
        }
        let mut blocks = Vec::new();
        let mapped_role = if role == "assistant" { "assistant" } else { "user" };
        if role == "tool" {
            blocks.push(json!({"type":"tool_result","tool_use_id":message["tool_call_id"],"content":content}));
        } else {
            blocks.extend(content);
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
    Ok((system.join("\n\n"), output))
}

fn completion_body(config:&Value,messages:Vec<Value>,tools:Vec<Value>)->Result<(Value,bool,bool),String>{
    let has_images=validate_vision_messages(config,&messages)?;
    let model = field(config, "model", "");
    if model.is_empty() { return Err("Bitte zuerst ein Dialogmodell auswählen.".into()); }
    let anthropic = field(config, "provider", "compatible") == "anthropic";
    let max_tokens = config["maxTokens"].as_u64().unwrap_or(512).clamp(32, 8192);
    let temperature = config["temperature"].as_f64().unwrap_or(0.8).clamp(0.0, if anthropic { 1.0 } else { 2.0 });
    let mut body = if anthropic {
        let (system, messages) = anthropic_messages(messages)?;
        json!({"model":model,"messages":messages,"system":system,"max_tokens":max_tokens,"temperature":temperature,"stream":true})
    } else { json!({"model":model,"messages":messages,"max_tokens":max_tokens,"temperature":temperature,"stream":true}) };
    let openai = !anthropic && (field(config, "provider", "compatible") == "openai" || Url::parse(&endpoint(config, "chat/completions")?).ok().and_then(|url| url.host_str().map(str::to_owned)).as_deref() == Some("api.openai.com"));
    if openai {
        // OpenAI's current parameter also supports reasoning tokens; max_tokens rejects o-series models.
        body.as_object_mut().unwrap().remove("max_tokens");
        body["max_completion_tokens"] = json!(max_tokens);
        body["stream_options"] = json!({"include_usage":true});
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
    Ok((body,anthropic,has_images))
}
/// Deliberately coarse, documented whole-context reservation for exact pinned
/// snapshots on the official TLS endpoint. Unknown models, aliases, proxies and
/// coding CLI internals have no claimed hard bound. Includes images/framing;
/// adding the enforced output cap again errs conservatively.
fn token_upper(config:&Value)->Option<u64>{
    if matches!(field(config,"provider","compatible"),"codex"|"claude-code"|"anthropic"){return None;}
    let url=Url::parse(&endpoint(config,"chat/completions").ok()?).ok()?;
    if url.scheme()!="https"||url.host_str()!=Some("api.openai.com")||url.port_or_known_default()!=Some(443)||url.path()!="/v1/chat/completions"||!url.username().is_empty()||url.password().is_some()||url.query().is_some()||url.fragment().is_some(){return None;}
    let context=match field(config,"model",""){"gpt-4o-mini-2024-07-18"|"gpt-4o-2024-08-06"=>128_000,_=>return None};
    Some(context+config["maxTokens"].as_u64().unwrap_or(512).clamp(32,8192))
}
pub(crate) fn preflight_budget(store:&crate::storage::Store,context:&crate::usage_budget::Context,config:&Value)->Result<(),String>{store.preflight_usage(context,!matches!(field(config,"provider","compatible"),"codex"|"claude-code"),token_upper(config))}
fn dialog_cost(config:&Value,messages:&[Value])->Result<crate::usage_budget::Cost,String>{
    validate_vision_messages(config,messages)?;let mut count=0;let mut bytes=0;
    for block in messages.iter().flat_map(|m|m["content"].as_array().into_iter().flatten()).filter(|b|b["type"]=="image_url"){
        let image=crate::vision::parse_data_uri(block["image_url"]["url"].as_str().ok_or("Bild-Daten-URL fehlt")?)?;count+=1;bytes+=image.bytes as u64;
    }
    Ok(if matches!(field(config,"provider","compatible"),"codex"|"claude-code"){crate::usage_budget::Cost::cli_dialog(count,bytes)}else{crate::usage_budget::Cost::dialog(count,bytes,token_upper(config))})
}
pub async fn complete(config: &Value, messages: Vec<Value>, tools: Vec<Value>, api_key: Option<String>, app: tauri::AppHandle, turn_id: String, cancel: Arc<AtomicBool>, usage:(&crate::storage::Store,&crate::usage_budget::Context),authority:&crate::life_runtime::cloud::CloudAuthority) -> Result<Completion, String> {
    if cancel.load(Ordering::Relaxed){return Err("Antwort abgebrochen.".into());}
    let cost=dialog_cost(config,&messages)?;
    let (store,context)=usage;
    if !authority.matches(context,&cancel){return Err("Ungültige native Anfragefreigabe.".into());}
    let permit=authority.admit().await?;
    let operation=async {let mut observed=None;let result=async{permit.check()?;complete_inner(config,messages,tools,api_key,app,turn_id,cancel,&mut observed).await}.await;(result,observed)};
    store.metered_usage(context,cost,operation).await
}
async fn complete_inner(config: &Value, messages: Vec<Value>, tools: Vec<Value>, api_key: Option<String>, app: tauri::AppHandle, turn_id: String, cancel: Arc<AtomicBool>, observed:&mut Option<u64>) -> Result<Completion, String> {
    if matches!(field(config,"provider","compatible"), "claude-code" | "codex") {
        return crate::cli_provider::complete(config,messages,tools,api_key,app,turn_id,cancel).await;
    }
    let (body,anthropic,has_images)=completion_body(config,messages,tools)?;
    let _ = app.emit("cohost://agent", json!({"kind":"status","turnId":turn_id,"status":"thinking"}));
    let mut request = client()?.post(endpoint(config, if anthropic { "messages" } else { "chat/completions" })?).json(&body);
    if anthropic { request = request.header("anthropic-version", "2023-06-01"); }
    let response = cancellable(&cancel, authorize(request, &api_key, anthropic).send()).await?.map_err(|_| "Dialoganbieter ist nicht erreichbar oder hat das Zeitlimit überschritten.".to_string())?;
    if has_images&&matches!(response.status().as_u16(),400|415|422){return Err(format!("Bildanfrage wurde vom Dialoganbieter abgelehnt (HTTP {}). Das ausgewählte Modell und der Endpoint müssen Vision und die übergebenen Bildformate unterstützen; es wird keine reine Textanalyse als Bildanalyse ausgegeben.",response.status().as_u16()));}
    let response = checked(response, "Dialogmodell").await?;
    completion_stream(response.bytes_stream(),&cancel,anthropic,body["stream_options"]["include_usage"].as_bool()==Some(true),USAGE_TRAILER_GRACE,observed,|text|emit_delta(&app,&turn_id,text)).await
}

/// Stream parser independent of AppHandle. A completed choice must not wait
/// indefinitely for optional usage metadata or invalidate a complete response
/// when the receipt connection subsequently fails. The charge stays unknown.
fn terminal_reason(value:&Value)->bool{matches!(value.as_str(),Some("stop"|"length"|"tool_calls"|"content_filter"|"function_call"))}
async fn completion_stream<S,B,E>(stream:S,cancel:&AtomicBool,anthropic:bool,expect_usage:bool,grace:Duration,observed:&mut Option<u64>,mut emit:impl FnMut(&str))->Result<Completion,String>
where S:futures_util::Stream<Item=Result<B,E>>,B:AsRef<[u8]> {
    let mut completion = Completion { text: String::new(), tool_calls: Vec::new() };
    let mut calls: BTreeMap<usize, Value> = BTreeMap::new();
    futures_util::pin_mut!(stream);
    let mut buffer: Vec<u8> = Vec::new();
    let mut total = 0usize;
    let mut finished = false;
    let mut choice_finished = false;
    let mut usage_deadline=None;
    loop {
        let next=if let Some(deadline)=usage_deadline{match cancellable(cancel,tokio::time::timeout_at(deadline,stream.next())).await?{Ok(next)=>next,Err(_)=>break}}else{cancellable(cancel,stream.next()).await?};
        let Some(chunk)=next else{break;};
        let chunk=match chunk{Ok(chunk)=>chunk,Err(_) if choice_finished=>break,Err(_)=>return Err("Verbindung zum Dialogmodell wurde unterbrochen.".into())};
        let chunk=chunk.as_ref();
        total += chunk.len();
        if total > MAX_RESPONSE {if choice_finished{break;}return Err("Dialogantwort überschreitet die Größenbegrenzung.".into());}
        buffer.extend_from_slice(chunk);
        // SSE is framed as lines. Parse only complete UTF-8 lines, even when network chunks split characters.
        while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            let line=match std::str::from_utf8(&line){Ok(line)=>line.trim(),Err(_) if choice_finished=>{finished=true;break;},Err(_)=>return Err("Anbieter lieferte ungültiges UTF-8.".into())};
            let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue; };
            if data.is_empty() { continue; }
            if data == "[DONE]" { finished = true; break; }
            let event:Value=match serde_json::from_str(data){Ok(event)=>event,Err(_) if choice_finished=>{finished=true;break;},Err(_)=>return Err("Ungültige Streaming-Antwort des Dialoganbieters.".into())};
            // Partial/running usage must never refund a possibly-sent upper.
            // A receipt is final only with/after a terminal choice, either on
            // that choice or in the standard empty-choices usage trailer.
            let terminal=choice_finished||event["choices"].as_array().and_then(|c|c.first()).is_some_and(|c|terminal_reason(&c["finish_reason"]));
            let receipt_shape=event["choices"].as_array().is_some_and(|c|c.is_empty()||c.first().is_some_and(|c|terminal_reason(&c["finish_reason"])));
            if !anthropic&&terminal&&receipt_shape { if let Some(usage)=validated_usage(&event["usage"]){*observed=Some(usage);if choice_finished{finished=true;}} }
            // The terminal choice is immutable. During the receipt grace we
            // accept only final usage, never extra text/tools or new errors.
            if choice_finished{if finished{break;}continue;}
            if anthropic {
                match event["type"].as_str().unwrap_or("") {
                    "content_block_start" => {
                        let block = &event["content_block"];
                        if block["type"] == "tool_use" {
                            calls.insert(event["index"].as_u64().unwrap_or(0) as usize, json!({"id":block["id"],"type":"function","function":{"name":block["name"],"arguments":""}}));
                        } else if let Some(text) = block["text"].as_str().filter(|s| !s.is_empty()) {
                            completion.text.push_str(text); emit(text);
                        }
                    }
                    "content_block_delta" => {
                        let delta = &event["delta"];
                        if let Some(text) = delta["text"].as_str() { completion.text.push_str(text); emit(text); }
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
                    // Official usage arrives in a separate empty-choices event
                    // before DONE. Continue bounded reading beyond finish_reason.
                    if terminal_reason(&choice["finish_reason"]) {
                        choice_finished = true;
                        if !expect_usage||observed.is_some(){finished=true;}else if usage_deadline.is_none(){usage_deadline=Some(tokio::time::Instant::now()+grace);}
                    }
                    let delta = &choice["delta"];
                    if let Some(text) = delta["content"].as_str() { completion.text.push_str(text); emit(text); }
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
            if finished{break;}
        }
        if finished { break; }
        if buffer.len() > 256 * 1024 {if choice_finished{break;}return Err("Streaming-Ereignis ist zu groß.".into());}
    }
    if cancel.load(Ordering::Relaxed) { return Err("Antwort abgebrochen.".into()); }
    if !finished&&!choice_finished { return Err("Dialogantwort wurde vor dem Abschluss unterbrochen.".into()); }
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

#[cfg(test)]
pub async fn synthesize_metered(config:&Value,text:&str,api_key:Option<String>,store:&crate::storage::Store,context:&crate::usage_budget::Context,cancel:Arc<AtomicBool>)->Result<Value,String>{
    let authority=crate::life_runtime::cloud::CloudAuthority::fixture(context,cancel.clone());
    synthesize_guarded(config,text,api_key,store,context,cancel,&authority).await
}
pub async fn synthesize_guarded(config:&Value,text:&str,api_key:Option<String>,store:&crate::storage::Store,context:&crate::usage_budget::Context,cancel:Arc<AtomicBool>,authority:&crate::life_runtime::cloud::CloudAuthority)->Result<Value,String>{
    if cancel.load(Ordering::Relaxed){return Err("Sprachausgabe wurde abgebrochen".into());}
    if text.trim().is_empty()||text.len()>20_000{return Err("Sprachtext ist leer oder zu lang (maximal 20.000 Bytes).".into());}
    if !authority.matches(context,&cancel){return Err("Ungültige native Anfragefreigabe.".into());}
    let permit=authority.admit().await?;
    // Count the exact sent Unicode characters, including provider audio tags.
    store.metered_usage(context,crate::usage_budget::Cost::tts(text.chars().count() as u64),async{let result=async{permit.check()?;cancellable(&cancel,synthesize(config,text,api_key)).await.and_then(|result|result)}.await;(result,None)}).await
}

fn validated_usage(value:&Value)->Option<u64>{
    let input=value["prompt_tokens"].as_u64()?;let output=value["completion_tokens"].as_u64()?;let total=value["total_tokens"].as_u64()?;
    (input.checked_add(output)==Some(total)&&total<=1_000_000_000).then_some(total)
}

pub async fn transcribe(config: &Value, audio_base64: &str, mime_type: &str, api_key: Option<String>) -> Result<String, String> {
    crate::transcription::transcribe(config,audio_base64,mime_type,api_key).await
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use crate::{storage::Store,usage_budget::{Context,Origin,Limits}};
    #[test]
    fn hard_token_bounds_require_exact_snapshots_and_official_tls_destination(){
        for model in ["gpt-4o-mini-2024-07-18","gpt-4o-2024-08-06"]{
            let config=json!({"provider":"openai","model":model,"maxTokens":512});assert_eq!(token_upper(&config),Some(128_512));
            let(body,_,_)=completion_body(&config,vec![json!({"role":"user","content":"fixture"})],vec![]).unwrap();assert_eq!(body["max_completion_tokens"],512);assert_eq!(body["stream_options"]["include_usage"],true);
            for base in ["http://api.openai.com/v1","https://api.openai.com:444/v1","https://api.openai.com/v2","https://proxy.invalid/v1","https://api.openai.com.attacker.invalid/v1","https://user:secret@api.openai.com/v1","https://api.openai.com/v1?proxy=true"]{
                let mut config=config.clone();config["baseUrl"]=json!(base);assert_eq!(token_upper(&config),None,"{base}");
            }
        }
        for provider in ["codex","claude-code","anthropic"]{assert_eq!(token_upper(&json!({"provider":provider,"model":"gpt-4o-2024-08-06"})),None);}
        for model in ["default","gpt-4o","gpt-4o-mini","gpt-5","unknown-2026-01-01"]{assert_eq!(token_upper(&json!({"provider":"openai","model":model})),None);}
        assert_eq!(token_upper(&json!({"provider":"compatible","model":"gpt-4o-2024-08-06","maxTokens":999_999})),Some(136_192));
    }
    #[test]
    fn receipts_require_integer_consistent_bounded_usage_and_images_are_recounted_per_round(){
        assert_eq!(validated_usage(&json!({"prompt_tokens":12,"completion_tokens":3,"total_tokens":15})),Some(15));
        for invalid in [json!({}),json!({"prompt_tokens":-1,"completion_tokens":3,"total_tokens":2}),json!({"prompt_tokens":1,"completion_tokens":3,"total_tokens":2}),json!({"prompt_tokens":1.5,"completion_tokens":3,"total_tokens":4.5}),json!({"prompt_tokens":1_000_000_000,"completion_tokens":1,"total_tokens":1_000_000_001})]{assert_eq!(validated_usage(&invalid),None);}
        let messages=vec![json!({"role":"user","content":[{"type":"image_url","image_url":{"url":FIXTURE_IMAGE}}]})];
        let config=json!({"provider":"openai","model":"gpt-4o-2024-08-06","visionEnabled":true});
        let first=dialog_cost(&config,&messages).unwrap();let second=dialog_cost(&config,&messages).unwrap();assert_eq!(first.images+second.images,2);assert_eq!(first.image_bytes,crate::vision::parse_data_uri(FIXTURE_IMAGE).unwrap().bytes as u64);assert!(first.direct_request);
        let cli=dialog_cost(&json!({"provider":"codex","model":"default","visionEnabled":true}),&messages).unwrap();assert!(!cli.direct_request);assert_eq!(cli.token_upper,None);assert_eq!(cli.images,1);
    }
    #[tokio::test]
    async fn global_model_catalogue_connection_uses_actual_custom_route_and_auth_header(){
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        use crate::{api_connections::{AuthStyle,Service},storage::api_connection_store::NewConnection};
        for provider in ["compatible","anthropic"]{
            let root=std::env::temp_dir().join(format!("orbitpal-global-models-{}",uuid::Uuid::new_v4()));let store=Store::new(root.clone()).unwrap();let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base=format!("http://{}/tenant/v1",listener.local_addr().unwrap());
            store.set_secret("synthetic-catalogue-item","synthetic-catalogue-marker").unwrap();let style=if provider=="anthropic"{AuthStyle::Anthropic}else{AuthStyle::Bearer};let created=store.create_api_connection(0,NewConnection{label:"Catalogue fixture".into(),service:Service::Compatible,destination:base.clone(),auth_style:style,existing_secret_id:Some("synthetic-catalogue-item".into())}).unwrap();
            let config=json!({"provider":provider,"baseUrl":base,"apiKeyId":created["connectionId"]});let key=store.key_for_purpose(&config,crate::credential_policy::Purpose::ModelCatalogue).unwrap();
            let server=tokio::spawn(async move{tokio::time::timeout(Duration::from_secs(3),async move{let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();loop{let mut chunk=[0;2048];let n=socket.read(&mut chunk).await.unwrap();assert_ne!(n,0);bytes.extend_from_slice(&chunk[..n]);assert!(bytes.len()<8192);if bytes.windows(4).any(|w|w==b"\r\n\r\n"){break;}}let response=b"{\"data\":[{\"id\":\"fixture-model\"}]}";socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.len()).as_bytes()).await.unwrap();socket.write_all(response).await.unwrap();String::from_utf8(bytes).unwrap()}).await.unwrap()});
            assert_eq!(models(&config,key).await.unwrap(),vec!["fixture-model"]);let request=server.await.unwrap().to_ascii_lowercase();assert!(request.starts_with("get /tenant/v1/models http/1.1"));let expected=if provider=="anthropic"{"x-api-key: synthetic-catalogue-marker"}else{"authorization: bearer synthetic-catalogue-marker"};assert!(request.contains(expected));if provider=="anthropic"{assert!(!request.contains("authorization:"));assert!(request.contains("anthropic-version: 2023-06-01"));}drop(store);std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn actual_local_tts_adapter_counts_exact_spoken_tags_and_blocks_second_http_send(){
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let root=std::env::temp_dir().join(format!("orbitpal-tts-budget-{}",uuid::Uuid::new_v4()));
        let store=Store::new(root.clone()).unwrap();let spoken="[whispering] ä🪐";let count=spoken.chars().count() as u64;
        store.set_usage_limits(&Limits{daily_tts_characters:count,..Limits::default()}).unwrap();
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base=format!("http://{}/v1",listener.local_addr().unwrap());
        let server=tokio::spawn(async move{
            let(mut stream,_)=tokio::time::timeout(Duration::from_secs(3),listener.accept()).await.unwrap().unwrap();let mut bytes=Vec::new();let mut buffer=[0u8;2048];
            let(header_end,len)=loop{let n=stream.read(&mut buffer).await.unwrap();assert_ne!(n,0);bytes.extend_from_slice(&buffer[..n]);assert!(bytes.len()<8192);if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){let header=String::from_utf8_lossy(&bytes[..end]);let len=header.lines().find_map(|line|{let(k,v)=line.split_once(':')?;k.eq_ignore_ascii_case("content-length").then(||v.trim().parse::<usize>().unwrap())}).unwrap();break(end+4,len);}};
            while bytes.len()<header_end+len{let n=stream.read(&mut buffer).await.unwrap();assert_ne!(n,0);bytes.extend_from_slice(&buffer[..n]);}
            let request:Value=serde_json::from_slice(&bytes[header_end..header_end+len]).unwrap();assert_eq!(request["input"],"[whispering] ä🪐");
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 3\r\nConnection: close\r\n\r\nID3").await.unwrap();listener
        });
        let config=json!({"provider":"compatible","baseUrl":base,"model":"fixture-tts","voiceId":"fixture"});let ctx=Context::new("nova",Origin::Autopilot);let cancel=Arc::new(AtomicBool::new(false));
        assert_eq!(synthesize_metered(&config,spoken,None,&store,&ctx,cancel.clone()).await.unwrap()["mimeType"],"audio/mpeg");let listener=server.await.unwrap();
        assert!(synthesize_metered(&config,"x",None,&store,&ctx,cancel).await.is_err());assert!(tokio::time::timeout(Duration::from_millis(30),listener.accept()).await.is_err());
        let total=store.usage_snapshot().unwrap().total;assert_eq!(total.tts_requests,1);assert_eq!(total.tts_characters,count);drop(store);std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod usage_stream_tests {
    use super::*;
    fn chunks(parts:&[&str])->impl futures_util::Stream<Item=Result<Vec<u8>,()>>{futures_util::stream::iter(parts.iter().map(|s|Ok(s.as_bytes().to_vec())).collect::<Vec<_>>())}
    const FINAL:&str="data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":\"stop\"}]}\n\n";
    #[tokio::test]
    async fn actual_stream_parser_reads_final_usage_after_finish_without_waiting_for_done(){
        let mut observed=None;let mut deltas=String::new();
        let stream=chunks(&[FINAL,"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n"]).chain(futures_util::stream::pending());
        let reply=tokio::time::timeout(Duration::from_secs(1),completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|s|deltas.push_str(s))).await.unwrap().unwrap();
        assert_eq!(reply.text,"hello");assert_eq!(deltas,"hello");assert_eq!(observed,Some(15));
    }
    #[tokio::test]
    async fn absent_or_hanging_usage_trailer_never_stalls_a_completed_answer(){
        for expect in [false,true]{
            let mut observed=None;let stream=chunks(&[FINAL]).chain(futures_util::stream::pending());
            let reply=tokio::time::timeout(Duration::from_secs(1),completion_stream(stream,&AtomicBool::new(false),false,expect,Duration::from_millis(20),&mut observed,|_|{})).await.unwrap().unwrap();assert_eq!(reply.text,"hello");assert_eq!(observed,None);
        }
    }
    #[tokio::test]
    async fn interruption_before_finish_is_error_but_after_finish_keeps_the_answer_and_unknown_receipt(){
        for finished in [false,true]{
            let first=if finished{FINAL}else{"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n"};
            let stream=chunks(&[first]).chain(futures_util::stream::iter([Err(())]));let mut observed=None;
            let reply=completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{}).await;
            assert_eq!(reply.is_ok(),finished);assert_eq!(observed,None);if finished{assert_eq!(reply.unwrap().text,"hello");}
        }
    }
    #[tokio::test]
    async fn running_usage_before_a_terminal_choice_is_not_a_final_receipt(){
        let early="data: {\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":null}],\"usage\":{\"prompt_tokens\":0,\"completion_tokens\":0,\"total_tokens\":0}}\n\n";
        let mut observed=None;let stream=chunks(&[early,FINAL]).chain(futures_util::stream::pending());
        let reply=completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{}).await.unwrap();assert_eq!(reply.text,"hello");assert_eq!(observed,None);
    }

    #[tokio::test]
    async fn optional_receipt_wait_never_accepts_more_text_or_tools_after_finish(){
        let extra="data: {\"choices\":[{\"delta\":{\"content\":\"late text\",\"tool_calls\":[{\"index\":0,\"id\":\"late\",\"function\":{\"name\":\"unexpected\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n";
        let mut observed=None;let stream=chunks(&[FINAL,extra,"data: [DONE]\n\n"]);
        let reply=completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{}).await.unwrap();assert_eq!(reply.text,"hello");assert!(reply.tool_calls.is_empty());assert_eq!(observed,None);
    }

    #[tokio::test]
    async fn oversized_or_unterminated_optional_trailer_preserves_complete_answer_without_a_refund(){
        for tail in [vec![b'x';MAX_RESPONSE+1],vec![b'x';256*1024+1]]{
            let stream=futures_util::stream::iter([Ok::<_,()>(FINAL.as_bytes().to_vec()),Ok(tail)]);let mut observed=None;
            let reply=completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{}).await.unwrap();assert_eq!(reply.text,"hello");assert!(reply.tool_calls.is_empty());assert_eq!(observed,None);
        }
        // Limits still reject an oversized response before any genuine terminal choice.
        let stream=futures_util::stream::iter([Ok::<_,()>(vec![b'x';MAX_RESPONSE+1])]);let mut observed=None;
        assert!(completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{}).await.is_err());assert_eq!(observed,None);
    }
    #[tokio::test]
    async fn only_documented_terminal_reason_strings_can_authorize_final_usage(){
        for reason in [json!(false),json!(1),json!({}),json!([]),json!(null),json!(""),json!("unknown")]{
            let event=json!({"choices":[{"delta":{"content":"hello"},"finish_reason":reason}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}});let wire=format!("data: {event}\n\ndata: [DONE]\n\n");let mut observed=None;
            let reply=completion_stream(chunks(&[&wire]),&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{}).await.unwrap();assert_eq!(reply.text,"hello");assert_eq!(observed,None,"{reason}");
        }
        for reason in ["stop","length","tool_calls","content_filter","function_call"]{
            let event=json!({"choices":[{"delta":{"content":"hello"},"finish_reason":reason}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}});let wire=format!("data: {event}\n\n");let mut observed=None;
            let stream=chunks(&[&wire]).chain(futures_util::stream::pending());let reply=tokio::time::timeout(Duration::from_secs(1),completion_stream(stream,&AtomicBool::new(false),false,true,Duration::from_millis(20),&mut observed,|_|{})).await.unwrap().unwrap();assert_eq!(reply.text,"hello");assert_eq!(observed,Some(15),"{reason}");
        }
    }

}
