//! Real Twitch EventSub and YouTube Live Chat connections, with bounded deduplication.
//! YouTube intentionally uses liveChatMessages.list and its pollingIntervalMillis.
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet, VecDeque}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::Emitter;
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message, Connector, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub use crate::chat_route::ChatRoute;
use crate::chat_route::ConnectionContext;
struct Running { cancel: Arc<AtomicBool>, task: JoinHandle<()>, app: tauri::AppHandle, context: Arc<Mutex<ConnectionContext>> }
#[derive(Default)]
pub struct ChatHub { tasks: Mutex<HashMap<String, Running>> }

fn text<'a>(v: &'a Value, key: &str) -> &'a str { v[key].as_str().unwrap_or("").trim() }
fn connection(app: &tauri::AppHandle, platform: &str, status: &str, message: &str, owner: &Value) {
    let _ = app.emit("cohost://connection", json!({"platform":platform,"status":status,"message":message,"ownerBotId":owner["ownerBotId"],"ownerName":owner["ownerName"]}));
}
fn client() -> Result<Client, String> {
    Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(35)).build().map_err(|_| "HTTP-Client konnte nicht erstellt werden.".into())
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
    pub async fn connect(&self, platform: &str, profile: &Value, secret: Option<String>, write_secret: Option<String>, app: tauri::AppHandle) -> Result<(), String> {
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
        let writer = if platform == "twitch" { Some(secret.clone()) } else { write_secret };
        let context = Arc::new(Mutex::new(ConnectionContext::new(platform, profile, writer, cancel.clone())?));
        let task_context = context.clone();
        let name = platform.to_string();
        connection(&app, platform, "connecting", "Verbindung wird hergestellt …", &context.lock().await.public_owner());
        let task = tokio::spawn(async move {
            let result = if name == "twitch" { twitch(config, secret, task_app.clone(), task_cancel.clone(), task_context.clone()).await } else { youtube(config, secret, task_app.clone(), task_cancel.clone(), task_context.clone()).await };
            task_context.lock().await.disconnected();
            if !cancelled(&task_cancel) {
                match result { Ok(()) => connection(&task_app, &name, "disconnected", "Verbindung beendet.", &task_context.lock().await.public_owner()), Err(error) => connection(&task_app, &name, "error", &error, &task_context.lock().await.public_owner()) }
            }
        });
        tasks.insert(platform.to_string(), Running { cancel, task, app, context });
        Ok(())
    }
    /// Resolve only an opaque identifier emitted by the active connection. User
    /// text/model output cannot select a destination or manufacture provenance.
    pub async fn resolve_route(&self, platform:&str, id:Option<&str>, responder:&Value)->Result<ChatRoute,String> {
        let tasks=self.tasks.lock().await;
        let running=tasks.get(platform).filter(|running|!running.task.is_finished()).ok_or("Die Chatverbindung ist nicht mehr aktiv. Bitte eine neue Nachricht auswählen.")?;
        let route=running.context.lock().await.route(platform,id,responder);
        route
    }
    /// Called only once, after the admitted speech job actually starts playing.
    /// All destination fields and the token come from its original connection.
    pub async fn send(&self, route:&ChatRoute, store:&crate::storage::Store, message:&str, job_cancel:&AtomicBool)->Result<(),String> {
        let (context,cancel)={
            let tasks=self.tasks.lock().await;
            let running=tasks.get(route.platform()).filter(|running|!running.task.is_finished()).ok_or("Chatantwort benötigt ihre ursprüngliche aktive Verbindung.")?;
            (running.context.clone(),running.cancel.clone())
        };
        if job_cancel.load(Ordering::SeqCst) {return Err("Chatantwort abgebrochen.".into());}
        let reply=crate::chat_route::prepare_current(&context,route,||Ok((store.bot_profile(route.responder_id())?,store.bot_profile(route.owner_id())?)),message).await?;
        let http=client()?;
        let request=reply_request(&http,&reply)?;
        // Disconnect/replacement invalidates a pending post rather than silently
        // redirecting it to the next connection. An already transmitted request
        // cannot be withdrawn from the platform.
        let result=tokio::select! {
            biased;
            _=async { while !cancel.load(Ordering::SeqCst) && !job_cancel.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_millis(20)).await; } }=>return Err("Chatantwort nach Verbindungswechsel abgebrochen.".into()),
            result=request.send()=>result.map_err(|_|"Chatantwort konnte nicht gesendet werden.".to_string())?,
        };
        let data=json_response(result,if reply.platform=="twitch" {"Twitch-Chatantwort (user:write:chat erforderlich)"} else {"YouTube-Chatantwort (OAuth erforderlich)"}).await?;
        if reply.platform=="twitch" {
            if !data["data"].as_array().and_then(|items|items.first()).is_some_and(|item|item["is_sent"].as_bool()==Some(true)) {return Err("Twitch hat die Chatantwort nicht zugestellt; Kanalregeln oder Rate-Limit prüfen.".into());}
        } else if data["id"].as_str().unwrap_or("").is_empty() {return Err("YouTube hat keine gesendete Chatnachricht bestätigt.".into());}
        Ok(())
    }
    pub async fn disconnect(&self, platform: &str) -> Result<(), String> {
        if !matches!(platform, "twitch" | "youtube") { return Err("Unbekannte Chatplattform.".into()); }
        if let Some(running) = self.tasks.lock().await.remove(platform) {
            running.cancel.store(true, Ordering::Relaxed);
            running.task.abort();
            running.context.lock().await.disconnected();
            connection(&running.app, platform, "disconnected", "Getrennt.", &running.context.lock().await.public_owner());
        }
        Ok(())
    }
}
impl Drop for ChatHub {
    fn drop(&mut self) {
        if let Ok(tasks) = self.tasks.try_lock() { for running in tasks.values() { running.cancel.store(true, Ordering::Relaxed); running.task.abort(); } }
    }
}

