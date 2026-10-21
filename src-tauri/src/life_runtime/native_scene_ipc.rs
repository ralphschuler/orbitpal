//! Private native document transport. None of these payloads can name an actor,
//! constructor, profile, output owner, layout policy or persistent Life effect.
use super::{
    action_host,
    native_document::AuthReceipt,
    native_scene::{self, ERROR},
};
use crate::world_output::*;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeRegistration {
    version: u16,
    capability: String,
    mount_id: String,
    normal_warm_frame_sequence: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeMeasurement {
    version: u16,
    capability: String,
    report: RenderMeasurement,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeAction {
    version: u16,
    capability: String,
    report: ActionReport,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeUnregister {
    version: u16,
    capability: String,
    render_session_id: String,
    binding_sequence: u64,
}

fn bound(
    state: &crate::AppState,
    label: &str,
    version: u16,
    capability: &str,
) -> Result<AuthReceipt, String> {
    if version != 1 {
        return Err(ERROR.into());
    }
    state
        .life_runtime
        .native_documents
        .authenticate_bound(label, capability, state.life_runtime.now())
        .map_err(|_| ERROR.into())
}
fn source_matches(core: &super::core::Core, auth: &AuthReceipt) -> bool {
    core.world_outputs
        .source(auth.document().bot())
        .is_some_and(
            |source| matches!(&source.route,SourceRoute::Native(p) if p.matches_auth(auth)),
        )
}
#[tauri::command]
pub(crate) async fn register_native_world_output(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: NativeRegistration,
) -> Result<RegisteredWorldActor, String> {
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterEntered);
    if request.version != 1 {
        return Err(ERROR.into());
    }
    let state = app.state::<crate::AppState>();
    state.updater.ensure_idle()?;
    if state.generation.try_lock().is_err() || state.overlay.has_any_pending_speech() {
        return Err(ERROR.into());
    }
    let auth = state
        .life_runtime
        .native_documents
        .authenticate(
            window.label(),
            &request.capability,
            &request.mount_id,
            request.normal_warm_frame_sequence,
            state.life_runtime.now(),
        )
        .map_err(|_| ERROR)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterAuthenticated);
    let bot = auth.document().bot();
    let lease = state
        .life_runtime
        .session_outputs
        .resolve(bot)?
        .ok_or(ERROR)?;
    let constructor = state.life_runtime.session_outputs.constructor_for(&lease)?;
    let (member, fresh) = state
        .life_runtime
        .native_scenes
        .begin(constructor, auth.clone())?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterMember);
    if fresh {
        native_scene::prepare(app.clone(), member.clone());
    }
    // Entry-local owner guard precedes Core. No Core path acquires this guard;
    // releasing it before every await is required by the cleanup handoff.
    let owned = member.owned.lock().map_err(|_| ERROR)?;
    let Some(owned) = owned.as_ref() else {
        #[cfg(feature="native-world-fixture")]
        crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterAwaitingLayout);
        return Ok(member.awaiting());
    };
    let mut core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    if owned.waiting_for_calibration(state.life_runtime.now()) {
        if !member.current()
            || !owned
                .context
                .current(&core, &state, state.life_runtime.now())
        {
            return Err(ERROR.into());
        }
        #[cfg(feature="native-world-fixture")]
        crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterAwaitingCalibration);
        return Ok(member.awaiting());
    }
    let source = owned.context.source(
        &core,
        &state,
        owned.route.clone(),
        owned.framing,
        state.life_runtime.now(),
    )?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterSource);
    let context = &source.context;
    let internal = RegisterWorldActor {
        version: 1,
        mount_id: auth.mount_id().into(),
        actor_id: bot.into(),
        world_instance_id: context.instance_id.clone(),
        stage_id: context.stage.as_str().into(),
        config_revision: context.config_revision,
        studio_proof: None,
    };
    let now = state.life_runtime.now();
    if !member.current() || !owned.route.context_current(&core, now) {
        return Err(ERROR.into());
    }
    let (reply, _, _) = core
        .world_outputs
        .register(&internal, source, now)
        .map_err(|_| ERROR)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterRegistryAccepted);
    #[cfg(feature="native-world-fixture")]
    match &reply.readiness {
        Readiness::AwaitingMeasurement { .. } => crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterAwaitingMeasurement),
        Readiness::Ready { .. } => crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterReady),
        Readiness::Unavailable { .. } => crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterUnavailable),
        Readiness::AwaitingLayout => {},
    }
    let mut binding = member.binding.lock().map_err(|_| ERROR)?;
    let identity = (reply.render_session_id.clone(), reply.binding_sequence);
    if binding.as_ref().is_some_and(|old| old != &identity) {
        member.cancel();
        return Err(ERROR.into());
    }
    *binding = Some(identity);
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::RegisterReplyPinned);
    Ok(reply)
}
#[tauri::command(async)]
pub(crate) fn report_native_world_measurement(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: NativeMeasurement,
) -> Result<MeasurementAck, String> {
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementEntered);
    let state = app.state::<crate::AppState>();
    let received = state.life_runtime.now();
    let auth = bound(&state, window.label(), request.version, &request.capability)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementAuthenticated);
    if !request.report.valid() {
        return Err(ERROR.into());
    }
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementValid);
    let member = state.life_runtime.native_scenes.find(&auth).ok_or(ERROR)?;
    let mut lock = member.owned.lock().map_err(|_| ERROR)?;
    let owned = lock.as_mut().ok_or(ERROR)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementOwned);
    let mut core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    if !source_matches(&core, &auth)
        || core
            .world_outputs
            .measurement_actor(&request.report.challenge_id)
            .as_deref()
            != Some(auth.document().bot())
    {
        return Err(ERROR.into());
    }
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementSourceMatches);
    let source = owned.context.source(
        &core,
        &state,
        owned.route.clone(),
        owned.framing,
        state.life_runtime.now(),
    )?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementSource);
    let now = state.life_runtime.now();
    if now < received || now - received > FRAME_MS || !member.current() {
        return Err(ERROR.into());
    }
    let mut ack = core
        .accept_world_measurement(&request.report, &source, received, now)
        .map_err(|_| ERROR)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementRegistryAccepted);
    let admitted = state.life_runtime.now();
    if !member.current() || !owned.route.context_current(&core, admitted) {
        member.cancel();
        return Err(ERROR.into());
    }
    // No Ready reply can escape while the shared proof remains Pending.
    let _binding_was_previously_ready =
        owned.accepted(request.report.frame_sequence, received, admitted)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementPromotionAccepted);
    ack.output = core
        .world_outputs
        .status(auth.document().bot(), state.life_runtime.now())
        .ok_or(ERROR)?;
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_native_ipc_phase(crate::native_world_fixture::FixtureIpcPhase::MeasurementAccepted);
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_frame(&app, member.constructor.bot(),
        member.constructor.signals(), member.sequence, request.report.frame_sequence,
        received, admitted, owned.route.live_current(admitted));
    Ok(ack)
}
#[tauri::command(async)]
pub(crate) fn report_native_world_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: NativeAction,
) -> Result<ActionReportAck, String> {
    let state = app.state::<crate::AppState>();
    let received = state.life_runtime.now();
    let auth = bound(&state, window.label(), request.version, &request.capability)?;
    if !request.report.valid() {
        return Err(ERROR.into());
    }
    let member = state.life_runtime.native_scenes.find(&auth).ok_or(ERROR)?;
    let mut lock = member.owned.lock().map_err(|_| ERROR)?;
    let owned = lock.as_mut().ok_or(ERROR)?;
    let mut core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    let (_, fence, _, frame, _) = request.report.parts();
    let now = state.life_runtime.now();
    if fence.actor_id != auth.document().bot()
        || !source_matches(&core, &auth)
        || !owned.route.live_current(now)
        || !owned.route.context_current(&core, now)
        || !member.current()
    {
        return Err(ERROR.into());
    }
    let ack = action_host::visual_report(
        &mut core,
        &state.store,
        &state.world,
        &request.report,
        received,
        || state.life_runtime.now(),
        super::wall_ms,
    )?;
    if matches!(ack.outcome, ReportOutcome::Accepted) {
        owned.accepted(frame.frame_sequence, received, state.life_runtime.now())?;
        #[cfg(feature="native-world-fixture")]
        crate::native_world_fixture::observe_frame(&app, member.constructor.bot(),
            member.constructor.signals(), member.sequence, frame.frame_sequence,
            received, state.life_runtime.now(), owned.route.live_current(state.life_runtime.now()));
    }
    let _ = action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    );
    drop(core);
    drop(lock);
    super::world_output::publish(&app);
    super::actions::publish(&app);
    crate::world_hub::publish(&app);
    Ok(ack)
}
#[tauri::command(async)]
pub(crate) fn unregister_native_world_output(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: NativeUnregister,
) -> Result<bool, String> {
    let state = app.state::<crate::AppState>();
    let auth = bound(&state, window.label(), request.version, &request.capability)?;
    let member = state.life_runtime.native_scenes.find(&auth).ok_or(ERROR)?;
    // Only the exact currently issued host identity may revoke this auth. Late
    // responses with old render IDs cannot touch another binding or document.
    let exact = member
        .binding
        .lock()
        .map_err(|_| ERROR)?
        .as_ref()
        .map_or_else(
            || {
                member.operation == request.render_session_id
                    && member.sequence == request.binding_sequence
            },
            |id| id.0 == request.render_session_id && id.1 == request.binding_sequence,
        );
    if !exact {
        return Ok(false);
    }
    state.life_runtime.native_documents.unregister(&auth);
    member.cancel();
    let mut core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    let removed = if source_matches(&core, &auth) {
        let removed = core.world_outputs.unregister(
            &UnregisterWorldActor {
                version: 1,
                render_session_id: request.render_session_id,
                binding_sequence: request.binding_sequence,
            },
            "",
        );
        core.interrupt_actions(
            Some(auth.document().bot()),
            crate::life_domain::CancelReason::Unavailable,
            state.life_runtime.now(),
        );
        removed
    } else {
        false
    };
    let _ = action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    );
    drop(core);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = native_scene::release_exact(&handle, member).await;
    });
    Ok(removed)
}

/// Acknowledges an asset fact only. It does not contain World readiness, a
/// renderer binding or any layout/action capability.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeAssetAck { version: u16, accepted: bool }
#[tauri::command]
pub(crate) fn report_native_world_asset(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: super::native_document::AssetReadinessReport,
) -> Result<NativeAssetAck, String> {
    let state = app.state::<crate::AppState>();
    state.life_runtime.native_documents.report_probe(window.label(), &request, state.life_runtime.now())
        .map_err(|_| ERROR)?;
    // Only the framework label and pinned native probe choose a participant.
    // This hint cannot authorize a new context or extend the probe deadline.
    state.life_runtime.native_scene_wake(window.label().strip_prefix("avatar-"));
    Ok(NativeAssetAck { version: 1, accepted: true })
}
