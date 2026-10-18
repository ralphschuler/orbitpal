//! Deliberately small, authenticated local MCP control surface.
//! Protocol sources: modelcontextprotocol.io/specification/{2025-11-25,2026-07-28}
//! (basic/transports, basic/versioning, basic, server/discover, server/tools).
//! This module never invokes a model or accepts settings, paths or credentials.
use axum::{body::Bytes, extract::State, http::{header, HeaderMap, StatusCode}, response::{IntoResponse, Response}, Json};
use base64::Engine;
use serde_json::{json, Value};
use std::{collections::VecDeque, future::Future, sync::{atomic::Ordering, Mutex}, time::{Duration, Instant}};
use tauri::{Emitter, Manager};
use tokio::sync::Semaphore;

pub const MAX_REQUEST_BYTES: usize = 16 * 1024;
pub const CONTROL_SKILL: &str = include_str!("../../character-template/skills/cohost-control/SKILL.md");
const MAX_ARGUMENT_BYTES: usize = 2048;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_BOTS: usize = 100;
const REQUESTS_PER_MINUTE: usize = 120;
const MODERN: &str = "2026-07-28";
const VERSIONS: [&str; 4] = [MODERN, "2025-11-25", "2025-06-18", "2025-03-26"];

pub struct ControlMcp {
    token: String,
    inflight: Semaphore,
    requests: Mutex<VecDeque<Instant>>,
}
impl Default for ControlMcp {
    fn default() -> Self { Self { token: uuid::Uuid::new_v4().to_string(), inflight: Semaphore::new(8), requests: Mutex::new(VecDeque::new()) } }
}
impl ControlMcp {
    pub fn bearer_token(&self) -> &str { &self.token }
    pub fn url(&self, app: &tauri::AppHandle) -> Result<String, String> {
        let port = app.state::<crate::AppState>().overlay.port.load(Ordering::Relaxed);
        if port == 0 { return Err("Der lokale Steuerungsserver ist noch nicht bereit".into()); }
        Ok(format!("http://127.0.0.1:{port}/mcp"))
    }
    /// Operator-only IPC may expose this short-lived credential for copying.
    /// It must never enter the library definition, profile export or logs.
    pub fn client_config(&self, app: &tauri::AppHandle) -> Result<Value, String> {
        let url = self.url(app)?;
        let token = self.bearer_token().to_owned();
        Ok(json!({"url":url,"token":token,"mcpServers":{"streamer-ai-cohost":{"type":"http","url":url,"headers":{"Authorization":format!("Bearer {token}")}}}}))
    }
    fn admit(&self, now: Instant) -> bool {
        let Ok(mut requests) = self.requests.lock() else { return false; };
        while requests.front().is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(60)) { requests.pop_front(); }
        if requests.len() >= REQUESTS_PER_MINUTE { return false; }
        requests.push_back(now); true
    }
}

#[derive(Debug, PartialEq)]
enum Command {
    Status, ListBots, ListEmotes { bot_id: String },
    PerformEmote { bot_id: String, emote_id: String },
    ShowCharacter { bot_id: String }, HideCharacter { bot_id: String },
    StopOutput, PauseAutoReplies,
}

fn server_info() -> Value { json!({"name":"streamer-ai-cohost-control","version":env!("CARGO_PKG_VERSION")}) }
fn instruction() -> &'static str { "Control Streamer AI Cohost through bounded host actions. Installations do not grant tools to bots. Emote calls require allowChat=true. Show/hide require an idle dialog runtime. Stop is global. No dialog, filesystem, configuration or permission-writing tools are provided." }

