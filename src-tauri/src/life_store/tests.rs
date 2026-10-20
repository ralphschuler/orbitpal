use super::*;
use crate::life_domain::{Activity, CommittedEvent, NeedEffect};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Barrier},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("orbitpal-life-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn store(&self, now: u64) -> Store {
        Store::new_at(self.0.clone(), now).unwrap()
    }
    fn seed_corrupt(&self, seed: impl FnOnce(&Connection)) {
        // Deliberately malformed legacy rows cannot be created through Store.
        // Disable FK checks only on this disposable fixture connection.
        let db = Connection::open(self.0.join("cohost.sqlite3")).unwrap();
        db.pragma_update(None, "foreign_keys", false).unwrap();
        assert_eq!(
            db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        seed(&db);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn needs() -> Needs {
    Needs::new(5_000, 5_000, 5_000, 5_000).unwrap()
}
fn initialize(store: &Store, id: &str, now: u64) -> LifeSnapshot {
    store
        .initialize_life(
            id,
            WorldId::new("private-life-world").unwrap(),
            needs(),
            RandomState::seeded(7),
            now,
        )
        .unwrap()
}
fn plan(
    store: &Store,
    id: &str,
    snapshot: &LifeSnapshot,
    effect: NeedEffect,
    now: u64,
) -> LifeCommit {
    store
        .apply_life_event(
            id,
            snapshot.state().revision(),
            0,
            LifeEvent::Plan {
                activity: Activity::Rest,
                completion_effect: effect,
            },
            now,
        )
        .unwrap()
}
fn complete(
    store: &Store,
    id: &str,
    snapshot: &LifeSnapshot,
    effect: NeedEffect,
    now: u64,
) -> LifeSnapshot {
    let p = plan(store, id, snapshot, effect, now);
    let action = p
        .snapshot
        .state()
        .current_action()
        .unwrap()
        .intent()
        .id()
        .clone();
    let s = store
        .apply_life_event(
            id,
            p.snapshot.state().revision(),
            0,
            LifeEvent::Start {
                action_id: action.clone(),
            },
            now,
        )
        .unwrap();
    store
        .apply_life_event(
            id,
            s.snapshot.state().revision(),
            0,
            LifeEvent::Complete { action_id: action },
            now,
        )
        .unwrap()
        .snapshot
}
fn raw(store: &Store) -> Value {
    let db = store.db.lock().unwrap();
    let mut q=db.prepare("SELECT bot_id,schema_version,revision,saved_at_ms,random_state,state_json FROM bot_life_states ORDER BY bot_id").unwrap();
    let states = q
        .query_map([], |row| {
            Ok(json!([
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?
            ]))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut q =
        db.prepare("SELECT bot_id,revision,origin,event_json FROM bot_life_events ORDER BY bot_id,revision").unwrap();
    let events = q
        .query_map([], |row| {
            Ok(json!([
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?
            ]))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    json!({"states":states,"events":events})
}
fn old_database(root: &PathBuf) {
    let db = Connection::open(root.join("cohost.sqlite3")).unwrap();
    // Exact pre-life table families; there is no fictitious life-v0 record.
    db.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);CREATE TABLE messages(id TEXT PRIMARY KEY,profile_id TEXT NOT NULL,role TEXT NOT NULL,text TEXT NOT NULL,source TEXT NOT NULL,user_name TEXT NOT NULL,created_at INTEGER NOT NULL);CREATE TABLE secret_ids(id TEXT PRIMARY KEY);CREATE TABLE bot_profiles(id TEXT PRIMARY KEY,profile TEXT NOT NULL);CREATE TABLE mcp_library(id TEXT PRIMARY KEY,server TEXT NOT NULL);").unwrap();
    let mut profile: Value = serde_json::from_str(super::super::DEFAULT_PROFILE).unwrap();
    profile["name"] = json!("Preserved old bot");
    db.execute(
        "INSERT INTO settings VALUES('profile',?1)",
        [profile.to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO bot_profiles VALUES('nova',?1)",
        [profile.to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO settings VALUES('automatic_update_checks','false')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO messages VALUES('old-message','nova','user','Synthetic old conversation','manual','Operator',1)",
        [],
    )
    .unwrap();
}

#[test]
fn real_pre_life_database_migrates_atomically_and_preserves_existing_data() {
    let fixture = Fixture::new();
    old_database(&fixture.0);
    let store = fixture.store(10);
    assert_eq!(store.profile().unwrap()["name"], "Preserved old bot");
    assert!(!store.automatic_update_checks().unwrap());
    assert_eq!(
        store.history().unwrap()[0]["text"],
        "Synthetic old conversation"
    );
    assert_eq!(
        store.life_availability("nova").unwrap(),
        LifeAvailability::NotInitialized
    );
    assert!(store.load_life("nova").unwrap().is_none());
    let version: i64 = store
        .db
        .lock()
        .unwrap()
        .query_row("SELECT version FROM life_schema", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    drop(store);
    let reopened = fixture.store(20);
    assert_eq!(reopened.history().unwrap().len(), 1);
    assert_eq!(raw(&reopened), json!({"states":[],"events":[]}));
}

#[test]
fn v1_initialization_is_owner_bound_and_cannot_reset_existing_ids_or_rng() {
    let fixture = Fixture::new();
    let store = fixture.store(10);
    let saved = initialize(&store, "nova", 10);
    assert_eq!(saved.state().revision(), 0);
    assert_eq!(saved.random().state(), 7);
    assert!(saved.events().is_empty());
    assert_eq!(
        store.life_availability("nova").unwrap(),
        LifeAvailability::Ready
    );
    let before = raw(&store);
    assert_eq!(
        store.initialize_life(
            "nova",
            WorldId::new("other").unwrap(),
            needs(),
            RandomState::seeded(999),
            20
        ),
        Err(LifeStoreError::AlreadyExists)
    );
    assert_eq!(
        store.initialize_life(
            "unknown",
            WorldId::new("world").unwrap(),
            needs(),
            RandomState::seeded(0),
            20
        ),
        Err(LifeStoreError::UnknownBot)
    );
    assert_eq!(
        store.load_life("../nova"),
        Err(LifeStoreError::InvalidInput)
    );
    assert_eq!(raw(&store), before);
}

#[test]
fn restart_interrupts_pending_actions_and_only_advances_a_bounded_offline_clock() {
    let fixture = Fixture::new();
    let store = fixture.store(100);
    let original = initialize(&store, "nova", 100);
    let planned = plan(
        &store,
        "nova",
        &original,
        NeedEffect::new(5_000, 0, 0, 0).unwrap(),
        100,
    );
    let action = planned.intents[0].id().clone();
    let started = store
        .apply_life_event(
            "nova",
            1,
            20,
            LifeEvent::Start {
                action_id: action.clone(),
            },
            100,
        )
        .unwrap();
    let random = started.snapshot.random();
    drop(store);
    let reopened = fixture.store(100 + 7 * 24 * 60 * 60 * 1000);
    let restored = reopened.load_life("nova").unwrap().unwrap();
    let current = restored.state().current_action().unwrap();
    assert_eq!(current.phase(), ActionPhase::Cancelled);
    assert_eq!(current.cancellation(), Some(CancelReason::Restart));
    assert_eq!(restored.state().needs(), original.state().needs());
    assert_eq!(
        restored.state().simulation_time().milliseconds(),
        20 + MAX_OFFLINE_MS
    );
    assert_eq!(restored.random(), random);
    assert_eq!(
        restored
            .events()
            .iter()
            .rev()
            .take(2)
            .map(|e| e.origin())
            .collect::<Vec<_>>(),
        vec![EventOrigin::Offline, EventOrigin::Restart]
    );
    assert!(matches!(
        restored.events().last().unwrap().record().event(),
        CommittedEvent::Advanced { .. }
    ));
    let stale = reopened
        .apply_life_event(
            "nova",
            restored.state().revision(),
            0,
            LifeEvent::Complete { action_id: action },
            restored.saved_at_ms(),
        )
        .unwrap();
    assert_eq!(stale.outcome, WriteOutcome::Stale);
    assert!(stale.intents.is_empty());
    assert_eq!(stale.snapshot, restored);
    let next = plan(
        &reopened,
        "nova",
        &restored,
        NeedEffect::default(),
        restored.saved_at_ms(),
    );
    assert_eq!(next.intents[0].id().ordinal(), 2);
    // No session/configuration grant is represented by the persisted state.
    assert!(crate::autopilot::AutopilotHub::default()
        .all()
        .unwrap()
        .is_empty());
}

#[test]
fn confirmed_values_survive_restart_and_repeated_completion_never_recredits() {
    let fixture = Fixture::new();
    let store = fixture.store(100);
    let first = initialize(&store, "nova", 100);
    let done = complete(
        &store,
        "nova",
        &first,
        NeedEffect::new(1_500, 0, 0, 0).unwrap(),
        100,
    );
    let action = done.state().current_action().unwrap().intent().id().clone();
    drop(store);
    let reopened = fixture.store(100);
    let loaded = reopened.load_life("nova").unwrap().unwrap();
    assert_eq!(loaded, done);
    assert_eq!(loaded.state().needs().energy().units(), 6_500);
    let before = raw(&reopened);
    let again = reopened
        .apply_life_event(
            "nova",
            loaded.state().revision(),
            60_000,
            LifeEvent::Complete { action_id: action },
            200,
        )
        .unwrap();
    assert_eq!(again.outcome, WriteOutcome::Duplicate);
    assert_eq!(again.snapshot, loaded);
    assert_eq!(raw(&reopened), before);
}

#[test]
fn backward_and_large_wall_clock_jumps_create_neither_penalties_nor_action_chains() {
    let fixture = Fixture::new();
    let store = fixture.store(50_000);
    let saved = initialize(&store, "nova", 50_000);
    drop(store);
    let backwards = fixture.store(10);
    let a = backwards.load_life("nova").unwrap().unwrap();
    assert_eq!(a.state(), saved.state());
    assert_eq!(a.saved_at_ms(), 10);
    assert!(a.events().is_empty());
    drop(backwards);
    let forward = fixture.store(life_domain::MAX_COUNTER);
    let b = forward.load_life("nova").unwrap().unwrap();
    assert_eq!(b.state().simulation_time().milliseconds(), MAX_OFFLINE_MS);
    assert_eq!(b.state().needs(), needs());
    assert!(b.state().current_action().is_none());
    assert_eq!(b.events().len(), 1);
    assert_eq!(b.events()[0].origin(), EventOrigin::Offline);
    drop(forward);
    let same = fixture.store(life_domain::MAX_COUNTER);
    assert_eq!(same.load_life("nova").unwrap().unwrap(), b);
}

#[test]
fn low_frequency_snapshots_defer_render_frame_ticks_without_losing_a_cumulative_checkpoint() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    let saved = initialize(&store, "nova", 0);
    let before = raw(&store);
    for elapsed in [-1, 0, 16, 1_000, SNAPSHOT_INTERVAL_MS - 1] {
        let result = store
            .apply_life_event("nova", 0, elapsed, LifeEvent::Tick {}, 30_000)
            .unwrap();
        assert_eq!(result.outcome, WriteOutcome::Deferred);
        assert_eq!(result.snapshot, saved);
        assert!(result.intents.is_empty());
    }
    assert_eq!(raw(&store), before);
    let result = store
        .apply_life_event("nova", 0, SNAPSHOT_INTERVAL_MS, LifeEvent::Tick {}, 30_000)
        .unwrap();
    assert_eq!(result.outcome, WriteOutcome::Applied);
    assert_eq!(
        result.snapshot.state().simulation_time().milliseconds(),
        30_000
    );
    assert_eq!(result.snapshot.events().len(), 1);
    let jump = store
        .apply_life_event("nova", 1, i64::MAX, LifeEvent::Tick {}, 60_000)
        .unwrap();
    assert_eq!(
        jump.snapshot.state().simulation_time().milliseconds(),
        30_000 + life_domain::MAX_ELAPSED_MS
    );
}

#[test]
fn bot_bound_store_views_never_read_or_mutate_another_actors_life_state() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    let a = initialize(&store, "nova", 0);
    let mut other = store.profile().unwrap();
    other["id"] = json!(format!("_{}", "x".repeat(95)));
    other["name"] = json!("Other");
    let other = store.save_profile(other).unwrap();
    let id = other["id"].as_str().unwrap();
    let b = initialize(&store, id, 0);
    let scoped = store.scoped("nova").unwrap();
    assert_eq!(scoped.load_life(id), Err(LifeStoreError::WrongBot));
    assert_eq!(scoped.life_availability(id), Err(LifeStoreError::WrongBot));
    assert_eq!(
        scoped.apply_life_event(id, 0, 0, LifeEvent::Tick {}, 0),
        Err(LifeStoreError::WrongBot)
    );
    complete(
        &scoped,
        "nova",
        &a,
        NeedEffect::new(1_000, 0, 0, 0).unwrap(),
        0,
    );
    assert_eq!(store.load_life(id).unwrap().unwrap(), b);
    assert_eq!(
        store
            .load_life("nova")
            .unwrap()
            .unwrap()
            .state()
            .needs()
            .energy()
            .units(),
        6_000
    );
}

#[test]
fn compare_and_swap_serializes_two_real_sqlite_connections() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    initialize(&store, "nova", 0);
    let a = store.scoped("nova").unwrap();
    let b = store.scoped("nova").unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let run = |store: Store, gate: Arc<Barrier>| {
        std::thread::spawn(move || {
            gate.wait();
            store.apply_life_event(
                "nova",
                0,
                0,
                LifeEvent::Plan {
                    activity: Activity::Wave,
                    completion_effect: NeedEffect::default(),
                },
                0,
            )
        })
    };
    let a = run(a, barrier.clone());
    let b = run(b, barrier.clone());
    barrier.wait();
    let results = [a.join().unwrap(), b.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(LifeStoreError::RevisionConflict)))
            .count(),
        1
    );
    let loaded = store.load_life("nova").unwrap().unwrap();
    assert_eq!(loaded.state().revision(), 1);
    assert_eq!(loaded.events().len(), 1);
    assert_eq!(
        loaded
            .state()
            .current_action()
            .unwrap()
            .intent()
            .id()
            .ordinal(),
        1
    );
}

#[test]
fn state_event_and_rng_updates_roll_back_on_insert_and_commit_failure() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    let original = initialize(&store, "nova", 0);
    let before = raw(&store);
    store.db.lock().unwrap().execute_batch("CREATE TRIGGER fail_life_event BEFORE INSERT ON bot_life_events BEGIN SELECT RAISE(ABORT,'synthetic-private-marker'); END;").unwrap();
    let event = LifeEvent::Plan {
        activity: Activity::Play,
        completion_effect: NeedEffect::new(1_000, 0, 0, 0).unwrap(),
    };
    let error = store
        .apply_life_event("nova", 0, 0, event.clone(), 1)
        .unwrap_err();
    assert_eq!(error, LifeStoreError::StorageUnavailable);
    assert!(!error.to_string().contains("synthetic-private-marker"));
    assert_eq!(raw(&store), before);
    store.db.lock().unwrap().execute_batch("DROP TRIGGER fail_life_event;PRAGMA foreign_keys=ON;CREATE TABLE life_test_parent(id TEXT PRIMARY KEY);CREATE TABLE life_test_child(id TEXT REFERENCES life_test_parent(id) DEFERRABLE INITIALLY DEFERRED);CREATE TRIGGER fail_life_commit AFTER INSERT ON bot_life_events BEGIN INSERT INTO life_test_child VALUES('missing-parent'); END;").unwrap();
    assert_eq!(
        store.apply_life_event("nova", 0, 0, event.clone(), 1),
        Err(LifeStoreError::StorageUnavailable)
    );
    assert_eq!(raw(&store), before);
    store
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_life_commit;")
        .unwrap();
    let valid = store.apply_life_event("nova", 0, 0, event, 1).unwrap();
    assert_eq!(valid.intents[0].id().ordinal(), 1);
    assert_ne!(valid.snapshot.random(), original.random());
    assert_eq!(valid.snapshot.state().revision(), 1);
}

#[test]
fn interrupted_uncommitted_write_and_migration_leave_no_half_state() {
    let fixture = Fixture::new();
    old_database(&fixture.0);
    {
        let mut db = Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("CREATE TABLE life_schema(singleton INTEGER PRIMARY KEY,version INTEGER);INSERT INTO life_schema VALUES(1,1);").unwrap();
        // Dropping a real uncommitted SQLite transaction models interruption.
    }
    let store = fixture.store(0);
    let original = initialize(&store, "nova", 0);
    let before = raw(&store);
    {
        let mut db = store.db.lock().unwrap();
        let tx = db.transaction().unwrap();
        tx.execute(
            "UPDATE bot_life_states SET state_json='synthetic-partial-state',revision=10 WHERE bot_id='nova'",
            [],
        )
        .unwrap();
    }
    assert_eq!(raw(&store), before);
    drop(store);
    let reopened = fixture.store(0);
    assert_eq!(reopened.load_life("nova").unwrap().unwrap(), original);
}

#[test]
fn failed_schema_creation_rolls_back_and_only_disables_optional_life() {
    let fixture = Fixture::new();
    old_database(&fixture.0);
    {
        let db = Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
        db.execute_batch("CREATE VIEW bot_life_states AS SELECT 'blocked' AS bot_id;")
            .unwrap();
    }
    let store = fixture.store(0);
    assert_eq!(store.profile().unwrap()["name"], "Preserved old bot");
    assert_eq!(
        store.life_availability("nova").unwrap(),
        LifeAvailability::Unavailable(LifeStoreError::StorageUnavailable)
    );
    assert!(!table_exists(&store.db.lock().unwrap(), "life_schema").unwrap());
    assert!(!table_exists(&store.db.lock().unwrap(), "bot_life_events").unwrap());
    store.save_automatic_update_checks(true).unwrap();
    assert!(store.automatic_update_checks().unwrap());
}

#[test]
fn future_and_unversioned_life_schemas_are_not_guessed_or_rewritten() {
    for future in [false, true] {
        let fixture = Fixture::new();
        old_database(&fixture.0);
        {
            let db = Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
            if future {
                db.execute_batch("CREATE TABLE life_schema(singleton INTEGER PRIMARY KEY,version INTEGER);INSERT INTO life_schema VALUES(1,2);").unwrap();
            } else {
                db.execute_batch("CREATE TABLE bot_life_states(bot_id TEXT,state_json TEXT);INSERT INTO bot_life_states VALUES('nova','synthetic-private-marker');").unwrap();
            }
        }
        let store = fixture.store(0);
        assert_eq!(
            store.life_availability("nova").unwrap(),
            LifeAvailability::Unavailable(LifeStoreError::UnsupportedSchema)
        );
        assert_eq!(store.profile().unwrap()["name"], "Preserved old bot");
        assert_eq!(store.history().unwrap().len(), 1);
        assert!(!table_exists(&store.db.lock().unwrap(), "bot_life_events").unwrap());
        assert_eq!(
            store.load_life("nova"),
            Err(LifeStoreError::UnsupportedSchema)
        );
    }
}

#[test]
fn one_corrupt_bot_stays_unavailable_untouched_while_other_bot_and_app_data_work() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    initialize(&store, "nova", 0);
    let mut other = store.profile().unwrap();
    other["id"] = json!("james");
    other["name"] = json!("James");
    store.save_profile(other).unwrap();
    let good = initialize(&store, "james", 0);
    store
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE bot_life_states SET state_json=?1 WHERE bot_id='nova'",
            ["synthetic-private-credential-marker"],
        )
        .unwrap();
    let bad_before = raw(&store)["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == "nova")
        .unwrap()
        .clone();
    drop(store);
    let reopened = fixture.store(60_000);
    assert_eq!(
        reopened.life_availability("nova").unwrap(),
        LifeAvailability::Unavailable(LifeStoreError::CorruptData)
    );
    let error = reopened.load_life("nova").unwrap_err();
    assert!(!error.to_string().contains("credential-marker"));
    assert_eq!(
        raw(&reopened)["states"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r[0] == "nova")
            .unwrap(),
        &bad_before
    );
    let loaded = reopened.load_life("james").unwrap().unwrap();
    assert_eq!(loaded.state().needs(), good.state().needs());
    assert_eq!(loaded.state().simulation_time().milliseconds(), 60_000);
    assert_eq!(reopened.profile().unwrap()["id"], "james");
    reopened.save_automatic_update_checks(false).unwrap();
    assert!(!reopened.automatic_update_checks().unwrap());
    reopened
        .add_message(
            "user",
            "Synthetic available conversation",
            "manual",
            "Operator",
        )
        .unwrap();
    assert_eq!(reopened.history().unwrap().len(), 1);
    assert_eq!(
        reopened.scoped("nova").unwrap().load_life("nova"),
        Err(LifeStoreError::CorruptData)
    );
}

#[test]
fn persisted_future_schema_actor_mismatch_and_event_corruption_fail_closed() {
    for kind in ["future", "actor", "event", "origin", "rng", "oversize"] {
        let fixture = Fixture::new();
        let store = fixture.store(0);
        let a = initialize(&store, "nova", 0);
        let saved = complete(&store, "nova", &a, NeedEffect::default(), 0);
        {
            let db = store.db.lock().unwrap();
            match kind {
                "future" => {
                    let mut state = serde_json::to_value(saved.state()).unwrap();
                    state["schemaVersion"] = json!(2);
                    db.execute(
                        "UPDATE bot_life_states SET state_json=?1",
                        [state.to_string()],
                    )
                    .unwrap();
                }
                "actor" => {
                    let mut state = serde_json::to_value(saved.state()).unwrap();
                    state["actorId"] = json!("james");
                    db.execute(
                        "UPDATE bot_life_states SET state_json=?1",
                        [state.to_string()],
                    )
                    .unwrap();
                }
                "event" => {
                    db.execute("DELETE FROM bot_life_events WHERE revision=2", [])
                        .unwrap();
                }
                "origin" => {
                    db.execute(
                        "UPDATE bot_life_events SET origin='offline' WHERE revision=3",
                        [],
                    )
                    .unwrap();
                }
                "rng" => {
                    db.execute(
                        "UPDATE bot_life_states SET random_state='synthetic-private-marker'",
                        [],
                    )
                    .unwrap();
                }
                "oversize" => {
                    db.execute_batch("PRAGMA ignore_check_constraints=ON;")
                        .unwrap();
                    db.execute(
                        "UPDATE bot_life_states SET state_json=?1",
                        ["x".repeat(MAX_STATE_BYTES + 1)],
                    )
                    .unwrap();
                }
                _ => unreachable!(),
            }
        }
        let before = raw(&store);
        drop(store);
        let reopened = fixture.store(1);
        let expected = if kind == "future" {
            LifeStoreError::UnsupportedSchema
        } else {
            LifeStoreError::CorruptData
        };
        assert_eq!(reopened.load_life("nova"), Err(expected));
        assert_eq!(raw(&reopened), before);
        assert!(reopened.profile().is_ok());
    }
}

#[test]
fn event_retention_and_terminal_watermarks_survive_many_commits_and_reopen() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    let mut saved = initialize(&store, "nova", 0);
    saved = complete(
        &store,
        "nova",
        &saved,
        NeedEffect::new(1, 0, 0, 0).unwrap(),
        0,
    );
    let first = saved
        .state()
        .current_action()
        .unwrap()
        .intent()
        .id()
        .clone();
    for _ in 0..150 {
        saved = complete(
            &store,
            "nova",
            &saved,
            NeedEffect::new(1, 0, 0, 0).unwrap(),
            0,
        );
    }
    assert_eq!(saved.events().len(), life_domain::MAX_RECENT_EVENTS);
    assert_eq!(
        raw(&store)["events"].as_array().unwrap().len(),
        life_domain::MAX_RECENT_EVENTS
    );
    assert_eq!(saved.state().needs().energy().units(), 5_151);
    drop(store);
    let reopened = fixture.store(0);
    let loaded = reopened.load_life("nova").unwrap().unwrap();
    assert_eq!(loaded, saved);
    let before = raw(&reopened);
    let stale = reopened
        .apply_life_event(
            "nova",
            loaded.state().revision(),
            0,
            LifeEvent::Complete { action_id: first },
            0,
        )
        .unwrap();
    assert_eq!(stale.outcome, WriteOutcome::Stale);
    assert_eq!(stale.snapshot, loaded);
    assert_eq!(raw(&reopened), before);
}

