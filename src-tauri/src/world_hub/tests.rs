use super::*;
use crate::{
    life_domain::{Needs, RandomState},
    world_state::{coordinates::Rect, Anchor, AnchorId, Slot, StageId},
};
use std::{path::PathBuf, sync::Arc};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("orbitpal-world-hub-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config(ids: &[&str]) -> WorldConfig {
    WorldConfig::new(
        StageId::new("native-shared-stage").unwrap(),
        Rect::full(),
        ids.iter().map(|id| ActorId::new(*id).unwrap()).collect(),
        vec![Anchor::new(
            AnchorId::new("meeting").unwrap(),
            crate::world_state::AnchorKind::Meeting,
            Point::new(500000, 500000).unwrap(),
            vec![Slot::new(
                SlotId::new("meeting-1").unwrap(),
                Point::new(500000, 500000).unwrap(),
            )],
        )
        .unwrap()],
    )
    .unwrap()
}
fn topology(ticket: u64, available: bool) -> TopologySample {
    TopologySample {
        ticket,
        observed: Instant::now(),
        displays: if available {
            vec![DisplaySample {
                geometry: OutputGeometry::new("private-monitor", -2560, 0, 2560, 1600, 2.0)
                    .unwrap(),
            }]
        } else {
            vec![]
        },
        fingerprint: if available {
            vec![(
                "private-monitor".into(),
                -2560,
                0,
                2560,
                1600,
                2.0f64.to_bits(),
            )]
        } else {
            vec![]
        },
    }
}
fn native(store: &Store, id: &str, revision: u64) -> NativeActor {
    NativeActor {
        stamp: SessionStamp::new(ActorId::new(id).unwrap(), format!("session-{id}"), revision)
            .unwrap(),
        lineage: store
            .load_life(id)
            .unwrap()
            .map(|snapshot| snapshot.state().world_id().clone()),
    }
}
fn prepare() -> (Fixture, Store, WorldHub, String, Vec<NativeActor>) {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone()).unwrap();
    let other = store.create_bot("Another", false).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (id, world) in [("nova", "lineage-nova"), (other.as_str(), "lineage-other")] {
        store
            .initialize_life(
                id,
                WorldId::new(world).unwrap(),
                Needs::new(1234, 2345, 3456, 4567).unwrap(),
                RandomState::seeded(9),
                crate::storage::life_store::wall_clock_ms(),
            )
            .unwrap();
    }
    let sessions = vec![native(&store, "nova", 1), native(&store, &other, 1)];
    let hub = WorldHub::new(&store);
    hub.save(
        &store,
        None,
        &config(&["nova", &other]),
        &OutputBinding {
            monitor_id: Some("private-monitor".into()),
        },
        &sessions,
        topology(1, true),
        0,
    )
    .unwrap();
    (fixture, store, hub, other, sessions)
}
fn life(store: &Store, id: &str) -> Value {
    let snapshot = store.load_life(id).unwrap().unwrap();
    json!({"state":snapshot.state(),"random":snapshot.random().state(),"saved":snapshot.saved_at_ms(),"events":snapshot.events().len()})
}
#[test]
fn actual_two_actor_shared_snapshot_keeps_lineages_and_public_scope_without_activation() {
    let (_fixture, store, hub, other, sessions) = prepare();
    let before = [life(&store, "nova"), life(&store, &other)];
    let first = serde_json::to_value(hub.snapshot_for("nova").unwrap()).unwrap();
    let second = serde_json::to_value(hub.snapshot_for(&other).unwrap()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first["snapshot"]["actors"].as_array().unwrap().len(), 2);
    assert!(hub.snapshot_for("outsider").is_none());
    hub.place(
        1,
        &ActorId::new("nova").unwrap(),
        Point::new(250000, 750000).unwrap(),
    )
    .unwrap();
    hub.synchronize(&store, &sessions, Some(topology(2, true)), 1)
        .unwrap();
    assert_eq!([life(&store, "nova"), life(&store, &other)], before);
    let event = hub
        .output_event("nova", &store.bot_profile("nova").unwrap())
        .unwrap();
    let text = event.to_string();
    for private in [
        "private-monitor",
        "session-nova",
        "lineage-secret",
        "random_state",
        "apiKey",
        "PERSONA",
        "lease",
        "membership",
    ] {
        assert!(!text.contains(private), "{private}");
    }
    assert_eq!(event["arena"]["presentationAvailable"], false);
    let restarted = WorldHub::new(&store);
    restarted
        .synchronize(&store, &[], Some(topology(1, true)), 0)
        .unwrap();
    let fresh = serde_json::to_value(restarted.snapshot().unwrap()).unwrap();
    assert_ne!(fresh["instanceId"], first["instanceId"]);
    assert!(fresh["snapshot"]["occupied"].as_array().unwrap().is_empty());
    assert!(restarted.inner.lock().unwrap().bound.is_empty());
    assert_eq!([life(&store, "nova"), life(&store, &other)], before);
}
#[test]
fn actual_hub_mutex_race_exclusive_slot_and_stale_session_release_are_fenced() {
    let (_fixture, _store, hub, _other, sessions) = prepare();
    let hub = Arc::new(hub);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut joins = Vec::new();
    for native in sessions {
        let hub = hub.clone();
        let barrier = barrier.clone();
        joins.push(std::thread::spawn(move || {
            let stamp = native.stamp;
            barrier.wait();
            let lease = hub.reserve(
                &stamp,
                WorkIdentity::operation("one-native-operation").unwrap(),
                &SlotId::new("meeting-1").unwrap(),
                1000,
                1,
            );
            (stamp, lease)
        }));
    }
    let results = joins
        .into_iter()
        .map(|join| join.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let (winner, lease) = results
        .into_iter()
        .find(|(_, result)| result.is_ok())
        .unwrap();
    let lease = lease.unwrap();
    assert!(hub.release(&winner, &lease, 2).unwrap());
    assert!(hub.release(&winner, &lease, 3).is_err());
    let mut inner = hub.inner.lock().unwrap();
    inner.world.as_mut().unwrap().unbind_session(&winner);
    inner.bound.remove(winner.actor_id().as_str());
    drop(inner);
    assert!(hub
        .reserve(
            &winner,
            WorkIdentity::operation("late-callback").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            4
        )
        .is_err());
}
#[test]
fn failed_store_cas_leaves_confirmed_hub_and_lease_unchanged_until_explicit_reload() {
    let (_fixture, store, hub, other, sessions) = prepare();
    let lease = hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("active".to_owned()).unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            1,
        )
        .unwrap();
    let before = serde_json::to_value(hub.snapshot().unwrap()).unwrap();
    let original = store.load_world_config().unwrap().unwrap();
    store
        .save_world_config(Some(1), &original.config, &original.output_binding)
        .unwrap();
    assert!(hub
        .save(
            &store,
            Some(1),
            &original.config,
            &original.output_binding,
            &sessions,
            topology(2, true),
            2
        )
        .is_err());
    assert_eq!(
        serde_json::to_value(hub.snapshot().unwrap()).unwrap(),
        before
    );
    hub.renew(&sessions[0].stamp, &lease, 1000, 3).unwrap();
    hub.synchronize(&store, &sessions, Some(topology(3, true)), 4)
        .unwrap();
    let after = serde_json::to_value(hub.snapshot().unwrap()).unwrap();
    assert_ne!(after["instanceId"], before["instanceId"]);
    assert!(after["snapshot"]["occupied"].as_array().unwrap().is_empty());
    assert!(hub.release(&sessions[0].stamp, &lease, 5).is_err());
    assert!(hub.snapshot_for(&other).is_some());
}
#[test]
fn live_corruption_disables_only_world_and_does_not_materialize_private_payload() {
    let (fixture, store, hub, other, sessions) = prepare();
    let before = [life(&store, "nova"), life(&store, &other)];
    let lease = hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("active").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            1,
        )
        .unwrap();
    let db = rusqlite::Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
    db.execute(
        "UPDATE shared_world_config SET config_json=?1",
        ["private-corrupt-marker"],
    )
    .unwrap();
    hub.synchronize(&store, &sessions, Some(topology(2, true)), 2)
        .unwrap();
    let snapshot = serde_json::to_value(hub.snapshot().unwrap()).unwrap();
    assert_eq!(snapshot["availability"], "unavailable");
    assert!(snapshot["snapshot"].is_null());
    assert!(!snapshot.to_string().contains("private-corrupt-marker"));
    assert!(hub.release(&sessions[0].stamp, &lease, 3).is_err());
    assert_eq!([life(&store, "nova"), life(&store, &other)], before);
    assert!(store.profile().is_ok());
}
#[test]
fn monitor_loss_and_late_probe_cannot_revive_arrival_or_lease_authority() {
    let (_fixture, store, hub, _other, sessions) = prepare();
    let lease = hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("active").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            1,
        )
        .unwrap();
    hub.synchronize(&store, &sessions, Some(topology(3, false)), 2)
        .unwrap();
    hub.synchronize(&store, &sessions, Some(topology(2, true)), 3)
        .unwrap();
    assert!(hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("new").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            4
        )
        .is_err());
    hub.synchronize(&store, &sessions, Some(topology(4, true)), 5)
        .unwrap();
    assert!(hub.renew(&sessions[0].stamp, &lease, 1000, 6).is_err());
    let replacement = hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("new").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            7,
        )
        .unwrap();
    assert!(hub.release(&sessions[0].stamp, &lease, 8).is_err());
    assert!(hub.release(&sessions[0].stamp, &replacement, 9).unwrap());
}
#[test]
fn actual_core_bridge_rejects_retained_stamps_after_stop_and_output_loss() {
    use crate::{
        life_availability::{Availability, Reason, Snapshot, Source},
        life_runtime::core::{Core, Mode, SpeechConfig},
    };
    let (_fixture, store, hub, _other, _sessions) = prepare();
    let mut core = Core::new(Snapshot {
        state: Availability::AwakeUnlocked,
        reason: Reason::Ready,
        generation: 1,
        source: Source::Unsupported,
    });
    core.start(
        "nova",
        Mode::Life,
        uuid::Uuid::new_v4().to_string(),
        SpeechConfig {
            speak: false,
            min_ms: 15000,
            max_ms: 15000,
        },
        Some(store.load_life("nova").unwrap().unwrap().state().clone()),
        7,
        false,
        true,
        0,
    )
    .unwrap();
    let slot = SlotId::new("meeting-1").unwrap();
    let lease = hub
        .reserve_current(
            &core,
            &store,
            "nova",
            WorkIdentity::operation("current").unwrap(),
            &slot,
            1000,
            1,
        )
        .unwrap();
    let retained = current_actors(&core)[0].stamp.clone();
    let session = &core.sessions["nova"];
    let (id, revision) = (session.session_id.clone(), session.revision);
    core.set_output_available("nova", &id, revision, false, 2);
    assert!(hub
        .reserve_current(
            &core,
            &store,
            "nova",
            WorkIdentity::operation("after-output-loss").unwrap(),
            &slot,
            1000,
            3
        )
        .is_err());
    assert!(hub.renew(&retained, &lease, 1000, 3).is_err());
    assert!(hub
        .snapshot()
        .unwrap()
        .snapshot
        .unwrap()
        .occupied
        .is_empty());
    core.set_output_available("nova", &id, core.sessions["nova"].revision, true, 4);
    let replacement = hub
        .reserve_current(
            &core,
            &store,
            "nova",
            WorkIdentity::operation("recovered").unwrap(),
            &slot,
            1000,
            5,
        )
        .unwrap();
    let recovered = current_actors(&core)[0].stamp.clone();
    assert_ne!(recovered, retained);
    core.stop("nova", 6);
    assert!(hub
        .reserve_current(
            &core,
            &store,
            "nova",
            WorkIdentity::operation("after-stop").unwrap(),
            &slot,
            1000,
            7
        )
        .is_err());
    assert!(hub.release(&recovered, &replacement, 7).is_err());
    assert!(hub
        .snapshot()
        .unwrap()
        .snapshot
        .unwrap()
        .occupied
        .is_empty());
}
#[test]
fn operator_placement_rechecks_current_persistent_revision_before_moving() {
    let (_fixture, store, hub, _other, sessions) = prepare();
    let record = store.load_world_config().unwrap().unwrap();
    store
        .save_world_config(Some(1), &record.config, &record.output_binding)
        .unwrap();
    assert!(hub
        .place_current(
            &store,
            &sessions,
            1,
            1,
            &ActorId::new("nova").unwrap(),
            Point::new(1, 1).unwrap()
        )
        .is_err());
    let snapshot = hub.snapshot().unwrap().snapshot.unwrap();
    assert_eq!(snapshot.config_revision, 2);
    assert_ne!(
        snapshot
            .actors
            .iter()
            .find(|actor| actor.actor_id.as_str() == "nova")
            .unwrap()
            .position,
        Point::new(1, 1).unwrap()
    );
}
#[test]
fn explicit_unknown_actor_repair_has_fresh_observation_epoch_and_no_revived_authority() {
    let (fixture, store, hub, other, sessions) = prepare();
    let record = store.load_world_config().unwrap().unwrap();
    let lease = hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("before-repair").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            1,
        )
        .unwrap();
    for n in 1..5 {
        hub.place(1, &ActorId::new("nova").unwrap(), Point::new(n, n).unwrap())
            .unwrap();
    }
    let old = serde_json::to_value(hub.snapshot().unwrap()).unwrap();
    let db = rusqlite::Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
    let mut broken = serde_json::to_value(&record.config).unwrap();
    broken["members"] = json!(["nova", "missing"]);
    db.execute(
        "UPDATE shared_world_config SET config_json=?1",
        [broken.to_string()],
    )
    .unwrap();
    hub.synchronize(&store, &sessions, None, 2).unwrap();
    assert_eq!(
        hub.settings(&store).reason,
        Some(WorldStoreError::UnknownActor)
    );
    hub.save(
        &store,
        Some(1),
        &record.config,
        &record.output_binding,
        &[],
        topology(2, true),
        3,
    )
    .unwrap();
    let fresh = serde_json::to_value(hub.snapshot().unwrap()).unwrap();
    assert_ne!(old["instanceId"], fresh["instanceId"]);
    assert!(fresh["snapshot"]["occupied"].as_array().unwrap().is_empty());
    assert!(hub.inner.lock().unwrap().bound.is_empty());
    assert!(hub.release(&sessions[0].stamp, &lease, 4).is_err());
    assert!(store.load_life(&other).unwrap().is_some());
}
#[test]
fn world_native_routing_targets_only_selected_participants_and_removal_clears() {
    let (_fixture, store, hub, other, sessions) = prepare();
    let initial = hub.output_targets();
    assert!(initial.contains(&"nova".to_owned()) && initial.contains(&other));
    let event = hub
        .output_event("nova", &json!({"output":{"mode":"studio"}}))
        .unwrap();
    assert_eq!(output_window(&event), Some("avatar-nova".into()));
    assert!(hub.output_event("outsider", &json!({})).is_none());
    assert!(!initial.contains(&"outsider".to_owned()));
    let draft = config(&["nova"]).with_revision(1).unwrap();
    hub.save(
        &store,
        Some(1),
        &draft,
        &OutputBinding::default(),
        &sessions,
        topology(2, true),
        1,
    )
    .unwrap();
    assert!(hub.output_targets().contains(&other));
    assert!(hub.output_event(&other, &json!({})).is_none());
    let clear = json!({"kind":"world-state","botId":other,"world":null,"arena":null});
    assert_eq!(output_window(&clear), Some(format!("avatar-{other}")));
    assert!(clear["world"].is_null());
    assert!(!hub.output_targets().contains(&other));
    assert_eq!(
        output_window(&json!({"kind":"world-state","botId":"../outsider"})),
        None
    );
}
#[test]
fn mirrored_same_id_conflicts_are_excluded_and_order_cannot_change_selection() {
    let (_fixture, store, hub, _other, sessions) = prepare();
    let old = hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("before-mirror").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            1,
        )
        .unwrap();
    let mut ambiguous = topology(2, true);
    ambiguous.displays.push(DisplaySample {
        geometry: OutputGeometry::new("private-monitor", -2560, 0, 1920, 1080, 1.0).unwrap(),
    });
    ambiguous.fingerprint.push((
        "private-monitor".into(),
        -2560,
        0,
        1920,
        1080,
        1f64.to_bits(),
    ));
    hub.synchronize(&store, &sessions, Some(ambiguous.clone()), 2)
        .unwrap();
    assert!(hub.renew(&sessions[0].stamp, &old, 1000, 3).is_err());
    assert!(hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("ambiguous").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            3
        )
        .is_err());
    let revision = hub.inner.lock().unwrap().topology_revision;
    ambiguous.ticket = 3;
    ambiguous.displays.reverse();
    ambiguous.fingerprint.reverse();
    hub.synchronize(&store, &sessions, Some(ambiguous), 4)
        .unwrap();
    assert_eq!(hub.inner.lock().unwrap().topology_revision, revision);
    let mut identical = topology(4, true);
    identical.displays.push(identical.displays[0].clone());
    identical.fingerprint.push(identical.fingerprint[0].clone());
    hub.synchronize(&store, &sessions, Some(identical), 5)
        .unwrap();
    assert_eq!(
        hub.inner
            .lock()
            .unwrap()
            .topology
            .as_ref()
            .unwrap()
            .displays
            .len(),
        1
    );
    assert!(hub
        .reserve(
            &sessions[0].stamp,
            WorkIdentity::operation("unambiguous").unwrap(),
            &SlotId::new("meeting-1").unwrap(),
            1000,
            6
        )
        .is_ok());
}
