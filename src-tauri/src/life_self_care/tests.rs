use super::*;
use crate::life_domain::{reduce, LifeEvent, NeedEffect, RandomState};

fn state(levels: [u16; 4]) -> LifeState {
    LifeState::new(
        ActorId::new("nova").unwrap(),
        WorldId::new("world_nova").unwrap(),
        Needs::new(levels[0], levels[1], levels[2], levels[3]).unwrap(),
    )
}
fn update(previous: &LifeState, levels: [u16; 4]) -> LifeState {
    let old = previous.needs();
    let values = [
        old.energy(),
        old.nourishment(),
        old.engagement(),
        old.social(),
    ];
    let delta: Vec<i32> = levels
        .iter()
        .zip(values)
        .map(|(new, old)| i32::from(*new) - i32::from(old.units()))
        .collect();
    reduce(
        previous,
        0,
        RandomState::seeded(7),
        LifeEvent::ApplyNeedEffect {
            effect: NeedEffect::new(delta[0], delta[1], delta[2], delta[3]).unwrap(),
        },
    )
    .unwrap()
    .state
}
fn policy() -> Policy {
    Policy::new(
        [Rule::new(6000, 8000, 1).unwrap(); 4],
        1000,
        300,
        2000,
        5000,
    )
    .unwrap()
}
fn chooser(snapshot: &LifeState) -> Chooser {
    Chooser::new(snapshot, policy(), Preferences::new([1; 4], 1).unwrap())
}
fn offer(need: Need, id: &str, cost: u16) -> Offer {
    Offer::new(need, Target::Slot(SlotId::new(id).unwrap()), cost).unwrap()
}
fn all_offers() -> Vec<Offer> {
    vec![
        offer(Need::Energy, "rest", 0),
        offer(Need::Nourishment, "snack", 0),
        offer(Need::Engagement, "toy", 0),
        offer(Need::Social, "meeting", 0),
    ]
}

#[test]
fn each_urgent_need_selects_only_a_supplied_advisory_offer() {
    for (index, need) in [
        Need::Energy,
        Need::Nourishment,
        Need::Engagement,
        Need::Social,
    ]
    .into_iter()
    .enumerate()
    {
        let mut levels = [9000; 4];
        levels[index] = 0;
        let snapshot = state(levels);
        let proposal = chooser(&snapshot)
            .choose(0, &snapshot, &all_offers(), None)
            .unwrap()
            .unwrap();
        assert_eq!(proposal.offer().need(), need);
        assert_eq!(proposal.selection(), Selection::New);
        assert_eq!(
            snapshot.needs(),
            Needs::new(levels[0], levels[1], levels[2], levels[3]).unwrap()
        );
    }
}

#[test]
fn no_admissible_offer_does_not_invent_food_rest_or_completion() {
    let snapshot = state([0; 4]);
    assert_eq!(
        chooser(&snapshot).choose(0, &snapshot, &[], None).unwrap(),
        None
    );
    let satisfied = state([6000; 4]);
    assert_eq!(
        chooser(&satisfied)
            .choose(0, &satisfied, &all_offers(), None)
            .unwrap(),
        None
    );
}

#[test]
fn unavailable_or_contended_offer_is_replaced_only_from_remaining_offers() {
    let snapshot = state([0, 9000, 9000, 9000]);
    let unavailable = offer(Need::Energy, "occupied", 0);
    let alternative = offer(Need::Energy, "free", 500);
    let current = CurrentChoice::new(unavailable.key().clone(), 0).unwrap();
    let proposal = chooser(&snapshot)
        .choose(10, &snapshot, &[alternative.clone()], Some(&current))
        .unwrap()
        .unwrap();
    assert_eq!(proposal.offer(), &alternative);
    assert_eq!(proposal.selection(), Selection::New);
    assert_eq!(
        chooser(&snapshot)
            .choose(10, &snapshot, &[], Some(&current))
            .unwrap(),
        None
    );
}

#[test]
fn minimum_duration_holds_a_valid_current_choice_then_allows_large_improvement() {
    let snapshot = state([5000, 0, 9000, 9000]);
    let offers = all_offers();
    let current = CurrentChoice::new(offers[0].key().clone(), 0).unwrap();
    let mut memory = chooser(&snapshot);
    let held = memory
        .choose(999, &snapshot, &offers, Some(&current))
        .unwrap()
        .unwrap();
    assert_eq!(held.offer(), &offers[0]);
    assert_eq!(held.selection(), Selection::MinimumDuration);
    assert_eq!(
        memory
            .choose(1000, &snapshot, &offers, Some(&current))
            .unwrap()
            .unwrap()
            .offer(),
        &offers[1]
    );
}

