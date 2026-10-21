//! Synthetic native callback/authentication ownership; no WebView or Ready.
use super::*;

fn signals() -> Arc<WindowSignals> { Arc::new(WindowSignals::default()) }
fn loaded(documents: &Documents, bot: &str, signals: &Arc<WindowSignals>, now: u64) -> DocumentGrant {
    assert!(documents.track(bot, signals));
    assert!(documents.started(bot, signals));
    let grant = documents.finished(bot, signals, now).ok().unwrap();
    // Pure document tests use synthetic cancellation identities only. These
    // are not constructor receipts, native Scene permits or renderer grants.
    documents.bind_delivery(&grant, grant.delivery_cancel(), Arc::new(AtomicBool::new(false))).ok().unwrap();
    grant
}
fn authenticate(documents: &Documents, grant: &DocumentGrant, mount: &str, now: u64) -> AuthReceipt {
    if grant.delivery.lease.get().is_none() {
        documents.bind_delivery(grant, grant.delivery_cancel(), Arc::new(AtomicBool::new(false))).ok().unwrap();
    }
    documents.authenticate(&format!("avatar-{}", grant.proof().bot()), grant.capability(), mount, Some(7), now).ok().unwrap()
}

#[test]
fn first_auth_requires_exact_original_delivery_and_lease_binding() {
    let documents = Documents::default(); let signals = signals();
    assert!(documents.track("nova", &signals)); assert!(documents.started("nova", &signals));
    let grant = documents.finished("nova", &signals, 100).ok().unwrap();
    assert!(grant.delivery_current()); // cancellation only, still no auth
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", Some(7), 101).is_err());
    let lease = Arc::new(AtomicBool::new(false));
    assert!(documents.bind_delivery(&grant, Arc::new(AtomicBool::new(false)), lease.clone()).is_err());
    assert!(grant.delivery.lease.get().is_none());
    documents.bind_delivery(&grant, grant.delivery_cancel(), lease.clone()).ok().unwrap();
    documents.bind_delivery(&grant, grant.delivery_cancel(), lease).ok().unwrap();
    assert!(authenticate(&documents, &grant, "mount", 102).current());
}

#[test]
fn delivered_unpinned_capability_cannot_rebind_after_semantic_revocation_and_clear_ui() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 100);
    let outputs = crate::life_runtime::action_output::OutputGate::default();
    outputs.ui_connect("current-ui"); outputs.ui_report("current-ui", 1, false);
    assert!(outputs.watch_delivery(outputs.ticket().unwrap(), "nova", "avatar-nova", &grant.delivery_cancel()));
    outputs.semantic_geometry(Some("nova"));
    outputs.ui_report("current-ui", 2, false);
    assert!(grant.proof().current()); assert!(!grant.delivery_current());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "old-mount", Some(7), 102).is_err());
    let retry = documents.finished("nova", &signals, 103).ok().unwrap();
    assert_eq!(retry.capability(), grant.capability()); assert_eq!(retry.issued_at(), 100);
    assert!(Arc::ptr_eq(&retry.delivery_cancel(), &grant.delivery_cancel()));
    assert!(!outputs.watch_delivery(outputs.ticket().unwrap(), "nova", "avatar-nova", &retry.delivery_cancel()));
    assert!(documents.bind_delivery(&retry, retry.delivery_cancel(), Arc::new(AtomicBool::new(false))).is_err());
    let next = documents.rotate("nova", &signals, 104).ok().unwrap();
    assert!(next.delivery_current()); assert!(!Arc::ptr_eq(&next.delivery_cancel(), &grant.delivery_cancel()));
    assert!(outputs.watch_delivery(outputs.ticket().unwrap(), "nova", "avatar-nova", &next.delivery_cancel()));
    assert!(authenticate(&documents, &next, "new-mount", 105).current());
}

#[test]
fn delivery_cancellation_revokes_pinned_auth_reports_and_renewals_atomically() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &grant, "mount", 1); let clone = auth.clone();
    grant.delivery_cancel().store(true, Ordering::SeqCst);
    assert!(!auth.current()); assert!(!clone.current()); assert!(grant.proof().current());
    assert!(documents.authenticate_bound("avatar-nova", grant.capability(), 2).is_err());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", None, 2).is_err());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "other", Some(7), 2).is_err());
    let retry = documents.current_grant("nova", &signals, 3).ok().unwrap();
    assert!(!retry.delivery_current()); assert!(Arc::ptr_eq(&retry.delivery_cancel(), &grant.delivery_cancel()));
}

