//! Actual cancellation registry only; no native constructor/Scene/Ready grant.
use super::*;

fn atom() -> Arc<AtomicBool> { Arc::new(AtomicBool::new(false)) }
fn attach(gate: &OutputGate, bot: &str) -> Arc<AtomicBool> {
    let cancel = atom();
    assert!(gate.watch_delivery(gate.ticket().unwrap(), bot, &format!("avatar-{bot}"), &cancel));
    cancel
}

#[test]
fn delivery_watch_is_cancellation_only_without_ui_or_surface_authorization() {
    let gate = OutputGate::default(); let cancel = attach(&gate, "nova");
    let state = gate.state.lock().unwrap();
    assert!(state.ui.is_none()); assert!(state.surfaces.is_empty()); assert!(state.watches.is_empty());
    assert_eq!(state.deliveries.len(), 1); assert!(!cancel.load(Ordering::SeqCst));
    drop(state);
    let wrong = atom();
    assert!(!gate.watch_delivery(gate.ticket().unwrap(), "nova", "main", &wrong));
    assert!(wrong.load(Ordering::SeqCst)); assert!(!cancel.load(Ordering::SeqCst));
}

#[test]
fn semantic_scope_revokes_only_matching_delivery_while_raw_geometry_is_inert() {
    let gate = OutputGate::default(); let a = attach(&gate, "a"); let b = attach(&gate, "b");
    gate.geometry(Some("a")); assert!(!a.load(Ordering::SeqCst)); assert!(!b.load(Ordering::SeqCst));
    gate.semantic_geometry(Some("a")); assert!(a.load(Ordering::SeqCst)); assert!(!b.load(Ordering::SeqCst));
    assert!(!gate.watch_delivery(gate.ticket().unwrap(), "a", "avatar-a", &a));
    gate.bot(Some("unrelated")); assert!(!b.load(Ordering::SeqCst));
    gate.window("avatar-a"); assert!(!b.load(Ordering::SeqCst));
    gate.window("main"); assert!(b.load(Ordering::SeqCst));
}

#[test]
fn window_and_operator_revocation_cover_exact_and_global_delivery_scopes() {
    let gate = OutputGate::default(); let a = attach(&gate, "a"); let b = attach(&gate, "b");
    gate.window("avatar-a"); assert!(a.load(Ordering::SeqCst)); assert!(!b.load(Ordering::SeqCst));
    let next = attach(&gate, "a"); gate.bot(None);
    assert!(next.load(Ordering::SeqCst)); assert!(b.load(Ordering::SeqCst));
}

#[test]
fn stale_ui_or_clear_heartbeat_cannot_revoke_or_revive_a_delivery() {
    let gate = OutputGate::default(); gate.ui_connect("current"); gate.ui_report("current", 10, false);
    let current = attach(&gate, "nova"); let original = gate.ticket();
    gate.ui_report("retired", crate::life_domain::MAX_COUNTER, true);
    gate.ui_report("current", 10, true); gate.ui_report("current", crate::life_domain::MAX_COUNTER + 1, true);
    gate.ui_disconnect("retired"); gate.ui_connect("");
    assert_eq!(gate.ticket(), original); assert!(!current.load(Ordering::SeqCst));
    gate.ui_report("current", 11, false); assert!(!current.load(Ordering::SeqCst));
    gate.ui_report("current", 12, true); assert!(current.load(Ordering::SeqCst));
    gate.ui_report("current", 13, false);
    assert!(!gate.watch_delivery(gate.ticket().unwrap(), "nova", "avatar-nova", &current));
    let reconnect = attach(&gate, "nova"); gate.ui_connect("replacement");
    assert!(reconnect.load(Ordering::SeqCst));
    let fresh = attach(&gate, "nova"); gate.ui_disconnect("current"); assert!(!fresh.load(Ordering::SeqCst));
    gate.ui_disconnect("replacement"); assert!(fresh.load(Ordering::SeqCst));
}

