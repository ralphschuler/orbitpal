//! Synthetic native request/event/getter observations only; no window or GUI.
use super::*;

fn normal() -> Bounds { Bounds { x: -2101, y: 121, width: 1041, height: 1441 } }
fn target() -> Bounds { Bounds { x: -2560, y: -200, width: 2560, height: 1600 } }
fn issued(signals: &Arc<WindowSignals>) -> (LayoutFields, FieldRequest, FieldRequest) {
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    let moved = signals.request_layout_move(&owner).unwrap();
    let resized = signals.request_layout_resize(&owner).unwrap();
    (owner, moved, resized)
}
fn observed(signals: &Arc<WindowSignals>) -> (LayoutFields, FieldRequest, FieldRequest, NativeFieldFact, NativeFieldFact) {
    let (owner, moved, resized) = issued(signals);
    let m = signals.observe_move_fact(target().x, target().y).unwrap();
    let r = signals.observe_resize_fact(target().width, target().height).unwrap();
    (owner, moved, resized, m, r)
}

#[test]
fn both_field_orders_match_even_after_the_other_fields_newer_event() {
    for resize_first in [false, true] {
        let signals = Arc::new(WindowSignals::default());
        let (owner, moved, resized) = issued(&signals);
        let (m, r) = if resize_first {
            let r = signals.observe_resize_fact(target().width, target().height).unwrap();
            let m = signals.observe_move_fact(target().x, target().y).unwrap();
            (m, r)
        } else {
            let m = signals.observe_move_fact(target().x, target().y).unwrap();
            let r = signals.observe_resize_fact(target().width, target().height).unwrap();
            (m, r)
        };
        assert!(signals.matches_request(&owner, &moved, &m).unwrap());
        assert!(signals.matches_request(&owner, &resized, &r).unwrap());
        // A correlated old same-valued OS event is not proof the getter changed.
        let stale = signals.begin_getter_check(&owner).unwrap();
        assert!(matches!(signals.match_getter(&owner, stale, normal()), Err(TypedError::Invalid)));
        // Only the native coordinator can arrange a later actual getter call.
        let later = signals.begin_getter_check(&owner).unwrap();
        assert!(signals.match_getter(&owner, later, target()).is_ok());
    }
}

#[test]
fn pending_legacy_requests_and_preowner_facts_cannot_be_adopted() {
    let signals = Arc::new(WindowSignals::default());
    assert!(signals.request_move(target().x, target().y));
    assert!(matches!(signals.begin_layout_fields(normal(), target()), Err(TypedError::Busy)));
    let old = signals.observe_move_fact(target().x, target().y).unwrap();
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    let moved = signals.request_layout_move(&owner).unwrap();
    assert_eq!(moved.request_serial, 2);
    assert!(matches!(signals.matches_request(&owner, &moved, &old), Err(TypedError::ForeignRequest)));
    assert!(!owner.cancelled.load(Ordering::SeqCst));
    assert!(signals.has_pending_move());
}

#[test]
fn identical_legacy_coalescing_revokes_owned_proof_and_keeps_legacy_fifo_exact() {
    let signals = Arc::new(WindowSignals::default());
    let control = WindowSignals::default();
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    let moved = signals.request_layout_move(&owner).unwrap();
    assert!(control.request_move(target().x, target().y));
    for _ in 0..100 {
        assert!(signals.request_move(target().x, target().y));
        assert!(control.request_move(target().x, target().y));
    }
    assert!(owner.cancelled.load(Ordering::SeqCst));
    assert_eq!(signals.state.lock().unwrap().moves.len(), 1);
    assert_eq!(signals.snapshot(), control.snapshot());
    let fact = signals.observe_move_fact(target().x, target().y).unwrap();
    control.observe_move(target().x, target().y);
    assert_eq!(signals.snapshot(), control.snapshot());
    assert!(signals.matches_request(&owner, &moved, &fact).is_err());
}

