use super::*;
use crate::{
    life_domain::{Activity, ActorId, CancelReason, LifeEvent, Needs, RandomState, WorldId},
    life_self_care::{End, Need, Offer, Target},
    world_state::{
        coordinates::{Point, Rect},
        Anchor, AnchorId, AnchorKind, RuntimeEpoch, SessionStamp, Slot, SlotId, StageId,
        WorkIdentity, WorldConfig, WorldState,
    },
};

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("orbitpal-care-{}", uuid::Uuid::new_v4())))
    }
    fn store(&self) -> Store {
        Store::new(self.0.clone()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn preferences_survive_restart_with_cas_and_bot_isolation_without_activation() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let bot = store.profile().unwrap()["id"].as_str().unwrap().to_owned();
    let other = store.create_bot("Other", false).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(store.life_care_preferences(&bot).unwrap().revision, 0);
    let mut config = CareConfig::default();
    config.restore_units[1] = 2300;
    store
        .save_life_care_preferences(&bot, 0, config.clone())
        .unwrap();
    assert!(store
        .save_life_care_preferences(&bot, 0, config.clone())
        .is_err());
    assert_eq!(store.life_care_preferences(&other).unwrap().revision, 0);
    assert!(store
        .scoped(&other)
        .unwrap()
        .life_care_preferences(&bot)
        .is_err());
    assert!(store.load_life(&bot).unwrap().is_none());
    drop(store);
    let reopened = fixture.store();
    assert_eq!(reopened.life_care_preferences(&bot).unwrap().config, config);
    assert!(reopened.load_life(&bot).unwrap().is_none());
}

#[test]
fn malformed_or_exhausted_preferences_fail_without_replacing_saved_product_data() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let bot = store.profile().unwrap()["id"].as_str().unwrap().to_owned();
    let config = CareConfig::default();
    store
        .save_life_care_preferences(&bot, 0, config.clone())
        .unwrap();
    let mut bad = config.clone();
    bad.restore_units[0] = 0;
    assert!(store.save_life_care_preferences(&bot, 1, bad).is_err());
    assert!(store
        .save_life_care_preferences(&bot, crate::life_domain::MAX_COUNTER, config.clone())
        .is_err());
    assert_eq!(store.life_care_preferences(&bot).unwrap().config, config);
    store
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE life_care_preferences SET config=?1 WHERE bot_id=?2",
            params!["x".repeat(LIMIT + 1), bot],
        )
        .unwrap();
    assert!(store.life_care_preferences(&bot).is_err());
}

