//! Native World host: monitor observation and IPC remain outside state locks.
#[path = "world_hub/state.rs"]
mod state;
use crate::{
    storage::world_store::OutputBinding,
    world_state::{coordinates::OutputGeometry, coordinates::Point, WorldConfig},
};
use serde_json::json;
use state::DisplaySample;
pub(crate) use state::{current_actors, output_window, ActionContext, NativeActor, TopologySample};
pub use state::{PublicWorld, WorldHub, WorldSettings};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
const MAIN_ONLY: &str = "Die gemeinsame Bühne wird ausschließlich im Studio bearbeitet.";
fn actor(id: &str) -> Result<crate::life_domain::ActorId, String> {
    crate::life_domain::ActorId::new(id)
        .map_err(|_| "Die Position liegt außerhalb des sicheren Bühnenbereichs.".into())
}
impl WorldHub {
    pub(crate) async fn sample_topology(&self, app: &tauri::AppHandle) -> TopologySample {
        let ticket = self
            .sample_ticket
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let handle = app.clone();
        let queued = app.run_on_main_thread(move || {
            let mut displays = Vec::new();
            let mut fingerprint = Vec::new();
            if let Some(window) = handle.get_webview_window("main") {
                if let Ok(monitors) = window.available_monitors() {
                    for monitor in monitors.into_iter().take(64) {
                        let p = monitor.position();
                        let size = monitor.size();
                        let scale = monitor.scale_factor();
                        let id = format!(
                            "{}:{}:{}",
                            p.x,
                            p.y,
                            monitor.name().map_or("Monitor", String::as_str)
                        );
                        if let Ok(geometry) =
                            OutputGeometry::new(&id, p.x, p.y, size.width, size.height, scale)
                        {
                            fingerprint.push((
                                id,
                                p.x,
                                p.y,
                                size.width,
                                size.height,
                                scale.to_bits(),
                            ));
                            displays.push(DisplaySample { geometry });
                        }
                    }
                }
            }
            fingerprint.sort();
            let _ = tx.send(TopologySample {
                ticket,
                observed: Instant::now(),
                displays,
                fingerprint,
            });
        });
        if queued.is_ok() {
            if let Ok(Ok(sample)) = tokio::time::timeout(Duration::from_secs(2), rx).await {
                return sample;
            }
        }
        TopologySample {
            ticket,
            observed: Instant::now(),
            displays: Vec::new(),
            fingerprint: Vec::new(),
        }
    }
}
pub fn publish(app: &tauri::AppHandle) {
    let state = app.state::<crate::AppState>();
    if let Ok(snapshot) = state.world.snapshot() {
        let _ = app.emit_to("main", "cohost://world-state", snapshot);
    }
    for id in state.world.output_targets() {
        if let Ok(profile) = state.store.bot_profile(&id) {
            let event = state.world.output_event(&id, &profile).unwrap_or_else(
                || json!({"kind":"world-state","botId":id,"world":null,"arena":null}),
            );
            state.overlay.publish(app, event);
        }
    }
}

fn main_only(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err(MAIN_ONLY.into())
    }
}
#[tauri::command]
pub fn get_world_state(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<PublicWorld, String> {
    main_only(window.label())?;
    state
        .life_runtime
        .with_world(window.app_handle(), |world, store, sessions, now| {
            world.synchronize(store, &sessions, None, now)
        })?;
    state.world.snapshot()
}
#[tauri::command]
pub fn get_world_settings(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<WorldSettings, String> {
    main_only(window.label())?;
    Ok(state.world.settings(&state.store))
}
#[tauri::command]
pub async fn save_world_config(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: Option<u64>,
    config: WorldConfig,
    output_binding: OutputBinding,
) -> Result<WorldSettings, String> {
    main_only(window.label())?;
    let state = app.state::<crate::AppState>();
    state.updater.ensure_idle()?;
    state.life_runtime.operator_activity();
    let sample = state.world.sample_topology(&app).await;
    state.updater.ensure_idle()?;
    let result = state
        .life_runtime
        .with_world(&app, |world, store, sessions, now| {
            world.save(
                store,
                expected_revision,
                &config,
                &output_binding,
                &sessions,
                sample,
                now,
            )
        })?;
    publish(&app);
    Ok(result)
}
#[tauri::command]
pub fn place_world_actor(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    expected_revision: u64,
    actor_id: String,
    position: Point,
) -> Result<PublicWorld, String> {
    main_only(window.label())?;
    let state = app.state::<crate::AppState>();
    state.updater.ensure_idle()?;
    state.life_runtime.operator_activity();
    let id = actor(&actor_id)?;
    let result = state
        .life_runtime
        .with_world(&app, |world, store, sessions, now| {
            world.place_current(store, &sessions, now, expected_revision, &id, position)
        })?;
    publish(&app);
    Ok(result)
}

#[cfg(test)]
pub(crate) fn action_fixture_topology(ticket: u64) -> state::TopologySample {
    state::TopologySample {
        ticket,
        observed: std::time::Instant::now(),
        displays: vec![],
        fingerprint: vec![],
    }
}
