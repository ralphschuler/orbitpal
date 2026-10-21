//! Operator UI entry points; none accepts facts, native receipts or platform impulses.
use crate::{
    content_packs::{ContentRef, Installed, Manifest, Preview},
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
pub(crate) fn native_snapshot(app: &tauri::AppHandle, bot: &str) -> Result<Snapshot,String> {
    use tauri::Manager;
    snapshot(&app.state::<AppState>(),&app.state::<ExperienceRuntime>(),bot)
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
pub fn create_content_pack_character(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    reference: ContentRef,
) -> Result<serde_json::Value, String> {
    main_only(&window)?;
    state.updater.ensure_idle()?;
    let _idle = state.generation.try_lock().map_err(|_| "experience_busy")?;
    let profile = state.store.create_content_pack_character(&reference)?;
    Ok(
        serde_json::json!({"version":1,"botId":profile["id"],"name":profile["name"],"contentRef":profile["contentRef"]}),
    )
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
    let packs=crate::life_runtime::set_content_pack_active_native(&app,&package_id,expected_revision,active)?;
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