#[test]
fn hysteresis_keeps_small_changes_but_not_large_improvement() {
    let snapshot = state([4000, 3800, 9000, 9000]);
    let offers = all_offers();
    let current = CurrentChoice::new(offers[0].key().clone(), 0).unwrap();
    let mut memory = chooser(&snapshot);
    let held = memory
        .choose(1000, &snapshot, &offers, Some(&current))
        .unwrap()
        .unwrap();
    assert_eq!(held.selection(), Selection::Hysteresis);
    assert_eq!(held.offer(), &offers[0]);
    let changed = update(&snapshot, [4000, 3000, 9000, 9000]);
    assert_eq!(
        memory
            .choose(1001, &changed, &offers, Some(&current))
            .unwrap()
            .unwrap()
            .offer(),
        &offers[1]
    );
}

#[test]
fn ongoing_choice_continues_to_target_but_releases_when_satisfied() {
    let snapshot = state([7000, 9000, 9000, 9000]);
    let rest = offer(Need::Energy, "rest", 0);
    let current = CurrentChoice::new(rest.key().clone(), 0).unwrap();
    let mut memory = chooser(&snapshot);
    assert_eq!(
        memory
            .choose(1000, &snapshot, &[rest.clone()], Some(&current))
            .unwrap()
            .unwrap()
            .offer(),
        &rest
    );
    let satisfied = update(&snapshot, [8000, 9000, 9000, 9000]);
    assert_eq!(
        memory
            .choose(1001, &satisfied, &[rest], Some(&current))
            .unwrap(),
        None
    );
}

#[test]
fn completion_cooldown_does_not_credit_a_need_or_block_another_need() {
    let snapshot = state([0, 1000, 9000, 9000]);
    let offers = all_offers();
    let mut memory = chooser(&snapshot);
    memory
        .record_end(100, offers[0].key(), End::Finished)
        .unwrap();
    assert_eq!(
        memory
            .choose(2099, &snapshot, &offers, None)
            .unwrap()
            .unwrap()
            .offer(),
        &offers[1]
    );
    assert_eq!(
        memory
            .choose(2100, &snapshot, &offers, None)
            .unwrap()
            .unwrap()
            .offer(),
        &offers[0]
    );
    assert_eq!(snapshot.needs().energy().units(), 0);
}

#[test]
fn interrupted_offer_has_exact_backoff_and_can_choose_an_alternative_after_cooldown() {
    let snapshot = state([0, 9000, 9000, 9000]);
    let first = offer(Need::Energy, "a", 0);
    let second = offer(Need::Energy, "b", 0);
    let mut memory = chooser(&snapshot);
    memory.record_end(0, first.key(), End::Interrupted).unwrap();
    assert_eq!(
        memory
            .choose(1999, &snapshot, &[first.clone(), second.clone()], None)
            .unwrap(),
        None
    );
    assert_eq!(
        memory
            .choose(2000, &snapshot, &[first.clone(), second], None)
            .unwrap()
            .unwrap()
            .offer()
            .key()
            .target,
        Target::Slot(SlotId::new("b").unwrap())
    );
    assert_eq!(
        memory
            .choose(4999, &snapshot, &[first.clone()], None)
            .unwrap(),
        None
    );
    assert_eq!(
        memory
            .choose(5000, &snapshot, &[first.clone()], None)
            .unwrap()
            .unwrap()
            .offer(),
        &first
    );
}

#[test]
fn an_unavailable_current_offer_cannot_bypass_backoff_or_minimum_duration() {
    let snapshot = state([0, 9000, 9000, 9000]);
    let rest = offer(Need::Energy, "rest", 0);
    let current = CurrentChoice::new(rest.key().clone(), 0).unwrap();
    let mut memory = chooser(&snapshot);
    memory.record_end(0, rest.key(), End::Unavailable).unwrap();
    assert_eq!(
        memory
            .choose(1, &snapshot, &[rest], Some(&current))
            .unwrap(),
        None
    );
}

#[test]
fn numeric_preferences_and_travel_cost_change_selection_without_randomness() {
    let snapshot = state([1000, 2000, 9000, 9000]);
    let offers = all_offers();
    let mut priority = Chooser::new(
        &snapshot,
        policy(),
        Preferences::new([1, 2, 1, 1], 1).unwrap(),
    );
    assert_eq!(
        priority
            .choose(0, &snapshot, &offers, None)
            .unwrap()
            .unwrap()
            .offer()
            .need(),
        Need::Nourishment
    );
    let near = offer(Need::Energy, "z_near", 0);
    let far = offer(Need::Energy, "a_far", 1000);
    assert_eq!(
        chooser(&snapshot)
            .choose(0, &snapshot, &[far, near.clone()], None)
            .unwrap()
            .unwrap()
            .offer(),
        &near
    );
}

