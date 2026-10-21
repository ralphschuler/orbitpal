//! Pure actual-domain fixtures. Domain reduce stands in for a *local test*
//! persistence result; these tests do not establish actual Store/Core/IPC wiring.
use super::*;
use crate::{
    life_domain::{NeedEffect, Needs, WorldId},
    world_state::{
        coordinates::Rect, Anchor, AnchorId, RuntimeEpoch, Slot, WorkIdentity, WorldConfig,
        WorldState,
    },
};
fn point() -> Point {
    Point::new(500_000, 500_000).unwrap()
}
fn initial(id: &str) -> LifeState {
    LifeState::new(
        ActorId::new(id).unwrap(),
        WorldId::new(format!("world-{id}")).unwrap(),
        Needs::new(1000, 2000, 3000, 4000).unwrap(),
    )
}
fn authority(id: &str) -> NativeAuthority {
    NativeAuthority::new(
        SessionStamp::new(ActorId::new(id).unwrap(), format!("session-{id}"), 1).unwrap(),
        WorldId::new(format!("world-{id}")).unwrap(),
        StageId::new("shared-stage").unwrap(),
        1,
        1,
        OutputStamp::new(format!("output-{id}"), 1).unwrap(),
        1,
    )
    .unwrap()
}
fn goal(kind: Activity) -> Goal {
    match kind {
        Activity::MoveTo | Activity::LookAt => Goal::Point(point()),
        Activity::Wave | Activity::Pet => Goal::Here(point()),
        Activity::Rest | Activity::Recharge => Goal::Slot {
            id: SlotId::new("rest-1").unwrap(),
            kind: AnchorKind::Rest,
            position: point(),
        },
        Activity::Play => Goal::Slot {
            id: SlotId::new("play-1").unwrap(),
            kind: AnchorKind::Play,
            position: point(),
        },
        Activity::Snack => Goal::Slot { id: SlotId::new("snack-1").unwrap(), kind: AnchorKind::Snack, position: point() },
        Activity::Socialize => Goal::Slot { id: SlotId::new("meeting-1").unwrap(), kind: AnchorKind::Meeting, position: point() },
    }
}
fn apply(state: &mut LifeState, random: &mut RandomState, event: LifeEvent) {
    let transition = life_domain::reduce(state, 0, *random, event).unwrap();
    *state = transition.state;
    *random = transition.random;
}
fn tick(state: &mut LifeState, random: &mut RandomState, elapsed: i64) {
    let transition = life_domain::reduce(state, elapsed, *random, LifeEvent::Tick {}).unwrap();
    *state = transition.state;
    *random = transition.random;
}
struct Rig {
    executor: Executor,
    authority: NativeAuthority,
    key: JobKey,
    state: LifeState,
    random: RandomState,
    delivery: String,
    receipt_sequence: u64,
    world: Option<(WorldState, crate::world_state::MemberHandle)>,
}
impl Rig {
    fn new(kind: Activity, presentation: Presentation) -> Self {
        Self::configured(kind, presentation, None)
    }
    fn configured(kind: Activity, presentation: Presentation, care: Option<&crate::life_self_care::CareConfig>) -> Self {
        let authority = authority("nova");
        let mut state = initial("nova");
        let mut random = RandomState::seeded(17);
        let capabilities = Capabilities::new(
            authority.clone(),
            vec![(kind, 200)],
            presentation == Presentation::NativeRestDwell,
        )
        .unwrap();
        let goal = if presentation == Presentation::NativeRestDwell || care.is_some() {
            Goal::Here(point())
        } else {
            goal(kind)
        };
        let plan = if let Some(config) = care {
            PreparedPlan::care(authority.clone(), &state, random, kind, goal.clone(), presentation, &capabilities, config)
        } else { PreparedPlan::new(
            authority.clone(),
            &state,
            random,
            kind,
            goal.clone(),
            presentation,
            &capabilities,
        ) }
        .unwrap();
        apply(&mut state, &mut random, plan.event().clone());
        let mut world = None;
        let lease = if let Goal::Slot { id, kind, position } = goal {
            let config = WorldConfig::new(
                StageId::new("shared-stage").unwrap(),
                Rect::full(),
                vec![ActorId::new("nova").unwrap()],
                vec![Anchor::new(
                    AnchorId::new("anchor").unwrap(),
                    kind,
                    position,
                    vec![Slot::new(id.clone(), position)],
                )
                .unwrap()],
            )
            .unwrap();
            let mut runtime =
                WorldState::new(config, RuntimeEpoch::new("world-runtime").unwrap()).unwrap();
            let member = runtime
                .bind_actor(authority.session.clone(), Some(&state), 0)
                .unwrap();
            let lease = runtime
                .reserve(
                    0,
                    &member,
                    WorkIdentity::Life(plan.action_id().clone()),
                    &id,
                    60_000,
                    0,
                )
                .unwrap();
            world = Some((runtime, member));
            Some(lease)
        } else {
            None
        };
        let mut executor = Executor::new("executor-runtime").unwrap();
        let key = executor.install_plan(plan, &state, lease, 0).unwrap();
        let delivery = executor.jobs["nova"].delivery.clone();
        Self {
            executor,
            authority,
            key,
            state,
            random,
            delivery,
            receipt_sequence: 0,
            world,
        }
    }
    fn receipt(&mut self, receipt: Receipt, now: u64) -> Result<Decision, ActionError> {
        self.receipt_sequence += 1;
        self.executor.receipt(
            &self.authority,
            &self.key,
            &self.delivery,
            self.receipt_sequence,
            receipt,
            &self.state,
            now,
        )
    }
    fn commit(&mut self, request: CommitRequest, now: u64) {
        let permit = self
            .executor
            .authorize_commit(&self.authority, &request, &self.state, now)
            .unwrap();
        assert_eq!(permit.expected_revision(), self.state.revision());
        apply(&mut self.state, &mut self.random, permit.event().clone());
        self.executor
            .confirm_commit(&self.authority, permit, &self.state, now)
            .unwrap();
    }
    fn start(&mut self) {
        let decision =
            if self.executor.jobs["nova"].policy.presentation == Presentation::NativeRestDwell {
                self.executor
                    .start_native_dwell(&self.authority, &self.key, &self.state, 2)
                    .unwrap()
            } else {
                assert_eq!(
                    self.receipt(Receipt::Delivered, 1).unwrap(),
                    Decision::Accepted
                );
                self.receipt(Receipt::Started, 2).unwrap()
            };
        let Decision::Commit(request) = decision else {
            panic!("expected native Start proposal")
        };
        self.commit(request, 2);
    }
    fn clean_world(&mut self) {
        while let Some(cleanup) = self.executor.take_cleanup() {
            if let (Some(lease), Some((world, member))) = (cleanup.lease, &mut self.world) {
                world.release(&lease, member, 3).unwrap();
            }
        }
    }
}
#[test]
fn native_dwell_completes_without_dialog_voice_visual_receipt_or_extra_reward() {
    let mut rig = Rig::new(Activity::Rest, Presentation::NativeRestDwell);
    let needs = rig.state.needs();
    assert!(
        rig.executor.take_dispatch().is_none(),
        "no fabricated visual Offer"
    );
    rig.start();
    for now in [2002, 4002, 6002, 8002] {
        assert_eq!(
            rig.executor
                .tick_native_dwell(&rig.authority, &rig.key, &rig.state, now)
                .unwrap(),
            Decision::Accepted
        );
    }
    let Decision::Commit(request) = rig
        .executor
        .tick_native_dwell(&rig.authority, &rig.key, &rig.state, 10002)
        .unwrap()
    else {
        panic!("expected Complete")
    };
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Started));
    assert_eq!(rig.state.needs(), needs);
    rig.commit(request, 10002);
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Completed));
    assert_eq!(
        rig.state.needs(),
        needs,
        "documented fallback has zero effect"
    );
    assert!(rig
        .executor
        .outbox
        .iter()
        .all(|event| !matches!(event.message, Message::Offer { .. })));
}
#[test]
fn pet_is_here_only_zero_effect_and_requires_actual_visual_completion() {
    let owner=authority("nova");
    let before=initial("nova");
    let caps=Capabilities::new(owner.clone(),vec![(Activity::Pet,1500)],false).unwrap();
    for invalid in [Goal::Point(point()),Goal::Slot{id:SlotId::new("pet-seat").unwrap(),kind:AnchorKind::Play,position:point()}] {
        assert!(PreparedPlan::new(owner.clone(),&before,RandomState::seeded(1),Activity::Pet,invalid,Presentation::Visual,&caps).is_err());
    }
    assert!(PreparedPlan::new(owner.clone(),&before,RandomState::seeded(1),Activity::Pet,Goal::Here(point()),Presentation::NativeRestDwell,&caps).is_err());
    let prepared=PreparedPlan::new(owner,&before,RandomState::seeded(1),Activity::Pet,Goal::Here(point()),Presentation::Visual,&caps).unwrap();
    assert_eq!(prepared.policy.minimum_active_ms,1500);
    assert_eq!(prepared.policy.effect.event_value(),NeedEffect::default());
    let mut rig=Rig::new(Activity::Pet,Presentation::Visual);
    let needs=rig.state.needs();
    rig.start();
    assert!(rig.receipt(Receipt::Completed(Evidence::VisualFinished),201).is_err());
    let Decision::Commit(request)=rig.receipt(Receipt::Completed(Evidence::VisualFinished),202).unwrap() else {panic!("completion proposal");};
    rig.commit(request,202);
    assert_eq!(rig.state.needs(),needs);
    assert_eq!(rig.state.current_action().unwrap().phase(),ActionPhase::Completed);
    let mut cancelled=Rig::new(Activity::Pet,Presentation::Visual);cancelled.start();
    cancelled.executor.cancel(&cancelled.authority,&cancelled.key,CancelReason::Operator,100).unwrap();
    assert_eq!(cancelled.receipt(Receipt::Completed(Evidence::VisualFinished),1002).unwrap(),Decision::Duplicate);
    assert_eq!(cancelled.state.needs(),needs);
    assert_eq!(cancelled.executor.status(&cancelled.key),Some(Status::Cancelled));
}
#[test]
fn fixed_native_plan_rejects_changed_effect_and_unconfirmed_state() {
    let authority = authority("nova");
    let before = initial("nova");
    let random = RandomState::seeded(5);
    let capabilities =
        Capabilities::new(authority.clone(), vec![(Activity::Wave, 200)], false).unwrap();
    let prepared = PreparedPlan::new(
        authority,
        &before,
        random,
        Activity::Wave,
        Goal::Here(point()),
        Presentation::Visual,
        &capabilities,
    )
    .unwrap();
    assert_eq!(prepared.expected_revision(), before.revision());
    let altered = life_domain::reduce(
        &before,
        0,
        random,
        LifeEvent::Plan {
            activity: Activity::Wave,
            completion_effect: NeedEffect::new(9999, 0, 0, 0).unwrap(),
        },
    )
    .unwrap()
    .state;
    let mut executor = Executor::new("fixed-policy").unwrap();
    assert_eq!(
        executor.install_plan(prepared.clone(), &altered, None, 0),
        Err(ActionError::InvalidCommit)
    );
    assert_eq!(
        executor.install_plan(prepared, &before, None, 0),
        Err(ActionError::InvalidCommit)
    );
    assert!(executor.jobs.is_empty() && executor.outbox.is_empty());
    assert_eq!(before, initial("nova"));
}
#[test]
fn actual_visual_action_effect_is_committed_once_and_lease_cleanup_is_explicit() {
    let mut rig = Rig::new(Activity::Play, Presentation::Visual);
    let needs = rig.state.needs();
    rig.start();
    assert_eq!(
        rig.receipt(Receipt::Completed(Evidence::VisualFinished), 500),
        Err(ActionError::InvalidTransition)
    );
    let Decision::Commit(request) = rig
        .receipt(Receipt::Completed(Evidence::VisualFinished), 1002)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        rig.state.needs(),
        needs,
        "receipt has no effect before commit"
    );
    rig.commit(request.clone(), 1002);
    assert_eq!(rig.state.needs().engagement().units(), 3500);
    let confirmed = rig.state.clone();
    assert_eq!(
        rig.executor
            .authorize_commit(&rig.authority, &request, &confirmed, 1003)
            .unwrap_err(),
        ActionError::InvalidCommit
    );
    assert_eq!(
        rig.receipt(Receipt::Completed(Evidence::VisualFinished), 1004)
            .unwrap(),
        Decision::Duplicate
    );
    assert_eq!(rig.state, confirmed);
    rig.clean_world();
    assert!(rig.world.as_ref().unwrap().0.snapshot().occupied.is_empty());
}
#[test]
fn out_of_order_completion_wrong_evidence_and_wrong_target_cannot_finish_move() {
    let mut rig = Rig::new(Activity::MoveTo, Presentation::Visual);
    assert_eq!(
        rig.receipt(Receipt::Started, 1),
        Err(ActionError::InvalidTransition)
    );
    assert_eq!(
        rig.receipt(Receipt::Completed(Evidence::Arrived(point())), 1),
        Err(ActionError::InvalidTransition)
    );
    rig.start();
    assert_eq!(
        rig.receipt(Receipt::Completed(Evidence::VisualFinished), 100),
        Err(ActionError::InvalidTransition)
    );
    assert_eq!(
        rig.receipt(
            Receipt::Completed(Evidence::Arrived(Point::new(1, 1).unwrap())),
            100
        ),
        Err(ActionError::InvalidTransition)
    );
    let Decision::Commit(request) = rig
        .receipt(Receipt::Completed(Evidence::Arrived(point())), 202)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(request.action_id(), rig.key.action_id());
}
#[test]
fn bot_source_world_session_output_asset_and_stage_fences_are_not_interchangeable() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    for changed in 0..6 {
        let mut wrong = rig.authority.clone();
        match changed {
            0 => {
                wrong.session =
                    SessionStamp::new(ActorId::new("other").unwrap(), "session", 1).unwrap()
            }
            1 => wrong.source_world = WorldId::new("wrong-world").unwrap(),
            2 => {
                wrong.session =
                    SessionStamp::new(ActorId::new("nova").unwrap(), "session-nova", 2).unwrap()
            }
            3 => wrong.output = OutputStamp::new("other-output", 1).unwrap(),
            4 => wrong.asset_revision = 2,
            _ => wrong.stage = StageId::new("other-stage").unwrap(),
        }
        assert_eq!(
            rig.executor.receipt(
                &wrong,
                &rig.key,
                &rig.delivery,
                1,
                Receipt::Delivered,
                &rig.state,
                1
            ),
            Err(ActionError::WrongAuthority)
        );
    }
    assert_eq!(
        rig.executor.status(&rig.key),
        Some(Status::AwaitingDelivery)
    );
    assert!(rig.executor.cleanup.is_empty());
}
#[test]
fn receipt_sequence_and_duplicates_never_restart_or_move_timers_backward() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    assert_eq!(
        rig.receipt(Receipt::Progress, 1002).unwrap(),
        Decision::Accepted
    );
    let active = rig.executor.jobs["nova"].active_ms;
    assert_eq!(
        rig.executor
            .receipt(
                &rig.authority,
                &rig.key,
                &rig.delivery,
                1,
                Receipt::Progress,
                &rig.state,
                2002
            )
            .unwrap(),
        Decision::Duplicate
    );
    assert_eq!(rig.executor.jobs["nova"].active_ms, active);
    assert_eq!(
        rig.receipt(Receipt::Started, 1000).unwrap(),
        Decision::Duplicate
    );
    assert_eq!(rig.executor.jobs["nova"].last_progress, Some(1002));
}
#[test]
fn unrelated_durable_tick_uses_current_revision_for_completion_but_wrong_ack_is_rejected() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    tick(&mut rig.state, &mut rig.random, 1000);
    let Decision::Commit(request) = rig
        .receipt(Receipt::Completed(Evidence::VisualFinished), 1002)
        .unwrap()
    else {
        panic!()
    };
    let before = rig.state.clone();
    let permit = rig
        .executor
        .authorize_commit(&rig.authority, &request, &rig.state, 1002)
        .unwrap();
    let mut wrong = before.clone();
    let mut random = rig.random;
    tick(&mut wrong, &mut random, 1000);
    apply(&mut wrong, &mut random, permit.event().clone());
    assert_eq!(
        rig.executor
            .confirm_commit(&rig.authority, permit, &wrong, 1002),
        Err(ActionError::InvalidCommit)
    );
    rig.commit(request, 1002);
    assert_eq!(rig.state.revision(), before.revision() + 1);
}
#[test]
fn stop_replaces_unsent_offer_with_higher_sequence_terminal_and_no_successor() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    let offer_sequence = rig.executor.outbox[0].sequence;
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 1)
        .unwrap();
    assert_eq!(rig.executor.outbox.len(), 1);
    let terminal = rig.executor.take_dispatch().unwrap();
    assert!(terminal.sequence > offer_sequence);
    assert!(matches!(
        terminal.message,
        Message::Terminal {
            status: Status::Cancelled,
            ..
        }
    ));
    assert_eq!(
        rig.receipt(Receipt::Started, 2).unwrap(),
        Decision::Duplicate
    );
    assert!(
        !rig.executor.can_admit(rig.key.actor()),
        "Cancel is not yet durably confirmed"
    );
    let Decision::Commit(request) = rig
        .executor
        .cancellation_commit(&rig.authority, &rig.key, &rig.state, CancelReason::Operator)
        .unwrap()
    else {
        panic!()
    };
    rig.commit(request, 2);
    rig.clean_world();
    assert!(rig.executor.can_admit(rig.key.actor()));
    assert!(rig.executor.take_dispatch().is_none());
}
#[test]
fn stop_during_pause_and_pending_complete_invalidates_late_ack_without_effect() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    rig.executor.pause(&rig.authority, &rig.key, 100).unwrap();
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 101)
        .unwrap();
    assert_eq!(
        rig.executor.resume(&rig.authority, &rig.key, 102),
        Err(ActionError::InvalidTransition)
    );
    let mut second = Rig::new(Activity::Wave, Presentation::Visual);
    second.start();
    let Decision::Commit(request) = second
        .receipt(Receipt::Completed(Evidence::VisualFinished), 1002)
        .unwrap()
    else {
        panic!()
    };
    second
        .executor
        .cancel(&second.authority, &second.key, CancelReason::Operator, 1003)
        .unwrap();
    assert_eq!(
        second
            .executor
            .authorize_commit(&second.authority, &request, &second.state, 1004)
            .unwrap_err(),
        ActionError::InvalidCommit
    );
    assert_eq!(
        second.state.current_action().unwrap().phase(),
        ActionPhase::Started
    );
    assert_eq!(second.executor.status(&second.key), Some(Status::Cancelled));
}
#[test]
fn pause_excludes_elapsed_dwell_and_long_sleep_fails_without_catchup() {
    let mut rig = Rig::new(Activity::Rest, Presentation::NativeRestDwell);
    rig.start();
    rig.executor
        .tick_native_dwell(&rig.authority, &rig.key, &rig.state, 4002)
        .unwrap();
    rig.executor.pause(&rig.authority, &rig.key, 4002).unwrap();
    rig.executor.resume(&rig.authority, &rig.key, 8002).unwrap();
    assert_eq!(
        rig.executor
            .tick_native_dwell(&rig.authority, &rig.key, &rig.state, 10002)
            .unwrap(),
        Decision::Accepted
    );
    assert_eq!(rig.executor.jobs["nova"].active_ms, 6000);
    assert_eq!(
        rig.executor
            .tick_native_dwell(&rig.authority, &rig.key, &rig.state, 20002)
            .unwrap(),
        Decision::Accepted
    );
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Failed));
    assert_eq!(
        rig.state.current_action().unwrap().phase(),
        ActionPhase::Started
    );
    assert!(rig.executor.take_cleanup().unwrap().cancel_reason.is_some());
}
#[test]
fn timeout_releases_exact_resource_and_never_confirms_the_pending_start() {
    let mut rig = Rig::new(Activity::Play, Presentation::Visual);
    assert_eq!(
        rig.receipt(Receipt::Delivered, 1).unwrap(),
        Decision::Accepted
    );
    let Decision::Commit(request) = rig.receipt(Receipt::Started, 2).unwrap() else {
        panic!()
    };
    rig.executor.advance(5000).unwrap();
    assert_eq!(
        rig.executor
            .authorize_commit(&rig.authority, &request, &rig.state, 5000)
            .unwrap_err(),
        ActionError::InvalidCommit
    );
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Failed));
    rig.clean_world();
    assert!(rig.world.as_ref().unwrap().0.snapshot().occupied.is_empty());
    assert_eq!(
        rig.state.current_action().unwrap().phase(),
        ActionPhase::Planned
    );
}
#[test]
fn unsupported_assets_and_nonopted_in_fallback_do_not_allocate_or_dispatch() {
    let authority = authority("nova");
    let state = initial("nova");
    let capabilities = Capabilities::new(authority.clone(), vec![], false).unwrap();
    assert!(matches!(
        PreparedPlan::new(
            authority.clone(),
            &state,
            RandomState::seeded(1),
            Activity::Wave,
            goal(Activity::Wave),
            Presentation::Visual,
            &capabilities
        ),
        Err(ActionError::UnsupportedCapability)
    ));
    assert!(matches!(
        PreparedPlan::new(
            authority,
            &state,
            RandomState::seeded(1),
            Activity::Rest,
            Goal::Here(point()),
            Presentation::NativeRestDwell,
            &capabilities
        ),
        Err(ActionError::UnsupportedCapability)
    ));
    assert!(state.current_action().is_none());
    assert_eq!(state.revision(), 0);
}
#[test]
fn future_rejected_receipt_keeps_clock_highwater_and_invalid_clock_revokes_outputs() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    assert_eq!(
        rig.executor.receipt(
            &rig.authority,
            &rig.key,
            "wrong-delivery",
            1,
            Receipt::Delivered,
            &rig.state,
            3000
        ),
        Err(ActionError::StaleReceipt)
    );
    assert_eq!(rig.executor.now, 3000);
    assert_eq!(
        rig.receipt(Receipt::Delivered, 1).unwrap(),
        Decision::Accepted
    );
    assert_eq!(rig.executor.now, 3000);
    assert_eq!(
        rig.executor.advance(MAX_COUNTER + 1),
        Err(ActionError::Exhausted)
    );
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Failed));
    assert_eq!(
        rig.executor.take_invalidated_output(),
        Some(rig.authority.output.clone())
    );
    assert!(rig.executor.outbox.is_empty());
    assert_eq!(rig.executor.advance(1), Err(ActionError::Exhausted));
}
#[test]
fn identity_exhaustion_is_atomic_and_public_wire_contains_no_native_authority() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    let json = serde_json::to_string(&rig.executor.outbox[0].message).unwrap();
    for marker in [
        "session-nova",
        "output-nova",
        "asset_revision",
        "topology_revision",
        "membership",
        "secret",
        "PERSONA",
    ] {
        assert!(!json.contains(marker), "{marker}");
    }
    rig.executor.next = MAX_COUNTER;
    assert_eq!(
        rig.executor
            .cancel(&rig.authority, &rig.key, CancelReason::Operator, 1),
        Err(ActionError::Exhausted)
    );
    assert!(rig.executor.outbox.is_empty());
    assert!(rig.executor.take_invalidated_output().is_some());
    assert!(!rig.executor.can_admit(rig.key.actor()));
}
fn install_wave(executor: &mut Executor, id: &str) -> (NativeAuthority, JobKey, LifeState) {
    let authority = authority(id);
    let mut state = initial(id);
    let mut random = RandomState::seeded(17);
    let capabilities =
        Capabilities::new(authority.clone(), vec![(Activity::Wave, 200)], false).unwrap();
    let plan = PreparedPlan::new(
        authority.clone(),
        &state,
        random,
        Activity::Wave,
        Goal::Here(point()),
        Presentation::Visual,
        &capabilities,
    )
    .unwrap();
    apply(&mut state, &mut random, plan.event().clone());
    let key = executor.install_plan(plan, &state, None, 0).unwrap();
    (authority, key, state)
}
#[test]
fn two_actor_jobs_are_independent_and_stop_removes_only_its_own_offer() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    let (other_authority, other_key, other_state) = install_wave(&mut rig.executor, "other");
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 1)
        .unwrap();
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Cancelled));
    assert_eq!(
        rig.executor.status(&other_key),
        Some(Status::AwaitingDelivery)
    );
    let delivery = rig.executor.jobs["other"].delivery.clone();
    assert_eq!(
        rig.executor
            .receipt(
                &other_authority,
                &other_key,
                &delivery,
                1,
                Receipt::Delivered,
                &other_state,
                2
            )
            .unwrap(),
        Decision::Accepted
    );
    assert_eq!(rig.executor.status(&other_key), Some(Status::Delivered));
    assert!(rig
        .executor
        .outbox
        .iter()
        .any(|event| event.key == other_key && matches!(event.message, Message::Offer { .. })));
    assert!(rig
        .executor
        .cleanup
        .iter()
        .all(|cleanup| cleanup.key.actor.as_str() == "nova"));
}
#[test]
fn drained_offers_still_reserve_room_for_every_outstanding_terminal_tombstone() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.executor.outbox.clear(); // the current output consumed its Offer
                                 // Model retained terminal messages for distinct earlier, already forgotten
                                 // actors. This fixture does not create live authority for those actors.
    for ordinal in 0..251 {
        let actor = ActorId::new(format!("retired-{ordinal}")).unwrap();
        let action = ActionId::new(
            actor.clone(),
            WorldId::new(format!("retired-world-{ordinal}")).unwrap(),
            1,
        )
        .unwrap();
        let key = JobKey {
            actor,
            action: action.clone(),
            run: "old-run".into(),
            serial: ordinal + 1,
        };
        rig.executor.outbox.push_back(Dispatch {
            output: OutputStamp::new("retired-output", 1).unwrap(),
            sequence: ordinal + 1,
            message: Message::Terminal {
                delivery_id: format!("old-{ordinal}"),
                action_id: action,
                status: Status::Completed,
                failure: None,
            },
            key,
        });
    }
    assert!(rig.executor.can_admit(&ActorId::new("other").unwrap()));
    let (other_authority, other_key, _state) = install_wave(&mut rig.executor, "other");
    assert!(!rig.executor.can_admit(&ActorId::new("third").unwrap()));
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 1)
        .unwrap();
    rig.executor
        .cancel(&other_authority, &other_key, CancelReason::Operator, 1)
        .unwrap();
    assert!(rig.executor.outbox.len() <= MAX_OUTBOX);
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Cancelled));
    assert_eq!(rig.executor.status(&other_key), Some(Status::Cancelled));
}
#[test]
fn successor_on_another_output_preserves_the_old_outputs_undrained_terminal() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    let old_offer = rig.executor.take_dispatch().unwrap(); // delayed by output A
    let old_key = rig.key.clone();
    let old_authority = rig.authority.clone();
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 1)
        .unwrap();
    let Decision::Commit(request) = rig
        .executor
        .cancellation_commit(&rig.authority, &rig.key, &rig.state, CancelReason::Operator)
        .unwrap()
    else {
        panic!()
    };
    rig.commit(request, 1);
    rig.clean_world();
    let mut next_authority = rig.authority.clone();
    next_authority.output = OutputStamp::new("output-B", 2).unwrap();
    let capabilities =
        Capabilities::new(next_authority.clone(), vec![(Activity::Wave, 200)], false).unwrap();
    let plan = PreparedPlan::new(
        next_authority.clone(),
        &rig.state,
        rig.random,
        Activity::Wave,
        Goal::Here(point()),
        Presentation::Visual,
        &capabilities,
    )
    .unwrap();
    apply(&mut rig.state, &mut rig.random, plan.event().clone());
    let next = rig
        .executor
        .install_plan(plan, &rig.state, None, 2)
        .unwrap();
    let terminal = rig.executor.take_dispatch().unwrap();
    let offer = rig.executor.take_dispatch().unwrap();
    assert_eq!(terminal.output, old_authority.output);
    assert!(matches!(
        terminal.message,
        Message::Terminal {
            status: Status::Cancelled,
            ..
        }
    ));
    assert!(terminal.sequence > old_offer.sequence);
    assert_eq!(offer.output, next_authority.output);
    assert!(offer.sequence > terminal.sequence);
    assert_eq!(offer.key, next);
    assert_eq!(
        rig.executor.receipt(
            &old_authority,
            &old_key,
            &rig.delivery,
            1,
            Receipt::Started,
            &rig.state,
            3
        ),
        Err(ActionError::StaleReceipt)
    );
}
#[test]
fn late_complete_and_backward_time_after_rejected_future_receipt_get_no_write_permit() {
    for future_rejection in [false, true] {
        let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
        rig.start();
        let Decision::Commit(request) = rig
            .receipt(Receipt::Completed(Evidence::VisualFinished), 202)
            .unwrap()
        else {
            panic!()
        };
        let before = rig.state.clone();
        if future_rejection {
            assert_eq!(
                rig.executor.receipt(
                    &rig.authority,
                    &rig.key,
                    "wrong",
                    9,
                    Receipt::Progress,
                    &rig.state,
                    10000
                ),
                Err(ActionError::StaleReceipt)
            );
        }
        assert_eq!(
            rig.executor
                .authorize_commit(
                    &rig.authority,
                    &request,
                    &rig.state,
                    if future_rejection { 203 } else { 10000 }
                )
                .unwrap_err(),
            ActionError::StaleReceipt
        );
        assert_eq!(rig.state, before, "no Complete write occurred");
        assert_eq!(
            rig.state.current_action().unwrap().phase(),
            ActionPhase::Started
        );
    }
}
#[test]
fn valid_prewrite_admission_truthfully_confirms_a_later_delivered_durable_result() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    let Decision::Commit(request) = rig
        .receipt(Receipt::Completed(Evidence::VisualFinished), 202)
        .unwrap()
    else {
        panic!()
    };
    let permit = rig
        .executor
        .authorize_commit(&rig.authority, &request, &rig.state, 202)
        .unwrap();
    // The local actual-domain write happens immediately after admission. Only
    // delivery of its already-completed result is delayed; no rollback is faked.
    apply(&mut rig.state, &mut rig.random, permit.event().clone());
    rig.executor
        .confirm_commit(&rig.authority, permit, &rig.state, 10000)
        .unwrap();
    assert_eq!(rig.executor.status(&rig.key), Some(Status::Completed));
    assert!(rig.executor.snapshot(&rig.key).unwrap().terminal_committed);
}
#[test]
fn stop_revokes_an_unconsumed_prewrite_permit() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    let Decision::Commit(request) = rig
        .receipt(Receipt::Completed(Evidence::VisualFinished), 202)
        .unwrap()
    else {
        panic!()
    };
    let permit = rig
        .executor
        .authorize_commit(&rig.authority, &request, &rig.state, 202)
        .unwrap();
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 203)
        .unwrap();
    assert_eq!(
        rig.executor
            .confirm_commit(&rig.authority, permit, &rig.state, 204),
        Err(ActionError::InvalidCommit)
    );
    assert_eq!(
        rig.state.current_action().unwrap().phase(),
        ActionPhase::Started
    );
    assert!(!rig.executor.can_admit(rig.key.actor()));
}
#[test]
fn progress_loss_is_enforced_by_native_advance_before_the_longer_run_deadline() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    rig.executor.advance(5003).unwrap();
    let snapshot = rig.executor.snapshot(&rig.key).unwrap();
    assert_eq!(snapshot.status, Status::Failed);
    assert_eq!(snapshot.failure, Some(Failure::Interrupted));
    assert!(!snapshot.terminal_committed);
    assert_eq!(
        rig.executor.take_cleanup().unwrap().cancel_reason,
        Some(CancelReason::Interrupted)
    );
}
#[test]
fn one_same_action_tick_rebase_preserves_proof_and_invalidates_the_old_request() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    let Decision::Commit(request) = rig
        .receipt(Receipt::Completed(Evidence::VisualFinished), 202)
        .unwrap()
    else {
        panic!()
    };
    tick(&mut rig.state, &mut rig.random, 1000);
    let refreshed = rig
        .executor
        .refresh_commit_request(&rig.authority, &request, &rig.state, 300)
        .unwrap();
    assert_eq!(refreshed.action_id(), request.action_id());
    assert_eq!(refreshed.proposed_at, request.proposed_at);
    assert_eq!(
        rig.executor
            .authorize_commit(&rig.authority, &request, &rig.state, 300)
            .unwrap_err(),
        ActionError::InvalidCommit
    );
    tick(&mut rig.state, &mut rig.random, 1000);
    assert_eq!(
        rig.executor
            .refresh_commit_request(&rig.authority, &refreshed, &rig.state, 400),
        Err(ActionError::InvalidCommit)
    );
    assert_eq!(
        rig.state.current_action().unwrap().phase(),
        ActionPhase::Started
    );
}
#[test]
fn native_stop_cancellation_reconciles_without_fabricating_completion_or_reactivation() {
    let mut rig = Rig::new(Activity::Wave, Presentation::Visual);
    rig.start();
    rig.executor
        .cancel(&rig.authority, &rig.key, CancelReason::Operator, 3)
        .unwrap();
    assert_eq!(
        rig.executor
            .confirm_native_cancelled(&rig.authority, &rig.key, &rig.state, 3),
        Err(ActionError::InvalidCommit)
    );
    let key = rig.key.clone();
    apply(
        &mut rig.state,
        &mut rig.random,
        LifeEvent::Cancel {
            action_id: key.action_id().clone(),
            reason: CancelReason::Operator,
        },
    );
    rig.executor
        .confirm_native_cancelled(&rig.authority, &key, &rig.state, 3)
        .unwrap();
    let snapshot = rig.executor.snapshot(&key).unwrap();
    assert_eq!(snapshot.status, Status::Cancelled);
    assert!(snapshot.terminal_committed);
    rig.clean_world();
    assert!(rig.executor.can_admit(key.actor()));
    assert_eq!(
        rig.state.current_action().unwrap().phase(),
        ActionPhase::Cancelled
    );
}

