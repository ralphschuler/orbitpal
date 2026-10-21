//! Bounded native reconciliation of explicit Life participants. Wakeups and
//! asset readiness schedule work; only the real document/layout/Registry path
//! can establish a surface. No saved output mode or action is changed here.
use super::{
    action_state::LocalFence,
    native_document::{AssetProbeState, DocumentGrant, LoadedDocumentReceipt},
    native_scene::{self, NativeMember, NativeRouteProof},
    session_output::ConstructorReceipt,
};
use crate::{life_domain::MAX_COUNTER, storage::action_store::ProfileToken, world_hub::ActionContext};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, OnceLock},
    time::Duration,
};
use tauri::{Emitter, Manager};

const MAX_MEMBERS: usize = 8;
const ERROR: &str = super::native_scene::ERROR;

#[derive(Clone)]
struct AttemptKey {
    route: String,
    lease: usize,
    signals: usize,
    local: LocalFence,
    world: ActionContext,
    profile: ProfileToken,
    load: u64,
    dialog: u64,
    resume: u64,
}

impl PartialEq for AttemptKey {
    fn eq(&self, other: &Self) -> bool {
        let mut world = other.world.clone(); world.position = self.world.position;
        self.route == other.route && self.lease == other.lease && self.signals == other.signals
            && self.local == other.local && self.world == world && self.profile == other.profile
            && self.load == other.load && self.dialog == other.dialog && self.resume == other.resume
    }
}
impl Eq for AttemptKey {}

