//! Actual Core -> World -> Store context for a constructor-bound Life window.
//! Unlike Studio preparation this derives authority from its explicit lease;
//! saved output selection is neither changed nor treated as a desktop grant.
use super::{
    action_state::{LocalFence, WorldActionFence},
    core::Core,
    native_document::AuthReceipt,
    native_layout::{LayoutHost, SurfaceCell},
    native_output_route::{LayoutRoute, LayoutRouteState, LayoutUi, PreparationTicket},
    session_output::ConstructorReceipt,
};
use crate::{
    overlay_presentation::{
        Bounds, LayoutError, LayoutHandle, LayoutMain, LayoutOwner, LayoutSample, NativeLayoutMain,
    },
    world_output::*,
    world_state::{coordinates::OutputGeometry, SessionStamp},
    AppState,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::Manager;

pub(super) struct SceneContext {
    pub constructor: ConstructorReceipt,
    pub auth: AuthReceipt,
    pub cancel: Arc<AtomicBool>,
    pub local: LocalFence,
    pub world: WorldActionFence,
    pub profile: Value,
    pub geometry: OutputGeometry,
    pub safe: crate::world_state::coordinates::Rect,
    pub profile_scale: f64,
    pub effective_scale: f64,
    pub walk: bool,
    ui: LayoutUi,
}
impl SceneContext {
    /// Caller holds Core; no GUI getter or native registry callback here.
    pub(super) fn read(
        core: &Core,
        app: &tauri::AppHandle,
        constructor: ConstructorReceipt,
        auth: AuthReceipt,
        cancel: Arc<AtomicBool>,
        ui: LayoutUi,
        walk: bool,
        now: u64,
    ) -> Result<Arc<Self>, LayoutError> {
        let state = app.state::<AppState>();
        if !constructor.lease_current()
            || !auth.current()
            || cancel.load(Ordering::SeqCst)
            || !Arc::ptr_eq(constructor.signals(), auth.document().signals())
            || auth.document().bot() != constructor.bot()
        {
            return Err(LayoutError::Stale);
        }
        let bot = constructor.bot();
        let local = core.local_fence(bot, now).ok_or(LayoutError::Stale)?;
        if core.local_action_reason(bot, now).is_some() {
            return Err(LayoutError::Busy);
        }
        let session = core.sessions.get(bot).ok_or(LayoutError::Stale)?;
        let life = session.state.as_ref().ok_or(LayoutError::Stale)?;
        let stamp = SessionStamp::new(
            crate::life_domain::ActorId::new(bot).map_err(|_| LayoutError::Stale)?,
            &session.session_id,
            session.revision,
        )
        .map_err(|_| LayoutError::Stale)?;
        let (world, safe) = state
            .world
            .registration_context(&state.store, &stamp, life.world_id())
            .map_err(|_| LayoutError::Stale)?;
        let geometry = state
            .world
            .native_output_geometry(&state.store, &world)
            .map_err(|_| LayoutError::Geometry)?;
        let profile = state
            .store
            .bot_profile(bot)
            .map_err(|_| LayoutError::Persistence)?;
        let profile_scale = profile["avatar"]["scale"].as_f64().unwrap_or(1.0);
        let effective_scale = state.presentation.snapshot(&profile)["scale"]
            .as_f64()
            .unwrap_or(profile_scale);
        if ![profile_scale, effective_scale]
            .into_iter()
            .all(|n| n.is_finite() && (0.3..=2.5).contains(&n))
        {
            return Err(LayoutError::Geometry);
        }
        Ok(Arc::new(Self {
            constructor,
            auth,
            cancel,
            local,
            world,
            profile,
            geometry,
            safe,
            profile_scale,
            effective_scale,
            walk,
            ui,
        }))
    }
    pub(super) fn current(&self, core: &Core, state: &AppState, now: u64) -> bool {
        if !self.constructor.lease_current()
            || !self.auth.current()
            || self.cancel.load(Ordering::SeqCst)
            || !core.local_current(self.constructor.bot(), &self.local, now)
            || !state
                .store
                .bot_profile(self.constructor.bot())
                .is_ok_and(|p| p == self.profile)
        {
            return false;
        }
        let Ok((mut world, _)) = state.world.registration_context(
            &state.store,
            &self.world.session,
            &self.world.source_world,
        ) else {
            return false;
        };
        let geometry_current = state
            .world
            .native_output_geometry(&state.store, &world)
            .is_ok_and(|g| {
                g.id() == self.geometry.id()
                    && g.physical_bounds() == self.geometry.physical_bounds()
                    && g.scale_factor().to_bits() == self.geometry.scale_factor().to_bits()
            });
        // Actual accepted movement changes only position. All host identities,
        // configuration, topology and session revisions remain frozen.
        world.position = self.world.position;
        world == self.world && geometry_current
    }
    pub(super) fn source(
        &self,
        core: &Core,
        state: &AppState,
        route: super::native_scene::NativeRouteProof,
        framing: NativePhysicalPolicy,
        now: u64,
    ) -> Result<NativeSource, String> {
        if !self.current(core, state, now) {
            return Err(super::native_scene::ERROR.into());
        }
        let (context, safe_region) = state.world.registration_context(
            &state.store,
            &self.world.session,
            &self.world.source_world,
        )?;
        Ok(NativeSource {
            route: SourceRoute::Native(route),
            context,
            profile_token: state
                .store
                .action_profile_token(self.constructor.bot(), &self.profile)
                .map_err(|_| super::native_scene::ERROR)?,
            ui_connection: String::new(),
            layout_token: json!([framing, self.profile["avatar"], self.profile["output"]]),
            viewport_limit: framing.viewport(),
            safe_region,
            profile_scale: self.profile_scale,
            effective_scale: self.effective_scale,
            native_walk: self.walk,
            owner: true,
            native_framing: Some(framing),
        })
    }
}
pub(super) struct SceneHost {
    pub app: tauri::AppHandle,
    pub expected: Arc<SceneContext>,
}
impl SceneHost {
    fn check(&self, cancel: &Arc<AtomicBool>, preparing: bool) -> Result<(), LayoutError> {
        let state = self.app.state::<AppState>();
        state.updater.ensure_idle().map_err(|_| LayoutError::Busy)?;
        if cancel.load(Ordering::SeqCst)
            || !state
                .life_runtime
                .session_outputs
                .constructor_current(&self.expected.constructor)
            || state.overlay.has_any_pending_speech()
            || state.generation.try_lock().is_err()
        {
            return Err(LayoutError::Stale);
        }
        let core = state
            .life_runtime
            .core
            .lock()
            .map_err(|_| LayoutError::Unavailable)?;
        if !self.expected.current(&core, &state, self.now())
            || cancel.load(Ordering::SeqCst)
            || preparing
                && core
                    .local_action_reason(self.expected.constructor.bot(), self.now())
                    .is_some()
            || !core.local_current(
                self.expected.constructor.bot(),
                &self.expected.local,
                self.now(),
            )
            || state.generation.try_lock().is_err()
        {
            return Err(LayoutError::Stale);
        }
        Ok(())
    }
}
impl LayoutHost for SceneHost {
    fn now(&self) -> u64 {
        self.app.state::<AppState>().life_runtime.now()
    }
    fn main(&self) -> Arc<dyn LayoutMain> {
        Arc::new(NativeLayoutMain(self.app.clone()))
    }
    fn refresh(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), LayoutError>> + Send + '_>>
    {
        Box::pin(async move {
            let state = self.app.state::<AppState>();
            let monitor = state
                .life_runtime
                .monitor
                .get()
                .ok_or(LayoutError::Unavailable)?;
            let sample = monitor.refresh().await;
            if sample.state != crate::life_availability::Availability::AwakeUnlocked
                || sample.generation != self.expected.local.system_generation
            {
                return Err(LayoutError::Stale);
            }
            Ok(())
        })
    }
    fn validate(&self, cancel: &Arc<AtomicBool>) -> Result<(), LayoutError> {
        self.check(cancel, true)
    }
    fn validate_live(&self, cancel: &Arc<AtomicBool>) -> Result<(), LayoutError> {
        self.check(cancel, false)
    }
    fn reserve(
        &self,
        handle: &LayoutHandle,
        sample: &LayoutSample,
        issued: u64,
        deadline: u64,
        cancel: Arc<AtomicBool>,
    ) -> Result<Arc<LayoutOwner>, LayoutError> {
        self.app
            .state::<AppState>()
            .presentation
            .reserve_native(handle, sample, issued, deadline, cancel)
    }
    fn persist(&self, bounds: Bounds, cancel: &Arc<AtomicBool>) -> Result<(), LayoutError> {
        self.check(cancel, true)?;
        // Session-only positioning must never replace a user's saved fixed
        // overlay bounds. The sole retained Hub Normal already owns restore.
        if !self.expected.constructor.persists_fixed_bounds() {
            return Ok(());
        }
        self.app
            .state::<AppState>()
            .store
            .scoped(self.expected.constructor.bot())
            .map_err(|_| LayoutError::Persistence)?
            .save_window_bounds(
                &serde_json::to_value(bounds).map_err(|_| LayoutError::Persistence)?,
            )
            .map_err(|_| LayoutError::Persistence)?;
        self.check(cancel, true)
    }
    fn attach(&self, cancel: &Arc<AtomicBool>) -> Result<(), LayoutError> {
        let state = self.app.state::<AppState>();
        state
            .life_runtime
            .session_outputs
            .bind_surface(&self.expected.constructor, cancel)
            .map_err(|_| LayoutError::Stale)?;
        state
            .life_runtime
            .monitor
            .get()
            .and_then(|m| m.gate.watch(cancel))
            .map(|_| ())
            .ok_or(LayoutError::Unavailable)
    }
    fn retain(
        &self,
        ticket: &PreparationTicket,
        handle: &LayoutHandle,
        cancel: &Arc<AtomicBool>,
    ) -> Result<LayoutRoute, LayoutError> {
        self.app
            .state::<AppState>()
            .life_runtime
            .native_routes
            .retain_preparation(
                ticket,
                handle.signals(),
                cancel,
                &self.expected.ui,
                self.now(),
            )
            .map_err(|_| LayoutError::Stale)
    }
    fn handoff(
        &self,
        ticket: &PreparationTicket,
        owner: &Arc<LayoutOwner>,
    ) -> Result<LayoutRoute, LayoutError> {
        self.app
            .state::<AppState>()
            .life_runtime
            .native_routes
            .handoff_preparation(
                ticket,
                owner.handle().signals(),
                &owner.cancel,
                &self.expected.ui,
                self.now(),
            )
            .map_err(|_| LayoutError::Stale)
    }
    fn route_state(&self, route: &LayoutRoute) -> LayoutRouteState {
        self.app
            .state::<AppState>()
            .life_runtime
            .native_routes
            .layout_state(route, self.now())
    }
    fn stage_surface(
        &self,
        route: &LayoutRoute,
        cell: &Arc<SurfaceCell>,
    ) -> Result<(), LayoutError> {
        self.app
            .state::<AppState>()
            .life_runtime
            .native_routes
            .stage_surface(route, cell, self.now())
            .map_err(|_| LayoutError::Stale)
    }
    fn release_route(&self, route: &LayoutRoute) -> bool {
        self.app
            .state::<AppState>()
            .life_runtime
            .native_routes
            .release_layout(route)
    }
    fn finish(&self, owner: &Arc<LayoutOwner>) -> bool {
        let state = self.app.state::<AppState>();
        let done = state.presentation.finish_native(owner);
        if done && owner.handle().current() {
            crate::overlay::configure_after_native_release(&self.app, owner.handle().clone());
            crate::overlay_presentation::sync_after_native_release(
                &self.app,
                owner.handle().bot(),
                owner.handle().signals().clone(),
            );
        }
        done
    }
    fn policy(&self) -> Option<crate::storage::profile_mutation::NativeLayoutPolicy> {
        self.app
            .state::<AppState>()
            .store
            .native_layout_policy(self.expected.constructor.bot())
    }
    fn monitor(&self) -> &str {
        self.expected.geometry.id()
    }
    fn save_current(&self, handle: &LayoutHandle, bounds: Bounds) {
        let state = self.app.state::<AppState>();
        if self.expected.constructor.persists_fixed_bounds()
            && handle.current()
            && !state.presentation.suppress_bounds(handle.bot())
        {
            if let Ok(store) = state.store.scoped(handle.bot()) {
                let _ =
                    store.save_window_bounds(&serde_json::to_value(bounds).unwrap_or(Value::Null));
            }
        }
    }
}
