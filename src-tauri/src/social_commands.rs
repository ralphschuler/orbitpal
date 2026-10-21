//! Main-only explicit social UI. The real native host owns scenes and receipts.
use crate::{
    character_development::Character, social::SceneKind, social_dialogue::Policy,
    storage::social_store::SocialSnapshot, AppState,
};
use tauri::State;
fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("social_main_only".into())
    }
}
fn mutate(window: &tauri::WebviewWindow, state: &AppState) -> Result<(), String> {
    main_only(window)?;
    state.updater.ensure_idle()
}
#[tauri::command]
pub(crate) fn get_social_snapshot(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<SocialSnapshot, String> {
    main_only(&window)?;
    crate::life_runtime::social_host::snapshot(&app, &bot_id)
}
#[tauri::command]
pub(crate) fn invite_social_scene(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    other_id: String,
    kind: SceneKind,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::request(&app, &bot_id, &other_id, kind)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn respond_social_invitation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    invitation_id: String,
    accept: bool,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::respond(&app, &bot_id, &invitation_id, accept)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn cancel_social_scene(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    scene_id: String,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_scene(&app, &bot_id, &scene_id)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn delete_social_memory(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    event_id: String,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state
        .store
        .social_memory_change(&bot_id, Some(&event_id), None)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn reset_social_memory(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state.store.social_memory_change(&bot_id, None, None)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn set_social_memory_shared(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    event_id: String,
    shared: bool,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state
        .store
        .social_memory_change(&bot_id, Some(&event_id), Some(shared))?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn set_social_development(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    expected_revision: u64,
    development: Character,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state
        .store
        .social_set_development(&bot_id, expected_revision, development)
}
#[tauri::command]
pub(crate) fn reset_social_development(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state.store.social_reset_development(&bot_id)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn reset_social_object_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state.store.social_reset_object_preferences(&bot_id)?;
    state.store.social_snapshot(&bot_id)
}
#[tauri::command]
pub(crate) fn set_social_dialogue_policy(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    expected_revision: u64,
    policy: Policy,
) -> Result<SocialSnapshot, String> {
    mutate(&window, &state)?;
    crate::life_runtime::social_host::cancel_actor(&app, &bot_id)?;
    state
        .store
        .social_set_dialogue_policy(&bot_id, expected_revision, policy)?;
    state.store.social_snapshot(&bot_id)
}
