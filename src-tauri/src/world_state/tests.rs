use super::*;
use crate::life_domain::{
    reduce, Activity, CancelReason, LifeEvent, NeedEffect, Needs, RandomState,
};
use coordinates::{
    choose_output, map_logical, map_physical, LogicalSize, OutputGeometry, PhysicalPoint,
};
use serde_json::{json, Value};
use std::sync::{Arc, Barrier, Mutex};

fn actor(id: &str) -> ActorId {
    ActorId::new(id).unwrap()
}
fn point(x: u32, y: u32) -> Point {
    Point::new(x, y).unwrap()
}
fn slot(id: &str) -> SlotId {
    SlotId::new(id).unwrap()
}
fn config(members: &[&str], play_x: u32) -> WorldConfig {
    let anchor = |id: &str, kind, location| {
        Anchor::new(
            AnchorId::new(id).unwrap(),
            kind,
            location,
            vec![Slot::new(slot(id), location)],
        )
        .unwrap()
    };
    WorldConfig::new(
        StageId::new("shared-stage").unwrap(),
        Rect::full(),
        members.iter().map(|id| actor(id)).collect(),
        vec![
            anchor("rest", AnchorKind::Rest, point(200_000, 500_000)),
            anchor("meeting", AnchorKind::Meeting, point(500_000, 500_000)),
            anchor("play", AnchorKind::Play, point(play_x, 500_000)),
        ],
    )
    .unwrap()
}
fn stage() -> WorldState {
    WorldState::new(
        config(&["nova", "james"], 800_000),
        RuntimeEpoch::new("boot-one").unwrap(),
    )
    .unwrap()
}
fn session(id: &str, nonce: &str) -> SessionStamp {
    SessionStamp::new(actor(id), nonce, 1).unwrap()
}
fn bind(world: &mut WorldState, id: &str, nonce: &str) -> MemberHandle {
    world.bind_actor(session(id, nonce), None, 0).unwrap()
}
fn operation(id: &str) -> WorkIdentity {
    WorkIdentity::operation(id).unwrap()
}
fn reserve(world: &mut WorldState, member: &MemberHandle, target: &str, now: u64) -> SlotLease {
    world
        .reserve(
            world.config().revision(),
            member,
            operation("native-task"),
            &slot(target),
            1_000,
            now,
        )
        .unwrap()
}
fn life(id: &str, lineage: &str) -> LifeState {
    LifeState::new(
        actor(id),
        WorldId::new(lineage).unwrap(),
        Needs::new(1200, 3400, 5600, 7800).unwrap(),
    )
}
fn planned(state: &LifeState) -> LifeState {
    reduce(
        state,
        0,
        RandomState::seeded(7),
        LifeEvent::Plan {
            activity: Activity::Rest,
            completion_effect: NeedEffect::new(100, 200, 0, 0).unwrap(),
        },
    )
    .unwrap()
    .state
}
fn action(state: &LifeState) -> ActionId {
    state.current_action().unwrap().intent().id().clone()
}
fn snapshot(world: &WorldState) -> Value {
    serde_json::to_value(world.snapshot()).unwrap()
}

