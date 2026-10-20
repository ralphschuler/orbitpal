//! Portable acknowledgements for asynchronous native window geometry requests.
//!
//! A getter can still expose the pre-expansion normal bounds after expansion
//! has been queued. Restoring therefore requires fresh, field-specific native
//! events as well as matching getter values. This module never performs native
//! calls or holds its short signal lock across caller code.

use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, MutexGuard, Weak}};

const MAX_PENDING_REQUESTS_PER_FIELD: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveSignal {
    pub serial: u64,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeSignal {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventSnapshot {
    pub serial: u64,
    pub moved: Option<MoveSignal>,
    pub resized: Option<ResizeSignal>,
    requested_move: u64,
    requested_resize: u64,
    acknowledged_move: u64,
    acknowledged_resize: u64,
    move_ack_event: u64,
    resize_ack_event: u64,
    valid: bool,
}

impl Default for EventSnapshot {
    fn default() -> Self {
        Self {
            serial: 0, moved: None, resized: None, requested_move: 0,
            requested_resize: 0, acknowledged_move: 0, acknowledged_resize: 0,
            move_ack_event: 0, resize_ack_event: 0, valid: true,
        }
    }
}

#[derive(Default)]
struct Signals {
    events: EventSnapshot,
    request_serial: u64,
    moves: VecDeque<MoveSignal>,
    resizes: VecDeque<ResizeSignal>,
    move_barrier: u64,
    resize_barrier: u64,
    typed_serial: u64,
    typed_owner: Option<OwnedFields>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypedError { Busy, ForeignRequest, StaleInstance, Invalid, Unavailable }
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field { Move, Resize }
#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundsField { Move(i32, i32), Resize(u32, u32) }
#[derive(Clone, Copy)]
enum FieldState {
    Unchanged,
    Unissued,
    Pending { request: u64, event_floor: u64 },
    Observed { request: u64, event: u64 },
}
struct OwnedFields {
    id: u64,
    initial: Bounds,
    target: Bounds,
    cancelled: Weak<AtomicBool>,
    moved: FieldState,
    resized: FieldState,
}
/// Private observation ownership only: no native layout/render/action grant.
pub(crate) struct LayoutFields {
    signals: Weak<WindowSignals>,
    owner: u64,
    cancelled: Arc<AtomicBool>,
}
impl Drop for LayoutFields {
    fn drop(&mut self) { self.cancelled.store(true, Ordering::SeqCst); }
}
pub(crate) struct FieldRequest {
    signals: Weak<WindowSignals>,
    owner: u64,
    field: Field,
    request_serial: u64,
    issued_after_event: u64,
    target: Bounds,
}
/// Correlation with a host FIFO request, never proof of OS setter causality.
pub(crate) struct NativeFieldFact {
    signals: Weak<WindowSignals>,
    field: Field,
    event_serial: u64,
    value: BoundsField,
    correlated_request: Option<u64>,
    owner: Option<u64>,
}
pub(crate) struct GetterFence {
    signals: Weak<WindowSignals>,
    owner: u64,
    event_serial: u64,
    requested_move: u64,
    requested_resize: u64,
}
pub(crate) struct ObservedFields;

/// Each actual native output window gets a fresh instance, normally behind an
/// Arc. A destroyed window must not acknowledge a replacement window's fence.
#[derive(Default)]
pub struct WindowSignals {
    state: Mutex<Signals>,
}

impl WindowSignals {
    fn cancel_typed(state: &Signals) {
        if let Some(cancelled) = state.typed_owner.as_ref().and_then(|owner| owner.cancelled.upgrade()) {
            cancelled.store(true, Ordering::SeqCst);
        }
    }
    fn lock_signals(&self) -> Result<MutexGuard<'_, Signals>, TypedError> {
        match self.state.lock() {
            Ok(state) => Ok(state),
            Err(poison) => {
                Self::cancel_typed(&poison.into_inner());
                Err(TypedError::Unavailable)
            }
        }
    }

    /// Establish a new restore epoch before invoking either restoring setter.
    /// Its first request per field gets a fresh ID even when an earlier
    /// rollback already has the same normal target pending.
    pub fn restore_fence(&self, target: Bounds, requires_move: bool, requires_resize: bool) -> RestoreFence {
        let Ok(mut state) = self.lock_signals() else {
            return RestoreFence::new(EventSnapshot {
                valid: false, ..EventSnapshot::default()
            }, target, requires_move, requires_resize);
        };
        Self::cancel_typed(&state);
        let fence = RestoreFence::new(state.events, target, requires_move, requires_resize);
        state.move_barrier = state.events.requested_move;
        state.resize_barrier = state.events.requested_resize;
        fence
    }

    pub fn has_pending_move(&self) -> bool {
        self.lock_signals().map(|state| !state.events.valid || !state.moves.is_empty()).unwrap_or(true)
    }

    pub fn has_pending_resize(&self) -> bool {
        self.lock_signals().map(|state| !state.events.valid || !state.resizes.is_empty()).unwrap_or(true)
    }

    /// Record immediately BEFORE invoking the native setter, even if a stale
    /// getter already equals the target. Repeated adjacent pending requests
    /// are coalesced. A false return means the caller must keep its guard.
    pub fn request_move(&self, x: i32, y: i32) -> bool {
        let Ok(mut state) = self.lock_signals() else { return false; };
        Self::cancel_typed(&state);
        Self::enqueue_move(&mut state, x, y)
    }

    fn enqueue_move(state: &mut Signals, x: i32, y: i32) -> bool {
        if !state.events.valid { return false; }
        if state.moves.back().is_some_and(|request| request.serial > state.move_barrier && request.x == x && request.y == y) {
            return true;
        }
        if state.moves.len() >= MAX_PENDING_REQUESTS_PER_FIELD || state.request_serial == u64::MAX {
            state.events.valid = false;
            return false;
        }
        state.request_serial += 1;
        let serial = state.request_serial;
        state.moves.push_back(MoveSignal { serial, x, y });
        state.events.requested_move = serial;
        true
    }

    pub fn request_resize(&self, width: u32, height: u32) -> bool {
        let Ok(mut state) = self.lock_signals() else { return false; };
        Self::cancel_typed(&state);
        Self::enqueue_resize(&mut state, width, height)
    }

    fn enqueue_resize(state: &mut Signals, width: u32, height: u32) -> bool {
        if !state.events.valid { return false; }
        if state.resizes.back().is_some_and(|request| request.serial > state.resize_barrier && request.width == width && request.height == height) {
            return true;
        }
        if state.resizes.len() >= MAX_PENDING_REQUESTS_PER_FIELD || state.request_serial == u64::MAX {
            state.events.valid = false;
            return false;
        }
        state.request_serial += 1;
        let serial = state.request_serial;
        state.resizes.push_back(ResizeSignal { serial, width, height });
        state.events.requested_resize = serial;
        true
    }

    pub fn observe_move(&self, x: i32, y: i32) {
        if let Ok(mut state) = self.lock_signals() {
            let correlated = Self::record_move(&mut state, x, y);
            let _ = Self::observe_typed(&mut state, Field::Move, BoundsField::Move(x, y), correlated);
        }
    }

    fn record_move(state: &mut Signals, x: i32, y: i32) -> Option<u64> {
        let mut correlated = None;
        state.events.serial = state.events.serial.saturating_add(1);
        let serial = state.events.serial;
        state.events.moved = Some(MoveSignal { serial, x, y });
        // An old normal-valued event must not jump past the queued
        // expansion and falsely acknowledge a later restoration.
        while state.moves.front().is_some_and(|request| request.x == x && request.y == y) {
            let request = state.moves.pop_front().expect("checked queue head");
            state.events.acknowledged_move = request.serial;
            state.events.move_ack_event = serial;
            correlated = Some(request.serial);
        }
        correlated
    }

    pub fn observe_resize(&self, width: u32, height: u32) {
        if let Ok(mut state) = self.lock_signals() {
            let correlated = Self::record_resize(&mut state, width, height);
            let _ = Self::observe_typed(&mut state, Field::Resize, BoundsField::Resize(width, height), correlated);
        }
    }

    fn record_resize(state: &mut Signals, width: u32, height: u32) -> Option<u64> {
        let mut correlated = None;
        state.events.serial = state.events.serial.saturating_add(1);
        let serial = state.events.serial;
        state.events.resized = Some(ResizeSignal { serial, width, height });
        while state.resizes.front().is_some_and(|request| request.width == width && request.height == height) {
            let request = state.resizes.pop_front().expect("checked queue head");
            state.events.acknowledged_resize = request.serial;
            state.events.resize_ack_event = serial;
            correlated = Some(request.serial);
        }
        correlated
    }

    pub fn snapshot(&self) -> EventSnapshot {
        self.lock_signals().map(|state| state.events).unwrap_or(EventSnapshot {
            serial: u64::MAX, valid: false, ..EventSnapshot::default()
        })
    }
}

impl WindowSignals {
    fn typed_usable(state: &Signals) -> Result<(), TypedError> {
        if !state.events.valid || state.events.serial == u64::MAX
            || state.request_serial == u64::MAX || state.typed_serial == u64::MAX {
            Self::cancel_typed(state);
            return Err(TypedError::Unavailable);
        }
        Ok(())
    }
    fn instance(&self, signals: &Weak<Self>) -> Result<(), TypedError> {
        if signals.upgrade().is_some_and(|current| std::ptr::eq(self, current.as_ref())) {
            Ok(())
        } else { Err(TypedError::StaleInstance) }
    }
    fn current_owner<'a>(&self, state: &'a Signals, owner: &LayoutFields) -> Result<&'a OwnedFields, TypedError> {
        self.instance(&owner.signals)?;
        state.typed_owner.as_ref().filter(|current| current.id == owner.owner
            && current.cancelled.ptr_eq(&Arc::downgrade(&owner.cancelled))
            && !owner.cancelled.load(Ordering::SeqCst)).ok_or(TypedError::ForeignRequest)
    }
    fn field_value(bounds: Bounds, field: Field) -> BoundsField {
        match field { Field::Move => BoundsField::Move(bounds.x, bounds.y),
            Field::Resize => BoundsField::Resize(bounds.width, bounds.height) }
    }
    pub(crate) fn begin_layout_fields(self: &Arc<Self>, initial: Bounds, target: Bounds) -> Result<LayoutFields, TypedError> {
        if initial.width == 0 || initial.height == 0 || target.width == 0 || target.height == 0 {
            return Err(TypedError::Invalid);
        }
        let mut state = self.lock_signals()?;
        if !state.moves.is_empty() || !state.resizes.is_empty()
            || state.typed_owner.as_ref().and_then(|o| o.cancelled.upgrade()).is_some_and(|c| !c.load(Ordering::SeqCst)) {
            return Err(TypedError::Busy);
        }
        Self::typed_usable(&state)?;
        let next = state.typed_serial.checked_add(1).filter(|id| *id < u64::MAX).ok_or(TypedError::Unavailable)?;
        state.typed_serial = next;
        let cancelled = Arc::new(AtomicBool::new(false));
        state.typed_owner = Some(OwnedFields { id: next, initial, target, cancelled: Arc::downgrade(&cancelled),
            moved: if (initial.x, initial.y) == (target.x, target.y) { FieldState::Unchanged } else { FieldState::Unissued },
            resized: if (initial.width, initial.height) == (target.width, target.height) { FieldState::Unchanged } else { FieldState::Unissued } });
        Ok(LayoutFields { signals: Arc::downgrade(self), owner: next, cancelled })
    }
    fn request_layout(self: &Arc<Self>, owner: &LayoutFields, field: Field) -> Result<FieldRequest, TypedError> {
        self.instance(&owner.signals)?;
        let mut state = self.lock_signals()?;
        let current = self.current_owner(&state, owner)?;
        let target = current.target;
        let field_state = match field { Field::Move => current.moved, Field::Resize => current.resized };
        match field_state { FieldState::Unchanged => return Err(TypedError::Invalid),
            FieldState::Unissued => (), _ => return Err(TypedError::Busy) }
        Self::typed_usable(&state)?;
        if state.request_serial >= u64::MAX - 1 {
            Self::cancel_typed(&state); return Err(TypedError::Unavailable);
        }
        let empty = match field { Field::Move => state.moves.is_empty(), Field::Resize => state.resizes.is_empty() };
        if !empty { Self::cancel_typed(&state); return Err(TypedError::ForeignRequest); }
        let event_floor = state.events.serial;
        let issued = match field {
            Field::Move => Self::enqueue_move(&mut state, target.x, target.y),
            Field::Resize => Self::enqueue_resize(&mut state, target.width, target.height),
        };
        if !issued { Self::cancel_typed(&state); return Err(TypedError::Unavailable); }
        let request = state.request_serial;
        let current = state.typed_owner.as_mut().expect("owner checked under signal lock");
        match field { Field::Move => current.moved = FieldState::Pending { request, event_floor },
            Field::Resize => current.resized = FieldState::Pending { request, event_floor } }
        Ok(FieldRequest { signals: Arc::downgrade(self), owner: owner.owner, field,
            request_serial: request, issued_after_event: event_floor, target })
    }
    pub(crate) fn request_layout_move(self: &Arc<Self>, owner: &LayoutFields) -> Result<FieldRequest, TypedError> {
        self.request_layout(owner, Field::Move)
    }
    pub(crate) fn request_layout_resize(self: &Arc<Self>, owner: &LayoutFields) -> Result<FieldRequest, TypedError> {
        self.request_layout(owner, Field::Resize)
    }
    fn observe_typed(state: &mut Signals, field: Field, value: BoundsField, correlated: Option<u64>) -> Result<(Option<u64>, Option<u64>), TypedError> {
        Self::typed_usable(state)?;
        if matches!(value, BoundsField::Resize(0, _) | BoundsField::Resize(_, 0)) {
            Self::cancel_typed(state); return Err(TypedError::Invalid);
        }
        let Some(current) = state.typed_owner.as_ref().filter(|owner| owner.cancelled.upgrade().is_some_and(|c| !c.load(Ordering::SeqCst)))
            else { return Ok((None, None)); };
        let field_state = match field { Field::Move => current.moved, Field::Resize => current.resized };
        let FieldState::Pending { request, event_floor } = field_state else {
            Self::cancel_typed(state); return Err(TypedError::ForeignRequest);
        };
        if correlated != Some(request) || state.events.serial <= event_floor || value != Self::field_value(current.target, field) {
            Self::cancel_typed(state); return Err(TypedError::ForeignRequest);
        }
        let id = current.id;
        let observed = FieldState::Observed { request, event: state.events.serial };
        let current = state.typed_owner.as_mut().expect("observed current owner");
        match field { Field::Move => current.moved = observed, Field::Resize => current.resized = observed }
        Ok((Some(request), Some(id)))
    }
    pub(crate) fn observe_move_fact(self: &Arc<Self>, x: i32, y: i32) -> Result<NativeFieldFact, TypedError> {
        let mut state = self.lock_signals()?;
        let correlated = Self::record_move(&mut state, x, y);
        let (correlated_request, owner) = Self::observe_typed(&mut state, Field::Move, BoundsField::Move(x, y), correlated)?;
        Ok(NativeFieldFact { signals: Arc::downgrade(self), field: Field::Move,
            event_serial: state.events.serial, value: BoundsField::Move(x, y), correlated_request, owner })
    }
    pub(crate) fn observe_resize_fact(self: &Arc<Self>, width: u32, height: u32) -> Result<NativeFieldFact, TypedError> {
        let mut state = self.lock_signals()?;
        let correlated = Self::record_resize(&mut state, width, height);
        let (correlated_request, owner) = Self::observe_typed(&mut state, Field::Resize, BoundsField::Resize(width, height), correlated)?;
        Ok(NativeFieldFact { signals: Arc::downgrade(self), field: Field::Resize,
            event_serial: state.events.serial, value: BoundsField::Resize(width, height), correlated_request, owner })
    }
    pub(crate) fn matches_request(&self, owner: &LayoutFields, request: &FieldRequest, fact: &NativeFieldFact) -> Result<bool, TypedError> {
        self.instance(&owner.signals)?; self.instance(&request.signals)?; self.instance(&fact.signals)?;
        let state = self.lock_signals()?;
        let current = self.current_owner(&state, owner)?;
        if request.owner != owner.owner || fact.owner != Some(owner.owner) { return Err(TypedError::ForeignRequest); }
        Self::typed_usable(&state)?;
        if fact.field != request.field || request.target != current.target || fact.correlated_request != Some(request.request_serial)
            || fact.event_serial <= request.issued_after_event || fact.value != Self::field_value(current.target, request.field) {
            return Ok(false);
        }
        let field_state = match request.field { Field::Move => current.moved, Field::Resize => current.resized };
        if !matches!(field_state, FieldState::Observed { request: r, event } if r == request.request_serial && event == fact.event_serial) { return Ok(false); }
        Ok(match request.field {
            Field::Move => state.events.moved.is_some_and(|e| e.serial == fact.event_serial && (e.x,e.y) == (request.target.x,request.target.y))
                && state.events.acknowledged_move == request.request_serial && state.events.requested_move == request.request_serial
                && state.events.move_ack_event == fact.event_serial,
            Field::Resize => state.events.resized.is_some_and(|e| e.serial == fact.event_serial && (e.width,e.height) == (request.target.width,request.target.height))
                && state.events.acknowledged_resize == request.request_serial && state.events.requested_resize == request.request_serial
                && state.events.resize_ack_event == fact.event_serial,
        })
    }
    fn fields_observed(owner: &OwnedFields) -> bool {
        let moved = matches!(owner.moved, FieldState::Observed { .. })
            || matches!(owner.moved, FieldState::Unchanged) && (owner.initial.x,owner.initial.y) == (owner.target.x,owner.target.y);
        let resized = matches!(owner.resized, FieldState::Observed { .. })
            || matches!(owner.resized, FieldState::Unchanged) && (owner.initial.width,owner.initial.height) == (owner.target.width,owner.target.height);
        moved && resized
    }
    pub(crate) fn begin_getter_check(self: &Arc<Self>, owner: &LayoutFields) -> Result<GetterFence, TypedError> {
        self.instance(&owner.signals)?;
        let state = self.lock_signals()?;
        let current = self.current_owner(&state, owner)?;
        Self::typed_usable(&state)?;
        if !Self::fields_observed(current) || !state.moves.is_empty() || !state.resizes.is_empty() { return Err(TypedError::Busy); }
        Ok(GetterFence { signals: Arc::downgrade(self), owner: owner.owner, event_serial: state.events.serial,
            requested_move: state.events.requested_move, requested_resize: state.events.requested_resize })
    }
    pub(crate) fn match_getter(&self, owner: &LayoutFields, fence: GetterFence, current_bounds: Bounds) -> Result<ObservedFields, TypedError> {
        self.instance(&owner.signals)?; self.instance(&fence.signals)?;
        let state = self.lock_signals()?;
        let current = self.current_owner(&state, owner)?;
        if fence.owner != owner.owner { return Err(TypedError::ForeignRequest); }
        Self::typed_usable(&state)?;
        if fence.event_serial != state.events.serial || fence.requested_move != state.events.requested_move
            || fence.requested_resize != state.events.requested_resize || !state.moves.is_empty() || !state.resizes.is_empty()
            || !Self::fields_observed(current) || current_bounds != current.target { return Err(TypedError::Invalid); }
        Ok(ObservedFields)
    }
    pub(crate) fn cancel_layout_fields(&self, owner: &LayoutFields) {
        if self.instance(&owner.signals).is_err() { return; }
        if let Ok(state) = self.lock_signals() {
            if self.current_owner(&state, owner).is_ok() { owner.cancelled.store(true, Ordering::SeqCst); }
        }
    }
}

