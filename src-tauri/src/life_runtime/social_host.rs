//! Real two-actor social work. The pure coordinator never grants a native surface,
//! reservation, arrival or completion; every transition below consumes host receipts.
use super::{cancel_actor_action, start_slot_activity, LifeRuntime};
use crate::{
    experience::{ActivityFact, ExperienceRuntime, FactKind, FactPhase},
    life_domain::{ActionId, Activity, LifeState},
    social::{Coordinator, Directive, Outcome, Participant, Phase, Reason, SceneKind, SceneView},
    social_dialogue::{DialogueHub, Turn},
    world_hub::ActionSlotTarget,
    world_state::{AnchorKind, SessionStamp, SlotId, SlotLease, WorkIdentity},
    AppState,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::Manager;
const ERROR: &str = "social_native_unavailable";
const LEASE_MS: u64 = 120_000;
#[derive(Clone, Debug, Eq, PartialEq)]
struct Scope {
    instance: String,
    revision: u64,
    topology: u64,
}
#[derive(Clone)]
struct Actor {
    participant: Participant,
    scope: Scope,
    slots: Vec<ActionSlotTarget>,
    life: LifeState,
}
#[derive(Clone)]
struct Reservation {
    bot: String,
    stamp: SessionStamp,
    lease: SlotLease,
}
#[derive(Default)]
struct Memory {
    initialized: bool,
    coordinator: Coordinator,
    scopes: BTreeMap<String, [Scope; 2]>,
    leases: BTreeMap<String, Vec<Reservation>>,
    terminal: BTreeMap<String, Directive>,
    actions: BTreeMap<String, Vec<ActionId>>,
    dialogues: DialogueHub,
    conversations: BTreeMap<String, [String; 2]>,
    turns: BTreeMap<String, Turn>,
    next_initiative: u64,
}
#[derive(Default)]
pub(crate) struct SocialRuntime {
    memory: Mutex<Memory>,
    in_flight: AtomicBool,
}
impl SocialRuntime {
    pub(crate) fn busy(&self, bot: &str) -> bool {
        self.memory
            .lock()
            .map(|m| m.coordinator.busy(bot))
            .unwrap_or(true)
    }
    /// Synchronous revocation; the next pulse performs durable terminal cleanup.
    pub(crate) fn cancel_actor(&self, bot: &str, now: u64) -> Result<(), String> {
        let mut m = self.memory.lock().map_err(|_| ERROR)?;
        m.dialogues.cancel_actor(bot);
        m.conversations
            .retain(|_, pair| !pair.iter().any(|id| id == bot));
        m.turns
            .retain(|_, turn| !turn.participants.iter().any(|id| id == bot));
        let scenes = m.coordinator.views(bot);
        for scene in scenes.into_iter().filter(|s| !s.phase.terminal()) {
            m.coordinator.cancel(&scene.id, Reason::Operator, now)?;
        }
        Ok(())
    }
}
fn actors(app: &tauri::AppHandle) -> Vec<Actor> {
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    let experience = app.state::<ExperienceRuntime>();
    let mut result = Vec::new();
    for member in runtime.native_scenes.members(None) {
        let bot = member.constructor.bot();
        if experience.paused(bot).unwrap_or(true)
            || !member.current()
            || !runtime
                .session_outputs
                .constructor_current(&member.constructor)
        {
            continue;
        }
        let Ok(settings) = state.store.experience_settings(bot) else {
            continue;
        };
        if !settings
            .preferences
            .allowed_animations
            .iter()
            .any(|a| a == "walk")
        {
            continue;
        }
        let Ok(owned) = member.owned.lock() else {
            continue;
        };
        let Some(owned) = owned.as_ref() else {
            continue;
        };
        let Ok(core) = runtime.core.lock() else {
            continue;
        };
        let now = runtime.now();
        if !owned.context.current(&core, &state, now) || core.local_fence(bot, now).is_none() {
            continue;
        }
        let Some(session) = core.sessions.get(bot) else {
            continue;
        };
        let Some(life) = session.state.clone() else {
            continue;
        };
        let Ok(source) =
            owned
                .context
                .source(&core, &state, owned.route.clone(), owned.framing, now)
        else {
            continue;
        };
        let Ok(output) = core.world_outputs.ready(bot, now) else {
            continue;
        };
        let Some(permit) = owned.action_permit(now) else {
            continue;
        };
        if !core.world_outputs.matches(bot, &source)
            || !permit.current(&core, now)
            || !permit.matches_output(&output)
        {
            continue;
        }
        let Ok(mut slots) = state.world.action_slots(&state.store, &source.context) else {
            continue;
        };
        let radius = u64::from(settings.preferences.movement_radius) * 1000;
        slots.retain(|slot| {
            u64::from(slot.position.x().abs_diff(source.context.position.x()))
                + u64::from(slot.position.y().abs_diff(source.context.position.y()))
                <= radius
        });
        let Ok(snapshot) = state.store.social_snapshot(bot) else {
            continue;
        };
        result.push(Actor {
            participant: Participant {
                actor: life.actor_id().clone(),
                world: life.world_id().clone(),
                session: session.session_id.clone(),
                generation: session.revision,
                available: true,
                needs: life.needs(),
                character: snapshot.development,
                familiarity: 0,
                affection: 0,
            },
            scope: Scope {
                instance: source.context.instance_id,
                revision: source.context.config_revision,
                topology: source.context.topology_revision,
            },
            slots,
            life,
        });
    }
    result.sort_by(|a, b| a.participant.actor.cmp(&b.participant.actor));
    result
}
fn kind_anchor(kind: SceneKind) -> AnchorKind {
    match kind {
        SceneKind::Greeting => AnchorKind::Meeting,
        SceneKind::Play => AnchorKind::Play,
        SceneKind::Rest => AnchorKind::Rest,
    }
}
fn choose_slots(a: &Actor, b: &Actor, kind: SceneKind) -> Option<[SlotId; 2]> {
    if a.scope != b.scope {
        return None;
    }
    // Slot IDs are opaque. Geometry and role came from the actual scoped World.
    for left in a.slots.iter().filter(|s| s.kind == kind_anchor(kind)) {
        for right in b.slots.iter().filter(|s| s.kind == kind_anchor(kind)) {
            if left.id != right.id {
                return Some([left.id.clone(), right.id.clone()]);
            }
        }
    }
    None
}
fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut m = state.life_runtime.social.memory.lock().map_err(|_| ERROR)?;
    if !m.initialized {
        state
            .store
            .social_recover(state.life_runtime.social_now())?;
        m.initialized = true;
    }
    Ok(())
}
pub(crate) fn request(
    app: &tauri::AppHandle,
    bot: &str,
    target: &str,
    kind: SceneKind,
) -> Result<String, String> {
    initialize(app)?;
    if bot == target {
        return Err(ERROR.into());
    }
    let state = app.state::<AppState>();
    state.updater.ensure_idle()?;
    let current = actors(app);
    let mut a = current
        .iter()
        .find(|a| a.participant.actor.as_str() == bot)
        .cloned()
        .ok_or(ERROR)?;
    let mut b = current
        .iter()
        .find(|a| a.participant.actor.as_str() == target)
        .cloned()
        .ok_or(ERROR)?;
    let slots = choose_slots(&a, &b, kind).ok_or(ERROR)?;
    {
        let core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
        if [bot, target].iter().any(|id| {
            core.actions
                .as_ref()
                .is_none_or(|actions| actions.actor_busy(id))
        }) {
            return Err(ERROR.into());
        }
    }
    for (actor, other) in [(&mut a, target), (&mut b, bot)] {
        if let Some(relation) = state
            .store
            .social_snapshot(actor.participant.actor.as_str())?
            .relationships
            .iter()
            .find(|r| r.other_id == other)
        {
            actor.participant.familiarity = relation.familiarity;
            actor.participant.affection = relation.affection;
        }
    }
    let sequence = state
        .store
        .social_next_scene_sequence(&[bot.into(), target.into()])?;
    let mut m = state.life_runtime.social.memory.lock().map_err(|_| ERROR)?;
    let scene = m.coordinator.invite(
        sequence,
        kind,
        [a.participant, b.participant],
        slots,
        state.life_runtime.social_now(),
    )?;
    m.scopes.insert(scene.id.clone(), [a.scope, b.scope]);
    if let Err(error) = state.store.social_save_scene(&scene) {
        m.coordinator.cancel(
            &scene.id,
            Reason::Unavailable,
            state.life_runtime.social_now(),
        )?;
        return Err(error);
    }
    drop(m);
    publish(app, &scene.participants);
    Ok(scene.id)
}
pub(crate) fn respond(
    app: &tauri::AppHandle,
    bot: &str,
    id: &str,
    accept: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut m = state.life_runtime.social.memory.lock().map_err(|_| ERROR)?;
    let scene = m
        .coordinator
        .views(bot)
        .into_iter()
        .find(|s| s.id == id && s.participants[1] == bot)
        .ok_or(ERROR)?;
    m.coordinator
        .respond(id, accept, state.life_runtime.social_now())?;
    for view in m
        .coordinator
        .views(bot)
        .into_iter()
        .filter(|v| v.id == scene.id)
    {
        state.store.social_save_scene(&view)?;
    }
    Ok(())
}
pub(crate) fn cancel_scene(app: &tauri::AppHandle, bot: &str, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut m = state.life_runtime.social.memory.lock().map_err(|_| ERROR)?;
    if !m.coordinator.views(bot).iter().any(|s| s.id == id) {
        return Err(ERROR.into());
    }
    m.coordinator
        .cancel(id, Reason::Operator, state.life_runtime.social_now())
}
pub(crate) fn cancel_actor(app: &tauri::AppHandle, bot: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    state
        .life_runtime
        .social
        .cancel_actor(bot, state.life_runtime.social_now())?;
    cancel_actor_action(app, bot)
}
fn publish(app: &tauri::AppHandle, pair: &[String; 2]) {
    let state = app.state::<AppState>();
    for bot in pair {
        if let Ok(snapshot) = state.store.social_snapshot(bot) {
            state.overlay.publish(
                app,
                serde_json::json!({"kind":"social-state","botId":bot,"snapshot":snapshot}),
            );
        }
    }
}
fn reserve(
    app: &tauri::AppHandle,
    scene: &SceneView,
    slots: &[SlotId; 2],
) -> Result<Vec<Reservation>, String> {
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    let core = runtime.core.lock().map_err(|_| ERROR)?;
    let mut reserved = Vec::new();
    for (bot, slot) in scene.participants.iter().zip(slots) {
        let stamp = crate::world_hub::current_actors(&core)
            .into_iter()
            .find(|a| a.stamp.actor_id().as_str() == bot)
            .ok_or(ERROR)?
            .stamp;
        match state.world.reserve_current(
            &core,
            &state.store,
            bot,
            WorkIdentity::operation(&scene.id).map_err(|_| ERROR)?,
            slot,
            LEASE_MS,
            runtime.now(),
        ) {
            Ok(lease) => reserved.push(Reservation {
                bot: bot.clone(),
                stamp,
                lease,
            }),
            Err(error) => {
                for old in &reserved {
                    state.world.release(&old.stamp, &old.lease, runtime.now())?;
                }
                return Err(error);
            }
        }
    }
    Ok(reserved)
}
fn release(app: &tauri::AppHandle, reservations: &[Reservation]) -> Result<(), String> {
    let state = app.state::<AppState>();
    let core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
    let actors = crate::world_hub::current_actors(&core);
    // synchronize explicitly unbinds obsolete sessions and releases their leases.
    state
        .world
        .synchronize(&state.store, &actors, None, state.life_runtime.now())?;
    for old in reservations {
        if actors.iter().any(|a| a.stamp == old.stamp) {
            state
                .world
                .release(&old.stamp, &old.lease, state.life_runtime.now())?;
        }
    }
    Ok(())
}
pub(super) fn pulse(runtime: &LifeRuntime, app: &tauri::AppHandle) {
    if runtime.stopping.load(Ordering::SeqCst)
        || runtime.social.in_flight.swap(true, Ordering::SeqCst)
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _ = drive(&app).await;
        state
            .life_runtime
            .social
            .in_flight
            .store(false, Ordering::SeqCst);
    });
}
async fn drive(app: &tauri::AppHandle) -> Result<(), String> {
    initialize(app)?;
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    let current = actors(app);
    let now = runtime.social_now();
    {
        let mut m = runtime.social.memory.lock().map_err(|_| ERROR)?;
        let views = current
            .iter()
            .flat_map(|a| m.coordinator.views(a.participant.actor.as_str()))
            .map(|s| (s.id.clone(), s))
            .collect::<BTreeMap<_, _>>();
        for scene in views.values().filter(|s| !s.phase.terminal()) {
            let changed = m.scopes.get(&scene.id).is_none_or(|scopes| {
                scene.participants.iter().enumerate().any(|(i, id)| {
                    current
                        .iter()
                        .find(|a| a.participant.actor.as_str() == id)
                        .is_none_or(|a| a.scope != scopes[i])
                })
            });
            if changed {
                m.coordinator.cancel(&scene.id, Reason::Unavailable, now)?;
            }
        }
        m.coordinator.reconcile(
            &current
                .iter()
                .map(|a| a.participant.clone())
                .collect::<Vec<_>>(),
            now,
        )?;
        for actor in &current {
            for scene in m.coordinator.views(actor.participant.actor.as_str()) {
                if !scene.phase.terminal() {
                    m.coordinator.observe(&scene.id, &actor.life, now)?;
                }
                if scene.phase == Phase::Invitation && now.saturating_sub(scene.at_ms) >= 1000 {
                    m.coordinator.auto_respond(&scene.id, now)?;
                }
            }
        }
    }
    // At most one deterministic invitation attempt per minute, never while an
    // actor already owns native work. No renderer timer or inactive catch-up.
    let initiative = {
        let mut m = runtime.social.memory.lock().map_err(|_| ERROR)?;
        if now >= m.next_initiative {
            m.next_initiative = now.saturating_add(60000);
            true
        } else {
            false
        }
    };
    if initiative {
        'pairs: for (i, a) in current.iter().enumerate() {
            if a.participant.character.traits.initiative < 500
                || a.participant.needs.social().units() > 6000
            {
                continue;
            }
            for b in current.iter().skip(i + 1) {
                if request(
                    app,
                    a.participant.actor.as_str(),
                    b.participant.actor.as_str(),
                    SceneKind::Greeting,
                )
                .is_ok()
                {
                    break 'pairs;
                }
            }
        }
    }
    for _ in 0..8 {
        let directives = {
            let mut m = runtime.social.memory.lock().map_err(|_| ERROR)?;
            let mut work = m.coordinator.drain();
            work.extend(m.terminal.values().cloned());
            m.terminal.clear();
            work
        };
        if directives.is_empty() {
            break;
        }
        for directive in directives {
            match &directive {
                Directive::Record { event } => {
                    state.store.social_apply_event(event)?;
                }
                Directive::Reserve { scene, slots } => {
                    let reserved = reserve(app, scene, slots);
                    let mut m = runtime.social.memory.lock().map_err(|_| ERROR)?;
                    if let Ok(leases) = reserved {
                        m.leases.insert(scene.id.clone(), leases);
                        m.coordinator
                            .reserved(&scene.id, true, runtime.social_now())?;
                    } else {
                        m.coordinator
                            .reserved(&scene.id, false, runtime.social_now())?;
                    }
                }
                Directive::Approach { scene, slots } | Directive::Perform { scene, slots, .. } => {
                    let activity = if matches!(directive, Directive::Approach { .. }) {
                        Activity::MoveTo
                    } else {
                        match scene.kind {
                            SceneKind::Greeting => Activity::Socialize,
                            SceneKind::Play => Activity::Play,
                            SceneKind::Rest => Activity::Rest,
                        }
                    };
                    let needs_reserve = runtime
                        .social
                        .memory
                        .lock()
                        .map_err(|_| ERROR)?
                        .leases
                        .get(&scene.id)
                        .is_none_or(|leases| leases.is_empty());
                    if needs_reserve {
                        match reserve(app, scene, slots) {
                            Ok(leases) => {
                                runtime
                                    .social
                                    .memory
                                    .lock()
                                    .map_err(|_| ERROR)?
                                    .leases
                                    .insert(scene.id.clone(), leases);
                            }
                            Err(_) => {
                                runtime
                                    .social
                                    .memory
                                    .lock()
                                    .map_err(|_| ERROR)?
                                    .coordinator
                                    .cancel(&scene.id, Reason::Unavailable, runtime.social_now())?;
                                continue;
                            }
                        }
                    }
                    for (i, bot) in scene.participants.iter().enumerate() {
                        let lease = runtime
                            .social
                            .memory
                            .lock()
                            .map_err(|_| ERROR)?
                            .leases
                            .get(&scene.id)
                            .and_then(|leases| leases.iter().find(|r| r.bot == *bot))
                            .map(|r| r.lease.clone())
                            .ok_or(ERROR)?;
                        let planned = start_slot_activity(
                            app,
                            bot,
                            activity,
                            slots[i].clone(),
                            Some(lease.clone()),
                        )
                        .await;
                        let old = runtime
                            .social
                            .memory
                            .lock()
                            .map_err(|_| ERROR)?
                            .leases
                            .get(&scene.id)
                            .and_then(|leases| leases.iter().find(|r| r.bot == *bot))
                            .cloned()
                            .ok_or(ERROR)?;
                        release(app, &[old])?;
                        let mut m = runtime.social.memory.lock().map_err(|_| ERROR)?;
                        if let Some(leases) = m.leases.get_mut(&scene.id) {
                            leases.retain(|r| r.bot != *bot);
                        }
                        match planned {
                            Ok(life) => {
                                if let Some(action) = life.current_action() {
                                    m.actions
                                        .entry(scene.id.clone())
                                        .or_default()
                                        .push(action.intent().id().clone());
                                }
                                if m.coordinator
                                    .bind_action(&scene.id, &life, runtime.social_now())
                                    .is_err()
                                {
                                    m.coordinator.cancel(
                                        &scene.id,
                                        Reason::Unavailable,
                                        runtime.social_now(),
                                    )?;
                                }
                            }
                            Err(_) => {
                                m.coordinator.cancel(
                                    &scene.id,
                                    Reason::Unavailable,
                                    runtime.social_now(),
                                )?;
                            }
                        }
                        if m.coordinator
                            .views(bot)
                            .iter()
                            .find(|s| s.id == scene.id)
                            .is_none_or(|s| s.phase.terminal())
                        {
                            break;
                        }
                    }
                }
                Directive::Terminal { scene, event, .. } => {
                    let leases = runtime
                        .social
                        .memory
                        .lock()
                        .map_err(|_| ERROR)?
                        .leases
                        .get(&scene.id)
                        .cloned()
                        .unwrap_or_default();
                    let result = (|| {
                        if event.outcome() != Outcome::Completed {
                            let actions = runtime
                                .social
                                .memory
                                .lock()
                                .map_err(|_| ERROR)?
                                .actions
                                .get(&scene.id)
                                .cloned()
                                .unwrap_or_default();
                            for action in actions {
                                super::cancel_actor_action_exact(
                                    app,
                                    action.actor_id().as_str(),
                                    &action,
                                )?;
                            }
                        }
                        release(app, &leases)?;
                        let changed = state.store.social_apply_event(event)?;
                        state.store.social_save_scene(scene)?;
                        if changed && event.outcome() == Outcome::Completed {
                            for bot in &scene.participants {
                                let fact = ActivityFact {
                                    id: event.id().into(),
                                    actor_ids: scene.participants.to_vec(),
                                    kind: match scene.kind {
                                        SceneKind::Greeting => FactKind::Greeting,
                                        SceneKind::Play => FactKind::Play,
                                        SceneKind::Rest => FactKind::Rest,
                                    },
                                    phase: FactPhase::Completed,
                                    at_ms: event.at_ms(),
                                    source_event_ids: event
                                        .source_events()
                                        .ok_or(ERROR)?
                                        .iter()
                                        .map(|id| {
                                            format!("{}_{}", id.actor_id().as_str(), id.sequence())
                                        })
                                        .collect(),
                                    shared: false,
                                };
                                state.store.record_experience_fact(bot, fact)?;
                            }
                        }
                        Ok::<_, String>(())
                    })();
                    let mut m = runtime.social.memory.lock().map_err(|_| ERROR)?;
                    if result.is_err() {
                        m.terminal.insert(scene.id.clone(), directive.clone());
                        continue;
                    }
                    m.actions.remove(&scene.id);
                    m.leases.remove(&scene.id);
                    m.scopes.remove(&scene.id);
                    m.coordinator.released(&scene.id)?;
                    drop(m);
                    publish(app, &scene.participants);
                }
            }
            // Persist each real protocol boundary so restart never resumes it.
            if let Some(scene) = match &directive {
                Directive::Reserve { scene, .. }
                | Directive::Approach { scene, .. }
                | Directive::Perform { scene, .. } => Some(scene),
                _ => None,
            } {
                state.store.social_save_scene(scene)?;
                publish(app, &scene.participants);
            }
        }
    }
    Ok(())
}
/// Called only by the existing successful native presentation receipt seam.
pub(crate) fn presented(app: &tauri::AppHandle, id: &str, bot: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut m = state.life_runtime.social.memory.lock().map_err(|_| ERROR)?;
    let Some(turn) = m.turns.get(id).filter(|turn| turn.speaker == bot).cloned() else {
        return Ok(());
    };
    let settings = state.store.experience_settings(bot)?;
    if !super::experience_allows_output(app, bot) {
        return Err(ERROR.into());
    }
    m.dialogues
        .presented(&turn, state.life_runtime.social_now(), settings.revision)?;
    m.turns.remove(id);
    Ok(())
}
