//! Real Twitch EventSub and YouTube Live Chat connections, with bounded deduplication.
//! YouTube intentionally uses liveChatMessages.list and its pollingIntervalMillis.
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet, VecDeque}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::Emitter;
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
#[derive(Default)]
struct ConnectionContext { connected: bool, user_id: Option<String>, live_chat_id: Option<String> }
struct Running { cancel: Arc<AtomicBool>, task: JoinHandle<()>, app: tauri::AppHandle, context: Arc<Mutex<ConnectionContext>> }
#[derive(Default)]
pub struct ChatHub { tasks: Mutex<HashMap<String, Running>> }

fn text<'a>(v: &'a Value, key: &str) -> &'a str { v[key].as_str().unwrap_or("").trim() }
fn connection(app: &tauri::AppHandle, platform: &str, status: &str, message: &str) {
    let _ = app.emit("cohost://connection", json!({"platform":platform,"status":status,"message":message}));
}
fn client() -> Result<Client, String> {
    Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(35)).build().map_err(|_| "HTTP-Client konnte nicht erstellt werden.".into())
}
fn cancelled(cancel: &AtomicBool) -> bool { cancel.load(Ordering::Relaxed) }
async fn sleep(cancel: &AtomicBool, duration: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + duration;
    while tokio::time::Instant::now() < deadline {
        if cancelled(cancel) { return false; }
        tokio::time::sleep(Duration::from_millis(100).min(deadline.saturating_duration_since(tokio::time::Instant::now()))).await;
    }
    !cancelled(cancel)
}

