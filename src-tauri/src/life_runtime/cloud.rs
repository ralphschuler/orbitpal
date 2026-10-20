//! Native-only admission for each autonomous network/CLI dispatch. A provider
//! name or loopback URL is never evidence that a request stays on this machine.
use super::{core::Claim, UNAVAILABLE};
use crate::life_availability::{AvailabilityGate, Lease};
use crate::usage_budget::Origin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::Manager;

#[derive(Clone)]
pub(crate) struct CloudAuthority {
    app: Option<tauri::AppHandle>,
    origin: Origin,
    bot_id: String,
    cancel: Arc<AtomicBool>,
    claim: Option<Claim>,
    #[cfg(test)]
    synthetic: Option<Arc<AvailabilityGate>>,
    #[cfg(test)]
    pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}
pub(crate) struct CloudPermit {
    gate: Option<(Arc<AvailabilityGate>, Lease)>,
    cancel: Arc<AtomicBool>,
    session: Option<(tauri::AppHandle, Claim)>,
}
impl CloudPermit {
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst)
            || self
                .gate
                .as_ref()
                .is_some_and(|(gate, lease)| !gate.current(*lease))
        {
            return Err(UNAVAILABLE.into());
        }
        if let Some((app, claim)) = &self.session {
            let state = app.state::<crate::AppState>();
            let core = state.life_runtime.core.lock().map_err(|_| UNAVAILABLE)?;
            if !core.session_cloud_current(claim, state.life_runtime.now()) {
                return Err(UNAVAILABLE.into());
            }
        }
        Ok(())
    }
}
impl CloudAuthority {
    pub(crate) fn native(
        app: &tauri::AppHandle,
        context: &crate::usage_budget::Context,
        cancel: Arc<AtomicBool>,
        claim: Option<Claim>,
    ) -> Self {
        Self {
            app: Some(app.clone()),
            origin: context.origin,
            bot_id: context.bot_id.clone(),
            cancel,
            claim,
            #[cfg(test)]
            synthetic: None,
            #[cfg(test)]
            pause: None,
        }
    }
    pub(crate) fn matches(
        &self,
        context: &crate::usage_budget::Context,
        cancel: &Arc<AtomicBool>,
    ) -> bool {
        self.origin == context.origin
            && self.bot_id == context.bot_id
            && Arc::ptr_eq(&self.cancel, cancel)
            && self
                .claim
                .as_ref()
                .is_none_or(|claim| claim.bot_id == context.bot_id)
    }
    pub(crate) async fn admit(&self) -> Result<CloudPermit, String> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(UNAVAILABLE.into());
        }
        if !self.origin.autonomous() {
            return Ok(CloudPermit {
                gate: None,
                cancel: self.cancel.clone(),
                session: None,
            });
        }
        #[cfg(test)]
        if let Some((entered, release)) = &self.pause {
            entered.notify_one();
            release.notified().await;
        }
        #[cfg(test)]
        if let Some(gate) = &self.synthetic {
            let lease = gate.watch(&self.cancel).ok_or(UNAVAILABLE)?;
            let permit = CloudPermit {
                gate: Some((gate.clone(), lease)),
                cancel: self.cancel.clone(),
                session: None,
            };
            permit.check()?;
            return Ok(permit);
        }
        let app = self.app.as_ref().ok_or(UNAVAILABLE)?;
        let state = app.state::<crate::AppState>();
        state
            .life_runtime
            .network_permit(app, self.claim.as_ref(), self.cancel.clone())
            .await
    }
    #[cfg(test)]
    pub(crate) fn deferred_fixture(
        context: &crate::usage_budget::Context,
        cancel: Arc<AtomicBool>,
    ) -> (Self, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
        let mut authority = Self::fixture(context, cancel);
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        authority.pause = Some((entered.clone(), release.clone()));
        (authority, entered, release)
    }
    #[cfg(test)]
    pub(crate) fn fixture(context: &crate::usage_budget::Context, cancel: Arc<AtomicBool>) -> Self {
        use crate::life_availability::{Observation, Source};
        let gate = Arc::new(AvailabilityGate::new(Source::Unsupported));
        let fence = gate.sample_fence();
        gate.apply_sample(fence, Observation::ready());
        Self {
            app: None,
            origin: context.origin,
            bot_id: context.bot_id.clone(),
            cancel,
            claim: None,
            synthetic: Some(gate),
            pause: None,
        }
    }
}
impl super::LifeRuntime {
    async fn network_permit(
        &self,
        app: &tauri::AppHandle,
        claim: Option<&Claim>,
        cancel: Arc<AtomicBool>,
    ) -> Result<CloudPermit, String> {
        let monitor = self.monitor.get().ok_or(UNAVAILABLE)?;
        let system = monitor.refresh().await;
        // Native window getters may wait for the main loop: never hold Core.
        let is_visible = super::visible(app);
        let route = claim.map(|claim| {
            app.state::<crate::AppState>()
                .store
                .bot_profile(&claim.bot_id)
                .ok()
                .is_some_and(|profile| super::output_available(app, &profile))
        });
        {
            let mut core = self.core.lock().map_err(|_| UNAVAILABLE)?;
            let now = self.now();
            core.observe_system(system, now);
            core.reconcile_visibility(is_visible, now);
            if let (Some(claim), Some(available)) = (claim, route) {
                core.set_output_available(
                    &claim.bot_id,
                    &claim.session_id,
                    claim.revision,
                    available,
                    now,
                );
            }
            if cancel.load(Ordering::SeqCst)
                || system.state != crate::life_availability::Availability::AwakeUnlocked
                || route == Some(false)
                || claim.is_some_and(|claim| !core.session_cloud_current(claim, now))
            {
                return Err(UNAVAILABLE.into());
            }
        }
        app.state::<crate::AppState>().updater.ensure_idle()?;
        let lease = monitor.gate.watch(&cancel).ok_or(UNAVAILABLE)?;
        let permit = CloudPermit {
            gate: Some((monitor.gate.clone(), lease)),
            cancel,
            session: claim.cloned().map(|claim| (app.clone(), claim)),
        };
        permit.check()?;
        Ok(permit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn synthetic_native_lease_revokes_waiting_transport_and_never_grants_unknown() {
        let cancel = Arc::new(AtomicBool::new(false));
        let authority = CloudAuthority::fixture(
            &crate::usage_budget::Context::new("nova", Origin::AutomaticChat),
            cancel.clone(),
        );
        let permit = authority.admit().await.unwrap();
        permit.check().unwrap();
        authority
            .synthetic
            .as_ref()
            .unwrap()
            .block(crate::life_availability::Reason::Locked);
        assert!(cancel.load(Ordering::SeqCst));
        assert!(permit.check().is_err());
        let unknown = CloudAuthority::fixture(
            &crate::usage_budget::Context::new("nova", Origin::Autopilot),
            Arc::new(AtomicBool::new(false)),
        );
        unknown
            .synthetic
            .as_ref()
            .unwrap()
            .block(crate::life_availability::Reason::ObservationUnavailable);
        assert!(unknown.admit().await.is_err());
    }
    #[tokio::test]
    async fn manual_admission_needs_no_platform_but_cancellation_still_wins() {
        let cancel = Arc::new(AtomicBool::new(false));
        let authority = CloudAuthority::fixture(
            &crate::usage_budget::Context::new("nova", Origin::Manual),
            cancel.clone(),
        );
        authority.synthetic.as_ref().unwrap().stop();
        let permit = authority.admit().await.unwrap();
        permit.check().unwrap();
        cancel.store(true, Ordering::SeqCst);
        assert!(permit.check().is_err());
        assert!(authority.admit().await.is_err());
    }
}