#[test]
fn exact_lease_cancellation_cannot_be_replaced_by_successor_on_same_document() {
    let documents = Documents::default(); let signals = signals();
    assert!(documents.track("nova", &signals)); assert!(documents.started("nova", &signals));
    let grant = documents.finished("nova", &signals, 0).ok().unwrap();
    let lease = Arc::new(AtomicBool::new(false)); let unrelated = Arc::new(AtomicBool::new(false));
    documents.bind_delivery(&grant, grant.delivery_cancel(), lease.clone()).ok().unwrap();
    let auth = authenticate(&documents, &grant, "mount", 1);
    unrelated.store(true, Ordering::SeqCst); assert!(auth.current());
    assert!(documents.bind_delivery(&grant, grant.delivery_cancel(), Arc::new(AtomicBool::new(false))).is_err());
    assert!(auth.current());
    lease.store(true, Ordering::SeqCst); assert!(!auth.current());
    let retry = documents.finished("nova", &signals, 2).ok().unwrap();
    assert!(documents.bind_delivery(&retry, retry.delivery_cancel(), Arc::new(AtomicBool::new(false))).is_err());
    assert!(documents.authenticate("avatar-nova", retry.capability(), "mount", None, 3).is_err());
    assert!(grant.proof().current());
}

#[test]
fn cancelled_first_lease_binding_is_terminal_even_before_any_authentication() {
    let documents = Documents::default(); let signals = signals();
    assert!(documents.track("nova", &signals)); assert!(documents.started("nova", &signals));
    let grant = documents.finished("nova", &signals, 0).ok().unwrap();
    let cancelled = Arc::new(AtomicBool::new(true));
    assert!(documents.bind_delivery(&grant, grant.delivery_cancel(), cancelled.clone()).is_err());
    assert!(Arc::ptr_eq(grant.delivery.lease.get().unwrap(), &cancelled));
    assert!(documents.bind_delivery(&grant, grant.delivery_cancel(), Arc::new(AtomicBool::new(false))).is_err());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", Some(7), 1).is_err());
}

#[test]
fn competing_delivery_bindings_publish_only_one_exact_lease() {
    for _ in 0..16 {
        let documents = Arc::new(Documents::default()); let signals = signals();
        assert!(documents.track("nova", &signals)); assert!(documents.started("nova", &signals));
        let grant = documents.finished("nova", &signals, 0).ok().unwrap();
        let a = Arc::new(AtomicBool::new(false)); let b = Arc::new(AtomicBool::new(false));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let joins = [a.clone(), b.clone()].map(|lease| {
            let documents=documents.clone(); let grant=grant.clone(); let barrier=barrier.clone();
            std::thread::spawn(move || { barrier.wait(); documents.bind_delivery(&grant, grant.delivery_cancel(), lease).is_ok() })
        });
        let [first, second] = joins.map(|join| join.join().unwrap()); assert_ne!(first, second);
        let auth = authenticate(&documents, &grant, "mount", 1);
        let (winner, loser) = if first { (a,b) } else { (b,a) };
        loser.store(true, Ordering::SeqCst); assert!(auth.current());
        winner.store(true, Ordering::SeqCst); assert!(!auth.current());
    }
}

#[test]
fn stale_grant_cannot_attach_delivery_to_rotated_or_foreign_registry() {
    let documents = Documents::default(); let signals = signals();
    let old = loaded(&documents, "nova", &signals, 0);
    let new = documents.rotate("nova", &signals, 1).ok().unwrap();
    assert!(documents.bind_delivery(&old, old.delivery_cancel(), Arc::new(AtomicBool::new(false))).is_err());
    assert!(new.delivery_current()); assert!(new.delivery.lease.get().is_none());
    let foreign = Documents::default(); let _ = loaded(&foreign, "nova", &signals, 1);
    assert!(foreign.bind_delivery(&new, new.delivery_cancel(), Arc::new(AtomicBool::new(false))).is_err());
    assert!(new.delivery_current());
    assert!(authenticate(&documents, &new, "new", 2).current());
}

