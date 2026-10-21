//! Main operator/Studio transport. Native movement delegates to the retained
//! NativeActionPermit factory and exact constructor-bound command dispatcher.
//! Registration, mirror metadata and command evaluation never start Life.
use super::{
    action_host, actions,
    core::{Core, Mode},
    LifeRuntime,
};
use crate::{life_domain::ActorId, world_output::*, world_state::SessionStamp, AppState};
use serde_json::{json, Value};
use std::sync::{atomic::AtomicBool, Arc};
use tauri::{Emitter, Manager};
const ERROR: &str =
    "Die World-Darstellung ist momentan nicht verfügbar. Prüfe Life, Bühne und Studio-Ausgabe.";
fn main_only(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err(ERROR.into())
    }
}
fn safe(_: impl std::fmt::Debug) -> String {
    ERROR.into()
}
struct StudioSample {
    layout: Value,
    presentation: Value,
    visible: bool,
}
/// Wry getters can wait for Main; never call while holding Core/World/Store.
fn sample(app: &tauri::AppHandle, profile: &Value) -> Result<StudioSample, String> {
    let window = app.get_webview_window("main").ok_or(ERROR)?;
    let size = window.inner_size().map_err(safe)?;
    let scale = window.scale_factor().map_err(safe)?;
    let visible = window.is_visible().map_err(safe)?;
    if size.width == 0 || size.height == 0 || !scale.is_finite() || scale <= 0.0 {
        return Err(ERROR.into());
    }
    Ok(StudioSample {
        layout: json!({"width":size.width,"height":size.height,"scale":scale}),
        presentation: app.state::<AppState>().presentation.snapshot(profile),
        visible,
    })
}
pub(super) fn native_walk(app: &tauri::AppHandle, profile: &Value) -> bool {
    let asset = profile["avatar"]["assetPath"].as_str();
    if asset.is_none() || asset == Some("nova") {
        return true;
    }
    crate::emote_capabilities::inspect(&app.state::<AppState>().store.root, profile).is_ok_and(
        |caps| {
            caps.actions.iter().any(|a| {
                matches!(a.kind, "clip" | "motion")
                    && a.action.to_ascii_lowercase().contains("walk")
                    && a.duration.is_some_and(|d| d.is_finite() && d > 0.0)
            })
        },
    )
}
fn source(
    core: &Core,
    app: &tauri::AppHandle,
    bot: &str,
    profile: &Value,
    sample: &StudioSample,
    walk: bool,
    now: u64,
) -> Result<NativeSource, String> {
    if !sample.visible || profile["output"]["mode"].as_str().unwrap_or("studio") != "studio" {
        return Err(ERROR.into());
    }
    let ui = core.world_ui_connection(now).ok_or(ERROR)?.to_owned();
    let state = app.state::<AppState>();
    if state.life_runtime.session_outputs.has_retained_constructor(bot) { return Err(ERROR.into()); }
    if state.store.bot_profile(bot)? != *profile {
        return Err(ERROR.into());
    }
    let session = core
        .sessions
        .get(bot)
        .filter(|s| s.mode == Mode::Life && !s.storage_failed)
        .ok_or(ERROR)?;
    let life = session.state.as_ref().ok_or(ERROR)?;
    let stamp = SessionStamp::new(
        ActorId::new(bot).map_err(safe)?,
        &session.session_id,
        session.revision,
    )
    .map_err(safe)?;
    let (context, safe_region) =
        state
            .world
            .registration_context(&state.store, &stamp, life.world_id())?;
    let owner = state.store.profile()?["id"] == bot;
    if owner && core.local_fence(bot, now).is_none() {
        return Err(ERROR.into());
    }
    let profile_scale = profile["avatar"]["scale"].as_f64().unwrap_or(1.0);
    let effective_scale = sample.presentation["scale"]
        .as_f64()
        .unwrap_or(profile_scale);
    Ok(NativeSource {
        route: SourceRoute::Studio,
        context,
        profile_token: state
            .store
            .action_profile_token(bot, profile)
            .map_err(safe)?,
        ui_connection: ui,
        layout_token: json!([sample.layout, sample.presentation]),
        viewport_limit: CssSize {
            width: sample.layout["width"].as_f64().ok_or(ERROR)?
                / sample.layout["scale"].as_f64().ok_or(ERROR)?,
            height: sample.layout["height"].as_f64().ok_or(ERROR)?
                / sample.layout["scale"].as_f64().ok_or(ERROR)?,
        },
        safe_region,
        profile_scale,
        effective_scale,
        native_walk: walk,
        owner,
        native_framing: None,
    })
}
#[tauri::command]
pub async fn register_world_output(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: RegisterWorldActor,
) -> Result<RegisteredWorldActor, String> {
    main_only(window.label())?;
    if !request.valid() {
        return Err(ERROR.into());
    }
    let state = app.state::<AppState>();
    let ticket = state.life_runtime.outputs.ticket().ok_or(ERROR)?;
    let profile = state.store.bot_profile(&request.actor_id)?;
    let native = sample(&app, &profile)?;
    let walk = native_walk(&app, &profile);
    let topology = state.world.sample_topology(&app).await;
    let mut core = state.life_runtime.core.lock().map_err(safe)?;
    let now = state.life_runtime.now();
    let proof = request.studio_proof.as_ref().ok_or(ERROR)?;
    if !core.world_ui_current(proof, now) {
        return Err(ERROR.into());
    }
    state.world.synchronize(
        &state.store,
        &crate::world_hub::current_actors(&core),
        Some(topology),
        now,
    )?;
    let source = source(&core, &app, &request.actor_id, &profile, &native, walk, now)?;
    // Source read-through can block; metadata is not a lease on the old UI time.
    let now = state.life_runtime.now();
    if !core.world_ui_current(proof, now) {
        return Err(ERROR.into());
    }
    state
        .life_runtime
        .outputs
        .admit_surface(
            ticket,
            &request.actor_id,
            "main",
            (&proof.connection_id, proof.sequence),
            || state.life_runtime.session_outputs.admit_studio_world_registration(&request.actor_id,
                || core.world_outputs.register(&request, source, now)),
        )
        .map_err(safe)
}
#[tauri::command(async)]
pub fn report_world_measurement(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    report: RenderMeasurement,
) -> Result<MeasurementAck, String> {
    main_only(window.label())?;
    let state = app.state::<AppState>();
    let received = state.life_runtime.now();
    if !report.valid() {
        return Err(ERROR.into());
    }
    let bot = state
        .life_runtime
        .core
        .lock()
        .map_err(safe)?
        .world_outputs
        .measurement_actor(&report.challenge_id)
        .ok_or(ERROR)?;
    let profile = state.store.bot_profile(&bot)?;
    let native = sample(&app, &profile)?;
    let walk = native_walk(&app, &profile);
    let mut core = state.life_runtime.core.lock().map_err(safe)?;
    let now = state.life_runtime.now();
    if now < received || now - received > FRAME_MS {
        return Err(ERROR.into());
    }
    let source = source(&core, &app, &bot, &profile, &native, walk, now)?;
    let admitted = state.life_runtime.now();
    core.accept_world_measurement(&report, &source, received, admitted)
        .map_err(safe)
}
#[tauri::command(async)]
pub fn unregister_world_output(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: UnregisterWorldActor,
) -> Result<Value, String> {
    main_only(window.label())?;
    let state = app.state::<AppState>();
    // Exact native connection/binding revocation precedes a possibly blocked Core.
    let revoked = state.life_runtime.outputs.unregister_surface(
        window.label(),
        &request.render_session_id,
        request.binding_sequence,
    );
    let mut core = state.life_runtime.core.lock().map_err(safe)?;
    let now = state.life_runtime.now();
    let connection = core.world_ui_connection(now).unwrap_or_default().to_owned();
    let removed = core.world_outputs.unregister(&request, &connection);
    let _ = action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    );
    drop(core);
    publish(&app);
    actions::publish(&app);
    Ok(json!({"version":1,"removed":removed||revoked}))
}
#[tauri::command(async)]
pub fn get_world_output_status(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    bot_id: String,
) -> Result<Value, String> {
    main_only(window.label())?;
    if !crate::storage::valid_id(&bot_id) {
        return Err(ERROR.into());
    }
    let core = state.life_runtime.core.lock().map_err(safe)?;
    let now = state.life_runtime.now();
    Ok(
        json!({"version":1,"botId":bot_id,"output":core.world_outputs.status(&bot_id,now),"visualAvailable":core.world_output_current(&bot_id,now) && state.store.profile().ok().is_some_and(|p|p["id"]==bot_id)}),
    )
}
#[tauri::command(async)]
pub fn report_world_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    report: ActionReport,
) -> Result<ActionReportAck, String> {
    main_only(window.label())?;
    let state = app.state::<AppState>();
    let received = state.life_runtime.now();
    if !report.valid() {
        return Err(ERROR.into());
    }
    let (_, fence, _, _, _) = report.parts();
    let mut core = state.life_runtime.core.lock().map_err(safe)?;
    let now = state.life_runtime.now();
    let connection = core.world_ui_connection(now).ok_or(ERROR)?;
    if core
        .world_outputs
        .source(&fence.actor_id)
        .is_none_or(|s| s.ui_connection != connection)
        || core
            .world_outputs
            .status(&fence.actor_id, now)
            .is_none_or(|o| {
                o.render_session_id != fence.render_session_id
                    || o.binding_sequence != fence.binding_sequence
            })
    {
        return Err(ERROR.into());
    }
    let mut ack = action_host::visual_report(
        &mut core,
        &state.store,
        &state.world,
        &report,
        received,
        || state.life_runtime.now(),
        super::wall_ms,
    )?;
    let _ = action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    );
    ack.confirmed_phase = state
        .store
        .load_life(&fence.actor_id)
        .ok()
        .flatten()
        .and_then(|s| {
            s.state()
                .current_action()
                .filter(|a| a.intent().id() == &fence.action_id)
                .map(|a| a.phase())
        });
    drop(core);
    publish(&app);
    crate::world_hub::publish(&app);
    actions::publish(&app);
    Ok(ack)
}
struct IntentGuard<'a> {
    runtime: &'a LifeRuntime,
    bot: String,
    serial: u64,
}
impl Drop for IntentGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut core) = self.runtime.core.lock() {
            core.abandon_rest_intent(&self.bot, self.serial);
        }
    }
}
#[tauri::command]
pub async fn start_life_move(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    bot_id: String,
    target: Option<crate::world_state::coordinates::Point>,
    slot_id: Option<String>,
    ui: Option<super::action_state::UiProof>,
    native_intent_id: Option<String>,
) -> Result<Value, String> {
    main_only(window.label())?;
    if let Some(id)=native_intent_id{return super::native_scene_actions::start_move(app,bot_id,id,target,slot_id).await;}
    if slot_id.is_some(){return Err(ERROR.into());}
    let target=target.ok_or(ERROR)?;let ui=ui.ok_or(ERROR)?;
    let state = app.state::<AppState>();
    state.updater.ensure_idle()?;
    let intent = state
        .life_runtime
        .core
        .lock()
        .map_err(safe)?
        .rest_intent(&bot_id, &ui)
        .filter(|i| i.move_target == Some(target))
        .ok_or(ERROR)?;
    let _intent = IntentGuard {
        runtime: &state.life_runtime,
        bot: bot_id.clone(),
        serial: intent.serial,
    };
    let monitor = state.life_runtime.monitor.get().ok_or(ERROR)?;
    let system = monitor.refresh().await;
    let topology = state.world.sample_topology(&app).await;
    let profile = state.store.bot_profile(&bot_id)?;
    let native = sample(&app, &profile)?;
    let walk = native_walk(&app, &profile);
    state.updater.ensure_idle()?;
    if !native.visible
        || profile["output"]["mode"].as_str().unwrap_or("studio") != "studio"
        || state.overlay.has_any_pending_speech()
    {
        return Err(ERROR.into());
    }
    let _generation = state.generation.try_lock().map_err(|_| ERROR)?;
    let cancel = Arc::new(AtomicBool::new(false));
    if !state.life_runtime.outputs.watch(
        intent.output_generation,
        &bot_id,
        "main",
        &cancel,
        (&intent.connection, intent.report_sequence),
    ) || monitor.gate.watch(&cancel).is_none()
    {
        return Err(ERROR.into());
    }
    let mut core = state.life_runtime.core.lock().map_err(safe)?;
    core.observe_system(system, state.life_runtime.now());
    core.reconcile_visibility(native.visible, state.life_runtime.now());
    if state.store.profile()?["id"] != bot_id || profile != intent.profile {
        return Err(ERROR.into());
    }
    state.world.synchronize(
        &state.store,
        &crate::world_hub::current_actors(&core),
        Some(topology),
        state.life_runtime.now(),
    )?;
    let fence = core
        .consume_rest_intent(&intent, &profile, state.life_runtime.now())
        .ok_or(ERROR)?;
    let context = state.world.action_context(
        &state.store,
        &crate::world_hub::current_actors(&core),
        &bot_id,
        state.life_runtime.now(),
    )?;
    let current_source = source(
        &core,
        &app,
        &bot_id,
        &profile,
        &native,
        walk,
        state.life_runtime.now(),
    )?;
    if !core.world_outputs.matches(&bot_id, &current_source) {
        return Err(ERROR.into());
    }
    let output = core
        .world_outputs
        .ready(&bot_id, state.life_runtime.now())
        .map_err(safe)?;
    action_host::admit_move(
        &mut core,
        &state.store,
        &state.world,
        &bot_id,
        &profile,
        fence,
        context,
        output,
        target,
        cancel,
        || state.life_runtime.now(),
        super::wall_ms,
    )?;
    let result = action_host::status(
        &core,
        &state.store,
        &state.world,
        &bot_id,
        state.life_runtime.now(),
    );
    drop(core);
    publish(&app);
    actions::publish(&app);
    result
}
pub(crate) fn publish(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let commands = state.life_runtime.core.lock().ok().map(|mut core| {
        let mut commands = Vec::new();
        let now = state.life_runtime.now();
        while let Some(command) = core.world_outputs.take_routed_command(now) { commands.push(command); }
        commands
    }).unwrap_or_default();
    // Capture the exact route under Core, then release it before any Native
    // Scene/Owned lookup or Main delivery. Native commands never reach mirrors.
    for command in commands {
        match command {
            RoutedWorldCommand::Studio(command) => { let _ = app.emit_to("main", "cohost://world-action", command); }
            RoutedWorldCommand::Native { command, route, issued_at } =>
                super::native_scene_dispatch::enqueue(app, command, route, issued_at),
            RoutedWorldCommand::Unavailable(command) => super::native_scene_dispatch::reject(app, &command),
        }
    }
}
