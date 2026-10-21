//! Main-window, read-only presentation of actual native work. This DTO is not
//! deserializable and contains no owner proof, profile, prompt or model output.
use super::{core::{Core, Mode}, LifeRuntime};
use crate::{
    action_executor::{Failure, Goal, JobSnapshot, Status},
    life_domain::{Activity, CancelReason},
    social::{Phase, Reason, SceneKind, SceneView},
    world_state::AnchorKind,
};
use serde::Serialize;
use std::sync::atomic::Ordering;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Target {
    Here,
    Point,
    Slot { #[serde(rename = "anchorKind")] anchor_kind: AnchorKind },
    Actor,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeIntent {
    version: u8,
    bot_id: String,
    session_id: String,
    source: &'static str,
    activity: &'static str,
    phase: &'static str,
    target: Option<Target>,
    reason: Option<&'static str>,
    actor_ids: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Scope {
    session: String,
    revision: u64,
    actions: Option<u64>,
}
fn scope(core: &Core, bot: &str) -> Option<Scope> {
    let session = core.sessions.get(bot).filter(|s| s.mode == Mode::Life && s.state.is_some())?;
    Some(Scope { session: session.session_id.clone(), revision: session.revision,
        actions: core.actions.as_ref().map(|actions| actions.sequence) })
}
fn target(goal: &Goal) -> Target {
    match goal {
        Goal::Here(_) => Target::Here,
        Goal::Point(_) => Target::Point,
        Goal::Slot { kind, .. } => Target::Slot { anchor_kind: *kind },
        Goal::Actor { .. } => Target::Actor,
    }
}
fn failure(reason: Failure) -> &'static str {
    match reason {
        Failure::Unsupported => "unsupported", Failure::OutputLost => "output_lost",
        Failure::Timeout => "timeout", Failure::Interrupted => "interrupted",
        Failure::Storage => "storage", Failure::Protocol => "protocol", Failure::Exhausted => "exhausted",
    }
}
fn cancellation(reason: CancelReason) -> &'static str {
    match reason {
        CancelReason::Operator => "operator", CancelReason::Interrupted => "interrupted",
        CancelReason::Unavailable => "unavailable", CancelReason::Timeout => "timeout",
        CancelReason::Restart => "restart",
    }
}
fn action_projection(bot: &str, session: &str, job: &JobSnapshot, goal: &Goal,
    cancelled: bool, approaching: bool, cancelled_for: Option<CancelReason>) -> RuntimeIntent {
    let phase = match job.status {
        Status::AwaitingDelivery => "preparing", Status::Delivered => "delivered",
        Status::Started if approaching || job.activity == Activity::MoveTo => "approach",
        Status::Started => "execution", Status::Paused => "paused", Status::Completed => "completed",
        Status::Cancelled => "cancelled", Status::Failed => "failed",
    };
    let phase = if cancelled && !matches!(job.status, Status::Completed | Status::Cancelled | Status::Failed) {
        "cancelling"
    } else { phase };
    RuntimeIntent { version: 1, bot_id: bot.into(), session_id: session.into(), source: "action",
        activity: job.activity.as_str(), phase, target: Some(target(goal)),
        reason: job.failure.map(failure).or_else(|| cancelled_for.map(cancellation)),
        actor_ids: vec![bot.into()] }
}
fn action(core: &Core, bot: &str, current: &Scope) -> Option<(RuntimeIntent, bool)> {
    let actions = core.actions.as_ref()?;
    let job = actions.jobs.get(bot)?;
    if job.fence.session != current.session || job.fence.revision != current.revision { return None; }
    let confirmed = core.sessions.get(bot)?.state.as_ref()?.current_action()?;
    if confirmed.intent().id() != job.key.action_id() { return None; }
    let snapshot = actions.executor.snapshot(&job.key)?;
    let goal = actions.executor.goal(&job.key)?;
    // These are already accepted visual observations, not a new frame/proof.
    let approaching = job.visual.as_ref().is_some_and(|visual|
        visual.from.distance(visual.destination) > 0.25 && visual.stable.is_empty());
    Some((action_projection(bot, &current.session, &snapshot, goal,
        job.cancel.load(Ordering::SeqCst), approaching, confirmed.cancellation()), job.grouped))
}
fn social_projection(bot: &str, session: &str, scene: &SceneView) -> RuntimeIntent {
    RuntimeIntent { version: 1, bot_id: bot.into(), session_id: session.into(), source: "social",
        activity: match scene.kind { SceneKind::Greeting => "greeting", SceneKind::Play => "play", SceneKind::Rest => "rest" },
        phase: match scene.phase {
            Phase::Invitation => "invitation", Phase::Accepted => "accepted",
            Phase::ResourcesReserved => "resources_reserved", Phase::Approach => "approach",
            Phase::Execution => "execution", Phase::Completed => "completed",
            Phase::Cancelled => "cancelled", Phase::Declined => "declined",
        }, target: None,
        reason: scene.reason.map(|reason| match reason { Reason::Declined => "declined", Reason::Timeout => "timeout",
            Reason::Unavailable => "unavailable", Reason::Operator => "operator", Reason::Restart => "restart" }),
        actor_ids: scene.participants.to_vec() }
}
fn select(action: Option<(RuntimeIntent, bool)>, social: Option<RuntimeIntent>) -> Option<RuntimeIntent> {
    match (action, social) {
        (Some((action, true)), Some(mut social)) => {
            if !matches!(social.phase, "completed" | "cancelled" | "declined")
                && matches!(action.phase, "paused" | "cancelling" | "cancelled" | "failed") {
                return Some(action);
            }
            social.target = action.target; Some(social)
        }
        (Some((action, _)), _) => Some(action),
        (None, social) => social,
    }
}
impl LifeRuntime {
    pub(crate) fn experience_intent(&self, bot: &str) -> Result<Option<RuntimeIntent>, String> {
        let (current, action) = {
            let core = self.core.lock().map_err(|_| "experience_native_unavailable")?;
            let Some(current) = scope(&core, bot) else { return Ok(None); };
            let action = action(&core, bot, &current);
            (current, action)
        };
        let scene = super::social_host::intent_snapshot::current(&self.social, bot, &current.session, current.revision)?;
        // Stop/replacement or a changed job during the separate Social read
        // invalidates this sample. No nested Core/Social/Store/Owned locks.
        let core = self.core.lock().map_err(|_| "experience_native_unavailable")?;
        if scope(&core, bot).as_ref() != Some(&current) { return Ok(None); }
        Ok(select(action, scene.as_ref().map(|scene| social_projection(bot, &current.session, scene))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{action_executor::Presentation, life_domain::{ActionId, ActorId, WorldId}, world_state::coordinates::Point};
    fn job(status: Status) -> JobSnapshot {
        JobSnapshot { version: 1, action_id: ActionId::new(ActorId::new("nova").unwrap(), WorldId::new("world").unwrap(), 1).unwrap(),
            activity: Activity::Rest, status, failure: None, presentation: Presentation::Visual, active_ms: 0, terminal_committed: false }
    }
    #[test]
    fn here_and_real_slot_are_distinct_and_no_coordinate_or_authority_is_exported() {
        let point = Point::new(500_000, 500_000).unwrap();
        let here = action_projection("nova", "session", &job(Status::Started), &Goal::Here(point), false, false, None);
        let slot = action_projection("nova", "session", &job(Status::Started), &Goal::Slot {
            id: crate::world_state::SlotId::new("slot").unwrap(), kind: AnchorKind::Rest, position: point }, false, true, None);
        let here = serde_json::to_value(here).unwrap(); let slot = serde_json::to_value(slot).unwrap();
        assert_eq!(here["target"]["type"], "here"); assert_eq!(here["phase"], "execution");
        assert_eq!(slot["target"]["anchorKind"], "rest"); assert_eq!(slot["phase"], "approach");
        assert!(slot["target"].get("position").is_none()); assert!(slot.get("outputProof").is_none());
    }
    #[test]
    fn real_failures_and_requested_cancellation_are_not_completed_actions() {
        let point = Point::new(500_000, 500_000).unwrap(); let mut failed = job(Status::Failed); failed.failure = Some(Failure::OutputLost);
        let intent = action_projection("nova", "session", &failed, &Goal::Here(point), false, false, None);
        assert_eq!(intent.phase, "failed"); assert_eq!(intent.reason, Some("output_lost"));
        assert_eq!(action_projection("nova", "session", &job(Status::Started), &Goal::Here(point), true, false, None).phase, "cancelling");
        assert_eq!(action_projection("nova", "session", &job(Status::Cancelled), &Goal::Here(point), true, false, Some(CancelReason::Operator)).reason, Some("operator"));
    }
    #[test]
    fn social_invitation_and_terminal_reason_are_real_scene_state_not_album_facts() {
        let mut scene = SceneView { id: "scene_1".into(), sequence: 1, kind: SceneKind::Play, phase: Phase::Invitation,
            participants: ["nova".into(), "friend".into()], inviter_id: "nova".into(), at_ms: 1, deadline_ms: 30_001, reason: None };
        let invitation = social_projection("nova", "session", &scene); assert_eq!(invitation.phase, "invitation");
        assert_eq!(invitation.actor_ids, vec!["nova", "friend"]);
        scene.phase = Phase::Cancelled; scene.reason = Some(Reason::Timeout);
        let cancelled = social_projection("nova", "session", &scene); assert_eq!(cancelled.reason, Some("timeout"));
        assert_eq!(select(None, Some(cancelled)).unwrap().phase, "cancelled"); assert!(select(None, None).is_none());
    }
    #[test]
    fn a_cancelled_group_child_cannot_be_labelled_as_an_executing_scene() {
        let mut scene = social_projection("nova", "session", &SceneView { id: "scene_1".into(), sequence: 1,
            kind: SceneKind::Play, phase: Phase::Execution, participants: ["nova".into(), "friend".into()],
            inviter_id: "nova".into(), at_ms: 1, deadline_ms: 30_001, reason: None });
        let point = Point::new(500_000, 500_000).unwrap();
        let cancelled = action_projection("nova", "session", &job(Status::Cancelled), &Goal::Here(point), true, false, Some(CancelReason::Operator));
        assert_eq!(select(Some((cancelled.clone(), true)), Some(scene.clone())).unwrap().phase, "cancelled");
        scene.phase = "cancelled"; scene.reason = Some("timeout");
        assert_eq!(select(Some((cancelled, true)), Some(scene)).unwrap().reason, Some("timeout"));
    }
    #[test]
    fn stop_and_replacement_do_not_inherit_a_previous_session_intention() {
        use super::super::core::SpeechConfig;
        use crate::{life_availability::{Availability, Reason, Snapshot, Source}, life_domain::{LifeState, Needs}};
        let mut core = Core::new(Snapshot { state: Availability::AwakeUnlocked, reason: Reason::Ready,
            generation: 1, source: Source::MacosCgSessionCompatibility });
        let state = LifeState::new(ActorId::new("nova").unwrap(), WorldId::new("world").unwrap(), Needs::new(5000, 5000, 5000, 5000).unwrap());
        let config = SpeechConfig { speak: false, min_ms: 15_000, max_ms: 15_000 };
        core.start("nova", Mode::Life, "11111111-1111-4111-8111-111111111111".into(), config.clone(), Some(state.clone()), 7, false, true, 0).unwrap();
        let previous = scope(&core, "nova").unwrap();
        core.stop("nova", 1); assert!(scope(&core, "nova").is_none());
        core.start("nova", Mode::Life, "22222222-2222-4222-8222-222222222222".into(), config, Some(state), 7, false, true, 2).unwrap();
        assert_ne!(scope(&core, "nova").unwrap(), previous); assert!(scope(&core, "friend").is_none());
    }
}