#[test]
fn equal_scores_use_fixed_need_and_target_order_not_slice_order() {
    let snapshot = state([0; 4]);
    let mut offers = all_offers();
    offers.push(offer(Need::Energy, "a", 0));
    let forward = chooser(&snapshot)
        .choose(0, &snapshot, &offers, None)
        .unwrap();
    offers.reverse();
    assert_eq!(
        forward,
        chooser(&snapshot)
            .choose(0, &snapshot, &offers, None)
            .unwrap()
    );
    assert_eq!(
        forward.unwrap().offer().target(),
        &Target::Slot(SlotId::new("a").unwrap())
    );
}

#[test]
fn repeated_same_time_reads_are_identical_advice_without_dispatch_identity() {
    let snapshot = state([0; 4]);
    let offers = all_offers();
    let mut memory = chooser(&snapshot);
    let first = memory.choose(0, &snapshot, &offers, None).unwrap();
    let after = memory.clone();
    for _ in 0..100 {
        assert_eq!(memory.choose(0, &snapshot, &offers, None).unwrap(), first);
    }
    assert_eq!(memory, after);
    let actor = Offer::new(
        Need::Social,
        Target::Actor(ActorId::new("blue").unwrap()),
        0,
    )
    .unwrap();
    let other = Offer::new(
        Need::Social,
        Target::Actor(ActorId::new("zara").unwrap()),
        0,
    )
    .unwrap();
    let social = state([9000, 9000, 9000, 0]);
    assert_eq!(
        chooser(&social)
            .choose(0, &social, &[other, actor.clone()], None)
            .unwrap()
            .unwrap()
            .offer(),
        &actor
    );
}

#[test]
fn maximum_integer_weights_are_bounded_and_need_snapshot_is_not_mutated() {
    let snapshot = state([0; 4]);
    let maximum = Policy::new(
        [Rule::new(9999, 10000, 1000).unwrap(); 4],
        600000,
        MAX_SCORE,
        600000,
        600000,
    )
    .unwrap();
    let mut memory = Chooser::new(
        &snapshot,
        maximum,
        Preferences::new([1000; 4], 1000).unwrap(),
    );
    let proposal = memory
        .choose(MAX_COUNTER, &snapshot, &all_offers(), None)
        .unwrap()
        .unwrap();
    assert_eq!(proposal.score, MAX_SCORE);
    assert_eq!(snapshot.needs(), Needs::new(0, 0, 0, 0).unwrap());
}

#[test]
fn invalid_rules_preferences_policy_and_targets_are_rejected() {
    for args in [
        (0, 8000, 1),
        (6000, 6000, 1),
        (6000, 10001, 1),
        (6000, 8000, 0),
        (6000, 8000, 1001),
    ] {
        assert_eq!(Rule::new(args.0, args.1, args.2), Err(Error::Policy));
    }
    let rules = [Rule::new(6000, 8000, 1).unwrap(); 4];
    for args in [
        (0, 1, 1, 1),
        (600001, 1, 1, 1),
        (1, MAX_SCORE + 1, 1, 1),
        (1, 1, 0, 1),
        (1, 1, 1, 600001),
    ] {
        assert_eq!(
            Policy::new(rules, args.0, args.1, args.2, args.3),
            Err(Error::Policy)
        );
    }
    assert_eq!(Preferences::new([0, 1, 1, 1], 0), Err(Error::Preferences));
    assert_eq!(Preferences::new([1001; 4], 0), Err(Error::Preferences));
    assert_eq!(Preferences::new([1; 4], 1001), Err(Error::Preferences));
    assert_eq!(
        Offer::new(
            Need::Energy,
            Target::Actor(ActorId::new("blue").unwrap()),
            0
        ),
        Err(Error::Offer)
    );
    assert_eq!(
        Offer::new(
            Need::Social,
            Target::Actor(ActorId::new("blue").unwrap()),
            1001
        ),
        Err(Error::Offer)
    );
}

#[test]
fn duplicate_or_oversized_offer_sets_are_atomic_errors() {
    let snapshot = state([0; 4]);
    let rest = offer(Need::Energy, "rest", 0);
    let mut memory = chooser(&snapshot);
    let before = memory.clone();
    assert_eq!(
        memory.choose(10, &snapshot, &[rest.clone(), rest], None),
        Err(Error::Offer)
    );
    let too_many: Vec<_> = (0..=MAX_OFFERS)
        .map(|i| offer(Need::Energy, &format!("slot_{i}"), 0))
        .collect();
    assert_eq!(
        memory.choose(10, &snapshot, &too_many, None),
        Err(Error::Capacity)
    );
    assert_eq!(memory, before);
}