impl ChatHub {
    pub async fn connect(&self, platform: &str, profile: &Value, secret: Option<String>, app: tauri::AppHandle) -> Result<(), String> {
        if !matches!(platform, "twitch" | "youtube") { return Err("Unbekannte Chatplattform.".into()); }
        let config = profile["chat"][platform].clone();
        let secret = secret.filter(|s| !s.trim().is_empty()).ok_or_else(|| if platform == "twitch" { "Bitte einen Twitch-Benutzertoken mit user:read:chat speichern.".to_string() } else { "Bitte einen YouTube-Data-API-Key speichern.".to_string() })?;
        if platform == "twitch" && (text(&config,"clientId").is_empty() || text(&config,"broadcasterId").is_empty()) {
            return Err("Twitch benötigt Client-ID und numerische Broadcaster-ID.".into());
        }
        if platform == "youtube" && text(&config,"liveChatId").is_empty() && text(&config,"videoId").is_empty() {
            return Err("YouTube benötigt eine Live-Video-URL/ID oder Live-Chat-ID.".into());
        }
        let mut tasks = self.tasks.lock().await;
        if let Some(previous) = tasks.remove(platform) { previous.cancel.store(true, Ordering::Relaxed); previous.task.abort(); }
        let cancel = Arc::new(AtomicBool::new(false));
        let task_cancel = cancel.clone();
        let task_app = app.clone();
        let context = Arc::new(Mutex::new(ConnectionContext::default()));
        let task_context = context.clone();
        let name = platform.to_string();
        connection(&app, platform, "connecting", "Verbindung wird hergestellt …");
        let task = tokio::spawn(async move {
            let result = if name == "twitch" { twitch(config, secret, task_app.clone(), task_cancel.clone(), task_context.clone()).await } else { youtube(config, secret, task_app.clone(), task_cancel.clone(), task_context.clone()).await };
            task_context.lock().await.connected = false;
            if !cancelled(&task_cancel) {
                match result { Ok(()) => connection(&task_app, &name, "disconnected", "Verbindung beendet."), Err(error) => connection(&task_app, &name, "error", &error) }
            }
        });
        tasks.insert(platform.to_string(), Running { cancel, task, app, context });
        Ok(())
    }
    /// Called only for an opted-in answer to a real incoming platform message.
    pub async fn send(&self, platform: &str, profile: &Value, secret: Option<String>, _app: tauri::AppHandle, message: &str) -> Result<(), String> {
        if !matches!(platform, "twitch" | "youtube") { return Err("Unbekannte Chatplattform.".into()); }
        let config = &profile["chat"][platform];
        if config["sendReplies"].as_bool() != Some(true) { return Err("Geschriebene Chatantworten sind deaktiviert.".into()); }
        let token = secret.filter(|key| !key.trim().is_empty()).ok_or_else(|| "Für Chatantworten fehlt der OAuth-Zugriffstoken.".to_string())?;
        let token = token.trim().strip_prefix("oauth:").unwrap_or(token.trim());
        let context = self.tasks.lock().await.get(platform).filter(|task| !task.task.is_finished() && !cancelled(&task.cancel)).map(|task| task.context.clone()).ok_or_else(|| "Chatantwort benötigt eine aktive Plattformverbindung.".to_string())?;
        let (connected, mut own_id, live_chat_id) = { let current = context.lock().await; (current.connected, current.user_id.clone(), current.live_chat_id.clone()) };
        if !connected { return Err("Chatantwort benötigt eine verbundene Plattform.".into()); }
        let message = message.split_whitespace().collect::<Vec<_>>().join(" ");
        if message.is_empty() { return Err("Chatantwort ist leer.".into()); }
        let limit = if platform == "twitch" { 500 } else { 200 };
        let message = if message.chars().count() > limit { message.chars().take(limit - 1).collect::<String>() + "…" } else { message };
        let http = client()?;
        if platform == "twitch" {
            let sender = own_id.ok_or_else(|| "Twitch-Bot-ID wurde noch nicht aufgelöst.".to_string())?;
            let response = http.post("https://api.twitch.tv/helix/chat/messages").bearer_auth(token).header("Client-Id", text(config,"clientId"))
                .json(&json!({"broadcaster_id":text(config,"broadcasterId"),"sender_id":sender,"message":message})).send().await.map_err(|_| "Twitch-Chatantwort konnte nicht gesendet werden.".to_string())?;
            let data = json_response(response,"Twitch-Chatantwort (user:write:chat erforderlich)").await?;
            if !data["data"].as_array().and_then(|items| items.first()).is_some_and(|item| item["is_sent"].as_bool() == Some(true)) {
                return Err("Twitch hat die Chatantwort nicht zugestellt; Kanalregeln oder Rate-Limit prüfen.".into());
            }
        } else {
            let chat_id = live_chat_id.ok_or_else(|| "YouTube Live-Chat-ID wurde noch nicht aufgelöst.".to_string())?;
            if own_id.is_none() {
                // Resolve the writer before posting so its echo is never fed back to the agent.
                let response = http.get("https://www.googleapis.com/youtube/v3/channels").bearer_auth(token).query(&[("part","id"),("mine","true")]).send().await.map_err(|_| "YouTube-Bot-Kanal konnte nicht aufgelöst werden.".to_string())?;
                let data = json_response(response,"YouTube-Schreibanmeldung").await?;
                own_id = data["items"].as_array().and_then(|items| items.first()).and_then(|item| item["id"].as_str()).map(str::to_string);
                if own_id.is_none() { return Err("YouTube-Schreibtoken hat keinen zugeordneten Kanal.".into()); }
                context.lock().await.user_id = own_id;
            }
            let response = http.post("https://www.googleapis.com/youtube/v3/liveChat/messages").bearer_auth(token).query(&[("part","snippet")])
                .json(&json!({"snippet":{"liveChatId":chat_id,"type":"textMessageEvent","textMessageDetails":{"messageText":message}}})).send().await.map_err(|_| "YouTube-Chatantwort konnte nicht gesendet werden.".to_string())?;
            let data = json_response(response,"YouTube-Chatantwort (OAuth erforderlich)").await?;
            if data["id"].as_str().unwrap_or("").is_empty() { return Err("YouTube hat keine gesendete Chatnachricht bestätigt.".into()); }
        }
        Ok(())
    }
    pub async fn disconnect(&self, platform: &str) -> Result<(), String> {
        if !matches!(platform, "twitch" | "youtube") { return Err("Unbekannte Chatplattform.".into()); }
        if let Some(running) = self.tasks.lock().await.remove(platform) {
            running.cancel.store(true, Ordering::Relaxed);
            running.task.abort();
            connection(&running.app, platform, "disconnected", "Getrennt.");
        }
        Ok(())
    }
}
impl Drop for ChatHub {
    fn drop(&mut self) {
        if let Ok(tasks) = self.tasks.try_lock() { for running in tasks.values() { running.cancel.store(true, Ordering::Relaxed); running.task.abort(); } }
    }
}

struct Seen { ids: HashSet<String>, order: VecDeque<String> }
impl Seen {
    fn new() -> Self { Self { ids: HashSet::new(), order: VecDeque::new() } }
    fn insert(&mut self, id: &str) -> bool {
        if id.is_empty() || !self.ids.insert(id.to_string()) { return false; }
        self.order.push_back(id.to_string());
        if self.order.len() > 5000 { if let Some(old) = self.order.pop_front() { self.ids.remove(&old); } }
        true
    }
}

