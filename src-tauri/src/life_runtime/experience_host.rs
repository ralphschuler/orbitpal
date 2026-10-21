//! Operator experience policy over the existing native owner and executor.
//! Neither preferences nor a catalogue entry can create a session or surface.
use super::action_host;
use crate::{
    experience::{ExperienceRuntime, InteractionKind, InteractionRequest, Preferences},
    life_domain::{Activity, CancelReason, LifeState},
    world_state::{AnchorKind, SlotId, SlotLease},
    AppState,
};
use serde_json::{json, Value};
use std::sync::{atomic::AtomicBool, Arc};
use tauri::Manager;

const ERROR: &str = "experience_native_unavailable";

pub(crate) fn experience_allows_output(app: &tauri::AppHandle, bot: &str) -> bool {
    let state = app.state::<AppState>();
    let runtime = app.state::<ExperienceRuntime>();
    state
        .store
        .experience_settings(bot)
        .ok()
        .zip(runtime.paused(bot).ok())
        .is_some_and(|(settings, paused)| settings.preferences.unsolicited_output(paused))
}

/// Cancels only this actor's exact current work. It does not revoke another
/// actor, manufacture a new UI generation or restart a stopped session.
pub(crate) fn cancel_actor_action(app: &tauri::AppHandle, bot: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.overlay.invalidate_speech(Some(bot));
    let mut core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    if let Some((claim, _)) = core.pending().filter(|(claim, _)| claim.bot_id == bot) {
        core.finish(&claim, state.life_runtime.now());
    }
    core.interrupt_actions(Some(bot), CancelReason::Operator, state.life_runtime.now());
    action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    )?;
    Ok(())
}

/// An obsolete scene may retire only the exact action that it admitted. A
/// manual replacement or a successor session is never its cancellation target.
pub(crate) fn cancel_actor_action_exact(
    app: &tauri::AppHandle,
    bot: &str,
    expected: &crate::life_domain::ActionId,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    if core
        .sessions
        .get(bot)
        .and_then(|session| session.state.as_ref())
        .and_then(|state| state.current_action())
        .is_none_or(|action| action.intent().id() != expected)
        || core
            .actions
            .as_ref()
            .and_then(|actions| actions.jobs.get(bot))
            .is_none_or(|job| job.key.action_id() != expected)
    {
        return Ok(());
    }
    core.interrupt_actions(Some(bot), CancelReason::Operator, state.life_runtime.now());
    action_host::poll(
        &mut core,
        &state.store,
        &state.world,
        || state.life_runtime.now(),
        super::wall_ms,
    )
}

pub(crate) async fn experience_preferences_changed(
    app: &tauri::AppHandle,
    bot: &str,
    preferences: &Preferences,
) -> Result<(), String> {
    preferences.validate()?;
    app.state::<ExperienceRuntime>().cancel_impulses(bot)?;
    cancel_actor_action(app, bot)
}

pub(crate) async fn experience_pause_changed(
    app: &tauri::AppHandle,
    bot: &str,
    _paused: bool,
) -> Result<(), String> {
    app.state::<ExperienceRuntime>().cancel_impulses(bot)?;
    cancel_actor_action(app, bot)
}

pub(crate) async fn content_pack_deactivating(
    app: &tauri::AppHandle,
    package: &str,
) -> Result<(), String> {
    if !crate::experience::valid_id(package) {
        return Err("pack_invalid_id".into());
    }
    let state = app.state::<AppState>();
    let bots = state.life_runtime.care.package_actors(package)?;
    for bot in bots {
        cancel_actor_action(app, &bot)?;
    }
    Ok(())
}

/// Shared admission for a social scene and an explicit operator interaction.
/// A scene's opaque lease transfers to a real Life ActionId while Core remains
/// held; observers can never steal the slot between release and admission.
pub(crate) async fn start_slot_activity(
    app: &tauri::AppHandle,
    bot: &str,
    activity: Activity,
    slot: SlotId,
    reserved: Option<SlotLease>,
) -> Result<LifeState, String> {
    start_activity(app, bot, activity, Some(slot), reserved).await
}