#[test]
fn two_actors_share_a_stage_without_rewriting_any_life_lineage_or_history() {
    let nova = planned(&life("nova", "nova-world"));
    let james = planned(&life("james", "james-world"));
    let before = [
        serde_json::to_vec(&nova).unwrap(),
        serde_json::to_vec(&james).unwrap(),
    ];
    let mut world = stage();
    let n = world
        .bind_actor(session("nova", "n"), Some(&nova), 0)
        .unwrap();
    let j = world
        .bind_actor(session("james", "j"), Some(&james), 0)
        .unwrap();
    world.move_actor(&n, point(250_000, 750_000)).unwrap();
    let shared = snapshot(&world);
    assert_eq!(shared["stageId"], "shared-stage");
    assert_eq!(shared["actors"].as_array().unwrap().len(), 2);
    let worlds = shared["actors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["life"]["worldId"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(worlds, BTreeSet::from(["nova-world", "james-world"]));
    world
        .reserve(
            0,
            &n,
            WorkIdentity::Life(action(&nova)),
            &slot("rest"),
            1000,
            0,
        )
        .unwrap();
    world
        .reserve(
            0,
            &j,
            WorkIdentity::Life(action(&james)),
            &slot("play"),
            1000,
            0,
        )
        .unwrap();
    assert_eq!(
        before,
        [
            serde_json::to_vec(&nova).unwrap(),
            serde_json::to_vec(&james).unwrap()
        ]
    );
}

#[test]
fn wrong_actor_lineage_and_unallocated_action_never_acquire_a_slot() {
    let state = planned(&life("nova", "own-world"));
    let mut world = stage();
    let member = world
        .bind_actor(session("nova", "session"), Some(&state), 0)
        .unwrap();
    for id in [
        ActionId::new(actor("james"), WorldId::new("own-world").unwrap(), 1).unwrap(),
        ActionId::new(actor("nova"), WorldId::new("other-world").unwrap(), 1).unwrap(),
        ActionId::new(actor("nova"), WorldId::new("own-world").unwrap(), 2).unwrap(),
    ] {
        assert_eq!(
            world.reserve(0, &member, WorkIdentity::Life(id), &slot("rest"), 1000, 0),
            Err(WorldError::WrongLineage)
        );
    }
    assert!(world.snapshot().occupied.is_empty());
    assert_eq!(
        world.bind_actor(session("james", "s"), Some(&state), 0),
        Err(WorldError::WrongLineage)
    );
}

#[test]
fn uninitialized_presence_can_use_a_native_operation_but_not_invent_a_life_action() {
    let mut world = stage();
    let member = bind(&mut world, "nova", "n");
    assert!(world.snapshot().actors.iter().all(|a| a.life.is_none()));
    let fake = ActionId::new(actor("nova"), WorldId::new("not-initialized").unwrap(), 1).unwrap();
    assert_eq!(
        world.reserve(0, &member, WorkIdentity::Life(fake), &slot("rest"), 1000, 0),
        Err(WorldError::WrongLineage)
    );
    reserve(&mut world, &member, "rest", 0);
}

#[test]
fn simultaneous_owners_cannot_both_reserve_one_exclusive_slot() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    let j = bind(&mut world, "james", "j");
    let state = Arc::new(Mutex::new(world));
    let barrier = Arc::new(Barrier::new(3));
    let threads = [n, j]
        .into_iter()
        .map(|member| {
            let state = state.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                state.lock().unwrap().reserve(
                    0,
                    &member,
                    operation("native-task"),
                    &slot("rest"),
                    1000,
                    0,
                )
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(WorldError::Occupied))
            .count(),
        1
    );
    assert_eq!(state.lock().unwrap().snapshot().occupied.len(), 1);
}

#[test]
fn timeout_backward_time_and_duplicate_acquisition_do_not_revive_a_lease() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    let old = reserve(&mut world, &n, "rest", 10_000);
    assert_eq!(
        reserve(&mut world, &n, "rest", 10_500),
        old,
        "idempotent acquisition does not extend TTL"
    );
    assert_eq!(
        world.validate_reservation(&old, &n, 11_000),
        Err(WorldError::Expired)
    );
    let new = reserve(&mut world, &n, "rest", 11_000);
    assert_ne!(old, new);
    assert_eq!(world.release(&old, &n, 10_000), Err(WorldError::Expired));
    assert!(world.validate_reservation(&new, &n, 11_999).is_ok());
    assert_eq!(
        world.renew(&old, &n, 1000, 12_000),
        Err(WorldError::Expired)
    );
    assert!(world.snapshot().occupied.is_empty());
}

