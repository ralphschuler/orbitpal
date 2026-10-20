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

fn output_ui(runtime: &super::super::LifeRuntime) {
    runtime.outputs.ui_connect("native-output-fixture");
    runtime.outputs.ui_report("native-output-fixture", 1, false);
}
fn output_watch(runtime: &super::super::LifeRuntime, bot: &str) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    assert!(runtime.outputs.watch(
        runtime.outputs.ticket().unwrap(), bot, &format!("avatar-{bot}"),
        &cancel, ("native-output-fixture", 1),
    ));
    cancel
}

#[test]
fn real_runtime_replacement_revokes_old_jobs_and_retired_events_cannot_touch_new_jobs_or_tickets() {
    let runtime = super::super::LifeRuntime::default();
    output_ui(&runtime);
    let a = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &a));
    let old_job = output_watch(&runtime, "a");
    let first = begin(&runtime.native_routes, "a", &a, 0);
    let unchanged = runtime.outputs.ticket();
    assert!(runtime.native_output_window_registered("a", "avatar-a", &a));
    assert_eq!(runtime.outputs.ticket(), unchanged);
    assert!(!old_job.load(Ordering::SeqCst));
    assert!(!first.cancel.load(Ordering::SeqCst));

    let b = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &b));
    assert!(old_job.load(Ordering::SeqCst));
    assert!(first.cancel.load(Ordering::SeqCst));
    let new_job = output_watch(&runtime, "a");
    let other_job = output_watch(&runtime, "other");
    let current = begin(&runtime.native_routes, "a", &b, 0);
    let ticket = runtime.outputs.ticket();
    for change in [NativeWindowChange::Closing, NativeWindowChange::Destroyed,
        NativeWindowChange::Geometry, NativeWindowChange::Scale, NativeWindowChange::Hidden] {
        assert!(!runtime.native_output_window_changed("a", &a, change));
        assert_eq!(runtime.outputs.ticket(), ticket);
        assert!(!new_job.load(Ordering::SeqCst));
        assert!(!other_job.load(Ordering::SeqCst));
        assert!(!current.cancel.load(Ordering::SeqCst));
    }
    let foreign = Arc::new(WindowSignals::default());
    assert!(!runtime.native_output_window_changed("a", &foreign, NativeWindowChange::Destroyed));
    assert_eq!(runtime.outputs.ticket(), ticket);
    assert!(runtime.native_output_window_changed("a", &b, NativeWindowChange::Closing));
    assert!(new_job.load(Ordering::SeqCst));
    assert!(current.cancel.load(Ordering::SeqCst));
    assert!(!other_job.load(Ordering::SeqCst));
    assert_ne!(runtime.outputs.ticket(), ticket);
    runtime.output_window_lost("main");
    assert!(other_job.load(Ordering::SeqCst));
}

#[test]
fn competing_close_or_missing_destroy_and_constructor_preserve_the_successor_watch() {
    // Both orders must work, including the missing-record path after Close.
    // Threads use the same production runtime entrypoints as the constructor
    // and native callback; no test hook can split classification/revocation.
    for close_first in [false, true] {
        for _ in 0..16 {
            let runtime = super::super::LifeRuntime::default();
            output_ui(&runtime);
            let old = Arc::new(WindowSignals::default());
            assert!(runtime.native_output_window_registered("a", "avatar-a", &old));
            if close_first {
                assert!(runtime.native_output_window_changed("a", &old, NativeWindowChange::Closing));
            }
            let old_job = output_watch(&runtime, "a");
            let replacement = Arc::new(WindowSignals::default());
            let successor_job = std::thread::scope(|scope| {
                let barrier = Arc::new(std::sync::Barrier::new(3));
                let event_barrier = barrier.clone();
                let old = old.clone();
                let event_runtime = &runtime;
                let event = scope.spawn(move || {
                    event_barrier.wait();
                    event_runtime.native_output_window_changed("a", &old, NativeWindowChange::Destroyed)
                });
                let constructor_barrier = barrier.clone();
                let constructor_runtime = &runtime;
                let constructor_replacement = replacement.clone();
                let constructor = scope.spawn(move || {
                    constructor_barrier.wait();
                    assert!(constructor_runtime.native_output_window_registered("a", "avatar-a", &constructor_replacement));
                    output_watch(constructor_runtime, "a")
                });
                barrier.wait();
                event.join().unwrap();
                constructor.join().unwrap()
            });
            assert!(old_job.load(Ordering::SeqCst));
            let current = runtime.native_routes.witness("a", "avatar-a", &replacement, runtime.now()).unwrap();
            assert!(!current.cancel.load(Ordering::SeqCst));
            assert!(!successor_job.load(Ordering::SeqCst));
        }
    }
}