#[test]
fn actor_world_stale_and_same_revision_changed_needs_are_rejected() {
    let snapshot = state([0; 4]);
    let mut memory = chooser(&snapshot);
    let blue = LifeState::new(
        ActorId::new("blue").unwrap(),
        WorldId::new("world_nova").unwrap(),
        snapshot.needs(),
    );
    let world = LifeState::new(
        snapshot.actor_id().clone(),
        WorldId::new("other").unwrap(),
        snapshot.needs(),
    );
    for wrong in [blue, world, state([1; 4])] {
        assert_eq!(memory.choose(0, &wrong, &[], None), Err(Error::Snapshot));
    }
    let next = update(&snapshot, [1; 4]);
    memory.choose(1, &next, &[], None).unwrap();
    assert_eq!(memory.choose(2, &snapshot, &[], None), Err(Error::Snapshot));
}

#[test]
fn clock_regression_deadline_exhaustion_and_retiming_are_atomic_errors() {
    let snapshot = state([0; 4]);
    let rest = offer(Need::Energy, "rest", 0);
    let mut memory = chooser(&snapshot);
    let current = CurrentChoice::new(rest.key().clone(), 0).unwrap();
    memory
        .choose(10, &snapshot, &[rest.clone()], Some(&current))
        .unwrap();
    let before = memory.clone();
    assert_eq!(memory.choose(9, &snapshot, &[], None), Err(Error::Clock));
    assert_eq!(
        memory.record_end(MAX_COUNTER, rest.key(), End::Unavailable),
        Err(Error::Clock)
    );
    assert_eq!(
        memory.choose(MAX_COUNTER + 1, &snapshot, &[], None),
        Err(Error::Clock)
    );
    let retimed = CurrentChoice::new(rest.key().clone(), 1).unwrap();
    assert_eq!(
        memory.choose(11, &snapshot, &[rest.clone()], Some(&retimed)),
        Err(Error::CurrentChoice)
    );
    let future = CurrentChoice::new(rest.key().clone(), 12).unwrap();
    assert_eq!(
        memory.choose(11, &snapshot, &[rest], Some(&future)),
        Err(Error::CurrentChoice)
    );
    assert_eq!(memory, before);
}

#[test]
fn backoff_capacity_never_evicts_an_unexpired_failure_to_allow_a_retry() {
    let snapshot = state([0; 4]);
    let mut memory = chooser(&snapshot);
    for i in 0..MAX_OFFERS {
        memory
            .record_end(
                0,
                offer(Need::Energy, &format!("slot_{i}"), 0).key(),
                End::Unavailable,
            )
            .unwrap();
    }
    let before = memory.clone();
    let extra = offer(Need::Energy, "extra", 0);
    assert_eq!(
        memory.record_end(0, extra.key(), End::Unavailable),
        Err(Error::Capacity)
    );
    assert_eq!(memory, before);
    memory
        .record_end(5000, extra.key(), End::Unavailable)
        .unwrap();
    assert_eq!(memory.blocked.len(), 1);
}

#[test]
fn seeded_actual_domain_snapshot_trace_is_reproducible_and_order_independent() {
    fn trace(seed: u32, reverse: bool) -> Vec<Option<OfferKey>> {
        let mut snapshot = state([5000; 4]);
        let mut memory = chooser(&snapshot);
        let mut input = seed;
        let mut offers = all_offers();
        if reverse {
            offers.reverse();
        }
        let mut trace = Vec::new();
        for step in 0..64 {
            // Test-local inputs, not the domain's private RNG API or product policy.
            let levels = [0; 4].map(|_| {
                input = input.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (input % 10001) as u16
            });
            snapshot = update(&snapshot, levels);
            let proposal = memory
                .choose(step * 1000, &snapshot, &offers, None)
                .unwrap();
            trace.push(
                proposal
                    .as_ref()
                    .map(|proposal| proposal.offer().key().clone()),
            );
            if let Some(proposal) = proposal {
                memory
                    .record_end(step * 1000, proposal.offer().key(), End::Interrupted)
                    .unwrap();
            }
        }
        trace
    }
    let first = trace(73, false);
    assert_eq!(first, trace(73, true));
    assert_ne!(first, trace(74, false));
    assert!(first.iter().any(Option::is_none));
    assert!(first.iter().any(Option::is_some));
}