#[test]
fn stop_and_same_text_session_reuse_fence_old_callbacks() {
    let mut world = stage();
    let original = session("nova", "same-name");
    let old_member = world.bind_actor(original.clone(), None, 0).unwrap();
    let old = reserve(&mut world, &old_member, "rest", 0);
    assert!(world.unbind_session(&original));
    assert!(world.snapshot().occupied.is_empty());
    assert_eq!(
        world.bind_actor(original.clone(), None, 1),
        Err(WorldError::InvalidOwner)
    );
    let replacement = world
        .bind_actor(
            SessionStamp::new(actor("nova"), "same-name", 2).unwrap(),
            None,
            1,
        )
        .unwrap();
    assert_ne!(old_member, replacement);
    let new = reserve(&mut world, &replacement, "rest", 1);
    assert_eq!(
        world.release(&old, &old_member, 2),
        Err(WorldError::InvalidOwner)
    );
    assert!(world.validate_reservation(&new, &replacement, 2).is_ok());
    assert!(!world.unbind_session(&original));
    assert_eq!(
        world.bind_actor(original, None, 2),
        Err(WorldError::InvalidOwner)
    );
    assert!(!world.unbind_session(&session("nova", "old-session")));
}

#[test]
fn position_updates_preserve_slot_authority_but_config_cas_and_changed_slots_are_checked() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    let j = bind(&mut world, "james", "j");
    let rest = reserve(&mut world, &n, "rest", 0);
    let play = reserve(&mut world, &j, "play", 0);
    world.move_actor(&n, point(100_000, 200_000)).unwrap();
    assert!(world.validate_reservation(&rest, &n, 1).is_ok());
    world
        .replace_config(0, config(&["nova", "james"], 750_000))
        .unwrap();
    assert_eq!(world.config().revision(), 1);
    assert!(world.validate_reservation(&rest, &n, 2).is_ok());
    assert_eq!(
        world.validate_reservation(&play, &j, 2),
        Err(WorldError::Expired)
    );
    assert_eq!(
        world.replace_config(0, config(&["nova"], 750_000)),
        Err(WorldError::StaleRevision)
    );
    assert_eq!(
        world.reserve(0, &n, operation("other"), &slot("meeting"), 1000, 2),
        Err(WorldError::StaleRevision)
    );
    assert_eq!(world.config().members().len(), 2);
}

#[test]
fn removing_and_readding_an_actor_does_not_reuse_its_membership_generation() {
    let mut world = stage();
    let old = bind(&mut world, "nova", "n");
    world
        .replace_config(0, config(&["james"], 800_000))
        .unwrap();
    world
        .replace_config(1, config(&["nova", "james"], 800_000))
        .unwrap();
    let new = bind(&mut world, "nova", "n");
    assert_ne!(old, new);
    assert_eq!(
        world.move_actor(&old, point(500_000, 500_000)),
        Err(WorldError::InvalidOwner)
    );
    assert!(world.move_actor(&new, point(500_000, 500_000)).is_ok());
}

#[test]
fn config_reload_has_no_occupancy_authority_or_replayed_positions() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    let lease = reserve(&mut world, &n, "rest", 0);
    world.move_actor(&n, point(100_000, 900_000)).unwrap();
    let saved = serde_json::to_vec(world.config()).unwrap();
    let mut restored = WorldState::new(
        serde_json::from_slice(&saved).unwrap(),
        RuntimeEpoch::new("boot-two").unwrap(),
    )
    .unwrap();
    assert_eq!(restored.config(), world.config());
    assert!(restored.snapshot().occupied.is_empty());
    assert!(restored
        .snapshot()
        .actors
        .iter()
        .all(|actor| actor.position == Rect::full().center() && actor.life.is_none()));
    assert_eq!(
        restored.validate_reservation(&lease, &n, 0),
        Err(WorldError::InvalidOwner)
    );
    let fresh = bind(&mut restored, "nova", "n");
    assert_ne!(fresh, n);
}