#[test]
fn raw_layout_geometry_keeps_auth_but_semantic_change_cancels_the_same_delivery() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let outputs = crate::life_runtime::action_output::OutputGate::default();
    assert!(outputs.watch_delivery(outputs.ticket().unwrap(), "nova", "avatar-nova", &grant.delivery_cancel()));
    let auth = authenticate(&documents, &grant, "mount", 1);
    outputs.geometry(Some("nova")); assert!(auth.current());
    outputs.semantic_geometry(Some("other")); assert!(auth.current());
    outputs.semantic_geometry(Some("nova")); assert!(!auth.current());
    assert!(grant.proof().current());
}

#[test]
fn finishing_requires_exact_tracked_constructor_and_real_started_phase() {
    let documents = Documents::default(); let signals = signals();
    assert!(matches!(documents.finished("nova", &signals, 0), Err(DocumentError::Stale)));
    assert!(!documents.started("nova", &signals));
    assert!(documents.track("nova", &signals));
    assert!(matches!(documents.finished("nova", &signals, 0), Err(DocumentError::Pending)));
    assert!(documents.started("nova", &signals));
    assert!(documents.finished("nova", &signals, 0).is_ok());
}

#[test]
fn duplicate_finish_retries_same_capability_and_same_document_without_renewing_issue_time() {
    let documents = Documents::default(); let signals = signals();
    let first = loaded(&documents, "nova", &signals, 100);
    assert!(valid_capability(first.capability()));
    let retry = documents.finished("nova", &signals, 5_099).ok().unwrap();
    assert_eq!(first.capability(), retry.capability());
    assert!(first.proof().same_document(&retry.proof()));
    assert!(Arc::ptr_eq(first.proof().signals(), &signals));
    assert!(matches!(documents.authenticate("avatar-nova", retry.capability(), "mount", Some(1), 5_100), Err(DocumentError::Expired)));
}

#[test]
fn real_reload_revokes_old_auth_before_the_new_page_finishes() {
    let documents = Documents::default(); let signals = signals();
    let old = loaded(&documents, "nova", &signals, 0);
    let old_auth = authenticate(&documents, &old, "old-mount", 1);
    assert!(documents.started("nova", &signals));
    assert!(!old.proof().current()); assert!(!old_auth.current());
    assert!(matches!(documents.authenticate("avatar-nova", old.capability(), "old-mount", None, 2), Err(DocumentError::Pending)));
    let new = documents.finished("nova", &signals, 2).ok().unwrap();
    assert_ne!(old.capability(), new.capability());
    assert!(!old.proof().same_document(&new.proof()));
    let current = authenticate(&documents, &new, "new-mount", 3);
    assert!(documents.authenticate("avatar-nova", old.capability(), "old-mount", None, 3).is_err());
    assert!(!documents.unregister(&old_auth)); assert!(current.current());
}

#[test]
fn same_label_new_arc_revokes_old_and_old_callbacks_do_not_touch_successor() {
    let documents = Documents::default(); let old_signals = signals(); let replacement = signals();
    let old = loaded(&documents, "nova", &old_signals, 0);
    let old_auth = authenticate(&documents, &old, "old", 1);
    let new = loaded(&documents, "nova", &replacement, 2);
    let auth = authenticate(&documents, &new, "new", 3);
    assert!(!old.proof().current()); assert!(!old_auth.current());
    assert!(!documents.started("nova", &old_signals));
    assert!(documents.finished("nova", &old_signals, 4).is_err());
    assert!(documents.rotate("nova", &old_signals, 4).is_err());
    assert!(!documents.retire("nova", &old_signals));
    assert!(!documents.unregister(&old_auth));
    assert!(auth.current()); assert!(Arc::ptr_eq(auth.document().signals(), &replacement));
}

#[test]
fn repeated_same_arc_track_is_inert_for_loaded_and_authenticated_document() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &grant, "mount", 1);
    assert!(documents.track("nova", &signals));
    let retry = documents.finished("nova", &signals, 2).ok().unwrap();
    assert_eq!(retry.capability(), grant.capability());
    assert!(auth.current()); assert!(auth.document().same_document(&retry.proof()));
}