#[test]
fn declared_long_station_duration_is_bounded_and_never_a_completion_receipt() {
    let before=initial("nova");let authority=authority("nova");
    let caps=Capabilities::new(authority.clone(),vec![(Activity::Rest,60_000)],false).unwrap();
    let prepared=PreparedPlan::care(authority,&before,RandomState::seeded(7),Activity::Rest,goal(Activity::Rest),Presentation::Visual,&caps,&Default::default()).unwrap();
    assert_eq!(prepared.policy.minimum_active_ms,60_000);assert_eq!(prepared.policy.maximum_run_ms,75_000);
    assert!(matches!(prepared.event(),LifeEvent::Plan{..}));assert!(before.current_action().is_none());
}

#[test]
fn saved_care_minimum_blocks_real_executor_completion_until_confirmed_active_time() {
    let config=crate::life_self_care::CareConfig{minimum_duration_ms:60_000,..Default::default()};
    let mut rig=Rig::configured(Activity::Play,Presentation::Visual,Some(&config));
    let needs=rig.state.needs();rig.start();
    assert_eq!(rig.receipt(Receipt::Completed(Evidence::VisualFinished),1002),Err(ActionError::InvalidTransition));
    for now in (1002..60_002).step_by(1000) {
        assert_eq!(rig.receipt(Receipt::Progress,now).unwrap(),Decision::Accepted);
    }
    assert_eq!(rig.receipt(Receipt::Completed(Evidence::VisualFinished),60_001),Err(ActionError::InvalidTransition));
    assert_eq!(rig.state.needs(),needs);
    let Decision::Commit(request)=rig.receipt(Receipt::Completed(Evidence::VisualFinished),60_002).unwrap() else {panic!("expected real Complete proposal");};
    assert_eq!(rig.executor.status(&rig.key),Some(Status::Started));
    assert_eq!(rig.state.needs(),needs,"elapsed time alone is not a Store receipt");
    rig.commit(request,60_002);
    assert_eq!(rig.executor.status(&rig.key),Some(Status::Completed));
    assert!(rig.state.needs().engagement().units()>needs.engagement().units());
}

