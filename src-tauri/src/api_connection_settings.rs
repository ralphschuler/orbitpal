//! Main-window-only global API management. No model/MCP allowlist grants this.
use crate::{
    api_connections::{AuthStyle, Binding, Purpose, Service},
    storage::api_connection_store::NewConnection,
    AppState,
};
use serde_json::Value;
use std::sync::atomic::Ordering;
use tauri::{Manager, State, WebviewWindow};
const MAIN_ONLY: &str = "Globale API-Zugänge werden ausschließlich im Studio verwaltet.";
fn main_only(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err(MAIN_ONLY.into())
    }
}
fn begin_write(window: &WebviewWindow, app: &tauri::AppHandle) -> Result<(), String> {
    main_only(window.label())?;
    let state = app.state::<AppState>();
    state.updater.ensure_idle()?;
    state.life_runtime.operator_activity();
    if let Ok(current) = state.cancel.lock() {
        if let Some(cancel) = current.as_ref() {
            cancel.store(true, Ordering::SeqCst);
        }
    }
    state.overlay.invalidate_speech(None);
    Ok(())
}
#[tauri::command(async)]
pub fn get_api_connections(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    main_only(window.label())?;
    Ok(state.store.api_snapshot())
}
#[tauri::command(async)]
pub fn get_api_binding(
    window: WebviewWindow,
    state: State<'_, AppState>,
    purpose: Purpose,
    config: Value,
) -> Result<Value, String> {
    main_only(window.label())?;
    Ok(state.store.api_binding(&config, purpose))
}
#[tauri::command]
pub fn create_api_connection(
    window: WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: u64,
    connection: NewConnection,
) -> Result<Value, String> {
    begin_write(&window, &app)?;
    app.state::<AppState>()
        .store
        .create_api_connection(expected_revision, connection)
}
#[tauri::command]
pub fn rename_api_connection(
    window: WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: u64,
    id: String,
    label: String,
) -> Result<Value, String> {
    begin_write(&window, &app)?;
    app.state::<AppState>()
        .store
        .rename_api_connection(expected_revision, &id, &label)
}
#[tauri::command]
pub fn set_default_api_connection(
    window: WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: u64,
    service: Service,
    destination: String,
    auth_style: AuthStyle,
    connection_id: Option<String>,
) -> Result<Value, String> {
    begin_write(&window, &app)?;
    let binding = Binding::normalized(service, &destination, auth_style)?;
    app.state::<AppState>().store.default_api_connection(
        expected_revision,
        binding,
        connection_id.as_deref(),
    )
}
#[tauri::command]
pub fn remove_api_connection(
    window: WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: u64,
    id: String,
) -> Result<Value, String> {
    begin_write(&window, &app)?;
    app.state::<AppState>()
        .store
        .remove_api_connection(expected_revision, &id)
}
#[tauri::command]
pub fn set_api_connection_secret(
    window: WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: u64,
    id: String,
    value: String,
) -> Result<Value, String> {
    let value = zeroize::Zeroizing::new(value);
    begin_write(&window, &app)?;
    app.state::<AppState>()
        .store
        .set_api_connection_secret(expected_revision, &id, &value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_access_management_is_main_only() {
        assert!(main_only("main").is_ok());
        for label in ["", "avatar-nova", "main-copy", "mcp"] {
            assert!(main_only(label).is_err());
        }
    }
}
