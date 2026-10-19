//! The OS permission is separate from WebView getUserMedia permission. Neither
//! a status query nor the settings link opens an audio device. Only an explicit
//! Studio action may request a previously undetermined OS grant.
use serde::Serialize;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Stable cross-platform wire vocabulary; not every OS emits every value.
pub enum Status { NotDetermined, Restricted, Denied, Authorized, Webview, Unknown }

#[derive(Serialize)]
pub struct Permission { platform: &'static str, status: Status }

fn studio(label: &str) -> Result<(), String> {
    if label == "main" { Ok(()) } else { Err("Mikrofonfreigaben werden ausschließlich im Studio verwaltet".into()) }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::Status;
    use block2::RcBlock;
    use objc2::{msg_send, runtime::{AnyClass, Bool}};
    use objc2_foundation::NSString;
    use std::{sync::Mutex, time::Duration};
    use tauri::Manager;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" { static AVMediaTypeAudio: &'static NSString; }

    fn device() -> Result<&'static AnyClass, String> {
        AnyClass::get(c"AVCaptureDevice").ok_or_else(|| "macOS-Mikrofonfreigabe ist nicht verfügbar".into())
    }

    pub fn status() -> Result<Status, String> {
        // AVFoundation's status query does not request access or record audio.
        let value: isize = unsafe { msg_send![device()?, authorizationStatusForMediaType: AVMediaTypeAudio] };
        Ok(match value { 0 => Status::NotDetermined, 1 => Status::Restricted, 2 => Status::Denied, 3 => Status::Authorized, _ => Status::Unknown })
    }

    pub async fn request(app: &tauri::AppHandle) -> Result<Status, String> {
        let current = status()?;
        if current != Status::NotDetermined { return Ok(current); }
        let guard = crate::permission_request::begin()?;
        let (send, receive) = tokio::sync::oneshot::channel();
        let pending_app = app.clone();
        app.run_on_main_thread(move || {
            // A queued request must not create a late dialog after its waiting
            // caller timed out. Recheck state because Settings may have changed.
            if send.is_closed() { return; }
            if let Err(error) = pending_app.state::<crate::AppState>().updater.ensure_idle() { let _ = send.send(Err(error)); return; }
            let device = match device() { Ok(device) => device, Err(error) => { let _ = send.send(Err(error)); return; } };
            match status() {
                Ok(Status::NotDetermined) => {},
                result => { let _ = send.send(result); return; },
            }
            // Retain the single-flight guard in the escaping native callback.
            // Timing out the caller cannot allow a second outstanding dialog.
            let completion = Mutex::new(Some((send, guard)));
            let callback = RcBlock::new(move |_granted: Bool| {
                if let Ok(mut completion) = completion.lock() {
                    if let Some((send, _guard)) = completion.take() {
                        // The callback boolean is not a durable permission
                        // snapshot. Read AVFoundation again for the response.
                        let _ = send.send(status());
                    }
                }
            });
            // AVFoundation copies the escaping block; the completion may run on
            // its own queue. It never starts a capture session or bypasses a denial.
            unsafe { let _: () = msg_send![device, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*callback]; }
        }).map_err(|_| "Die macOS-Mikrofonfreigabe konnte nicht angefragt werden.".to_owned())?;
        crate::permission_request::receive(receive, Duration::from_secs(120),
            "Die Mikrofonfreigabe wurde nicht rechtzeitig beantwortet. Bitte den macOS-Dialog oder die Systemeinstellungen prüfen und das Zuhören danach erneut starten.",
            "macOS hat die Mikrofonfreigabe nicht beantwortet.").await
    }
}

fn current() -> Result<Permission, String> {
    #[cfg(target_os = "macos")]
    { Ok(Permission { platform: "macos", status: macos::status()? }) }
    #[cfg(not(target_os = "macos"))]
    { Ok(Permission { platform: std::env::consts::OS, status: Status::Webview }) }
}

#[tauri::command]
pub fn get_microphone_permission(window: tauri::WebviewWindow) -> Result<Permission, String> {
    studio(window.label())?;
    current()
}

#[tauri::command]
pub async fn request_microphone_permission(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<Permission, String> {
    studio(window.label())?;
    app.state::<crate::AppState>().updater.ensure_idle()?;
    #[cfg(target_os = "macos")]
    {
        macos::request(&app).await?;
        app.state::<crate::AppState>().updater.ensure_idle()?;
        current()
    }
    #[cfg(not(target_os = "macos"))]
    { current() }
}

#[tauri::command]
pub fn open_microphone_settings(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    studio(window.label())?;
    #[cfg(target_os = "macos")]
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
    #[cfg(target_os = "windows")]
    let url = "ms-settings:privacy-microphone";
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    { app.opener().open_url(url, None::<&str>).map_err(|_| "Die Mikrofoneinstellungen konnten nicht geöffnet werden. Öffne sie bitte in den Systemeinstellungen.".into()) }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { let _ = app; Err("Öffne die Mikrofonfreigabe und Audioeingabe in den Einstellungen deiner Desktop-Umgebung.".into()) }
}

use tauri::Manager;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_management_is_studio_only() {
        assert!(studio("main").is_ok());
        for label in ["avatar", "avatar-nova", "overlay", ""] { assert!(studio(label).is_err()); }
    }
    #[test]
    fn statuses_never_imply_browser_capture_success() {
        assert_eq!(serde_json::to_value(Status::NotDetermined).unwrap(), "notDetermined");
        assert_eq!(serde_json::to_value(Status::Webview).unwrap(), "webview");
        assert_ne!(Status::Webview, Status::Authorized);
    }
}
