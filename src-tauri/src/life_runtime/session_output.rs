//! Session-only presentation of explicit Life participants. This reuses the
//! ordinary native overlay and its playback gate; it grants no World motion.
use crate::window_signals::WindowSignals;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, Weak},
};
use tauri::{Emitter, Manager};

const ERROR: &str = "Die Life-Figur konnte nicht im Desktop-Fenster angezeigt werden.";
const MAX_ROUTES: usize = 64;
const MAX_SEQUENCE: u64 = 9_007_199_254_740_991;

#[derive(Clone)]
pub(crate) struct Lease {
    pub(crate) bot: String,
    pub(crate) id: String,
    cancel: Arc<AtomicBool>,
}
impl Lease {
    pub(crate) fn current(&self) -> bool { !self.cancel.load(Ordering::SeqCst) }
    pub(crate) fn revoke(&self) { self.cancel.store(true, Ordering::SeqCst); }
}
pub(crate) struct StartGuard { app: tauri::AppHandle, lease: Lease, armed: bool }
impl StartGuard {
    pub(crate) fn new(app: &tauri::AppHandle, lease: Lease, armed: bool) -> Self { Self { app: app.clone(), lease, armed } }
    pub(crate) fn complete(&mut self) { self.armed = false; }
}
impl Drop for StartGuard {
    fn drop(&mut self) {
        if self.armed { self.app.state::<crate::AppState>().life_runtime.session_outputs.cancel_exact(&self.app, &self.lease); }
    }
}
#[derive(Clone)]
struct Target {
    window: tauri::WebviewWindow,
    signals: Arc<WindowSignals>,
    created: bool,
    was_visible: bool,
}
/// Captured by native code only. A label or WebviewWindow equality is not
/// constructor identity: the unforgeable signals Arc and exact lease are both
/// required, and the retained window is never reacquired by label for its work.
#[derive(Clone)]
pub(crate) struct ConstructorReceipt {
    lease: Lease,
    window: tauri::WebviewWindow,
    signals: Arc<WindowSignals>,
    created_for_session: bool,
}
impl ConstructorReceipt {
    pub(super) fn bot(&self) -> &str { &self.lease.bot }
    pub(super) fn route_id(&self) -> &str { &self.lease.id }
    pub(super) fn lease_current(&self) -> bool { self.lease.current() }
    /// Read-only cancellation identity of the exact native session route.
    /// A document delivered for this lease cannot adopt a later same-window
    /// Start. This conveys no presentation, rendering or action authority.
    pub(super) fn lease_cancel(&self) -> Arc<AtomicBool> { self.lease.cancel.clone() }
    pub(super) fn signals(&self) -> &Arc<WindowSignals> { &self.signals }
    pub(super) fn persists_fixed_bounds(&self) -> bool { !self.created_for_session }
    pub(super) fn window(&self) -> &tauri::WebviewWindow { &self.window }
    pub(super) fn same_constructor(&self, other: &Self) -> bool {
        self.lease.bot == other.lease.bot && self.lease.id == other.lease.id
            && Arc::ptr_eq(&self.lease.cancel, &other.lease.cancel)
            && Arc::ptr_eq(&self.signals, &other.signals)
    }
}
struct Entry {
    lease: Lease,
    sequence: u64,
    state: &'static str,
    opening: bool,
    target: Option<Target>,
    jobs: Vec<Weak<AtomicBool>>,
    cleanup_queued: bool,
}
impl Entry {
    fn revoke(&self) {
        self.lease.revoke();
        for job in &self.jobs { if let Some(job) = job.upgrade() { job.store(true, Ordering::SeqCst); } }
    }
}
#[derive(Default)]
struct State {
    sequence: u64,
    entries: HashMap<String, Entry>,
    starts: Vec<(Vec<String>, Weak<AtomicBool>)>,
    stop_all: u64,
    stop_bots: HashMap<String, u64>,
}
#[derive(Clone, Copy)]
pub(crate) struct StartFence { all: u64, bot: u64 }
impl State {
    fn advance(&mut self) -> Result<u64, String> {
        if self.sequence >= MAX_SEQUENCE {
            for entry in self.entries.values() { entry.revoke(); }
            for (_, start) in &self.starts { if let Some(start) = start.upgrade() { start.store(true, Ordering::SeqCst); } }
            return Err(ERROR.into());
        }
        self.sequence += 1;
        Ok(self.sequence)
    }
}
#[derive(Default)]
pub(crate) struct SessionOutputs { state: Mutex<State> }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    version: u8,
    bot_id: String,
    sequence: u64,
    route_id: Option<String>,
    output: Option<&'static str>,
    state: &'static str,
    reason: Option<&'static str>,
}
impl SessionOutputs {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, String> {
        self.state.lock().map_err(|poison| {
            let state = poison.into_inner();
            for entry in state.entries.values() { entry.revoke(); }
            for (_, start) in &state.starts { if let Some(start) = start.upgrade() { start.store(true, Ordering::SeqCst); } }
            ERROR.to_owned()
        })
    }
    fn projection(state: &State, bot: &str) -> Projection {
        let entry = state.entries.get(bot);
        let active = entry.filter(|entry| entry.lease.current() || entry.state == "unavailable");
        Projection {
            version: 1, bot_id: bot.into(),
            sequence: entry.map_or(state.sequence, |entry| entry.sequence),
            route_id: active.map(|entry| entry.lease.id.clone()),
            output: active.map(|_| "overlay"),
            state: active.map_or("inactive", |entry| entry.state),
            reason: active.filter(|entry| entry.state == "unavailable").map(|_| "window_unavailable"),
        }
    }
    pub(crate) fn snapshot(&self, bot: &str) -> Result<Projection, String> {
        let state = self.lock()?;
        Ok(Self::projection(&state, bot))
    }
    pub(crate) fn publish(&self, app: &tauri::AppHandle, bot: &str) {
        if let Ok(projection) = self.snapshot(bot) {
            let _ = app.emit("cohost://life-output", &projection);
            let _ = app.state::<crate::AppState>().overlay.sender.send(json!({"kind":"life-output","botId":bot,"lifeOutput":projection}));
        }
    }
    fn matches(entry: &Entry, lease: &Lease) -> bool {
        entry.lease.id == lease.id && Arc::ptr_eq(&entry.lease.cancel, &lease.cancel)
    }
    pub(crate) fn begin(&self, bot: &str) -> Result<(Lease, bool), String> {
        self.begin_at(bot, None)
    }
    pub(crate) fn start_fence(&self, bot: &str) -> Result<StartFence, String> {
        let state = self.lock()?;
        if state.sequence >= MAX_SEQUENCE { return Err(ERROR.into()); }
        Ok(StartFence { all: state.stop_all, bot: state.stop_bots.get(bot).copied().unwrap_or(0) })
    }
    pub(crate) fn start_current(&self, bot: &str, fence: &StartFence) -> bool {
        self.lock().is_ok_and(|state| state.sequence < MAX_SEQUENCE && state.stop_all == fence.all && state.stop_bots.get(bot).copied().unwrap_or(0) == fence.bot)
    }
    pub(crate) fn begin_for_start(&self, bot: &str, fence: &StartFence) -> Result<(Lease, bool), String> {
        self.begin_at(bot, Some(fence))
    }
    fn begin_at(&self, bot: &str, fence: Option<&StartFence>) -> Result<(Lease, bool), String> {
        let mut state = self.lock()?;
        if fence.is_some_and(|fence| state.stop_all != fence.all || state.stop_bots.get(bot).copied().unwrap_or(0) != fence.bot) { return Err(ERROR.into()); }
        if let Some(entry) = state.entries.get(bot) {
            if entry.lease.current() && entry.state == "visible" { return Ok((entry.lease.clone(), false)); }
            return Err(ERROR.into());
        }
        if state.entries.len() >= MAX_ROUTES { return Err(ERROR.into()); }
        let sequence = state.advance()?;
        let lease = Lease { bot: bot.into(), id: uuid::Uuid::new_v4().to_string(), cancel: Arc::new(AtomicBool::new(false)) };
        state.entries.insert(bot.into(), Entry { lease: lease.clone(), sequence, state: "opening", opening: true, target: None, jobs: Vec::new(), cleanup_queued: false });
        Ok((lease, true))
    }
    pub(crate) fn attach(&self, lease: &Lease, window: tauri::WebviewWindow, signals: Arc<WindowSignals>, created: bool, was_visible: bool) -> bool {
        let Ok(mut state) = self.lock() else { lease.cancel.store(true, Ordering::SeqCst); return false; };
        let Some(entry) = state.entries.get_mut(&lease.bot).filter(|entry| Self::matches(entry, lease)) else { return false; };
        entry.target = Some(Target { window, signals, created, was_visible });
        lease.current()
    }
    pub(crate) fn finish_open(&self, app: &tauri::AppHandle, lease: &Lease, visible: bool) {
        let cleanup = {
            let Ok(mut state) = self.lock() else { lease.cancel.store(true, Ordering::SeqCst); return; };
            let Ok(sequence) = state.advance() else { return; };
            let Some(entry) = state.entries.get_mut(&lease.bot).filter(|entry| Self::matches(entry, lease)) else { return; };
            entry.opening = false; entry.sequence = sequence;
            if lease.current() { entry.state = if visible { "visible" } else { "unavailable" }; }
            !lease.current()
        };
        self.publish(app, &lease.bot);
        if cleanup { self.cleanup(app, lease.clone()); }
    }
    /// Some(false) deliberately prevents fallback to the saved Studio route.
    pub(crate) fn available(&self, bot: &str) -> Option<bool> {
        let Ok(state) = self.lock() else { return Some(false); };
        state.entries.get(bot).map(|entry| entry.lease.current() && entry.state == "visible")
    }
    pub(crate) fn resolve(&self, bot: &str) -> Result<Option<Lease>, String> {
        let state = self.lock()?;
        match state.entries.get(bot) {
            None => Ok(None),
            Some(entry) if entry.lease.current() && entry.state == "visible" => Ok(Some(entry.lease.clone())),
            Some(_) => Err(ERROR.into()),
        }
    }
    /// Native-only snapshot of the exact currently visible session constructors.
    /// This observes retained identity only; actual GUI visibility is checked on
    /// Main by delivery and layout, and cannot be inferred from this snapshot.
    pub(super) fn active_constructors(&self) -> Result<Vec<ConstructorReceipt>, String> {
        let state = self.lock()?;
        let mut receipts = state.entries.values().filter(|entry|
            entry.lease.current() && entry.state == "visible" && !entry.opening && !entry.cleanup_queued)
            .filter_map(|entry| entry.target.as_ref().map(|target| ConstructorReceipt {
                lease: entry.lease.clone(), window: target.window.clone(), signals: target.signals.clone(),
                created_for_session: target.created,
            })).collect::<Vec<_>>();
        receipts.sort_by(|a, b| a.bot().cmp(b.bot()));
        Ok(receipts)
    }
    /// Exact retained targets, including cancelled/loading/restoring ones.
    /// This is negative evidence only: it cannot imply visibility or Ready.
    pub(super) fn retained_constructors(&self) -> Result<Vec<ConstructorReceipt>, String> {
        let state = self.lock()?;
        let mut receipts = state.entries.values().filter_map(|entry| entry.target.as_ref().map(|target| ConstructorReceipt {
            lease: entry.lease.clone(), window: target.window.clone(), signals: target.signals.clone(),
            created_for_session: target.created,
        })).collect::<Vec<_>>();
        receipts.sort_by(|a, b| a.bot().cmp(b.bot()));
        Ok(receipts)
    }
    pub(super) fn has_retained_constructor(&self, bot: &str) -> bool {
        self.lock().map(|state| state.entries.get(bot).is_some_and(|entry| entry.target.is_some())).unwrap_or(true)
    }
    /// Core is already held. Keep this short SessionOutputs guard until the
    /// Studio Registry mutation returns, so a target cannot attach between the
    /// negative constructor check and Studio admission. No GUI/await/callback
    /// into SessionOutputs is permitted in create; it only mutates Registry.
    pub(super) fn admit_studio_world_registration<T>(
        &self, bot: &str,
        create: impl FnOnce() -> Result<T, crate::world_output::RenderReason>,
    ) -> Result<T, crate::world_output::RenderReason> {
        use crate::world_output::RenderReason;
        let state = self.lock().map_err(|_| RenderReason::Cancelled)?;
        if state.entries.get(bot).is_some_and(|entry| entry.target.is_some()) {
            return Err(RenderReason::ScopeMismatch);
        }
        create()
    }
    pub(super) fn constructor_for(&self, lease: &Lease) -> Result<ConstructorReceipt, String> {
        let state = self.lock()?;
        let entry = state.entries.get(&lease.bot).filter(|entry|
            Self::matches(entry, lease) && entry.lease.current() && entry.state == "visible"
                && !entry.opening && !entry.cleanup_queued).ok_or(ERROR)?;
        let target = entry.target.as_ref().ok_or(ERROR)?;
        Ok(ConstructorReceipt { lease: lease.clone(), window: target.window.clone(), signals: target.signals.clone(), created_for_session: target.created })
    }
    pub(super) fn constructor_current(&self, receipt: &ConstructorReceipt) -> bool {
        self.lock().is_ok_and(|state| state.entries.get(receipt.bot()).is_some_and(|entry|
            Self::matches(entry, &receipt.lease) && entry.lease.current() && entry.state == "visible"
                && !entry.opening && !entry.cleanup_queued
                && entry.target.as_ref().is_some_and(|target| Arc::ptr_eq(&target.signals, &receipt.signals))))
    }
    /// The same physical cancellation atom is revoked on session replacement,
    /// Stop and destruction; adding a watcher cannot revive an obsolete target.
    pub(super) fn bind_surface(&self, receipt: &ConstructorReceipt, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let mut state = self.lock()?;
        let entry = state.entries.get_mut(receipt.bot()).filter(|entry|
            Self::matches(entry, &receipt.lease) && entry.lease.current() && entry.state == "visible"
                && !entry.opening && !entry.cleanup_queued
                && entry.target.as_ref().is_some_and(|target| Arc::ptr_eq(&target.signals, &receipt.signals))).ok_or(ERROR)?;
        entry.jobs.retain(|job| job.strong_count() > 0);
        if !entry.jobs.iter().any(|job| job.upgrade().is_some_and(|job| Arc::ptr_eq(&job, cancel))) {
            if entry.jobs.len() >= 64 { return Err(ERROR.into()); }
            entry.jobs.push(Arc::downgrade(cancel));
        }
        if cancel.load(Ordering::SeqCst) { return Err(ERROR.into()); }
        Ok(())
    }
    pub(crate) fn lease_for_stop(&self, bot: &str) -> Option<Lease> {
        self.lock().ok()?.entries.get(bot).map(|entry| entry.lease.clone())
    }
    pub(crate) fn owns(&self, lease: &Lease) -> bool {
        self.lock().is_ok_and(|state| state.entries.get(&lease.bot).is_some_and(|entry| Self::matches(entry, lease)))
    }
    pub(crate) fn adopt(&self, lease: &Lease, rotate: bool) -> Result<Lease, String> {
        let mut state = self.lock()?;
        if !state.entries.get(&lease.bot).is_some_and(|entry| Self::matches(entry,lease)&&entry.lease.current()&&entry.state=="visible"&&!entry.cleanup_queued) { return Err(ERROR.into()); }
        if !rotate { return Ok(lease.clone()); }
        let sequence=state.advance()?;
        let entry=state.entries.get_mut(&lease.bot).ok_or(ERROR)?;
        entry.revoke();entry.jobs.clear();
        let successor=Lease {bot:lease.bot.clone(),id:uuid::Uuid::new_v4().to_string(),cancel:Arc::new(AtomicBool::new(false))};
        entry.lease=successor.clone();entry.sequence=sequence;
        Ok(successor)
    }
    pub(crate) fn has_route(&self, bot: &str) -> bool {
        self.lock().map(|state| state.entries.get(bot).is_some_and(|entry| entry.lease.current())).unwrap_or(false)
    }
    pub(crate) fn bind_job(&self, lease: &Lease, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let mut state = self.lock()?;
        let entry = state.entries.get_mut(&lease.bot).filter(|entry| Self::matches(entry, lease) && entry.lease.current() && entry.state == "visible").ok_or(ERROR)?;
        entry.jobs.retain(|job| job.strong_count() > 0);
        if !entry.jobs.iter().any(|job| job.upgrade().is_some_and(|job| Arc::ptr_eq(&job, cancel))) {
            if entry.jobs.len() >= 64 { return Err(ERROR.into()); }
            entry.jobs.push(Arc::downgrade(cancel));
        }
        Ok(())
    }
    pub(crate) fn begin_group(&self, bots: Vec<String>) -> Result<Arc<AtomicBool>, String> {
        let mut state = self.lock()?;
        state.starts.retain(|(_, start)| start.strong_count() > 0);
        if state.starts.len() >= 8 { return Err(ERROR.into()); }
        let cancel = Arc::new(AtomicBool::new(false));
        state.starts.push((bots, Arc::downgrade(&cancel)));
        Ok(cancel)
    }
    pub(crate) fn cancel(&self, app: &tauri::AppHandle, bot: Option<&str>) {
        self.finish_cancel(app, self.cancel_routes(bot));
    }
    pub(crate) fn finish_cancel(&self, app: &tauri::AppHandle, leases: Vec<Lease>) {
        for lease in leases { self.publish(app, &lease.bot); self.cleanup(app, lease); }
    }
    pub(crate) fn cancel_routes(&self, bot: Option<&str>) -> Vec<Lease> {
        {
            let Ok(mut state) = self.lock() else { return Vec::new(); };
            for (bots, start) in &state.starts {
                if bot.is_none_or(|bot| bots.iter().any(|candidate| candidate == bot)) {
                    if let Some(start) = start.upgrade() { start.store(true, Ordering::SeqCst); }
                }
            }
            let sequence = state.advance().unwrap_or(MAX_SEQUENCE);
            if let Some(bot) = bot {
                if state.stop_bots.len() >= MAX_ROUTES && !state.stop_bots.contains_key(bot) {
                    state.stop_all = sequence; state.stop_bots.clear();
                }
                state.stop_bots.insert(bot.to_owned(), sequence);
            } else { state.stop_all = sequence; state.stop_bots.clear(); }
            state.entries.values_mut().filter(|entry| bot.is_none_or(|bot| entry.lease.bot == bot)).map(|entry| {
                entry.revoke(); entry.state = "inactive"; entry.sequence = sequence;
                entry.lease.clone()
            }).collect::<Vec<_>>()
        }
    }
    pub(crate) fn cancel_exact(&self, app: &tauri::AppHandle, lease: &Lease) {
        {
            let Ok(mut state) = self.lock() else { lease.cancel.store(true, Ordering::SeqCst); return; };
            let sequence = state.advance().unwrap_or(MAX_SEQUENCE);
            let Some(entry) = state.entries.get_mut(&lease.bot).filter(|entry| Self::matches(entry, lease)) else { return; };
            entry.revoke(); entry.state = "inactive"; entry.sequence = sequence;
        }
        self.publish(app, &lease.bot); self.cleanup(app, lease.clone());
    }
    fn cleanup(&self, app: &tauri::AppHandle, lease: Lease) {
        let target = {
            let Ok(mut state) = self.lock() else { return; };
            let Some(entry) = state.entries.get_mut(&lease.bot).filter(|entry| Self::matches(entry, &lease)) else { return; };
            if entry.opening || entry.cleanup_queued { return; } // One actual closure owns late cleanup.
            let target = entry.target.clone();
            if target.is_some() { entry.cleanup_queued = true; }
            if target.is_none() { state.entries.remove(&lease.bot); }
            target
        };
        let Some(target) = target else { return; };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state=app.state::<crate::AppState>();
            let receipt=ConstructorReceipt{lease:lease.clone(),window:target.window.clone(),signals:target.signals.clone(),created_for_session:target.created};
            if let Some(member)=state.life_runtime.native_scenes.for_constructor(&receipt){
                if super::native_scene::release_exact(&app,member).await.is_err(){
                    state.life_runtime.session_outputs.cleanup_unavailable(&app,&lease);return;
                }
            }
            if state.presentation.current_window(&lease.bot,&target.signals)&&state.presentation.native_guarded(&lease.bot){
                state.life_runtime.session_outputs.cleanup_unavailable(&app,&lease);return;
            }
            let main = app.clone();
            let _ = app.run_on_main_thread(move || {
                let state = main.state::<crate::AppState>();
                if !state.life_runtime.session_outputs.cleanup_current(&lease) { return; }
                if !state.presentation.current_window(&lease.bot, &target.signals) {
                    state.life_runtime.session_outputs.retire(&main, &lease); return;
                }
                // User-selected legacy overlay ownership survives session Stop.
                let configured = state.store.bot_profile(&lease.bot).is_ok_and(|profile|
                    profile["output"]["mode"] == "overlay" && profile["output"]["streamingEnabled"] == true);
                if target.created && !configured {
                    // Retain the record until this exact Destroyed callback.
                    let _ = target.window.close();
                } else {
                    crate::overlay_presentation::sync_or_report(&main, &lease.bot);
                    if !target.was_visible && !configured { let _ = target.window.hide(); }
                    state.life_runtime.session_outputs.retire(&main, &lease);
                }
            });
        });
    }
    fn cleanup_unavailable(&self,app:&tauri::AppHandle,lease:&Lease){
        if let Ok(mut state)=self.lock(){
            let sequence=state.advance().unwrap_or(MAX_SEQUENCE);
            if let Some(entry)=state.entries.get_mut(&lease.bot).filter(|entry|Self::matches(entry,lease)){
                entry.state="unavailable";entry.sequence=sequence;
            }
        }
        self.publish(app,&lease.bot);
    }
    fn cleanup_current(&self, lease: &Lease) -> bool {
        self.lock().is_ok_and(|state| state.entries.get(&lease.bot).is_some_and(|entry|
            Self::matches(entry, lease) && !entry.lease.current() && entry.cleanup_queued && !entry.opening))
    }
    fn retire(&self, app: &tauri::AppHandle, lease: &Lease) {
        if let Ok(mut state) = self.lock() {
            if state.entries.get(&lease.bot).is_some_and(|entry| Self::matches(entry, lease)) {
                state.entries.remove(&lease.bot); let _ = state.advance();
            }
        }
        self.publish(app, &lease.bot);
    }
    pub(crate) fn destroyed(&self, app: &tauri::AppHandle, bot: &str, signals: &Arc<WindowSignals>) {
        let lease = {
            let Ok(mut state) = self.lock() else { return; };
            let sequence = state.advance().unwrap_or(MAX_SEQUENCE);
            let Some(entry) = state.entries.get_mut(bot).filter(|entry| entry.target.as_ref().is_some_and(|target| Arc::ptr_eq(&target.signals, signals))) else { return; };
            entry.sequence = sequence;
            if entry.lease.current() { entry.revoke(); entry.state = "unavailable"; None } else { Some(entry.lease.clone()) }
        };
        if let Some(lease) = lease { self.retire(app, &lease); } else { self.publish(app, bot); }
    }
}