#[test]
fn legacy_other_field_and_restore_calls_revoke_without_changing_their_results() {
    for restore in [false, true] {
        let signals = Arc::new(WindowSignals::default());
        let control = WindowSignals::default();
        let owner = signals.begin_layout_fields(normal(), target()).unwrap();
        signals.request_layout_move(&owner).unwrap();
        assert!(control.request_move(target().x, target().y));
        if restore {
            let a = signals.restore_fence(normal(), true, false);
            let b = control.restore_fence(normal(), true, false);
            assert_eq!(a.confirmed(signals.snapshot(), normal()), b.confirmed(control.snapshot(), normal()));
        } else {
            assert!(signals.request_resize(normal().width, normal().height));
            assert!(control.request_resize(normal().width, normal().height));
        }
        assert!(owner.cancelled.load(Ordering::SeqCst));
        assert_eq!(signals.snapshot(), control.snapshot());
    }
}

#[test]
fn restoring_after_owned_expansion_keeps_the_legacy_stop_before_expansion_barrier() {
    let signals = Arc::new(WindowSignals::default());
    let (owner, _, _) = issued(&signals);
    let restore = signals.restore_fence(normal(), true, true);
    assert!(owner.cancelled.load(Ordering::SeqCst));
    assert!(signals.request_move(normal().x, normal().y));
    assert!(signals.request_resize(normal().width, normal().height));
    signals.observe_move(normal().x, normal().y);
    signals.observe_resize(normal().width, normal().height);
    assert!(!restore.confirmed(signals.snapshot(), normal()));
    signals.observe_move(target().x, target().y);
    signals.observe_resize(target().width, target().height);
    assert!(!restore.confirmed(signals.snapshot(), normal()));
    signals.observe_resize(normal().width, normal().height);
    signals.observe_move(normal().x, normal().y);
    assert!(restore.confirmed(signals.snapshot(), normal()));
}

#[test]
fn no_op_fields_need_no_events_and_duplicate_issue_does_not_replace_a_request() {
    let signals = Arc::new(WindowSignals::default());
    let owner = signals.begin_layout_fields(normal(), normal()).unwrap();
    assert!(matches!(signals.begin_layout_fields(normal(), target()), Err(TypedError::Busy)));
    assert!(!owner.cancelled.load(Ordering::SeqCst));
    assert!(matches!(signals.request_layout_move(&owner), Err(TypedError::Invalid)));
    assert!(matches!(signals.request_layout_resize(&owner), Err(TypedError::Invalid)));
    let fence = signals.begin_getter_check(&owner).unwrap();
    assert!(signals.match_getter(&owner, fence, normal()).is_ok());
    drop(owner);
    let moved_target = Bounds { x: target().x, y: target().y, ..normal() };
    let owner = signals.begin_layout_fields(normal(), moved_target).unwrap();
    let moved = signals.request_layout_move(&owner).unwrap();
    let serial = signals.snapshot().requested_move;
    assert!(matches!(signals.request_layout_move(&owner), Err(TypedError::Busy)));
    assert_eq!(signals.snapshot().requested_move, serial);
    assert!(!owner.cancelled.load(Ordering::SeqCst));
    let fact = signals.observe_move_fact(moved_target.x, moved_target.y).unwrap();
    assert!(signals.matches_request(&owner, &moved, &fact).unwrap());
    let fence = signals.begin_getter_check(&owner).unwrap();
    assert!(signals.match_getter(&owner, fence, moved_target).is_ok());
}

#[test]
fn actual_unexpected_duplicate_or_no_op_events_cancel_current_ownership() {
    for case in 0..4 {
        let signals = Arc::new(WindowSignals::default());
        let owner = signals.begin_layout_fields(normal(), target()).unwrap();
        match case {
            0 => { assert!(signals.observe_move_fact(target().x, target().y).is_err()); }
            1 => { signals.request_layout_move(&owner).unwrap(); assert!(signals.observe_move_fact(42, 42).is_err()); }
            2 => {
                signals.request_layout_move(&owner).unwrap();
                signals.observe_move_fact(target().x, target().y).unwrap();
                assert!(signals.observe_move_fact(target().x, target().y).is_err());
            }
            _ => { signals.observe_resize(normal().width, normal().height); }
        }
        assert!(owner.cancelled.load(Ordering::SeqCst));
        assert!(signals.snapshot().serial > 0);
    }
    let signals = Arc::new(WindowSignals::default());
    let owner = signals.begin_layout_fields(normal(), normal()).unwrap();
    assert!(signals.observe_move_fact(normal().x, normal().y).is_err());
    assert!(owner.cancelled.load(Ordering::SeqCst));
}