#[test]
fn geometry_replacement_releases_old_places_without_changing_life_membership() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    let old = reserve(&mut world, &n, "rest", 0);
    world.invalidate_geometry().unwrap();
    assert!(world.snapshot().occupied.is_empty());
    let new = reserve(&mut world, &n, "rest", 1);
    assert_ne!(old.resource_generation, new.resource_generation);
    assert_eq!(world.release(&old, &n, 1), Err(WorldError::Expired));
    assert!(world.validate_reservation(&new, &n, 1).is_ok());
}

#[test]
fn terminal_life_refresh_releases_action_only_and_refuses_revision_rollback() {
    let initial = life("nova", "source");
    let planned = planned(&initial);
    let mut world = stage();
    let n = world
        .bind_actor(session("nova", "n"), Some(&planned), 0)
        .unwrap();
    let j = bind(&mut world, "james", "j");
    world
        .reserve(
            0,
            &n,
            WorkIdentity::Life(action(&planned)),
            &slot("rest"),
            1000,
            0,
        )
        .unwrap();
    let other = reserve(&mut world, &j, "play", 0);
    let cancelled = reduce(
        &planned,
        0,
        RandomState::seeded(10),
        LifeEvent::Cancel {
            action_id: action(&planned),
            reason: CancelReason::Operator,
        },
    )
    .unwrap()
    .state;
    world.refresh_actor(&n, &cancelled).unwrap();
    assert_eq!(world.snapshot().occupied.len(), 1);
    assert!(world.validate_reservation(&other, &j, 1).is_ok());
    assert_eq!(
        world.refresh_actor(&n, &initial),
        Err(WorldError::StaleRevision)
    );
    assert_eq!(
        world.bind_actor(session("nova", "new-session"), Some(&initial), 1),
        Err(WorldError::StaleRevision)
    );
    assert_eq!(
        world.bind_actor(session("nova", "new-session"), None, 1),
        Err(WorldError::WrongLineage)
    );
    assert_eq!(
        world.reserve(
            0,
            &n,
            WorkIdentity::Life(action(&planned)),
            &slot("rest"),
            1000,
            1
        ),
        Err(WorldError::WrongLineage)
    );
    assert_eq!(
        cancelled.needs(),
        planned.needs(),
        "reservations never grant a completion effect"
    );
}

#[test]
fn source_lineage_replacement_requires_fresh_binding_and_cannot_accept_old_action_ack() {
    let old_life = planned(&life("nova", "old-world"));
    let new_life = planned(&life("nova", "new-world"));
    let mut world = stage();
    let stamp = session("nova", "n");
    let old_member = world.bind_actor(stamp.clone(), Some(&old_life), 0).unwrap();
    let lease = world
        .reserve(
            0,
            &old_member,
            WorkIdentity::Life(action(&old_life)),
            &slot("rest"),
            1000,
            0,
        )
        .unwrap();
    assert_eq!(
        world.refresh_actor(&old_member, &new_life),
        Err(WorldError::WrongLineage)
    );
    assert_eq!(
        world.bind_actor(stamp, Some(&new_life), 1),
        Err(WorldError::WrongLineage)
    );
    let new_member = world
        .bind_actor(session("nova", "fresh-native-session"), Some(&new_life), 1)
        .unwrap();
    assert_ne!(old_member, new_member);
    assert!(world.snapshot().occupied.is_empty());
    assert_eq!(
        world.validate_reservation(&lease, &old_member, 1),
        Err(WorldError::InvalidOwner)
    );
}

#[test]
fn initializing_life_needs_a_new_native_session_binding() {
    let mut world = stage();
    let stamp = session("nova", "ambient-session");
    let ambient = world.bind_actor(stamp.clone(), None, 0).unwrap();
    let lease = reserve(&mut world, &ambient, "rest", 0);
    let initialized = planned(&life("nova", "initialized-world"));
    assert_eq!(
        world.bind_actor(stamp.clone(), Some(&initialized), 1),
        Err(WorldError::WrongLineage)
    );
    assert!(world.validate_reservation(&lease, &ambient, 1).is_ok());
    let fresh = world
        .bind_actor(session("nova", "life-session"), Some(&initialized), 2)
        .unwrap();
    assert!(!world.unbind_session(&stamp));
    assert_ne!(ambient, fresh);
    assert!(world.snapshot().occupied.is_empty());
}