async fn json_response(response: reqwest::Response, platform: &str) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        let hint = match status.as_u16() { 401 | 403 => "Zugangsdaten, Berechtigungen und API-Kontingent prüfen", 404 => "Live-Video oder Kanal wurde nicht gefunden", 429 => "Anfragelimit erreicht", _ => "Anbieter vorübergehend nicht verfügbar" };
        return Err(format!("{platform}: HTTP {}; {hint}.", status.as_u16()));
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| format!("{platform}: Antwort konnte nicht gelesen werden."))?;
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 { return Err(format!("{platform}: Antwort ist zu groß.")); }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| format!("{platform}: Ungültige API-Antwort."))
}

async fn twitch(config: Value, secret: String, app: tauri::AppHandle, cancel: Arc<AtomicBool>, context: Arc<Mutex<ConnectionContext>>) -> Result<(), String> {
    let token = secret.trim().strip_prefix("oauth:").unwrap_or(secret.trim());
    let http = client()?;
    let validation = http.get("https://id.twitch.tv/oauth2/validate").header("Authorization", format!("OAuth {token}")).send().await.map_err(|_| "Twitch-Token konnte nicht geprüft werden.".to_string())?;
    let validation = json_response(validation, "Twitch-Anmeldung").await?;
    if validation["client_id"].as_str() != Some(text(&config, "clientId")) { return Err("Der Twitch-Token gehört zu einer anderen Client-ID.".into()); }
    if !validation["scopes"].as_array().is_some_and(|scopes| scopes.iter().any(|scope| scope == "user:read:chat")) { return Err("Dem Twitch-Token fehlt die Berechtigung user:read:chat.".into()); }
    let own_id = validation["user_id"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| "EventSub benötigt einen Twitch-Benutzertoken; App-Tokens werden nicht unterstützt.".to_string())?;
    if !text(&config,"userId").is_empty() && text(&config,"userId") != own_id { return Err("Twitch User-ID stimmt nicht mit dem gespeicherten Token überein.".into()); }
    let own_id = own_id.to_string();
    context.lock().await.user_id = Some(own_id.clone());
    let mut seen = Seen::new();
    let mut failures = 0u32;
    while !cancelled(&cancel) {
        match twitch_session(&http, &config, token, &own_id, &app, &cancel, &mut seen, &context).await {
            Ok(()) => return Ok(()),
            Err(error) if error.starts_with("DAUERHAFT:") => return Err(error.trim_start_matches("DAUERHAFT:").to_string()),
            Err(_) => {
                context.lock().await.connected = false;
                failures += 1;
                if failures >= 8 { return Err("Twitch-Verbindung wiederholt fehlgeschlagen. Netzwerk und Token prüfen und neu verbinden.".into()); }
                let seconds = (2u64.pow(failures.min(5))).min(30);
                connection(&app, "twitch", "connecting", &format!("Verbindung unterbrochen; neuer Versuch in {seconds} s."));
                if !sleep(&cancel, Duration::from_secs(seconds)).await { return Ok(()); }
            }
        }
    }
    Ok(())
}

async fn socket(url: &str) -> Result<Socket, String> {
    let parsed = Url::parse(url).map_err(|_| "Ungültige Twitch-WebSocket-URL.".to_string())?;
    if parsed.scheme() != "wss" || parsed.host_str() != Some("eventsub.wss.twitch.tv") { return Err("DAUERHAFT:Unerwarteter Twitch-WebSocket-Endpunkt.".into()); }
    tokio::time::timeout(Duration::from_secs(20), connect_async(url)).await.map_err(|_| "Twitch-WebSocket Zeitlimit.".to_string())?.map(|pair| pair.0).map_err(|_| "Twitch-WebSocket nicht erreichbar.".to_string())
}
async fn welcome(socket: &mut Socket) -> Result<Value, String> {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(15), socket.next()).await.map_err(|_| "Twitch-Welcome Zeitlimit.".to_string())?.ok_or_else(|| "Twitch-WebSocket wurde geschlossen.".to_string())?.map_err(|_| "Twitch-WebSocket Fehler.".to_string())?;
        match frame {
            Message::Text(data) => {
                let event: Value = serde_json::from_str(&data).map_err(|_| "Ungültiges Twitch-Ereignis.".to_string())?;
                if event["metadata"]["message_type"] == "session_welcome" { return Ok(event["payload"]["session"].clone()); }
            }
            Message::Ping(bytes) => { socket.send(Message::Pong(bytes)).await.map_err(|_| "Twitch-Pong fehlgeschlagen.".to_string())?; }
            Message::Close(_) => return Err("Twitch-WebSocket wurde geschlossen.".into()),
            _ => {}
        }
    }
}