fn reply_request(http:&Client, reply:&crate::chat_route::PreparedReply)->Result<reqwest::RequestBuilder,String> {
    let request=if reply.platform=="twitch" {
        http.post("https://api.twitch.tv/helix/chat/messages").bearer_auth(&reply.token).header("Client-Id",&reply.client_id)
            .json(&json!({"broadcaster_id":reply.channel,"sender_id":reply.sender,"message":reply.message}))
    } else if reply.platform=="youtube" {
        http.post("https://www.googleapis.com/youtube/v3/liveChat/messages").bearer_auth(&reply.token).query(&[("part","snippet")])
            .json(&json!({"snippet":{"liveChatId":reply.channel,"type":"textMessageEvent","textMessageDetails":{"messageText":reply.message}}}))
    } else { return Err("Unbekannte Chatplattform.".into()); };
    Ok(request)
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
                context.lock().await.disconnected();
                failures += 1;
                if failures >= 8 { return Err("Twitch-Verbindung wiederholt fehlgeschlagen. Netzwerk und Token prüfen und neu verbinden.".into()); }
                let seconds = (2u64.pow(failures.min(5))).min(30);
                connection(&app, "twitch", "connecting", &format!("Verbindung unterbrochen; neuer Versuch in {seconds} s."), &context.lock().await.public_owner());
                if !sleep(&cancel, Duration::from_secs(seconds)).await { return Ok(()); }
            }
        }
    }
    Ok(())
}