#[test]
fn actual_studio_binding_advance_fences_new_attach_without_revoking_admitted_delivery() {
    use crate::world_output::{Readiness, RegisteredWorldActor, RenderRole};
    let gate = OutputGate::default(); gate.ui_connect("studio"); gate.ui_report("studio", 1, false);
    let original = gate.ticket().unwrap(); let delivery = attach(&gate, "nova"); let surface = atom();
    // Only the real OutputGate admission boundary is exercised. This synthetic
    // Studio metadata callback does not fabricate a Native SourceRoute/Ready.
    gate.admit_surface(original, "other", "main", ("studio", 1), || Ok((RegisteredWorldActor {
        version:1, render_session_id:"studio-render".into(), binding_sequence:1,
        surface_id:"studio-surface".into(), role:RenderRole::Mirror, readiness:Readiness::AwaitingLayout,
    }, surface.clone(), true))).ok().unwrap();
    assert_ne!(gate.ticket(), Some(original)); assert!(!delivery.load(Ordering::SeqCst));
    assert!(gate.watch_delivery(original, "nova", "avatar-nova", &delivery));
    let stale = atom(); assert!(!gate.watch_delivery(original, "new", "avatar-new", &stale));
    assert!(stale.load(Ordering::SeqCst)); assert!(!delivery.load(Ordering::SeqCst));
}

#[test]
fn delivery_capacity_is_bounded_and_only_dead_or_cancelled_slots_are_reclaimed() {
    let gate = OutputGate::default(); let mut retained = Vec::new();
    for i in 0..LIMIT { retained.push(attach(&gate, &format!("bot-{i}"))); }
    let overflow = atom();
    assert!(!gate.watch_delivery(gate.ticket().unwrap(), "overflow", "avatar-overflow", &overflow));
    assert!(overflow.load(Ordering::SeqCst)); assert!(retained.iter().all(|atom| !atom.load(Ordering::SeqCst)));
    retained[0].store(true, Ordering::SeqCst); let replacement = attach(&gate, "replacement");
    assert_eq!(gate.state.lock().unwrap().deliveries.len(), LIMIT);
    assert!(!gate.watch_delivery(gate.ticket().unwrap(), "bot-0", "avatar-bot-0", &retained[0]));
    retained.pop(); let extra = attach(&gate, "extra");
    assert!(!replacement.load(Ordering::SeqCst)); assert!(!extra.load(Ordering::SeqCst));
    assert_eq!(gate.state.lock().unwrap().deliveries.len(), LIMIT);
}

#[test]
fn stale_ticket_and_counter_exhaustion_cancel_candidate_without_touching_other_delivery() {
    let gate = OutputGate::default(); let original = gate.ticket().unwrap();
    gate.bot(Some("a")); let stale = atom();
    assert!(!gate.watch_delivery(original, "a", "avatar-a", &stale)); assert!(stale.load(Ordering::SeqCst));
    let retained = attach(&gate, "b"); gate.state.lock().unwrap().generation = crate::life_domain::MAX_COUNTER;
    let excess = atom(); assert!(!gate.watch_delivery(crate::life_domain::MAX_COUNTER, "a", "avatar-a", &excess));
    assert!(excess.load(Ordering::SeqCst)); assert!(!retained.load(Ordering::SeqCst));
    gate.bot(Some("b")); assert!(retained.load(Ordering::SeqCst));
}

#[test]
fn poisoned_or_dropped_output_gate_cancels_deliveries_without_recovering_authority() {
    let gate = OutputGate::default(); let retained = attach(&gate, "nova");
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = gate.state.lock().unwrap(); panic!("synthetic output poison");
    })).is_err());
    assert!(gate.ticket().is_none()); assert!(retained.load(Ordering::SeqCst)); assert!(gate.state.is_poisoned());
    let candidate = atom(); assert!(!gate.watch_delivery(0, "other", "avatar-other", &candidate));
    assert!(candidate.load(Ordering::SeqCst));
    let after_drop = { let gate = OutputGate::default(); attach(&gate, "nova") };
    assert!(after_drop.load(Ordering::SeqCst));
}

#[test]
fn competing_initial_attach_and_semantic_revoke_never_leave_a_live_delivery() {
    for _ in 0..32 {
        let gate = Arc::new(OutputGate::default()); let ticket = gate.ticket().unwrap(); let cancel = atom();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let joining_gate = gate.clone(); let joining_atom = cancel.clone(); let joining_barrier = barrier.clone();
        let attach = std::thread::spawn(move || {
            joining_barrier.wait(); joining_gate.watch_delivery(ticket, "nova", "avatar-nova", &joining_atom)
        });
        let revoke = std::thread::spawn(move || { barrier.wait(); gate.semantic_geometry(Some("nova")); });
        let _ = attach.join().unwrap(); revoke.join().unwrap();
        assert!(cancel.load(Ordering::SeqCst));
    }
}
