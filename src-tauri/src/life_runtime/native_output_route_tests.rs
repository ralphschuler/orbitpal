//! Synthetic lifecycle tests only; no native windows, sessions or GUI APIs.
use super::*;

fn tracked(routes: &NativeOutputRoutes, bot: &str) -> Arc<WindowSignals> {
    let signals = Arc::new(WindowSignals::default());
    routes.track_window(bot, &format!("avatar-{bot}"), &signals, 0).unwrap();
    signals
}
fn begin(
    routes: &NativeOutputRoutes,
    bot: &str,
    signals: &Arc<WindowSignals>,
    now: u64,
) -> PreparationTicket {
    let witness = routes.witness(bot, &format!("avatar-{bot}"), signals, now).unwrap();
    routes.begin_preparation(witness, now).unwrap()
}

#[test]
fn replacement_revokes_old_instance_and_ignores_its_deferred_callbacks() {
    let routes = NativeOutputRoutes::default();
    let old = tracked(&routes, "a");
    let old_ticket = begin(&routes, "a", &old, 0);
    let next = Arc::new(WindowSignals::default());
    routes.track_window("a", "avatar-a", &next, 1).unwrap();
    assert!(old_ticket.cancel.load(Ordering::SeqCst));
    assert!(old_ticket.window.cancel.load(Ordering::SeqCst));
    let current = begin(&routes, "a", &next, 2);
    assert_ne!(old_ticket.window.instance, current.window.instance);
    assert!(!routes.window_changed("a", &old, NativeWindowChange::Destroyed));
    assert!(!routes.window_changed("a", &old, NativeWindowChange::Geometry));
    routes.cancel_preparation(&old_ticket);
    routes.take_preparation(&current, 3).unwrap();
    assert!(matches!(routes.witness("a", "avatar-a", &old, 3), Err(RouteError::StaleWindow)));
}

#[test]
fn identical_constructor_tracking_preserves_pending_identity() {
    let routes = NativeOutputRoutes::default();
    let signals = tracked(&routes, "a");
    let pending = begin(&routes, "a", &signals, 10);
    routes.track_window("a", "avatar-a", &signals, 11).unwrap();
    let again = routes.witness("a", "avatar-a", &signals, 11).unwrap();
    assert_eq!(again.instance, pending.window.instance);
    assert_eq!(again.revision, pending.window.revision);
    routes.take_preparation(&pending, 12).unwrap();
}

#[test]
fn actor_label_and_exact_arc_scope_are_required_before_allocating_an_intent() {
    let routes = NativeOutputRoutes::default();
    let a = tracked(&routes, "a");
    let b = tracked(&routes, "b");
    for label in ["main", "avatar", "avatar-b", "avatar-a-extra", ""] {
        assert!(matches!(routes.witness("a", label, &a, 0), Err(RouteError::InvalidScope)));
        assert_eq!(routes.track_window("a", label, &a, 0), Err(RouteError::InvalidScope));
    }
    assert_eq!(routes.track_window("../a", "avatar-../a", &a, 0), Err(RouteError::InvalidScope));
    assert!(matches!(routes.witness("a", "avatar-a", &b, 0), Err(RouteError::StaleWindow)));
    let mut forged = routes.witness("a", "avatar-a", &a, 0).unwrap();
    forged.bot = "b".into();
    assert!(matches!(routes.begin_preparation(forged, 0), Err(RouteError::StaleWindow)));
    assert_eq!(routes.state.lock().unwrap().windows.values().filter(|w|w.pending.is_some()).count(), 0);
}

#[test]
fn every_native_geometry_visibility_and_close_change_invalidates_only_its_instance() {
    for change in [
        NativeWindowChange::Geometry,
        NativeWindowChange::Scale,
        NativeWindowChange::Hidden,
        NativeWindowChange::Closing,
        NativeWindowChange::Destroyed,
    ] {
        let routes = NativeOutputRoutes::default();
        let a = tracked(&routes, "a");
        let b = tracked(&routes, "b");
        let before = begin(&routes, "a", &a, 0);
        let other = begin(&routes, "b", &b, 0);
        assert!(routes.window_changed("a", &a, change));
        assert!(before.cancel.load(Ordering::SeqCst));
        assert!(before.window.cancel.load(Ordering::SeqCst));
        assert!(matches!(routes.take_preparation(&before, 1), Err(RouteError::StaleWindow)));
        routes.take_preparation(&other, 1).unwrap();
    }
}

#[test]
fn geometry_requires_a_new_revision_and_never_reuses_a_cancelled_witness() {
    let routes = NativeOutputRoutes::default();
    let signals = tracked(&routes, "a");
    let witness = routes.witness("a", "avatar-a", &signals, 0).unwrap();
    let old_revision = witness.revision;
    routes.window_changed("a", &signals, NativeWindowChange::Geometry);
    assert!(matches!(routes.begin_preparation(witness, 1), Err(RouteError::StaleWindow)));
    let next = begin(&routes, "a", &signals, 1);
    assert!(next.window.revision > old_revision);
    routes.take_preparation(&next, 2).unwrap();
}