#[test]
fn maximum_saved_care_duration_has_matching_bounded_policy_and_capabilities() {
    let before=initial("nova");let authority=authority("nova");
    let config=crate::life_self_care::CareConfig{minimum_duration_ms:600_000,..Default::default()};
    let caps=Capabilities::new(authority.clone(),vec![(Activity::Play,600_000)],false).unwrap();
    let prepared=PreparedPlan::care(authority.clone(),&before,RandomState::seeded(7),Activity::Play,goal(Activity::Play),Presentation::Visual,&caps,&config).unwrap();
    assert_eq!(prepared.policy.minimum_active_ms,600_000);
    assert_eq!(prepared.policy.maximum_run_ms,615_000);
    assert!(Capabilities::new(authority.clone(),vec![(Activity::Play,600_001)],false).is_err());
    assert!(Capabilities::new(authority.clone(),vec![(Activity::MoveTo,60_001)],false).is_err());
    assert!(PreparedPlan::new(authority.clone(),&before,RandomState::seeded(7),Activity::Play,goal(Activity::Play),Presentation::Visual,&caps).is_err(),"ordinary non-care policies keep their fixed run bounds");
    let invalid=crate::life_self_care::CareConfig{minimum_duration_ms:600_001,..config};
    assert!(PreparedPlan::care_visual_minimum(Activity::Play,&goal(Activity::Play),&invalid).is_err());
    assert_eq!(before,initial("nova"));
}