pub fn tools() -> Vec<Value> {
    let bot = json!({"type":"string","minLength":1,"maxLength":96,"pattern":"^[A-Za-z0-9_-]+$","description":"Exact bot ID returned by cohost_list_bots; do not guess."});
    let mut descriptors = Vec::new();
    for (name, description, fields, required, read_only, idempotent) in [
        ("cohost_status", "Read public app status and dialog-busy state; no credentials, conversation or local paths.", json!({}), json!([]), true, true),
        ("cohost_list_bots", "List up to 100 bots with names and public output status. Does not change the selected bot.", json!({}), json!([]), true, true),
        ("cohost_list_emotes", "List this bot's emotes that allowChat=true and can be requested through this generic control server.", json!({"botId":bot}), json!(["botId"]), true, true),
        ("cohost_perform_emote", "Dispatch an existing allowChat=true emote, including its configured optional sound. Dispatch is not proof that a renderer completed it.", json!({"botId":bot,"emoteId":{"type":"string","minLength":1,"maxLength":96,"pattern":"^[A-Za-z0-9_-]+$"}}), json!(["botId","emoteId"]), false, false),
        ("cohost_show_character", "Open this bot's native character window and use overlay output. Requires no active dialog; a busy result makes no output change.", json!({"botId":bot}), json!(["botId"]), false, true),
        ("cohost_hide_character", "Close this bot's character output and restore studio mode. Requires no active dialog; a busy result makes no output change.", json!({"botId":bot}), json!(["botId"]), false, true),
        ("cohost_stop_output", "Globally cancel the active dialog and stop queued/playing output for all bots. Does not start a new dialog.", json!({}), json!([]), false, true),
        ("cohost_pause_auto_replies", "Request that the open control room pause automatic replies. The result confirms dispatch of the request, not persistent settings or UI acknowledgement.", json!({}), json!([]), false, true),
    ] {
        descriptors.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":fields,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":read_only,"destructiveHint":false,"idempotentHint":idempotent,"openWorldHint":false}}));
    }
    descriptors
}

fn allowed_keys(value: &Value, keys: &[&str]) -> bool { value.as_object().is_some_and(|object| object.keys().all(|key| keys.contains(&key.as_str()))) }
fn argument_id(arguments: &Value, field: &str) -> Result<String, &'static str> {
    arguments[field].as_str().filter(|id| crate::storage::valid_id(id)).map(str::to_owned).ok_or("Bot- und Emote-IDs müssen 1–96 Buchstaben, Zahlen, Bindestriche oder Unterstriche enthalten")
}
fn command(name: &str, arguments: &Value) -> Result<Command, &'static str> {
    if !arguments.is_object() || arguments.to_string().len() > MAX_ARGUMENT_BYTES { return Err("Werkzeugargumente müssen ein JSON-Objekt mit höchstens 2 KiB sein"); }
    let no_args = || if allowed_keys(arguments, &[]) { Ok(()) } else { Err("Dieses Werkzeug akzeptiert keine Argumente") };
    let bot_args = || if allowed_keys(arguments, &["botId"]) { argument_id(arguments, "botId") } else { Err("Nur botId ist für dieses Werkzeug erlaubt") };
    Ok(match name {
        "cohost_status" => { no_args()?; Command::Status },
        "cohost_list_bots" => { no_args()?; Command::ListBots },
        "cohost_list_emotes" => Command::ListEmotes { bot_id: bot_args()? },
        "cohost_perform_emote" => {
            if !allowed_keys(arguments, &["botId", "emoteId"]) { return Err("Nur botId und emoteId sind erlaubt"); }
            Command::PerformEmote { bot_id: argument_id(arguments, "botId")?, emote_id: argument_id(arguments, "emoteId")? }
        },
        "cohost_show_character" => Command::ShowCharacter { bot_id: bot_args()? },
        "cohost_hide_character" => Command::HideCharacter { bot_id: bot_args()? },
        "cohost_stop_output" => { no_args()?; Command::StopOutput },
        "cohost_pause_auto_replies" => { no_args()?; Command::PauseAutoReplies },
        _ => return Err("Unbekanntes Cohost-Werkzeug"),
    })
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() { return None; }
    Some(value)
}
fn secure_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0u8, |difference, (a,b)| difference | (a ^ b)) == 0
}
fn authenticate(headers: &HeaderMap, control: &ControlMcp, port: u16) -> Result<(), (StatusCode, &'static str)> {
    let authority = single_header(headers, "host").ok_or((StatusCode::FORBIDDEN, "Ungültiger lokaler Host"))?;
    if port == 0 || ![format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")].iter().any(|expected| authority.eq_ignore_ascii_case(expected)) {
        return Err((StatusCode::FORBIDDEN, "Ungültiger lokaler Host"));
    }
    if headers.contains_key(header::ORIGIN) {
        let origin = single_header(headers, "origin").ok_or((StatusCode::FORBIDDEN, "Unerlaubter Ursprung"))?;
        if ![format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}"), format!("http://[::1]:{port}")].iter().any(|expected| origin.eq_ignore_ascii_case(expected)) {
            return Err((StatusCode::FORBIDDEN, "Unerlaubter Ursprung"));
        }
    }
    let authorization = single_header(headers, "authorization").ok_or((StatusCode::UNAUTHORIZED, "Lokaler MCP-Bearer-Token fehlt"))?;
    let Some((scheme, token)) = authorization.split_once(' ') else { return Err((StatusCode::UNAUTHORIZED, "Ungültiger lokaler MCP-Bearer-Token")); };
    if !scheme.eq_ignore_ascii_case("Bearer") || !secure_equal(token.as_bytes(), control.token.as_bytes()) { return Err((StatusCode::UNAUTHORIZED, "Ungültiger lokaler MCP-Bearer-Token")); }
    Ok(())
}
fn response(status: StatusCode, body: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store"), (header::X_CONTENT_TYPE_OPTIONS, "nosniff")], Json(body)).into_response()
}
fn rpc_error(status: StatusCode, id: Value, code: i64, message: &str) -> Response { response(status, json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})) }
fn unsupported(id: Value, requested: &str) -> Response {
    response(StatusCode::BAD_REQUEST, json!({"jsonrpc":"2.0","id":id,"error":{"code":-32022,"message":"Unsupported protocol version","data":{"supported":VERSIONS,"requested":requested.chars().take(96).collect::<String>()}}}))
}
fn rpc_result(id: Value, mut result: Value, modern: bool) -> Response {
    if modern { result["resultType"] = json!("complete"); result["_meta"] = json!({"io.modelcontextprotocol/serverInfo":server_info()}); }
    let body = json!({"jsonrpc":"2.0","id":id,"result":result});
    if body.to_string().len() > MAX_RESPONSE_BYTES { return rpc_error(StatusCode::INTERNAL_SERVER_ERROR, body["id"].clone(), -32603, "Steuerungsantwort überschreitet die Größenbegrenzung"); }
    response(StatusCode::OK, body)
}
fn tool_result(result: Result<Value, String>) -> Value {
    match result {
        Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false}),
        Err(message) => json!({"content":[{"type":"text","text":message.chars().take(500).collect::<String>()}],"isError":true}),
    }
}
fn decoded_header(value: &str) -> Option<String> {
    if let Some(encoded) = value.strip_prefix("=?base64?").and_then(|value| value.strip_suffix("?=")) {
        if encoded.len() > 512 { return None; }
        return String::from_utf8(base64::engine::general_purpose::STANDARD.decode(encoded).ok()?).ok();
    }
    Some(value.to_owned())
}
fn valid_request_id(id: &Value) -> bool { id.as_str().is_some_and(|value| !value.is_empty() && value.len() <= 96 && !value.chars().any(char::is_control)) || id.as_i64().is_some() || id.as_u64().is_some() }

