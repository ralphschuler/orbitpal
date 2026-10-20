//! Actual Core/Store/World slot ownership. No renderer or native surface grant.
use super::*;
use crate::{
    life_availability::{Availability, Reason, Snapshot, Source},
    life_domain::{ActionId, Activity, LifeEvent, NeedEffect},
    life_runtime::core::{Core, Mode, SpeechConfig},
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn core_for(store: &Store, ids: &[&str]) -> Core {
    let mut core = Core::new(Snapshot { state: Availability::AwakeUnlocked, reason: Reason::Ready, generation: 1, source: Source::Unsupported });
    for id in ids {
        core.start(id, Mode::Life, uuid::Uuid::new_v4().to_string(), SpeechConfig { speak: false, min_ms: 15000, max_ms: 15000 }, Some(store.load_life(id).unwrap().unwrap().state().clone()), 7, false, true, 0).unwrap();
    }
    core
}
fn plan(core: &mut Core, store: &Store, bot: &str) -> ActionId {
    let (commit, ()) = store.apply_life_action_checked(bot, 1000, |actual, _| Ok((
        LifeEvent::Plan { activity: Activity::MoveTo, completion_effect: NeedEffect::new(0, 0, 0, 0).unwrap() }, actual.state().revision(), (),
    ))).unwrap();
    let session = &core.sessions[bot];
    let identity = (session.session_id.clone(), session.revision);
    assert!(core.replace_confirmed(bot, &identity.0, identity.1, commit.snapshot.state().clone(), None));
    commit.snapshot.state().current_action().unwrap().intent().id().clone()
}
fn context(hub: &WorldHub, store: &Store, core: &Core, bot: &str, now: u64) -> ActionContext {
    hub.action_context(store, &current_actors(core), bot, now).unwrap()
}

#[test]
fn slot_geometry_is_real_and_reservation_requires_the_actual_current_plan() {
    let (_fixture, store, hub, other, _) = prepare();
    let mut core = core_for(&store, &["nova", &other]);
    let ctx = context(&hub, &store, &core, "nova", 0);
    let target = hub.slot_target(&store, &ctx, &SlotId::new("meeting-1").unwrap()).unwrap();
    assert_eq!(target.kind, crate::world_state::AnchorKind::Meeting);
    assert_eq!(target.position, Point::new(500000, 500000).unwrap());
    assert!(hub.slot_target(&store, &ctx, &SlotId::new("missing").unwrap()).is_err());
    let before = store.load_life("nova").unwrap().unwrap();
    let fabricated = ActionId::new(ActorId::new("nova").unwrap(), before.state().world_id().clone(), 1).unwrap();
    assert!(hub.reserve_action_slot_current(&core, &store, &ctx, target.clone(), fabricated, 1000, || 0).is_err());
    assert!(hub.snapshot().unwrap().snapshot.unwrap().occupied.is_empty());
    let action = plan(&mut core, &store, "nova");
    let ctx = context(&hub, &store, &core, "nova", 0);
    let slot = hub.reserve_action_slot_current(&core, &store, &ctx, target, action, 1000, || 0).unwrap();
    assert!(hub.validate_action_slot_current(&core, &store, &ctx, &slot, || 1).is_ok());
    assert_eq!(hub.snapshot().unwrap().snapshot.unwrap().occupied.len(), 1);
    assert_eq!(store.load_life("nova").unwrap().unwrap().state().needs(), before.state().needs());
}

#[test]
fn two_real_plans_compete_for_one_exclusive_slot_without_moving_actors() {
    let (_fixture, store, hub, other, _) = prepare();
    let mut core = core_for(&store, &["nova", &other]);
    let first = plan(&mut core, &store, "nova");
    let second = plan(&mut core, &store, &other);
    let a = context(&hub, &store, &core, "nova", 0);
    let b = context(&hub, &store, &core, &other, 0);
    let target = hub.slot_target(&store, &a, &SlotId::new("meeting-1").unwrap()).unwrap();
    let positions = hub.snapshot().unwrap().snapshot.unwrap().actors.iter().map(|a| (a.actor_id.clone(), a.position)).collect::<Vec<_>>();
    let winner = hub.reserve_action_slot_current(&core, &store, &a, target.clone(), first, 1000, || 0).unwrap();
    assert!(hub.reserve_action_slot_current(&core, &store, &b, target, second, 1000, || 0).is_err());
    let snapshot = hub.snapshot().unwrap().snapshot.unwrap();
    assert_eq!(snapshot.occupied.len(), 1); assert_eq!(snapshot.occupied[0].actor_id.as_str(), "nova");
    assert_eq!(snapshot.actors.iter().map(|a| (a.actor_id.clone(), a.position)).collect::<Vec<_>>(), positions);
    assert!(hub.release(&a.session, &winner.lease(), 1).unwrap());
}

#[test]
fn exact_renewal_expiry_and_late_release_cannot_revoke_a_replacement() {
    let (_fixture, store, hub, other, _) = prepare();
    let mut core = core_for(&store, &["nova", &other]);
    let first = plan(&mut core, &store, "nova"); let second = plan(&mut core, &store, &other);
    let a = context(&hub, &store, &core, "nova", 0); let b = context(&hub, &store, &core, &other, 0);
    let target = hub.slot_target(&store, &a, &SlotId::new("meeting-1").unwrap()).unwrap();
    let mut old = hub.reserve_action_slot_current(&core, &store, &a, target.clone(), first, 1000, || 0).unwrap();
    hub.renew_action_slot_current(&core, &store, &a, &mut old, 1000, || 900).unwrap();
    assert!(hub.validate_action_slot_current(&core, &store, &a, &old, || 1899).is_ok());
    assert!(hub.validate_action_slot_current(&core, &store, &a, &old, || 1900).is_err());
    let successor = hub.reserve_action_slot_current(&core, &store, &b, target, second, 1000, || 1900).unwrap();
    assert!(hub.release(&a.session, &old.lease(), 1901).is_err());
    assert!(hub.validate_action_slot_current(&core, &store, &b, &successor, || 1901).is_ok());
    assert_eq!(hub.snapshot().unwrap().snapshot.unwrap().occupied[0].actor_id.as_str(), other);
}

#[test]
fn stop_or_output_loss_is_rederived_before_any_lease_renewal() {
    for stopped in [false, true] {
        let (_fixture, store, hub, other, _) = prepare();
        let mut core = core_for(&store, &["nova", &other]); let action = plan(&mut core, &store, "nova");
        let ctx = context(&hub, &store, &core, "nova", 0);
        let target = hub.slot_target(&store, &ctx, &SlotId::new("meeting-1").unwrap()).unwrap();
        let mut slot = hub.reserve_action_slot_current(&core, &store, &ctx, target, action, 1000, || 0).unwrap();
        if stopped { core.stop("nova", 1); } else {
            let current = &core.sessions["nova"]; let id = current.session_id.clone(); let revision = current.revision;
            core.set_output_available("nova", &id, revision, false, 1);
        }
        assert!(hub.renew_action_slot_current(&core, &store, &ctx, &mut slot, 1000, || 2).is_err());
        assert!(hub.snapshot().unwrap().snapshot.unwrap().occupied.is_empty());
    }
}

#[test]
fn changed_slot_geometry_revokes_old_goal_instead_of_retargeting() {
    let (_fixture, store, hub, other, _) = prepare();
    let mut core = core_for(&store, &["nova", &other]); let action = plan(&mut core, &store, "nova");
    let ctx = context(&hub, &store, &core, "nova", 0);
    let target = hub.slot_target(&store, &ctx, &SlotId::new("meeting-1").unwrap()).unwrap();
    let mut slot = hub.reserve_action_slot_current(&core, &store, &ctx, target, action, 1000, || 0).unwrap();
    let record = store.load_world_config().unwrap().unwrap();
    let mut wire = serde_json::to_value(&record.config).unwrap(); wire["anchors"][0]["slots"][0]["position"] = json!({"x":600000,"y":500000});
    let changed: WorldConfig = serde_json::from_value(wire).unwrap();
    hub.save(&store, Some(1), &changed, &record.output_binding, &current_actors(&core), topology(2, true), 1).unwrap();
    assert!(hub.renew_action_slot_current(&core, &store, &ctx, &mut slot, 1000, || 2).is_err());
    assert!(hub.snapshot().unwrap().snapshot.unwrap().occupied.is_empty());
    assert_eq!(slot.target().position, Point::new(500000, 500000).unwrap());
}

#[test]
fn expiry_during_actual_store_mutex_wait_cannot_renew_an_old_slot() {
    let (_fixture, store, hub, other, _) = prepare();
    let mut core = core_for(&store, &["nova", &other]); let action = plan(&mut core, &store, "nova");
    let ctx = context(&hub, &store, &core, "nova", 0);
    let target = hub.slot_target(&store, &ctx, &SlotId::new("meeting-1").unwrap()).unwrap();
    let mut slot = hub.reserve_action_slot_current(&core, &store, &ctx, target, action, 1000, || 0).unwrap();
    let clock = AtomicU64::new(0); let first = AtomicBool::new(true);
    let (held_tx, held_rx) = std::sync::mpsc::channel(); let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (read_tx, read_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let store_ref = &store;
        let writer = scope.spawn(move || store_ref.apply_life_action_checked::<()>("nova", 1001, |_, _| {
            held_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            Err(crate::storage::life_store::LifeStoreError::InvalidInput)
        }));
        held_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let clock_ref = &clock;
        let release = scope.spawn(move || { read_rx.recv_timeout(Duration::from_secs(3)).unwrap(); clock_ref.store(1001, Ordering::SeqCst); release_tx.send(()).unwrap(); });
        assert!(hub.renew_action_slot_current(&core, &store, &ctx, &mut slot, 1000, || {
            let sampled = clock.load(Ordering::SeqCst);
            if first.swap(false, Ordering::SeqCst) { read_tx.send(()).unwrap(); }
            sampled
        }).is_err());
        assert!(writer.join().unwrap().is_err()); release.join().unwrap();
    });
    assert!(!slot.fresh(1001)); assert!(hub.snapshot().unwrap().snapshot.unwrap().occupied.is_empty());
}