async fn migrate(old: &mut Socket, url: &str, app: &tauri::AppHandle, seen: &mut Seen, own_id: &str) -> Result<(Socket, Value), String> {
    // Continue dispatching the previous connection during the replacement handshake.
    let connect = socket(url);
    tokio::pin!(connect);
    let mut replacement = loop {
        tokio::select! {
            result = &mut connect => break result?,
            frame = old.next() => {
                if let Some(Ok(frame)) = frame { migration_frame(old, frame, app, seen, own_id).await?; }
                else { break connect.await?; }
            }
        }
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let session = loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return Err("Twitch-Reconnect-Welcome Zeitlimit.".into()),
            frame = old.next() => {
                if let Some(Ok(frame)) = frame { migration_frame(old, frame, app, seen, own_id).await?; }
                else { break welcome(&mut replacement).await?; }
            }
            frame = replacement.next() => {
                let frame = frame.ok_or_else(|| "Twitch-Reconnect geschlossen.".to_string())?.map_err(|_| "Twitch-Reconnect fehlgeschlagen.".to_string())?;
                match frame {
                    Message::Text(data) => {
                        let event: Value = serde_json::from_str(&data).map_err(|_| "Ungültiger Twitch-Reconnect.".to_string())?;
                        if event["metadata"]["message_type"] == "session_welcome" { break event["payload"]["session"].clone(); }
                    }
                    Message::Ping(bytes) => { replacement.send(Message::Pong(bytes)).await.map_err(|_| "Twitch-Pong fehlgeschlagen.".to_string())?; }
                    Message::Close(_) => return Err("Twitch-Reconnect geschlossen.".into()),
                    _ => {}
                }
            }
        }
    };
    let _ = old.close(None).await;
    Ok((replacement, session))
}

async fn migration_frame(socket: &mut Socket, frame: Message, app: &tauri::AppHandle, seen: &mut Seen, own_id: &str) -> Result<(), String> {
    match frame {
        Message::Text(data) => {
            if data.len() > 256 * 1024 { return Err("Twitch-Ereignis zu groß.".into()); }
            let event: Value = serde_json::from_str(&data).map_err(|_| "Ungültiges Twitch-Ereignis.".to_string())?;
            if event["metadata"]["message_type"] == "notification" { emit_twitch(app, &event, seen, own_id); }
            if event["metadata"]["message_type"] == "revocation" { return Err("DAUERHAFT:Twitch hat das Chatabonnement widerrufen.".into()); }
        }
        Message::Ping(bytes) => { socket.send(Message::Pong(bytes)).await.map_err(|_| "Twitch-Pong fehlgeschlagen.".to_string())?; }
        _ => {}
    }
    Ok(())
}