pub(crate) async fn open(app: &tauri::AppHandle, lease: Lease) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let main = app.clone(); let actual = lease.clone();
    let queued = app.run_on_main_thread(move || {
        let result = if actual.current() { crate::overlay::open_session_for_bot(&main, &actual) } else { Err(ERROR.into()) };
        main.state::<crate::AppState>().life_runtime.session_outputs.finish_open(&main, &actual, result.is_ok());
        let _ = tx.send(result);
    });
    if queued.is_err() {
        app.state::<crate::AppState>().life_runtime.session_outputs.finish_open(app, &lease, false);
        return Err(ERROR.into());
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await.map_err(|_| ERROR)?.map_err(|_| ERROR)?
}

struct SceneStartGuard {
    app: tauri::AppHandle,
    cancel: Arc<AtomicBool>,
    started: Vec<(String, String, Lease)>,
    added_routes: Vec<Lease>,
    complete: bool,
}
/// Actual group rollback seam, independent of GUI scheduling and used by the
/// production guard. Core serialization protects the exact session comparison.
pub(super) fn rollback_core(
    core: &mut super::Core, store: &crate::storage::Store, world: &crate::world_hub::WorldHub,
    autopilot: &crate::autopilot::AutopilotHub, bot: &str, expected: &str, outputs: &SessionOutputs, lease: &Lease, now: u64,
) -> (bool, Option<crate::autopilot::Runtime>) {
    if !outputs.owns(lease) { return (false,None); }
    let Some(removed) = core.stop_exact(bot, expected, None, now) else { return (false, None); };
    lease.revoke();
    let config = store.bot_profile(bot).ok().and_then(|profile| crate::autopilot::config(&profile).ok()).unwrap_or_default();
    let runtime = autopilot.set(bot, false, config).ok();
    super::cancel_saved_action(store, bot, removed.state.as_ref());
    world.revoke_actor(store, bot);
    let _ = super::action_host::poll(core, store, world, || now, super::wall_ms);
    (true, runtime)
}
/// A newer pending start may be using this same visible lease before adoption
/// rotates its identity. Keep the Core guard through this revocation decision.
pub(super) fn revoke_unclaimed_route(core: &super::Core, outputs: &SessionOutputs, lease: &Lease) -> bool {
    if core.has_pending_start(&lease.bot) || !outputs.owns(lease) { return false; }
    lease.revoke();
    true
}
impl Drop for SceneStartGuard {
    fn drop(&mut self) {
        if self.complete { return; }
        self.cancel.store(true, Ordering::SeqCst);
        let state = self.app.state::<crate::AppState>();
        for (bot, session, lease) in self.started.iter().rev() {
            state.life_runtime.rollback_scene_session(&self.app, bot, session, lease);
        }
        for lease in &self.added_routes { state.life_runtime.cancel_unclaimed_scene_route(&self.app, lease); }
    }
}
pub(crate) async fn start_scene(app: &tauri::AppHandle, expected_revision: Option<u64>) -> Result<super::RuntimeSnapshot, String> {
    let state = app.state::<crate::AppState>();
    state.updater.ensure_idle()?;
    let settings = serde_json::to_value(state.world.settings(&state.store)).map_err(|_| ERROR)?;
    if settings["availability"] != "ready" { return Err("Die gemeinsame Bühne ist vorübergehend nicht verfügbar.".into()); }
    let config = &settings["record"]["config"];
    let revision = config["revision"].as_u64().ok_or(ERROR)?;
    if expected_revision.is_some_and(|expected| expected != revision) { return Err("Die Bühne wurde inzwischen geändert. Bitte neu laden.".into()); }
    let members = scene_members(config)?;
    for bot in &members {
        state.store.bot_profile(bot)?;
    }
    let before = {
        let core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
        let mut sessions = HashMap::new();
        for bot in &members {
            if let Some(session) = core.sessions.get(bot) {
                if session.mode != super::Mode::Life { return Err("Bitte die laufende Antwort oder Aktualisierung vor dem Start von Life beenden.".into()); }
                sessions.insert(bot.clone(), session.session_id.clone());
            }
        }
        sessions
    };
    let cancel = state.life_runtime.session_outputs.begin_group(members.clone())?;
    let mut guard = SceneStartGuard { app: app.clone(), cancel: cancel.clone(), started: Vec::new(), added_routes: Vec::new(), complete: false };
    for bot in &members {
        if cancel.load(Ordering::SeqCst) { return Err("Der Start von Life wurde abgebrochen.".into()); }
        // Recheck the saved scene before each member; no selection/profile
        // writes and no snapshot invented from a partially applied editor.
        let current = serde_json::to_value(state.world.settings(&state.store)).map_err(|_| ERROR)?;
        if current["instanceId"] != settings["instanceId"] || current["record"] != settings["record"] {
            return Err("Die Bühne wurde inzwischen geändert. Bitte neu laden.".into());
        }
        let old_route = state.life_runtime.session_outputs.resolve(bot).ok().flatten();
        let outcome = super::set_mode_with_cancel(app, bot, super::Mode::Life, true, true, Some(cancel.clone())).await?;
        let lease = outcome.output.ok_or(ERROR)?;
        if old_route.is_none() { guard.added_routes.push(lease.clone()); }
        if !before.contains_key(bot) {
            let session = outcome.runtime.sessions.iter().find(|session| &session.bot_id == bot).ok_or(ERROR)?.session_id.clone();
            guard.started.push((bot.clone(), session, lease));
        }
    }
    if cancel.load(Ordering::SeqCst) { return Err("Der Start von Life wurde abgebrochen.".into()); }
    let runtime = state.life_runtime.snapshot_for(&state.autopilot)?;
    guard.complete = true;
    Ok(runtime)
}
fn scene_members(config: &Value) -> Result<Vec<String>, String> {
    let members = config["members"].as_array().filter(|members| !members.is_empty() && members.len() <= 8).ok_or(ERROR)?;
    let mut result = Vec::new();
    for member in members {
        let bot = member.as_str().filter(|bot| crate::storage::valid_id(bot)).ok_or(ERROR)?;
        if result.iter().any(|current| current == bot) { return Err(ERROR.into()); }
        result.push(bot.to_owned());
    }
    Ok(result)
}

