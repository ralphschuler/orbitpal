//! Operator-only configuration. No control-MCP or agent tool exposes this API.
use serde_json::{json, Value};

fn main_only(label: &str) -> Result<(), String> {
    if label != "main" { return Err("Schlüsselsitzungen werden ausschließlich im Studio verwaltet.".into()); }
    Ok(())
}

fn public(snapshot: crate::credential_session::Snapshot) -> Value {
    json!({"supported":snapshot.supported,"enabled":snapshot.enabled,"locked":snapshot.locked,"rememberedKeys":snapshot.remembered_keys})
}

#[tauri::command]
pub fn get_credential_session(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>) -> Result<Value, String> {
    main_only(window.label())?; state.store.credential_session().map(public)
}

#[tauri::command]
pub fn set_credential_session(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>, enabled: bool) -> Result<Value, String> {
    main_only(window.label())?; state.store.configure_credential_session(enabled).map(public)
}

#[tauri::command]
pub fn lock_credential_session(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>) -> Result<Value, String> {
    main_only(window.label())?; state.store.lock_credential_session().map(public)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_is_operator_studio_only_and_snapshots_contain_no_ids_or_values() {
        assert!(main_only("main").is_ok());
        for label in ["", "avatar", "avatar-nova", "mcp", "main-other"] { assert!(main_only(label).is_err()); }
        let value = public(crate::credential_session::Session::new(true, true).snapshot().unwrap());
        assert_eq!(value, json!({"supported":true,"enabled":true,"locked":false,"rememberedKeys":0}));
    }
}