#[test]
fn targeted_stop_and_global_operator_revocation_do_not_grant_any_layout() {
    let routes = NativeOutputRoutes::default();
    let a = tracked(&routes, "a");
    let b = tracked(&routes, "b");
    let first = begin(&routes, "a", &a, 0);
    let second = begin(&routes, "b", &b, 0);
    routes.revoke(Some("a"));
    assert!(first.cancel.load(Ordering::SeqCst));
    assert!(!second.cancel.load(Ordering::SeqCst));
    routes.take_preparation(&second, 1).unwrap();
    let again = begin(&routes, "a", &a, 1);
    routes.revoke(None);
    assert!(again.cancel.load(Ordering::SeqCst));
    // A new witness is merely a preparation identity, never a ready route.
    assert!(routes.witness("a", "avatar-a", &a, 2).is_ok());
}

#[test]
fn single_use_cancel_and_drop_never_remove_a_newer_intent() {
    let routes = NativeOutputRoutes::default();
    let signals = tracked(&routes, "a");
    let first = begin(&routes, "a", &signals, 0);
    assert!(matches!(
        routes.begin_preparation(routes.witness("a", "avatar-a", &signals, 0).unwrap(), 0),
        Err(RouteError::Busy)
    ));
    routes.cancel_preparation(&first);
    let successor = begin(&routes, "a", &signals, 1);
    routes.cancel_preparation(&first);
    routes.take_preparation(&successor, 2).unwrap();
    assert!(matches!(routes.take_preparation(&successor, 2), Err(RouteError::StaleWindow)));
    drop(begin(&routes, "a", &signals, 3));
    let after_drop = begin(&routes, "a", &signals, 3);
    routes.take_preparation(&after_drop, 4).unwrap();
}

#[test]
fn exact_expiry_and_backward_clock_fail_closed_without_touching_successor() {
    let routes = NativeOutputRoutes::default();
    let signals = tracked(&routes, "a");
    let exact = begin(&routes, "a", &signals, 1);
    routes.take_preparation(&exact, 5_001).unwrap();
    let expired = begin(&routes, "a", &signals, 5_002);
    assert_eq!(routes.take_preparation(&expired, 10_003), Err(RouteError::Expired));
    let next = begin(&routes, "a", &signals, 10_003);
    assert_eq!(routes.take_preparation(&expired, 10_004), Err(RouteError::Expired));
    routes.take_preparation(&next, 10_004).unwrap();
    let backwards = begin(&routes, "a", &signals, 20_000);
    assert_eq!(routes.take_preparation(&backwards, 19_999), Err(RouteError::Expired));
    let witness = routes.witness("a", "avatar-a", &signals, MAX_COUNTER).unwrap();
    assert!(matches!(routes.begin_preparation(witness, MAX_COUNTER), Err(RouteError::Expired)));
}

#[test]
fn pending_and_window_capacities_are_bounded_and_released_by_real_cleanup() {
    let routes = NativeOutputRoutes::default();
    let windows: Vec<_> = (0..MAX_WINDOWS).map(|i| tracked(&routes, &format!("a{i}"))).collect();
    let extra = Arc::new(WindowSignals::default());
    assert_eq!(routes.track_window("extra", "avatar-extra", &extra, 0), Err(RouteError::Capacity));
    let pending: Vec<_> = (0..MAX_INTENTS).map(|i| begin(&routes, &format!("a{i}"), &windows[i], 0)).collect();
    let witness = routes.witness("a8", "avatar-a8", &windows[8], 0).unwrap();
    assert!(matches!(routes.begin_preparation(witness, 0), Err(RouteError::Capacity)));
    routes.cancel_preparation(&pending[0]);
    let replacement = begin(&routes, "a8", &windows[8], 1);
    routes.take_preparation(&replacement, 1).unwrap();
    routes.window_changed("a0", &windows[0], NativeWindowChange::Destroyed);
    routes.track_window("extra", "avatar-extra", &extra, 2).unwrap();
    assert_eq!(routes.state.lock().unwrap().windows.len(), MAX_WINDOWS);
    // Expired pending entries are reaped before the next admission.
    let renewed = begin(&routes, "a9", &windows[9], PREPARATION_MS + 1);
    routes.take_preparation(&renewed, PREPARATION_MS + 2).unwrap();
}

#[test]
fn abandoned_native_arcs_are_reaped_without_retaining_windows_or_intents() {
    let routes = NativeOutputRoutes::default();
    let signals = tracked(&routes, "a");
    let pending = begin(&routes, "a", &signals, 0);
    drop(signals);
    let next = tracked(&routes, "b");
    assert!(pending.cancel.load(Ordering::SeqCst));
    assert!(pending.window.cancel.load(Ordering::SeqCst));
    assert!(!routes.state.lock().unwrap().windows.contains_key("a"));
    routes.take_preparation(&begin(&routes, "b", &next, 1), 1).unwrap();
}