#[test]
fn missing_reaped_and_capacity_failed_tracking_preserve_conservative_loss_cancellation() {
    let runtime = super::super::LifeRuntime::default();
    output_ui(&runtime);
    let old = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &old));
    let old_job = output_watch(&runtime, "a");
    drop(old); // reap must not erase the old label's loss cancellation.
    let replacement = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &replacement));
    assert!(old_job.load(Ordering::SeqCst));
    let job = output_watch(&runtime, "a");
    assert!(runtime.native_output_window_changed("a", &replacement, NativeWindowChange::Closing));
    assert!(job.load(Ordering::SeqCst));
    let untracked_job = output_watch(&runtime, "a");
    assert!(!runtime.native_output_window_changed("a", &replacement, NativeWindowChange::Destroyed));
    assert!(untracked_job.load(Ordering::SeqCst));

    let signals: Vec<_> = (0..MAX_WINDOWS).map(|i| {
        let bot = format!("b{i}");
        let signals = Arc::new(WindowSignals::default());
        assert!(runtime.native_output_window_registered(&bot, &format!("avatar-{bot}"), &signals));
        signals
    }).collect();
    let unknown = Arc::new(WindowSignals::default());
    let job = output_watch(&runtime, "overflow");
    assert!(!runtime.native_output_window_registered("overflow", "avatar-overflow", &unknown));
    assert!(job.load(Ordering::SeqCst));
    let job = output_watch(&runtime, "overflow");
    assert!(!runtime.native_output_window_changed("overflow", &unknown, NativeWindowChange::Destroyed));
    assert!(job.load(Ordering::SeqCst));
    assert_eq!(signals.len(), MAX_WINDOWS);
}

#[test]
fn poisoned_native_lifecycle_stays_unavailable_and_conservatively_cancels_output() {
    let runtime = super::super::LifeRuntime::default();
    output_ui(&runtime);
    let signals = Arc::new(WindowSignals::default());
    assert!(runtime.native_output_window_registered("a", "avatar-a", &signals));
    let pending = begin(&runtime.native_routes, "a", &signals, 0);
    let job = output_watch(&runtime, "a");
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = runtime.native_routes.state.lock().unwrap();
        panic!("synthetic native route poison");
    })).is_err());
    assert!(!runtime.native_output_window_changed("a", &signals, NativeWindowChange::Destroyed));
    assert!(job.load(Ordering::SeqCst));
    assert!(pending.cancel.load(Ordering::SeqCst));
    assert!(pending.window.cancel.load(Ordering::SeqCst));
    let job = output_watch(&runtime, "a");
    assert!(!runtime.native_output_window_registered("a", "avatar-a", &signals));
    assert!(job.load(Ordering::SeqCst));
    assert!(runtime.native_routes.state.is_poisoned());
    assert!(matches!(runtime.native_routes.witness("a", "avatar-a", &signals, 1), Err(RouteError::Unavailable)));
}

fn layout_handoff(routes:&NativeOutputRoutes,bot:&str,signals:&Arc<WindowSignals>,now:u64)->(LayoutRoute,Arc<AtomicBool>){
    let ui=routes.layout_ui().unwrap();let ticket=begin(routes,bot,signals,now);let cancel=Arc::new(AtomicBool::new(false));
    routes.retain_preparation(&ticket,signals,&cancel,&ui,now).unwrap();let route=routes.handoff_preparation(&ticket,signals,&cancel,&ui,now).unwrap();
    assert!(ticket.cancel.load(Ordering::SeqCst));assert!(!cancel.load(Ordering::SeqCst));(route,cancel)
}
fn layout_ui(routes:&NativeOutputRoutes){routes.ui_connect("studio");routes.ui_report("studio",1,false);}

