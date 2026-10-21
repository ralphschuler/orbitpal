//! Native-only authority for an explicitly started desktop Life participant.
//! Public bot/session/window labels are correlation, never this capability.
use super::{
    action_state::LocalFence, core::Core, native_layout::NativeSurfaceProof,
    session_output::ConstructorReceipt,
};
use super::{
    native_document::AuthReceipt,
    native_layout::{LiveNativeSurface, NativeLayoutRestoreOutcome, PromotionGuard},
    native_scene_host::{SceneContext, SceneHost},
};
use crate::world_output::*;
use crate::{
    world_output::{OutputProof, SourceRoute},
    world_state::SessionStamp,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
pub(super) const ERROR:&str="Die Desktop-Bühne konnte nicht bestätigt werden. Prüfe Bildschirm, Figurengröße und Life-Status.";

/// Read-only route identity. Only the native scene coordinator can create one
/// from a retained constructor, an explicit session and a real layout owner.
#[derive(Clone)]
pub(crate) struct NativeRouteProof {
    constructor: ConstructorReceipt,
    local: LocalFence,
    session: SessionStamp,
    auth: super::native_document::AuthReceipt,
    scene: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    surface: NativeSurfaceProof,
}
impl NativeRouteProof {
    fn atoms_current(&self) -> bool {
        self.constructor.lease_current()
            && self.auth.current()
            && !self.scene.load(Ordering::SeqCst)
    }
    pub(crate) fn preparing_current(&self, now: u64) -> bool {
        self.atoms_current() && self.surface.preparing_current(now)
    }
    pub(crate) fn live_current(&self, now: u64) -> bool {
        self.atoms_current() && self.surface.live_current(now)
    }
    pub(crate) fn same_route(&self, other: &Self) -> bool {
        self.constructor.same_constructor(&other.constructor)
            && self.local == other.local
            && self.session == other.session
            && self.auth.same_auth(&other.auth)
            && Arc::ptr_eq(&self.scene, &other.scene)
            && Arc::ptr_eq(&self.released, &other.released)
            && self.surface.same_surface(&other.surface)
    }
    pub(crate) fn namespace(&self) -> &str {
        self.constructor.route_id()
    }
    pub(crate) fn retained(&self) -> bool {
        !self.released.load(Ordering::SeqCst)
    }
    pub(super) fn matches_auth(&self, auth: &AuthReceipt) -> bool {
        self.auth.same_auth(auth)
    }
    pub(super) fn constructor(&self) -> &ConstructorReceipt {
        &self.constructor
    }
    /// Pure/short retained checks only: called with Core -> World -> Store held.
    /// In particular this cannot inspect Core.actions (temporarily taken during
    /// executor operations), re-enter Store/World, or call a Wry getter.
    pub(crate) fn context_current(&self, core: &Core, now: u64) -> bool {
        core.local_current(self.constructor.bot(), &self.local, now)
            && core.sessions.get(self.constructor.bot()).is_some_and(|s| {
                SessionStamp::new(self.session.actor_id().clone(), &s.session_id, s.revision)
                    .is_ok_and(|stamp| stamp == self.session)
            })
    }
}

/// A Live route may authorize the shared executor. Pending layout/calibration
/// can never construct this permit; its factory remains in this module.
#[derive(Clone)]
pub(crate) struct NativeActionPermit {
    route: NativeRouteProof,
}
impl NativeActionPermit {
    pub(crate) fn current(&self, core: &Core, now: u64) -> bool {
        self.route.live_current(now) && self.route.context_current(core, now)
    }
    pub(crate) fn matches_output(&self, output: &OutputProof) -> bool {
        matches!(&output.route, SourceRoute::Native(route) if self.route.same_route(route))
            && output.binding.actor_id == self.route.constructor.bot()
    }
}

pub(super) struct OwnedSurface {
    pub context: Arc<SceneContext>,
    pub route: NativeRouteProof,
    pub framing: NativePhysicalPolicy,
    pub preparation_deadline: u64,
    promotion: Option<PromotionGuard>,
    live: Option<LiveNativeSurface>,
    observed_ready: bool,
}
impl OwnedSurface {
    /// Actual retained Live geometry; preparing/unmeasured owners supply none.
    pub(super) fn live_bounds(&self, now:u64)->Option<crate::overlay_presentation::Bounds> {
        let live=self.live.as_ref()?;
        (self.observed_ready && live.current(now)).then_some(live.bounds)
    }
    /// Registry accepted a genuinely newer frame immediately before this call.
    /// The first calibration promotes; only a later idle renewal may admit an
    /// initial action, after the renderer has received its Ready binding.
    pub(super) fn accepted(&mut self, frame: u64, received: u64, now: u64) -> Result<bool, String> {
        if let Some(pending) = self.promotion.take() {
            self.live = Some(
                pending
                    .commit_live(frame, received, now)
                    .map_err(|_| ERROR)?,
            );
            Ok(false)
        } else if let Some(live) = &self.live {
            live.accepted_frame(frame, received, now)
                .map_err(|_| ERROR)?;
            self.observed_ready = true;
            Ok(true)
        } else {
            Err(ERROR.into())
        }
    }
    async fn release(self) -> NativeLayoutRestoreOutcome {
        if let Some(live) = self.live {
            return live.release().await;
        }
        if let Some(pending) = self.promotion {
            return pending.release().await;
        }
        NativeLayoutRestoreOutcome::GuardedUnavailable
    }
    pub(super) fn action_permit(&self, now: u64) -> Option<NativeActionPermit> {
        (self.observed_ready && self.route.live_current(now)).then(|| NativeActionPermit {
            route: self.route.clone(),
        })
    }
    pub(super) fn waiting_for_calibration(&self, now: u64) -> bool {
        self.promotion.is_some()
            && now <= self.preparation_deadline
            && !self.route.preparing_current(now)
    }
}
pub(super) struct NativeMember {
    pub constructor: ConstructorReceipt,
    pub auth: AuthReceipt,
    pub cancel: Arc<AtomicBool>,
    pub operation: String,
    pub sequence: u64,
    pub owned: Mutex<Option<OwnedSurface>>,
    pub(super) dispatch: super::native_scene_dispatch::DispatchQueue,
    pub binding: Mutex<Option<(String, u64)>>,
    preparing: AtomicBool,
    failed: AtomicBool,
    releasing: AtomicBool,
    released: Arc<AtomicBool>,
    release_result: Mutex<Option<NativeLayoutRestoreOutcome>>,
    done: tokio::sync::Notify,
}
impl NativeMember {
    pub(super) fn cleanup_released(&self) -> bool {
        self.released.load(Ordering::SeqCst)
    }
    pub(super) fn current(&self) -> bool {
        self.constructor.lease_current()
            && self.auth.current()
            && !self.cancel.load(Ordering::SeqCst)
    }
    pub(super) fn awaiting(&self) -> RegisteredWorldActor {
        RegisteredWorldActor {
            version: 1,
            render_session_id: self.operation.clone(),
            binding_sequence: self.sequence,
            surface_id: self.operation.clone(),
            role: RenderRole::OwnerCandidate,
            readiness: if self.failed.load(Ordering::SeqCst) || !self.current() {
                Readiness::Unavailable {
                    reason: RenderReason::Cancelled,
                }
            } else {
                Readiness::AwaitingLayout
            },
        }
    }
    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.done.notify_waiters();
    }
}
#[derive(Default)]
struct NativeSceneState {
    serial: u64,
    members: BTreeMap<String, Arc<NativeMember>>,
}
#[derive(Default)]
pub(super) struct NativeScenes {
    state: Mutex<NativeSceneState>,
}
impl NativeScenes {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, NativeSceneState>, String> {
        self.state.lock().map_err(|poison| {
            for member in poison.into_inner().members.values() {
                member.cancel();
            }
            ERROR.to_owned()
        })
    }
    pub(super) fn find(&self, auth: &AuthReceipt) -> Option<Arc<NativeMember>> {
        self.lock()
            .ok()?
            .members
            .get(auth.document().bot())
            .filter(|m| m.auth.same_auth(auth) && m.current())
            .cloned()
    }
    pub(super) fn for_constructor(
        &self,
        constructor: &ConstructorReceipt,
    ) -> Option<Arc<NativeMember>> {
        self.lock()
            .ok()?
            .members
            .get(constructor.bot())
            .filter(|m| m.constructor.same_constructor(constructor))
            .cloned()
    }
    /// Only called after a genuine document warm and exact native constructor
    /// receipt. One active explicit participant per bot, eight in total.
    pub(super) fn begin(
        &self,
        constructor: ConstructorReceipt,
        auth: AuthReceipt,
    ) -> Result<(Arc<NativeMember>, bool), String> {
        if !constructor.lease_current()
            || !auth.current()
            || constructor.bot() != auth.document().bot()
            || !Arc::ptr_eq(constructor.signals(), auth.document().signals())
        {
            return Err(ERROR.into());
        }
        let mut state = self.lock()?;
        if !constructor.lease_current() || !auth.current() {
            return Err(ERROR.into());
        }
        if let Some(old) = state.members.get(constructor.bot()) {
            if old.auth.same_auth(&auth)
                && old.constructor.same_constructor(&constructor)
                && old.current()
            {
                return Ok((old.clone(), false));
            }
            // Cancellation requests alone cannot release retained physical work.
            if old.current()
                || !old.released.load(Ordering::SeqCst)
                || old.preparing.load(Ordering::SeqCst)
                || old.owned.lock().map(|v| v.is_some()).unwrap_or(true)
            {
                return Err(ERROR.into());
            }
        }
        state.members.retain(|_, m| {
            !m.released.load(Ordering::SeqCst)
                || m.current()
                || m.preparing.load(Ordering::SeqCst)
                || m.owned.lock().map(|v| v.is_some()).unwrap_or(true)
        });
        if state.members.len() >= MAX_LAYERS || state.serial >= crate::life_domain::MAX_COUNTER {
            return Err(ERROR.into());
        }
        state.serial += 1;
        let member = Arc::new(NativeMember {
            constructor,
            auth,
            cancel: Arc::new(AtomicBool::new(false)),
            operation: uuid::Uuid::new_v4().to_string(),
            sequence: state.serial,
            owned: Mutex::new(None),
            dispatch: super::native_scene_dispatch::DispatchQueue::default(),
            binding: Mutex::new(None),
            preparing: AtomicBool::new(true),
            failed: AtomicBool::new(false),
            releasing: AtomicBool::new(false),
            released: Arc::new(AtomicBool::new(false)),
            release_result: Mutex::new(None),
            done: tokio::sync::Notify::new(),
        });
        state
            .members
            .insert(member.constructor.bot().into(), member.clone());
        Ok((member, true))
    }
    pub(super) fn cancel(&self, bot: Option<&str>) {
        if let Ok(state) = self.lock() {
            for (id, m) in &state.members {
                if bot.is_none_or(|b| b == id) {
                    m.cancel();
                }
            }
        }
    }
    pub(super) fn members(&self, bot: Option<&str>) -> Vec<Arc<NativeMember>> {
        self.lock()
            .map(|state| {
                state
                    .members
                    .iter()
                    .filter(|(id, _)| bot.is_none_or(|b| b == id.as_str()))
                    .map(|(_, m)| m.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    /// The asset receipt is one-use and belongs to the exact physical page,
    /// original delivery and lease. Holding Scene -> Documents prevents a
    /// concurrent registration from crossing the last physical cleanup check.
    pub(super) fn rotate_for_attempt(
        &self,
        documents: &super::native_document::Documents,
        constructor: &ConstructorReceipt,
        ready: super::native_document::AssetReadyReceipt,
        now: u64,
    ) -> Result<super::native_document::DocumentGrant, String> {
        let state = self.lock()?;
        if !constructor.lease_current() || !ready.current(now)
            || ready.loaded().bot() != constructor.bot()
            || !Arc::ptr_eq(ready.loaded().signals(), constructor.signals())
            || !Arc::ptr_eq(&ready.lease_cancel(), &constructor.lease_cancel())
            || state.members.get(constructor.bot()).is_some_and(|m| {
                !m.cleanup_released() || m.current() || m.preparing.load(Ordering::SeqCst)
                    || m.owned.lock().map(|owned| owned.is_some()).unwrap_or(true)
            })
        { return Err(ERROR.into()); }
        documents.rotate_for_attempt(ready, now).map_err(|_| ERROR.into())
    }
    /// Serializes the final idle check with native registration. Documents has
    /// no callback into Scene/Core, so this short Scene -> Documents order is
    /// independent of the Owned -> Core -> World -> Store execution path.
    pub(super) fn rotate_document_if_idle(
        &self,
        documents: &super::native_document::Documents,
        constructor: &ConstructorReceipt,
        now: u64,
    ) -> Result<super::native_document::DocumentGrant, String> {
        let state = self.lock()?;
        if !constructor.lease_current()
            || state.members.get(constructor.bot()).is_some_and(|m| {
                !m.cleanup_released() || m.current() || m.preparing.load(Ordering::SeqCst)
            })
        {
            return Err(ERROR.into());
        }
        documents
            .rotate(constructor.bot(), constructor.signals(), now)
            .map_err(|_| ERROR.into())
    }
}

/// Exactly one async layout preparation for the member. A failed/abandoned
/// task never leaves the member apparently preparing or its Normal unowned.
struct PreparationDone(Arc<NativeMember>);
impl Drop for PreparationDone {
    fn drop(&mut self) {
        self.0.preparing.store(false, Ordering::SeqCst);
        self.0.done.notify_waiters();
    }
}
pub(super) fn prepare(app: tauri::AppHandle, member: Arc<NativeMember>) {
    tauri::async_runtime::spawn(async move {
        let _done = PreparationDone(member.clone());
        let failed = prepare_inner(&app, &member).await.is_err();
        if failed {
            member.failed.store(true, Ordering::SeqCst);
            member.cancel();
            let _ = app.emit_to(
                "main",
                "cohost://agent",
                serde_json::json!({"kind":"error","botId":member.constructor.bot(),"text":ERROR}),
            );
        }
        drop(_done);
        if failed {
            let _ = release_exact(&app, member).await;
        }
    });
}
async fn prepare_inner(app: &tauri::AppHandle, member: &Arc<NativeMember>) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    if !member.current()
        || !state
            .life_runtime
            .session_outputs
            .constructor_current(&member.constructor)
    {
        return Err(ERROR.into());
    }
    // Immutable D5 starts before monitor, profile, World or Store read-through.
    let issued = state.life_runtime.now();
    let handle = state
        .presentation
        .native_handle(member.constructor.bot())
        .map_err(|_| ERROR)?;
    if !Arc::ptr_eq(handle.signals(), member.constructor.signals()) {
        return Err(ERROR.into());
    }
    let witness = state
        .life_runtime
        .native_routes
        .witness(
            member.constructor.bot(),
            member.constructor.window().label(),
            member.constructor.signals(),
            issued,
        )
        .map_err(|_| ERROR)?;
    let ticket = state
        .life_runtime
        .native_routes
        .begin_preparation(witness, issued)
        .map_err(|_| ERROR)?;
    let ui = state
        .life_runtime
        .native_routes
        .session_layout(&member.cancel)
        .map_err(|_| ERROR)?;
    let profile = state.store.bot_profile(member.constructor.bot())?;
    let walk = super::world_output::native_walk(app, &profile);
    let topology = state.world.sample_topology(app).await;
    let expected = {
        let core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
        state.world.synchronize(
            &state.store,
            &crate::world_hub::current_actors(&core),
            Some(topology),
            state.life_runtime.now(),
        )?;
        SceneContext::read(
            &core,
            app,
            member.constructor.clone(),
            member.auth.clone(),
            member.cancel.clone(),
            ui,
            walk,
            state.life_runtime.now(),
        )
        .map_err(|_| ERROR)?
    };
    if state.life_runtime.now() > ticket.deadline() {
        return Err(ERROR.into());
    }
    let preparation_deadline = ticket.deadline();
    let host = Arc::new(SceneHost {
        app: app.clone(),
        expected: expected.clone(),
    });
    let prepared = super::native_layout::prepare_layout(
        super::native_layout::NativeLayoutIntent::new(host, ticket, handle),
    )
    .await
    .map_err(|_| ERROR)?;
    let bounds = prepared.bounds;
    let geometry = expected.geometry.physical_bounds();
    if (bounds.x, bounds.y, bounds.width, bounds.height) != geometry
        || prepared.scale.to_bits() != expected.geometry.scale_factor().to_bits()
    {
        let _ = prepared.release().await;
        return Err(ERROR.into());
    }
    let framing = NativePhysicalPolicy::new(
        prepared.reference_physical_height,
        bounds.width,
        bounds.height,
        prepared.scale,
        if expected.profile["avatar"]["camera"] == "full" {
            NormalCamera::Full
        } else {
            NormalCamera::Portrait
        },
    )
    .map_err(|_| ERROR)?;
    let guard = prepared
        .begin_promotion(state.life_runtime.now())
        .map_err(|_| ERROR)?;
    let route = NativeRouteProof {
        constructor: member.constructor.clone(),
        local: expected.local.clone(),
        session: expected.world.session.clone(),
        auth: member.auth.clone(),
        scene: member.cancel.clone(),
        released: member.released.clone(),
        surface: guard.proof(),
    };
    {
        let core = state.life_runtime.core.lock().map_err(|_| ERROR)?;
        if !member.current() || !expected.current(&core, &state, state.life_runtime.now()) {
            return Err(ERROR.into());
        }
    }
    let mut owned = member.owned.lock().map_err(|_| ERROR)?;
    if !member.current() || owned.is_some() {
        return Err(ERROR.into());
    }
    *owned = Some(OwnedSurface {
        context: expected,
        route,
        framing,
        preparation_deadline,
        promotion: Some(guard),
        live: None,
        observed_ready: false,
    });
    #[cfg(feature="native-world-fixture")]
    crate::native_world_fixture::observe_prepared(app, member.constructor.bot(),
        member.constructor.signals(), member.sequence, member.auth.normal_warm_frame_sequence());
    Ok(())
}

/// Cancel before acquiring Core, await exact owned cleanup, then verify the
/// actual retained Hub owner drained. No Core/World/Store/GUI guard crosses await.
pub(super) async fn quiesce(app: &tauri::AppHandle, bot: Option<&str>) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let members = state.life_runtime.native_scenes.members(bot);
    for member in &members {
        member.cancel();
    }
    state.life_runtime.native_routes.revoke(bot);
    for member in members {
        release_exact(app, member).await?;
    }
    Ok(())
}
/// Background cleanup owns the exact member even if the waiting command future
/// is dropped. Its retained charge ends only at actual terminal restoration.
pub(super) async fn release_exact(
    app: &tauri::AppHandle,
    member: Arc<NativeMember>,
) -> Result<(), String> {
    member.cancel();
    if !member.releasing.swap(true, Ordering::SeqCst) {
        let handle = app.clone();
        let exact = member.clone();
        tauri::async_runtime::spawn(async move {
            let result = release_member(&handle, &exact)
                .await
                .unwrap_or(NativeLayoutRestoreOutcome::GuardedUnavailable);
            if result != NativeLayoutRestoreOutcome::GuardedUnavailable {
                exact.released.store(true, Ordering::SeqCst);
            }
            if let Ok(mut stored) = exact.release_result.lock() {
                *stored = Some(result);
            }
            #[cfg(feature="native-world-fixture")]
            crate::native_world_fixture::observe_cleanup(&handle, exact.constructor.bot(),
                exact.constructor.signals(), exact.sequence, match result {
                    NativeLayoutRestoreOutcome::Restored => crate::native_world_fixture::FixtureRestore::Restored,
                    NativeLayoutRestoreOutcome::Retired => crate::native_world_fixture::FixtureRestore::Retired,
                    NativeLayoutRestoreOutcome::ReleasedWithoutGeometry => crate::native_world_fixture::FixtureRestore::WithoutGeometry,
                    NativeLayoutRestoreOutcome::GuardedUnavailable => crate::native_world_fixture::FixtureRestore::Guarded,
                });
            exact.done.notify_waiters();
        });
    }
    loop {
        let notified = member.done.notified();
        if let Some(result) = *member.release_result.lock().map_err(|_| ERROR)? {
            return if result == NativeLayoutRestoreOutcome::GuardedUnavailable {
                Err(ERROR.into())
            } else {
                Ok(())
            };
        }
        notified.await;
    }
}
async fn release_member(
    app: &tauri::AppHandle,
    member: &Arc<NativeMember>,
) -> Result<NativeLayoutRestoreOutcome, String> {
    let state = app.state::<crate::AppState>();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    revoke_view(app, member.clone()).await;
    while member.preparing.load(Ordering::SeqCst) {
        let notified = member.done.notified();
        if !member.preparing.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::timeout_at(deadline, notified)
            .await
            .map_err(|_| ERROR)?;
    }
    let owned = member.owned.lock().map_err(|_| ERROR)?.take();
    let result = if let Some(owned) = owned {
        tokio::time::timeout_at(deadline, owned.release())
            .await
            .map_err(|_| ERROR)?
    } else {
        NativeLayoutRestoreOutcome::ReleasedWithoutGeometry
    };
    if result == NativeLayoutRestoreOutcome::GuardedUnavailable {
        return Ok(result);
    }
    while state
        .presentation
        .current_window(member.constructor.bot(), member.constructor.signals())
        && state.presentation.native_guarded(member.constructor.bot())
    {
        if tokio::time::Instant::now() >= deadline {
            return Err(ERROR.into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(result)
}
async fn revoke_view(app: &tauri::AppHandle, member: Arc<NativeMember>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    if app
        .run_on_main_thread(move || {
            let state = handle.state::<crate::AppState>();
            if !member.cleanup_released()
                && member.auth.document().current()
                && state
                    .presentation
                    .current_window(member.constructor.bot(), member.constructor.signals())
            {
                // Best-effort client invalidation only. This is neither a renderer
                // cleared-layer receipt nor native Normal restoration evidence.
                let _ = member
                    .constructor
                    .window()
                    .eval("window.__orbitpalNativeWorldRevoke?.()");
            }
            let _ = tx.send(());
        })
        .is_ok()
    {
        let _ = tokio::time::timeout(Duration::from_secs(1), rx).await;
    }
}

/// A scheduling hint built only from an actually idle, fresh native renderer.
/// It authorizes no dialog or playback: the generation turn must still cancel
/// and restore every owner before strict PresentationHub::admit_dialog succeeds.
pub(crate) struct NativeDialogueHandoff { route: NativeRouteProof }
impl NativeDialogueHandoff {
    pub(crate) fn matches_owner(&self, owner: &Arc<crate::overlay_presentation::LayoutOwner>, now: u64) -> bool {
        self.route.atoms_current() && self.route.surface.owned_by(owner)
            && self.route.surface.live_current(now) && !owner.in_flight()
    }
}
pub(super) fn blocks_dialog_scheduling(app: &tauri::AppHandle) -> bool {
    let state = app.state::<crate::AppState>();
    let mut receipts = Vec::new();
    for member in state.life_runtime.native_scenes.members(None) {
        // Never take Owned after Core: the measurement path owns Owned -> Core.
        let candidate = member.owned.lock().ok().and_then(|owned| owned.as_ref()
            .filter(|o| o.observed_ready && member.current() && o.route.live_current(state.life_runtime.now()))
            .map(|o| (o.context.clone(), o.route.clone())));
        let Some((context, route)) = candidate else { continue; };
        if !state.life_runtime.session_outputs.constructor_current(&member.constructor) { continue; }
        let Ok(core) = state.life_runtime.core.lock() else { return true; };
        if core.local_actions_busy() || !context.current(&core, &state, state.life_runtime.now())
            || !member.current() || !route.live_current(state.life_runtime.now())
        { continue; }
        receipts.push(NativeDialogueHandoff { route });
    }
    state.presentation.blocks_dialog_scheduling(&receipts, state.life_runtime.now())
}

/// Read-only observation pairing for this nonshipping fixture's synchronous
/// main-window Stop IPC. No receipt may stop, restore or acknowledge anything.
#[cfg(feature="native-world-fixture")]
pub(crate) struct FixtureStopReceipt { member: Arc<NativeMember>, session: SessionStamp }
#[cfg(feature="native-world-fixture")]
pub(crate) fn fixture_stop_receipts(app: &tauri::AppHandle) -> Vec<FixtureStopReceipt> {
    let state=app.state::<crate::AppState>();
    state.life_runtime.native_scenes.members(None).into_iter().filter_map(|member| {
        if member.cleanup_released() || !state.life_runtime.session_outputs.constructor_current(&member.constructor) { return None; }
        let owned=member.owned.lock().ok()?;
        let surface=owned.as_ref()?;
        let core=state.life_runtime.core.lock().ok()?;
        let current=core.sessions.get(member.constructor.bot())?;
        let stamp=SessionStamp::new(crate::life_domain::ActorId::new(member.constructor.bot()).ok()?,
            &current.session_id, current.revision).ok()?;
        if surface.route.session != stamp { return None; }
        Some(FixtureStopReceipt { member: member.clone(), session: stamp })
    }).collect()
}
#[cfg(feature="native-world-fixture")]
pub(crate) fn observe_fixture_stopped(app: &tauri::AppHandle, receipts: Vec<FixtureStopReceipt>) {
    let state=app.state::<crate::AppState>();
    for receipt in receipts {
        let stopped=state.life_runtime.core.lock().is_ok_and(|core|core.sessions
            .get(receipt.member.constructor.bot()).is_none_or(|session|
                SessionStamp::new(receipt.session.actor_id().clone(), &session.session_id, session.revision)
                    .is_ok_and(|stamp| stamp != receipt.session)));
        if stopped { crate::native_world_fixture::observe_stop(app,
            receipt.member.constructor.bot(), receipt.member.constructor.signals(), receipt.member.sequence); }
    }
}