/// An immutable worker/Main handoff. The original watch is registered before
/// Core/World/Store waits. Main may inspect atoms_current only; current performs
/// the real checked read-through on a worker, never on the GUI thread.
#[derive(Clone)]
pub(super) struct SceneAttempt {
    constructor: ConstructorReceipt,
    loaded: LoadedDocumentReceipt,
    key: AttemptKey,
    profile: Value,
    avatar_binding: Value,
    ticket: u64,
    cancel: Arc<AtomicBool>,
    internal_cancel: Arc<AtomicBool>,
    operational: Arc<OnceLock<DocumentGrant>>,
    surface: Arc<OnceLock<NativeRouteProof>>,
}
impl SceneAttempt {
    pub(super) fn constructor(&self) -> &ConstructorReceipt { &self.constructor }
    pub(super) fn loaded(&self) -> &LoadedDocumentReceipt { &self.loaded }
    pub(super) fn avatar_binding(&self) -> &Value { &self.avatar_binding }
    pub(super) fn cancel(&self) -> Arc<AtomicBool> { self.cancel.clone() }
    pub(super) fn ticket(&self) -> u64 { self.ticket }
    pub(super) fn atoms_current(&self) -> bool {
        !self.cancel.load(Ordering::SeqCst) && !self.internal_cancel.load(Ordering::SeqCst)
            && self.constructor.lease_current() && self.loaded.current()
    }
    fn retire(&self) {
        // Mark the local cause BEFORE cancelling the shared delivery latch.
        // A failed attempt must not become its own automatic resume event.
        self.internal_cancel.store(true, Ordering::SeqCst);
        self.cancel.store(true, Ordering::SeqCst);
    }
    fn external_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst) && !self.internal_cancel.load(Ordering::SeqCst)
    }
    pub(super) fn current(&self, app: &tauri::AppHandle) -> bool {
        if !self.atoms_current() { return false; }
        let state = app.state::<crate::AppState>();
        if state.updater.ensure_idle().is_err()
            || state.overlay.has_any_pending_speech()
            || !state.life_runtime.session_outputs.constructor_current(&self.constructor)
        { return false; }
        let Ok(core) = state.life_runtime.core.lock() else { return false; };
        let now = state.life_runtime.now();
        if !core.local_current(self.constructor.bot(), &self.key.local, now) { return false; }
        let Ok((mut world, _)) = state.world.registration_context(
            &state.store, &self.key.world.session, &self.key.world.source_world,
        ) else { return false; };
        // Accepted movement may update position, never this admission context.
        world.position = self.key.world.position;
        let matches = world == self.key.world
            && state.store.action_profile_token(self.constructor.bot(), &self.profile).ok() == Some(self.key.profile)
            && self.atoms_current();
        // Store/World may have waited. Re-sample the actual Core clock/fence.
        matches && core.local_current(self.constructor.bot(), &self.key.local, state.life_runtime.now())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase { Loading, Releasing, Preparing, Ready, Unavailable }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Projection {
    pub bot_id: String,
    pub route_id: String,
    pub sequence: u64,
    pub phase: Phase,
}
trait AttemptState {
    fn bot(&self) -> &str;
    fn same_key(&self, other: &Self) -> bool;
    fn retire(&self);
    fn external_cancelled(&self) -> bool;
}
impl AttemptState for SceneAttempt {
    fn bot(&self) -> &str { self.constructor.bot() }
    fn same_key(&self, other: &Self) -> bool { self.key == other.key }
    fn retire(&self) { SceneAttempt::retire(self); }
    fn external_cancelled(&self) -> bool { SceneAttempt::external_cancelled(self) }
}
struct Cell<A> {
    attempt: Arc<A>,
    serial: u64,
    resume: u64,
    external_seen: bool,
    running: bool,
    phase: Phase,
    cleanup_done: bool,
}
struct State<A> {
    serial: u64,
    dialog: u64,
    unavailable: bool,
    cells: BTreeMap<String, Cell<A>>,
}
impl<A> Default for State<A> {
    fn default() -> Self { Self { serial: 0, dialog: 0, unavailable: false, cells: BTreeMap::new() } }
}
impl<A: AttemptState> State<A> {
    fn fail_closed(&mut self) {
        self.unavailable = true;
        for cell in self.cells.values() { cell.attempt.retire(); }
    }
    fn advance(&mut self) -> Option<u64> {
        let Some(next) = self.serial.checked_add(1).filter(|n| *n <= MAX_COUNTER) else {
            self.fail_closed(); return None;
        };
        self.serial = next; Some(next)
    }
    fn admit(&mut self, attempt: Arc<A>, may_begin: bool, resume: u64) -> Option<u64> {
        if self.unavailable { attempt.retire(); return None; }
        let old = self.cells.get(attempt.bot());
        if old.is_some_and(|c| c.attempt.same_key(&attempt) || c.running) || !may_begin
            || (old.is_none() && self.cells.len() >= MAX_MEMBERS)
        { attempt.retire(); return None; }
        let Some(serial) = self.advance() else { attempt.retire(); return None; };
        self.cells.insert(attempt.bot().to_owned(), Cell { attempt, serial, resume,
            external_seen: false, running: true, phase: Phase::Releasing, cleanup_done: false });
        Some(serial)
    }
    fn transition_started(&mut self) {
        let Some(epoch) = self.advance() else { return; };
        self.dialog = epoch;
        for cell in self.cells.values() { cell.attempt.retire(); }
    }
    fn finish(&mut self, bot: &str, serial: u64, phase: Phase) {
        if let Some(cell) = self.cells.get_mut(bot).filter(|c| c.serial == serial) {
            cell.phase = phase; cell.running = false;
            cell.cleanup_done = phase == Phase::Unavailable;
        }
    }
    fn observe_external(&mut self) {
        // The atom is written only by the admitted external OutputGate/system/
        // exact lease, unless retire first marks an internal failure above.
        let bots = self.cells.iter().filter(|(_, c)| !c.running && !c.external_seen && c.attempt.external_cancelled())
            .map(|(bot, _)| bot.clone()).collect::<Vec<_>>();
        for bot in bots {
            let Some(epoch) = self.advance() else { return; };
            if let Some(cell) = self.cells.get_mut(&bot) {
                cell.external_seen = true; cell.resume = epoch;
            }
        }
    }
}
#[derive(Default)]
pub(super) struct Lifecycle { state: Mutex<State<SceneAttempt>>, dirty: AtomicBool }
impl Lifecycle {
    fn lock(&self) -> Option<std::sync::MutexGuard<'_, State<SceneAttempt>>> {
        match self.state.lock() {
            Ok(state) if !state.unavailable => Some(state),
            Ok(_) => None,
            Err(poison) => { poison.into_inner().fail_closed(); None }
        }
    }
    pub(super) fn wake(&self, _bot: Option<&str>) { self.dirty.store(true, Ordering::SeqCst); }
    fn transition_started(&self) {
        if let Some(mut state) = self.lock() { state.transition_started(); }
        self.wake(None);
    }
    /// Called exactly once for a successful native Core claim, before its
    /// monitor watch or worker dispatch. Even a pre-quiesce failure must leave
    /// a new context to reconcile after the real claim is finished.
    pub(super) fn claim_started(&self) { self.transition_started(); }
    /// Called only for an actual generation-owned dialog turn before quiesce.
    /// Polls, failed claim checks and renderer requests cannot advance either
    /// native transition; this scheduling epoch grants no playback or Ready.
    pub(super) fn dialog_started(&self) { self.transition_started(); }
    pub(super) fn cancel(&self) {
        if let Some(mut state) = self.lock() { state.fail_closed(); }
    }
    pub(super) fn snapshot(&self, now: u64) -> Vec<Projection> {
        let cells = self.lock().map(|state| state.cells.iter().map(|(bot, cell)|
            (bot.clone(), cell.serial, cell.phase, cell.attempt.clone())).collect::<Vec<_>>()).unwrap_or_default();
        // No lifecycle guard nests with the retained native owner's locks.
        cells.into_iter().map(|(bot, serial, phase, attempt)| Projection {
            bot_id: bot, route_id: attempt.constructor.route_id().into(), sequence: serial,
            phase: if phase == Phase::Ready && (!attempt.atoms_current()
                || !attempt.surface.get().is_some_and(|proof| proof.live_current(now))) { Phase::Unavailable } else { phase },
        }).collect()
    }
    fn epochs(&self, bot: &str) -> Option<(u64, u64)> {
        let state = self.lock()?;
        Some((state.dialog, state.cells.get(bot).map_or(0, |c| c.resume)))
    }
    fn phase(&self, bot: &str, serial: u64, phase: Phase) {
        if let Some(mut state) = self.lock() {
            if let Some(cell) = state.cells.get_mut(bot).filter(|c| c.serial == serial) { cell.phase = phase; }
        }
    }
    fn finish(&self, bot: &str, serial: u64, phase: Phase) {
        if let Some(mut state) = self.lock() {
            state.finish(bot, serial, phase);
        }
    }
    /// Runs only after the existing real monitor/topology/World synchronization.
    /// No lifecycle lock is held while sampling Core/Store or launching work.
    pub(super) fn pulse(&self, app: &tauri::AppHandle) {
        self.dirty.store(false, Ordering::SeqCst);
        if let Some(mut state) = self.lock() { state.observe_external(); } else { return; }
        let state = app.state::<crate::AppState>();
        let constructors = match state.life_runtime.session_outputs.active_constructors() {
            Ok(receipts) => receipts,
            Err(_) => { self.cancel(); return; }
        };
        let active = constructors.iter().map(|receipt| receipt.bot().to_owned()).collect::<Vec<_>>();
        let ready = self.lock().map(|inner| inner.cells.iter().filter(|(_, c)| !c.running && c.phase == Phase::Ready)
            .map(|(bot, c)| (bot.clone(), c.serial, c.attempt.clone())).collect::<Vec<_>>()).unwrap_or_default();
        for (bot, serial, attempt) in ready {
            if !attempt.operational.get().is_some_and(|grant| attempt.current(app) && observed_live(app, &attempt, grant)) {
                if !attempt.external_cancelled() { attempt.retire(); publish_failure(app, &bot); }
                self.phase(&bot, serial, Phase::Unavailable);
            }
        }
        // Retire bookkeeping only after exact physical cleanup, never merely
        // because a cancelled lease disappeared from the active snapshot.
        let stale = self.lock().map(|inner| inner.cells.iter()
            .filter(|(bot, c)| !active.contains(bot) && !c.running && !c.attempt.constructor.lease_current())
            .map(|(bot, c)| (bot.clone(), c.serial, c.attempt.constructor.clone())).collect::<Vec<_>>()).unwrap_or_default();
        for (bot, serial, receipt) in stale {
            let released = state.life_runtime.native_scenes.for_constructor(&receipt).is_none_or(|m| m.cleanup_released());
            if released {
                if let Some(mut inner) = self.lock() {
                    if inner.cells.get(&bot).is_some_and(|c| c.serial == serial && !c.running) { inner.cells.remove(&bot); }
                }
            }
        }
        let mut candidates = BTreeMap::new();
        for constructor in constructors {
            let Some((dialog, resume)) = self.epochs(constructor.bot()) else { return; };
            if let Some(candidate) = capture(app, constructor, dialog, resume) {
                candidates.insert(candidate.0.constructor.bot().to_owned(), candidate);
            }
        }
        let mut launches = Vec::new();
        let mut cleanups = Vec::new();
        {
            let Some(mut inner) = self.lock() else { return; };
            for (bot, cell) in &mut inner.cells {
                if !candidates.get(bot).is_some_and(|(a, _)| a.key == cell.attempt.key)
                    || (cell.phase == Phase::Unavailable && !cell.attempt.atoms_current()) {
                    cell.attempt.retire();
                    if !cell.running && !cell.cleanup_done {
                        cell.running = true; cell.phase = Phase::Releasing;
                        cleanups.push((cell.attempt.clone(), cell.serial));
                    }
                }
            }
            for (bot, (attempt, may_begin)) in candidates {
                let resume = inner.cells.get(&bot).map_or(0, |c| c.resume);
                if let Some(serial) = inner.admit(attempt.clone(), may_begin, resume) { launches.push((attempt, serial)); }
            }
        }
        for (attempt, serial) in cleanups {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                release_captured(&app, &attempt).await;
                app.state::<crate::AppState>().life_runtime.native_lifecycle.finish(attempt.constructor.bot(), serial, Phase::Unavailable);
            });
        }
        for (attempt, serial) in launches {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { run(app, attempt, serial).await; });
        }
    }
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        let state = self.state.get_mut().unwrap_or_else(|poison| poison.into_inner());
        state.fail_closed();
    }
}