#[test]
fn admitted_execution_deadline_reads_the_frozen_care_job_without_refreshing_it() {
    for minimum in [60_000,600_000] {
        let config=crate::life_self_care::CareConfig{minimum_duration_ms:minimum,..Default::default()};
        let mut rig=Rig::configured(Activity::Play,Presentation::Visual,Some(&config));
        let maximum=minimum+15_000;
        assert_eq!(rig.executor.execution_deadline(&rig.authority,&rig.key).unwrap(),maximum+RECEIPT_TIMEOUT_MS);
        assert_eq!(rig.executor.execution_deadline(&authority("blue"),&rig.key),Err(ActionError::WrongAuthority));
        rig.start();
        assert_eq!(rig.executor.execution_deadline(&rig.authority,&rig.key).unwrap(),maximum+2);
        assert_eq!(rig.executor.execution_deadline(&rig.authority,&rig.key).unwrap(),maximum+2,"readback is not a new clock/deadline");
        rig.executor.fail(&rig.authority,&rig.key,Failure::Interrupted,3).unwrap();
        assert!(rig.executor.execution_deadline(&rig.authority,&rig.key).is_err());
    }
    let rig=Rig::new(Activity::Play,Presentation::Visual);
    assert_eq!(rig.executor.execution_deadline(&rig.authority,&rig.key).unwrap(),15_000+RECEIPT_TIMEOUT_MS);
}
