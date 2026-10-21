//! Operator UI entry points; none accepts facts, native receipts or platform impulses.
use crate::{
    content_packs::{Installed, Manifest, Preview},
    experience::{self, ExperienceRuntime, InteractionRequest, Preferences, Snapshot},
    AppState,
};
use tauri::{Emitter, State};
fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("experience_main_only".into());
    }
    Ok(())
}
fn snapshot(
    state: &AppState,
    runtime: &ExperienceRuntime,
    bot_id: &str,
) -> Result<Snapshot, String> {
    Ok(Snapshot {
        version: 1,
        bot_id: bot_id.into(),
        settings: state.store.experience_settings(bot_id)?,
        paused: runtime.paused(bot_id)?,
        facts: state.store.experience_facts(bot_id)?,
        desktop_capabilities: Default::default(),
        impulses: runtime.impulse_status(bot_id)?,
    })
}
#[tauri::command]
pub fn get_experience_snapshot(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    runtime: State<'_, ExperienceRuntime>,
    bot_id: String,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    snapshot(&state, &runtime, &bot_id)
}
#[tauri::command]
pub async fn set_experience_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    runtime: State<'_, ExperienceRuntime>,
    bot_id: String,
    expected_revision: u64,
    preferences: Preferences,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    let next = state
        .store
        .set_experience_preferences(&bot_id, expected_revision, preferences)?;
    crate::life_runtime::experience_preferences_changed(&app, &bot_id, &next.preferences).await?;
    let result = snapshot(&state, &runtime, &bot_id)?;
    let _ = app.emit_to("main", "experience-state", &result);
    Ok(result)
}
#[tauri::command]
pub async fn set_experience_pause(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    runtime: State<'_, ExperienceRuntime>,
    bot_id: String,
    paused: bool,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    state.store.bot_profile(&bot_id)?;
    if paused {
        runtime.set_paused(&bot_id, true)?;
    }
    crate::life_runtime::experience_pause_changed(&app, &bot_id, paused).await?;
    if !paused {
        runtime.set_paused(&bot_id, false)?;
    }
    let result = snapshot(&state, &runtime, &bot_id)?;
    let _ = app.emit_to("main", "experience-state", &result);
    Ok(result)
}
#[tauri::command]
pub async fn request_character_interaction(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    request: InteractionRequest,
) -> Result<serde_json::Value, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    request.validate()?;
    state.store.bot_profile(&bot_id)?;
    crate::life_runtime::request_character_interaction(&app, &bot_id, &request).await
}
#[tauri::command]
pub fn delete_experience_fact(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    bot_id: String,
    event_id: Option<String>,
) -> Result<Vec<experience::ActivityFact>, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    state
        .store
        .edit_experience_fact(&bot_id, event_id.as_deref(), None)
}
#[tauri::command]
pub fn set_experience_fact_shared(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    bot_id: String,
    event_id: String,
    shared: bool,
) -> Result<Vec<experience::ActivityFact>, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    state
        .store
        .edit_experience_fact(&bot_id, Some(&event_id), Some(shared))
}
#[tauri::command]
pub fn list_content_packs(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Installed>, String> {
    main_only(&window)?;
    state.store.content_packs()
}
#[tauri::command]
pub fn preview_content_pack(
    window: tauri::WebviewWindow,
    manifest_json: String,
) -> Result<Preview, String> {
    main_only(&window)?;
    Ok(Manifest::parse(&manifest_json)?.preview())
}
#[tauri::command]
pub fn install_content_pack(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    manifest_json: String,
) -> Result<Vec<Installed>, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    state
        .store
        .install_content_pack(Manifest::parse(&manifest_json)?)
}
#[tauri::command]
pub async fn set_content_pack_active(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    package_id: String,
    expected_revision: u64,
    active: bool,
) -> Result<Vec<Installed>, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    if !experience::valid_id(&package_id) {
        return Err("pack_invalid_id".into());
    }
    let current = state.store.content_packs()?;
    if !current
        .iter()
        .any(|p| p.manifest.id == package_id && p.revision == expected_revision)
    {
        return Err("experience_stale_revision".into());
    }
    if !active {
        crate::life_runtime::content_pack_deactivating(&app, &package_id).await?;
    }
    let packs = state
        .store
        .set_content_pack_active(&package_id, expected_revision, active)?;
    let _ = app.emit_to("main", "content-packs-changed", &packs);
    Ok(packs)
}
#[tauri::command]
pub fn delete_content_pack(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    package_id: String,
) -> Result<Vec<Installed>, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    state.store.delete_content_pack(&package_id)
}