/// Initial placement is a monitor-relative WINDOW CENTRE, not World arrival.
pub(crate) fn initial_window_position(app: &tauri::AppHandle, window: &tauri::WebviewWindow, bot: &str) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let Ok(world) = state.world.snapshot().and_then(|value| serde_json::to_value(value).map_err(|_| ERROR.to_owned())) else { return Ok(()); };
    let Some(actor) = world["snapshot"]["actors"].as_array().and_then(|actors| actors.iter().find(|actor| actor["actorId"] == bot)) else { return Ok(()); };
    let Some(x) = actor["position"]["x"].as_u64().filter(|value| *value <= 1_000_000) else { return Err(ERROR.into()); };
    let Some(y) = actor["position"]["y"].as_u64().filter(|value| *value <= 1_000_000) else { return Err(ERROR.into()); };
    let settings = serde_json::to_value(state.world.settings(&state.store)).map_err(|_| ERROR)?;
    let preferred = settings["record"]["outputBinding"]["monitorId"].as_str();
    let monitors = window.available_monitors().map_err(|_| ERROR)?;
    let monitor = if let Some(preferred) = preferred {
        monitors.iter().find(|monitor| {
            let p = monitor.position(); format!("{}:{}:{}", p.x, p.y, monitor.name().map(String::as_str).unwrap_or("Monitor")) == preferred
        }).cloned().ok_or(ERROR)?
    } else { window.current_monitor().map_err(|_| ERROR)?.or_else(|| monitors.first().cloned()).ok_or(ERROR)? };
    let size = window.inner_size().map_err(|_| ERROR)?; let bounds = monitor.size(); let origin = monitor.position();
    let position = window_centre(origin.x, origin.y, bounds.width, bounds.height, size.width, size.height, x as u32, y as u32);
    window.set_position(tauri::PhysicalPosition::new(position.0, position.1)).map_err(|_| ERROR.into())
}
fn window_centre(left: i32, top: i32, width: u32, height: u32, window_width: u32, window_height: u32, x: u32, y: u32) -> (i32, i32) {
    fn axis(origin: i32, span: u32, size: u32, canonical: u32) -> i32 {
        let origin = i64::from(origin);
        let desired = origin + (f64::from(span) * f64::from(canonical) / 1_000_000.).round() as i64 - i64::from(size) / 2;
        desired.clamp(origin, origin + i64::from(span.saturating_sub(size))).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
    (axis(left, width, window_width, x), axis(top, height, window_height, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_opening_route_cannot_become_visible_or_reused() {
        let outputs = SessionOutputs::default();
        let (lease, fresh) = outputs.begin("nova").unwrap(); assert!(fresh);
        let mut state = outputs.state.lock().unwrap();
        state.entries["nova"].lease.cancel.store(true, Ordering::SeqCst);
        let sequence = state.advance().unwrap(); state.entries.get_mut("nova").unwrap().sequence = sequence;
        let stopped = SessionOutputs::projection(&state, "nova");
        assert_eq!(stopped.state, "inactive"); assert!(stopped.route_id.is_none());
        drop(state);
        assert!(!lease.current()); assert!(outputs.begin("nova").is_err()); assert!(outputs.resolve("nova").is_err());
    }
    #[test]
    fn unrelated_route_and_late_old_lease_keep_exact_identity() {
        let outputs = SessionOutputs::default();
        let (old, _) = outputs.begin("nova").unwrap(); let (other, _) = outputs.begin("other").unwrap();
        { let mut state = outputs.state.lock().unwrap(); state.entries.remove("nova"); }
        let (new, _) = outputs.begin("nova").unwrap();
        let state = outputs.state.lock().unwrap();
        assert!(!SessionOutputs::matches(&state.entries["nova"], &old));
        assert!(SessionOutputs::matches(&state.entries["nova"], &new));
        assert!(other.current()); assert_ne!(old.id, new.id);
    }
    #[test]
    fn native_window_centres_use_physical_monitor_origin_and_clamp_rectangles() {
        assert_eq!(window_centre(-1920, 0, 1920, 1080, 520, 720, 500_000, 500_000), (-1220, 180));
        assert_eq!(window_centre(0, 0, 1920, 1080, 520, 720, 0, 1_000_000), (0, 360));
        assert_eq!(window_centre(100, -300, 300, 200, 520, 720, 1_000_000, 0), (100, -300));
    }
    #[test]
    fn scene_start_requires_explicit_bounded_distinct_persisted_members() {
        assert_eq!(scene_members(&json!({"members":["nova","other"]})).unwrap(), ["nova", "other"]);
        for value in [json!({"members":[]}), json!({"members":["nova","nova"]}), json!({"members":["../other"]}), json!({"members":[null]}), json!({"members":["a","b","c","d","e","f","g","h","i"]})] {
            assert!(scene_members(&value).is_err());
        }
    }
    fn visible(outputs: &SessionOutputs, bot: &str) -> Lease {
        let (lease, _) = outputs.begin(bot).unwrap();
        let mut state = outputs.state.lock().unwrap();
        let entry = state.entries.get_mut(bot).unwrap(); entry.state = "visible"; entry.opening = false;
        lease
    }
    #[test]
    fn stop_revokes_bound_jobs_and_pending_group_before_any_gui_cleanup() {
        let outputs = SessionOutputs::default();
        let lease = visible(&outputs, "nova"); let other = visible(&outputs, "other");
        let job = Arc::new(AtomicBool::new(false)); outputs.bind_job(&lease, &job).unwrap();
        let group = outputs.begin_group(vec!["nova".into(), "third".into()]).unwrap();
        let separate = outputs.begin_group(vec!["other".into()]).unwrap();
        let before = outputs.snapshot("nova").unwrap().sequence;
        assert_eq!(outputs.cancel_routes(Some("nova")).len(), 1);
        assert!(!lease.current()); assert!(job.load(Ordering::SeqCst)); assert!(group.load(Ordering::SeqCst));
        assert!(other.current()); assert!(!separate.load(Ordering::SeqCst));
        let after = outputs.snapshot("nova").unwrap();
        assert!(after.sequence > before); assert_eq!(after.state, "inactive"); assert!(after.route_id.is_none());
        assert!(outputs.bind_job(&lease, &Arc::new(AtomicBool::new(false))).is_err());
    }
    #[test]
    fn poisoned_route_registry_cancels_jobs_and_never_falls_back_to_studio() {
        let outputs = SessionOutputs::default(); let lease = visible(&outputs, "nova");
        let job = Arc::new(AtomicBool::new(false)); outputs.bind_job(&lease, &job).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = outputs.state.lock().unwrap(); panic!("synthetic registry poison");
        }));
        assert!(outputs.resolve("nova").is_err()); assert!(!lease.current()); assert!(job.load(Ordering::SeqCst));
        assert_eq!(outputs.available("nova"), Some(false)); assert!(outputs.begin("other").is_err());
    }
    #[test]
    fn sequence_exhaustion_revokes_existing_route_and_job_without_new_owner() {
        let outputs = SessionOutputs::default(); let lease = visible(&outputs, "nova");
        let job = Arc::new(AtomicBool::new(false)); outputs.bind_job(&lease, &job).unwrap();
        outputs.state.lock().unwrap().sequence = MAX_SEQUENCE;
        assert!(outputs.begin("other").is_err()); assert!(!lease.current()); assert!(job.load(Ordering::SeqCst));
        assert!(!outputs.state.lock().unwrap().entries.contains_key("other"));
    }
    #[test]
    fn admitted_old_cleanup_cannot_act_on_a_successor_using_the_same_bot_window() {
        let outputs=SessionOutputs::default();let old=visible(&outputs,"nova");
        outputs.cancel_routes(Some("nova"));
        outputs.state.lock().unwrap().entries.get_mut("nova").unwrap().cleanup_queued=true;
        assert!(outputs.cleanup_current(&old));
        outputs.state.lock().unwrap().entries.remove("nova");
        let new=visible(&outputs,"nova");
        assert!(!outputs.cleanup_current(&old));assert!(new.current());
        assert_eq!(outputs.resolve("nova").unwrap().unwrap().id,new.id);
    }
    #[test]
    fn stop_fences_both_before_core_and_during_core_prevent_late_route_creation() {
        let outputs=SessionOutputs::default();let first=outputs.start_fence("nova").unwrap();
        outputs.cancel_routes(None);assert!(!outputs.start_current("nova",&first));
        assert!(outputs.begin_for_start("nova",&first).is_err());
        let between=outputs.start_fence("nova").unwrap();
        outputs.cancel_routes(None);assert!(outputs.begin_for_start("nova",&between).is_err());
        let after=outputs.start_fence("nova").unwrap();
        outputs.cancel_routes(Some("other"));assert!(outputs.start_current("nova",&after));
        assert!(outputs.begin_for_start("nova",&after).is_ok());
    }
    #[test]
    fn partial_scene_rollback_preserves_prior_and_replaced_sessions_and_saved_output() {
        use crate::life_domain::{ActorId, LifeState, Needs, WorldId};
        let root = std::env::temp_dir().join(format!("orbitpal-scene-rollback-{}", uuid::Uuid::new_v4()));
        let store = crate::storage::Store::new(root.clone()).unwrap();
        let profile = store.profile().unwrap(); let original = profile.clone(); let bot = profile["id"].as_str().unwrap();
        let other = store.create_bot("Independent", false).unwrap()["id"].as_str().unwrap().to_owned();
        let world = crate::world_hub::WorldHub::new(&store);
        let autopilot = crate::autopilot::AutopilotHub::default();
        let mut core = super::super::Core::new(crate::life_availability::Snapshot {
            state: crate::life_availability::Availability::Unknown,
            reason: crate::life_availability::Reason::Initializing, generation: 1,
            source: crate::life_availability::Source::Unsupported,
        });
        let scene_session = "00000000-0000-4000-8000-000000000001";
        let prior_session = "00000000-0000-4000-8000-000000000002";
        for (id, session) in [(bot, scene_session), (other.as_str(), prior_session)] {
            let life = LifeState::new(ActorId::new(id).unwrap(), WorldId::new(format!("world_{id}")).unwrap(), Needs::new(5000,5000,5000,5000).unwrap());
            core.start(id, super::super::Mode::Life, session.into(), super::super::SpeechConfig { speak: false, min_ms: 15000, max_ms: 15000 }, Some(life), 7, false, true, 0).unwrap();
            autopilot.set(id, true, crate::autopilot::Config::default()).unwrap();
        }
        let outputs = SessionOutputs::default(); let lease = visible(&outputs, bot);
        assert!(!rollback_core(&mut core,&store,&world,&autopilot,bot,"stale-session",&outputs,&lease,1).0);
        assert!(core.sessions.contains_key(bot)); assert!(lease.current());
        let successor=outputs.adopt(&lease,true).unwrap();
        core.renew_start_authority(bot).unwrap();
        assert!(!rollback_core(&mut core,&store,&world,&autopilot,bot,scene_session,&outputs,&lease,2).0);
        assert!(core.sessions.contains_key(bot));assert!(successor.current());assert!(!lease.current());
        assert!(rollback_core(&mut core,&store,&world,&autopilot,bot,scene_session,&outputs,&successor,3).0);
        assert!(!core.sessions.contains_key(bot)); assert_eq!(core.sessions[&other].session_id,prior_session);
        assert!(autopilot.runtime(&other,&crate::autopilot::Config::default()).active);
        assert!(!lease.current()); assert_eq!(store.bot_profile(bot).unwrap()["output"], original["output"]);
        drop(world); drop(store); let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn scene_route_rollback_preserves_pending_shared_lease_then_exact_adoption() {
        use crate::life_domain::{ActorId, LifeState, Needs, WorldId};
        for completes in [false, true] {
            let mut core = super::super::Core::new(crate::life_availability::Snapshot {
                state: crate::life_availability::Availability::Unknown,
                reason: crate::life_availability::Reason::Initializing, generation: 1,
                source: crate::life_availability::Source::Unsupported,
            });
            let session = "00000000-0000-4000-8000-000000000003";
            let life = LifeState::new(ActorId::new("nova").unwrap(), WorldId::new("world_nova").unwrap(), Needs::new(5000,5000,5000,5000).unwrap());
            core.start("nova",super::super::Mode::Life,session.into(),super::super::SpeechConfig { speak:false,min_ms:15000,max_ms:15000 },Some(life),7,false,true,0).unwrap();
            let outputs = SessionOutputs::default();
            let old = visible(&outputs,"nova");
            let other = visible(&outputs,"other");
            core.begin_start("nova","independent-successor".into()).unwrap();
            let fence = outputs.start_fence("nova").unwrap();
            let (pending, fresh) = outputs.begin_for_start("nova",&fence).unwrap();
            assert!(!fresh); assert_eq!(pending.id,old.id);
            assert!(core.stop_exact("nova",session,None,1).is_none());
            assert!(!revoke_unclaimed_route(&core,&outputs,&old));
            assert!(pending.current()); assert!(core.start_current("nova","independent-successor"));
            if completes {
                core.renew_start_authority("nova").unwrap();
                let successor = outputs.adopt(&pending,true).unwrap();
                core.cancel_start("nova","independent-successor");
                assert!(!revoke_unclaimed_route(&core,&outputs,&old));
                assert!(successor.current()); assert!(!old.current());
                assert!(core.sessions.contains_key("nova"));
            } else {
                core.cancel_start("nova","independent-successor");
                assert!(core.stop_exact("nova",session,None,2).is_some());
                assert!(revoke_unclaimed_route(&core,&outputs,&old));
                assert!(!old.current());
            }
            assert!(other.current());
        }
    }
}