/// Deterministic product simulation over the real World leases and SQLite Life
/// reducer. Synthetic execution below is not WebGL or native-platform evidence.
#[test]
fn two_actors_care_for_thirty_minutes_with_single_capacity_and_interrupted_work() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let a = store.profile().unwrap()["id"].as_str().unwrap().to_owned();
    let b = store.create_bot("Second", false).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let ids = [a, b];
    let config = CareConfig::default();
    let kinds = [
        AnchorKind::Rest,
        AnchorKind::Snack,
        AnchorKind::Play,
        AnchorKind::Meeting,
    ];
    let names = ["rest", "snack", "toy", "meeting"];
    let anchors = kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| {
            let position = Point::new(200_000 + i as u32 * 150_000, 500_000).unwrap();
            Anchor::new(
                AnchorId::new(names[i]).unwrap(),
                kind,
                position,
                vec![Slot::new(SlotId::new(names[i]).unwrap(), position)],
            )
            .unwrap()
        })
        .collect();
    let world_config = WorldConfig::new(
        StageId::new("care-stage").unwrap(),
        Rect::full(),
        ids.iter().map(|id| ActorId::new(id).unwrap()).collect(),
        anchors,
    )
    .unwrap();
    let mut world = WorldState::new(world_config, RuntimeEpoch::new("care-run").unwrap()).unwrap();
    let mut states = ids.each_ref().map(|id| {
        store
            .initialize_life(
                id,
                WorldId::new(format!("life-{id}")).unwrap(),
                Needs::new(5000, 5000, 5000, 5000).unwrap(),
                RandomState::seeded(7),
                100,
            )
            .unwrap()
            .state()
            .clone()
    });
    let handles = ids
        .each_ref()
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            world
                .bind_actor(
                    SessionStamp::new(ActorId::new(id).unwrap(), format!("session-{i}"), 1)
                        .unwrap(),
                    Some(&states[i]),
                    0,
                )
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut choosers = states
        .each_ref()
        .map(|state| config.chooser(state).unwrap());
    let needs = [
        Need::Energy,
        Need::Nourishment,
        Need::Engagement,
        Need::Social,
    ];
    let activities = [
        Activity::Rest,
        Activity::Snack,
        Activity::Play,
        Activity::Socialize,
    ];
    let mut counts = [[0usize; 4]; 2];
    let mut interrupted = 0;
    for at in (30_000..=1_800_000).step_by(30_000) {
        world.advance(at).unwrap();
        for i in 0..2 {
            states[i] = store
                .apply_life_event(
                    &ids[i],
                    states[i].revision(),
                    30_000,
                    LifeEvent::ApplyNeedEffect {
                        effect: config.decay(30_000).unwrap(),
                    },
                    100 + at,
                )
                .unwrap()
                .snapshot
                .state()
                .clone();
        }
        let mut jobs = Vec::new();
        for i in 0..2 {
            let occupied = world
                .snapshot()
                .occupied
                .into_iter()
                .map(|slot| slot.slot_id)
                .collect::<Vec<_>>();
            let offers = needs
                .iter()
                .enumerate()
                .filter(|(n, _)| !occupied.contains(&SlotId::new(names[*n]).unwrap()))
                .map(|(n, need)| {
                    Offer::new(*need, Target::Slot(SlotId::new(names[n]).unwrap()), 0).unwrap()
                })
                .collect::<Vec<_>>();
            let Some(proposal) = choosers[i]
                .choose(
                    states[i].simulation_time().milliseconds(),
                    &states[i],
                    &offers,
                    None,
                )
                .unwrap()
            else {
                continue;
            };
            let n = needs
                .iter()
                .position(|need| *need == proposal.offer().need())
                .unwrap();
            let effect = match n {
                0 => crate::life_domain::NeedEffect::new(500, 0, 0, 0),
                1 => crate::life_domain::NeedEffect::new(0, 1500, 0, 0),
                2 => crate::life_domain::NeedEffect::new(0, 0, 500, 0),
                _ => crate::life_domain::NeedEffect::new(0, 0, 0, 1000),
            }
            .unwrap();
            states[i] = store
                .apply_life_event(
                    &ids[i],
                    states[i].revision(),
                    0,
                    LifeEvent::Plan {
                        activity: activities[n],
                        completion_effect: effect,
                    },
                    100 + at,
                )
                .unwrap()
                .snapshot
                .state()
                .clone();
            let action = states[i].current_action().unwrap().intent().id().clone();
            world
                .observe_actor(states[i].actor_id(), &states[i])
                .unwrap();
            let lease = world
                .reserve(
                    0,
                    &handles[i],
                    WorkIdentity::Life(action.clone()),
                    &SlotId::new(names[n]).unwrap(),
                    20_000,
                    at,
                )
                .unwrap();
            states[i] = store
                .apply_life_event(
                    &ids[i],
                    states[i].revision(),
                    0,
                    LifeEvent::Start {
                        action_id: action.clone(),
                    },
                    100 + at,
                )
                .unwrap()
                .snapshot
                .state()
                .clone();
            jobs.push((
                i,
                n,
                action,
                lease,
                proposal.offer().key().clone(),
                states[i].needs(),
            ));
        }
        let occupied = world.snapshot().occupied;
        assert_eq!(
            occupied.len(),
            occupied
                .iter()
                .map(|slot| &slot.slot_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
        for (i, n, action, lease, key, before) in jobs {
            let cancel = at == 30_000 && i == 0;
            let event = if cancel {
                LifeEvent::Cancel {
                    action_id: action,
                    reason: CancelReason::Operator,
                }
            } else {
                LifeEvent::Complete { action_id: action }
            };
            states[i] = store
                .apply_life_event(&ids[i], states[i].revision(), 0, event, 100 + at + 10_000)
                .unwrap()
                .snapshot
                .state()
                .clone();
            if cancel {
                assert_eq!(states[i].needs(), before);
                interrupted += 1;
            } else {
                counts[i][n] += 1;
            }
            choosers[i]
                .record_end(
                    states[i].simulation_time().milliseconds(),
                    &key,
                    if cancel {
                        End::Interrupted
                    } else {
                        End::Finished
                    },
                )
                .unwrap();
            world.release(&lease, &handles[i], at + 10_000).unwrap();
        }
        assert!(world.snapshot().occupied.is_empty());
    }
    assert_eq!(interrupted, 1);
    for i in 0..2 {
        assert!(
            counts[i].iter().all(|count| *count > 0),
            "actor {i}: {counts:?}"
        );
        assert!(states[i].needs().energy().units() >= 4000);
        assert!(states[i].needs().nourishment().units() >= 4000);
        assert!(states[i].needs().engagement().units() >= 4000);
        assert!(states[i].needs().social().units() >= 4000);
        assert_eq!(
            store.load_life(&ids[i]).unwrap().unwrap().state(),
            &states[i]
        );
    }
}