#[test]
fn getter_fence_cannot_survive_a_later_event_request_or_restore() {
    for case in 0..3 {
        let signals = Arc::new(WindowSignals::default());
        let (owner, _, _, _, _) = observed(&signals);
        let fence = signals.begin_getter_check(&owner).unwrap();
        match case {
            0 => signals.observe_move(target().x, target().y),
            1 => { assert!(signals.request_move(target().x, target().y)); }
            _ => { signals.restore_fence(normal(), true, true); }
        }
        assert!(signals.match_getter(&owner, fence, target()).is_err());
        assert!(owner.cancelled.load(Ordering::SeqCst));
    }
}

#[test]
fn stale_owner_requests_facts_getters_cancel_and_drop_never_modify_successor() {
    let signals = Arc::new(WindowSignals::default());
    let (a, old_move, _, old_fact, _) = observed(&signals);
    let old_getter = signals.begin_getter_check(&a).unwrap();
    signals.cancel_layout_fields(&a);
    let b = signals.begin_layout_fields(normal(), target()).unwrap();
    let moved = signals.request_layout_move(&b).unwrap();
    let resized = signals.request_layout_resize(&b).unwrap();
    let before = signals.snapshot();
    assert!(signals.request_layout_move(&a).is_err());
    assert!(signals.matches_request(&a, &old_move, &old_fact).is_err());
    assert!(signals.matches_request(&b, &old_move, &old_fact).is_err());
    assert!(signals.begin_getter_check(&a).is_err());
    assert!(signals.match_getter(&b, old_getter, target()).is_err());
    signals.cancel_layout_fields(&a);
    drop(a);
    assert!(!b.cancelled.load(Ordering::SeqCst));
    assert_eq!(signals.snapshot(), before);
    let m = signals.observe_move_fact(target().x, target().y).unwrap();
    let r = signals.observe_resize_fact(target().width, target().height).unwrap();
    assert!(signals.matches_request(&b, &moved, &m).unwrap());
    assert!(signals.matches_request(&b, &resized, &r).unwrap());
    let fence = signals.begin_getter_check(&b).unwrap();
    assert!(signals.match_getter(&b, fence, target()).is_ok());
}

#[test]
fn equal_numeric_ids_in_another_instance_cannot_match_or_cancel() {
    let first = Arc::new(WindowSignals::default());
    let second = Arc::new(WindowSignals::default());
    let (a, old_request, _, old_fact, _) = observed(&first);
    let (b, request, _, fact, _) = observed(&second);
    assert_eq!(a.owner, b.owner);
    assert_eq!(old_request.request_serial, request.request_serial);
    assert_eq!(old_fact.event_serial, fact.event_serial);
    let getter = first.begin_getter_check(&a).unwrap();
    assert!(matches!(second.request_layout_move(&a), Err(TypedError::StaleInstance)));
    assert!(matches!(second.matches_request(&b, &old_request, &old_fact), Err(TypedError::StaleInstance)));
    assert!(matches!(second.match_getter(&b, getter, target()), Err(TypedError::StaleInstance)));
    second.cancel_layout_fields(&a);
    assert!(!b.cancelled.load(Ordering::SeqCst));
    assert!(second.matches_request(&b, &request, &fact).unwrap());
    drop(first);
    assert!(a.signals.upgrade().is_none());
    assert!(matches!(second.begin_getter_check(&a), Err(TypedError::StaleInstance)));
    assert!(!b.cancelled.load(Ordering::SeqCst));
}

