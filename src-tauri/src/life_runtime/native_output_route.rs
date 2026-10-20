//! Native-window identity and preparation cancellation only.
//!
//! There is deliberately no Prepared/Ready state, renderer registration, serde
//! implementation or conversion to World/action authority. A later real layout
//! and renderer adapter must supply those proofs. Callbacks acquire only this
//! short lock, before Core/World/Store/GUI work, and never call caller code.
use crate::{life_domain::MAX_COUNTER, window_signals::WindowSignals};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, Weak,
    },
};

const MAX_WINDOWS: usize = 64;
const MAX_INTENTS: usize = 8;
const PREPARATION_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RouteError {
    InvalidScope,
    StaleWindow,
    Busy,
    Expired,
    Capacity,
    Unavailable,
}
#[derive(Clone, Copy)]
pub(crate) enum NativeWindowChange {
    Geometry,
    Scale,
    Hidden,
    Closing,
    Destroyed,
}

/// Native-only evidence of a current window instance/route revision. Not a
/// rendering, session, membership, layout or action capability.
pub(super) struct WindowWitness {
    bot: String,
    instance: String,
    revision: u64,
    signals: Weak<WindowSignals>,
    cancel: Arc<AtomicBool>,
}
pub(super) struct PreparationTicket {
    window: WindowWitness,
    id: u64,
    issued_at: u64,
    deadline: u64,
    cancel: Arc<AtomicBool>,
}
impl Drop for PreparationTicket {
    fn drop(&mut self) {
        // Dropping an async caller never keeps its pending slot busy beyond
        // the next bounded registry operation; no lock is taken from Drop.
        self.cancel.store(true, Ordering::SeqCst);
    }
}
struct Pending {
    id: u64,
    deadline: u64,
    cancel: Arc<AtomicBool>,
}
struct Window {
    instance: String,
    revision: u64,
    signals: Weak<WindowSignals>,
    cancel: Arc<AtomicBool>,
    pending: Option<Pending>,
}
impl Window {
    fn cancel(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(pending) = self.pending.take() {
            pending.cancel.store(true, Ordering::SeqCst);
        }
    }
    fn same_instance(&self, signals: &Arc<WindowSignals>) -> bool {
        self.signals
            .upgrade()
            .is_some_and(|current| Arc::ptr_eq(&current, signals))
    }
    fn matches(&self, witness: &WindowWitness) -> bool {
        self.instance == witness.instance
            && self.revision == witness.revision
            && Weak::ptr_eq(&self.signals, &witness.signals)
            && self.signals.strong_count() > 0
            && Arc::ptr_eq(&self.cancel, &witness.cancel)
            && !witness.cancel.load(Ordering::SeqCst)
    }
}
#[derive(Default)]
struct State {
    serial: u64,
    windows: BTreeMap<String, Window>,
}
impl State {
    fn next(&mut self) -> Result<u64, RouteError> {
        if self.serial >= MAX_COUNTER {
            // Exhaustion cannot mint an aliased epoch or leave a prior proof
            // usable. Only restarting the process can issue new counters.
            for window in self.windows.values_mut() {
                window.cancel();
            }
            return Err(RouteError::Unavailable);
        }
        self.serial += 1;
        Ok(self.serial)
    }
    fn reap(&mut self, now: u64) {
        self.windows.retain(|_, window| {
            if window.signals.strong_count() == 0 {
                window.cancel();
                return false;
            }
            if window
                .pending
                .as_ref()
                .is_some_and(|pending| pending.cancel.load(Ordering::SeqCst) || now > pending.deadline)
            {
                if let Some(pending) = window.pending.take() {
                    pending.cancel.store(true, Ordering::SeqCst);
                }
            }
            true
        });
    }
}
#[derive(Default)]
pub(super) struct NativeOutputRoutes {
    state: Mutex<State>,
}
impl Drop for NativeOutputRoutes {
    fn drop(&mut self) {
        let state = self.state.get_mut().unwrap_or_else(|poison| poison.into_inner());
        for window in state.windows.values_mut() {
            window.cancel();
        }
    }
}
impl NativeOutputRoutes {
    /// Poison may deny work, but must not leave previously issued cancellation
    /// atoms apparently live. Recovery is cancellation-only: poison is never
    /// cleared and this path returns no state guard or successor identity.
    fn lock(&self) -> Result<MutexGuard<'_, State>, RouteError> {
        match self.state.lock() {
            Ok(state) => Ok(state),
            Err(poison) => {
                let mut state = poison.into_inner();
                for window in state.windows.values_mut() {
                    window.cancel();
                }
                Err(RouteError::Unavailable)
            }
        }
    }
    /// Called only by the real native overlay constructor. Repeating the SAME
    /// Arc is idempotent; replacing it cancels the retired instance first.
    pub(super) fn track_window(
        &self,
        bot: &str,
        label: &str,
        signals: &Arc<WindowSignals>,
        now: u64,
    ) -> Result<(), RouteError> {
        if !crate::storage::valid_id(bot) || label != format!("avatar-{bot}") {
            return Err(RouteError::InvalidScope);
        }
        let mut state = self.lock()?;
        state.reap(now);
        if state.serial >= MAX_COUNTER {
            // Use the same terminal cancellation path as every other counter
            // admission, including a ticket minted at the final counter.
            return state.next().map(|_| ());
        }
        if state.windows.get(bot).is_some_and(|w| w.same_instance(signals)) {
            return Ok(());
        }
        if !state.windows.contains_key(bot) && state.windows.len() >= MAX_WINDOWS {
            return Err(RouteError::Capacity);
        }
        let revision = state.next()?;
        if let Some(mut old) = state.windows.remove(bot) {
            old.cancel();
        }
        state.windows.insert(
            bot.into(),
            Window {
                instance: uuid::Uuid::new_v4().to_string(),
                revision,
                signals: Arc::downgrade(signals),
                cancel: Arc::new(AtomicBool::new(false)),
                pending: None,
            },
        );
        Ok(())
    }

    /// The future native transport must derive bot/label/signals itself. No IPC
    /// payload can construct or deserialize this witness.
    pub(super) fn witness(
        &self,
        bot: &str,
        label: &str,
        signals: &Arc<WindowSignals>,
        now: u64,
    ) -> Result<WindowWitness, RouteError> {
        if !crate::storage::valid_id(bot) || label != format!("avatar-{bot}") {
            return Err(RouteError::InvalidScope);
        }
        let mut state = self.lock()?;
        state.reap(now);
        let window = state.windows.get(bot).ok_or(RouteError::StaleWindow)?;
        if !window.same_instance(signals) || window.cancel.load(Ordering::SeqCst) {
            return Err(RouteError::StaleWindow);
        }
        Ok(WindowWitness {
            bot: bot.into(),
            instance: window.instance.clone(),
            revision: window.revision,
            signals: window.signals.clone(),
            cancel: window.cancel.clone(),
        })
    }

    /// Reserves cancellation identity for a future preparation attempt. It does
    /// NOT prove layout or allow any registration/measurement/action.
    pub(super) fn begin_preparation(
        &self,
        witness: WindowWitness,
        now: u64,
    ) -> Result<PreparationTicket, RouteError> {
        let deadline = now
            .checked_add(PREPARATION_MS)
            .filter(|value| *value <= MAX_COUNTER)
            .ok_or(RouteError::Expired)?;
        let mut state = self.lock()?;
        state.reap(now);
        let window = state.windows.get(&witness.bot).ok_or(RouteError::StaleWindow)?;
        if !window.matches(&witness) {
            return Err(RouteError::StaleWindow);
        }
        if window.pending.is_some() {
            return Err(RouteError::Busy);
        }
        if state.windows.values().filter(|w| w.pending.is_some()).count() >= MAX_INTENTS {
            return Err(RouteError::Capacity);
        }
        let id = state.next()?;
        let cancel = Arc::new(AtomicBool::new(false));
        state.windows.get_mut(&witness.bot).expect("window checked under lock").pending = Some(Pending {
            id,
            deadline,
            cancel: cancel.clone(),
        });
        Ok(PreparationTicket { window: witness, id, issued_at: now, deadline, cancel })
    }

    /// Single-use completion of the pending identity only. Success is NOT a
    /// prepared-layout proof and cannot promote a native rendering capability.
    pub(super) fn take_preparation(
        &self,
        ticket: &PreparationTicket,
        now: u64,
    ) -> Result<(), RouteError> {
        if now < ticket.issued_at || now > ticket.deadline {
            self.cancel_preparation(ticket);
            return Err(RouteError::Expired);
        }
        let mut state = self.lock()?;
        state.reap(now);
        let window = state.windows.get_mut(&ticket.window.bot).ok_or(RouteError::StaleWindow)?;
        if !window.matches(&ticket.window)
            || ticket.cancel.load(Ordering::SeqCst)
            || window.pending.as_ref().is_none_or(|pending| {
                pending.id != ticket.id || !Arc::ptr_eq(&pending.cancel, &ticket.cancel)
            })
        {
            return Err(RouteError::StaleWindow);
        }
        window.pending.take();
        ticket.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// A late failed/cancelled attempt never clears a successor's pending ID.
    pub(super) fn cancel_preparation(&self, ticket: &PreparationTicket) {
        ticket.cancel.store(true, Ordering::SeqCst);
        let Ok(mut state) = self.lock() else { return; };
        if let Some(window) = state.windows.get_mut(&ticket.window.bot) {
            if window.matches(&ticket.window)
                && window.pending.as_ref().is_some_and(|pending| {
                    pending.id == ticket.id && Arc::ptr_eq(&pending.cancel, &ticket.cancel)
                })
            {
                window.pending.take();
            }
        }
    }

    /// Cancellation happens BEFORE any GUI/Store/Core wait. A retired native
    /// callback cannot revoke the replacement sharing the same public label.
    pub(super) fn window_changed(
        &self,
        bot: &str,
        signals: &Arc<WindowSignals>,
        change: NativeWindowChange,
    ) -> bool {
        let Ok(mut state) = self.lock() else { return false; };
        if !state.windows.get(bot).is_some_and(|w| w.same_instance(signals)) {
            return false;
        }
        let revision = state.next();
        let window = state.windows.get_mut(bot).expect("instance checked under lock");
        window.cancel();
        if matches!(change, NativeWindowChange::Destroyed | NativeWindowChange::Closing) {
            state.windows.remove(bot);
        } else if let Ok(revision) = revision {
            window.revision = revision;
            window.cancel = Arc::new(AtomicBool::new(false));
        }
        true
    }

    /// Operator/Stop/profile invalidation has native scope, independent of a
    /// renderer nonce. This also handles main hiding before acquiring Core.
    pub(super) fn revoke(&self, bot: Option<&str>) {
        let Ok(mut state) = self.lock() else { return; };
        let revision = state.next();
        for (id, window) in &mut state.windows {
            if bot.is_none_or(|bot| bot == id) {
                window.cancel();
                if let Ok(revision) = revision {
                    window.revision = revision;
                    window.cancel = Arc::new(AtomicBool::new(false));
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "native_output_route_tests.rs"]
mod tests;
