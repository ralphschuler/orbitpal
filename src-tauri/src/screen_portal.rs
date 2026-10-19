//! Session-only Wayland sources. A portal lease is not a bot image grant.
//! No restore tokens, screenshots, frame caches or model-callable controls.
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashMap, sync::{Arc, Mutex, Weak, atomic::{AtomicBool, Ordering}, mpsc}, time::{Duration, Instant}};

#[cfg(target_os = "linux")]
#[path = "screen_portal_linux.rs"]
mod linux;

pub fn is_wayland() -> bool {
    cfg!(target_os = "linux") && (std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland")))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub backend: &'static str,
    pub phase: &'static str,
    pub display_id: String,
    pub message: String,
}

struct FrameRequest {
    #[cfg_attr(not(target_os="linux"),allow(dead_code))]
    cancel: Arc<AtomicBool>,
    reply: mpsc::SyncSender<Result<image::RgbImage, String>>,
}

struct Session {
    display_id: String,
    model: Value,
    cancelled: AtomicBool,
    status: Mutex<Status>,
    turns: Mutex<Vec<Weak<AtomicBool>>>,
    sender: Mutex<Option<mpsc::SyncSender<FrameRequest>>>,
}

impl Session {
    fn new(model: Value) -> Self {
        let display_id = format!("wayland:{}", uuid::Uuid::new_v4());
        Self { display_id: display_id.clone(), model, cancelled: AtomicBool::new(false),
            status: Mutex::new(Status { backend: "wayland", phase: "selecting", display_id,
                message: "Quelle im Wayland-Freigabedialog auswählen".into() }),
            turns: Mutex::new(Vec::new()), sender: Mutex::new(None) }
    }

    fn close(&self, message: &str) {
        if self.cancelled.swap(true, Ordering::SeqCst) { return; }
        if let Ok(mut turns) = self.turns.lock() {
            for cancel in turns.drain(..).filter_map(|v| v.upgrade()) { cancel.store(true, Ordering::SeqCst); }
        }
        if let Ok(mut sender) = self.sender.lock() { sender.take(); }
        if let Ok(mut status) = self.status.lock() {
            status.phase = "closed"; status.message = message.into();
        }
    }

    fn ready(&self, sender: mpsc::SyncSender<FrameRequest>) -> Result<(), String> {
        let mut current = self.sender.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?;
        if self.cancelled.load(Ordering::SeqCst) { return Err("Wayland-Auswahl wurde abgebrochen".into()); }
        *current = Some(sender);
        let mut status = self.status.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?;
        status.phase = "ready"; status.message = "Wayland-Quelle verbunden; Bilder nur bei Modellanfragen".into();
        Ok(())
    }

    fn snapshot(&self) -> Result<Status, String> {
        self.status.lock().map(|v| v.clone()).map_err(|_| "Wayland-Sitzung ist nicht verfügbar".into())
    }

    fn attach(&self, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let mut turns = self.turns.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?;
        if self.cancelled.load(Ordering::SeqCst) { return Err("Wayland-Freigabe wurde beendet. Bitte die Quelle erneut auswählen.".into()); }
        turns.retain(|v| v.strong_count() > 0); turns.push(Arc::downgrade(cancel)); Ok(())
    }

    fn capture(&self, cancel: Arc<AtomicBool>) -> Result<crate::vision::ValidatedImage, String> {
        self.attach(&cancel)?;
        let (reply, response) = mpsc::sync_channel(1);
        {
            let sender = self.sender.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?;
            sender.as_ref().ok_or("Bitte die Wayland-Quelle zuerst ausdrücklich auswählen")?
                .try_send(FrameRequest { cancel: cancel.clone(), reply })
                .map_err(|_| "Die Wayland-Aufnahme ist beschäftigt oder wurde getrennt".to_owned())?;
        }
        let deadline = Instant::now() + Duration::from_secs(7);
        loop {
            if self.cancelled.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) { return Err("Wayland-Aufnahme abgebrochen".into()); }
            if Instant::now() >= deadline { cancel.store(true, Ordering::SeqCst); return Err("Wayland hat kein frisches Bild geliefert. Bitte die Quelle erneut auswählen.".into()); }
            match response.recv_timeout(Duration::from_millis(30)) {
                Ok(image) => {
                    let image = image?;
                    if self.cancelled.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) { return Err("Wayland-Aufnahme abgebrochen".into()); }
                    return crate::screen_capture::encode(image);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(_) => return Err("Die Wayland-Aufnahme wurde getrennt. Bitte die Quelle erneut auswählen.".into()),
            }
        }
    }
}

