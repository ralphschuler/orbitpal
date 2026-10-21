//! Structured native retrieval context. No private history, persona, grants or images.
use crate::{
    character_development::Goal,
    social::{Outcome, SceneKind},
    storage::{social_store::Memory, Store},
};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutonomyContext {
    pub version: u8,
    pub event_id: String,
    pub owner_id: String,
    pub goal: Option<Goal>,
    pub events: Vec<Fact>,
    pub participants: Vec<String>,
    pub allowed_facts: Vec<Fact>,
    pub epoch: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Fact {
    pub event_id: String,
    pub kind: SceneKind,
    pub outcome: Outcome,
    pub participants: [String; 2],
    pub at_ms: u64,
    pub shared: bool,
}
impl AutonomyContext {
    pub(crate) fn new(
        owner: &str,
        memory: &Memory,
        goal: Option<&Goal>,
        epoch: u64,
    ) -> Result<Self, String> {
        if !crate::storage::valid_id(owner)
            || memory.event_id.len() > 96
            || !crate::storage::valid_id(&memory.event_id)
            || memory.origin != "own" && !(memory.origin == "shared" && memory.shared)
            || memory.origin == "own" && !memory.participants.iter().any(|id| id == owner)
        {
            return Err("autonomy fact not visible".into());
        }
        let fact = Fact {
            event_id: memory.event_id.clone(),
            kind: memory.kind,
            outcome: memory.outcome,
            participants: memory.participants.clone(),
            at_ms: memory.at_ms,
            shared: memory.shared,
        };
        Ok(Self {
            version: 1,
            event_id: memory.event_id.clone(),
            owner_id: owner.into(),
            goal: goal.cloned(),
            participants: memory.participants.to_vec(),
            events: vec![fact.clone()],
            allowed_facts: vec![fact],
            epoch,
        })
    }
    pub(crate) fn from_goal(owner: &str, goal: &Goal, epoch: u64) -> Result<Self, String> {
        if !crate::storage::valid_id(owner)
            || !crate::storage::valid_id(&goal.id)
            || goal.progress > goal.target
        {
            return Err("invalid autonomy goal".into());
        }
        Ok(Self {
            version: 1,
            event_id: format!("goal_{}_{}", goal.id, goal.progress),
            owner_id: owner.into(),
            goal: Some(goal.clone()),
            events: vec![],
            participants: vec![owner.into()],
            allowed_facts: vec![],
            epoch,
        })
    }
    pub(crate) fn retrieval_query(&self) -> String {
        let Some(fact) = self.events.first() else {
            return self.goal.as_ref().map(|goal|format!("OrbitPal current local goal {:?}, confirmed progress {} of {}; not an experienced event",goal.kind,goal.progress,goal.target)).unwrap_or_default();
        };
        let kind = match fact.kind {
            SceneKind::Greeting => "greeting",
            SceneKind::Play => "shared play",
            SceneKind::Rest => "shared rest",
        };
        let outcome = match fact.outcome {
            Outcome::Completed => "confirmed completed",
            Outcome::Cancelled => "cancelled without shared success",
            Outcome::Declined => "invitation declined",
            Outcome::Invited => "invitation only, not completed",
        };
        format!(
            "OrbitPal event {}: {} {}; participants {} {}",
            fact.event_id, kind, outcome, fact.participants[0], fact.participants[1]
        )
    }
    pub(crate) fn prompt(&self) -> String {
        format!("NATIVE AUTONOMY CONTEXT, observation only. Do not invent events, change needs/relationships, grant access or issue actions. Silence is allowed. Only these own/explicitly shared facts may describe the other character: {}",serde_json::to_string(self).unwrap_or_default())
    }
    pub(crate) fn current(&self, store: &Store, bot: &str) -> Result<(), String> {
        if bot != self.owner_id || store.social_epoch()? != self.epoch {
            return Err("autonomy context changed".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retrieval_uses_confirmed_event_and_cancel_is_not_success() {
        let m = Memory {
            event_id: "scene_1_cancelled".into(),
            sequence: 1,
            kind: SceneKind::Play,
            outcome: Outcome::Cancelled,
            participants: ["nova".into(), "blue".into()],
            at_ms: 1,
            origin: "own".into(),
            shared: false,
        };
        let c = AutonomyContext::new("nova", &m, None, 0).unwrap();
        assert!(c
            .retrieval_query()
            .contains("cancelled without shared success"));
        assert!(!c.prompt().contains("PERSONA.md"));
    }
    #[test]
    fn third_actor_requires_explicit_shared_fact() {
        let mut m = Memory {
            event_id: "scene_1_completed".into(),
            sequence: 1,
            kind: SceneKind::Play,
            outcome: Outcome::Completed,
            participants: ["nova".into(), "blue".into()],
            at_ms: 1,
            origin: "own".into(),
            shared: false,
        };
        assert!(AutonomyContext::new("third", &m, None, 0).is_err());
        m.origin = "shared".into();
        assert!(AutonomyContext::new("third", &m, None, 0).is_err());
        m.shared = true;
        assert!(AutonomyContext::new("third", &m, None, 0).is_ok());
    }
}