async fn twitch_session(http: &Client, config: &Value, token: &str, own_id: &str, app: &tauri::AppHandle, cancel: &AtomicBool, seen: &mut Seen, context: &Mutex<ConnectionContext>) -> Result<(), String> {
    let mut ws = socket("wss://eventsub.wss.twitch.tv/ws?keepalive_timeout_seconds=30").await?;
    let mut session = welcome(&mut ws).await?;
    let session_id = session["id"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| "Twitch lieferte keine Session-ID.".to_string())?;
    // Subscribe immediately after welcome (Twitch's default deadline is ten seconds).
    let mut notifications = true;
    for kind in ["channel.chat.message", "channel.chat.notification"] {
        let response = http.post("https://api.twitch.tv/helix/eventsub/subscriptions")
            .bearer_auth(token).header("Client-Id", text(config,"clientId"))
            .json(&json!({"type":kind,"version":"1","condition":{"broadcaster_user_id":text(config,"broadcasterId"),"user_id":own_id},"transport":{"method":"websocket","session_id":session_id}}))
            .send().await.map_err(|_| "Twitch-Abonnement konnte nicht erstellt werden.".to_string())?;
        if !response.status().is_success() {
            if kind == "channel.chat.message" {
                return Err(format!("DAUERHAFT:Twitch-Chatabonnement abgelehnt (HTTP {}). Token und Kanal-ID prüfen.", response.status().as_u16()));
            }
            // Chat messages remain usable when optional chat notifications are unavailable.
            notifications = false;
        }
    }
    context.lock().await.connected = true;
    connection(app, "twitch", "connected", if notifications { "Twitch EventSub Live-Chat und Chat-Ereignisse verbunden." } else { "Twitch Live-Chat verbunden; zusätzliche Chat-Ereignisse sind nicht verfügbar." });
    let mut deadline = tokio::time::Instant::now() + Duration::from_secs(session["keepalive_timeout_seconds"].as_u64().unwrap_or(30) + 5);
    while !cancelled(cancel) {
        let incoming = tokio::select! {
            frame = ws.next() => frame,
            _ = tokio::time::sleep_until(deadline) => return Err("Twitch-Keepalive ausgeblieben.".into()),
            _ = tokio::time::sleep(Duration::from_millis(200)) => continue,
        };
        let frame = incoming.ok_or_else(|| "Twitch-WebSocket geschlossen.".to_string())?.map_err(|_| "Twitch-WebSocket unterbrochen.".to_string())?;
        match frame {
            Message::Ping(bytes) => { ws.send(Message::Pong(bytes)).await.map_err(|_| "Twitch-Pong fehlgeschlagen.".to_string())?; }
            Message::Text(data) => {
                if data.len() > 256 * 1024 { return Err("Twitch-Ereignis zu groß.".into()); }
                let event: Value = serde_json::from_str(&data).map_err(|_| "Ungültiges Twitch-Ereignis.".to_string())?;
                match event["metadata"]["message_type"].as_str().unwrap_or("") {
                    "notification" => { emit_twitch(app, &event, seen, own_id); }
                    "revocation" => return Err("DAUERHAFT:Twitch hat das Chatabonnement widerrufen. Anmeldung und Berechtigungen prüfen.".into()),
                    "session_reconnect" => {
                        let url = event["payload"]["session"]["reconnect_url"].as_str().ok_or_else(|| "Twitch-Reconnect-URL fehlt.".to_string())?;
                        // Keep the old socket open until the replacement welcomes us. The subscription migrates automatically.
                        let (replacement, replacement_session) = migrate(&mut ws, url, app, seen, own_id).await?;
                        ws = replacement;
                        session = replacement_session;
                    }
                    _ => {}
                }
                deadline = tokio::time::Instant::now() + Duration::from_secs(session["keepalive_timeout_seconds"].as_u64().unwrap_or(30) + 5);
            }
            Message::Close(_) => return Err("Twitch-WebSocket geschlossen.".into()),
            _ => {}
        }
    }
    let _ = ws.close(None).await;
    Ok(())
}

fn emit_twitch(app: &tauri::AppHandle, event: &Value, seen: &mut Seen, own_id: &str) {
    let details = &event["payload"]["event"];
    if details["chatter_user_id"].as_str() == Some(own_id) { return; }
    let id = details["message_id"].as_str().unwrap_or_else(|| event["metadata"]["message_id"].as_str().unwrap_or(""));
    if !seen.insert(id) { return; }
    let kind = event["metadata"]["subscription_type"].as_str().unwrap_or("");
    let message = if kind == "channel.chat.notification" {
        let content = details["message"]["text"].as_str().unwrap_or("");
        let system = details["system_message"].as_str().unwrap_or("");
        if content.is_empty() { system.to_string() } else if system.is_empty() { content.to_string() } else { format!("{system} {content}") }
    } else { details["message"]["text"].as_str().unwrap_or("").to_string() };
    if message.is_empty() { return; }
    let user = details["chatter_user_name"].as_str().filter(|s| !s.is_empty()).unwrap_or("Twitch");
    let _ = app.emit("cohost://chat", json!({"id":id,"platform":"twitch","user":user,"userId":details["chatter_user_id"].as_str().unwrap_or(""),"text":message,"createdAt":event["metadata"]["message_timestamp"],"eventType":if kind == "channel.chat.notification" { details["notice_type"].as_str().unwrap_or("notification") } else { "message" }}));
}

fn youtube_video(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.len() == 11 && input.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return Ok(input.to_string()); }
    let url = Url::parse(input).map_err(|_| "Bitte eine gültige YouTube-Video-ID oder URL angeben.".to_string())?;
    let host = url.host_str().unwrap_or("");
    let candidate = if host == "youtu.be" { url.path_segments().and_then(|mut parts| parts.next()).map(str::to_string) }
        else if matches!(host, "youtube.com" | "www.youtube.com" | "m.youtube.com") {
            url.query_pairs().find(|(key,_)| key == "v").map(|(_,value)| value.into_owned()).or_else(|| {
                let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
                if parts.len() >= 2 && matches!(parts[0], "live" | "shorts" | "embed") { Some(parts[1].to_string()) } else { None }
            })
        } else { None };
    candidate.filter(|id| id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).ok_or_else(|| "Die URL enthält keine gültige YouTube-Video-ID.".into())
}