#[test]
fn bot_copy_import_export_keep_life_local_and_do_not_transfer_grants_or_identity() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    let source = initialize(&store, "nova", 0);
    complete(
        &store,
        "nova",
        &source,
        NeedEffect::new(1_000, 0, 0, 0).unwrap(),
        0,
    );
    let mut profile = store.profile().unwrap();
    profile["screenContext"] = json!({"enabled":true,"displayId":"synthetic-monitor"});
    profile["customUserSetting"] = json!("preserved");
    profile["lifeState"] = json!({"private":"synthetic-life-marker"});
    store.save_profile(profile).unwrap();
    let copied = store.create_bot("Copied bot", true).unwrap();
    let copied_id = copied["id"].as_str().unwrap();
    assert_ne!(copied_id, "nova");
    assert!(store.load_life(copied_id).unwrap().is_none());
    assert_eq!(copied["screenContext"]["enabled"], false);
    assert!(copied.get("lifeState").is_none());
    assert_eq!(copied["customUserSetting"], "preserved");
    store.select_bot("nova").unwrap();
    let package = fixture.0.join("profile.json");
    store.export(&package).unwrap();
    let encoded = fs::read_to_string(&package).unwrap();
    assert!(!encoded.contains("private-life-world"));
    assert!(!encoded.contains("synthetic-life-marker"));
    let mut bundle: Value = serde_json::from_str(&encoded).unwrap();
    bundle["lifeState"] =
        serde_json::to_value(store.load_life("nova").unwrap().unwrap().state()).unwrap();
    bundle["profile"]["lifeSnapshot"] = json!({"active":true,"screenGrant":true});
    fs::write(&package, bundle.to_string()).unwrap();
    let imported = store.import(&package).unwrap();
    let imported_id = imported["id"].as_str().unwrap();
    assert_ne!(imported_id, "nova");
    assert_ne!(imported_id, copied_id);
    assert!(store.load_life(imported_id).unwrap().is_none());
    assert!(imported.get("lifeSnapshot").is_none());
    assert_eq!(imported["screenContext"]["enabled"], false);
    assert_eq!(imported["allowSkillScripts"], false);
    assert_eq!(
        store
            .load_life("nova")
            .unwrap()
            .unwrap()
            .state()
            .needs()
            .energy()
            .units(),
        6_000
    );
}

