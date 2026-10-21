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
use tauri::{Manager, Emitter};

const ERROR: &str = "experience_native_unavailable";

/// A UI label cannot supply this owner. The native constructor, live permit,
/// exact calibrated binding and real window are joined before a menu exists.
pub(crate) fn native_interaction_owner(app: &tauri::AppHandle, bot: &str) -> Result<crate::native_interaction::MenuOwner, String> {
    let state = app.state::<AppState>();
    state.updater.ensure_idle()?;
    let runtime = &state.life_runtime;
    let members = runtime.native_scenes.members(Some(bot));
    if members.len()!=1 { return Err(ERROR.into()); }
    let member = members.into_iter().next().ok_or(ERROR)?;
    let guard = member.owned.lock().map_err(|_| ERROR)?;
    let owned = guard.as_ref().ok_or(ERROR)?;
    let core = runtime.core.lock().map_err(|_| ERROR)?;
    let now = runtime.now();
    if !member.current() || !runtime.session_outputs.constructor_current(&member.constructor)
        || !owned.context.current(&core,&state,now) { return Err(ERROR.into()); }
    let output = core.world_outputs.ready(bot,now).map_err(|_| ERROR)?;
    let permit = owned.action_permit(now).ok_or(ERROR)?;
    if !permit.current(&core,now) || !permit.matches_output(&output) { return Err(ERROR.into()); }
    let point = (output.binding.position.x*owned.framing.scale_factor, output.binding.position.y*owned.framing.scale_factor);
    let scope = format!("{}:{}:{}:{}",owned.route.namespace(),output.binding.render_session_id,output.binding.binding_sequence,output.binding.measurement_revision);
    let window = member.constructor.window().clone();
    let signals = member.constructor.signals().clone();
    let handle=app.clone(); let member=member.clone();
    let current=Arc::new(move || {
        let state=handle.state::<AppState>();
        let runtime=&state.life_runtime;
        if !member.current() || !runtime.session_outputs.constructor_current(&member.constructor) || state.updater.ensure_idle().is_err() { return false; }
        let Ok(core)=runtime.core.lock() else { return false; };
        permit.current(&core,runtime.now()) && permit.matches_output(&output) && core.world_outputs.check_proof(&output,runtime.now())
    });
    drop(core); drop(guard);
    crate::native_interaction::MenuOwner::new(bot.into(),window,scope,signals,point,current)
}

/// Notifications are queued at real checked action commits before job retirement.
pub(super) fn observe_facts(runtime: &super::LifeRuntime, app: &tauri::AppHandle) {
    let bots=runtime.core.lock().ok().and_then(|mut core|core.actions.as_mut().map(|actions|std::mem::take(&mut actions.fact_notifications))).unwrap_or_default();
    for bot in bots {
        if let Ok(snapshot)=crate::experience_commands::native_snapshot(app,&bot) { let _=app.emit_to("main","experience-state",snapshot); }
    }
}

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

/// The platform adapter supplies a sealed native receive lease, not chat text.
/// A proposal never creates a session, changes a preset or grants model tools.
pub(crate) async fn consume_platform_impulse(app:&tauri::AppHandle,owner:&str,kind:&crate::experience::ImpulseKind,actors:&[String],authority:&crate::platform_impulses::DispatchLease)->Result<(),String> {
    use crate::experience::ImpulseKind;
    let state=app.state::<AppState>();
    let experience=app.state::<ExperienceRuntime>();
    let preferences=state.store.experience_settings(owner)?.preferences;
    if authority.owner()!=owner || !authority.current(&state.store) || !preferences.stream_impulses
        || !preferences.unsolicited_output(experience.paused(owner)?) || actors.is_empty() || actors.len()>8
        || !actors.iter().any(|actor|actor==owner)
        || actors.iter().any(|actor| !preferences.impulse_participants.contains(actor)
            || !experience_allows_output(app,actor)) {return Err(ERROR.into());}
    {
        let core=state.life_runtime.core.lock().map_err(|_|ERROR)?;
        if core.sessions.get(owner).is_none_or(|session|session.session_id!=authority.session_id()
            ||session.mode!=super::core::Mode::Life) || state.overlay.has_any_pending_speech() {return Err(ERROR.into());}
    }
    match kind {
        ImpulseKind::Greeting|ImpulseKind::Play=>{
            let other=actors.iter().find(|actor|actor.as_str()!=owner).ok_or(ERROR)?;
            super::social_host::request_from_stream(app,owner,other,
                if *kind==ImpulseKind::Greeting {crate::social::SceneKind::Greeting}else{crate::social::SceneKind::Play},
                Arc::new(authority.clone()))?;
            Ok(())
        },
        ImpulseKind::VoteRest|ImpulseKind::VotePlay=>{
            if state.life_runtime.social.busy(owner) {return Err(ERROR.into());}
            // An individual configured suggestion. It is not an invented poll
            // result or a claim that every permitted participant has acted.
            start_activity(app,owner,if *kind==ImpulseKind::VoteRest{Activity::Rest}else{Activity::Play},
                None,None,None,None,None,Some((owner,authority))).await?;
            Ok(())
        },
    }
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
    let state = app.state::<AppState>();
    state.life_runtime.social.cancel_actor(bot, state.life_runtime.social_now())?;
    state.life_runtime.desktop.cancel_actor(bot);
    cancel_actor_action(app, bot)
}