async fn start_activity(
    app: &tauri::AppHandle,
    bot: &str,
    activity: Activity,
    slot: Option<SlotId>,
    reserved: Option<SlotLease>,
) -> Result<LifeState, String> {
    let state = app.state::<AppState>();
    state.updater.ensure_idle()?;
    if app.state::<ExperienceRuntime>().paused(bot)? || state.overlay.has_any_pending_speech() {
        return Err(ERROR.into());
    }
    let runtime = &state.life_runtime;
    let monitor = runtime.monitor.get().ok_or(ERROR)?;
    let system = monitor.refresh().await;
    let topology = state.world.sample_topology(app).await;
    let _generation = state.generation.try_lock().map_err(|_| ERROR)?;
    let mut members = runtime.native_scenes.members(Some(bot));
    if members.len() != 1 {
        return Err(ERROR.into());
    }
    let member = members.pop().ok_or(ERROR)?;
    let owned_guard = member.owned.lock().map_err(|_| ERROR)?;
    let owned = owned_guard.as_ref().ok_or(ERROR)?;
    let mut core = runtime.core.lock().map_err(|_| ERROR)?;
    core.observe_system(system, runtime.now());
    state.world.synchronize(
        &state.store,
        &crate::world_hub::current_actors(&core),
        Some(topology),
        runtime.now(),
    )?;
    if !member.current()
        || !runtime
            .session_outputs
            .constructor_current(&member.constructor)
        || !owned.context.current(&core, &state, runtime.now())
        || app.state::<ExperienceRuntime>().paused(bot)?
        || state.overlay.has_any_pending_speech()
    {
        return Err(ERROR.into());
    }
    let source = owned.context.source(
        &core,
        &state,
        owned.route.clone(),
        owned.framing,
        runtime.now(),
    )?;
    if !core.world_outputs.matches(bot, &source) {
        return Err(ERROR.into());
    }
    let fence = core.local_fence(bot, runtime.now()).ok_or(ERROR)?;
    let output = core
        .world_outputs
        .ready(bot, runtime.now())
        .map_err(|_| ERROR)?;
    let permit = owned.action_permit(runtime.now()).ok_or(ERROR)?;
    if !permit.current(&core, runtime.now()) || !permit.matches_output(&output) {
        return Err(ERROR.into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let ticket = runtime.outputs.ticket().ok_or(ERROR)?;
    if !runtime
        .outputs
        .watch_delivery(ticket, bot, member.constructor.window().label(), &cancel)
        || monitor.gate.watch(&cancel).is_none()
    {
        return Err(ERROR.into());
    }
    runtime
        .session_outputs
        .bind_surface(&member.constructor, &cancel)?;
    if let Some(lease) = reserved {
        let slot_id = slot.as_ref().ok_or(ERROR)?;
        let stamp = state.world.validate_current_lease(
            &core,
            &state.store,
            bot,
            &lease,
            slot_id,
            runtime.now(),
        )?;
        if !state.world.release(&stamp, &lease, runtime.now())? {
            return Err(ERROR.into());
        }
    }
    if activity == Activity::MoveTo {
        action_host::admit_slot_move(
            &mut core,
            &state.store,
            &state.world,
            bot,
            &owned.context.profile,
            fence,
            source.context,
            output,
            permit,
            slot.ok_or(ERROR)?,
            cancel,
            || runtime.now(),
            super::wall_ms,
        )?;
    } else {
        action_host::admit_native_care(
            &mut core,
            &state.store,
            &state.world,
            bot,
            &owned.context.profile,
            fence,
            source.context,
            output,
            permit,
            activity,
            slot,
            cancel,
            || runtime.now(),
            super::wall_ms,
        )?;
    }
    core.sessions
        .get(bot)
        .and_then(|session| session.state.clone())
        .ok_or_else(|| ERROR.into())
}

fn result(bot: &str, kind: InteractionKind, status: &str, reason: Option<&str>) -> Value {
    json!({"version":1,"botId":bot,"kind":kind,"status":status,"reason":reason})
}

pub(crate) async fn request_character_interaction(
    app: &tauri::AppHandle,
    bot: &str,
    request: &InteractionRequest,
) -> Result<Value, String> {
    request.validate()?;
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    if request.kind == InteractionKind::Pause {
        app.state::<ExperienceRuntime>().set_paused(bot, true)?;
        experience_pause_changed(app, bot, true).await?;
        return Ok(result(bot, request.kind, "accepted", None));
    }
    if app.state::<ExperienceRuntime>().paused(bot)? {
        return Ok(result(bot, request.kind, "declined", Some("resting")));
    }
    let busy = runtime
        .core
        .lock()
        .map_err(|_| ERROR)?
        .actions
        .as_ref()
        .is_some_and(|a| a.actor_busy(bot));
    if busy || state.overlay.has_any_pending_speech() {
        return Ok(result(bot, request.kind, "declined", Some("busy")));
    }
    if request.target_actor_id.is_some() {
        return Ok(result(bot, request.kind, "declined", Some("unavailable")));
    }
    if request.kind == InteractionKind::Pet {
        let profile = state.store.bot_profile(bot)?;
        let caps = crate::emote_capabilities::inspect(&state.store.root, &profile)?;
        let emote = profile["emotes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|emote| {
                emote["action"].as_str().is_some_and(|action| {
                    action.eq_ignore_ascii_case("pet")
                        && crate::emote_capabilities::resolve(&caps, action).is_ok()
                })
            })
            .and_then(|emote| emote["id"].as_str());
        let Some(id) = emote else {
            return Ok(result(bot, request.kind, "declined", Some("unavailable")));
        };
        crate::overlay::trigger_emote_profile(app, &profile, id, false)?;
        return Ok(result(bot, request.kind, "accepted", None));
    }
    let wanted = if request.kind == InteractionKind::Home {
        AnchorKind::Rest
    } else {
        AnchorKind::Play
    };
    // Select through the current native source, not from a renderer ID.
    let selected = {
        let members = runtime.native_scenes.members(Some(bot));
        let member = members.first().ok_or(ERROR)?;
        let guard = member.owned.lock().map_err(|_| ERROR)?;
        let owned = guard.as_ref().ok_or(ERROR)?;
        let core = runtime.core.lock().map_err(|_| ERROR)?;
        let source = owned.context.source(
            &core,
            &state,
            owned.route.clone(),
            owned.framing,
            runtime.now(),
        )?;
        let mut slots = state.world.action_slots(&state.store, &source.context)?;
        slots.retain(|slot| slot.kind == wanted);
        slots.sort_by_key(|slot| {
            (
                slot.position.x().abs_diff(source.context.position.x()) as u64
                    + slot.position.y().abs_diff(source.context.position.y()) as u64,
                slot.id.clone(),
            )
        });
        slots.first().map(|slot| slot.id.clone())
    };
    if request.kind == InteractionKind::Home && selected.is_none() {
        return Ok(result(bot, request.kind, "declined", Some("unavailable")));
    }
    let activity = if request.kind == InteractionKind::Home {
        Activity::MoveTo
    } else {
        Activity::Play
    };
    match start_activity(app, bot, activity, selected, None).await {
        Ok(_) => Ok(result(bot, request.kind, "accepted", None)),
        Err(_) => Ok(result(bot, request.kind, "declined", Some("unavailable"))),
    }
}