#[test]
fn operator_placement_clears_only_affected_arrivals_without_minting_or_replacing_sessions() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    let j = bind(&mut world, "james", "j");
    let old = reserve(&mut world, &n, "rest", 0);
    let other = reserve(&mut world, &j, "play", 0);
    world
        .place_actor(0, &actor("nova"), point(300_000, 500_000))
        .unwrap();
    assert_eq!(world.member_handle(&actor("nova")), Some(n.clone()));
    assert_eq!(
        world.validate_reservation(&old, &n, 1),
        Err(WorldError::Expired)
    );
    assert!(world.validate_reservation(&other, &j, 1).is_ok());
    assert_eq!(
        world.place_actor(1, &actor("nova"), point(0, 0)),
        Err(WorldError::StaleRevision)
    );
    assert_eq!(
        world.place_actor(0, &actor("hidden"), point(0, 0)),
        Err(WorldError::InvalidOwner)
    );
    world.stop_all_sessions();
    assert!(world.snapshot().occupied.is_empty());
    assert_eq!(world.member_handle(&actor("nova")), None);
    world
        .place_actor(0, &actor("nova"), point(400_000, 500_000))
        .unwrap();
    assert_eq!(
        world.member_handle(&actor("nova")),
        None,
        "manual placement cannot activate"
    );
    assert_eq!(
        serde_json::to_value(world.snapshot_for_actor(&actor("nova")).unwrap()).unwrap(),
        serde_json::to_value(world.snapshot_for_actor(&actor("james")).unwrap()).unwrap()
    );
    assert!(world.snapshot_for_actor(&actor("hidden")).is_err());
}

#[test]
fn confirmed_config_revision_is_bounded_and_cannot_reset_runtime_cas() {
    let configured = config(&["nova"], 800_000).with_revision(8).unwrap();
    assert_eq!(configured.revision(), 8);
    assert_eq!(
        configured.with_revision(MAX_COUNTER + 1),
        Err(WorldError::InvalidConfig)
    );
    let mut world = WorldState::new(configured, RuntimeEpoch::new("boot").unwrap()).unwrap();
    world.replace_config(8, config(&["nova"], 800_000)).unwrap();
    assert_eq!(world.config().revision(), 9);
    assert_eq!(
        world.replace_config(0, config(&["nova"], 800_000)),
        Err(WorldError::StaleRevision)
    );
}

#[test]
fn inactive_and_stopped_actor_observations_update_without_reviving_authority() {
    let planned = planned(&life("nova", "stored-source"));
    let mut world = stage();
    world.observe_actor(&actor("nova"), &planned).unwrap();
    assert!(world.member_handle(&actor("nova")).is_none());
    assert!(world
        .snapshot()
        .actors
        .iter()
        .find(|a| a.actor_id == actor("nova"))
        .unwrap()
        .life
        .is_some());
    let stamp = session("nova", "native");
    let handle = world.bind_actor(stamp.clone(), Some(&planned), 0).unwrap();
    let lease = world
        .reserve(
            0,
            &handle,
            WorkIdentity::Life(action(&planned)),
            &slot("rest"),
            1000,
            0,
        )
        .unwrap();
    let cancelled = reduce(
        &planned,
        0,
        RandomState::seeded(7),
        LifeEvent::Cancel {
            action_id: action(&planned),
            reason: CancelReason::Operator,
        },
    )
    .unwrap()
    .state;
    world.unbind_session(&stamp);
    world.observe_actor(&actor("nova"), &cancelled).unwrap();
    assert!(world.member_handle(&actor("nova")).is_none());
    assert!(world.snapshot().occupied.is_empty());
    assert_eq!(
        world.validate_reservation(&lease, &handle, 1),
        Err(WorldError::InvalidOwner)
    );
    assert_eq!(
        world.observe_actor(&actor("nova"), &planned),
        Err(WorldError::StaleRevision)
    );
    let other = life("nova", "wrong-source");
    assert_eq!(
        world.observe_actor(&actor("nova"), &other),
        Err(WorldError::WrongLineage)
    );
    assert_eq!(
        world.observe_actor(&actor("nova"), &other),
        Err(WorldError::WrongLineage),
        "rejected lineage cannot erase the source-world fence"
    );
    assert_eq!(
        world.observe_actor(&actor("hidden"), &life("hidden", "source")),
        Err(WorldError::InvalidOwner)
    );
    assert_eq!(
        world.observe_actor(&actor("james"), &cancelled),
        Err(WorldError::WrongLineage)
    );
}

