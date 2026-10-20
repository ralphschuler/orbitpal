//! Main-window command boundary and existing native-loop integration.
use super::{
    action_host,
    action_state::{RestIntent, UiProof},
    LifeRuntime,
};
use crate::{life_domain::CancelReason, usage_budget::Origin};
use serde_json::Value;
use std::sync::{atomic::AtomicBool, Arc};
use tauri::{Emitter, Manager};
const MAIN_ONLY: &str = "Lokale Lebensaktionen werden ausschließlich im Studio gesteuert.";
fn main_only(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err(MAIN_ONLY.into())
    }
}
/// A config-enabled OBS link is not a connected output. #72 will add real
/// registration; this nonvisual path currently supports only actual native UI.
fn rest_output(app: &tauri::AppHandle, profile: &Value) -> bool {
    match profile["output"]["mode"].as_str().unwrap_or("studio") {
        "studio" => {
            app.get_webview_window("main")
                .is_some_and(|w| w.is_visible().ok() == Some(true))
                && app
                    .state::<crate::AppState>()
                    .store
                    .profile()
                    .ok()
                    .is_some_and(|p| p["id"] == profile["id"])
        }
        "overlay" => profile["id"]
            .as_str()
            .and_then(|id| app.get_webview_window(&format!("avatar-{id}")))
            .is_some_and(|w| w.is_visible().ok() == Some(true)),
        _ => false,
    }
}
pub(crate) fn prepare_operator<R: tauri::Runtime>(
    state: &crate::AppState,
    invoke: &tauri::ipc::Invoke<R>,
) {
    state.life_runtime.outputs.bot(None);
    let mut core = state
        .life_runtime
        .core
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let proof = match invoke.message.payload() {
        tauri::ipc::InvokeBody::Json(value) => {
            let bot = value["botId"]
                .as_str()
                .filter(|id| crate::storage::valid_id(id));
            let connection = value["ui"]["connectionId"]
                .as_str()
                .filter(|id| id.len() <= 96);
            let sequence = value["ui"]["sequence"]
                .as_u64()
                .filter(|seq| *seq > 0 && *seq <= crate::life_domain::MAX_COUNTER);
            match (bot, connection, sequence) {
                (Some(bot), Some(connection), Some(sequence)) => Some((
                    bot.to_owned(),
                    UiProof {
                        connection_id: connection.into(),
                        sequence,
                    },
                )),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some((bot, proof)) = proof {
        if let Ok(profile) = state.store.profile() {
            if profile["id"] == bot {
                core.prepare_rest_intent(&bot, profile, &proof, state.life_runtime.now());
                return;
            }
        }
    }
    core.operator_activity(state.life_runtime.now());
}
struct IntentGuard<'a> {
    runtime: &'a LifeRuntime,
    intent: RestIntent,
}
impl Drop for IntentGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut core) = self.runtime.core.lock() {
            core.abandon_rest_intent(&self.intent.bot, self.intent.serial);
        }
    }
}
#[tauri::command]
pub fn get_life_actions(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
    bot_id: String,
) -> Result<Value, String> {
    main_only(window.label())?;
    let core = state
        .life_runtime
        .core
        .lock()
        .map_err(|_| action_host::UNAVAILABLE)?;
    action_host::status(
        &core,
        &state.store,
        &state.world,
        &bot_id,
        state.life_runtime.now(),
    )
}
#[tauri::command]
pub async fn start_life_rest(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    bot_id: String,
    ui: UiProof,
) -> Result<Value, String> {
    main_only(window.label())?;
    let state = app.state::<crate::AppState>();
    state.updater.ensure_idle()?;
    let intent = state
        .life_runtime
        .core
        .lock()
        .map_err(|_| action_host::UNAVAILABLE)?
        .rest_intent(&bot_id, &ui)
        .ok_or(action_host::UNAVAILABLE)?;
    let _guard = IntentGuard {
        runtime: &state.life_runtime,
        intent: intent.clone(),
    };
    let monitor = state
        .life_runtime
        .monitor
        .get()
        .ok_or(action_host::UNAVAILABLE)?;
    let system = monitor.refresh().await;
    let topology = state.world.sample_topology(&app).await;
    let profile = state.store.bot_profile(&bot_id)?;
    let output_ticket = state
        .life_runtime
        .outputs
        .ticket()
        .ok_or(action_host::UNAVAILABLE)?;
    let output = rest_output(&app, &profile);
    let visible = super::visible(&app);
    state.updater.ensure_idle()?;
    if !visible || !output || state.overlay.has_any_pending_speech() {
        return Err(action_host::UNAVAILABLE.into());
    }
    let _generation = state
        .generation
        .try_lock()
        .map_err(|_| action_host::UNAVAILABLE)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let label = if profile["output"]["mode"] == "overlay" {
        format!("avatar-{bot_id}")
    } else {
        "main".into()
    };
    if !state
        .life_runtime
        .outputs
        .watch(output_ticket, &bot_id, &label, &cancel)
        || monitor.gate.watch(&cancel).is_none()
    {
        return Err(action_host::UNAVAILABLE.into());
    }
    let mut core = state
        .life_runtime
        .core
        .lock()
        .map_err(|_| action_host::UNAVAILABLE)?;
    core.observe_system(system, state.life_runtime.now());
    core.reconcile_visibility(visible, state.life_runtime.now());
    if state.store.profile()?["id"] != bot_id || profile != intent.profile {
        return Err(action_host::UNAVAILABLE.into());
    }
    state.world.synchronize(
        &state.store,
        &crate::world_hub::current_actors(&core),
        Some(topology),
        state.life_runtime.now(),
    )?;
    let context = state.world.action_context(
        &state.store,
        &crate::world_hub::current_actors(&core),
        &bot_id,
        state.life_runtime.now(),
    )?;
    let fence = core
        .consume_rest_intent(&intent, &profile, state.life_runtime.now())
        .ok_or(action_host::UNAVAILABLE)?;
    let result = action_host::admit_rest(
        &mut core,
        &state.store,
        &state.world,
        &bot_id,
        &profile,
        fence,
        context,
        cancel,
        || state.life_runtime.now(),
        super::wall_ms,
    );
    let _ = action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    );
    let status = action_host::status(
        &core,
        &state.store,
        &state.world,
        &bot_id,
        state.life_runtime.now(),
    );
    drop(core);
    publish(&app);
    result?;
    status
}
pub(super) fn publish(app: &tauri::AppHandle) {
    let state = app.state::<crate::AppState>();
    let snapshots = state
        .life_runtime
        .core
        .lock()
        .ok()
        .map(|core| {
            let mut bots = core
                .sessions
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            if let Some(actions) = &core.actions {
                bots.extend(actions.jobs.keys().cloned());
                bots.extend(
                    actions
                        .retired
                        .iter()
                        .map(|s| s.action_id.actor_id().as_str().to_owned()),
                );
            }
            bots.into_iter()
                .filter_map(|bot| {
                    action_host::status(
                        &core,
                        &state.store,
                        &state.world,
                        &bot,
                        state.life_runtime.now(),
                    )
                    .ok()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for snapshot in snapshots {
        let _ = app.emit_to("main", "cohost://life-actions", snapshot);
    }
}
impl LifeRuntime {
    pub(super) fn poll_actions(&self, app: &tauri::AppHandle) {
        // Wry getters may await Main. Collect all route evidence before Core locks.
        let state = app.state::<crate::AppState>();
        let bots = self
            .core
            .lock()
            .ok()
            .and_then(|core| {
                core.actions.as_ref().map(|actions| {
                    actions
                        .jobs
                        .iter()
                        .map(|(id, job)| (id.clone(), job.key.clone()))
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        let unavailable = bots
            .into_iter()
            .filter(|(bot, _)| {
                state
                    .store
                    .bot_profile(bot)
                    .ok()
                    .is_none_or(|profile| !rest_output(app, &profile))
            })
            .collect::<Vec<_>>();
        if let Ok(mut core) = self.core.lock() {
            for (bot, key) in unavailable {
                if core
                    .actions
                    .as_ref()
                    .and_then(|a| a.jobs.get(&bot))
                    .is_some_and(|job| job.key == key)
                {
                    core.interrupt_actions(Some(&bot), CancelReason::Unavailable, self.now());
                }
            }
            let _ = action_host::poll(
                &mut core,
                &state.store,
                &state.world,
                || self.now(),
                super::wall_ms,
            );
        }
        publish(app);
    }
    pub(crate) fn before_dialog(
        &self,
        app: &tauri::AppHandle,
        origin: Origin,
    ) -> Result<(), String> {
        let state = app.state::<crate::AppState>();
        let mut core = self.core.lock().map_err(|_| action_host::UNAVAILABLE)?;
        if origin.autonomous() && core.local_actions_busy() {
            return Err(
                "Bitte die lokale Lebensaktion vor der automatischen Antwort beenden.".into(),
            );
        }
        core.activity(self.now());
        let result = action_host::poll(
            &mut core,
            &state.store,
            &state.world,
            || self.now(),
            super::wall_ms,
        );
        drop(core);
        publish(app);
        result
    }
}
