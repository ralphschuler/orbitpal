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
impl PreparationTicket {
    pub(super) fn deadline(&self) -> u64 { self.deadline }
    pub(super) fn issued_at(&self) -> u64 { self.issued_at }
    pub(super) fn cancel_atom(&self) -> Arc<AtomicBool> { self.cancel.clone() }
}
/// Cancellation routing only. Its independent atom/slot is retained before a
/// Hub owner can exist; guarded consumption later activates the same record.
/// Cleanup, not Pending reaping or expiry, frees its capacity.
#[derive(Clone)]
pub(super) struct LayoutRoute {
    id: u64,
    bot: String,
    instance: String,
    signals: Weak<WindowSignals>,
    cancel: Arc<AtomicBool>,
}
/// Only an exact live active layout may be temporarily Classifying. It is not
/// current proof and cannot authorize Prepared/Ready while an event is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutRouteState { Current, Classifying, Revoked }
pub(crate) struct GeometryClassification { id: u64, serial: u64, cancel: Arc<AtomicBool> }
struct LayoutSlot {
    route: LayoutRoute,
    issued_at: u64,
    deadline: u64,
    classifying: Option<u64>,
    active: bool,
}
#[derive(Clone)]
pub(super) struct LayoutUi { connection: String, generation: u64 }
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
    layouts: BTreeMap<u64, LayoutSlot>,
    ui: Option<(String, u64, bool)>,
    ui_generation: u64,
}
impl State {
    fn cancel_layouts(&mut self, bot: Option<&str>) {
        for slot in self.layouts.values() {
            if bot.is_none_or(|bot| bot == slot.route.bot) { slot.route.cancel.store(true, Ordering::SeqCst); }
        }
    }
    fn next(&mut self) -> Result<u64, RouteError> {
        if self.serial >= MAX_COUNTER {
            // Exhaustion cannot mint an aliased epoch or leave a prior proof
            // usable. Only restarting the process can issue new counters.
            for window in self.windows.values_mut() {
                window.cancel();
            }
            self.cancel_layouts(None);
            return Err(RouteError::Unavailable);
        }
        self.serial += 1;
        Ok(self.serial)
    }
    fn reap(&mut self, now: u64) {
        for slot in self.layouts.values() {
            if now < slot.issued_at || now > slot.deadline { slot.route.cancel.store(true, Ordering::SeqCst); }
        }
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
        state.cancel_layouts(None);
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
                state.cancel_layouts(None);
                Err(RouteError::Unavailable)
            }
        }
    }
    fn track_window_locked(
        state: &mut State,
        bot: &str,
        signals: &Arc<WindowSignals>,
    ) -> Result<(), RouteError> {
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
        state.cancel_layouts(Some(bot));
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

    #[cfg(test)]
    fn track_window(
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
        Self::track_window_locked(&mut state, bot, signals)
    }

    /// Real constructor boundary: revoke old legacy output jobs BEFORE exposing
    /// a new incarnation under the same route guard. Missing also revokes, even
    /// when reap just removed a dead Weak; failed tracking is not retirement.
    pub(super) fn track_window_with_output(
        &self,
        bot: &str,
        label: &str,
        signals: &Arc<WindowSignals>,
        now: u64,
        outputs: &super::action_output::OutputGate,
    ) -> Result<(), RouteError> {
        if !crate::storage::valid_id(bot) || label != format!("avatar-{bot}") {
            return Err(RouteError::InvalidScope);
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                for window in state.windows.values_mut() { window.cancel(); }
                state.cancel_layouts(None);
                outputs.window(label);
                return Err(RouteError::Unavailable);
            }
        };
        state.reap(now);
        let identical = state.serial < MAX_COUNTER
            && state.windows.get(bot).is_some_and(|w| w.same_instance(signals));
        if !identical { outputs.window(label); }
        Self::track_window_locked(&mut state, bot, signals)
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
        let charged=state.windows.values().filter_map(|w|w.pending.as_ref().map(|p|p.id)).chain(state.layouts.keys().copied()).collect::<std::collections::BTreeSet<_>>().len();
        if charged >= MAX_INTENTS {
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

    /// Charge exact cleanup before any Store wait. Pending and its reserved
    /// record share one ID/capacity slot; this cannot preserve expected geometry
    /// or produce layout currentness until the subsequent guarded handoff.
    pub(super) fn retain_preparation(&self,ticket:&PreparationTicket,signals:&Arc<WindowSignals>,cancel:&Arc<AtomicBool>,ui:&LayoutUi,now:u64)->Result<LayoutRoute,RouteError>{
        let mut state=self.lock()?;state.reap(now);
        if now<ticket.issued_at||now>ticket.deadline||cancel.load(Ordering::SeqCst)||ticket.cancel.load(Ordering::SeqCst)||state.layouts.contains_key(&ticket.id){return Err(RouteError::Expired);}
        if state.ui_generation!=ui.generation||state.ui.as_ref().is_none_or(|(id,seq,blocked)|id!=&ui.connection||*seq==0||*blocked){return Err(RouteError::StaleWindow);}
        let window=state.windows.get(&ticket.window.bot).ok_or(RouteError::StaleWindow)?;
        if !window.matches(&ticket.window)||!window.same_instance(signals)||window.pending.as_ref().is_none_or(|p|p.id!=ticket.id||!Arc::ptr_eq(&p.cancel,&ticket.cancel)){return Err(RouteError::StaleWindow);}
        let route=LayoutRoute{id:ticket.id,bot:ticket.window.bot.clone(),instance:ticket.window.instance.clone(),signals:Arc::downgrade(signals),cancel:cancel.clone()};
        state.layouts.insert(route.id,LayoutSlot{route:route.clone(),issued_at:ticket.issued_at,deadline:ticket.deadline,classifying:None,active:false});Ok(route)
    }
    /// The Hub reservation/cleanup owner exists before this exchange. No caller
    /// closure or Hub/Core/GUI lock is entered under the native route guard.
    pub(super) fn handoff_preparation(
        &self, ticket: &PreparationTicket, signals: &Arc<WindowSignals>,
        cancel: &Arc<AtomicBool>, ui: &LayoutUi, now: u64,
    ) -> Result<LayoutRoute, RouteError> {
        let mut state = self.lock()?;
        state.reap(now);
        if now < ticket.issued_at || now > ticket.deadline || cancel.load(Ordering::SeqCst) {
            return Err(RouteError::Expired);
        }
        if state.ui_generation != ui.generation || state.ui.as_ref().is_none_or(|(id, seq, blocked)| id != &ui.connection || *seq == 0 || *blocked) {
            return Err(RouteError::StaleWindow);
        }
        let window = state.windows.get(&ticket.window.bot).ok_or(RouteError::StaleWindow)?;
        if !window.matches(&ticket.window) || !window.same_instance(signals)
            || ticket.cancel.load(Ordering::SeqCst)
            || window.pending.as_ref().is_none_or(|p| p.id != ticket.id || !Arc::ptr_eq(&p.cancel, &ticket.cancel)) {
            return Err(RouteError::StaleWindow);
        }
        let route = LayoutRoute { id: ticket.id, bot: ticket.window.bot.clone(), instance: ticket.window.instance.clone(), signals: Arc::downgrade(signals), cancel: cancel.clone() };
        let slot=state.layouts.get_mut(&route.id).filter(|slot|slot.route.matches(&route)&&!slot.active).ok_or(RouteError::StaleWindow)?;
        slot.active=true;
        state.windows.get_mut(&route.bot).expect("checked window").pending.take();
        ticket.cancel.store(true, Ordering::SeqCst);
        Ok(route)
    }
    pub(super) fn layout_state(&self, route: &LayoutRoute, now: u64) -> LayoutRouteState {
        let Ok(mut state) = self.lock() else { return LayoutRouteState::Revoked; };
        state.reap(now);
        let Some(slot) = state.layouts.get(&route.id).filter(|slot| slot.active && slot.route.matches(route)
            && !route.cancel.load(Ordering::SeqCst)
            && state.windows.get(&route.bot).is_some_and(|window| window.instance == route.instance
                && Weak::ptr_eq(&window.signals, &route.signals) && window.signals.strong_count() > 0))
        else { return LayoutRouteState::Revoked; };
        if slot.classifying.is_some() { LayoutRouteState::Classifying } else { LayoutRouteState::Current }
    }
    pub(super) fn layout_current(&self, route: &LayoutRoute, now: u64) -> bool {
        self.layout_state(route, now) == LayoutRouteState::Current
    }
    pub(super) fn release_layout(&self, route: &LayoutRoute)->bool {
        route.cancel.store(true, Ordering::SeqCst);
        let Ok(mut state) = self.lock() else { return false; };
        if state.layouts.get(&route.id).is_some_and(|slot| !slot.route.matches(route)){return false;}
        state.layouts.remove(&route.id);true
    }
    /// Raw geometry invalidates old proofs before the one typed observation.
    /// Only the registered attempt survives until classification finishes.
    pub(super) fn begin_geometry(&self, bot: &str, signals: &Arc<WindowSignals>, outputs: &super::action_output::OutputGate) -> Option<GeometryClassification> {
        let mut state = self.lock().ok()?;
        if !state.windows.get(bot).is_some_and(|window| window.same_instance(signals)) { return None; }
        let serial = state.next().ok()?;
        let instance = state.windows.get(bot)?.instance.clone();
        Self::change_window_locked(&mut state, bot, signals, NativeWindowChange::Geometry);
        outputs.geometry(Some(bot));
        for slot in state.layouts.values().filter(|slot|slot.route.bot==bot&&slot.route.instance==instance&&!slot.active){slot.route.cancel.store(true,Ordering::SeqCst);}
        let slot = state.layouts.values_mut().find(|slot| slot.active && slot.route.bot == bot && slot.route.instance == instance && !slot.route.cancel.load(Ordering::SeqCst))?;
        if slot.classifying.is_some() { slot.route.cancel.store(true, Ordering::SeqCst); return None; }
        slot.classifying = Some(serial);
        Some(GeometryClassification { id: slot.route.id, serial, cancel: slot.route.cancel.clone() })
    }
    pub(super) fn finish_geometry(&self, fact: GeometryClassification, accepted: bool) {
        let Ok(mut state) = self.lock() else { fact.cancel.store(true, Ordering::SeqCst); return; };
        let Some(slot) = state.layouts.get_mut(&fact.id).filter(|slot| Arc::ptr_eq(&slot.route.cancel, &fact.cancel)) else { fact.cancel.store(true, Ordering::SeqCst); return; };
        if slot.classifying != Some(fact.serial) || !accepted { slot.route.cancel.store(true, Ordering::SeqCst); }
        if slot.classifying == Some(fact.serial) { slot.classifying = None; }
    }
    pub(super) fn ui_connect(&self, id: &str) {
        let Ok(mut state) = self.lock() else { return; };
        state.cancel_layouts(None);
        state.ui_generation = state.ui_generation.saturating_add(1).min(MAX_COUNTER);
        state.ui = if !id.is_empty()&&id.len()<=96&&!id.chars().any(char::is_control){Some((id.to_owned(), 0, true))}else{None};
    }
    pub(super) fn ui_disconnect(&self, id: &str) {
        let Ok(mut state) = self.lock() else { return; };
        if state.ui.as_ref().is_some_and(|(current, _, _)| current == id) {
            state.cancel_layouts(None); state.ui = None;
            state.ui_generation = state.ui_generation.saturating_add(1).min(MAX_COUNTER);
        }
    }
    pub(super) fn ui_report(&self, id: &str, sequence: u64, blocked: bool) {
        let Ok(mut state) = self.lock() else { return; };
        if state.ui.as_ref().is_none_or(|(current, old, _)| current != id || sequence <= *old || sequence > MAX_COUNTER) { return; }
        state.ui = Some((id.to_owned(), sequence, blocked));
        if blocked { state.cancel_layouts(None); state.ui_generation = state.ui_generation.saturating_add(1).min(MAX_COUNTER); }
    }
    pub(super) fn layout_ui(&self) -> Result<LayoutUi, RouteError> {
        let state = self.lock()?;
        let (id, sequence, blocked) = state.ui.as_ref().ok_or(RouteError::StaleWindow)?;
        if *blocked || *sequence == 0 || state.ui_generation >= MAX_COUNTER { return Err(RouteError::StaleWindow); }
        Ok(LayoutUi { connection: id.clone(), generation: state.ui_generation })
    }
    pub(super) fn layout_ui_connection<'a>(&self, ui: &'a LayoutUi) -> &'a str { &ui.connection }

    fn change_window_locked(
        state: &mut State,
        bot: &str,
        signals: &Arc<WindowSignals>,
        change: NativeWindowChange,
    ) -> bool {
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

    #[cfg(test)]
    fn window_changed(
        &self,
        bot: &str,
        signals: &Arc<WindowSignals>,
        change: NativeWindowChange,
    ) -> bool {
        let Ok(mut state) = self.lock() else { return false; };
        Self::change_window_locked(&mut state, bot, signals, change)
    }

    /// One pre-Core native event boundary. Hold NativeOutputRoutes -> OutputGate
    /// through both identity classification and legacy loss cancellation, so a
    /// constructor cannot publish a successor between the check and revocation.
    /// Neither registry calls GUI/Core/Store, emits events or invokes caller code.
    pub(super) fn window_changed_with_output(
        &self,
        bot: &str,
        signals: &Arc<WindowSignals>,
        change: NativeWindowChange,
        outputs: &super::action_output::OutputGate,
    ) -> bool {
        if !crate::storage::valid_id(bot) { return false; }
        let lost = matches!(change, NativeWindowChange::Closing | NativeWindowChange::Destroyed);
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                // Poison cannot prove which incarnation is current. Preserve
                // conservative legacy cancellation, without recovering grants.
                let mut state = poison.into_inner();
                for window in state.windows.values_mut() { window.cancel(); }
                state.cancel_layouts(None);
                if lost { outputs.window(&format!("avatar-{bot}")); }
                return false;
            }
        };
        if state.windows.get(bot).is_some_and(|w| !w.same_instance(signals)) {
            // Only a healthy, known different Arc proves the callback retired.
            // Do not even advance OutputGate's global admission generation.
            return false;
        }
        let current = Self::change_window_locked(&mut state, bot, signals, change);
        // Geometry classified by begin_geometry is the only exception.
        state.cancel_layouts(Some(bot));
        // Missing can follow Close or failed constructor tracking. It is not
        // proof of retirement. Revoke under THIS guard too, preventing a late
        // Missing Destroyed from interposing between successor track and loss.
        if lost { outputs.window(&format!("avatar-{bot}")); }
        drop(state);
        current
    }

    /// Operator/Stop/profile invalidation has native scope, independent of a
    /// renderer nonce. This also handles main hiding before acquiring Core.
    pub(super) fn revoke(&self, bot: Option<&str>) {
        let Ok(mut state) = self.lock() else { return; };
        let revision = state.next();
        state.cancel_layouts(bot);
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
impl LayoutRoute {
    fn matches(&self, other: &Self) -> bool { self.id == other.id && self.bot == other.bot && self.instance == other.instance && Weak::ptr_eq(&self.signals, &other.signals) && Arc::ptr_eq(&self.cancel, &other.cancel) }
}
#[cfg(test)]
#[path = "native_output_route_tests.rs"]
mod tests;