#[test]
fn validated_persistent_schema_rejects_unknown_fields_duplicates_ranges_and_capacities() {
    let original = serde_json::to_value(config(&["nova", "james"], 800_000)).unwrap();
    assert_eq!(
        serde_json::from_value::<WorldConfig>(original.clone()).unwrap(),
        config(&["nova", "james"], 800_000)
    );
    for change in [
        "schema",
        "revision",
        "coordinate",
        "rect",
        "duplicate-member",
        "duplicate-slot",
        "authority",
        "members",
        "anchors",
    ] {
        let mut v = original.clone();
        match change {
            "schema" => v["schemaVersion"] = json!(2), "revision" => v["revision"] = json!(MAX_COUNTER + 1),
            "coordinate" => v["anchors"][0]["position"]["x"] = json!(1_000_001),
            "rect" => v["safeRegion"]["min"] = v["safeRegion"]["max"].clone(),
            "duplicate-member" => v["members"] = json!(["nova", "nova"]),
            "duplicate-slot" => v["anchors"][1]["slots"][0]["id"] = json!("rest"),
            "authority" => v["runtimeEpoch"] = json!("forged"),
            "members" => v["members"] = json!((0..65).map(|i| format!("actor{i}")).collect::<Vec<_>>()),
            _ => v["anchors"] = json!((0..65).map(|i| json!({"id":format!("a{i}"),"kind":"rest","position":{"x":0,"y":0},"slots":[{"id":format!("s{i}"),"position":{"x":0,"y":0}}]})).collect::<Vec<_>>()),
        }
        assert!(
            serde_json::from_value::<WorldConfig>(v).is_err(),
            "{change}"
        );
    }
    for id in ["", "../escape", "a b", "é", "bad\n"] {
        assert!(StageId::new(id).is_err());
    }
    assert!(StageId::new("x".repeat(97)).is_err());
}

#[test]
fn exhausted_identity_and_clock_fail_closed_without_reusing_a_lease() {
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    reserve(&mut world, &n, "rest", 0);
    world.next_identity = MAX_COUNTER;
    assert_eq!(
        world.reserve(0, &n, operation("new"), &slot("play"), 1000, 0),
        Err(WorldError::CounterExhausted)
    );
    assert!(world.snapshot().occupied.is_empty());
    assert!(world.move_actor(&n, point(0, 0)).is_err());
    let mut world = stage();
    let n = bind(&mut world, "nova", "n");
    reserve(&mut world, &n, "rest", 0);
    assert_eq!(
        world.advance(MAX_COUNTER + 1),
        Err(WorldError::InvalidClock)
    );
    assert!(world.snapshot().occupied.is_empty());
    assert!(world
        .reserve(0, &n, operation("new"), &slot("play"), 1000, 0)
        .is_err());
}