pub(crate) async fn experience_pause_changed(
    app: &tauri::AppHandle,
    bot: &str,
    _paused: bool,
) -> Result<(), String> {
    app.state::<ExperienceRuntime>().cancel_impulses(bot)?;
    let state = app.state::<AppState>();
    state.life_runtime.social.cancel_actor(bot, state.life_runtime.social_now())?;
    state.life_runtime.desktop.cancel_actor(bot);
    cancel_actor_action(app, bot)
}

/// Catalog CAS and revocation share the actual Core admission lock. No new
/// action can start between deactivation and exact dependent-job cancellation.
pub(crate) fn set_content_pack_active_native(app:&tauri::AppHandle,package:&str,expected:u64,active:bool)->Result<Vec<crate::content_packs::Installed>,String> {
    if !crate::experience::valid_id(package) {return Err("pack_invalid_id".into());}
    let state=app.state::<AppState>();
    state.updater.ensure_idle()?;
    let mut core=state.life_runtime.core.lock().map_err(|_|ERROR)?;
    let packs=state.store.set_content_pack_active(package,expected,active)?;
    if !active {
        let bots=core.actions.as_ref().map(|actions|actions.jobs.iter().filter(|(_,job)|job.content.as_ref().is_some_and(|usage|usage.reference.package_id==package)).map(|(bot,_)|bot.clone()).collect::<Vec<_>>()).unwrap_or_default();
        // Revoke every dependent job token before any optional coordinator
        // failure can prevent another actor's actual cancellation.
        for bot in &bots {
            core.interrupt_actions(Some(bot),CancelReason::Operator,state.life_runtime.now());
        }
        let mut errors=Vec::new();
        for bot in &bots {
            if let Err(error)=state.life_runtime.social.cancel_actor(bot,state.life_runtime.social_now()) {errors.push(error);}
        }
        if let Err(error)=action_host::poll(&mut core,&state.store,&state.world,||state.life_runtime.now(),super::wall_ms) {errors.push(error);}
        if !errors.is_empty() {return Err(errors.join("; "));}
    }
    Ok(packs)
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
    guard: Option<super::social_host::SceneActionGuard>,
) -> Result<LifeState, String> {
    start_activity(app, bot, activity, Some(slot), reserved, guard, None, None, None).await
}

pub(crate) async fn start_point_activity(app:&tauri::AppHandle, bot:&str, target:crate::world_state::coordinates::Point, expected:crate::world_output::OutputProof)->Result<LifeState,String> {
    start_activity(app,bot,Activity::MoveTo,None,None,None,Some(target),Some(expected),None).await
}

async fn start_activity(
    app: &tauri::AppHandle,
    bot: &str,
    activity: Activity,
    slot: Option<SlotId>,
    reserved: Option<SlotLease>,
    guard: Option<super::social_host::SceneActionGuard>,
    point: Option<crate::world_state::coordinates::Point>,
    expected: Option<crate::world_output::OutputProof>,
    platform: Option<(&str,&crate::platform_impulses::DispatchLease)>,
) -> Result<LifeState, String> {
    start_activity_guarded(app,bot,activity,slot,reserved,guard,point,expected,platform,None).await
}

