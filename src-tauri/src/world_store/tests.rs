use super::*;
use crate::{
    life_domain::{ActorId, Needs, RandomState, WorldId},
    world_state::{
        coordinates::{Point, Rect},
        Anchor, AnchorId, AnchorKind, Slot, SlotId, StageId,
    },
};
use serde_json::json;
use std::path::PathBuf;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("orbitpal-world-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn draft(members: &[&str]) -> WorldConfig {
    WorldConfig::new(
        StageId::new("shared-stage").unwrap(),
        Rect::full(),
        members
            .iter()
            .map(|id| ActorId::new(*id).unwrap())
            .collect(),
        vec![Anchor::new(
            AnchorId::new("rest").unwrap(),
            AnchorKind::Rest,
            Point::new(200000, 500000).unwrap(),
            vec![Slot::new(
                SlotId::new("rest-1").unwrap(),
                Point::new(200000, 500000).unwrap(),
            )],
        )
        .unwrap()],
    )
    .unwrap()
}
fn life_rows(store: &Store) -> Vec<(String, i64, i64, i64, String, String)> {
    let db = store.db.lock().unwrap();
    let mut query=db.prepare("SELECT bot_id,schema_version,revision,saved_at_ms,random_state,state_json FROM bot_life_states ORDER BY bot_id").unwrap();
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    rows
}
#[test]
fn actual_two_lineages_joining_configuration_preserves_every_life_row_and_export_boundary() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone()).unwrap();
    let second = store.create_bot("Second", false).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (id, world) in [("nova", "source-nova"), (second.as_str(), "source-second")] {
        store
            .initialize_life(
                id,
                WorldId::new(world).unwrap(),
                Needs::new(1234, 2345, 3456, 4567).unwrap(),
                RandomState::seeded(17),
                100,
            )
            .unwrap();
    }
    let before = life_rows(&store);
    let config = draft(&["nova", &second]);
    let binding = OutputBinding {
        monitor_id: Some("private-monitor-marker".into()),
    };
    let saved = store.save_world_config(None, &config, &binding).unwrap();
    assert_eq!(saved.config.revision(), 1);
    assert_eq!(life_rows(&store), before);
    let selected = store.profile().unwrap()["id"].as_str().unwrap().to_owned();
    let package = fixture.0.join("fixture-profile.json");
    store.export(&package).unwrap();
    let exported = std::fs::read_to_string(package).unwrap();
    assert!(!exported.contains("shared-stage"));
    assert!(!exported.contains("private-monitor-marker"));
    let copy = store.create_bot("Copied", true).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(copy, selected);
    let current = store.load_world_config().unwrap().unwrap();
    assert!(!current
        .config
        .members()
        .iter()
        .any(|id| id.as_str() == copy));
    assert_eq!(life_rows(&store), before);
}
#[test]
fn actual_cas_serializes_two_connections_and_rejects_stale_or_changed_stage() {
    let fixture = Fixture::new();
    let first = Store::new(fixture.0.clone()).unwrap();
    let second = Store::new(fixture.0.clone()).unwrap();
    let saved = first
        .save_world_config(None, &draft(&["nova"]), &OutputBinding::default())
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let config = saved.config.clone();
    let start = barrier.clone();
    let handle = std::thread::spawn(move || {
        start.wait();
        first.save_world_config(Some(1), &config, &OutputBinding::default())
    });
    barrier.wait();
    let second_result = second.save_world_config(Some(1), &saved.config, &OutputBinding::default());
    let first_result = handle.join().unwrap();
    assert_eq!(
        usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
        1
    );
    assert!(matches!(
        first_result.err().or(second_result.err()),
        Some(WorldStoreError::StaleRevision)
    ));
    let current = second.load_world_config().unwrap().unwrap();
    assert_eq!(current.config.revision(), 2);
    assert_eq!(
        second
            .save_world_config(Some(1), &saved.config, &OutputBinding::default())
            .err(),
        Some(WorldStoreError::StaleRevision)
    );
    let changed = WorldConfig::new(
        StageId::new("other-stage").unwrap(),
        Rect::full(),
        vec![],
        vec![],
    )
    .unwrap()
    .with_revision(2)
    .unwrap();
    assert_eq!(
        second
            .save_world_config(Some(2), &changed, &OutputBinding::default())
            .err(),
        Some(WorldStoreError::WrongStage)
    );
}
#[test]
fn corrupt_or_unsupported_optional_world_never_blocks_healthy_profile_or_life_startup() {
    for corruption in [
        "UPDATE shared_world_config SET config_json='private-corrupt-marker'",
        "UPDATE shared_world_config SET schema_version=99",
        "UPDATE shared_world_config SET config_json=CAST(zeroblob(262145) AS TEXT)",
        "UPDATE shared_world_config SET output_json=x'00'",
    ] {
        let fixture = Fixture::new();
        let store = Store::new_at(fixture.0.clone(), 100).unwrap();
        store
            .initialize_life(
                "nova",
                WorldId::new("stable-lineage").unwrap(),
                Needs::new(1000, 2000, 3000, 4000).unwrap(),
                RandomState::seeded(11),
                100,
            )
            .unwrap();
        store
            .save_world_config(None, &draft(&["nova"]), &OutputBinding::default())
            .unwrap();
        let before = life_rows(&store);
        drop(store);
        let db = Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
        db.execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        db.execute_batch(corruption).unwrap();
        drop(db);
        let restarted = Store::new_at(fixture.0.clone(), 100).unwrap();
        assert_eq!(restarted.profile().unwrap()["id"], "nova");
        let error = restarted.load_world_config().err().unwrap();
        assert!(!error.message().contains("private-corrupt-marker"));
        assert!(restarted.load_life("nova").unwrap().is_some());
        assert_eq!(life_rows(&restarted), before);
        assert!(
            restarted
                .save_world_config(None, &draft(&[]), &OutputBinding::default())
                .is_err(),
            "corruption must never trigger implicit reset"
        );
    }
}
#[test]
fn unknown_members_can_be_removed_explicitly_but_scoped_stores_cannot_edit_installation() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone()).unwrap();
    assert_eq!(
        store
            .save_world_config(None, &draft(&["missing"]), &OutputBinding::default())
            .err(),
        Some(WorldStoreError::UnknownActor)
    );
    let saved = store
        .save_world_config(None, &draft(&["nova"]), &OutputBinding::default())
        .unwrap();
    assert_eq!(
        store
            .scoped("nova")
            .unwrap()
            .save_world_config(Some(1), &saved.config, &OutputBinding::default())
            .err(),
        Some(WorldStoreError::InvalidScope)
    );
    let mut encoded = serde_json::to_value(&saved.config).unwrap();
    encoded["members"] = json!(["missing"]);
    store
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE shared_world_config SET config_json=?1",
            [encoded.to_string()],
        )
        .unwrap();
    assert_eq!(
        store.load_world_config().err(),
        Some(WorldStoreError::UnknownActor)
    );
    let operator = store.world_settings_record().unwrap().unwrap();
    assert_eq!(operator.config.revision(), 1);
    let repaired = draft(&["nova"]).with_revision(1).unwrap();
    store
        .save_world_config(Some(1), &repaired, &OutputBinding::default())
        .unwrap();
    assert_eq!(
        store
            .load_world_config()
            .unwrap()
            .unwrap()
            .config
            .revision(),
        2
    );
}
#[test]
fn malformed_schema_and_private_binding_types_fail_closed_without_payloads() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone()).unwrap();
    assert!(store
        .save_world_config(
            None,
            &draft(&[]),
            &OutputBinding {
                monitor_id: Some("x".repeat(513))
            }
        )
        .is_err());
    assert!(store
        .save_world_config(
            None,
            &draft(&[]),
            &OutputBinding {
                monitor_id: Some("private\nmarker".into())
            }
        )
        .is_err());
    drop(store);
    let db = Connection::open(fixture.0.join("cohost.sqlite3")).unwrap();
    db.execute_batch(
        "DROP TABLE shared_world_config;CREATE TABLE shared_world_config(unexpected TEXT)",
    )
    .unwrap();
    drop(db);
    let store = Store::new(fixture.0.clone()).unwrap();
    assert!(store.profile().is_ok());
    assert_eq!(
        store.load_world_config().err(),
        Some(WorldStoreError::UnsupportedSchema)
    );
}
#[test]
fn imported_package_cannot_inherit_an_orphaned_installation_world_membership() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone()).unwrap();
    store
        .save_world_config(None, &draft(&["nova"]), &OutputBinding::default())
        .unwrap();
    let orphan = draft(&["incoming"]).with_revision(1).unwrap();
    // A valid typed orphan is repairable, but is never an import invitation.
    let encoded = serde_json::to_string(&orphan).unwrap();
    store
        .db
        .lock()
        .unwrap()
        .execute("UPDATE shared_world_config SET config_json=?1", [&encoded])
        .unwrap();
    assert_eq!(
        store.load_world_config().err(),
        Some(WorldStoreError::UnknownActor)
    );
    let mut profile = store.profile().unwrap();
    profile["id"] = json!("incoming");
    profile["name"] = json!("Imported");
    let path = fixture.0.join("incoming.json");
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"format":"streamer-ai-cohost-profile","version":2,"profile":profile}),
        )
        .unwrap(),
    )
    .unwrap();
    let imported = store.import(&path).unwrap();
    let id = imported["id"].as_str().unwrap();
    assert_ne!(id, "incoming");
    assert!(!store
        .world_settings_record()
        .unwrap()
        .unwrap()
        .config
        .members()
        .iter()
        .any(|member| member.as_str() == id));
    assert_eq!(
        store
            .db
            .lock()
            .unwrap()
            .query_row("SELECT config_json FROM shared_world_config", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        encoded
    );
    assert_eq!(
        store.load_world_config().err(),
        Some(WorldStoreError::UnknownActor)
    );
    // Keep the old member list intact even if the optional layout is unknown.
    store
        .db
        .lock()
        .unwrap()
        .execute("UPDATE shared_world_config SET schema_version=99", [])
        .unwrap();
    assert!(profile_id_reserved(
        &*store.db.lock().unwrap(),
        "different-incoming"
    ));
    // Ordinary profile and optional Life access remain usable.
    assert!(store.bot_profile(id).is_ok());
    assert!(store.load_life(id).unwrap().is_none());
}
