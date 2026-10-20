use super::*;
use serde_json::{json, Value};

fn actor(name: &str) -> ActorId {
    ActorId::new(name).unwrap()
}
fn world() -> WorldId {
    WorldId::new("desktop-main").unwrap()
}
fn initial() -> LifeState {
    LifeState::new(actor("nova"), world(), Needs::new(5_000, 5_000, 5_000, 5_000).unwrap())
}
fn plan(state: &LifeState, random: RandomState, effect: NeedEffect) -> Transition {
    reduce(state, 0, random, LifeEvent::Plan { activity: Activity::Rest, completion_effect: effect }).unwrap()
}
fn id(state: &LifeState) -> ActionId {
    state.current_action().unwrap().intent().id().clone()
}
fn complete_one(state: &LifeState, random: RandomState, effect: NeedEffect) -> Transition {
    let planned = plan(state, random, effect);
    let started =
        reduce(&planned.state, 25, planned.random, LifeEvent::Start { action_id: id(&planned.state) }).unwrap();
    reduce(&started.state, 75, started.random, LifeEvent::Complete { action_id: id(&started.state) }).unwrap()
}
fn state_json(state: &LifeState) -> Value {
    serde_json::to_value(state).unwrap()
}
fn rejects(value: Value) {
    assert!(serde_json::from_value::<LifeState>(value).is_err());
}

#[test]
fn identical_inputs_replay_identical_states_intents_and_randomness() {
    fn replay() -> Vec<Transition> {
        let mut turns = vec![plan(&initial(), RandomState::seeded(41), NeedEffect::new(1_000, 0, 0, 0).unwrap())];
        let p = turns.last().unwrap();
        turns.push(reduce(&p.state, 10, p.random, LifeEvent::Start { action_id: id(&p.state) }).unwrap());
        let p = turns.last().unwrap();
        turns.push(reduce(&p.state, 5_000, p.random, LifeEvent::Tick {}).unwrap());
        let p = turns.last().unwrap();
        turns.push(reduce(&p.state, 50, p.random, LifeEvent::Complete { action_id: id(&p.state) }).unwrap());
        let p = turns.last().unwrap();
        turns.push(plan(&p.state, p.random, NeedEffect::default()));
        turns
    }
    let a = replay();
    let b = replay();
    assert_eq!(a, b);
    assert_eq!(a[0].intents.len(), 1);
    assert!(a[1..4].iter().all(|t| t.intents.is_empty()));
    assert_eq!(a[4].intents[0].id().ordinal(), 2);
    for turn in a {
        assert_eq!(serde_json::from_value::<LifeState>(state_json(&turn.state)).unwrap(), turn.state);
    }
}

#[test]
fn controlled_random_stream_has_a_fixed_vector_and_only_planning_consumes_it() {
    let seed = RandomState::seeded(0);
    let (next, value) = seed.draw();
    assert_eq!(next.state(), 0x9e3779b97f4a7c15);
    assert_eq!(value, 0xe220a8397b1dcdaf);
    let a = plan(&initial(), seed, NeedEffect::default());
    let b = plan(&initial(), RandomState::seeded(1), NeedEffect::default());
    assert_eq!(a.intents[0].variation(), 0xcdaf);
    assert_eq!(b.intents[0].variation(), 0x5cc1);
    assert_ne!(a.intents, b.intents);
    assert_eq!(a.intents[0].id(), b.intents[0].id()); // IDs are monotonic, not random authority tokens.
    let tick = reduce(&initial(), 1, seed, LifeEvent::Tick {}).unwrap();
    assert_eq!(tick.random, seed);
}

