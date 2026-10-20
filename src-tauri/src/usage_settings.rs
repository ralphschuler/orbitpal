use crate::usage_budget::{Limits, Snapshot};
pub(crate) fn main_only(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err("Verbrauchsbudgets werden ausschließlich im Studio verwaltet.".into())
    }
}
#[tauri::command(async)]
pub fn get_usage_budget(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Snapshot, String> {
    main_only(window.label())?;
    state.store.usage_snapshot()
}
#[tauri::command]
pub fn set_usage_limits(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
    limits: Limits,
) -> Result<Snapshot, String> {
    main_only(window.label())?;
    state.store.set_usage_limits(&limits)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_settings_are_studio_only() {
        assert!(main_only("main").is_ok());
        for label in ["avatar-nova", "mcp", "main-other", ""] {
            assert!(main_only(label).is_err());
        }
    }
}
