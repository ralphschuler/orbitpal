use super::*;
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("orbitpal-social-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
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
fn bot_id(value: serde_json::Value) -> String {
    value["id"].as_str().unwrap().into()
}
fn completed_live(
    store: &Store,
    sequence: u64,
    ids: [&str; 2],
) -> (crate::social::Coordinator, SceneView, Vec<SocialEvent>) {
    use crate::{
        life_domain::{Activity, ActorId, LifeEvent, NeedEffect, Needs, RandomState, WorldId},
        social::{Coordinator, Directive, Participant},
    };
    let start = sequence * 100000;
    let mut states = ids.map(|id| {
        if store.load_life(id).unwrap().is_none() {
            store
                .initialize_life(
                    id,
                    WorldId::new("source_world").unwrap(),
                    Needs::new(5000, 5000, 5000, 5000).unwrap(),
                    RandomState::seeded(1),
                    start,
                )
                .unwrap();
        }
        store.load_life(id).unwrap().unwrap().state().clone()
    });
    let actors = states.clone().map(|state| Participant {
        actor: ActorId::new(state.actor_id().as_str()).unwrap(),
        world: state.world_id().clone(),
        session: format!("session_{}", state.actor_id().as_str()),
        generation: 1,
        available: true,
        needs: state.needs(),
        character: Character::default(),
        familiarity: 0,
        affection: 0,
    });
    let mut c = Coordinator::default();
    let view = c
        .invite(
            sequence,
            SceneKind::Play,
            actors,
            [
                crate::world_state::SlotId::new("left").unwrap(),
                crate::world_state::SlotId::new("right").unwrap(),
            ],
            start,
        )
        .unwrap();
    c.respond(&view.id, true, start).unwrap();
    c.reserved(&view.id, true, start).unwrap();
    for (activity, elapsed, now) in [
        (Activity::MoveTo, 1, start + 1),
        (Activity::Play, 3000, start + 3001),
    ] {
        for state in &mut states {
            let id = state.actor_id().as_str().to_owned();
            let plan = store
                .apply_life_event(
                    &id,
                    state.revision(),
                    0,
                    LifeEvent::Plan {
                        activity,
                        completion_effect: NeedEffect::default(),
                    },
                    now,
                )
                .unwrap();
            c.bind_action(&view.id, plan.snapshot.state(), now).unwrap();
            let action = plan
                .snapshot
                .state()
                .current_action()
                .unwrap()
                .intent()
                .id()
                .clone();
            let started = store
                .apply_life_event(
                    &id,
                    plan.snapshot.state().revision(),
                    0,
                    LifeEvent::Start {
                        action_id: action.clone(),
                    },
                    now,
                )
                .unwrap();
            let done = store
                .apply_life_event(
                    &id,
                    started.snapshot.state().revision(),
                    elapsed,
                    LifeEvent::Complete { action_id: action },
                    now,
                )
                .unwrap();
            *state = done.snapshot.state().clone();
            c.observe(&view.id, state, now).unwrap();
        }
    }
    let scene = c
        .views(ids[0])
        .into_iter()
        .find(|v| v.id == view.id)
        .unwrap();
    let events = c
        .drain()
        .into_iter()
        .filter_map(|d| match d {
            Directive::Record { event } | Directive::Terminal { event, .. } => Some(event),
            _ => None,
        })
        .collect();
    (c, scene, events)
}

#[test]
fn actual_pair_transaction_is_once_only_directed_and_bot_isolated() {
    let f = Fixture::new();
    let store = f.store();
    let blue = bot_id(store.create_bot("Blue", false).unwrap());
    let third = bot_id(store.create_bot("Third", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let sequence = store
        .social_next_scene_sequence(&ids.map(str::to_owned))
        .unwrap();
    let (_, scene, events) = completed_live(&store, sequence, ids);
    for event in &events {
        assert!(store.social_apply_event(event).unwrap());
        assert!(!store.social_apply_event(event).unwrap());
    }
    store.social_save_scene(&scene).unwrap();
    let nova = store.social_snapshot("nova").unwrap();
    assert_eq!(nova.summary.completed, 1);
    assert_eq!(nova.memories.len(), 2);
    assert_eq!(nova.development.hobbies[0].experience, 25);
    assert_eq!(nova.relationships[0].familiarity, 51);
    assert!(store.social_snapshot(&third).unwrap().memories.is_empty());
    assert!(store
        .scoped("nova")
        .unwrap()
        .social_snapshot(&blue)
        .is_err());
    drop(store);
    let reopened = f.store();
    assert_eq!(
        reopened.social_snapshot("nova").unwrap().relationships[0].familiarity,
        51
    );
}
#[test]
fn confirmed_development_milestones_persist_once_without_recreating_deleted_facts() {
    use crate::experience::{DevelopmentHobby, FactKind, FactPhase};
    let f = Fixture::new();
    let store = f.store();
    let blue = bot_id(store.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    store.social_reconcile_goal("nova", &[HobbyKind::Play]).unwrap();
    let mut last = None;
    for index in 1..=10 {
        let sequence = store.social_next_scene_sequence(&ids.map(str::to_owned)).unwrap();
        let (_, _, events) = completed_live(&store, sequence, ids);
        // Exercise the ordinary native activity credit path as well as paired
        // social credit. Both must share the exact same progress deduplication.
        if index == 10 {
            let completed = store.load_life("nova").unwrap().unwrap();
            assert!(store.social_credit_activity("nova", &completed).unwrap());
            assert!(!store.social_credit_activity("nova", &completed).unwrap());
        }
        for event in &events {
            store.social_apply_event(event).unwrap();
            assert!(!store.social_apply_event(event).unwrap());
        }
        last = events.last().cloned();
    }
    let facts = store.experience_facts("nova").unwrap();
    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|fact| fact.phase == FactPhase::Completed
        && !fact.shared && fact.actor_ids == ["nova"]
        && fact.source_event_ids.len() == 1));
    let goal = facts.iter().find(|fact| fact.kind == FactKind::GoalAchieved).unwrap();
    assert_eq!(goal.development.as_ref().unwrap().hobby, DevelopmentHobby::Play);
    assert!(goal.development.as_ref().unwrap().goal_id.is_some());
    let hobby = facts.iter().find(|fact| fact.kind == FactKind::HobbyMilestone).unwrap();
    assert_eq!(hobby.development.as_ref().unwrap().variation, 1);
    assert!(hobby.development.as_ref().unwrap().goal_id.is_none());
    let removed = hobby.id.clone();
    store.edit_experience_fact("nova", Some(&removed), None).unwrap();
    assert!(!store.social_apply_event(last.as_ref().unwrap()).unwrap());
    let completed = store.load_life("nova").unwrap().unwrap();
    assert!(!store.social_credit_activity("nova", &completed).unwrap());
    drop(store);
    let store = f.store();
    let facts = store.experience_facts("nova").unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].kind, FactKind::GoalAchieved);
    assert_eq!(store.social_snapshot("nova").unwrap().development.hobbies[0].experience, 250);
}
#[test]
fn corrupt_optional_album_preserves_confirmed_credit_and_durable_original_milestone_time() {
    let f = Fixture::new();
    let store = f.store();
    let blue = bot_id(store.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    store.social_reconcile_goal("nova", &[HobbyKind::Play]).unwrap();
    let mut completion_time = 0;
    for index in 1..=5 {
        let sequence = store.social_next_scene_sequence(&ids.map(str::to_owned)).unwrap();
        let (_, _, events) = completed_live(&store, sequence, ids);
        if index == 5 {
            store.db.lock().unwrap().execute(
                "INSERT INTO settings(key,value) VALUES('experience-facts-v1:nova','invalid JSON')", [],
            ).unwrap();
            let completion = store.load_life("nova").unwrap().unwrap();
            completion_time = completion.saved_at_ms();
            // A delayed retry retains its original wall timestamp even after a
            // genuine later simulation checkpoint on the next calendar day.
            store.apply_life_event("nova", completion.state().revision(), 30_000,
                crate::life_domain::LifeEvent::Tick {}, completion_time + 86_400_000).unwrap();
            assert!(store.social_credit_activity("nova", &completion).unwrap());
        }
        for event in &events { store.social_apply_event(event).unwrap(); }
    }
    let native = store.social_snapshot("nova").unwrap();
    assert_eq!(native.summary.completed, 5);
    assert_eq!(native.development.hobbies[0].experience, 125);
    assert_eq!(native.development.goal.unwrap().status, crate::character_development::GoalStatus::Achieved);
    assert!(store.experience_facts("nova").is_err());
    drop(store);
    let store = f.store();
    // Repair only the controlled corrupt fixture, keeping the durable queue.
    store.db.lock().unwrap().execute(
        "UPDATE settings SET value='[]' WHERE key='experience-facts-v1:nova'", [],
    ).unwrap();
    let facts = store.experience_facts("nova").unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].at_ms, completion_time);
    assert_eq!(facts[0].kind, crate::experience::FactKind::GoalAchieved);
    assert_eq!(store.experience_facts("nova").unwrap().len(), 1);
}
#[test]
fn corrupt_or_full_milestone_queue_preserves_native_success_and_exposes_recoverable_failure() {
    use crate::experience::{ActivityFact, DevelopmentHobby, DevelopmentMilestone, FactKind, FactPhase, MAX_FACTS};
    for corrupt in [false, true] {
        let f = Fixture::new();
        let store = f.store();
        let blue = bot_id(store.create_bot("Blue", false).unwrap());
        let ids = ["nova", blue.as_str()];
        store.social_reconcile_goal("nova", &[HobbyKind::Play]).unwrap();
        let raw = if corrupt { "invalid JSON".into() } else {
            serde_json::to_string(&(0..MAX_FACTS).map(|index| ActivityFact {
                id: format!("retained_{index}"), actor_ids: vec!["nova".into()],
                kind: FactKind::HobbyMilestone, phase: FactPhase::Completed,
                at_ms: 1, source_event_ids: vec!["native_source".into()], shared: false,
                development: Some(DevelopmentMilestone { hobby: DevelopmentHobby::Play, variation: 1, goal_id: None }),
            }).collect::<Vec<_>>()).unwrap()
        };
        let mut completed = None;
        for index in 1..=5 {
            let sequence = store.social_next_scene_sequence(&ids.map(str::to_owned)).unwrap();
            let (_, _, events) = completed_live(&store, sequence, ids);
            if index == 5 {
                store.db.lock().unwrap().execute(
                    "INSERT INTO settings(key,value) VALUES('experience-development-v1:nova',?1)", [&raw],
                ).unwrap();
            }
            for event in &events { store.social_apply_event(event).unwrap(); }
            completed = events.last().cloned();
        }
        assert_eq!(store.social_snapshot("nova").unwrap().summary.completed, 5);
        assert_eq!(store.social_snapshot("nova").unwrap().development.hobbies[0].experience, 125);
        let retained: String = store.db.lock().unwrap().query_row(
            "SELECT value FROM settings WHERE key='experience-development-v1:nova'", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(retained, raw);
        assert!(store.experience_facts("nova").is_err());
        // The existing optional observer sees the durable error and can pause
        // future admission without changing this actual completed scene.
        assert!(store.record_experience_social_event("nova", completed.as_ref().unwrap()).is_err());
        assert!(store.edit_experience_fact("nova", None, None).unwrap().is_empty());
        assert!(store.experience_facts("nova").unwrap().is_empty());
        assert!(!store.social_apply_event(completed.as_ref().unwrap()).unwrap());
        assert!(store.experience_facts("nova").unwrap().is_empty());
    }
}
#[test]
fn shared_facts_are_explicit_and_deletion_invalidates_inflight_context() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let third = bot_id(s.create_bot("Third", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let seq = s
        .social_next_scene_sequence(&ids.map(str::to_owned))
        .unwrap();
    let (_, _, events) = completed_live(&s, seq, ids);
    for e in &events {
        s.social_apply_event(e).unwrap();
    }
    assert!(s.social_take_context(&third, 104000).unwrap().is_none());
    let event = events.last().unwrap();
    s.social_memory_change("nova", Some(event.id()), Some(true))
        .unwrap();
    let context = s.social_take_context(&third, 104000).unwrap().unwrap();
    assert!(context.current(&s, &third).is_ok());
    s.social_memory_change("nova", Some(event.id()), None)
        .unwrap();
    assert!(context.current(&s, &third).is_err());
    assert!(!s.social_apply_event(event).unwrap());
    assert!(!s
        .social_snapshot("nova")
        .unwrap()
        .memories
        .iter()
        .any(|m| m.event_id == event.id()));
}
#[test]
fn restart_marks_pending_scene_interrupted_without_success_or_session() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let seq = s
        .social_next_scene_sequence(&ids.map(str::to_owned))
        .unwrap();
    let (_, mut scene, events) = completed_live(&s, seq, ids);
    s.social_apply_event(&events[0]).unwrap();
    scene.phase = Phase::Approach;
    s.social_save_scene(&scene).unwrap();
    drop(s);
    let s = f.store();
    s.social_recover(200000).unwrap();
    let view = s.social_snapshot("nova").unwrap();
    assert_eq!(view.summary.completed, 0);
    assert_eq!(view.summary.cancelled, 1);
    assert_eq!(view.scenes[0].reason, Some(Reason::Restart));
    assert_eq!(
        view.trace.iter().map(|v| v.phase).collect::<Vec<_>>(),
        vec![Phase::Approach, Phase::Cancelled]
    );
    assert!(!serde_json::to_string(&view)
        .unwrap()
        .contains("session_nova"));
    s.social_recover(200000).unwrap();
    assert_eq!(s.social_snapshot("nova").unwrap().summary.cancelled, 1);
}
#[test]
fn actual_sqlite_failure_rolls_back_both_relationships_and_credit() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let seq = s
        .social_next_scene_sequence(&ids.map(str::to_owned))
        .unwrap();
    let (_, _, events) = completed_live(&s, seq, ids);
    s.social_ready().unwrap();
    s.db.lock().unwrap().execute_batch("CREATE TRIGGER reject_social BEFORE INSERT ON social_memories BEGIN SELECT RAISE(ABORT,'controlled failure'); END;").unwrap();
    assert!(s.social_apply_event(events.last().unwrap()).is_err());
    assert!(s.social_snapshot("nova").unwrap().relationships.is_empty());
    assert!(s.social_snapshot(&blue).unwrap().relationships.is_empty());
    assert_eq!(
        s.social_snapshot("nova").unwrap().development.hobbies[0].experience,
        0
    );
}
#[test]
fn memory_retention_is_bounded_without_duplicate_progress_after_eviction() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let mut first = None;
    for _ in 0..70 {
        let seq = s
            .social_next_scene_sequence(&ids.map(str::to_owned))
            .unwrap();
        let (_, _, events) = completed_live(&s, seq, ids);
        for e in &events {
            s.social_apply_event(e).unwrap();
        }
        if first.is_none() {
            first = events.last().cloned();
        }
    }
    let before = s.social_snapshot("nova").unwrap();
    assert_eq!(before.memories.len(), 128);
    assert_eq!(before.summary.completed, 70);
    assert!(!s.social_apply_event(first.as_ref().unwrap()).unwrap());
    assert_eq!(s.social_snapshot("nova").unwrap().summary.completed, 70);
}