#[test]
fn layout_handoff_revoke_has_no_unregistered_cancellation_interval(){
    for stop_before in [true,false]{let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);let ui=routes.layout_ui().unwrap();let ticket=begin(&routes,"a",&signals,0);let cancel=Arc::new(AtomicBool::new(false));
        if stop_before{routes.revoke(Some("a"));assert!(routes.handoff_preparation(&ticket,&signals,&cancel,&ui,0).is_err());}
        else{routes.retain_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();let route=routes.handoff_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();routes.revoke(Some("a"));assert!(cancel.load(Ordering::SeqCst));assert!(!routes.layout_current(&route,0));routes.release_layout(&route);}
    }
}
#[test]
fn expected_geometry_never_publishes_between_raw_invalidation_and_classification(){
    let routes=NativeOutputRoutes::default();let outputs=super::super::action_output::OutputGate::default();let signals=tracked(&routes,"a");layout_ui(&routes);let (route,cancel)=layout_handoff(&routes,"a",&signals,0);
    let fact=routes.begin_geometry("a",&signals,&outputs).unwrap();assert!(!routes.layout_current(&route,0));assert!(!cancel.load(Ordering::SeqCst));
    routes.finish_geometry(fact,true);assert!(routes.layout_current(&route,0));
    let fact=routes.begin_geometry("a",&signals,&outputs).unwrap();routes.revoke(Some("a"));routes.finish_geometry(fact,true);assert!(cancel.load(Ordering::SeqCst));assert!(!routes.layout_current(&route,0));routes.release_layout(&route);
}
#[test]
fn cancelled_expired_replaced_layout_slots_stay_charged_until_exact_release(){
    let routes=NativeOutputRoutes::default();layout_ui(&routes);let mut held=Vec::new();
    for i in 0..8{let bot=format!("a{i}");let signals=tracked(&routes,&bot);let (route,_)=layout_handoff(&routes,&bot,&signals,0);held.push((signals,route));}
    routes.revoke(None);let extra=tracked(&routes,"extra");let witness=routes.witness("extra","avatar-extra",&extra,1).unwrap();assert!(matches!(routes.begin_preparation(witness,1),Err(RouteError::Capacity)));
    routes.release_layout(&held[0].1);let ticket=begin(&routes,"extra",&extra,1);assert!(routes.take_preparation(&ticket,2).is_ok());
    routes.release_layout(&held[0].1);assert_eq!(routes.state.lock().unwrap().layouts.len(),7);
    for (_,route) in held.iter().skip(1){routes.release_layout(route);}
}
#[test]
fn stale_ui_reports_and_cleanup_cannot_revoke_current_layout_successor(){
    let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);let (old,old_cancel)=layout_handoff(&routes,"a",&signals,0);
    routes.ui_connect("next");routes.ui_report("next",1,false);assert!(old_cancel.load(Ordering::SeqCst));routes.release_layout(&old);
    let (current,cancel)=layout_handoff(&routes,"a",&signals,1);routes.ui_disconnect("studio");routes.ui_report("studio",999,true);routes.ui_report("next",1,true);routes.release_layout(&old);assert!(!cancel.load(Ordering::SeqCst));assert!(routes.layout_current(&current,1));
    routes.ui_report("next",2,true);assert!(cancel.load(Ordering::SeqCst));routes.release_layout(&current);
}
#[test]
fn layout_expiry_is_original_inclusive_and_cannot_be_renewed_by_observation(){
    let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);let (route,cancel)=layout_handoff(&routes,"a",&signals,10);
    assert!(routes.layout_current(&route,5010));assert!(!routes.layout_current(&route,5011));assert!(cancel.load(Ordering::SeqCst));assert!(!routes.layout_current(&route,10));routes.release_layout(&route);
}
#[test]
fn layout_replacement_and_old_geometry_finish_never_touch_new_instance(){
    let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);let outputs=super::super::action_output::OutputGate::default();let (old,cancel)=layout_handoff(&routes,"a",&signals,0);let old_fact=routes.begin_geometry("a",&signals,&outputs).unwrap();
    let next=Arc::new(WindowSignals::default());routes.track_window("a","avatar-a",&next,1).unwrap();assert!(cancel.load(Ordering::SeqCst));let (current,current_cancel)=layout_handoff(&routes,"a",&next,1);routes.finish_geometry(old_fact,true);routes.release_layout(&old);assert!(!current_cancel.load(Ordering::SeqCst));assert!(routes.layout_current(&current,1));routes.release_layout(&current);
}
#[test]
fn inactive_cleanup_is_charged_before_hub_reservation_even_when_pending_is_reaped(){
    for competing in [false,true]{let routes=NativeOutputRoutes::default();layout_ui(&routes);let ui=routes.layout_ui().unwrap();let mut held=Vec::new();
        for i in 0..8{let bot=format!("a{i}");let signals=tracked(&routes,&bot);let ticket=begin(&routes,&bot,&signals,0);let cancel=Arc::new(AtomicBool::new(false));let route=routes.retain_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();held.push((signals,ticket,route,cancel));}
        let extra=tracked(&routes,"extra");
        // No Hub owner is necessary for charging. A canceled/dropped old
        // Pending can be reaped concurrently with a new admission, but its
        // registered inactive cleanup ID still occupies the same one slot.
        held[0].1.cancel.store(true,Ordering::SeqCst);
        if competing{let reaped_signals=held[0].0.clone();std::thread::scope(|scope|{let (barrier_a,barrier_b)=(Arc::new(std::sync::Barrier::new(2)),Arc::new(std::sync::Barrier::new(2)));let a=barrier_a.clone();let b=barrier_b.clone();let routes=&routes;let reaper=scope.spawn(move||{a.wait();let _=routes.witness("a0","avatar-a0",&reaped_signals,1);b.wait();});barrier_a.wait();let witness=routes.witness("extra","avatar-extra",&extra,1).unwrap();assert!(matches!(routes.begin_preparation(witness,1),Err(RouteError::Capacity)));barrier_b.wait();reaper.join().unwrap();});}
        else{let witness=routes.witness("extra","avatar-extra",&extra,1).unwrap();assert!(matches!(routes.begin_preparation(witness,1),Err(RouteError::Capacity)));}
        assert!(routes.handoff_preparation(&held[0].1,&held[0].0,&held[0].3,&ui,1).is_err());
        routes.release_layout(&held[0].2);let ticket=begin(&routes,"extra",&extra,1);routes.cancel_preparation(&ticket);
        for (_,_,route,_) in &held{routes.release_layout(route);}
    }
}