fn capture(app: &tauri::AppHandle, constructor: ConstructorReceipt, dialog: u64, resume: u64)
    -> Option<(Arc<SceneAttempt>, bool)>
{
    let state = app.state::<crate::AppState>();
    // Initial admission only: once attached, other actors' own raw geometry may
    // advance the global ticket without revoking this same delivery watch.
    let ticket = state.life_runtime.outputs.ticket()?;
    let cancel = Arc::new(AtomicBool::new(false));
    if !state.life_runtime.outputs.watch_delivery(ticket, constructor.bot(), constructor.window().label(), &cancel)
        || state.life_runtime.monitor.get()?.gate.watch(&cancel).is_none()
        || state.life_runtime.session_outputs.bind_surface(&constructor, &cancel).is_err()
    { cancel.store(true, Ordering::SeqCst); return None; }
    let loaded = state.life_runtime.native_documents.loaded(constructor.bot(), constructor.signals()).ok()?;
    let core = state.life_runtime.core.lock().ok()?;
    let now = state.life_runtime.now();
    let local = core.local_fence(constructor.bot(), now)?;
    let world = state.world.action_context(&state.store, &crate::world_hub::current_actors(&core), constructor.bot(), now).ok()?;
    state.world.native_output_geometry(&state.store, &world).ok()?;
    let profile = state.store.bot_profile(constructor.bot()).ok()?;
    let profile_token = state.store.action_profile_token(constructor.bot(), &profile).ok()?;
    let may_begin = core.local_action_reason(constructor.bot(), state.life_runtime.now()).is_none()
        && state.generation.try_lock().is_ok() && !state.overlay.has_any_pending_speech()
        && state.updater.ensure_idle().is_ok();
    let key = AttemptKey { route: constructor.route_id().into(), lease: Arc::as_ptr(&constructor.lease_cancel()) as usize,
        signals: Arc::as_ptr(constructor.signals()) as usize, local, world, profile: profile_token,
        load: loaded.load_id(), dialog, resume };
    let attempt = Arc::new(SceneAttempt { avatar_binding: crate::emote_capabilities::binding(&profile),
        constructor, loaded, key, profile, ticket, cancel, internal_cancel: Arc::new(AtomicBool::new(false)), operational: Arc::new(OnceLock::new()), surface: Arc::new(OnceLock::new()) });
    drop(core);
    attempt.current(app).then_some((attempt, may_begin))
}