#[test]
fn first_register_requires_positive_warm_frame_and_strict_original_five_seconds() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 100);
    for warm in [None, Some(0), Some(MAX_COUNTER + 1)] {
        assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", warm, 100).is_err());
    }
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", Some(MAX_COUNTER), 5_099).is_ok());
    let replacement = documents.rotate("nova", &signals, 6_000).ok().unwrap();
    assert!(matches!(documents.authenticate("avatar-nova", replacement.capability(), "new", Some(1), 11_000), Err(DocumentError::Expired)));
    assert!(documents.authenticate("avatar-nova", replacement.capability(), "new", Some(1), 5_999).is_err());
}

#[test]
fn registered_document_renews_same_mount_without_ttl_or_warm_retiming() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &grant, "mount", 1);
    let renewed = documents.authenticate("avatar-nova", grant.capability(), "mount", None, 60_000).ok().unwrap();
    assert!(auth.same_auth(&renewed)); assert_eq!(renewed.normal_warm_frame_sequence(), 7);
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", Some(8), 60_001).is_err());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "other", Some(7), 60_001).is_err());
    assert!(renewed.current());
}

#[test]
fn unregistration_is_terminal_and_late_unregistration_cannot_cancel_rotation() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &grant, "mount", 1);
    assert!(documents.unregister(&auth)); assert!(!documents.unregister(&auth));
    assert!(!auth.current()); assert!(grant.proof().current());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", Some(7), 2).is_err());
    assert!(documents.authenticate("avatar-nova", grant.capability(), "other", Some(8), 2).is_err());
    let next = documents.rotate("nova", &signals, 3).ok().unwrap();
    let current = authenticate(&documents, &next, "other", 4);
    assert!(!grant.proof().current()); assert!(!documents.unregister(&auth));
    assert!(current.current());
}

#[test]
fn deliberate_rotation_requires_finished_page_and_creates_fresh_capability_epoch_and_mount() {
    let documents = Documents::default(); let signals = signals();
    assert!(documents.track("nova", &signals));
    assert!(matches!(documents.rotate("nova", &signals, 0), Err(DocumentError::Pending)));
    assert!(documents.started("nova", &signals));
    assert!(matches!(documents.rotate("nova", &signals, 0), Err(DocumentError::Pending)));
    let old = documents.finished("nova", &signals, 100).ok().unwrap();
    let old_auth = authenticate(&documents, &old, "old", 101);
    assert!(matches!(documents.rotate("nova", &signals, 99), Err(DocumentError::Expired)));
    assert!(old_auth.current());
    let next = documents.rotate("nova", &signals, 102).ok().unwrap();
    assert_ne!(old.capability(), next.capability());
    assert!(!old_auth.current()); assert!(!old.proof().same_document(&next.proof()));
    let current = authenticate(&documents, &next, "new", 103);
    let retry = documents.finished("nova", &signals, 104).ok().unwrap();
    assert_eq!(next.capability(), retry.capability()); assert!(current.current());
}

#[test]
fn cross_bot_or_wrong_label_authentication_never_mutates_valid_owner() {
    let documents = Documents::default();
    let a = loaded(&documents, "nova", &signals(), 0);
    let b = loaded(&documents, "other", &signals(), 0);
    let good = authenticate(&documents, &b, "right", 1);
    for label in ["main", "avatar-", "avatar-other", "avatar-nova/extra"] {
        assert!(documents.authenticate(label, a.capability(), "wrong", Some(1), 2).is_err());
    }
    assert!(good.current()); assert!(!a.proof().same_document(&b.proof()));
}

#[test]
fn closed_token_and_mount_input_refuses_malformed_values_without_revoking_current_auth() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &grant, "mount", 1);
    for cap in ["".to_owned(), "a".repeat(63), "g".repeat(64), "A".repeat(64), format!("{}\n", grant.capability())] {
        assert!(documents.authenticate("avatar-nova", &cap, "mount", None, 2).is_err());
    }
    for mount in ["".to_owned(), "x".repeat(97), "mount/other".into(), "mount\0".into()] {
        assert!(documents.authenticate("avatar-nova", grant.capability(), &mount, None, 2).is_err());
    }
    assert!(documents.authenticate("avatar-nova", grant.capability(), "mount", None, MAX_COUNTER + 1).is_err());
    assert!(auth.current());
}

