use super::*;
use crate::life_domain::{reduce, Activity, LifeEvent, NeedEffect, RandomState};
pub(crate) fn states(ids: [&str; 2]) -> [LifeState; 2] {
    ids.map(|id| {
        LifeState::new(
            ActorId::new(id).unwrap(),
            WorldId::new("source_world").unwrap(),
            Needs::new(5000, 5000, 5000, 5000).unwrap(),
        )
    })
}
fn actor(state: &LifeState) -> Participant {
    Participant {
        actor: state.actor_id().clone(),
        world: state.world_id().clone(),
        session: format!("session_{}", state.actor_id().as_str()),
        generation: 1,
        available: true,
        needs: state.needs(),
        character: Character::default(),
        familiarity: 0,
        affection: 0,
    }
}
fn plan(state: &LifeState, activity: Activity) -> LifeState {
    reduce(
        state,
        0,
        RandomState::seeded(1),
        LifeEvent::Plan {
            activity,
            completion_effect: NeedEffect::default(),
        },
    )
    .unwrap()
    .state
}
fn finish(state: &LifeState, ms: i64) -> LifeState {
    let id = state.current_action().unwrap().intent().id().clone();
    let started = reduce(
        state,
        0,
        RandomState::seeded(1),
        LifeEvent::Start {
            action_id: id.clone(),
        },
    )
    .unwrap();
    reduce(
        &started.state,
        ms,
        started.random,
        LifeEvent::Complete { action_id: id },
    )
    .unwrap()
    .state
}
pub(crate) fn completed(
    sequence: u64,
    ids: [&str; 2],
    mut states: [LifeState; 2],
) -> (Coordinator, SceneView, Vec<SocialEvent>, [LifeState; 2]) {
    let mut c = Coordinator::default();
    let start = sequence * 100000;
    let view = c
        .invite(
            sequence,
            SceneKind::Play,
            states.clone().map(|s| actor(&s)),
            [SlotId::new("left").unwrap(), SlotId::new("right").unwrap()],
            start,
        )
        .unwrap();
    c.respond(&view.id, true, start).unwrap();
    c.reserved(&view.id, true, start).unwrap();
    for state in &mut states {
        *state = plan(state, Activity::MoveTo);
        c.bind_action(&view.id, state, start).unwrap();
        *state = finish(state, 1);
        c.observe(&view.id, state, start + 1).unwrap();
    }
    for state in &mut states {
        *state = plan(state, Activity::Play);
        c.bind_action(&view.id, state, start + 1).unwrap();
        *state = finish(state, 3000);
        c.observe(&view.id, state, start + 3001).unwrap();
    }
    let terminal = c
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
    (c, terminal, events, states)
}
#[test]
fn complete_requires_both_real_bound_arrivals_and_both_real_completions() {
    let (mut c, view, events, states) = completed(1, ["nova", "blue"], states(["nova", "blue"]));
    assert_eq!(view.phase, Phase::Completed);
    let success = events
        .iter()
        .find(|e| e.outcome() == Outcome::Completed)
        .unwrap();
    assert_eq!(
        success.actions().unwrap()[0],
        *states[0].current_action().unwrap().intent().id()
    );
    assert_eq!(success.source_events().unwrap().len(), 2);
    c.observe(&view.id, &states[0], 103002).unwrap();
    assert!(c.drain().is_empty());
    assert_eq!(
        events
            .iter()
            .filter(|e| e.outcome() == Outcome::Completed)
            .count(),
        1
    );
}
#[test]
fn actor_exclusivity_survives_terminal_until_actual_cleanup_ack() {
    let (mut c, view, _, _) = completed(1, ["nova", "blue"], states(["nova", "blue"]));
    let a = states(["nova", "third"]).map(|s| actor(&s));
    let slots = [SlotId::new("l").unwrap(), SlotId::new("r").unwrap()];
    assert!(c
        .invite(2, SceneKind::Play, a.clone(), slots.clone(), 104000)
        .is_err());
    c.released(&view.id).unwrap();
    assert!(c.invite(2, SceneKind::Play, a, slots, 104000).is_ok());
}
#[test]
fn decline_is_respected_and_negative_affection_does_not_wrap() {
    let mut c = Coordinator::default();
    let mut actors = states(["nova", "blue"]).map(|s| actor(&s));
    actors[1].affection = -10000;
    let view = c
        .invite(
            1,
            SceneKind::Greeting,
            actors.clone(),
            [SlotId::new("a").unwrap(), SlotId::new("b").unwrap()],
            0,
        )
        .unwrap();
    assert!(!c.auto_respond(&view.id, 1).unwrap());
    assert_eq!(c.views("nova")[0].phase, Phase::Declined);
    c.released(&view.id).unwrap();
    assert!(c
        .invite(
            2,
            SceneKind::Greeting,
            actors.clone(),
            [SlotId::new("a").unwrap(), SlotId::new("b").unwrap()],
            2
        )
        .is_err());
    assert!(c
        .invite(
            3,
            SceneKind::Greeting,
            actors,
            [SlotId::new("a").unwrap(), SlotId::new("b").unwrap()],
            60002
        )
        .is_ok());
}
#[test]
fn timeout_missing_actor_and_manual_abort_release_only_after_host_cleanup() {
    for reason in [Reason::Timeout, Reason::Unavailable, Reason::Operator] {
        let mut c = Coordinator::default();
        let actors = states(["nova", "blue"]).map(|s| actor(&s));
        let view = c
            .invite(
                1,
                SceneKind::Play,
                actors.clone(),
                [SlotId::new("a").unwrap(), SlotId::new("b").unwrap()],
                0,
            )
            .unwrap();
        match reason {
            Reason::Timeout => c.reconcile(&actors, 30000).unwrap(),
            Reason::Unavailable => c.reconcile(&actors[..1], 1).unwrap(),
            _ => c.cancel(&view.id, reason, 1).unwrap(),
        };
        let output = c.drain();
        assert_eq!(
            output
                .iter()
                .filter(|d| matches!(d, Directive::Terminal { .. }))
                .count(),
            1
        );
        c.cancel(&view.id, reason, 30001).unwrap();
        assert!(c.drain().is_empty());
        c.released(&view.id).unwrap();
    }
}
#[test]
fn unbound_old_completion_is_not_an_arrival() {
    let mut c = Coordinator::default();
    let states = states(["nova", "blue"]);
    let view = c
        .invite(
            1,
            SceneKind::Play,
            states.clone().map(|s| actor(&s)),
            [SlotId::new("a").unwrap(), SlotId::new("b").unwrap()],
            0,
        )
        .unwrap();
    c.respond(&view.id, true, 0).unwrap();
    c.reserved(&view.id, true, 0).unwrap();
    let unrelated = finish(&plan(&states[0], Activity::MoveTo), 1);
    c.observe(&view.id, &unrelated, 1).unwrap();
    assert_eq!(c.views("nova")[0].phase, Phase::Approach);
    assert!(!c
        .drain()
        .iter()
        .any(|d| matches!(d, Directive::Perform { .. })));
}
#[test]
fn simultaneous_invitations_have_a_deterministic_exclusive_winner() {
    fn run(reverse: bool) -> Vec<String> {
        let mut c = Coordinator::default();
        let mut offers = vec![
            (
                1,
                SceneKind::Play,
                states(["a", "b"]).map(|s| actor(&s)),
                [SlotId::new("l1").unwrap(), SlotId::new("r1").unwrap()],
            ),
            (
                2,
                SceneKind::Play,
                states(["b", "c"]).map(|s| actor(&s)),
                [SlotId::new("l2").unwrap(), SlotId::new("r2").unwrap()],
            ),
        ];
        if reverse {
            offers.reverse();
        }
        c.invite_batch(offers, 0)
            .into_iter()
            .filter_map(Result::ok)
            .map(|v| v.inviter_id)
            .collect()
    }
    assert_eq!(run(false), vec!["a"]);
    assert_eq!(run(false), run(true));
}
