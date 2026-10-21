//! Optional read-only desktop geometry feeds ordinary native point actions.
//! Geometry is a short-lived proposal, never a display/capture/session grant.
use super::LifeRuntime;
use crate::{
    desktop_affordances::{self, GeometryLease, GeometrySample, ReadContext, Rect, WindowIdentity},
    experience::{ExperienceRuntime, Preferences},
    life_domain::{ActionId, ActionPhase, Activity, LifeState},
    world_output::OutputProof,
    world_state::coordinates::{Point, Rect as SafeRect},
    AppState,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
use tauri::Manager;
const ERROR: &str = "desktop_geometry_unavailable";
const SAMPLE_MS: u64 = 750;
const GEOMETRY_MS: u64 = 1500;
#[derive(Clone, Debug, Eq, PartialEq)]
struct Key {
    session: String,
    revision: u64,
    route: String,
    binding: String,
    binding_sequence: u64,
    instance: String,
    config: u64,
    topology: u64,
    display: String,
    bounds: (i32, i32, u32, u32),
    scale: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Window,
    Cursor,
}
#[derive(Clone, Debug)]
struct Target {
    kind: Kind,
    point: Point,
    window: Option<Rect>,
    identity: Option<WindowIdentity>,
}
#[derive(Clone)]
struct Pending {
    action: ActionId,
    target: Target,
    activity: Activity,
}
struct Entry {
    key: Key,
    preferences: u64,
    home: Point,
    geometry: GeometryLease,
    pending: Option<Pending>,
    admitting: bool,
    cancel: Arc<AtomicBool>,
    next: u64,
    sample_after: u64,
    sampled: u64,
    proposed: Option<Point>,
    rest: Option<WindowRestGuard>,
}
/// Retained evidence only: this guard cannot read the OS, lock Core/Store or
/// create a renderer receipt. A genuine native job supplies its own cancel atom.
#[derive(Clone)]
pub(crate) struct WindowRestGuard {
    bot: String,
    key: Key,
    preferences: u64,
    generation: u64,
    target: Target,
    proof: OutputProof,
    state: Arc<Mutex<RestFreshness>>,
}
struct RestFreshness {
    valid: bool,
    sampled: u64,
    job: Option<Weak<AtomicBool>>,
}
impl RestFreshness {
    fn invalidate(&mut self) {
        self.valid = false;
        if let Some(cancel) = self.job.as_ref().and_then(Weak::upgrade) {
            cancel.store(true, Ordering::SeqCst);
        }
    }
    fn current(&mut self, now: u64) -> bool {
        if !self.valid || now < self.sampled || now - self.sampled > GEOMETRY_MS {
            self.invalidate();
        }
        self.valid
    }
    fn observe(&mut self, sampled: u64, now: u64, matching: bool) -> bool {
        if !self.current(now)
            || !matching
            || sampled < self.sampled
            || sampled > now
            || now - sampled > GEOMETRY_MS
        {
            self.invalidate();
            return false;
        }
        // An unchanged timestamp never extends the lease.
        self.sampled = sampled;
        true
    }
}
impl WindowRestGuard {
    fn new(
        capture: &Capture,
        target: &Target,
        sample: &GeometrySample,
        now: u64,
    ) -> Result<Self, String> {
        if target.kind != Kind::Window
            || target.identity.is_none()
            || !rest_allowed(capture)
            || sample.window_identity != target.identity
            || sample.window_top != target.window
            || target.window.is_none()
            || capture.position != target.point
            || sample.source_generation != capture.context.generation
            || sample.display_id != capture.context.display_id
            || sample.work_area != capture.context.physical_scene
            || sample.at_ms > now
            || now - sample.at_ms > GEOMETRY_MS
        {
            return Err(ERROR.into());
        }
        Ok(Self {
            bot: capture.bot.clone(),
            key: capture.key.clone(),
            preferences: capture.preferences,
            generation: capture.context.generation,
            target: target.clone(),
            proof: capture.proof.clone(),
            state: Arc::new(Mutex::new(RestFreshness {
                valid: true,
                sampled: sample.at_ms,
                job: None,
            })),
        })
    }
    pub(crate) fn current(&self, bot: &str, output: &OutputProof, now: u64) -> bool {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                error.into_inner().invalidate();
                return false;
            }
        };
        if bot != self.bot
            || output.binding != self.proof.binding
            || output.stamp != self.proof.stamp
            || output.profile_token != self.proof.profile_token
            || !Arc::ptr_eq(&output.cancel, &self.proof.cancel)
            || !matches!((&output.route,&self.proof.route),(crate::world_output::SourceRoute::Native(a),crate::world_output::SourceRoute::Native(b)) if a.same_route(b))
            || output.cancel.load(Ordering::SeqCst)
            || output.binding.reduced_motion
            || !output.binding.station_supported
        {
            state.invalidate();
            return false;
        }
        state.current(now)
    }
    pub(crate) fn bind(&self, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                error.into_inner().invalidate();
                cancel.store(true, Ordering::SeqCst);
                return Err(ERROR.into());
            }
        };
        if !state.valid
            || cancel.load(Ordering::SeqCst)
            || state
                .job
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|old| !Arc::ptr_eq(&old, cancel))
        {
            cancel.store(true, Ordering::SeqCst);
            return Err(ERROR.into());
        }
        state.job = Some(Arc::downgrade(cancel));
        Ok(())
    }
    fn invalidate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.invalidate();
    }
    fn observe(&self, capture: &Capture, sample: &GeometrySample, now: u64) {
        let matching = capture.key == self.key
            && capture.preferences == self.preferences
            && rest_allowed(capture)
            && capture.context.generation == self.generation
            && sample.source_generation == self.generation
            && sample.display_id == self.key.display
            && sample.work_area == capture.context.physical_scene
            && sample.window_identity == self.target.identity
            && sample.window_top == self.target.window
            && capture.position == self.target.point
            && capture.proof.binding == self.proof.binding
            && Arc::ptr_eq(&capture.proof.cancel, &self.proof.cancel);
        match self.state.lock() {
            Ok(mut state) => {
                state.observe(sample.at_ms, now, matching);
            }
            Err(error) => {
                error.into_inner().invalidate();
            }
        }
    }
}
#[derive(Default)]
pub(super) struct DesktopPlaces {
    entries: Mutex<BTreeMap<String, Entry>>,
    in_flight: AtomicBool,
}
impl DesktopPlaces {
    pub(super) fn busy(&self, bot: &str) -> bool {
        self.entries
            .lock()
            .map(|entries| {
                entries
                    .get(bot)
                    .is_some_and(|e| e.admitting || e.pending.is_some())
            })
            .unwrap_or(true)
    }
    pub(super) fn admission_current(&self, bot: &str, target: Point, now: u64) -> bool {
        self.entries.lock().is_ok_and(|entries| {
            entries.get(bot).is_some_and(|entry| {
                entry.admitting
                    && entry.proposed == Some(target)
                    && !entry.cancel.load(Ordering::SeqCst)
                    && now >= entry.sampled
                    && now - entry.sampled <= 1500
            })
        })
    }
    /// No Core acquisition here: Stop/preset hooks may call while Core is held.
    pub(super) fn cancel_actor(&self, bot: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(bot) {
                entry.cancel.store(true, Ordering::SeqCst);
                entry.geometry.invalidate();
                if let Some(guard) = &entry.rest {
                    guard.invalidate();
                }
            }
        }
    }
}
#[derive(Clone)]
struct Capture {
    bot: String,
    key: Key,
    preferences: u64,
    prefs: Preferences,
    context: ReadContext,
    proof: OutputProof,
    position: Point,
    safe: SafeRect,
    life: LifeState,
    window: tauri::WebviewWindow,
}
fn rest_allowed(capture: &Capture) -> bool {
    capture.prefs.desktop_anchors
        && capture.prefs.allowed_animations.iter().any(|a| a == "rest")
        && !capture.proof.binding.reduced_motion
        && capture.proof.binding.station_supported
}
fn capture(app: &tauri::AppHandle, bot: &str) -> Result<Capture, String> {
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    if runtime.stopping.load(Ordering::SeqCst)
        || app.state::<ExperienceRuntime>().paused(bot)?
        || state.updater.ensure_idle().is_err()
    {
        return Err(ERROR.into());
    }
    let settings = state.store.experience_settings(bot)?;
    if !settings.preferences.desktop_anchors && !settings.preferences.cursor_play {
        return Err(ERROR.into());
    }
    let members = runtime.native_scenes.members(Some(bot));
    if members.len() != 1 {
        return Err(ERROR.into());
    }
    let member = members.into_iter().next().ok_or(ERROR)?;
    if !member.current()
        || !runtime
            .session_outputs
            .constructor_current(&member.constructor)
    {
        return Err(ERROR.into());
    }
    let owned = member.owned.lock().map_err(|_| ERROR)?;
    let owned = owned.as_ref().ok_or(ERROR)?;
    let core = runtime.core.lock().map_err(|_| ERROR)?;
    let now = runtime.now();
    if !owned.context.current(&core, &state, now) || core.local_fence(bot, now).is_none() {
        return Err(ERROR.into());
    }
    let source = owned
        .context
        .source(&core, &state, owned.route.clone(), owned.framing, now)?;
    let proof = core.world_outputs.ready(bot, now).map_err(|_| ERROR)?;
    let permit = owned.action_permit(now).ok_or(ERROR)?;
    if !core.world_outputs.matches(bot, &source)
        || !permit.current(&core, now)
        || !permit.matches_output(&proof)
    {
        return Err(ERROR.into());
    }
    let bounds = owned.live_bounds(now).ok_or(ERROR)?;
    if bounds.width != owned.framing.physical_width
        || bounds.height != owned.framing.physical_height
    {
        return Err(ERROR.into());
    }
    let (x, y, width, height) = owned.context.geometry.physical_bounds();
    let session = core.sessions.get(bot).ok_or(ERROR)?;
    let life = session.state.clone().ok_or(ERROR)?;
    let key = Key {
        session: session.session_id.clone(),
        revision: session.revision,
        route: owned.route.namespace().into(),
        binding: proof.binding.render_session_id.clone(),
        binding_sequence: proof.binding.binding_sequence,
        instance: source.context.instance_id,
        config: source.context.config_revision,
        topology: source.context.topology_revision,
        display: owned.context.geometry.id().into(),
        bounds: (bounds.x, bounds.y, bounds.width, bounds.height),
        scale: owned.framing.scale_factor.to_bits(),
    };
    let context = ReadContext {
        generation: member.sequence,
        display_id: key.display.clone(),
        physical_display: Rect {
            x,
            y,
            width,
            height,
        },
        physical_scene: Rect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        },
        scale: owned.framing.scale_factor,
    };
    Ok(Capture {
        bot: bot.into(),
        key,
        preferences: settings.revision,
        prefs: settings.preferences,
        context,
        proof,
        position: source.context.position,
        safe: source.safe_region,
        life,
        window: member.constructor.window().clone(),
    })
}
fn current(app: &tauri::AppHandle, expected: &Capture) -> bool {
    let Ok(actual) = capture(app, &expected.bot) else {
        return false;
    };
    if actual.key != expected.key || actual.preferences != expected.preferences {
        return false;
    }
    let state = app.state::<AppState>();
    let Ok(core) = state.life_runtime.core.lock() else {
        return false;
    };
    core.world_outputs
        .check_proof(&expected.proof, state.life_runtime.now())
}
fn actual_bounds(capture: &Capture) -> bool {
    let (Ok(position), Ok(size)) = (capture.window.outer_position(), capture.window.inner_size())
    else {
        return false;
    };
    (position.x, position.y, size.width, size.height) == capture.key.bounds
}
async fn read_main(app: &tauri::AppHandle, capture: &Capture) -> Option<GeometrySample> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    let capture = capture.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let queued_cancel = cancelled.clone();
    let deadline = Instant::now() + Duration::from_millis(500);
    if app
        .run_on_main_thread(move || {
            let sample = if !queued_cancel.load(Ordering::SeqCst)
                && Instant::now() < deadline
                && current(&handle, &capture)
                && actual_bounds(&capture)
            {
                let now = handle.state::<AppState>().life_runtime.now();
                let sample =
                    desktop_affordances::read_native(&capture.context, &capture.prefs, false, now);
                if !queued_cancel.load(Ordering::SeqCst)
                    && Instant::now() < deadline
                    && actual_bounds(&capture)
                    && current(&handle, &capture)
                {
                    sample
                } else {
                    None
                }
            } else {
                None
            };
            let _ = send.send(sample);
        })
        .is_err()
    {
        return None;
    }
    let result = tokio::time::timeout(Duration::from_millis(500), receive)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
    cancelled.store(true, Ordering::SeqCst);
    result
}
fn candidates(
    geometry: &GeometryLease,
    generation: u64,
    display: &str,
    now: u64,
    prefs: &Preferences,
    home: Point,
    position: Option<Point>,
    safe: SafeRect,
    sample: &GeometrySample,
) -> Vec<Target> {
    let mut result = vec![];
    for kind in [Kind::Window, Kind::Cursor] {
        let mut one = prefs.clone();
        one.desktop_anchors = kind == Kind::Window && prefs.desktop_anchors;
        one.cursor_play = kind == Kind::Cursor && prefs.cursor_play;
        for point in geometry.anchors(generation, display, now, &one, false) {
            let Ok(point) = Point::new(point.x, point.y) else {
                continue;
            };
            if !safe.contains(point)
                || !super::self_care::within_radius(home, point, prefs.movement_radius)
                || position.is_some_and(|position| {
                    u64::from(point.x().abs_diff(position.x()))
                        + u64::from(point.y().abs_diff(position.y()))
                        < 5000
                })
            {
                continue;
            }
            result.push(Target {
                kind,
                point,
                window: if kind == Kind::Window {
                    sample.window_top
                } else {
                    None
                },
                identity: if kind == Kind::Window {
                    sample.window_identity
                } else {
                    None
                },
            });
        }
    }
    result
}
fn target_current(target: &Target, sample: &GeometrySample, candidates: &[Target]) -> bool {
    candidates.iter().any(|candidate| {
        candidate.kind == target.kind
            && match target.kind {
                Kind::Window => {
                    candidate.window == target.window
                        && candidate.identity == target.identity
                        && candidate.point == target.point
                }
                Kind::Cursor => {
                    u64::from(candidate.point.x().abs_diff(target.point.x()))
                        + u64::from(candidate.point.y().abs_diff(target.point.y()))
                        <= 25000
                }
            }
    })
}
fn cancel_pending(app: &tauri::AppHandle, pending: Option<Pending>) -> Result<(), String> {
    if let Some(pending) = pending {
        super::cancel_actor_action_exact(app, pending.action.actor_id().as_str(), &pending.action)?;
    }
    Ok(())
}
fn confirmed_window_arrival(
    life: &LifeState,
    pending: &Pending,
    position: Point,
    allowed: bool,
) -> bool {
    pending.activity == Activity::MoveTo
        && pending.target.kind == Kind::Window
        && pending.target.identity.is_some()
        && position == pending.target.point
        && allowed
        && life.current_action().is_some_and(|action| {
            action.intent().id() == &pending.action
                && action.intent().activity() == Activity::MoveTo
                && action.phase() == ActionPhase::Completed
        })
}
async fn start_window_rest(
    app: &tauri::AppHandle,
    capture: &Capture,
    pending: &Pending,
    sample: &GeometrySample,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    let guard = WindowRestGuard::new(capture, &pending.target, sample, runtime.now())?;
    {
        let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
        let entry = entries.get_mut(&capture.bot).ok_or(ERROR)?;
        if entry.key != capture.key
            || entry.cancel.load(Ordering::SeqCst)
            || entry
                .pending
                .as_ref()
                .is_none_or(|old| old.action != pending.action)
        {
            return Err(ERROR.into());
        }
        entry.rest = Some(guard.clone());
        entry.admitting = true;
        entry.proposed = Some(pending.target.point);
    }
    let planned =
        super::start_window_rest_activity(app, &capture.bot, capture.proof.clone(), guard.clone())
            .await;
    // Retain an admitted real action before any fallible post-await cleanup.
    if let Ok(life) = &planned {
        if let Some(action) = life.current_action() {
            let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
            if let Some(entry) = entries.get_mut(&capture.bot) {
                entry.pending = Some(Pending {
                    action: action.intent().id().clone(),
                    target: pending.target.clone(),
                    activity: Activity::Rest,
                });
            }
        }
    }
    if planned.is_err()
        || !current(app, capture)
        || !guard.current(&capture.bot, &capture.proof, runtime.now())
    {
        guard.invalidate();
        if let Ok(life) = &planned {
            if let Some(action) = life.current_action() {
                super::cancel_actor_action_exact(app, &capture.bot, action.intent().id())?;
            }
        }
    }
    let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
    if let Some(entry) = entries.get_mut(&capture.bot) {
        entry.admitting = false;
        entry.proposed = None;
        entry.next = runtime
            .now()
            .saturating_add(u64::from(capture.prefs.initiative_cooldown_ms));
        if planned.is_err() {
            entry.rest = None;
            entry.pending = None;
        }
    }
    planned.map(|_| ())
}
fn retire(app: &tauri::AppHandle, bot: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let pending = {
        let mut entries = state
            .life_runtime
            .desktop
            .entries
            .lock()
            .map_err(|_| ERROR)?;
        let Some(entry) = entries.get_mut(bot) else {
            return Ok(());
        };
        entry.cancel.store(true, Ordering::SeqCst);
        entry.geometry.invalidate();
        if let Some(guard) = &entry.rest {
            guard.invalidate();
        }
        entry.pending.clone()
    };
    cancel_pending(app, pending)?;
    state
        .life_runtime
        .desktop
        .entries
        .lock()
        .map_err(|_| ERROR)?
        .remove(bot);
    Ok(())
}
pub(super) fn pulse(runtime: &LifeRuntime, app: &tauri::AppHandle) {
    if runtime.stopping.load(Ordering::SeqCst)
        || runtime.desktop.in_flight.swap(true, Ordering::SeqCst)
    {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        let _ = drive(&handle).await;
        state
            .life_runtime
            .desktop
            .in_flight
            .store(false, Ordering::SeqCst);
    });
}
async fn drive(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    let bots = runtime
        .native_scenes
        .members(None)
        .into_iter()
        .map(|m| m.constructor.bot().to_owned())
        .collect::<Vec<_>>();
    let stale = {
        let entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
        entries
            .keys()
            .filter(|id| !bots.contains(id))
            .cloned()
            .collect::<Vec<_>>()
    };
    for bot in stale {
        retire(app, &bot)?;
    }
    for bot in bots {
        let capture = match capture(app, &bot) {
            Ok(c) => c,
            Err(_) => {
                retire(app, &bot)?;
                continue;
            }
        };
        let now = runtime.now();
        let replace = {
            let entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
            entries.get(&bot).is_none_or(|e| {
                e.key != capture.key
                    || e.preferences != capture.preferences
                    || e.cancel.load(Ordering::SeqCst)
            })
        };
        if replace {
            retire(app, &bot)?;
            runtime.desktop.entries.lock().map_err(|_| ERROR)?.insert(
                bot.clone(),
                Entry {
                    key: capture.key.clone(),
                    preferences: capture.preferences,
                    home: capture.position,
                    geometry: Default::default(),
                    pending: None,
                    admitting: false,
                    cancel: Arc::new(AtomicBool::new(false)),
                    next: now,
                    sample_after: 0,
                    sampled: 0,
                    proposed: None,
                    rest: None,
                },
            );
        }
        if runtime
            .desktop
            .entries
            .lock()
            .map_err(|_| ERROR)?
            .get(&bot)
            .is_none_or(|entry| now < entry.sample_after)
        {
            continue;
        }
        let sample = read_main(app, &capture).await;
        if !current(app, &capture) || sample.is_none() {
            let pending = {
                let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
                let Some(entry) = entries.get_mut(&bot) else {
                    continue;
                };
                entry.geometry.invalidate();
                if let Some(guard) = &entry.rest {
                    guard.invalidate();
                }
                entry.next = runtime
                    .now()
                    .saturating_add(u64::from(capture.prefs.initiative_cooldown_ms));
                entry.pending.clone()
            };
            cancel_pending(app, pending)?;
            if let Some(entry) = runtime
                .desktop
                .entries
                .lock()
                .map_err(|_| ERROR)?
                .get_mut(&bot)
            {
                entry.pending = None;
            }
            continue;
        }
        let sample = sample.ok_or(ERROR)?;
        let now = runtime.now();
        let (target, pending, cancel) = {
            let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
            let Some(entry) = entries.get_mut(&bot) else {
                continue;
            };
            if entry.cancel.load(Ordering::SeqCst) || entry.key != capture.key {
                continue;
            }
            entry.sample_after = now.saturating_add(SAMPLE_MS);
            entry.sampled = sample.at_ms;
            if !entry.geometry.observe(sample.clone()) {
                if let Some(guard) = &entry.rest {
                    guard.invalidate();
                }
                continue;
            }
            if let Some(guard) = &entry.rest {
                guard.observe(&capture, &sample, now);
            }
            let targets = candidates(
                &entry.geometry,
                capture.context.generation,
                &capture.context.display_id,
                now,
                &capture.prefs,
                entry.home,
                Some(capture.position),
                capture.safe,
                &sample,
            );
            (
                targets.first().cloned(),
                entry.pending.clone(),
                entry.cancel.clone(),
            )
        };
        if let Some(pending) = pending {
            let targets = {
                let entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
                let entry = entries.get(&bot).ok_or(ERROR)?;
                candidates(
                    &entry.geometry,
                    capture.context.generation,
                    &capture.context.display_id,
                    now,
                    &capture.prefs,
                    entry.home,
                    None,
                    capture.safe,
                    &sample,
                )
            };
            let phase = capture
                .life
                .current_action()
                .filter(|a| a.intent().id() == &pending.action)
                .map(|a| a.phase());
            let invalid = cancel.load(Ordering::SeqCst)
                || !target_current(&pending.target, &sample, &targets)
                || runtime
                    .desktop
                    .entries
                    .lock()
                    .map_err(|_| ERROR)?
                    .get(&bot)
                    .and_then(|entry| entry.rest.as_ref())
                    .is_some_and(|guard| !guard.current(&bot, &capture.proof, now));
            if invalid {
                cancel_pending(app, Some(pending))?;
            } else if confirmed_window_arrival(
                &capture.life,
                &pending,
                capture.position,
                rest_allowed(&capture),
            ) {
                // Completion came from the real MoveTo pipeline; no timer or
                // coordinate proximity can invent arrival or station dwell.
                let _ = start_window_rest(app, &capture, &pending, &sample).await;
                continue;
            }
            if invalid
                || matches!(
                    phase,
                    None | Some(ActionPhase::Completed | ActionPhase::Cancelled)
                )
            {
                if let Some(entry) = runtime
                    .desktop
                    .entries
                    .lock()
                    .map_err(|_| ERROR)?
                    .get_mut(&bot)
                {
                    if let Some(guard) = entry.rest.take() {
                        guard.invalidate();
                    }
                    entry.pending = None;
                    entry.next =
                        now.saturating_add(u64::from(capture.prefs.initiative_cooldown_ms));
                }
            }
            continue;
        }
        let Some(target) = target else {
            continue;
        };
        if runtime.social.busy(&bot) {
            continue;
        }
        let busy = {
            let core = runtime.core.lock().map_err(|_| ERROR)?;
            core.actions.as_ref().is_none_or(|a| a.actor_busy(&bot))
        };
        if busy {
            continue;
        }
        {
            let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
            let Some(entry) = entries.get_mut(&bot) else {
                continue;
            };
            if now < entry.next || entry.cancel.load(Ordering::SeqCst) {
                continue;
            }
            entry.admitting = true;
            entry.proposed = Some(target.point);
        }
        let planned =
            super::start_point_activity(app, &bot, target.point, capture.proof.clone()).await;
        // Retain the actual Plan before any fallible cancellation/readback.
        if let Ok(life) = &planned {
            if let Some(action) = life.current_action() {
                if let Some(entry) = runtime
                    .desktop
                    .entries
                    .lock()
                    .map_err(|_| ERROR)?
                    .get_mut(&bot)
                {
                    entry.pending = Some(Pending {
                        action: action.intent().id().clone(),
                        target: target.clone(),
                        activity: Activity::MoveTo,
                    });
                }
            }
        }
        if let Ok(life) = &planned {
            if cancel.load(Ordering::SeqCst) || !current(app, &capture) {
                if let Some(action) = life.current_action() {
                    super::cancel_actor_action_exact(app, &bot, action.intent().id())?;
                }
            }
        }
        let mut entries = runtime.desktop.entries.lock().map_err(|_| ERROR)?;
        let Some(entry) = entries.get_mut(&bot) else {
            continue;
        };
        entry.admitting = false;
        entry.proposed = None;
        entry.next = runtime
            .now()
            .saturating_add(u64::from(capture.prefs.initiative_cooldown_ms));
        if entry.cancel.load(Ordering::SeqCst) || entry.key != capture.key {
            continue;
        }
        if let Ok(life) = planned {
            if let Some(action) = life.current_action().filter(|a| {
                a.intent().activity() == Activity::MoveTo
                    && matches!(a.phase(), ActionPhase::Planned | ActionPhase::Started)
            }) {
                entry.pending = Some(Pending {
                    action: action.intent().id().clone(),
                    target,
                    activity: Activity::MoveTo,
                });
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> GeometrySample {
        GeometrySample {
            source_generation: 7,
            display_id: "left".into(),
            at_ms: 100,
            work_area: Rect {
                x: -1920,
                y: -200,
                width: 1920,
                height: 1080,
            },
            window_top: Some(Rect {
                x: -1200,
                y: 100,
                width: 400,
                height: 600,
            }),
            window_identity: Some(WindowIdentity {
                number: 17,
                owner: 23,
            }),
            cursor: Some((-960, 340)),
        }
    }
    fn prefs() -> Preferences {
        Preferences {
            desktop_anchors: true,
            movement_radius: 1000,
            ..Default::default()
        }
    }
    fn targets(sample: &GeometrySample, prefs: &Preferences, now: u64) -> Vec<Target> {
        let mut lease = GeometryLease::default();
        assert!(lease.observe(sample.clone()));
        candidates(
            &lease,
            7,
            "left",
            now,
            prefs,
            Point::new(500000, 500000).unwrap(),
            None,
            SafeRect::full(),
            sample,
        )
    }
    #[test]
    fn actual_geometry_mapping_preserves_negative_origin_and_separate_optins() {
        let sample = sample();
        let mut p = prefs();
        let window = targets(&sample, &p, 100);
        assert_eq!(window.len(), 1);
        assert_eq!(window[0].kind, Kind::Window);
        assert_eq!(window[0].point, Point::new(479166, 277777).unwrap());
        p.desktop_anchors = false;
        assert!(targets(&sample, &p, 100).is_empty());
        p.cursor_play = true;
        assert!(targets(&sample, &p, 100).is_empty());
        p.preset = crate::experience::Preset::Stream;
        let cursor = targets(&sample, &p, 100);
        assert_eq!(cursor.len(), 1);
        assert_eq!(cursor[0].point, Point::new(500000, 500000).unwrap());
    }
    #[test]
    fn expiry_safe_region_and_home_radius_cannot_be_bypassed_by_a_geometry_offer() {
        let sample = sample();
        let mut lease = GeometryLease::default();
        lease.observe(sample.clone());
        let mut p = prefs();
        assert!(targets(&sample, &p, 1601).is_empty());
        p.movement_radius = 0;
        assert!(targets(&sample, &p, 100).is_empty());
        p.movement_radius = 1000;
        let safe = SafeRect::new(
            Point::new(600000, 600000).unwrap(),
            Point::new(900000, 900000).unwrap(),
        )
        .unwrap();
        assert!(candidates(
            &lease,
            7,
            "left",
            100,
            &p,
            Point::new(500000, 500000).unwrap(),
            None,
            safe,
            &sample
        )
        .is_empty());
        assert!(candidates(
            &lease,
            8,
            "left",
            100,
            &p,
            Point::new(500000, 500000).unwrap(),
            None,
            SafeRect::full(),
            &sample
        )
        .is_empty());
    }
    #[test]
    fn changed_or_removed_window_retires_target_and_cursor_drift_is_bounded() {
        let sample = sample();
        let p = prefs();
        let target = targets(&sample, &p, 100).remove(0);
        assert!(target_current(&target, &sample, &targets(&sample, &p, 100)));
        let mut changed = sample.clone();
        changed.window_top.as_mut().unwrap().x += 1;
        assert!(!target_current(
            &target,
            &changed,
            &targets(&changed, &p, 100)
        ));
        changed.window_top = None;
        assert!(!target_current(
            &target,
            &changed,
            &targets(&changed, &p, 100)
        ));
        let p = Preferences {
            desktop_anchors: false,
            cursor_play: true,
            preset: crate::experience::Preset::Stream,
            ..p
        };
        let cursor = targets(&sample, &p, 100).remove(0);
        changed = sample.clone();
        changed.cursor = Some((-950, 340));
        assert!(target_current(
            &cursor,
            &changed,
            &targets(&changed, &p, 100)
        ));
        changed.cursor = Some((-500, 340));
        assert!(!target_current(
            &cursor,
            &changed,
            &targets(&changed, &p, 100)
        ));
    }
    #[test]
    fn admission_is_restricted_to_exact_fresh_proposal_and_synchronous_cancel() {
        let places = DesktopPlaces::default();
        let point = Point::new(400000, 400000).unwrap();
        let key = Key {
            session: "session".into(),
            revision: 1,
            route: "route".into(),
            binding: "binding".into(),
            binding_sequence: 1,
            instance: "instance".into(),
            config: 1,
            topology: 1,
            display: "left".into(),
            bounds: (-1920, 0, 1920, 1080),
            scale: 1f64.to_bits(),
        };
        places.entries.lock().unwrap().insert(
            "nova".into(),
            Entry {
                key,
                preferences: 1,
                home: point,
                geometry: Default::default(),
                pending: None,
                admitting: true,
                cancel: Arc::new(AtomicBool::new(false)),
                next: 0,
                sample_after: 0,
                sampled: 100,
                proposed: Some(point),
                rest: None,
            },
        );
        assert!(places.admission_current("nova", point, 1600));
        assert!(!places.admission_current("nova", point, 1601));
        assert!(!places.admission_current("nova", point, 99));
        assert!(!places.admission_current("blue", point, 100));
        assert!(!places.admission_current("nova", Point::new(400001, 400000).unwrap(), 100));
        places.cancel_actor("nova");
        assert!(!places.admission_current("nova", point, 100));
    }
    #[test]
    fn window_rest_expiry_is_sticky_and_cancels_the_bound_native_job() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut state = RestFreshness {
            valid: true,
            sampled: 100,
            job: Some(Arc::downgrade(&cancel)),
        };
        assert!(state.current(1600));
        assert!(!cancel.load(Ordering::SeqCst));
        assert!(!state.current(1601));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(
            !state.observe(1601, 1601, true),
            "fresh geometry cannot revive a retired rest job"
        );
    }
    #[test]
    fn only_a_new_matching_native_sample_renews_window_rest() {
        let mut state = RestFreshness {
            valid: true,
            sampled: 100,
            job: None,
        };
        assert!(state.observe(100, 1000, true));
        assert_eq!(state.sampled, 100);
        assert!(
            !state.current(1601),
            "timestamp reuse does not extend expiry"
        );
        let mut state = RestFreshness {
            valid: true,
            sampled: 100,
            job: None,
        };
        for sampled in [
            850, 1600, 2350, 3100, 3850, 4600, 5350, 6100, 6850, 7600, 8350, 9100, 9850, 10600,
        ] {
            assert!(state.observe(sampled, sampled, true));
            assert!(state.current(sampled + 749));
        }
        for (sampled, now, matching) in [
            (99, 100, true),
            (101, 100, true),
            (100, 99, true),
            (100, 100, false),
        ] {
            let cancel = Arc::new(AtomicBool::new(false));
            let mut state = RestFreshness {
                valid: true,
                sampled: 100,
                job: Some(Arc::downgrade(&cancel)),
            };
            assert!(!state.observe(sampled, now, matching));
            assert!(cancel.load(Ordering::SeqCst));
        }
    }
    #[test]
    fn identical_geometry_from_another_window_or_owner_never_preserves_rest() {
        let original = sample();
        let p = prefs();
        let target = targets(&original, &p, 100).remove(0);
        for owner_changed in [false, true] {
            let mut successor = original.clone();
            let identity = successor.window_identity.as_mut().unwrap();
            if owner_changed {
                identity.owner += 1;
            } else {
                identity.number += 1;
            }
            assert_eq!(successor.window_top, original.window_top);
            assert!(!target_current(
                &target,
                &successor,
                &targets(&successor, &p, 100)
            ));
        }
        let mut anonymous = original;
        anonymous.window_identity = None;
        assert!(!GeometryLease::default().observe(anonymous));
        let optout = Preferences {
            desktop_anchors: false,
            ..p
        };
        assert!(!target_current(
            &target,
            &sample(),
            &targets(&sample(), &optout, 100)
        ));
    }
    #[test]
    fn window_rest_transition_requires_the_exact_confirmed_move_completion() {
        use crate::life_domain::{
            reduce, ActorId, LifeEvent, NeedEffect, Needs, RandomState, WorldId,
        };
        let initial = LifeState::new(
            ActorId::new("nova").unwrap(),
            WorldId::new("world").unwrap(),
            Needs::new(5000, 5000, 5000, 5000).unwrap(),
        );
        let plan = reduce(
            &initial,
            0,
            RandomState::seeded(1),
            LifeEvent::Plan {
                activity: Activity::MoveTo,
                completion_effect: NeedEffect::default(),
            },
        )
        .unwrap();
        let id = plan.state.current_action().unwrap().intent().id().clone();
        let target = targets(&sample(), &prefs(), 100).remove(0);
        let mut pending = Pending {
            action: id.clone(),
            target: target.clone(),
            activity: Activity::MoveTo,
        };
        assert!(!confirmed_window_arrival(
            &plan.state,
            &pending,
            target.point,
            true
        ));
        let start = reduce(
            &plan.state,
            0,
            plan.random,
            LifeEvent::Start {
                action_id: id.clone(),
            },
        )
        .unwrap();
        assert!(!confirmed_window_arrival(
            &start.state,
            &pending,
            target.point,
            true
        ));
        let complete = reduce(
            &start.state,
            1000,
            start.random,
            LifeEvent::Complete { action_id: id },
        )
        .unwrap();
        assert!(confirmed_window_arrival(
            &complete.state,
            &pending,
            target.point,
            true
        ));
        assert!(!confirmed_window_arrival(
            &complete.state,
            &pending,
            target.point,
            false
        ));
        assert!(!confirmed_window_arrival(
            &complete.state,
            &pending,
            Point::new(0, 0).unwrap(),
            true
        ));
        let successor = reduce(
            &complete.state,
            0,
            complete.random,
            LifeEvent::Plan {
                activity: Activity::LookAt,
                completion_effect: NeedEffect::default(),
            },
        )
        .unwrap();
        assert!(!confirmed_window_arrival(
            &successor.state,
            &pending,
            target.point,
            true
        ));
        pending.target.kind = Kind::Cursor;
        assert!(!confirmed_window_arrival(
            &complete.state,
            &pending,
            target.point,
            true
        ));
        pending.target = target;
        pending.activity = Activity::Rest;
        assert!(
            !confirmed_window_arrival(&complete.state, &pending, pending.target.point, true),
            "a completed rest never loops into another rest"
        );
    }
}