#[test]
fn exact_retirement_invalidates_all_clones_but_retrack_never_reuses_old_epoch() {
    let documents = Documents::default(); let signals = signals();
    let old = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &old, "mount", 1); let clone = auth.clone();
    assert!(documents.retire("nova", &signals)); assert!(!auth.current()); assert!(!clone.current());
    let new = loaded(&documents, "nova", &signals, 2);
    assert!(!old.proof().same_document(&new.proof()));
    let current = authenticate(&documents, &new, "mount", 3);
    assert!(!documents.unregister(&auth)); assert!(current.current());
}

#[test]
fn bounded_constructor_records_reject_extra_without_evicting_existing_documents() {
    let documents = Documents::default(); let mut retained = Vec::new();
    for i in 0..MAX_DOCUMENTS {
        let s = signals(); let bot = format!("bot-{i}");
        let grant = loaded(&documents, &bot, &s, 0);
        retained.push((bot, s, grant));
    }
    assert!(!documents.track("overflow", &signals()));
    assert_eq!(documents.state.lock().unwrap().entries.len(), MAX_DOCUMENTS);
    assert!(retained.iter().all(|(_, _, grant)| grant.proof().current()));
    assert!(documents.retire(&retained[0].0, &retained[0].1));
    assert!(documents.track("replacement", &signals()));
    assert_eq!(documents.state.lock().unwrap().entries.len(), MAX_DOCUMENTS);
}

#[test]
fn epoch_exhaustion_revokes_all_and_never_wraps_or_recovers_authority() {
    let documents = Documents::default(); let a = signals(); let b = signals();
    let grant = loaded(&documents, "nova", &a, 0); let auth = authenticate(&documents, &grant, "mount", 1);
    loaded(&documents, "other", &b, 0);
    documents.state.lock().unwrap().next = MAX_COUNTER;
    assert!(!documents.started("other", &b));
    assert!(!auth.current()); assert!(!grant.proof().current());
    assert!(!documents.track("third", &signals()));
    assert!(matches!(documents.finished("nova", &a, 2), Err(DocumentError::Unavailable)));
    assert_eq!(documents.state.lock().unwrap().next, MAX_COUNTER);
}

#[test]
fn poisoned_registry_cancels_retained_proofs_and_never_clears_poison() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0); let auth = authenticate(&documents, &grant, "mount", 1);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = documents.state.lock().unwrap(); panic!("synthetic registry poison");
    })).is_err());
    assert!(!documents.track("other", &Arc::new(WindowSignals::default())));
    assert!(!grant.proof().current()); assert!(!auth.current());
    assert!(documents.state.is_poisoned());
    assert!(matches!(documents.rotate("nova", &signals, 2), Err(DocumentError::Unavailable)));
}

#[test]
fn registry_drop_revokes_owned_document_and_authentication_without_caller_cleanup() {
    let signals = signals();
    let (grant, auth) = {
        let documents = Documents::default();
        let grant = loaded(&documents, "nova", &signals, 0); let auth = authenticate(&documents, &grant, "mount", 1);
        (grant, auth)
    };
    assert!(!grant.proof().current()); assert!(!auth.current());
}

#[test]
fn equal_bot_epoch_and_window_arc_in_distinct_registries_do_not_share_document_authority() {
    let a = Documents::default(); let b = Documents::default(); let signals = signals();
    let first = loaded(&a, "nova", &signals, 0); let second = loaded(&b, "nova", &signals, 0);
    assert!(!first.proof().same_document(&second.proof()));
    assert_ne!(first.capability(), second.capability());
    let auth = authenticate(&b, &second, "mount", 1);
    assert!(a.authenticate("avatar-nova", second.capability(), "mount", Some(7), 1).is_err());
    assert!(auth.current());
}

#[test]
fn competing_replacement_and_retired_callbacks_preserve_live_successor() {
    for _ in 0..16 {
        let documents = Arc::new(Documents::default()); let a = signals(); let b = signals();
        let old = loaded(&documents, "nova", &a, 0); let old_auth = authenticate(&documents, &old, "old", 1);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let registry = documents.clone(); let replacement = b.clone(); let gate = barrier.clone();
        let successor = std::thread::spawn(move || {
            gate.wait();
            let grant = loaded(&registry, "nova", &replacement, 2);
            authenticate(&registry, &grant, "new", 3)
        });
        let registry = documents.clone(); let gate = barrier.clone();
        let retired = std::thread::spawn(move || {
            gate.wait();
            registry.started("nova", &a);
            registry.retire("nova", &a);
            registry.unregister(&old_auth);
        });
        let auth = successor.join().unwrap(); retired.join().unwrap();
        assert!(auth.current()); assert!(Arc::ptr_eq(auth.document().signals(), &b));
        assert!(!old.proof().current());
    }
}

