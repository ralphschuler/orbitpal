//! Read-only child of social_host: no coordinator transition or durable-history read.
use super::SocialRuntime;
use crate::social::SceneView;

pub(crate) fn current(
    runtime: &SocialRuntime,
    bot: &str,
    session: &str,
    revision: u64,
) -> Result<Option<SceneView>, String> {
    let memory = runtime.memory.lock().map_err(|_| "experience_native_unavailable")?;
    // A terminal scene is displayed only while native cleanup is still pending.
    // Retained terminal coordinator/history rows are not a new current intention.
    let mut scenes = memory.coordinator.session_views(bot, session, revision);
    scenes.retain(|scene| !scene.phase.terminal() || memory.terminal.contains_key(&scene.id));
    Ok(scenes.into_iter().max_by_key(|scene| scene.sequence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{character_development::Character, life_domain::{ActorId, Needs, WorldId},
        social::{Participant, Reason, SceneKind}, world_state::SlotId};
    fn actor(bot: &str) -> Participant {
        Participant { actor: ActorId::new(bot).unwrap(), world: WorldId::new("world").unwrap(),
            session: format!("session_{bot}"), generation: 1, available: true,
            needs: Needs::new(5000, 5000, 5000, 5000).unwrap(), character: Character::default(),
            familiarity: 0, affection: 0 }
    }
    #[test]
    fn actual_invitation_is_exactly_bot_session_scoped_and_retained_history_is_not_current() {
        let runtime = SocialRuntime::default();
        let scene = runtime.memory.lock().unwrap().coordinator.invite(1, SceneKind::Play,
            [actor("nova"), actor("friend")], [SlotId::new("left").unwrap(), SlotId::new("right").unwrap()], 1).unwrap();
        assert_eq!(current(&runtime, "nova", "session_nova", 1).unwrap().unwrap().id, scene.id);
        assert!(current(&runtime, "other", "session_nova", 1).unwrap().is_none());
        assert!(current(&runtime, "nova", "replacement", 1).unwrap().is_none());
        assert!(current(&runtime, "nova", "session_nova", 2).unwrap().is_none());
        {
            let mut memory = runtime.memory.lock().unwrap();
            memory.coordinator.cancel(&scene.id, Reason::Operator, 2).unwrap();
            memory.take_work();
        }
        assert_eq!(current(&runtime, "nova", "session_nova", 1).unwrap().unwrap().reason, Some(Reason::Operator));
        runtime.memory.lock().unwrap().terminal.remove(&scene.id);
        assert!(current(&runtime, "nova", "session_nova", 1).unwrap().is_none());
    }
}
