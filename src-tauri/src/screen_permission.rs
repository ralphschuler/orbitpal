//! OS-wide screen-recording permission, separate from each bot's image grant.
//! Neither status checks nor requesting permission capture any pixels.
use serde::Serialize;
use tauri::Manager;
#[cfg(target_os="macos")]
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all="camelCase")]
#[allow(dead_code)]
pub enum Status { Authorized, NotGranted, SystemManaged }
#[derive(Debug, Serialize)]
pub struct Permission { platform: &'static str, status: Status }
fn studio(label: &str) -> Result<(), String> {
    if label=="main" { Ok(()) } else { Err("Bildschirmfreigaben werden ausschließlich im Studio verwaltet.".into()) }
}
#[cfg(any(target_os="macos",test))]
fn macos_status(granted: bool) -> Status {
    // Preflight cannot distinguish first use, denial and a pending restart.
    if granted { Status::Authorized } else { Status::NotGranted }
}
#[cfg(target_os="macos")]
mod macos {
    #[link(name="CoreGraphics",kind="framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    pub fn granted() -> bool { unsafe { CGPreflightScreenCaptureAccess() } }
    pub fn request() -> bool { unsafe { CGRequestScreenCaptureAccess() } }
}
fn current() -> Permission {
    #[cfg(target_os="macos")]
    { Permission {platform:"macos",status:macos_status(macos::granted())} }
    #[cfg(not(target_os="macos"))]
    { Permission {platform:std::env::consts::OS,status:Status::SystemManaged} }
}
#[tauri::command]
pub fn get_screen_capture_permission(window:tauri::WebviewWindow) -> Result<Permission,String> {
    studio(window.label())?;Ok(current())
}
#[tauri::command]
pub async fn request_screen_capture_permission(window:tauri::WebviewWindow,app:tauri::AppHandle) -> Result<Permission,String> {
    studio(window.label())?;
    app.state::<crate::AppState>().updater.ensure_idle()?;
    #[cfg(target_os="macos")]
    {
        let (send,receive)=tokio::sync::oneshot::channel();
        app.run_on_main_thread(move||{
            if !macos::granted(){let _=macos::request();}
            // Re-read preflight; a request return value does not prove that
            // this process can capture yet after a settings/restart change.
            let _=send.send(current());
        }).map_err(|_|"Die macOS-Bildschirmfreigabe konnte nicht angefragt werden.".to_owned())?;
        receive.await.map_err(|_|"Die macOS-Bildschirmfreigabe wurde unterbrochen.".to_owned())
    }
    #[cfg(not(target_os="macos"))]
    { Ok(current()) }
}
#[tauri::command]
pub fn open_screen_capture_settings(window:tauri::WebviewWindow,app:tauri::AppHandle) -> Result<(),String> {
    studio(window.label())?;
    #[cfg(target_os="macos")]
    { app.opener().open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",None::<&str>).map_err(|_|"Die Bildschirmfreigabe konnte nicht geöffnet werden. Öffne Datenschutz & Sicherheit in den Systemeinstellungen.".into()) }
    #[cfg(not(target_os="macos"))]
    { let _=app;Err("Die Bildschirmfreigabe wird auf diesem System von der Desktop-Umgebung verwaltet.".into()) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_explicit_studio_boundary_can_manage_permission() {
        assert!(studio("main").is_ok());for label in ["avatar","avatar-nova","obs","", "main-other"] {assert!(studio(label).is_err());}
    }
    #[test]
    fn preflight_false_never_claims_a_specific_denial_or_success() {
        assert_eq!(macos_status(false),Status::NotGranted);assert_eq!(macos_status(true),Status::Authorized);
        assert_eq!(serde_json::to_value(Status::NotGranted).unwrap(),"notGranted");
        assert_ne!(Status::SystemManaged,Status::Authorized);
    }
}