#[test]
fn bound_reports_cannot_create_first_auth_or_use_foreign_or_old_document() {
    let documents = Documents::default(); let signals = signals();
    let old = loaded(&documents, "nova", &signals, 100);
    assert!(documents.authenticate_bound("avatar-nova", old.capability(), 100).is_err());
    assert!(documents.state.lock().unwrap().entries.get("nova").is_some_and(|entry|
        matches!(&entry.document.as_ref().unwrap().phase, Phase::Finished { pinned: None, .. })));
    let old_auth = authenticate(&documents, &old, "first", 101);
    assert!(documents.authenticate_bound("main", old.capability(), 102).is_err());
    assert!(documents.authenticate_bound("avatar-other", old.capability(), 102).is_err());
    assert!(documents.authenticate_bound("avatar-nova", old.capability(), 99).is_err());
    assert!(old_auth.current());
    let next = documents.rotate("nova", &signals, 103).ok().unwrap();
    let current = authenticate(&documents, &next, "second", 104);
    assert!(documents.authenticate_bound("avatar-nova", old.capability(), 105).is_err());
    let bound = documents.authenticate_bound("avatar-nova", next.capability(), 60_000).ok().unwrap();
    assert!(bound.same_auth(&current)); assert_eq!(bound.mount_id(), "second");
    assert_eq!(bound.normal_warm_frame_sequence(), 7);
}

#[test]
fn bound_reports_cannot_revive_terminal_authentication_or_pending_navigation() {
    let documents = Documents::default(); let signals = signals();
    let grant = loaded(&documents, "nova", &signals, 0);
    let auth = authenticate(&documents, &grant, "mount", 1);
    assert!(documents.unregister(&auth));
    assert!(documents.authenticate_bound("avatar-nova", grant.capability(), 2).is_err());
    assert!(!auth.current());
    assert!(documents.started("nova", &signals));
    assert!(matches!(documents.authenticate_bound("avatar-nova", grant.capability(), 3), Err(DocumentError::Pending)));
}

#[test]
fn current_grant_cannot_finish_pending_page_or_retime_finished_bootstrap_deadline() {
    let documents = Documents::default(); let signals = signals();
    assert!(documents.track("nova", &signals)); assert!(documents.started("nova", &signals));
    assert!(matches!(documents.current_grant("nova", &signals, 0), Err(DocumentError::Pending)));
    let grant = documents.finished("nova", &signals, 100).ok().unwrap();
    let retry = documents.current_grant("nova", &signals, 5_100).ok().unwrap();
    assert_eq!(grant.capability(), retry.capability());
    assert_eq!(grant.issued_at(), 100); assert_eq!(retry.issued_at(), 100);
    assert_eq!(documents.finished("nova", &signals, 5_101).ok().unwrap().issued_at(), 100);
    assert!(documents.authenticate("avatar-nova", retry.capability(), "mount", Some(1), 5_100).is_err());
}

#[test]
fn native_invalidation_revokes_exact_document_without_promoting_or_touching_replacement() {
    let documents = Documents::default(); let a = signals(); let b = signals();
    let old = loaded(&documents, "nova", &a, 0);
    let new = loaded(&documents, "nova", &b, 1); let auth = authenticate(&documents, &new, "mount", 2);
    assert!(!documents.invalidate("nova", &a)); assert!(auth.current());
    assert!(documents.invalidate("nova", &b)); assert!(!auth.current()); assert!(!new.proof().current());
    assert!(documents.current_grant("nova", &b, 3).is_err());
    assert!(documents.finished("nova", &b, 3).is_err());
    assert!(documents.started("nova", &b));
    assert!(matches!(documents.current_grant("nova", &b, 3), Err(DocumentError::Pending)));
    let finished = documents.finished("nova", &b, 3).ok().unwrap();
    assert!(!finished.proof().same_document(&old.proof())); assert!(finished.proof().current());
}