#[test]
fn rejected_inputs_and_domain_errors_preserve_every_persisted_row() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    let a = initialize(&store, "nova", 0);
    let p = plan(&store, "nova", &a, NeedEffect::default(), 0);
    let before = raw(&store);
    let id = p.intents[0].id().clone();
    assert_eq!(
        store.apply_life_event(
            "nova",
            0,
            0,
            LifeEvent::Complete {
                action_id: id.clone()
            },
            0
        ),
        Err(LifeStoreError::RevisionConflict)
    );
    assert_eq!(
        store.apply_life_event(
            "nova",
            1,
            0,
            LifeEvent::Complete {
                action_id: id.clone()
            },
            0
        ),
        Err(LifeStoreError::Domain(LifeError::InvalidTransition))
    );
    assert_eq!(
        store.apply_life_event(
            "nova",
            1,
            0,
            LifeEvent::Cancel {
                action_id: id,
                reason: CancelReason::Restart
            },
            0
        ),
        Err(LifeStoreError::InvalidInput)
    );
    assert_eq!(
        store.apply_life_event("nova", 1, 0, LifeEvent::Tick {}, u64::MAX),
        Err(LifeStoreError::InvalidInput)
    );
    assert_eq!(raw(&store), before);
}

#[test]
fn invalid_unaddressable_row_does_not_disable_an_existing_valid_bot() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    initialize(&store, "nova", 0);
    let malformed_id = "x".repeat(97);
    fixture.seed_corrupt(|db| { db.execute("INSERT INTO bot_life_states(bot_id,schema_version,revision,saved_at_ms,random_state,state_json) SELECT ?1,schema_version,revision,saved_at_ms,random_state,state_json FROM bot_life_states WHERE bot_id='nova'",[&malformed_id]).unwrap(); });
    let bad_before = raw(&store)["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == malformed_id)
        .unwrap()
        .clone();
    drop(store);
    let reopened = fixture.store(1);
    assert_eq!(
        reopened.life_availability("nova").unwrap(),
        LifeAvailability::Ready
    );
    assert_eq!(
        reopened.life_recovery_report().unwrap().unidentified_rows,
        1
    );
    assert_eq!(reopened.life_recovery_report().unwrap().schema_error, None);
    assert_eq!(
        raw(&reopened)["states"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r[0] == malformed_id)
            .unwrap(),
        &bad_before
    );
    assert_eq!(
        reopened.load_life(&malformed_id),
        Err(LifeStoreError::InvalidInput)
    );
}