#[test]
fn typed_layout_state_separates_live_classification_from_current_proof(){
    let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);
    let outputs=super::super::action_output::OutputGate::default();let (route,cancel)=layout_handoff(&routes,"a",&signals,0);
    assert_eq!(routes.layout_state(&route,0),LayoutRouteState::Current);
    let fact=routes.begin_geometry("a",&signals,&outputs).unwrap();
    assert_eq!(routes.layout_state(&route,0),LayoutRouteState::Classifying);
    assert!(!routes.layout_current(&route,0));assert!(!cancel.load(Ordering::SeqCst));
    routes.finish_geometry(fact,true);assert_eq!(routes.layout_state(&route,0),LayoutRouteState::Current);
    let fact=routes.begin_geometry("a",&signals,&outputs).unwrap();routes.finish_geometry(fact,false);
    assert_eq!(routes.layout_state(&route,0),LayoutRouteState::Revoked);assert!(cancel.load(Ordering::SeqCst));routes.release_layout(&route);
}

#[test]
fn revoked_route_conditions_take_precedence_over_open_classification(){
    for reason in ["stop","expired","backward","replacement","poison"]{
        let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);
        let outputs=super::super::action_output::OutputGate::default();let (route,cancel)=layout_handoff(&routes,"a",&signals,10);
        let fact=routes.begin_geometry("a",&signals,&outputs).unwrap();
        assert_eq!(routes.layout_state(&route,10),LayoutRouteState::Classifying);
        let mut now=10;let mut replacement=None;
        match reason{
            "stop"=>routes.revoke(Some("a")),
            "expired"=>now=5011,
            "backward"=>now=9,
            "replacement"=>{let next=Arc::new(WindowSignals::default());routes.track_window("a","avatar-a",&next,10).unwrap();replacement=Some(next);},
            "poison"=>assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{let _guard=routes.state.lock().unwrap();panic!("synthetic route classification poison");})).is_err()),
            _=>unreachable!(),
        }
        assert_eq!(routes.layout_state(&route,now),LayoutRouteState::Revoked);
        assert!(cancel.load(Ordering::SeqCst));routes.finish_geometry(fact,true);
        assert_eq!(routes.layout_state(&route,10),LayoutRouteState::Revoked);
        drop(replacement);routes.release_layout(&route);
    }
}

#[test]
fn inactive_and_foreign_layout_records_cannot_be_classifying_or_current(){
    let routes=NativeOutputRoutes::default();let signals=tracked(&routes,"a");layout_ui(&routes);
    let ui=routes.layout_ui().unwrap();let ticket=begin(&routes,"a",&signals,0);let cancel=Arc::new(AtomicBool::new(false));
    let route=routes.retain_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();
    assert_eq!(routes.layout_state(&route,0),LayoutRouteState::Revoked,"inactive cleanup capacity is not layout proof");
    let active=routes.handoff_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();
    let mut foreign=active.clone();foreign.cancel=Arc::new(AtomicBool::new(false));
    assert_eq!(routes.layout_state(&foreign,0),LayoutRouteState::Revoked);
    assert_eq!(routes.layout_state(&active,0),LayoutRouteState::Current);assert!(!cancel.load(Ordering::SeqCst));routes.release_layout(&active);
}