#[derive(Default)]
pub struct PortalState { sessions: Mutex<HashMap<String, Arc<Session>>> }

impl PortalState {
    fn begin(&self, bot_id: &str, model: Value) -> Result<Arc<Session>, String> {
        let session = Arc::new(Session::new(model));
        let previous = self.sessions.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?.insert(bot_id.into(), session.clone());
        if let Some(previous) = previous { previous.close("Wayland-Quelle wird neu ausgewählt"); }
        Ok(session)
    }

    pub fn status(&self, bot_id: &str) -> Result<Status, String> {
        if !is_wayland() { return Ok(Status { backend: "native", phase: "not_required", display_id: String::new(), message: String::new() }); }
        match self.sessions.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?.get(bot_id) {
            Some(session) => session.snapshot(),
            None => Ok(Status { backend: "wayland", phase: "closed", display_id: String::new(), message: "Bitte die Wayland-Quelle ausdrücklich auswählen".into() }),
        }
    }

    pub async fn select(&self, bot_id: &str, model: Value) -> Result<Status, String> {
        if !is_wayland() { return Err("Der Wayland-Dialog ist nur in einer Linux-Wayland-Sitzung verfügbar".into()); }
        let session = self.begin(bot_id, model)?;
        #[cfg(target_os = "linux")]
        if let Err(error) = linux::select(session.clone()).await { session.close(&error); return Err(error); }
        let status=session.snapshot()?;
        if status.phase!="ready"||session.cancelled.load(Ordering::SeqCst){return Err(status.message);}
        Ok(status)
    }

    pub fn close_bot(&self, bot_id: &str, message: &str) {
        if let Ok(sessions) = self.sessions.lock() { if let Some(session) = sessions.get(bot_id) { session.close(message); } }
    }

    pub fn close_all(&self) {
        if let Ok(sessions) = self.sessions.lock() { for session in sessions.values() { session.close("Wayland-Freigabe durch Stop oder Beenden geschlossen"); } }
    }

    pub fn reconcile(&self, profile: &Value) -> Result<(), String> {
        let config = crate::screen_context::config(profile)?;
        if let Some(session) = self.sessions.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?.get(profile["id"].as_str().unwrap_or_default()) {
            if !config.enabled || config.display_id != session.display_id || profile["model"] != session.model {
                session.close("Bildschirm- oder Modelleinstellungen wurden geändert. Bitte die Wayland-Quelle erneut auswählen.");
            }
        }
        Ok(())
    }

    pub fn capture_for(&self, profile: &Value, cancel: Arc<AtomicBool>) -> Result<impl FnOnce() -> Result<crate::vision::ValidatedImage, String> + Send + 'static, String> {
        let config = crate::screen_context::config(profile)?;
        let session = self.sessions.lock().map_err(|_| "Wayland-Sitzung ist nicht verfügbar")?
            .get(profile["id"].as_str().ok_or("Bot-ID fehlt")?).cloned().ok_or("Bitte die Wayland-Quelle zuerst ausdrücklich auswählen")?;
        if !config.enabled || config.display_id != session.display_id || profile["model"] != session.model || session.cancelled.load(Ordering::SeqCst) || session.snapshot()?.phase!="ready" {
            return Err("Wayland-Quelle gehört nicht zur aktuellen Bot-, Bildschirm- und Modellauswahl. Bitte erneut auswählen.".into());
        }
        session.attach(&cancel)?;
        Ok(move || session.capture(cancel))
    }
}

#[tauri::command]
pub fn get_screen_session(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>, bot_id: String) -> Result<Status, String> {
    main_only(window.label())?; state.store.bot_profile(&bot_id)?;
    state.screen_context.portal.status(&bot_id)
}

#[tauri::command]
pub async fn select_screen_source(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>, bot_id: String) -> Result<Status, String> {
    main_only(window.label())?; state.updater.ensure_idle()?;
    let profile = state.store.bot_profile(&bot_id)?;
    state.screen_context.portal.select(&bot_id, profile["model"].clone()).await
}

#[tauri::command]
pub fn disconnect_screen_source(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>, bot_id: String) -> Result<(), String> {
    main_only(window.label())?; state.store.bot_profile(&bot_id)?;
    state.screen_context.portal.close_bot(&bot_id, "Wayland-Freigabe im Studio getrennt"); Ok(())
}