#[test]
fn counter_exhaustion_revokes_all_old_tokens_and_cannot_wrap() {
    let routes = NativeOutputRoutes::default();
    let a = tracked(&routes, "a");
    let b = tracked(&routes, "b");
    let pending_a = begin(&routes, "a", &a, 0);
    let pending_b = begin(&routes, "b", &b, 0);
    routes.state.lock().unwrap().serial = MAX_COUNTER;
    routes.revoke(Some("a"));
    assert!(pending_a.cancel.load(Ordering::SeqCst));
    assert!(pending_b.cancel.load(Ordering::SeqCst));
    assert!(matches!(routes.witness("a", "avatar-a", &a, 1), Err(RouteError::StaleWindow)));
    let next = Arc::new(WindowSignals::default());
    assert_eq!(routes.track_window("c", "avatar-c", &next, 1), Err(RouteError::Unavailable));
    assert_eq!(routes.state.lock().unwrap().serial, MAX_COUNTER);
}

#[test]
fn exhausted_tracking_cancels_a_pending_ticket_minted_at_the_final_counter() {
    for repeated in [false, true] {
        let routes = NativeOutputRoutes::default();
        let signals = tracked(&routes, "a");
        let witness = routes.witness("a", "avatar-a", &signals, 0).unwrap();
        routes.state.lock().unwrap().serial = MAX_COUNTER - 1;
        let pending = routes.begin_preparation(witness, 0).unwrap();
        assert_eq!(pending.id, MAX_COUNTER);
        assert!(!pending.cancel.load(Ordering::SeqCst));
        let next = Arc::new(WindowSignals::default());
        let result = if repeated {
            routes.track_window("a", "avatar-a", &signals, 1)
        } else {
            routes.track_window("b", "avatar-b", &next, 1)
        };
        assert_eq!(result, Err(RouteError::Unavailable));
        assert!(pending.cancel.load(Ordering::SeqCst));
        assert!(pending.window.cancel.load(Ordering::SeqCst));
        assert!(matches!(routes.take_preparation(&pending, 1), Err(RouteError::StaleWindow)));
        let state = routes.state.lock().unwrap();
        assert_eq!(state.serial, MAX_COUNTER);
        assert_eq!(state.windows.len(), 1);
    }
}

#[test]
fn poisoned_native_event_or_stop_cancels_retained_tokens_without_recovering_authority() {
    for native_event in [false, true] {
        let routes = NativeOutputRoutes::default();
        let signals = tracked(&routes, "a");
        let pending = begin(&routes, "a", &signals, 0);
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = routes.state.lock().unwrap();
            panic!("synthetic registry poison");
        }));
        assert!(poisoned.is_err());
        assert!(!pending.cancel.load(Ordering::SeqCst));
        if native_event {
            assert!(!routes.window_changed("a", &signals, NativeWindowChange::Geometry));
        } else {
            routes.revoke(Some("a"));
        }
        assert!(pending.cancel.load(Ordering::SeqCst));
        assert!(pending.window.cancel.load(Ordering::SeqCst));
        assert!(routes.state.is_poisoned());
        assert!(matches!(routes.witness("a", "avatar-a", &signals, 1), Err(RouteError::Unavailable)));
        assert_eq!(routes.take_preparation(&pending, 1), Err(RouteError::Unavailable));
        let next = Arc::new(WindowSignals::default());
        assert_eq!(routes.track_window("b", "avatar-b", &next, 1), Err(RouteError::Unavailable));
        assert!(routes.state.is_poisoned());
    }
}

#[test]
fn dropping_the_registry_cancels_retained_tickets_and_restart_inherits_nothing() {
    let signals = Arc::new(WindowSignals::default());
    let old = {
        let routes = NativeOutputRoutes::default();
        routes.track_window("a", "avatar-a", &signals, 0).unwrap();
        begin(&routes, "a", &signals, 0)
    };
    assert!(old.cancel.load(Ordering::SeqCst));
    assert!(old.window.cancel.load(Ordering::SeqCst));
    let restarted = NativeOutputRoutes::default();
    restarted.track_window("a", "avatar-a", &signals, 0).unwrap();
    assert!(matches!(restarted.take_preparation(&old, 1), Err(RouteError::StaleWindow)));
    restarted.take_preparation(&begin(&restarted, "a", &signals, 1), 2).unwrap();
}

#[test]
fn actual_runtime_hooks_use_the_exact_registry_and_presentation_invalidation_latch() {
    let runtime = super::super::LifeRuntime::default();
    let old = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &old));
    let first = begin(&runtime.native_routes, "a", &old, 0);
    let replacement = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &replacement));
    let second = begin(&runtime.native_routes, "a", &replacement, 0);
    assert!(first.cancel.load(Ordering::SeqCst));
    assert!(!runtime.native_output_window_changed("a", &old, NativeWindowChange::Destroyed));
    assert!(!second.cancel.load(Ordering::SeqCst));
    runtime.invalidate_world_presentation(Some("a"));
    assert!(second.cancel.load(Ordering::SeqCst));
    let third = begin(&runtime.native_routes, "a", &replacement, 0);
    runtime.output_window_lost("main");
    assert!(third.cancel.load(Ordering::SeqCst));
}

