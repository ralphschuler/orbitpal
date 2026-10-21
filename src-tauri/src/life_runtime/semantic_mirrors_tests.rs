//! Public metadata/sequence fixtures only. These create no Native proof,
//! constructor, visible frame, action, playback lease or successful GPU output.
use super::*;
use crate::life_availability::{Availability, Reason, Snapshot, Source};
use serde_json::Value;

fn make_core() -> Core {
    Core::new(Snapshot { state: Availability::Unknown, reason: Reason::Initializing,
        generation: 1, source: Source::Unsupported })
}

#[test]
fn empty_reads_keep_one_epoch_and_advance_independently_without_starting_life() {
    let mut core = make_core();
    let first = core.semantic_mirror_packet(BTreeSet::new(), 0).unwrap();
    let second = core.semantic_mirror_packet(BTreeSet::new(), 0).unwrap();
    assert_eq!(first.version, 1); assert_eq!(first.sequence, 1); assert_eq!(second.sequence, 2);
    assert_eq!(first.epoch, second.epoch); assert!(uuid::Uuid::parse_str(&first.epoch).is_ok());
    assert!(first.owners.is_empty()); assert!(second.object_owner_actor_id.is_none());
    assert!(core.sessions.is_empty()); assert!(!core.any_active());
    assert!(!core.actions.as_ref().unwrap().busy());
    assert_ne!(first.epoch, make_core().semantic_mirror_packet(BTreeSet::new(), 0).unwrap().epoch);
}

#[test]
fn pending_and_cleanup_rows_suppress_without_selecting_a_world_or_claiming_ready() {
    let mut core = make_core();
    let packet = core.semantic_mirror_packet(BTreeSet::from(["b".into(), "a".into()]), 10).unwrap();
    assert_eq!(packet.owners.iter().map(|row| row.actor_id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert!(packet.object_owner_actor_id.is_none());
    let json = serde_json::to_value(&packet).unwrap();
    for row in json["owners"].as_array().unwrap() {
        assert_eq!(row["controlled"], true); assert_eq!(row["visible"], false);
        for name in ["owner", "worldInstanceId", "sourceWorldId", "stageId", "configRevision"] {
            assert_eq!(row[name], Value::Null);
        }
    }
    let cleared = core.semantic_mirror_packet(BTreeSet::new(), 11).unwrap();
    assert_eq!(cleared.epoch, packet.epoch); assert!(cleared.sequence > packet.sequence);
    assert!(cleared.owners.is_empty()); assert!(core.sessions.is_empty());
}

#[test]
fn obs_scope_omits_other_actors_and_redacts_a_foreign_object_election() {
    let packet = SemanticMirrorPacket { version: 1, epoch: uuid::Uuid::new_v4().to_string(), sequence: 7,
        owners: vec![SemanticMirrorOwner::suppressed("a".into()), SemanticMirrorOwner::suppressed("b".into())],
        object_owner_actor_id: Some("a".into()) };
    let b = packet.for_actor("b");
    assert_eq!(b.epoch, packet.epoch); assert_eq!(b.sequence, packet.sequence);
    assert_eq!(b.owners, [packet.owners[1].clone()]);
    assert!(b.object_owner_actor_id.is_none());
    assert_eq!(packet.for_actor("a").object_owner_actor_id.as_deref(), Some("a"));
    assert!(packet.for_actor("not-a-participant").owners.is_empty());
    assert_eq!(packet.owners.len(), 2);
}

#[test]
fn sequence_exhaustion_cannot_wrap_or_change_the_core_or_epoch() {
    let mut core = make_core(); core.semantic_mirror_sequence = crate::life_domain::MAX_COUNTER;
    let epoch = core.semantic_mirror_epoch.clone();
    assert!(core.semantic_mirror_packet(BTreeSet::from(["a".into()]), 0).is_err());
    assert_eq!(core.semantic_mirror_sequence, crate::life_domain::MAX_COUNTER);
    assert_eq!(core.semantic_mirror_epoch, epoch); assert!(core.sessions.is_empty());
}