#[test]
fn changed_or_extra_global_metadata_is_rejected_on_an_already_open_store() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    initialize(&store, "nova", 0);
    let before = raw(&store);
    store
        .db
        .lock()
        .unwrap()
        .execute("UPDATE life_schema SET version=2", [])
        .unwrap();
    assert_eq!(
        store.load_life("nova"),
        Err(LifeStoreError::UnsupportedSchema)
    );
    assert_eq!(
        store.apply_life_event("nova", 0, 30_000, LifeEvent::Tick {}, 30_000),
        Err(LifeStoreError::UnsupportedSchema)
    );
    assert_eq!(raw(&store), before);
    store
        .db
        .lock()
        .unwrap()
        .execute_batch(
            "UPDATE life_schema SET version=1;PRAGMA ignore_check_constraints=ON;INSERT INTO life_schema VALUES(2,1);",
        )
        .unwrap();
    assert_eq!(store.load_life("nova"), Err(LifeStoreError::CorruptData));
    assert_eq!(raw(&store), before);
}

#[test]
fn sqlite_blob_owner_is_quarantined_without_disabling_a_valid_bot() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    initialize(&store, "nova", 0);
    let blob = b"bad-owner".as_slice();
    fixture.seed_corrupt(|db| { db.execute("INSERT INTO bot_life_states(bot_id,schema_version,revision,saved_at_ms,random_state,state_json) SELECT ?1,schema_version,revision,saved_at_ms,random_state,state_json FROM bot_life_states WHERE bot_id='nova'",[blob]).unwrap(); });
    let before: String = store
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT state_json FROM bot_life_states WHERE typeof(bot_id)='blob'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(store);
    let reopened = fixture.store(1);
    assert_eq!(
        reopened.life_availability("nova").unwrap(),
        LifeAvailability::Ready
    );
    let report = reopened.life_recovery_report().unwrap();
    assert_eq!(report.schema_error, None);
    assert_eq!(report.unidentified_rows, 1);
    let after: String = reopened
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT state_json FROM bot_life_states WHERE typeof(bot_id)='blob'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    assert!(reopened.profile().is_ok());
}

