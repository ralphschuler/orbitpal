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
    use std::sync::Mutex;

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

    pub async fn request() -> Result<Status, String> {
        let current = status()?;
        if current != Status::NotDetermined { return Ok(current); }
        let (send, receive) = tokio::sync::oneshot::channel();
        {
            let send = Mutex::new(Some(send));
            let callback = RcBlock::new(move |_granted: Bool| {
                if let Ok(mut send) = send.lock() { if let Some(send) = send.take() { let _ = send.send(()); } }
            });
            // AVFoundation copies the escaping block; the completion may run on
            // its own queue. It never starts a capture session or bypasses a denial.
            unsafe { let _: () = msg_send![device()?, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*callback]; }
        }
        tokio::time::timeout(std::time::Duration::from_secs(120), receive).await
            .map_err(|_| "Die Mikrofonfreigabe wurde nicht beantwortet. Bitte erneut auf Start klicken.".to_owned())?
            .map_err(|_| "macOS hat die Mikrofonfreigabe nicht beantwortet".to_owned())?;
        status()
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
    { Ok(Permission { platform: "macos", status: macos::request().await? }) }
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