#[test]
fn plan_start_complete_are_distinct_and_effect_is_confirmed_exactly_once() {
    let original = initial();
    let planned = plan(&original, RandomState::seeded(1), NeedEffect::new(2_000, -1_000, 500, 100).unwrap());
    assert_eq!(original.needs(), planned.state.needs());
    assert_eq!(planned.state.current_action().unwrap().phase(), ActionPhase::Planned);
    assert_eq!(planned.state.revision(), 1);
    let started =
        reduce(&planned.state, 30, planned.random, LifeEvent::Start { action_id: id(&planned.state) }).unwrap();
    assert_eq!(started.state.needs(), original.needs());
    assert_eq!(started.state.current_action().unwrap().phase(), ActionPhase::Started);
    let finished =
        reduce(&started.state, 70, started.random, LifeEvent::Complete { action_id: id(&started.state) }).unwrap();
    assert_eq!(finished.state.current_action().unwrap().phase(), ActionPhase::Completed);
    assert_eq!(finished.state.current_action().unwrap().started_at().unwrap().milliseconds(), 30);
    assert_eq!(finished.state.current_action().unwrap().finished_at().unwrap().milliseconds(), 100);
    assert_eq!(finished.state.needs(), Needs::new(7_000, 4_000, 5_500, 5_100).unwrap());
    assert_eq!(finished.state.recent_events().map(|r| r.id().sequence()).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert!(matches!(finished.state.recent_events().last().unwrap().event(), CommittedEvent::Completed { .. }));
    let duplicate =
        reduce(&finished.state, i64::MAX, finished.random, LifeEvent::Complete { action_id: id(&finished.state) })
            .unwrap();
    assert_eq!(duplicate.outcome, Outcome::Duplicate);
    assert_eq!(duplicate.state, finished.state);
    assert_eq!(duplicate.random, finished.random);
    assert!(duplicate.intents.is_empty());
    assert_eq!(duplicate.applied_elapsed_ms, 0);
}

#[test]
fn cancelled_actions_never_receive_completion_credit_or_implicit_followup() {
    for start_first in [false, true] {
        let mut turn = plan(&initial(), RandomState::seeded(9), NeedEffect::new(5_000, 5_000, 5_000, 5_000).unwrap());
        if start_first {
            turn = reduce(&turn.state, 20, turn.random, LifeEvent::Start { action_id: id(&turn.state) }).unwrap();
        }
        let cancel = reduce(
            &turn.state,
            1,
            turn.random,
            LifeEvent::Cancel { action_id: id(&turn.state), reason: CancelReason::Operator },
        )
        .unwrap();
        assert_eq!(cancel.state.needs(), initial().needs());
        assert_eq!(cancel.state.current_action().unwrap().phase(), ActionPhase::Cancelled);
        assert_eq!(cancel.state.current_action().unwrap().cancellation(), Some(CancelReason::Operator));
        assert!(cancel.intents.is_empty());
        let late =
            reduce(&cancel.state, 10_000, cancel.random, LifeEvent::Complete { action_id: id(&cancel.state) }).unwrap();
        assert_eq!(late.outcome, Outcome::Stale);
        assert_eq!(late.state, cancel.state);
        let again = reduce(
            &cancel.state,
            10,
            cancel.random,
            LifeEvent::Cancel { action_id: id(&cancel.state), reason: CancelReason::Timeout },
        )
        .unwrap();
        assert_eq!(again.outcome, Outcome::Duplicate);
        assert_eq!(again.state, cancel.state);
    }
}

#[test]
fn invalid_transitions_wrong_actor_and_future_ids_are_atomic() {
    let seed = RandomState::seeded(13);
    let p = plan(&initial(), seed, NeedEffect::default());
    assert_eq!(
        reduce(&p.state, 1, p.random, LifeEvent::Complete { action_id: id(&p.state) }),
        Err(LifeError::InvalidTransition)
    );
    assert_eq!(
        reduce(
            &p.state,
            1,
            p.random,
            LifeEvent::Plan { activity: Activity::Play, completion_effect: NeedEffect::default() }
        ),
        Err(LifeError::ActionBusy)
    );
    assert_eq!(
        reduce(
            &p.state,
            1,
            p.random,
            LifeEvent::Start { action_id: ActionId::new(actor("james"), world(), 1).unwrap() }
        ),
        Err(LifeError::WrongActor)
    );
    assert_eq!(
        reduce(
            &p.state,
            1,
            p.random,
            LifeEvent::Start { action_id: ActionId::new(actor("nova"), world(), 2).unwrap() }
        ),
        Err(LifeError::UnknownAction)
    );
    assert_eq!(
        reduce(
            &initial(),
            1,
            seed,
            LifeEvent::Complete { action_id: ActionId::new(actor("nova"), world(), 1).unwrap() }
        ),
        Err(LifeError::UnknownAction)
    );
    let started = reduce(&p.state, 0, p.random, LifeEvent::Start { action_id: id(&p.state) }).unwrap();
    let again =
        reduce(&started.state, 999, started.random, LifeEvent::Start { action_id: id(&started.state) }).unwrap();
    assert_eq!(again.outcome, Outcome::Duplicate);
    assert_eq!(again.state, started.state);
    assert_eq!(p.state.revision(), 1);
    assert_eq!(p.state.simulation_time().milliseconds(), 0);
    assert_eq!(p.random.state(), seed.draw().0.state());
}

#[test]
fn history_is_bounded_and_old_completion_never_becomes_new_credit() {
    let mut turn = complete_one(&initial(), RandomState::seeded(2), NeedEffect::new(1, 0, 0, 0).unwrap());
    let first = id(&turn.state);
    for _ in 0..(MAX_RECENT_EVENTS * 3) {
        turn = reduce(&turn.state, 1, turn.random, LifeEvent::Tick {}).unwrap();
    }
    assert_eq!(turn.state.recent_events().len(), MAX_RECENT_EVENTS);
    assert!(turn.state.recent_events().all(|e| matches!(e.event(), CommittedEvent::Advanced { .. })));
    let duplicate = reduce(&turn.state, 0, turn.random, LifeEvent::Complete { action_id: first.clone() }).unwrap();
    assert_eq!(duplicate.outcome, Outcome::Duplicate);
    assert_eq!(duplicate.state, turn.state);
    for _ in 0..(MAX_RECENT_EVENTS * 2) {
        turn = complete_one(&turn.state, turn.random, NeedEffect::new(1, 0, 0, 0).unwrap());
    }
    let before = turn.state.needs();
    let stale = reduce(&turn.state, 0, turn.random, LifeEvent::Complete { action_id: first }).unwrap();
    assert_eq!(stale.outcome, Outcome::Stale);
    assert_eq!(stale.state, turn.state);
    assert_eq!(stale.state.needs(), before);
    assert_eq!(stale.state.recent_events().len(), MAX_RECENT_EVENTS);
    assert_eq!(stale.state.recent_events().last().unwrap().id().sequence(), stale.state.revision());
    assert_eq!(serde_json::from_value::<LifeState>(state_json(&stale.state)).unwrap(), stale.state);
}

#[test]
fn one_step_time_is_capped_and_backwards_time_cannot_rewind_or_create_actions() {
    let seed = RandomState::seeded(7);
    for elapsed in [i64::MIN, -1, 0] {
        let turn = reduce(&initial(), elapsed, seed, LifeEvent::Tick {}).unwrap();
        assert_eq!(turn.state, initial());
        assert_eq!(turn.outcome, Outcome::NoChange);
        assert_eq!(turn.elapsed_was_limited, elapsed < 0);
    }
    let turn = reduce(&initial(), i64::MAX, seed, LifeEvent::Tick {}).unwrap();
    assert_eq!(turn.state.simulation_time().milliseconds(), MAX_ELAPSED_MS);
    assert_eq!(turn.applied_elapsed_ms, MAX_ELAPSED_MS);
    assert!(turn.elapsed_was_limited);
    assert_eq!(turn.state.needs(), initial().needs()); // Domain adds no punishment/decay policy.
    assert!(turn.intents.is_empty());
    assert_eq!(turn.state.recent_events().len(), 1);
    assert_eq!(turn.random, seed);
    let next = reduce(&turn.state, -100, turn.random, LifeEvent::Tick {}).unwrap();
    assert_eq!(next.state, turn.state);
}

#[test]
fn needs_are_fixed_point_clamped_and_nonfinite_inputs_fail_at_the_numeric_boundary() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(NeedLevel::from_normalized(value), Err(LifeError::InvalidNumber));
    }
    for value in [-1.0, 1.0001, f64::MAX] {
        assert_eq!(NeedLevel::from_normalized(value), Err(LifeError::OutOfRange));
    }
    assert_eq!(NeedLevel::from_normalized(0.5).unwrap().units(), 5_000);
    assert_eq!(NeedLevel::new(10_001), Err(LifeError::OutOfRange));
    assert_eq!(Needs::new(0, 0, 0, 10_001), Err(LifeError::OutOfRange));
    assert_eq!(NeedEffect::new(i32::MIN, 0, 0, 0), Err(LifeError::OutOfRange));
    assert_eq!(NeedEffect::new(0, i32::MAX, 0, 0), Err(LifeError::OutOfRange));
    let state = initial();
    let turn = reduce(
        &state,
        0,
        RandomState::seeded(0),
        LifeEvent::ApplyNeedEffect { effect: NeedEffect::new(10_000, -10_000, 10_000, -10_000).unwrap() },
    )
    .unwrap();
    assert_eq!(turn.state.needs(), Needs::new(10_000, 0, 10_000, 0).unwrap());
    for invalid in ["-1", "10001", "0.5", "NaN", "Infinity", "null", "1e100"] {
        assert!(serde_json::from_str::<NeedLevel>(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn validated_identifiers_have_distinct_types_and_reject_paths_control_and_unicode() {
    for invalid in ["", " ", "nova/b", "../nova", "nova:b", "nova.b", "nova\n", "nóva", "nova b"] {
        assert!(ActorId::new(invalid).is_err());
        assert!(WorldId::new(invalid).is_err());
        assert!(serde_json::from_value::<ActorId>(json!(invalid)).is_err());
        assert!(serde_json::from_value::<WorldId>(json!(invalid)).is_err());
    }
    assert!(ActorId::new("A".repeat(MAX_ID_BYTES)).is_ok());
    assert!(ActorId::new("A".repeat(MAX_ID_BYTES + 1)).is_err());
    for valid in ["_nova", "-nova", "a5340831-8231-4c05-b909-529289e3b004"] {
        assert!(ActorId::new(valid).is_ok());
        assert_eq!(serde_json::from_value::<ActorId>(json!(valid)).unwrap().as_str(), valid);
    }
    for ordinal in [0, MAX_COUNTER + 1, u64::MAX] {
        assert!(ActionId::new(actor("nova"), world(), ordinal).is_err());
        assert!(serde_json::from_value::<ActionId>(
            json!({"actorId":"nova","worldId":"desktop-main","ordinal":ordinal})
        )
        .is_err());
    }
}

#[test]
fn deserialize_rejects_schema_ranges_unknown_fields_and_inconsistent_references() {
    let state = complete_one(&initial(), RandomState::seeded(3), NeedEffect::new(10, 0, 0, 0).unwrap()).state;
    let base = state_json(&state);
    for schema in [0, 2, u16::MAX] {
        let mut bad = base.clone();
        bad["schemaVersion"] = json!(schema);
        rejects(bad);
    }
    for value in [json!(-1), json!(10_001), json!(0.1), json!("NaN"), json!(null)] {
        let mut bad = base.clone();
        bad["needs"]["energy"] = value;
        rejects(bad);
    }
    for field in ["session", "permissions", "credentials", "screenImage", "prompt", "persona", "privateConversation"] {
        let mut bad = base.clone();
        bad[field] = json!("synthetic-private-marker");
        rejects(bad);
    }
    for (path, value) in [
        (vec!["actorId"], json!("../nova")),
        (vec!["worldId"], json!("different-world")),
        (vec!["simulationTime"], json!(MAX_SIMULATION_TIME_MS + 1)),
        (vec!["nextActionOrdinal"], json!(1)),
        (vec!["currentAction", "intent", "id", "actorId"], json!("james")),
        (vec!["currentAction", "intent", "id", "ordinal"], json!(5)),
        (vec!["currentAction", "intent", "worldId"], json!("other")),
        (vec!["currentAction", "phase"], json!("planned")),
        (vec!["currentAction", "startedAt"], json!(null)),
        (vec!["currentAction", "finishedAt"], json!(99_999)),
        (vec!["currentAction", "intent", "completionEffect", "energy"], json!(10_001)),
    ] {
        let mut bad = base.clone();
        let mut cursor = &mut bad;
        for name in path {
            cursor = &mut cursor[name];
        }
        *cursor = value;
        rejects(bad);
    }
    let mut bad = base;
    bad["recentEvents"][1]["id"]["sequence"] = json!(1);
    rejects(bad);
    assert!(serde_json::from_value::<LifeEvent>(json!({"type":"tick","permission":true})).is_err());
    assert!(serde_json::from_value::<LifeEvent>(json!({"type":"plan","activity":"shell","completion_effect":{"energy":0,"nourishment":0,"engagement":0,"social":0}})).is_err());
    assert!(serde_json::from_value::<LifeEvent>(json!({"type":"plan","activity":"rest","completion_effect":{"energy":10001,"nourishment":0,"engagement":0,"social":0}})).is_err());
}

#[test]
fn malformed_history_and_action_phase_payloads_are_not_accepted() {
    let state = complete_one(&initial(), RandomState::seeded(3), NeedEffect::default()).state;
    for field in ["id", "at", "event"] {
        let mut bad = state_json(&state);
        bad["recentEvents"][0].as_object_mut().unwrap().remove(field);
        rejects(bad);
    }
    let mut bad = state_json(&state);
    bad["recentEvents"][0]["at"] = json!(state.simulation_time().milliseconds() + 1);
    rejects(bad);
    let mut bad = state_json(&state);
    bad["recentEvents"][0]["id"]["actorId"] = json!("james");
    rejects(bad);
    let mut bad = state_json(&state);
    bad["recentEvents"][2]["event"]["action_id"]["ordinal"] = json!(99);
    rejects(bad);
    let mut bad = state_json(&state);
    bad["currentAction"]["cancellation"] = json!("operator");
    rejects(bad);
    let mut bad = state_json(&state);
    bad["currentAction"]["startedAt"] = json!(80);
    bad["currentAction"]["finishedAt"] = json!(70);
    rejects(bad);
    let mut bad = state_json(&state);
    bad["revision"] = json!(MAX_COUNTER + 1);
    rejects(bad);
    let mut bad = state_json(&state);
    bad["recentEvents"] = json!([]);
    rejects(bad);
    let mut bad = state_json(&initial());
    bad["nextActionOrdinal"] = json!(2);
    rejects(bad);
    let record = state_json(&state)["recentEvents"][0].clone();
    let mut bad = state_json(&state);
    bad["recentEvents"] = json!(vec![record; MAX_RECENT_EVENTS + 1]);
    rejects(bad);
}

#[test]
fn mutation_boundary_revalidates_even_internal_malformed_states_and_effects() {
    // Private fields prevent these constructions outside this module. The
    // reducer still validates to guard future internal constructors/migrations.
    let seed = RandomState::seeded(0);
    let mut bad = initial();
    bad.needs.energy = NeedLevel(10_001);
    assert_eq!(reduce(&bad, 0, seed, LifeEvent::Tick {}), Err(LifeError::OutOfRange));
    let mut bad = initial();
    bad.schema_version = 2;
    assert_eq!(reduce(&bad, 0, seed, LifeEvent::Tick {}), Err(LifeError::UnsupportedSchema));
    let mut bad = initial();
    bad.actor_id = ActorId("../nova".into());
    assert_eq!(reduce(&bad, 0, seed, LifeEvent::Tick {}), Err(LifeError::InvalidId));
    let bad_effect = NeedEffect { energy: 10_001, ..NeedEffect::default() };
    assert_eq!(
        reduce(&initial(), 0, seed, LifeEvent::ApplyNeedEffect { effect: bad_effect }),
        Err(LifeError::OutOfRange)
    );
    assert_eq!(
        reduce(&initial(), 0, seed, LifeEvent::Plan { activity: Activity::Rest, completion_effect: bad_effect }),
        Err(LifeError::OutOfRange)
    );
}

#[test]
fn exhausted_clock_and_counters_fail_without_wrap_or_partial_credit() {
    let seed = RandomState::seeded(0);
    let mut clock = reduce(&initial(), 1, seed, LifeEvent::Tick {}).unwrap().state;
    clock.simulation_time = SimulationTime(MAX_SIMULATION_TIME_MS);
    clock.revision = MAX_COUNTER - 1;
    clock.recent_events.0.back_mut().unwrap().id.sequence = Sequence(MAX_COUNTER - 1);
    clock.recent_events.0.back_mut().unwrap().at = clock.simulation_time;
    clock.validate().unwrap();
    assert_eq!(reduce(&clock, 1, seed, LifeEvent::Tick {}), Err(LifeError::ClockExhausted));
    assert_eq!(clock.needs(), initial().needs());
    let mut sequence = plan(&initial(), seed, NeedEffect::default()).state;
    sequence.revision = MAX_COUNTER - 1;
    sequence.next_action_ordinal = Sequence(MAX_COUNTER);
    sequence.resolved_action_ordinal = MAX_COUNTER - 2;
    sequence.current_action.as_mut().unwrap().intent.id.ordinal = Sequence(MAX_COUNTER - 1);
    let record = sequence.recent_events.0.back_mut().unwrap();
    record.id.sequence = Sequence(MAX_COUNTER - 1);
    if let CommittedEvent::Planned { intent } = &mut record.event {
        intent.id.ordinal = Sequence(MAX_COUNTER - 1);
    }
    // Cancel the last valid action, then planning must fail rather than reuse an ID.
    sequence =
        reduce(&sequence, 0, seed, LifeEvent::Cancel { action_id: id(&sequence), reason: CancelReason::Operator })
            .unwrap()
            .state;
    assert_eq!(sequence.revision(), MAX_COUNTER);
    assert_eq!(
        reduce(
            &sequence,
            0,
            seed,
            LifeEvent::Plan { activity: Activity::Play, completion_effect: NeedEffect::default() }
        ),
        Err(LifeError::CounterExhausted)
    );
    assert_eq!(reduce(&sequence, 1, seed, LifeEvent::Tick {}), Err(LifeError::CounterExhausted));
    assert_eq!(
        reduce(&sequence, 1, seed, LifeEvent::Cancel { action_id: id(&sequence), reason: CancelReason::Operator })
            .unwrap()
            .outcome,
        Outcome::Duplicate
    );
}

#[test]
fn public_projection_has_only_whitelisted_observations_and_cannot_be_restored_as_state() {
    let planned = plan(&initial(), RandomState::seeded(0), NeedEffect::new(1_000, 0, 0, 0).unwrap());
    let public = planned.state.public_projection();
    assert_eq!(public.actor_id(), &actor("nova"));
    assert_eq!(public.world_id(), &world());
    assert_eq!(public.activity().unwrap().phase(), ActionPhase::Planned);
    let json = serde_json::to_value(public).unwrap();
    let object = json.as_object().unwrap();
    assert_eq!(
        object.keys().cloned().collect::<std::collections::BTreeSet<_>>(),
        ["schemaVersion", "actorId", "worldId", "revision", "simulationTime", "activity"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert_eq!(
        object["activity"].as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<_>>(),
        ["id", "activity", "phase"].into_iter().map(str::to_owned).collect()
    );
    for key in [
        "needs",
        "completionEffect",
        "variation",
        "recentEvents",
        "session",
        "permissions",
        "credentials",
        "screenImage",
        "prompt",
    ] {
        assert!(!serde_json::to_string(&json).unwrap().contains(key));
    }
    assert!(serde_json::from_value::<LifeState>(json).is_err());
}

#[test]
fn deterministic_long_trace_keeps_bounds_ordering_and_actor_isolation() {
    let mut a = initial();
    let mut b = LifeState::new(actor("james"), world(), initial().needs());
    let mut rng = RandomState::seeded(0x00f00d);
    let b_original = b.clone();
    for step in 0..500 {
        let event = match a.current_action().map(|v| v.phase()) {
            None | Some(ActionPhase::Completed | ActionPhase::Cancelled) => LifeEvent::Plan {
                activity: if step % 2 == 0 { Activity::Play } else { Activity::Rest },
                completion_effect: NeedEffect::new(99, -111, 222, -333).unwrap(),
            },
            Some(ActionPhase::Planned) => LifeEvent::Start { action_id: id(&a) },
            Some(ActionPhase::Started) if step % 7 == 0 => {
                LifeEvent::Cancel { action_id: id(&a), reason: CancelReason::Interrupted }
            }
            Some(ActionPhase::Started) => LifeEvent::Complete { action_id: id(&a) },
        };
        let next = reduce(&a, if step % 11 == 0 { i64::MAX } else { 20 }, rng, event).unwrap();
        assert!(next.applied_elapsed_ms <= MAX_ELAPSED_MS);
        assert!(next.intents.len() <= 1);
        assert_eq!(next.state.revision(), a.revision() + 1);
        next.state.validate().unwrap();
        let ids: Vec<_> = next.state.recent_events().map(|r| r.id().sequence()).collect();
        assert!(ids.windows(2).all(|w| w[1] == w[0] + 1));
        for level in [
            next.state.needs().energy(),
            next.state.needs().nourishment(),
            next.state.needs().engagement(),
            next.state.needs().social(),
        ] {
            assert!(level.units() <= NEED_SCALE);
        }
        a = serde_json::from_value(state_json(&next.state)).unwrap();
        rng = next.random;
    }
    assert_eq!(b, b_original);
    let a_id = id(&a);
    assert_eq!(reduce(&b, 0, rng, LifeEvent::Complete { action_id: a_id }), Err(LifeError::WrongActor));
    b = complete_one(&b, rng, NeedEffect::default()).state;
    assert_eq!(b.current_action().unwrap().intent().id().ordinal(), 1);
    assert_ne!(a.current_action().unwrap().intent().id(), b.current_action().unwrap().intent().id());
}

#[test]
fn persisted_terminal_watermark_and_retained_causality_prevent_completion_recredit() {
    let finished = complete_one(&initial(), RandomState::seeded(0), NeedEffect::new(1_000, 0, 0, 0).unwrap());
    let mut rollback = state_json(&finished.state);
    rollback["currentAction"]["phase"] = json!("started");
    rollback["currentAction"]["finishedAt"] = Value::Null;
    rejects(rollback.clone()); // Independent terminal watermark remains resolved.
    rollback["resolvedActionOrdinal"] = json!(0);
    rejects(rollback); // Retained completion still contradicts the rolled-back phase.
    let mut effect_swap = state_json(&finished.state);
    effect_swap["currentAction"]["intent"]["completionEffect"]["energy"] = json!(9_000);
    rejects(effect_swap); // Same intent must match retained plan.
    let mut reordering = state_json(&finished.state);
    reordering["recentEvents"][1]["event"] = json!({"type":"completed","action_id":id(&finished.state)});
    reordering["recentEvents"][2]["event"] = json!({"type":"started","action_id":id(&finished.state)});
    rejects(reordering);
    let mut duplicate = state_json(&finished.state);
    duplicate["recentEvents"][1]["event"] = duplicate["recentEvents"][0]["event"].clone();
    rejects(duplicate);
    let mut old = finished;
    for _ in 0..MAX_RECENT_EVENTS {
        old = reduce(&old.state, 1, old.random, LifeEvent::Tick {}).unwrap();
    }
    assert!(old.state.recent_events().all(|r| matches!(r.event(), CommittedEvent::Advanced { .. })));
    let mut rollback = state_json(&old.state);
    rollback["currentAction"]["phase"] = json!("started");
    rollback["currentAction"]["finishedAt"] = Value::Null;
    rejects(rollback); // Terminal watermark remains after the completion record is evicted.
    let decoded: LifeState = serde_json::from_value(state_json(&old.state)).unwrap();
    let duplicate = reduce(&decoded, 0, old.random, LifeEvent::Complete { action_id: id(&decoded) }).unwrap();
    assert_eq!(duplicate.state.needs(), decoded.needs());
    assert_eq!(duplicate.outcome, Outcome::Duplicate);
}

#[test]
fn wrong_world_confirmation_is_rejected_even_for_same_actor_and_ordinal() {
    let a = plan(&initial(), RandomState::seeded(0), NeedEffect::default());
    let other_world = LifeState::new(actor("nova"), WorldId::new("another-desktop").unwrap(), initial().needs());
    let b = plan(&other_world, a.random, NeedEffect::new(1_000, 0, 0, 0).unwrap());
    let started = reduce(&b.state, 0, b.random, LifeEvent::Start { action_id: id(&b.state) }).unwrap();
    assert_eq!(a.intents[0].id().ordinal(), b.intents[0].id().ordinal());
    assert_ne!(a.intents[0].id(), b.intents[0].id());
    assert_eq!(
        reduce(&started.state, 0, started.random, LifeEvent::Complete { action_id: a.intents[0].id().clone() }),
        Err(LifeError::WrongWorld)
    );
}

#[test]
fn recorded_tick_duration_must_agree_with_its_clock_advance() {
    let a = reduce(&initial(), 10, RandomState::seeded(0), LifeEvent::Tick {}).unwrap();
    let b = reduce(&a.state, 5, a.random, LifeEvent::Tick {}).unwrap();
    let mut bad = state_json(&a.state);
    bad["recentEvents"][0]["event"]["elapsed_ms"] = json!(11);
    rejects(bad);
    let mut bad = state_json(&b.state);
    bad["recentEvents"][1]["event"]["elapsed_ms"] = json!(60_000);
    rejects(bad);
    let mut bad = state_json(&initial());
    bad["simulationTime"] = json!(1);
    rejects(bad);
}

#[test]
fn full_history_requires_the_original_plan_and_each_transition_in_order() {
    let complete = complete_one(&initial(), RandomState::seeded(0), NeedEffect::new(1_000, 0, 0, 0).unwrap());
    let mut bad = state_json(&complete.state);
    let mut only_completion = bad["recentEvents"][2].clone();
    only_completion["id"]["sequence"] = json!(1);
    bad["revision"] = json!(1);
    bad["recentEvents"] = json!([only_completion]);
    rejects(bad);
    let planned = plan(&initial(), RandomState::seeded(0), NeedEffect::default());
    let mut bad = state_json(&planned.state);
    bad["recentEvents"][0]["event"] = json!({"type":"needs_changed","effect":NeedEffect::default()});
    rejects(bad);
    let mut bad = state_json(&planned.state);
    bad["currentAction"]["intent"]["id"]["ordinal"] = json!(2);
    bad["recentEvents"][0]["event"]["intent"]["id"]["ordinal"] = json!(2);
    bad["nextActionOrdinal"] = json!(3);
    bad["resolvedActionOrdinal"] = json!(1);
    rejects(bad);
}