#[test]
fn sqlite_counter_boundary_refuses_negative_oversized_and_exhausted_values() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let actors = ["nova".into(), blue];
    s.social_ready().unwrap();
    for value in [-1_i64, i64::MAX] {
        s.db.lock()
            .unwrap()
            .execute(
                "UPDATE social_meta SET sequence=?1,epoch=?1 WHERE id=1",
                [value],
            )
            .unwrap();
        assert!(s.social_next_scene_sequence(&actors).is_err());
        assert!(s.social_epoch().is_err());
    }
    s.db.lock()
        .unwrap()
        .execute(
            "UPDATE social_meta SET sequence=?1,epoch=?1 WHERE id=1",
            [sql_counter(MAX_COUNTER).unwrap()],
        )
        .unwrap();
    assert_eq!(s.social_epoch().unwrap(), MAX_COUNTER);
    assert!(s.social_next_scene_sequence(&actors).is_err());
    assert!(sql_counter(MAX_COUNTER + 1).is_err());
}

#[test]
fn terminal_pair_and_scene_commit_atomically_and_accept_retained_actual_completion() {
    use crate::life_domain::{Activity, LifeEvent, NeedEffect};
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let seq = s
        .social_next_scene_sequence(&ids.map(str::to_owned))
        .unwrap();
    let (_, scene, events) = completed_live(&s, seq, ids);
    let event = events.last().unwrap();
    // A later genuine Plan does not erase the immutable actual Completed witness.
    for id in ids {
        let old = s.load_life(id).unwrap().unwrap();
        s.apply_life_event(
            id,
            old.state().revision(),
            0,
            LifeEvent::Plan {
                activity: Activity::LookAt,
                completion_effect: NeedEffect::default(),
            },
            event.at_ms() + 1,
        )
        .unwrap();
    }
    s.db.lock().unwrap().execute_batch("CREATE TRIGGER reject_terminal BEFORE INSERT ON social_scenes BEGIN SELECT RAISE(ABORT,'controlled failure'); END;").unwrap();
    assert!(s.social_commit_terminal(&scene, event).is_err());
    assert_eq!(s.social_snapshot("nova").unwrap().summary.completed, 0);
    assert!(s.social_snapshot(&blue).unwrap().relationships.is_empty());
    s.db.lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_terminal;")
        .unwrap();
    assert!(s.social_commit_terminal(&scene, event).unwrap());
    assert!(!s.social_commit_terminal(&scene, event).unwrap());
    for id in ids {
        let snap = s.social_snapshot(id).unwrap();
        assert_eq!(snap.summary.completed, 1);
        assert_eq!(snap.scenes[0].phase, Phase::Completed);
        assert_eq!(snap.trace.iter().filter(|s| s.id == scene.id).count(), 1);
    }
}
#[test]
fn dialogue_policy_and_daily_conservative_estimate_survive_reopen_and_reject_clock_rollback() {
    use crate::social_dialogue::{Policy, Quote};
    let f = Fixture::new();
    let s = f.store();
    let mut policy = Policy::default();
    policy.model_enabled = true;
    policy.estimated_micros_per_million = Some(1000);
    policy.token_limit = 100;
    policy.daily_token_limit = 100;
    policy.cost_limit_micros = 10;
    policy.daily_cost_limit_micros = 10;
    let settings = s.social_set_dialogue_policy("nova", 0, policy).unwrap();
    assert_eq!(settings.revision, 1);
    assert!(s.social_set_dialogue_policy("nova", 0, policy).is_err());
    let day = 86400000 * 100;
    assert!(s
        .social_reserve_dialogue_estimate(
            "nova",
            day,
            Quote {
                tokens: 60,
                cost_micros: Some(6)
            },
            &policy
        )
        .unwrap());
    drop(s);
    let s = f.store();
    assert_eq!(s.social_dialogue_settings("nova").unwrap().policy, policy);
    assert!(!s
        .social_reserve_dialogue_estimate(
            "nova",
            day,
            Quote {
                tokens: 60,
                cost_micros: Some(6)
            },
            &policy
        )
        .unwrap());
    assert!(s
        .social_reserve_dialogue_estimate(
            "nova",
            day - 86400000,
            Quote {
                tokens: 1,
                cost_micros: Some(1)
            },
            &policy
        )
        .is_err());
    assert!(s
        .social_reserve_dialogue_estimate(
            "nova",
            day + 86400000,
            Quote {
                tokens: 60,
                cost_micros: Some(6)
            },
            &policy
        )
        .unwrap());
    assert!(!s
        .social_reserve_dialogue_estimate(
            "nova",
            day + 86400000,
            Quote {
                tokens: 1,
                cost_micros: None
            },
            &policy
        )
        .unwrap());
    let scoped = s.scoped("nova").unwrap();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    assert!(scoped.social_dialogue_settings(&blue).is_err());
}
#[test]
fn paired_experience_fact_comes_only_from_retained_committed_social_event() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let ids = ["nova", blue.as_str()];
    let seq = s
        .social_next_scene_sequence(&ids.map(str::to_owned))
        .unwrap();
    let (_, scene, events) = completed_live(&s, seq, ids);
    let event = events.last().unwrap();
    assert!(s.record_experience_social_event("nova", event).is_err());
    s.social_commit_terminal(&scene, event).unwrap();
    for id in ids {
        assert!(s.record_experience_social_event(id, event).unwrap());
        assert!(!s.record_experience_social_event(id, event).unwrap());
    }
    let third = bot_id(s.create_bot("Third", false).unwrap());
    assert!(matches!(
        s.record_experience_social_event(&third, event),
        Ok(false) | Err(_)
    ));
    assert!(s.experience_facts(&third).unwrap().is_empty());
}
#[test]
fn shared_dialogue_day_estimate_is_aggregated_across_speakers_after_restart() {
    use crate::social_dialogue::{Policy, Quote};
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    let mut policy = Policy::default();
    policy.model_enabled = true;
    policy.estimated_micros_per_million = Some(1000);
    policy.token_limit = 100;
    policy.cost_limit_micros = 10;
    policy.daily_token_limit = 100;
    policy.daily_cost_limit_micros = 10;
    assert!(s
        .social_reserve_dialogue_estimate(
            "nova",
            86400000,
            Quote {
                tokens: 60,
                cost_micros: Some(6)
            },
            &policy
        )
        .unwrap());
    drop(s);
    let s = f.store();
    assert!(!s
        .social_reserve_dialogue_estimate(
            &blue,
            86400000,
            Quote {
                tokens: 60,
                cost_micros: Some(6)
            },
            &policy
        )
        .unwrap());
    assert!(s
        .social_reserve_dialogue_estimate(
            &blue,
            86400000,
            Quote {
                tokens: 40,
                cost_micros: Some(4)
            },
            &policy
        )
        .unwrap());
    assert!(!s
        .social_reserve_dialogue_estimate(
            "nova",
            86400000,
            Quote {
                tokens: 1,
                cost_micros: Some(1)
            },
            &policy
        )
        .unwrap());
}
#[test]
fn content_template_seed_is_creation_only_and_atomic() {
    let f = Fixture::new();
    let s = f.store();
    let blue = bot_id(s.create_bot("Blue", false).unwrap());
    s.social_ready().unwrap();
    {
        let mut db = s.db.lock().unwrap();
        let tx = db.transaction().unwrap();
        seed_template_character(&tx, &blue, 200, 800).unwrap();
        tx.commit().unwrap();
    }
    let profile = s.social_snapshot(&blue).unwrap();
    assert_eq!(profile.development.traits.sociability, 200);
    assert_eq!(profile.development.traits.rest_preference, 800);
    assert_eq!(profile.revision, 0);
    assert!(profile.memories.is_empty());
    assert!(profile
        .development
        .hobbies
        .iter()
        .all(|h| h.experience == 0));
    {
        let mut db = s.db.lock().unwrap();
        let tx = db.transaction().unwrap();
        assert!(seed_template_character(&tx, &blue, 999, 999).is_err());
        assert!(seed_template_character(&tx, "missing", 200, 800).is_err());
        assert!(seed_template_character(&tx, "nova", 1001, 0).is_err());
    }
    assert_eq!(
        s.social_snapshot(&blue)
            .unwrap()
            .development
            .traits
            .sociability,
        200
    );
}