#[cfg(test)]
#[path = "window_signals/typed_tests.rs"]
mod typed_tests;

/// Capture this fence BEFORE enqueueing the restoring setters. Keep the same
/// fence during retries: taking a new fence each time can discard a legitimate
/// acknowledgement from a coalesced native move/resize event.
#[derive(Clone, Copy, Debug)]
pub struct RestoreFence {
    serial: u64,
    target: Bounds,
    requires_move: bool,
    requires_resize: bool,
    requested_move: u64,
    requested_resize: u64,
    valid: bool,
}

impl RestoreFence {
    /// `target` must already use the current native pixel grid. The two flags
    /// describe fields changed/requested during expansion, including requests
    /// whose effects have not yet reached the native getters.
    pub fn new(
        events: EventSnapshot,
        target: Bounds,
        requires_move: bool,
        requires_resize: bool,
    ) -> Self {
        Self {
            serial: events.serial, target, requires_move, requires_resize,
            requested_move: events.requested_move, requested_resize: events.requested_resize,
            valid: events.valid,
        }
    }

    pub fn confirmed(&self, events: EventSnapshot, current: Bounds) -> bool {
        if !self.valid || !events.valid || current != self.target {
            return false;
        }
        let moved = !self.requires_move || events.moved.is_some_and(|event| {
            event.serial > self.serial && event.x == self.target.x && event.y == self.target.y
                && events.acknowledged_move > self.requested_move
                && events.acknowledged_move == events.requested_move
                && events.move_ack_event > self.serial
        });
        let resized = !self.requires_resize || events.resized.is_some_and(|event| {
            event.serial > self.serial
                && event.width == self.target.width
                && event.height == self.target.height
                && events.acknowledged_resize > self.requested_resize
                && events.acknowledged_resize == events.requested_resize
                && events.resize_ack_event > self.serial
        });
        moved && resized
    }
}