async fn youtube(config: Value, key: String, app: tauri::AppHandle, cancel: Arc<AtomicBool>, context: Arc<Mutex<ConnectionContext>>) -> Result<(), String> {
    let http = client()?;
    let mut live_chat_id = text(&config, "liveChatId").to_string();
    if live_chat_id.is_empty() {
        let video = youtube_video(text(&config, "videoId"))?;
        let response = http.get("https://www.googleapis.com/youtube/v3/videos").query(&[("part","liveStreamingDetails"),("id",&video),("key",key.trim())]).send().await.map_err(|_| "YouTube-Video konnte nicht aufgelöst werden.".to_string())?;
        let data = json_response(response, "YouTube-Video").await?;
        live_chat_id = data["items"].as_array().and_then(|items| items.first()).and_then(|item| item["liveStreamingDetails"]["activeLiveChatId"].as_str()).unwrap_or("").to_string();
        if live_chat_id.is_empty() { return Err("Dieses YouTube-Video hat keinen aktiven öffentlichen Live-Chat.".into()); }
    }
    let mut seen = Seen::new();
    let mut page = String::new();
    let mut failures = 0u32;
    let mut connected = false;
    while !cancelled(&cancel) {
        let mut request = http.get("https://www.googleapis.com/youtube/v3/liveChat/messages").query(&[("liveChatId",live_chat_id.as_str()),("part","snippet,authorDetails"),("maxResults","200"),("key",key.trim())]);
        if !page.is_empty() { request = request.query(&[("pageToken",page.as_str())]); }
        let response = match request.send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) if matches!(response.status().as_u16(), 401 | 403 | 404) => { return json_response(response, "YouTube Live-Chat").await.map(|_| ()); }
            _ => {
                failures += 1;
                connected = false;
                context.lock().await.connected = false;
                if failures >= 8 { return Err("YouTube-Verbindung wiederholt fehlgeschlagen. API-Key und Kontingent prüfen.".into()); }
                let delay = (2u64.pow(failures.min(5))).max(5).min(60);
                connection(&app,"youtube","connecting",&format!("YouTube unterbrochen; neuer Versuch in {delay} s."));
                if !sleep(&cancel,Duration::from_secs(delay)).await { return Ok(()); }
                continue;
            }
        };
        let data = json_response(response, "YouTube Live-Chat").await?;
        if !connected {
            { let mut current = context.lock().await; current.connected = true; current.live_chat_id = Some(live_chat_id.clone()); }
            connection(&app,"youtube","connected","YouTube Live-Chat verbunden (API-Polling)."); connected = true;
        }
        failures = 0;
        page = data["nextPageToken"].as_str().unwrap_or("").to_string();
        if let Some(items) = data["items"].as_array() {
            for item in items {
                if context.lock().await.user_id.as_deref().is_some_and(|id| item["authorDetails"]["channelId"].as_str() == Some(id)) { continue; }
                let id = item["id"].as_str().unwrap_or("");
                if !seen.insert(id) { continue; }
                let snippet = &item["snippet"];
                let raw_kind = snippet["type"].as_str().unwrap_or("textMessageEvent");
                let kind = if raw_kind == "textMessageEvent" { "message" } else { raw_kind };
                let message = snippet["displayMessage"].as_str().or_else(|| snippet["textMessageDetails"]["messageText"].as_str()).unwrap_or("");
                if message.is_empty() { continue; }
                let _ = app.emit("cohost://chat",json!({"id":id,"platform":"youtube","user":item["authorDetails"]["displayName"].as_str().unwrap_or("YouTube"),"userId":item["authorDetails"]["channelId"].as_str().unwrap_or(""),"text":message,"createdAt":snippet["publishedAt"],"eventType":kind}));
            }
        }
        if data.get("offlineAt").is_some_and(|v| !v.is_null()) { return Err("Der YouTube-Livestream wurde beendet.".into()); }
        // Never poll before the server interval. Five seconds is a conservative fallback.
        let interval = data["pollingIntervalMillis"].as_u64().unwrap_or(5000).max(1000);
        if !sleep(&cancel,Duration::from_millis(interval)).await { return Ok(()); }
    }
    Ok(())
}