async fn release_members(app: &tauri::AppHandle, members: Vec<Arc<NativeMember>>) -> Result<(), String> {
    for member in members { native_scene::release_exact(app, member).await?; }
    Ok(())
}
async fn release_captured(app: &tauri::AppHandle, attempt: &SceneAttempt) {
    let members = app.state::<crate::AppState>().life_runtime.native_scenes.members(Some(attempt.constructor.bot()))
        .into_iter().filter(|m| m.constructor.same_constructor(&attempt.constructor)).collect();
    let _ = release_members(app, members).await;
}

struct Worker {
    app: tauri::AppHandle, attempt: Arc<SceneAttempt>, serial: u64, completed: bool,
}
impl Worker {
    fn finish(&mut self, phase: Phase) {
        self.app.state::<crate::AppState>().life_runtime.native_lifecycle.finish(self.attempt.constructor.bot(), self.serial, phase);
        self.completed = true;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if self.completed { return; }
        self.attempt.retire();
        let app = self.app.clone(); let attempt = self.attempt.clone(); let serial = self.serial;
        // Cleanup retains the cell's running charge even if this worker future
        // is dropped; a successor cannot rotate while old physical work remains.
        tauri::async_runtime::spawn(async move {
            release_captured(&app, &attempt).await;
            app.state::<crate::AppState>().life_runtime.native_lifecycle.finish(attempt.constructor.bot(), serial, Phase::Unavailable);
        });
    }
}
fn publish_failure(app: &tauri::AppHandle, bot: &str) {
    let _ = app.emit_to("main", "cohost://agent", serde_json::json!({"kind":"error","botId":bot,"text":ERROR}));
}
async fn run(app: tauri::AppHandle, attempt: Arc<SceneAttempt>, serial: u64) {
    let mut guard = Worker { app: app.clone(), attempt: attempt.clone(), serial, completed: false };
    let outcome = run_inner(&app, &attempt, serial).await;
    if outcome.is_ok() { guard.finish(Phase::Ready); return; }
    if !matches!(outcome, Err(AttemptFailure::External)) { attempt.retire(); publish_failure(&app, attempt.constructor.bot()); }
    release_captured(&app, &attempt).await;
    guard.finish(Phase::Unavailable);
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttemptFailure { External, Failed }
impl From<String> for AttemptFailure { fn from(_: String) -> Self { Self::Failed } }
impl From<&str> for AttemptFailure { fn from(_: &str) -> Self { Self::Failed } }
fn check_current(app: &tauri::AppHandle, attempt: &SceneAttempt) -> Result<(), AttemptFailure> {
    if attempt.current(app) { Ok(()) }
    else if attempt.external_cancelled() { Err(AttemptFailure::External) }
    else { Err(AttemptFailure::Failed) }
}
async fn run_inner(app: &tauri::AppHandle, attempt: &Arc<SceneAttempt>, serial: u64) -> Result<(), AttemptFailure> {
    let state = app.state::<crate::AppState>();
    let runtime = &state.life_runtime;
    // Capture the exact old members before awaiting, never look up a successor
    // by label during cleanup. New native registration is owned by this worker.
    let old = runtime.native_scenes.members(Some(attempt.constructor.bot()));
    release_members(app, old).await?;
    check_current(app, attempt)?;
    runtime.native_lifecycle.phase(attempt.constructor.bot(), serial, Phase::Loading);
    let probe = runtime.native_documents.begin_probe(&attempt.loaded, attempt.avatar_binding.clone(),
        attempt.cancel.clone(), attempt.constructor.lease_cancel(), runtime.now()).map_err(|_| ERROR)?;
    super::native_scene_document::deliver_probe(app, attempt, &probe).await?;
    for _ in 0..650 {
        check_current(app, attempt)?;
        match runtime.native_documents.probe_status(&probe, runtime.now()).map_err(|_| ERROR)? {
            AssetProbeState::Ready => break,
            AssetProbeState::Failed => return Err(ERROR.into()),
            _ => { tokio::time::sleep(Duration::from_millis(100)).await; continue; }
        }
    }
    check_current(app, attempt)?;
    let ready = runtime.native_documents.consume_ready(&probe, runtime.now()).map_err(|_| ERROR)?;
    // A later retained cleanup may have appeared while loading. The exact idle
    // Scene->Documents boundary below independently denies that replacement.
    check_current(app, attempt)?;
    let grant = runtime.native_scenes.rotate_for_attempt(&runtime.native_documents,
        &attempt.constructor, ready, runtime.now())?;
    attempt.operational.set(grant.clone()).map_err(|_| ERROR)?;
    runtime.native_lifecycle.phase(attempt.constructor.bot(), serial, Phase::Preparing);
    super::native_scene_document::bootstrap_attempt(app, attempt, grant.clone()).await?;
    await_live(app, attempt, &grant).await
}
fn observed_live(app: &tauri::AppHandle, attempt: &SceneAttempt, grant: &DocumentGrant) -> bool {
    let state = app.state::<crate::AppState>();
    let Some(member) = state.life_runtime.native_scenes.for_constructor(&attempt.constructor) else { return false; };
    if !member.current() || !member.auth.document().same_document(&grant.proof()) { return false; }
    let Ok(owned) = member.owned.lock() else { return false; };
    let Some(owned) = owned.as_ref() else { return false; };
    if !owned.route.live_current(state.life_runtime.now()) { return false; }
    let Ok(core) = state.life_runtime.core.lock() else { return false; };
    let now = state.life_runtime.now();
    let ready = attempt.atoms_current() && core.local_current(attempt.constructor.bot(), &attempt.key.local, now)
        && core.world_outputs.source(attempt.constructor.bot()).is_some_and(|source|
            matches!(&source.route, crate::world_output::SourceRoute::Native(route) if route.matches_auth(&member.auth)))
        && core.world_outputs.status(attempt.constructor.bot(), now).is_some_and(|output|
            matches!(output.readiness, crate::world_output::Readiness::Ready { .. }))
        && owned.route.live_current(state.life_runtime.now()) && attempt.atoms_current();
    if ready { let _ = attempt.surface.set(owned.route.clone()); }
    ready
}
async fn await_live(app: &tauri::AppHandle, attempt: &SceneAttempt, grant: &DocumentGrant) -> Result<(), AttemptFailure> {
    let runtime = &app.state::<crate::AppState>().life_runtime;
    // Observation only: first auth has <5s and the independent actual layout D5
    // has <=5s. This outer bound cannot publish or extend either native proof.
    let deadline = grant.issued_at().checked_add(10_000).filter(|n| *n <= MAX_COUNTER).ok_or(ERROR)?;
    for _ in 0..200 {
        check_current(app, attempt)?;
        if runtime.now() > deadline { return Err(ERROR.into()); }
        if observed_live(app, attempt, grant) { return Ok(()); }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(ERROR.into())
}

#[cfg(test)]
#[path = "native_scene_lifecycle_tests.rs"]
mod tests;