#[test]
fn native_display_geometry_uses_exact_current_context_and_observed_physical_display() {
    let (_fixture, store, hub, other, _) = prepare();
    let core = core_for(&store, &["nova", &other]);
    let ctx = context(&hub, &store, &core, "nova", 0);
    let geometry = hub.native_output_geometry(&store, &ctx).unwrap();
    assert_eq!(geometry.id(), "private-monitor");
    assert_eq!(geometry.physical_bounds(), (-2560, 0, 2560, 1600));
    assert_eq!(geometry.scale_factor(), 2.0);
    assert_eq!(geometry.logical_size().width, 1280.0);
    assert_eq!(geometry.logical_size().height, 800.0);
    let mut foreign = ctx.clone(); foreign.instance_id = "foreign-world".into();
    assert!(hub.native_output_geometry(&store, &foreign).is_err());
    let mut foreign = ctx.clone(); foreign.session = context(&hub, &store, &core, &other, 0).session;
    assert!(hub.native_output_geometry(&store, &foreign).is_err());
    assert_eq!(hub.native_output_geometry(&store, &ctx).unwrap().physical_bounds(), geometry.physical_bounds());
}

#[test]
fn changed_or_expired_topology_cannot_return_geometry_for_an_old_context() {
    let (_fixture, store, hub, other, _) = prepare();
    let core = core_for(&store, &["nova", &other]);
    let old = context(&hub, &store, &core, "nova", 0);
    let mut changed = topology(2, true);
    changed.displays[0].geometry = OutputGeometry::new("private-monitor", 100, -200, 1920, 1080, 1.5).unwrap();
    changed.fingerprint = vec![("private-monitor".into(), 100, -200, 1920, 1080, 1.5f64.to_bits())];
    hub.synchronize(&store, &current_actors(&core), Some(changed), 1).unwrap();
    assert!(hub.native_output_geometry(&store, &old).is_err());
    let current = context(&hub, &store, &core, "nova", 1);
    let geometry = hub.native_output_geometry(&store, &current).unwrap();
    assert_eq!(geometry.physical_bounds(), (100, -200, 1920, 1080));
    assert_eq!(geometry.scale_factor(), 1.5);
    hub.inner.lock().unwrap().topology.as_mut().unwrap().observed = Instant::now() - Duration::from_secs(6);
    assert!(hub.native_output_geometry(&store, &current).is_err());
}