fn main_only(label: &str) -> Result<(), String> {
    if label == "main" { Ok(()) } else { Err("Bildschirmquellen werden ausschließlich vom Streamer im Studio freigegeben".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn profile(id: &str, session: &Session) -> Value { json!({"id":id,"screenContext":{"enabled":true,"displayId":session.display_id},"model":session.model}) }

    #[test] fn portal_lease_is_not_a_grant_and_cannot_cross_bots_or_destinations() {
        let hub=PortalState::default();let session=hub.begin("nova",json!({"model":"fixture"})).unwrap();
        let (sender,_receiver)=mpsc::sync_channel(1);session.ready(sender).unwrap();
        let mut current=profile("nova",&session);let cancel=Arc::new(AtomicBool::new(false));
        assert!(hub.capture_for(&profile("other",&session),cancel.clone()).is_err());
        current["screenContext"]["enabled"]=json!(false);assert!(hub.capture_for(&current,cancel.clone()).is_err());
        current=profile("nova",&session);current["model"]["model"]=json!("changed");assert!(hub.capture_for(&current,cancel.clone()).is_err());
        assert!(hub.capture_for(&profile("nova",&session),cancel).is_ok());
        for label in ["avatar-nova","main-spoof",""] { assert!(main_only(label).is_err()); } assert!(main_only("main").is_ok());
    }
    #[test] fn revocation_restart_and_bot_changes_cannot_revive_pending_frames() {
        let hub=PortalState::default();let old=hub.begin("nova",json!({"model":"fixture"})).unwrap();
        let cancel=Arc::new(AtomicBool::new(false));old.attach(&cancel).unwrap();let other=hub.begin("other",old.model.clone()).unwrap();
        let newer=hub.begin("nova",old.model.clone()).unwrap();assert!(cancel.load(Ordering::SeqCst));assert_ne!(newer.display_id,old.display_id);
        assert!(old.attach(&Arc::new(AtomicBool::new(false))).is_err());assert!(!other.cancelled.load(Ordering::SeqCst));
        let (sender,_)=mpsc::sync_channel(1);assert!(old.ready(sender).is_err());
        hub.reconcile(&profile("nova",&newer)).unwrap();assert!(!newer.cancelled.load(Ordering::SeqCst));
        let mut changed=profile("nova",&newer);changed["screenContext"]["displayId"]=json!("different");hub.reconcile(&changed).unwrap();assert!(newer.cancelled.load(Ordering::SeqCst));
        hub.close_all();assert!(other.cancelled.load(Ordering::SeqCst));
    }
    #[test] fn fresh_requests_have_no_cached_frame_and_late_results_are_discarded() {
        let hub=PortalState::default();let session=hub.begin("nova",json!({"model":"fixture"})).unwrap();
        let (sender,receiver)=mpsc::sync_channel::<FrameRequest>(1);session.ready(sender).unwrap();
        let worker=std::thread::spawn(move || { for value in [30,160] { let request=receiver.recv().unwrap();request.reply.send(Ok(image::RgbImage::from_pixel(1,1,image::Rgb([value,20,80])))).unwrap(); } });
        let mut last=String::new();for _ in 0..2 { let image=hub.capture_for(&profile("nova",&session),Arc::new(AtomicBool::new(false))).unwrap()().unwrap();assert_ne!(image.base64,last);last=image.base64; }worker.join().unwrap();
        let cancel=Arc::new(AtomicBool::new(false));session.attach(&cancel).unwrap();session.close("fixture revoke");assert!(cancel.load(Ordering::SeqCst));assert!(session.capture(Arc::new(AtomicBool::new(false))).is_err());
        assert!(!serde_json::to_string(&session.snapshot().unwrap()).unwrap().contains("base64"));
    }
    #[test] fn revoke_while_waiting_discards_a_late_synthetic_frame_and_model_changes_cancel_turns() {
        let hub=PortalState::default();let session=hub.begin("nova",json!({"model":"fixture"})).unwrap();
        let (sender,receiver)=mpsc::sync_channel::<FrameRequest>(1);session.ready(sender).unwrap();
        let capture=hub.capture_for(&profile("nova",&session),Arc::new(AtomicBool::new(false))).unwrap();
        let task=std::thread::spawn(capture);let request=receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut edited=profile("nova",&session);edited["model"]["baseUrl"]=json!("https://changed.invalid");hub.reconcile(&edited).unwrap();
        assert!(request.cancel.load(Ordering::SeqCst));
        let _=request.reply.send(Ok(image::RgbImage::from_pixel(2,2,image::Rgb([255,0,0]))));
        assert!(task.join().unwrap().is_err());assert!(hub.capture_for(&profile("nova",&session),Arc::new(AtomicBool::new(false))).is_err());
    }
}