/// Match Tao/GTK's Physical -> Logical<i32> -> Physical integer conversion.
/// Call this only for that native backend; other backends may preserve exact
/// physical pixels. This is not a broad geometry tolerance: only one bounded
/// native grid roundtrip is accepted. Unusable scales/ranges leave the request
/// unchanged so the caller's strict acknowledgement remains fail-closed.
pub fn normalize_physical_bounds(bounds: Bounds, scale_factor: f64) -> Bounds {
    // GTK exposes an integer widget scale. A bounded range also prevents an
    // invalid native value from collapsing a real window to a tiny grid cell.
    if !scale_factor.is_finite() || !(1.0..=16.0).contains(&scale_factor) {
        return bounds;
    }
    let grid = |physical: f64| -> Option<f64> {
        let logical = (physical / scale_factor).round();
        if logical < i32::MIN as f64 || logical > i32::MAX as f64 {
            return None;
        }
        Some((logical * scale_factor).round())
    };
    let Some(x) = grid(bounds.x as f64) else { return bounds; };
    let Some(y) = grid(bounds.y as f64) else { return bounds; };
    let Some(width) = grid(bounds.width as f64) else { return bounds; };
    let Some(height) = grid(bounds.height as f64) else { return bounds; };
    if x < i32::MIN as f64 || x > i32::MAX as f64
        || y < i32::MIN as f64 || y > i32::MAX as f64
        || width < 0.0 || width > u32::MAX as f64
        || height < 0.0 || height > u32::MAX as f64
        || (bounds.width > 0 && width == 0.0)
        || (bounds.height > 0 && height == 0.0)
    {
        return bounds;
    }
    let normalized = Bounds { x: x as i32, y: y as i32, width: width as u32, height: height as u32 };
    let limit = (scale_factor / 2.0).ceil() as i64;
    let within_grid = (normalized.x as i64 - bounds.x as i64).abs() <= limit
        && (normalized.y as i64 - bounds.y as i64).abs() <= limit
        && (normalized.width as i64 - bounds.width as i64).abs() <= limit
        && (normalized.height as i64 - bounds.height as i64).abs() <= limit;
    if within_grid { normalized } else { bounds }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal() -> Bounds { Bounds { x: -2101, y: 121, width: 1041, height: 1441 } }
    fn monitor() -> Bounds { Bounds { x: -2560, y: 0, width: 2560, height: 1600 } }
    fn request_both(events: &WindowSignals, bounds: Bounds) {
        assert!(events.request_move(bounds.x, bounds.y));
        assert!(events.request_resize(bounds.width, bounds.height));
    }
    fn expanded() -> WindowSignals {
        let events = WindowSignals::default();
        request_both(&events, monitor());
        events.observe_move(monitor().x, monitor().y);
        events.observe_resize(monitor().width, monitor().height);
        events
    }

    #[test]
    fn stop_before_delayed_expansion_cannot_release_on_still_normal_getters() {
        let events = WindowSignals::default();
        request_both(&events, monitor());
        let fence = RestoreFence::new(events.snapshot(), normal(), true, true);
        request_both(&events, normal());
        assert!(!fence.confirmed(events.snapshot(), normal()));

        // Even old normal-valued events delivered after the fence cannot skip
        // the still-pending expansion requests at each field's queue head.
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));

        // Expansion was queued before Stop and only arrives afterwards.
        events.observe_resize(monitor().width, monitor().height);
        events.observe_move(monitor().x, monitor().y);
        assert!(!fence.confirmed(events.snapshot(), monitor()));
        assert!(!fence.confirmed(events.snapshot(), normal()));

        events.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));
        events.observe_move(normal().x, normal().y);
        assert!(fence.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn move_and_resize_acknowledgements_can_arrive_in_either_order() {
        for resize_first in [false, true] {
            let events = expanded();
            let fence = RestoreFence::new(events.snapshot(), normal(), true, true);
            request_both(&events, normal());
            if resize_first { events.observe_resize(normal().width, normal().height); }
            else { events.observe_move(normal().x, normal().y); }
            assert!(!fence.confirmed(events.snapshot(), normal()));
            if resize_first { events.observe_move(normal().x, normal().y); }
            else { events.observe_resize(normal().width, normal().height); }
            assert!(fence.confirmed(events.snapshot(), normal()));
        }
    }

    #[test]
    fn a_global_new_serial_does_not_refresh_the_other_fields_old_ack() {
        let events = expanded();
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        let fence = RestoreFence::new(events.snapshot(), normal(), true, true);
        request_both(&events, normal());
        events.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));
        events.observe_move(normal().x, normal().y);
        assert!(fence.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn only_fields_changed_by_expansion_need_a_new_event() {
        let events = WindowSignals::default();
        assert!(events.request_resize(monitor().width, monitor().height));
        events.observe_resize(monitor().width, monitor().height);
        let size_only = RestoreFence::new(events.snapshot(), normal(), false, true);
        assert!(events.request_resize(normal().width, normal().height));
        events.observe_resize(normal().width, normal().height);
        assert!(size_only.confirmed(events.snapshot(), normal()));
        assert!(events.request_move(monitor().x, monitor().y));
        events.observe_move(monitor().x, monitor().y);
        let move_only = RestoreFence::new(events.snapshot(), normal(), true, false);
        assert!(events.request_move(normal().x, normal().y));
        events.observe_move(normal().x, normal().y);
        assert!(move_only.confirmed(events.snapshot(), normal()));
        let unchanged = RestoreFence::new(events.snapshot(), normal(), false, false);
        assert!(unchanged.confirmed(events.snapshot(), normal()));
        assert!(!unchanged.confirmed(events.snapshot(), monitor()));
    }

    #[test]
    fn later_wrong_values_and_stale_getters_cannot_confirm_a_fence() {
        let events = expanded();
        let fence = RestoreFence::new(events.snapshot(), normal(), true, true);
        request_both(&events, normal());
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(events.snapshot(), monitor()));
        events.observe_move(monitor().x, monitor().y);
        assert!(!fence.confirmed(events.snapshot(), normal()));
        events.observe_move(normal().x, normal().y);
        events.observe_resize(monitor().width, monitor().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn another_windows_signals_do_not_implicitly_acknowledge_this_window() {
        let first = expanded();
        let second = expanded();
        let fence = RestoreFence::new(first.snapshot(), normal(), true, true);
        request_both(&first, normal());
        request_both(&second, normal());
        second.observe_move(normal().x, normal().y);
        second.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(first.snapshot(), normal()));
        // The owner must also retain Arc identity when binding a fence; this
        // helper never looks up another window's events by shared bot label.
    }

    #[test]
    fn dpi_roundtrip_allows_odd_physical_bounds_to_restore_on_a_new_grid() {
        let target = normalize_physical_bounds(normal(), 2.0);
        assert_eq!(target, Bounds { x: -2102, y: 122, width: 1042, height: 1442 });
        let events = expanded();
        let fence = RestoreFence::new(events.snapshot(), target, true, true);
        request_both(&events, target);
        events.observe_move(target.x, target.y);
        events.observe_resize(target.width, target.height);
        assert!(fence.confirmed(events.snapshot(), target));
        assert!(!fence.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn normalization_is_bounded_idempotent_and_rejects_invalid_scales() {
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0, 16.0] {
            let first = normalize_physical_bounds(normal(), scale);
            assert_eq!(normalize_physical_bounds(first, scale), first);
            assert!((first.width as i64 - normal().width as i64).abs() <= (scale / 2.0).ceil() as i64);
        }
        for scale in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.5, 16.01, 1e100] {
            assert_eq!(normalize_physical_bounds(normal(), scale), normal());
        }
        let excessive = Bounds { width: u32::MAX, ..normal() };
        assert_eq!(normalize_physical_bounds(excessive, 1.0), excessive);
    }

    #[test]
    fn serial_saturation_and_poisoned_signal_state_fail_closed() {
        let events = WindowSignals::default();
        events.state.lock().unwrap().events.serial = u64::MAX;
        let fence = RestoreFence::new(events.snapshot(), normal(), true, true);
        request_both(&events, normal());
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));

        let poisoned = WindowSignals::default();
        let _ = std::panic::catch_unwind(|| {
            let _guard = poisoned.state.lock().unwrap();
            panic!("fixture poisons only a private signal mutex");
        });
        let fence = RestoreFence::new(poisoned.snapshot(), normal(), false, false);
        poisoned.observe_move(normal().x, normal().y);
        assert!(!fence.confirmed(poisoned.snapshot(), normal()));
        assert!(!poisoned.restore_fence(normal(), false, false).confirmed(poisoned.snapshot(), normal()));
    }

    #[test]
    fn pending_requests_coalesce_but_missing_or_reordered_acks_remain_guarded() {
        let events = WindowSignals::default();
        for _ in 0..100 { request_both(&events, monitor()); }
        let fence = RestoreFence::new(events.snapshot(), normal(), true, true);
        request_both(&events, normal());

        // Restore-valued events delivered ahead of the outstanding expansion
        // cannot be mistaken for acknowledgement of its later restore.
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));
        events.observe_move(monitor().x, monitor().y);
        events.observe_resize(monitor().width, monitor().height);
        assert!(!fence.confirmed(events.snapshot(), normal()));
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(fence.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn unbounded_unacknowledged_native_requests_fail_closed() {
        let events = WindowSignals::default();
        for index in 0..MAX_PENDING_REQUESTS_PER_FIELD {
            assert!(events.request_move(index as i32, 0));
        }
        assert!(!events.request_move(999, 0));
        let fence = RestoreFence::new(events.snapshot(), normal(), false, false);
        assert!(!fence.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn a_new_stop_epoch_can_ack_an_identical_pending_rollback_target() {
        let events = expanded();
        let rollback = events.restore_fence(normal(), true, true);
        request_both(&events, normal());
        let stop = events.restore_fence(normal(), true, true);
        request_both(&events, normal());
        assert!(!stop.confirmed(events.snapshot(), normal()));
        // Native backends may coalesce the two adjacent identical targets into
        // one actual change/event. There is no intervening expansion to skip.
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(rollback.confirmed(events.snapshot(), normal()));
        assert!(stop.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn start_stop_start_stop_requires_the_last_expansion_then_last_restore() {
        let events = expanded();
        let _first_stop = events.restore_fence(normal(), true, true);
        request_both(&events, normal());
        request_both(&events, monitor());
        let final_stop = events.restore_fence(normal(), true, true);
        request_both(&events, normal());
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(!final_stop.confirmed(events.snapshot(), normal()));
        events.observe_move(monitor().x, monitor().y);
        events.observe_resize(monitor().width, monitor().height);
        assert!(!final_stop.confirmed(events.snapshot(), normal()));
        events.observe_move(normal().x, normal().y);
        events.observe_resize(normal().width, normal().height);
        assert!(final_stop.confirmed(events.snapshot(), normal()));
    }

    #[test]
    fn pending_flags_allow_callers_to_skip_only_settled_unchanged_fields() {
        let events = WindowSignals::default();
        assert!(!events.has_pending_move());
        assert!(!events.has_pending_resize());
        request_both(&events, monitor());
        assert!(events.has_pending_move());
        assert!(events.has_pending_resize());
        events.observe_move(monitor().x, monitor().y);
        assert!(!events.has_pending_move());
        assert!(events.has_pending_resize());
        events.observe_resize(monitor().width, monitor().height);
        assert!(!events.has_pending_resize());
    }
}