async fn socket(url: &str) -> Result<Socket, String> {
    let parsed = Url::parse(url).map_err(|_| "Ungültige Twitch-WebSocket-URL.".to_string())?;
    if parsed.scheme() != "wss" || parsed.host_str() != Some("eventsub.wss.twitch.tv") { return Err("DAUERHAFT:Unerwarteter Twitch-WebSocket-Endpunkt.".into()); }
    // Match tokio-tungstenite's previous Mozilla-root configuration, loading
    // patched roots directly instead of its legacy 0.26 re-export dependency.
    // Rustls retains normal certificate-chain and hostname verification.
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let connect = connect_async_tls_with_config(url, None, false, Some(Connector::Rustls(Arc::new(config))));
    tokio::time::timeout(Duration::from_secs(20), connect).await.map_err(|_| "Twitch-WebSocket Zeitlimit.".to_string())?.map(|pair| pair.0).map_err(|_| "Twitch-WebSocket nicht erreichbar.".to_string())
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

async fn migrate(old: &mut Socket, url: &str, app: &tauri::AppHandle, seen: &mut Seen, own_id: &str, context: &Mutex<ConnectionContext>) -> Result<(Socket, Value), String> {
    // Continue dispatching the previous connection during the replacement handshake.
    let connect = socket(url);
    tokio::pin!(connect);
    let mut replacement = loop {
        tokio::select! {
            result = &mut connect => break result?,
            frame = old.next() => {
                if let Some(Ok(frame)) = frame { migration_frame(old, frame, app, seen, own_id, context).await?; }
                else { break connect.await?; }
            }
        }
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let session = loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return Err("Twitch-Reconnect-Welcome Zeitlimit.".into()),
            frame = old.next() => {
                if let Some(Ok(frame)) = frame { migration_frame(old, frame, app, seen, own_id, context).await?; }
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

async fn migration_frame(socket: &mut Socket, frame: Message, app: &tauri::AppHandle, seen: &mut Seen, own_id: &str, context: &Mutex<ConnectionContext>) -> Result<(), String> {
    match frame {
        Message::Text(data) => {
            if data.len() > 256 * 1024 { return Err("Twitch-Ereignis zu groß.".into()); }
            let event: Value = serde_json::from_str(&data).map_err(|_| "Ungültiges Twitch-Ereignis.".to_string())?;
            if event["metadata"]["message_type"] == "notification" { emit_twitch(app, &event, seen, own_id, context).await; }
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
    context.lock().await.mark_connected(text(config,"broadcasterId"));
    connection(app, "twitch", "connected", if notifications { "Twitch EventSub Live-Chat und Chat-Ereignisse verbunden." } else { "Twitch Live-Chat verbunden; zusätzliche Chat-Ereignisse sind nicht verfügbar." }, &context.lock().await.public_owner());
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
                    "notification" => { emit_twitch(app, &event, seen, own_id, context).await; }
                    "revocation" => return Err("DAUERHAFT:Twitch hat das Chatabonnement widerrufen. Anmeldung und Berechtigungen prüfen.".into()),
                    "session_reconnect" => {
                        let url = event["payload"]["session"]["reconnect_url"].as_str().ok_or_else(|| "Twitch-Reconnect-URL fehlt.".to_string())?;
                        // Keep the old socket open until the replacement welcomes us. The subscription migrates automatically.
                        let (replacement, replacement_session) = migrate(&mut ws, url, app, seen, own_id, context).await?;
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

async fn emit_twitch(app: &tauri::AppHandle, event: &Value, seen: &mut Seen, own_id: &str, context: &Mutex<ConnectionContext>) {
    let details = &event["payload"]["event"];
    let Some(reply_route_id)=context.lock().await.incoming_route(details["broadcaster_user_id"].as_str().unwrap_or("")) else {return;};
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
    let _ = app.emit("cohost://chat", json!({"id":id,"platform":"twitch","replyRouteId":reply_route_id,"user":user,"userId":details["chatter_user_id"].as_str().unwrap_or(""),"text":message,"createdAt":event["metadata"]["message_timestamp"],"eventType":if kind == "channel.chat.notification" { details["notice_type"].as_str().unwrap_or("notification") } else { "message" }}));
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

// Read-key destinations are fixed host code, separate from OAuth reply identity.
fn youtube_video_request(http:&Client,video:&str,key:&str)->reqwest::RequestBuilder{
    http.get("https://www.googleapis.com/youtube/v3/videos").query(&[("part","liveStreamingDetails"),("id",video),("key",key.trim())])
}
fn youtube_messages_request(http:&Client,live_chat_id:&str,page:&str,key:&str)->reqwest::RequestBuilder{
    let request=http.get("https://www.googleapis.com/youtube/v3/liveChat/messages").query(&[("liveChatId",live_chat_id),("part","snippet,authorDetails"),("maxResults","200"),("key",key.trim())]);
    if page.is_empty(){request}else{request.query(&[("pageToken",page)])}
}

async fn youtube(config: Value, key: String, app: tauri::AppHandle, cancel: Arc<AtomicBool>, context: Arc<Mutex<ConnectionContext>>) -> Result<(), String> {
    let http = client()?;
    let mut live_chat_id = text(&config, "liveChatId").to_string();
    if live_chat_id.is_empty() {
        let video = youtube_video(text(&config, "videoId"))?;
        let response = youtube_video_request(&http,&video,&key).send().await.map_err(|_| "YouTube-Video konnte nicht aufgelöst werden.".to_string())?;
        let data = json_response(response, "YouTube-Video").await?;
        live_chat_id = data["items"].as_array().and_then(|items| items.first()).and_then(|item| item["liveStreamingDetails"]["activeLiveChatId"].as_str()).unwrap_or("").to_string();
        if live_chat_id.is_empty() { return Err("Dieses YouTube-Video hat keinen aktiven öffentlichen Live-Chat.".into()); }
    }
    // Freeze/validate the write identity during this explicit connection start.
    // A later profile or selected-bot token is never substituted for it.
    let writer=context.lock().await.write_token().map(str::to_owned);
    if let Some(token)=writer {
        let response=http.get("https://www.googleapis.com/youtube/v3/channels").bearer_auth(token.trim()).query(&[("part","id"),("mine","true")]).send().await.map_err(|_|"YouTube-Bot-Kanal konnte nicht aufgelöst werden.".to_string())?;
        let data=json_response(response,"YouTube-Schreibanmeldung").await?;
        let own_id=data["items"].as_array().and_then(|items|items.first()).and_then(|item|item["id"].as_str()).filter(|id|!id.is_empty()).ok_or("YouTube-Schreibtoken hat keinen zugeordneten Kanal.")?;
        context.lock().await.user_id=Some(own_id.into());
    }
    let mut seen = Seen::new();
    let mut page = String::new();
    let mut failures = 0u32;
    let mut connected = false;
    while !cancelled(&cancel) {
        let request=youtube_messages_request(&http,&live_chat_id,&page,&key);
        let response = match request.send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) if matches!(response.status().as_u16(), 401 | 403 | 404) => { return json_response(response, "YouTube Live-Chat").await.map(|_| ()); }
            _ => {
                failures += 1;
                connected = false;
                context.lock().await.disconnected();
                if failures >= 8 { return Err("YouTube-Verbindung wiederholt fehlgeschlagen. API-Key und Kontingent prüfen.".into()); }
                let delay = (2u64.pow(failures.min(5))).max(5).min(60);
                connection(&app,"youtube","connecting",&format!("YouTube unterbrochen; neuer Versuch in {delay} s."), &context.lock().await.public_owner());
                if !sleep(&cancel,Duration::from_secs(delay)).await { return Ok(()); }
                continue;
            }
        };
        let data = json_response(response, "YouTube Live-Chat").await?;
        if !connected {
            context.lock().await.mark_connected(&live_chat_id);
            connection(&app,"youtube","connected","YouTube Live-Chat verbunden (API-Polling).", &context.lock().await.public_owner()); connected = true;
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
                let Some(reply_route_id)=context.lock().await.incoming_route(snippet["liveChatId"].as_str().unwrap_or(&live_chat_id)) else {continue;};
                let _ = app.emit("cohost://chat",json!({"id":id,"platform":"youtube","replyRouteId":reply_route_id,"user":item["authorDetails"]["displayName"].as_str().unwrap_or("YouTube"),"userId":item["authorDetails"]["channelId"].as_str().unwrap_or(""),"text":message,"createdAt":snippet["publishedAt"],"eventType":kind}));
            }
        }
        if data.get("offlineAt").is_some_and(|v| !v.is_null()) { return Err("Der YouTube-Livestream wurde beendet.".into()); }
        // Never poll before the server interval. Five seconds is a conservative fallback.
        let interval = data["pollingIntervalMillis"].as_u64().unwrap_or(5000).max(1000);
        if !sleep(&cancel,Duration::from_millis(interval)).await { return Ok(()); }
    }
    Ok(())
}

#[cfg(test)]
mod route_request_tests {
    use super::*;
    use crate::chat_route::ConnectionContext;
    #[test]
    fn outgoing_requests_for_both_platforms_use_the_origin_not_the_selected_bot() {
        for platform in ["twitch","youtube"] {
            let config=if platform=="twitch" {json!({"clientId":"client-a","broadcasterId":"channel-a","userId":"account-a","secretId":"owner-auth","sendReplies":true})}
                else {json!({"liveChatId":"channel-a","apiKeyId":"owner-read","sendSecretId":"owner-auth","sendReplies":true})};
            let owner=json!({"id":"owner","name":"Owner","chat":{platform:config}});
            let mut second=owner.clone();second["id"]=json!("second-bot");
            let mut context=ConnectionContext::new(platform,&owner,Some("original-account-token".into()),Arc::new(AtomicBool::new(false))).unwrap();
            context.user_id=Some("account-a".into());context.mark_connected("channel-a");
            // Manual and automatic answers share the same admitted route path.
            for responder in [&owner,&second] {
                let route=context.route(platform,context.route_id(),responder).unwrap();
                let reply=context.prepare_reply(&route,responder,&owner,"Hello").unwrap();
                let request=reply_request(&Client::new(),&reply).unwrap().build().unwrap();
                assert_eq!(request.headers()["Authorization"],"Bearer original-account-token");
                let body:Value=serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
                if platform=="twitch" {
                    assert_eq!(request.url().as_str(),"https://api.twitch.tv/helix/chat/messages");assert_eq!(request.headers()["Client-Id"],"client-a");
                    assert_eq!(body,json!({"broadcaster_id":"channel-a","sender_id":"account-a","message":"Hello"}));
                } else {
                    assert_eq!(request.url().as_str(),"https://www.googleapis.com/youtube/v3/liveChat/messages?part=snippet");
                    assert_eq!(body["snippet"]["liveChatId"],"channel-a");
                }
                let mut changed=responder.clone();changed["chat"][platform][if platform=="twitch" {"broadcasterId"} else {"liveChatId"}]=json!("channel-b");
                assert!(context.prepare_reply(&route,&changed,&owner,"Must never reach channel-b").is_err());
            }
        }
    }
    #[test]
    fn managed_youtube_read_key_uses_only_the_fixed_data_api_not_oauth(){
        use crate::{api_connections::{AuthStyle,Purpose,Service},storage::{Store,api_connection_store::NewConnection}};
        let root=std::env::temp_dir().join(format!("orbitpal-youtube-global-{}",uuid::Uuid::new_v4()));let store=Store::new(root.clone()).unwrap();
        // cfg(test) uses the thread-local in-memory credential backend.
        store.set_secret("synthetic-youtube-item","synthetic-youtube-marker").unwrap();
        let created=store.create_api_connection(0,NewConnection{label:"YouTube read fixture".into(),service:Service::YoutubeData,destination:"https://www.googleapis.com/youtube/v3".into(),auth_style:AuthStyle::YoutubeApiKey,existing_secret_id:Some("synthetic-youtube-item".into())}).unwrap();
        let config=json!({"apiKeyId":created["connectionId"]});let key=store.api_key_for(&config,Purpose::YoutubeData).unwrap().unwrap();
        for request in [youtube_video_request(&client().unwrap(),"fixture-video",&key),youtube_messages_request(&client().unwrap(),"fixture-chat","next-page",&key)]{
            let request=request.build().unwrap();assert_eq!(request.url().scheme(),"https");assert_eq!(request.url().host_str(),Some("www.googleapis.com"));assert!(request.url().path().starts_with("/youtube/v3/"));
            assert_eq!(request.url().query_pairs().filter(|(k,_)|k=="key").map(|(k,v)|(k.into_owned(),v.into_owned())).collect::<Vec<_>>(),vec![("key".to_string(),"synthetic-youtube-marker".to_string())]);assert!(request.headers().get("authorization").is_none());
        }
        drop(store);std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn platform_http_client_does_not_forward_a_key_query_on_redirect(){
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let sink=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let source=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}/read?key=synthetic-query-marker",source.local_addr().unwrap());let target=sink.local_addr().unwrap();
        let server=tokio::spawn(async move{let(mut socket,_)=source.accept().await.unwrap();let mut request=vec![];loop{let mut chunk=[0;1024];let n=socket.read(&mut chunk).await.unwrap();assert_ne!(n,0);request.extend_from_slice(&chunk[..n]);assert!(request.len()<8192);if request.windows(4).any(|w|w==b"\r\n\r\n"){break;}}assert!(String::from_utf8(request).unwrap().starts_with("GET /read?key=synthetic-query-marker HTTP/1.1"));socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{target}/leak\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();});
        let response=tokio::time::timeout(Duration::from_secs(3),client().unwrap().get(url).send()).await.unwrap().unwrap();assert_eq!(response.status().as_u16(),302);server.await.unwrap();assert!(tokio::time::timeout(Duration::from_millis(100),sink.accept()).await.is_err());
    }
    #[test]
    fn incoming_events_cannot_claim_another_channel_or_a_revoked_session() {
        let owner=json!({"id":"owner","chat":{"twitch":{"sendReplies":true}}});
        let mut context=ConnectionContext::new("twitch",&owner,None,Arc::new(AtomicBool::new(false))).unwrap();
        assert!(context.incoming_route("channel-a").is_none());context.mark_connected("channel-a");
        assert!(context.incoming_route("channel-a").is_some());assert!(context.incoming_route("channel-b").is_none());
        context.disconnected();assert!(context.incoming_route("channel-a").is_none());
    }
}