/// One JSON response per POST; no SSE/session, sampling or model reentrancy.
async fn serve<F, Fut>(control: &ControlMcp, port: u16, headers: &HeaderMap, body: &[u8], dispatch: F) -> Response
where F: FnOnce(Command) -> Fut, Fut: Future<Output = Result<Value, String>> {
    if let Err((status, message)) = authenticate(headers, control, port) {
        let mut reply = rpc_error(status, Value::Null, -32600, message);
        if status == StatusCode::UNAUTHORIZED { reply.headers_mut().insert(header::WWW_AUTHENTICATE, header::HeaderValue::from_static("Bearer realm=\"streamer-ai-cohost\"")); }
        return reply;
    }
    if body.len() > MAX_REQUEST_BYTES { return rpc_error(StatusCode::PAYLOAD_TOO_LARGE, Value::Null, -32600, "MCP-Anfrage überschreitet 16 KiB"); }
    if !control.admit(Instant::now()) { let mut reply = rpc_error(StatusCode::TOO_MANY_REQUESTS, Value::Null, -32603, "Zu viele lokale Steuerungsanfragen"); reply.headers_mut().insert(header::RETRY_AFTER, header::HeaderValue::from_static("60")); return reply; }
    let Ok(_permit) = control.inflight.try_acquire() else { return rpc_error(StatusCode::TOO_MANY_REQUESTS, Value::Null, -32603, "Zu viele gleichzeitige Steuerungsanfragen"); };
    if !single_header(headers, "content-type").is_some_and(|value| value.split(';').next().is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))) { return rpc_error(StatusCode::UNSUPPORTED_MEDIA_TYPE, Value::Null, -32600, "MCP benötigt application/json"); }
    let accept = headers.get_all(header::ACCEPT).iter().filter_map(|value| value.to_str().ok()).collect::<Vec<_>>().join(",").to_ascii_lowercase();
    let accepted = |wanted: &str| accept.split(',').any(|entry| entry.split(';').next().is_some_and(|mime| mime.trim() == wanted));
    if !accepted("application/json") || !accepted("text/event-stream") { return rpc_error(StatusCode::NOT_ACCEPTABLE, Value::Null, -32600, "Accept muss application/json und text/event-stream enthalten"); }
    let request: Value = match serde_json::from_slice(body) { Ok(value) => value, Err(_) => return rpc_error(StatusCode::BAD_REQUEST, Value::Null, -32700, "Ungültiges JSON") };
    let id = request.get("id").filter(|id| valid_request_id(id)).cloned().unwrap_or(Value::Null);
    if !request.is_object() || request["jsonrpc"] != "2.0" || !request["method"].as_str().is_some_and(|method| !method.is_empty() && method.len() <= 96 && !method.chars().any(char::is_control)) || request.get("id").is_some_and(|id| !valid_request_id(id)) || request.get("result").is_some() || request.get("error").is_some() { return rpc_error(StatusCode::BAD_REQUEST, id, -32600, "Ungültige JSON-RPC-Anfrage; Stapelanfragen sind nicht unterstützt"); }
    let method = request["method"].as_str().unwrap_or_default();
    let empty = json!({}); let params = request.get("params").unwrap_or(&empty);
    if !params.is_object() || params.get("_meta").is_some_and(|meta| !meta.is_object()) { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "MCP-Parameter und _meta müssen Objekte sein"); }
    let header_version = single_header(headers, "mcp-protocol-version");
    if headers.contains_key("mcp-protocol-version") && header_version.is_none() { return rpc_error(StatusCode::BAD_REQUEST, id, -32020, "Ungültiger MCP-Protocol-Version-Header"); }
    let body_version = params["_meta"]["io.modelcontextprotocol/protocolVersion"].as_str();
    let version = header_version.or(body_version).or_else(|| if method == "initialize" { params["protocolVersion"].as_str() } else { None }).unwrap_or("2025-03-26");
    if !VERSIONS.contains(&version) { return unsupported(id, version); }
    let modern = version == MODERN;
    if modern {
        if header_version != Some(MODERN) || body_version != Some(MODERN) || single_header(headers, "mcp-method") != Some(method) { return rpc_error(StatusCode::BAD_REQUEST, id, -32020, "MCP-Header und Anfragemetadaten fehlen oder stimmen nicht überein"); }
        if !params["_meta"]["io.modelcontextprotocol/clientCapabilities"].is_object() { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Moderne MCP-Anfragen benötigen clientCapabilities in _meta"); }
        if method == "tools/call" {
            let name = single_header(headers, "mcp-name").and_then(decoded_header);
            if name.as_deref() != params["name"].as_str() || name.is_none() { return rpc_error(StatusCode::BAD_REQUEST, id, -32020, "Mcp-Name stimmt nicht mit dem Werkzeugnamen überein"); }
        }
    } else if body_version.is_some_and(|body| body != version) { return rpc_error(StatusCode::BAD_REQUEST, id, -32020, "Widersprüchliche MCP-Protokollversionen"); }
    if request.get("id").is_none() {
        if method == "notifications/initialized" && !modern && allowed_keys(params, &["_meta"]) { return StatusCode::ACCEPTED.into_response(); }
        if method == "notifications/cancelled" && !modern && allowed_keys(params, &["_meta", "requestId", "reason"]) { return StatusCode::ACCEPTED.into_response(); }
        return rpc_error(StatusCode::BAD_REQUEST, Value::Null, -32600, "Diese Methode benötigt eine Anfrage-ID; keine Steuerungsaktion wurde ausgeführt");
    }
    match method {
        "initialize" if !modern => {
            if !allowed_keys(params, &["protocolVersion", "capabilities", "clientInfo", "_meta"]) || !params["capabilities"].is_object() || !params["clientInfo"].is_object() { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Ungültige MCP-Initialisierung"); }
            let Some(requested) = params["protocolVersion"].as_str() else { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "protocolVersion fehlt"); };
            if !VERSIONS[1..].contains(&requested) { return unsupported(id, requested); }
            if requested != version { return rpc_error(StatusCode::BAD_REQUEST, id, -32020, "Initialisierungs- und Header-Version stimmen nicht überein"); }
            rpc_result(id, json!({"protocolVersion":requested,"capabilities":{"tools":{"listChanged":false}},"serverInfo":server_info(),"instructions":instruction()}), false)
        },
        "server/discover" if modern => {
            if !allowed_keys(params, &["_meta"]) { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Discovery akzeptiert nur Metadaten"); }
            rpc_result(id, json!({"supportedVersions":VERSIONS,"capabilities":{"tools":{"listChanged":false}},"instructions":instruction()}), true)
        },
        "ping" => {
            if !allowed_keys(params, &["_meta"]) { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Ping akzeptiert keine Nutzargumente"); }
            rpc_result(id, json!({}), modern)
        },
        "tools/list" => {
            if !allowed_keys(params, &["_meta", "cursor"]) || params.get("cursor").is_some() { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Der feste Werkzeugkatalog unterstützt keinen Cursor"); }
            rpc_result(id, json!({"tools":tools()}), modern)
        },
        "tools/call" => {
            if !allowed_keys(params, &["_meta", "name", "arguments"]) { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Unbekannte Werkzeugaufruf-Parameter"); }
            let Some(name) = params["name"].as_str() else { return rpc_error(StatusCode::BAD_REQUEST, id, -32602, "Werkzeugname fehlt"); };
            let arguments = params.get("arguments").unwrap_or(&empty);
            let action = match command(name, arguments) { Ok(action) => action, Err(message) => return rpc_error(StatusCode::BAD_REQUEST, id, -32602, message) };
            rpc_result(id, tool_result(dispatch(action).await), modern)
        },
        _ => rpc_error(StatusCode::NOT_FOUND, id, -32601, "MCP-Methode ist nicht verfügbar"),
    }
}

pub async fn http(State(app): State<tauri::AppHandle>, headers: HeaderMap, body: Bytes) -> Response {
    let state = app.state::<crate::AppState>();
    let port = state.overlay.port.load(Ordering::Relaxed);
    let handle = &app;
    serve(&state.control_mcp, port, &headers, &body, |command| async move { execute(handle, command) }).await
}
pub async fn no_stream(State(app): State<tauri::AppHandle>, headers: HeaderMap) -> Response {
    let state = app.state::<crate::AppState>();
    if let Err((status, message)) = authenticate(&headers, &state.control_mcp, state.overlay.port.load(Ordering::Relaxed)) { return rpc_error(status, Value::Null, -32600, message); }
    let mut reply = rpc_error(StatusCode::METHOD_NOT_ALLOWED, Value::Null, -32601, "Dieser MCP-Endpunkt unterstützt ausschließlich POST mit JSON-Antworten");
    reply.headers_mut().insert(header::ALLOW, header::HeaderValue::from_static("POST")); reply
}

fn safe_name(value: &Value, fallback: &str) -> String { value.as_str().unwrap_or(fallback).chars().filter(|ch| !ch.is_control()).take(160).collect() }
fn public_bot(profile: &Value, selected: bool, window_open: bool) -> Value {
    let mode = profile["output"]["mode"].as_str().filter(|mode| matches!(*mode, "studio" | "overlay" | "obs")).unwrap_or("unknown");
    json!({"id":profile["id"].as_str().unwrap_or_default(),"name":safe_name(&profile["name"],"Bot"),"selected":selected,"output":{"mode":mode,"enabled":profile["output"]["streamingEnabled"].as_bool().unwrap_or(false),"windowOpen":window_open}})
}
fn public_emotes(profile: &Value) -> Value {
    let emotes: Vec<Value> = profile["emotes"].as_array().into_iter().flatten().filter(|emote| emote["allowChat"].as_bool() == Some(true) && emote["id"].as_str().is_some_and(crate::storage::valid_id)).take(100).map(|emote| json!({"id":emote["id"],"name":safe_name(&emote["name"],emote["id"].as_str().unwrap_or("Emote")),"duration":emote["duration"].as_f64().unwrap_or(2.5).clamp(0.2,15.0),"allowChat":true})).collect();
    json!({"botId":profile["id"],"emotes":emotes,"onlyChatAllowed":true})
}
fn execute(app: &tauri::AppHandle, command: Command) -> Result<Value, String> {
    let state = app.state::<crate::AppState>();
    let show_character = matches!(&command, Command::ShowCharacter { .. });
    match command {
        Command::Status => {
            let bots = state.store.list_bots().map_err(|_| "Botstatus konnte nicht gelesen werden")?;
            let dialog_busy = state.generation.try_lock().is_err();
            Ok(json!({"application":"Streamer AI Cohost","version":env!("CARGO_PKG_VERSION"),"botCount":bots.len(),"dialogBusy":dialog_busy,"control":"local-authenticated","supportsNewDialogs":false}))
        },
        Command::ListBots => {
            let bots = state.store.list_bots().map_err(|_| "Botliste konnte nicht gelesen werden")?;
            let summaries: Vec<Value> = bots.iter().take(MAX_BOTS).map(|bot| public_bot(&bot["profile"],bot["active"].as_bool().unwrap_or(false),crate::overlay::is_open(app,bot["id"].as_str().unwrap_or_default()))).collect();
            Ok(json!({"bots":summaries,"truncated":bots.len()>MAX_BOTS}))
        },
        Command::ListEmotes { bot_id } => {
            let profile = state.store.bot_profile(&bot_id).map_err(|_| "Bot wurde nicht gefunden oder ist nicht verfügbar")?;
            Ok(public_emotes(&profile))
        },
        Command::PerformEmote { bot_id, emote_id } => {
            state.updater.ensure_idle()?;
            let profile = state.store.bot_profile(&bot_id).map_err(|_| "Bot wurde nicht gefunden oder ist nicht verfügbar")?;
            // Generic MCP calls do not carry a trusted viewer/operator origin.
            // Enforce the stricter emote grant for internal and external clients.
            crate::overlay::trigger_emote_profile(app,&profile,&emote_id,true).map_err(|_| "Emote ist nicht vorhanden, nicht für Chat freigegeben oder konnte nicht übermittelt werden")?;
            Ok(json!({"botId":bot_id,"emoteId":emote_id,"dispatched":true}))
        },
        Command::ShowCharacter { bot_id } | Command::HideCharacter { bot_id } => {
            // Never wait for the generation lock: this call can originate from
            // that same generation through the app's own MCP client.
            let _idle = state.generation.try_lock().map_err(|_| "Ausgabewechsel während einer laufenden Antwort gesperrt. Nach Ende der Antwort erneut anfordern")?;
            state.store.bot_profile(&bot_id).map_err(|_| "Bot wurde nicht gefunden oder ist nicht verfügbar")?;
            if state.overlay.has_pending_speech(&bot_id) { return Err("Ausgabewechsel während einer laufenden oder vorbereiteten Sprachausgabe gesperrt. Nach Ende der Ausgabe erneut anfordern".into()); }
            let show = show_character;
            if show { crate::overlay::open_for_bot(app,&bot_id).map_err(|_| "Figurenfenster konnte nicht geöffnet werden; bitte Ausgabestatus im Regiepult prüfen")?; }
            else { crate::overlay::close_for_bot(app,&bot_id).map_err(|_| "Figurenausgabe konnte nicht beendet werden; bitte Ausgabestatus im Regiepult prüfen")?; }
            Ok(json!({"botId":bot_id,"windowOpen":crate::overlay::is_open(app,&bot_id),"outputMode":if show {"overlay"} else {"studio"}}))
        },
        Command::StopOutput => {
            if let Some(cancel) = state.cancel.lock().map_err(|_| "Aktive Antwort konnte nicht abgebrochen werden")?.as_ref() { cancel.store(true,Ordering::Relaxed); }
            state.overlay.publish(app,json!({"kind":"stop"}));
            app.emit("cohost://control",json!({"action":"stop"})).map_err(|_| "Ausgabe wurde gestoppt; das Regiepult konnte nicht benachrichtigt werden")?;
            Ok(json!({"stopped":true,"scope":"all-bots"}))
        },
        Command::PauseAutoReplies => {
            app.emit("cohost://control",json!({"action":"pause"})).map_err(|_| "Pausenanforderung konnte nicht übermittelt werden")?;
            Ok(json!({"pauseRequested":true,"uiAcknowledged":false}))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PORT: u16 = 24931;
    fn headers(control: &ControlMcp, version: &str, method: &str, name: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (key,value) in [("host",format!("127.0.0.1:{PORT}")),("authorization",format!("Bearer {}",control.bearer_token())),("content-type","application/json".into()),("accept","application/json, text/event-stream".into()),("mcp-protocol-version",version.into()),("mcp-method",method.into())] { headers.insert(header::HeaderName::from_bytes(key.as_bytes()).unwrap(),value.parse().unwrap()); }
        if let Some(name) = name { headers.insert("mcp-name",name.parse().unwrap()); }
        headers
    }
    fn request(method: &str, mut params: Value, modern: bool) -> Value {
        if modern { params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":MODERN,"io.modelcontextprotocol/clientCapabilities":{}}); }
        json!({"jsonrpc":"2.0","id":"fixture","method":method,"params":params})
    }
    async fn inspect(reply: Response) -> (StatusCode, Value) {
        let status = reply.status(); let bytes = axum::body::to_bytes(reply.into_body(),MAX_RESPONSE_BYTES).await.unwrap();
        (status,if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
    }
    async fn fixture(control: &ControlMcp, headers: &HeaderMap, request: &Value) -> (StatusCode, Value) {
        inspect(serve(control,PORT,headers,request.to_string().as_bytes(),|action| async move { Ok(json!({"fixtureAction":format!("{action:?}")})) }).await).await
    }
    #[tokio::test]
    async fn legacy_initialization_notification_and_tool_list_are_real_rpc() {
        let control = ControlMcp::default();
        for version in VERSIONS.iter().skip(1) {
            let mut headers = headers(&control,version,"initialize",None);
            let (status,body) = fixture(&control,&headers,&request("initialize",json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}),false)).await;
            assert_eq!(status,StatusCode::OK); assert_eq!(body["id"],"fixture"); assert_eq!(body["result"]["protocolVersion"],*version); assert!(body["result"]["capabilities"]["tools"].is_object());
            let initialized = json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}});
            headers.insert("mcp-method","notifications/initialized".parse().unwrap());
            assert_eq!(fixture(&control,&headers,&initialized).await,(StatusCode::ACCEPTED,Value::Null));
            headers.insert("mcp-method","tools/list".parse().unwrap());
            let (_,listed) = fixture(&control,&headers,&request("tools/list",json!({}),false)).await;
            assert_eq!(listed["result"]["tools"].as_array().unwrap().len(),8); assert!(listed["result"].get("resultType").is_none());
        }
    }
    #[tokio::test]
    async fn modern_discovery_validates_metadata_and_mirrored_headers() {
        let control = ControlMcp::default(); let mut headers = headers(&control,MODERN,"server/discover",None);
        let (status,body) = fixture(&control,&headers,&request("server/discover",json!({}),true)).await;
        assert_eq!(status,StatusCode::OK); assert_eq!(body["result"]["resultType"],"complete"); assert_eq!(body["result"]["supportedVersions"][0],MODERN); assert_eq!(body["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],"streamer-ai-cohost-control");
        headers.insert("mcp-method","tools/list".parse().unwrap());
        assert_eq!(fixture(&control,&headers,&request("server/discover",json!({}),true)).await.1["error"]["code"],-32020);
        headers.insert("mcp-method","tools/call".parse().unwrap()); headers.insert("mcp-name","=?base64?Y29ob3N0X3N0YXR1cw==?=".parse().unwrap());
        let (_,called) = fixture(&control,&headers,&request("tools/call",json!({"name":"cohost_status","arguments":{}}),true)).await;
        assert_eq!(called["result"]["isError"],false); assert_eq!(called["result"]["content"][0]["type"],"text");
        headers.insert("mcp-name","cohost_stop_output".parse().unwrap());
        assert_eq!(fixture(&control,&headers,&request("tools/call",json!({"name":"cohost_status"}),true)).await.1["error"]["code"],-32020);
    }
    #[tokio::test]
    async fn invalid_credentials_origins_and_dns_rebinding_never_dispatch() {
        let control = ControlMcp::default(); let fresh = ControlMcp::default(); assert_ne!(control.bearer_token(),fresh.bearer_token());
        let request = request("tools/call",json!({"name":"cohost_stop_output"}),false);
        for (field,value,status) in [("authorization","Bearer wrong",StatusCode::UNAUTHORIZED),("origin","https://attacker.invalid",StatusCode::FORBIDDEN),("origin","null",StatusCode::FORBIDDEN),("host","attacker.invalid:24931",StatusCode::FORBIDDEN),("host","127.0.0.1:1234",StatusCode::FORBIDDEN)] {
            let mut headers = headers(&control,"2025-11-25","tools/call",None); headers.insert(header::HeaderName::from_bytes(field.as_bytes()).unwrap(),value.parse().unwrap());
            let reply = serve(&control,PORT,&headers,request.to_string().as_bytes(),|_| async { panic!("unauthorized action dispatched"); #[allow(unreachable_code)] Ok(Value::Null) }).await;
            assert_eq!(reply.status(),status);
        }
        let mut duplicate = headers(&control,"2025-11-25","tools/call",None); duplicate.append(header::AUTHORIZATION,"Bearer duplicate".parse().unwrap());
        assert_eq!(fixture(&control,&duplicate,&request).await.0,StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn malformed_unknown_oversized_and_notification_calls_cannot_have_effects() {
        let control = ControlMcp::default(); let headers = headers(&control,"2025-11-25","tools/call",None);
        for invalid in [json!([]),json!({"jsonrpc":"2.0","id":null,"method":"tools/call"}),json!({"jsonrpc":"2.0","id":1.5,"method":"tools/call"}),json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"cohost_stop_output"}}),request("tools/call",json!({"name":"cohost_set_secret"}),false),request("tools/call",json!({"name":"cohost_show_character","arguments":{"botId":"../private"}}),false),request("tools/call",json!({"name":"cohost_stop_output","arguments":{"prompt":"new dialog"}}),false)] {
            let reply = serve(&control,PORT,&headers,invalid.to_string().as_bytes(),|_| async { panic!("invalid action dispatched"); #[allow(unreachable_code)] Ok(Value::Null) }).await;
            assert_eq!(reply.status(),StatusCode::BAD_REQUEST);
        }
        let (status,body) = inspect(serve(&control,PORT,&headers,b"{",|_| async { Ok(Value::Null) }).await).await;
        assert_eq!(status,StatusCode::BAD_REQUEST); assert_eq!(body["error"]["code"],-32700);
        assert_eq!(serve(&control,PORT,&headers,&vec![b' ';MAX_REQUEST_BYTES+1],|_| async { Ok(Value::Null) }).await.status(),StatusCode::PAYLOAD_TOO_LARGE);
        assert!(command("cohost_perform_emote",&json!({"botId":"nova","emoteId":"wave","extra":true})).is_err());
        assert!(command("cohost_status",&json!({"large":"x".repeat(MAX_ARGUMENT_BYTES)})).is_err());
    }
    #[tokio::test]
    async fn unsupported_versions_and_missing_modern_capabilities_fail_explicitly() {
        let control = ControlMcp::default(); let mut headers = headers(&control,"1900-01-01","ping",None);
        let (status,body) = fixture(&control,&headers,&request("ping",json!({}),false)).await;
        assert_eq!(status,StatusCode::BAD_REQUEST); assert_eq!(body["error"]["code"],-32022); assert!(body["error"]["data"]["supported"].as_array().unwrap().iter().any(|version| version == MODERN));
        headers.insert("mcp-protocol-version",MODERN.parse().unwrap());
        let (status,body) = fixture(&control,&headers,&json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":MODERN}}})).await;
        assert_eq!(status,StatusCode::BAD_REQUEST); assert_eq!(body["error"]["code"],-32602);
        headers.remove("mcp-protocol-version");
        assert_eq!(fixture(&control,&headers,&request("ping",json!({}),true)).await.1["error"]["code"],-32020);
    }
    #[tokio::test]
    async fn execution_failures_remain_call_tool_results_with_matching_ids() {
        let control = ControlMcp::default(); let headers = headers(&control,MODERN,"tools/call",Some("cohost_show_character"));
        let request = request("tools/call",json!({"name":"cohost_show_character","arguments":{"botId":"nova"}}),true);
        let (status,body) = inspect(serve(&control,PORT,&headers,request.to_string().as_bytes(),|action| async move { assert_eq!(action,Command::ShowCharacter { bot_id:"nova".into() }); Err("Ausgabewechsel während einer laufenden Antwort gesperrt".into()) }).await).await;
        assert_eq!(status,StatusCode::OK); assert_eq!(body["id"],"fixture"); assert_eq!(body["result"]["isError"],true); assert_eq!(body["result"]["resultType"],"complete"); assert!(body.get("error").is_none());
    }
    #[test]
    fn public_summaries_do_not_serialize_profiles_paths_keys_or_conversations() {
        let profile = json!({"id":"nova","name":"Nova","agents":"private marker","history":["private marker"],"model":{"apiKeyId":"private marker"},"avatar":{"assetPath":"private marker"},"output":{"mode":"overlay","streamingEnabled":true},"emotes":[{"id":"wave","name":"Winken","allowChat":true,"sound":"private marker","action":"private marker"},{"id":"hidden","allowChat":false},{"id":"../bad","allowChat":true}]});
        let public = json!({"bot":public_bot(&profile,true,false),"emotes":public_emotes(&profile)});
        assert!(!public.to_string().contains("private marker")); assert_eq!(public["emotes"]["emotes"].as_array().unwrap().len(),1); assert_eq!(public["emotes"]["emotes"][0]["id"],"wave");
        assert!(tools().iter().all(|tool| tool["inputSchema"]["additionalProperties"] == false));
        for forbidden in ["send_message","set_secret","save_profile","grant_tool","read_file"] { assert!(!tools().iter().any(|tool| tool["name"] == forbidden)); }
        assert!(CONTROL_SKILL.starts_with("---\nname: cohost-control\n"));
    }
    #[test]
    fn request_window_and_concurrency_are_bounded_without_sleeping() {
        let control = ControlMcp::default(); let now = Instant::now();
        for _ in 0..REQUESTS_PER_MINUTE { assert!(control.admit(now)); }
        assert!(!control.admit(now)); assert!(control.admit(now+Duration::from_secs(60)));
        let permits: Vec<_> = (0..8).map(|_| control.inflight.try_acquire().unwrap()).collect(); assert!(control.inflight.try_acquire().is_err()); drop(permits); assert!(control.inflight.try_acquire().is_ok());
    }
}