#[test]
fn native_display_selection_matches_configured_monitor_and_existing_deterministic_fallback() {
    let (_fixture, store, hub, other, _) = prepare();
    let core = core_for(&store, &["nova", &other]);
    let sample = TopologySample {
        ticket: 2,
        observed: Instant::now(),
        displays: vec![
            DisplaySample { geometry: OutputGeometry::new("z-monitor", 0, 0, 2000, 1200, 2.0).unwrap() },
            DisplaySample { geometry: OutputGeometry::new("a-monitor", -1000, 50, 1000, 800, 1.25).unwrap() },
        ],
        fingerprint: vec![
            ("z-monitor".into(), 0, 0, 2000, 1200, 2.0f64.to_bits()),
            ("a-monitor".into(), -1000, 50, 1000, 800, 1.25f64.to_bits()),
        ],
    };
    for (preferred, expected, ticket) in [("z-monitor", "z-monitor", 2), ("missing-monitor", "a-monitor", 3)] {
        let record = store.load_world_config().unwrap().unwrap();
        let old = context(&hub, &store, &core, "nova", 0);
        let mut next = sample.clone(); next.ticket = ticket;
        hub.save(&store, Some(record.config.revision()), &record.config, &OutputBinding { monitor_id: Some(preferred.into()) }, &current_actors(&core), next, 0).unwrap();
        assert!(hub.native_output_geometry(&store, &old).is_err());
        let current = context(&hub, &store, &core, "nova", 0);
        let actual = hub.native_output_geometry(&store, &current).unwrap();
        assert_eq!(actual.id(), expected);
        let source = sample.displays.iter().find(|display| display.geometry.id() == expected).unwrap();
        assert_eq!(actual.physical_bounds(), source.geometry.physical_bounds());
        assert_eq!(actual.scale_factor(), source.geometry.scale_factor());
    }
}