#[test]
fn invalid_utf8_text_owner_is_quarantined_without_blocking_valid_life() {
    let fixture = Fixture::new();
    let store = fixture.store(0);
    initialize(&store, "nova", 0);
    fixture.seed_corrupt(|db| { db.execute_batch("INSERT INTO bot_life_states(bot_id,schema_version,revision,saved_at_ms,random_state,state_json) SELECT CAST(x'ff' AS TEXT),schema_version,revision,saved_at_ms,random_state,state_json FROM bot_life_states WHERE bot_id='nova';").unwrap(); });
    let before: Vec<u8> = store
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT CAST(bot_id AS BLOB) FROM bot_life_states WHERE bot_id<> 'nova'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, vec![255]);
    drop(store);
    let reopened = fixture.store(1);
    assert_eq!(
        reopened.life_availability("nova").unwrap(),
        LifeAvailability::Ready
    );
    assert_eq!(reopened.life_recovery_report().unwrap().schema_error, None);
    assert_eq!(
        reopened.life_recovery_report().unwrap().unidentified_rows,
        1
    );
    let after: Vec<u8> = reopened
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT CAST(bot_id AS BLOB) FROM bot_life_states WHERE bot_id<> 'nova'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    assert!(reopened.profile().is_ok());
}

#[test]
fn imports_cannot_attach_to_orphaned_local_state_or_event_identities() {
    for orphan_events in [false, true] {
        let fixture = Fixture::new();
        let store = fixture.store(0);
        let mut incoming = store.profile().unwrap();
        incoming["id"] = json!("incoming");
        incoming["name"] = json!("Incoming");
        if orphan_events {
            let state = LifeState::new(
                ActorId::new("incoming").unwrap(),
                WorldId::new("orphan-world").unwrap(),
                needs(),
            );
            let p = life_domain::reduce(
                &state,
                0,
                RandomState::seeded(7),
                LifeEvent::Plan {
                    activity: Activity::Rest,
                    completion_effect: NeedEffect::default(),
                },
            )
            .unwrap();
            let record = p.state.recent_events().last().unwrap();
            fixture.seed_corrupt(|db| { db.execute(
                    "INSERT INTO bot_life_events(bot_id,revision,origin,event_json) VALUES('incoming',1,'live',?1)",
                    [serde_json::to_string(record).unwrap()],
                )
                .unwrap(); });
        } else {
            let state = LifeState::new(
                ActorId::new("incoming").unwrap(),
                WorldId::new("orphan-world").unwrap(),
                Needs::new(9_900, 1, 2, 3).unwrap(),
            );
            fixture.seed_corrupt(|db| { db.execute("INSERT INTO bot_life_states(bot_id,schema_version,revision,saved_at_ms,random_state,state_json) VALUES('incoming',1,0,0,'0000000000000007',?1)",[serde_json::to_string(&state).unwrap()]).unwrap(); });
        }
        let before = raw(&store);
        let package = fixture.0.join("incoming.json");
        fs::write(
            &package,
            json!({"format":"streamer-ai-cohost-profile","version":2,"profile":incoming})
                .to_string(),
        )
        .unwrap();
        let imported = store.import(&package).unwrap();
        let imported_id = imported["id"].as_str().unwrap().to_owned();
        assert_ne!(imported_id, "incoming");
        assert!(store.load_life(&imported_id).unwrap().is_none());
        assert_eq!(raw(&store), before);
        drop(store);
        let reopened = fixture.store(0);
        assert!(reopened.load_life(&imported_id).unwrap().is_none());
        assert_eq!(raw(&reopened), before);
    }
}