pub(crate) async fn start_window_rest_activity(app:&tauri::AppHandle,bot:&str,expected:crate::world_output::OutputProof,guard:super::desktop_places::WindowRestGuard)->Result<LifeState,String> {
    start_activity_guarded(app,bot,Activity::Rest,None,None,None,None,Some(expected),None,Some(guard)).await
}

async fn start_activity_guarded(
    app: &tauri::AppHandle,
    bot: &str,
    activity: Activity,
    slot: Option<SlotId>,
    reserved: Option<SlotLease>,
    guard: Option<super::social_host::SceneActionGuard>,
    point: Option<crate::world_state::coordinates::Point>,
    expected: Option<crate::world_output::OutputProof>,
    platform: Option<(&str,&crate::platform_impulses::DispatchLease)>,
    window_rest: Option<super::desktop_places::WindowRestGuard>,
) -> Result<LifeState, String> {
    let state = app.state::<AppState>();
    let cancel = Arc::new(AtomicBool::new(false));
    let stream_source=if let Some((_,lease))=platform {Some(lease.action_fence(&state.store)?)}
        else if let Some(guard)=&guard {guard.action_fence(&state.store)?} else {None};
    if let Some(source)=&stream_source {source.bind_cancel(&cancel)?;}
    if let Some(guard)=&window_rest {
        if activity!=Activity::Rest || slot.is_some() || reserved.is_some() || point.is_some()
            || expected.as_ref().is_none_or(|output|!guard.current(bot,output,state.life_runtime.now())) {return Err(ERROR.into());}
        guard.bind(&cancel)?;
    }
    if let Some((_,lease))=platform {
        if lease.owner()!=bot || !lease.current(&state.store) {return Err(ERROR.into());}
        lease.bind_cancel(&cancel)?;
    }
    if let Some(guard)=&guard {
        if guard.actor()!=bot || !guard.current() || !guard.source_current(&state.store) {return Err(ERROR.into());}
        guard.bind(&cancel)?;
    }
    let grouped = reserved.is_some();
    if grouped && guard.is_none() {return Err(ERROR.into());}
    state.updater.ensure_idle()?;
    if app.state::<ExperienceRuntime>().paused(bot)? || state.overlay.has_any_pending_speech() {
        return Err(ERROR.into());
    }
    let runtime = &state.life_runtime;
    // Copy advisory origin before Owned/Core: Care's chooser lock precedes
    // these locks, so admission must never acquire it in the inverse order.
    let origin = runtime.care.origin(bot);
    let monitor = runtime.monitor.get().ok_or(ERROR)?;
    let system = monitor.refresh().await;
    let topology = state.world.sample_topology(app).await;
    let mut members = runtime.native_scenes.members(Some(bot));
    if members.len() != 1 {
        return Err(ERROR.into());
    }
    let member = members.pop().ok_or(ERROR)?;
    let owned_guard = member.owned.lock().map_err(|_| ERROR)?;
    let owned = owned_guard.as_ref().ok_or(ERROR)?;
    // Observe a real dialog without owning its generation turn: the retained
    // Live owner's cleanup also checks that lock. Core serializes native Plan/
    // SQL admission; a racing dialog must quiesce/cancel before any output.
    drop(state.generation.try_lock().map_err(|_| ERROR)?);
    let mut core = runtime.core.lock().map_err(|_| ERROR)?;
    drop(state.generation.try_lock().map_err(|_| ERROR)?);
    if stream_source.as_ref().is_some_and(|source|!source.current() || core.sessions.get(source.owner()).is_none_or(|session|session.session_id!=source.session_id())) {return Err(ERROR.into());}
    if guard.as_ref().is_some_and(|guard|!guard.source_current(&state.store)
        || guard.source_session().is_some_and(|(owner,session)|core.sessions.get(owner).is_none_or(|actual|actual.session_id!=session))) {return Err(ERROR.into());}
    if platform.is_some_and(|(owner,lease)|!lease.current(&state.store)
        || core.sessions.get(owner).is_none_or(|session|session.session_id!=lease.session_id())) {return Err(ERROR.into());}
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
        || cancel.load(std::sync::atomic::Ordering::SeqCst)
        || guard.as_ref().is_some_and(|guard|guard.actor()!=bot || !guard.current())
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
    let preferences = state.store.experience_settings(bot)?.preferences;
    let target = slot.as_ref().map(|id| state.world.slot_target(&state.store, &source.context, id))
        .transpose()?.map(|slot| slot.position).or(point).unwrap_or(source.context.position);
    let home = origin.filter(|(session, context, _)| session == &fence.session
        && context == &(source.context.instance_id.clone(), source.context.config_revision, source.context.topology_revision))
        .map(|(_, _, point)| point).unwrap_or(source.context.position);
    let moving = target != source.context.position;
    if !preferences.allowed_animations.iter().any(|animation| animation == "idle")
        || (moving && !preferences.allowed_animations.iter().any(|animation| animation == "walk"))
        || !super::self_care::within_radius(home, target, preferences.movement_radius) {
        return Err(ERROR.into());
    }
    let output = core
        .world_outputs
        .ready(bot, runtime.now())
        .map_err(|_| ERROR)?;
    let permit = owned.action_permit(runtime.now()).ok_or(ERROR)?;
    if point.is_some_and(|point|!runtime.desktop.admission_current(bot,point,runtime.now())) {return Err(ERROR.into());}
    if !permit.current(&core, runtime.now()) || !permit.matches_output(&output) {
        return Err(ERROR.into());
    }
    if expected.as_ref().is_some_and(|expected|expected.binding!=output.binding
        || !core.world_outputs.check_proof(expected,runtime.now()) || !permit.matches_output(expected)) {return Err(ERROR.into());}
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
    if let Some(stream)=stream_source {
        if window_rest.is_some() || point.is_some(){return Err(ERROR.into());}
        action_host::admit_native_stream_activity(&mut core,&state.store,&state.world,bot,&owned.context.profile,
            fence,source.context,output,permit,activity,slot,stream,cancel,||runtime.now(),super::wall_ms)?;
    } else if let Some(guard)=window_rest {
        action_host::admit_native_window_rest(&mut core,&state.store,&state.world,bot,&owned.context.profile,
            fence,source.context,output,permit,guard,cancel,||runtime.now(),super::wall_ms)?;
    } else if activity == Activity::MoveTo {
        if let Some(target)=point {
            action_host::admit_native_move(&mut core,&state.store,&state.world,bot,&owned.context.profile,
                fence,source.context,output,permit,target,cancel,||runtime.now(),super::wall_ms)?;
        } else { action_host::admit_slot_move(
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
        )?; }
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
    if grouped { if let Some(job)=core.actions.as_mut().and_then(|actions|actions.jobs.get_mut(bot)) {job.grouped=true;} }
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
    if busy || runtime.social.busy(bot) || state.overlay.has_any_pending_speech() {
        return Ok(result(bot, request.kind, "declined", Some("busy")));
    }
    if let Some(target) = &request.target_actor_id {
        if request.kind != InteractionKind::Play { return Ok(result(bot, request.kind, "declined", Some("unavailable"))); }
        return Ok(match super::social_host::request(app, bot, target, crate::social::SceneKind::Play) {
            Ok(_) => result(bot, request.kind, "waiting", None),
            Err(_) => result(bot, request.kind, "declined", Some("unavailable")),
        });
    }
    if request.kind == InteractionKind::Pet {
        let preferences=state.store.experience_settings(bot)?.preferences;
        if !preferences.allowed_animations.iter().any(|animation|animation=="pet") {
            return Ok(result(bot,request.kind,"declined",Some("unavailable")));
        }
        let owner=native_interaction_owner(app,bot)?;
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
        if !owner.is_current() || state.store.bot_profile(bot)?!=profile || runtime.social.busy(bot) {
            return Ok(result(bot,request.kind,"declined",Some("unavailable")));
        }
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
    match start_activity(app, bot, activity, selected, None, None, None, None, None).await {
        Ok(_) => Ok(result(bot, request.kind, "accepted", None)),
        Err(_) => Ok(result(bot, request.kind, "declined", Some("unavailable"))),
    }
}