#[test]
fn dropping_owner_does_not_clear_pending_requests_or_allow_an_early_successor() {
    let signals = Arc::new(WindowSignals::default());
    let (a, _, _) = issued(&signals);
    let prior_id = a.owner;
    drop(a);
    assert!(signals.has_pending_move() && signals.has_pending_resize());
    assert!(matches!(signals.begin_layout_fields(normal(), target()), Err(TypedError::Busy)));
    signals.observe_move(target().x, target().y);
    signals.observe_resize(target().width, target().height);
    let b = signals.begin_layout_fields(target(), target()).unwrap();
    assert!(b.owner > prior_id);
    let fence = signals.begin_getter_check(&b).unwrap();
    assert!(signals.match_getter(&b, fence, target()).is_ok());
}

#[test]
fn typed_counter_failure_mints_no_fact_and_preserves_legacy_saturation_and_coalescing() {
    let signals = Arc::new(WindowSignals::default());
    let control = WindowSignals::default();
    signals.state.lock().unwrap().events.serial = u64::MAX - 1;
    control.state.lock().unwrap().events.serial = u64::MAX - 1;
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    signals.request_layout_move(&owner).unwrap();
    assert!(control.request_move(target().x, target().y));
    assert!(matches!(signals.observe_move_fact(target().x, target().y), Err(TypedError::Unavailable)));
    control.observe_move(target().x, target().y);
    assert_eq!(signals.snapshot(), control.snapshot());
    assert!(owner.cancelled.load(Ordering::SeqCst));
    assert!(matches!(signals.observe_move_fact(target().x, target().y), Err(TypedError::Unavailable)));
    control.observe_move(target().x, target().y);
    assert_eq!(signals.snapshot(), control.snapshot());

    let signals = Arc::new(WindowSignals::default());
    let control = WindowSignals::default();
    signals.state.lock().unwrap().request_serial = u64::MAX - 1;
    control.state.lock().unwrap().request_serial = u64::MAX - 1;
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    assert!(matches!(signals.request_layout_move(&owner), Err(TypedError::Unavailable)));
    assert!(!signals.has_pending_move());
    for _ in 0..2 {
        assert!(signals.request_move(target().x, target().y));
        assert!(control.request_move(target().x, target().y));
        assert_eq!(signals.snapshot(), control.snapshot());
    }
    let signals = Arc::new(WindowSignals::default());
    signals.state.lock().unwrap().typed_serial = u64::MAX;
    assert!(matches!(signals.begin_layout_fields(normal(), target()), Err(TypedError::Unavailable)));
    assert!(matches!(signals.observe_resize_fact(10, 10), Err(TypedError::Unavailable)));
    assert!(signals.request_resize(20, 20)); // Legacy behavior does not depend on typed counter.
}

#[test]
fn poison_cancels_owner_and_cannot_produce_facts_or_recover_state() {
    let signals = Arc::new(WindowSignals::default());
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = signals.state.lock().unwrap();
        panic!("synthetic signal poison");
    })).is_err());
    assert!(matches!(signals.observe_move_fact(1, 1), Err(TypedError::Unavailable)));
    assert!(owner.cancelled.load(Ordering::SeqCst));
    assert!(!signals.request_move(1, 1));
    assert!(signals.state.is_poisoned());
    assert!(!signals.snapshot().valid);
}

#[test]
fn zero_dimensions_fail_typed_validation_without_altering_legacy_zero_event() {
    let signals = Arc::new(WindowSignals::default());
    let control = WindowSignals::default();
    let owner = signals.begin_layout_fields(normal(), target()).unwrap();
    for bad in [Bounds { width: 0, ..normal() }, Bounds { height: 0, ..normal() }] {
        assert!(matches!(signals.begin_layout_fields(bad, target()), Err(TypedError::Invalid)));
        assert!(matches!(signals.begin_layout_fields(normal(), bad), Err(TypedError::Invalid)));
    }
    assert!(!owner.cancelled.load(Ordering::SeqCst));
    assert!(matches!(signals.observe_resize_fact(0, 0), Err(TypedError::Invalid)));
    control.observe_resize(0, 0);
    assert_eq!(signals.snapshot(), control.snapshot());
    assert!(owner.cancelled.load(Ordering::SeqCst));
}