#[test]
fn public_projection_contains_only_explicit_participants_and_no_session_or_reservation_capabilities(
) {
    let mut world = stage();
    let state = planned(&life("nova", "source-world"));
    let member = world
        .bind_actor(session("nova", "private-session"), Some(&state), 0)
        .unwrap();
    world
        .reserve(
            0,
            &member,
            WorkIdentity::Life(action(&state)),
            &slot("rest"),
            1000,
            0,
        )
        .unwrap();
    assert_eq!(
        world.bind_actor(session("hidden-bot", "s"), None, 0),
        Err(WorldError::InvalidOwner)
    );
    let public = serde_json::to_string(&world.snapshot()).unwrap();
    for private in [
        "private-session",
        "boot-one",
        "nextIdentity",
        "sourceWorld",
        "needs",
        "recentEvents",
        "completionEffect",
        "serial",
        "expiresAt",
        "member",
        "random",
    ] {
        assert!(!public.contains(private), "{private}");
    }
    let actor_keys = snapshot(&world)["actors"][1]["life"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actor_keys,
        BTreeSet::from(
            [
                "actorId",
                "worldId",
                "schemaVersion",
                "revision",
                "simulationTime",
                "activity"
            ]
            .map(str::to_owned)
        )
    );
    let persisted = serde_json::to_string(world.config()).unwrap();
    assert!(!persisted.contains("occupied"));
    assert!(!persisted.contains("private-session"));
}

#[test]
fn negative_monitor_origins_mixed_dpi_and_bounded_stage_mapping_are_explicit() {
    let output = OutputGeometry::new("left", -2560, 0, 2560, 1600, 2.0).unwrap();
    assert_eq!(
        output.logical_size(),
        LogicalSize {
            width: 1280.0,
            height: 800.0
        }
    );
    let figure = LogicalSize {
        width: 100.0,
        height: 200.0,
    };
    assert_eq!(
        map_physical(point(500_000, 500_000), Rect::full(), &output, figure, 16.0).unwrap(),
        PhysicalPoint { x: -1280, y: 800 }
    );
    for scale in [0.5, 1.0, 1.25, 2.0, 3.0] {
        let out = OutputGeometry::new("screen", -2500, -1600, 2500, 1600, scale).unwrap();
        for p in [
            point(0, 0),
            point(1_000_000, 1_000_000),
            point(250_000, 750_000),
        ] {
            let mapped = map_physical(p, Rect::full(), &out, figure, 16.0).unwrap();
            assert!(mapped.x >= -2500 && mapped.x <= 0 && mapped.y >= -1600 && mapped.y <= 0);
        }
    }
    let obs = map_logical(
        point(500_000, 500_000),
        Rect::full(),
        LogicalSize {
            width: 640.0,
            height: 480.0,
        },
        figure,
        16.0,
    )
    .unwrap();
    assert_eq!(obs.x, 320.0);
    assert_eq!(obs.y, 240.0);
}

#[test]
fn removed_monitor_falls_back_deterministically_and_invalid_geometry_never_reports_success() {
    let outputs = [
        OutputGeometry::new("z", 0, 0, 1920, 1080, 1.0).unwrap(),
        OutputGeometry::new("a", -2000, 0, 2000, 1500, 1.5).unwrap(),
    ];
    assert_eq!(choose_output(&outputs, "removed").unwrap().id(), "a");
    assert_eq!(choose_output(&outputs, "z").unwrap().id(), "z");
    assert!(choose_output(&[], "removed").is_none());
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 8.1] {
        assert!(OutputGeometry::new("screen", 0, 0, 1920, 1080, scale).is_err());
    }
    assert!(OutputGeometry::new("screen", i32::MAX, 0, 1, 1, 1.0).is_err());
    let view = LogicalSize {
        width: 100.0,
        height: 100.0,
    };
    for figure in [
        LogicalSize {
            width: 101.0,
            height: 50.0,
        },
        LogicalSize {
            width: f64::NAN,
            height: 10.0,
        },
        LogicalSize {
            width: 1.0,
            height: f64::INFINITY,
        },
    ] {
        assert!(map_logical(point(0, 0), Rect::full(), view, figure, 0.0).is_err());
    }
    assert!(map_logical(point(0, 0), Rect::full(), view, view, 1.0).is_err());
}
