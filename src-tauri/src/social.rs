//! Native two-participant scene protocol. The host owns all real leases/actions.
use crate::{
    character_development::Character,
    life_domain::{
        ActionId, ActionPhase, ActorId, CommittedEvent, EventId, LifeState, Needs, WorldId,
        MAX_COUNTER,
    },
    world_state::SlotId,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
const MAX_SCENES: usize = 32;
const MAX_DIRECTIVES: usize = 256;
const WAIT_MS: u64 = 30_000;
const COOLDOWN_MS: u64 = 60_000;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SceneKind {
    Greeting,
    Play,
    Rest,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Invitation,
    Accepted,
    ResourcesReserved,
    Approach,
    Execution,
    Completed,
    Cancelled,
    Declined,
}
impl Phase {
    pub(crate) fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Declined)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reason {
    Declined,
    Timeout,
    Unavailable,
    Operator,
    Restart,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SceneView {
    pub id: String,
    pub sequence: u64,
    pub kind: SceneKind,
    pub phase: Phase,
    pub participants: [String; 2],
    pub inviter_id: String,
    pub at_ms: u64,
    pub deadline_ms: u64,
    pub reason: Option<Reason>,
}
impl SceneView {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.sequence == 0
            || self.sequence > MAX_COUNTER
            || self.id != format!("scene_{}", self.sequence)
            || self.participants[0] == self.participants[1]
            || self
                .participants
                .iter()
                .any(|id| !crate::storage::valid_id(id))
            || !self.participants.contains(&self.inviter_id)
            || self.at_ms > self.deadline_ms
            || self.deadline_ms > MAX_COUNTER
        {
            return Err("invalid social scene".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Participant {
    pub actor: ActorId,
    pub world: WorldId,
    pub session: String,
    pub generation: u64,
    pub available: bool,
    pub needs: Needs,
    pub character: Character,
    pub familiarity: u16,
    pub affection: i16,
}
impl Participant {
    fn current(&self, other: &Self) -> bool {
        self.actor == other.actor
            && self.world == other.world
            && self.session == other.session
            && self.generation == other.generation
            && other.available
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Invited,
    Declined,
    Cancelled,
    Completed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SocialEvent {
    sequence: u64,
    event_id: String,
    kind: SceneKind,
    outcome: Outcome,
    participants: [String; 2],
    at_ms: u64,
    reason: Option<Reason>,
    actions: Option<[ActionId; 2]>,
    source_events: Option<[EventId; 2]>,
}
impl SocialEvent {
    fn from_scene(scene: &SceneView, outcome: Outcome, now: u64) -> Self {
        Self {
            sequence: scene.sequence,
            event_id: format!("scene_{}_{outcome:?}", scene.sequence).to_lowercase(),
            kind: scene.kind,
            outcome,
            participants: scene.participants.clone(),
            at_ms: now,
            reason: scene.reason,
            actions: None,
            source_events: None,
        }
    }
    pub(crate) fn interrupted(scene: &SceneView, now: u64) -> Self {
        let mut scene = scene.clone();
        scene.reason = Some(Reason::Restart);
        Self::from_scene(&scene, Outcome::Cancelled, now)
    }
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }
    pub(crate) fn id(&self) -> &str {
        &self.event_id
    }
    pub(crate) fn kind(&self) -> SceneKind {
        self.kind
    }
    pub(crate) fn outcome(&self) -> Outcome {
        self.outcome
    }
    pub(crate) fn participants(&self) -> &[String; 2] {
        &self.participants
    }
    pub(crate) fn at_ms(&self) -> u64 {
        self.at_ms
    }
    pub(crate) fn actions(&self) -> Option<&[ActionId; 2]> {
        self.actions.as_ref()
    }
    pub(crate) fn source_events(&self) -> Option<&[EventId; 2]> {
        self.source_events.as_ref()
    }
}
#[derive(Clone, Debug)]
pub(crate) enum Directive {
    Reserve {
        scene: SceneView,
        slots: [SlotId; 2],
    },
    Approach {
        scene: SceneView,
        slots: [SlotId; 2],
    },
    Perform {
        scene: SceneView,
        slots: [SlotId; 2],
        minimum_ms: u64,
    },
    Record {
        event: SocialEvent,
    },
    Terminal {
        scene: SceneView,
        event: SocialEvent,
        slots: [SlotId; 2],
    },
}
#[derive(Clone, Debug)]
struct Running {
    view: SceneView,
    actors: [Participant; 2],
    slots: [SlotId; 2],
    actions: [Option<ActionId>; 2],
    source_events: [Option<EventId>; 2],
    done: [bool; 2],
    execute_at: u64,
    cleanup_pending: bool,
}
#[derive(Clone, Default)]
pub(crate) struct Coordinator {
    scenes: BTreeMap<String, Running>,
    busy: BTreeMap<String, String>,
    cooldown: BTreeMap<(String, String), u64>,
    outbox: VecDeque<Directive>,
    last_ms: u64,
}
impl Coordinator {
    fn transact<T>(
        &mut self,
        now: u64,
        f: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        if now < self.last_ms || now > MAX_COUNTER {
            return Err("invalid social clock".into());
        }
        let mut next = self.clone();
        next.last_ms = now;
        next.cooldown.retain(|_, until| *until > now);
        let result = f(&mut next)?;
        if next.outbox.len() > MAX_DIRECTIVES {
            return Err("social output capacity".into());
        }
        *self = next;
        Ok(result)
    }
    pub(crate) fn busy(&self, bot: &str) -> bool {
        self.busy.contains_key(bot)
    }
    pub(crate) fn views(&self, bot: &str) -> Vec<SceneView> {
        self.scenes
            .values()
            .filter(|s| s.view.participants.iter().any(|id| id == bot))
            .map(|s| s.view.clone())
            .collect()
    }
    pub(crate) fn drain(&mut self) -> Vec<Directive> {
        self.outbox.drain(..).collect()
    }
    /// A batch is sorted before admission, so simultaneous invitations have a fixed winner.
    pub(crate) fn invite_batch(
        &mut self,
        mut offers: Vec<(u64, SceneKind, [Participant; 2], [SlotId; 2])>,
        now: u64,
    ) -> Vec<Result<SceneView, String>> {
        offers.sort_by_key(|(_, _, a, _)| {
            (
                a[0].actor.as_str().to_owned(),
                a[1].actor.as_str().to_owned(),
            )
        });
        offers
            .into_iter()
            .map(|(seq, kind, actors, slots)| self.invite(seq, kind, actors, slots, now))
            .collect()
    }
    pub(crate) fn invite(
        &mut self,
        sequence: u64,
        kind: SceneKind,
        actors: [Participant; 2],
        slots: [SlotId; 2],
        now: u64,
    ) -> Result<SceneView, String> {
        self.transact(now, |next| {
            let ids = [
                actors[0].actor.as_str().to_owned(),
                actors[1].actor.as_str().to_owned(),
            ];
            let pair = pair(&ids);
            if sequence == 0
                || sequence > MAX_COUNTER
                || ids[0] == ids[1]
                || slots[0] == slots[1]
                || actors.iter().any(|a| {
                    !a.available
                        || !crate::storage::valid_id(&a.session)
                        || a.generation == 0
                        || a.generation > MAX_COUNTER
                        || a.character.validate().is_err()
                        || a.familiarity > 10000
                        || !(-10000..=10000).contains(&a.affection)
                })
                || ids.iter().any(|id| next.busy.contains_key(id))
                || next.cooldown.contains_key(&pair)
            {
                return Err("social invitation unavailable".into());
            }
            if next.scenes.len() == MAX_SCENES {
                let old = next
                    .scenes
                    .iter()
                    .find(|(_, s)| s.view.phase.terminal() && !s.cleanup_pending)
                    .map(|(id, _)| id.clone())
                    .ok_or("social scene capacity")?;
                next.scenes.remove(&old);
            }
            let deadline = now
                .checked_add(WAIT_MS)
                .filter(|n| *n <= MAX_COUNTER)
                .ok_or("social clock exhausted")?;
            let view = SceneView {
                id: format!("scene_{sequence}"),
                sequence,
                kind,
                phase: Phase::Invitation,
                participants: ids.clone(),
                inviter_id: ids[0].clone(),
                at_ms: now,
                deadline_ms: deadline,
                reason: None,
            };
            if next.scenes.contains_key(&view.id) {
                return Err("social identity already used".into());
            }
            for id in ids {
                next.busy.insert(id, view.id.clone());
            }
            next.outbox.push_back(Directive::Record {
                event: SocialEvent::from_scene(&view, Outcome::Invited, now),
            });
            next.scenes.insert(
                view.id.clone(),
                Running {
                    view: view.clone(),
                    actors,
                    slots,
                    actions: [None, None],
                    source_events: [None, None],
                    done: [false; 2],
                    execute_at: 0,
                    cleanup_pending: false,
                },
            );
            Ok(view)
        })
    }
    pub(crate) fn auto_respond(&mut self, id: &str, now: u64) -> Result<bool, String> {
        let scene = self.scenes.get(id).ok_or("unknown social scene")?;
        let guest = &scene.actors[1];
        let score = i32::from(guest.character.traits.sociability)
            + i32::from(guest.character.interests.company)
            + i32::from(guest.familiarity / 10)
            + (i32::from(guest.affection) / 20).clamp(-500, 500);
        let accept = guest.available
            && guest.needs.energy().units() >= 2500
            && guest.needs.nourishment().units() >= 2000
            && score >= 900;
        self.respond(id, accept, now)?;
        Ok(accept)
    }
    pub(crate) fn respond(&mut self, id: &str, accept: bool, now: u64) -> Result<(), String> {
        self.transact(now, |next| {
            let phase = next
                .scenes
                .get(id)
                .ok_or("unknown social scene")?
                .view
                .phase;
            if phase != Phase::Invitation {
                return Ok(());
            }
            if !accept {
                return next.abort(id, Reason::Declined, now);
            }
            let scene = next.scenes.get_mut(id).unwrap();
            scene.view.phase = Phase::Accepted;
            next.outbox.push_back(Directive::Reserve {
                scene: scene.view.clone(),
                slots: scene.slots.clone(),
            });
            Ok(())
        })
    }
    pub(crate) fn reserved(&mut self, id: &str, success: bool, now: u64) -> Result<(), String> {
        self.transact(now, |next| {
            if next
                .scenes
                .get(id)
                .ok_or("unknown social scene")?
                .view
                .phase
                != Phase::Accepted
            {
                return Ok(());
            }
            if !success {
                return next.abort(id, Reason::Unavailable, now);
            }
            let scene = next.scenes.get_mut(id).unwrap();
            scene.view.phase = Phase::ResourcesReserved;
            scene.view.phase = Phase::Approach;
            next.outbox.push_back(Directive::Approach {
                scene: scene.view.clone(),
                slots: scene.slots.clone(),
            });
            Ok(())
        })
    }
    /// Only actual native Plan readback may be bound. Wire IDs cannot call this seam.
    pub(crate) fn bind_action(
        &mut self,
        id: &str,
        confirmed: &LifeState,
        now: u64,
    ) -> Result<(), String> {
        self.transact(now, |next| {
            let scene = next.scenes.get_mut(id).ok_or("unknown social scene")?;
            let i = scene
                .actors
                .iter()
                .position(|a| a.actor == *confirmed.actor_id() && a.world == *confirmed.world_id())
                .ok_or("social actor changed")?;
            let action = confirmed
                .current_action()
                .filter(|a| matches!(a.phase(), ActionPhase::Planned | ActionPhase::Started))
                .ok_or("social action not planned")?;
            let expected = if scene.view.phase == Phase::Approach {
                "move_to"
            } else if scene.view.phase == Phase::Execution {
                match scene.view.kind {
                    SceneKind::Greeting => "socialize",
                    SceneKind::Play => "play",
                    SceneKind::Rest => "rest",
                }
            } else {
                return Err("social phase changed".into());
            };
            if serde_json::to_value(action.intent().activity())
                .ok()
                .as_ref()
                .and_then(|v| v.as_str())
                != Some(expected)
            {
                return Err("social action kind changed".into());
            }
            if scene.actions[i]
                .as_ref()
                .is_some_and(|old| old != action.intent().id())
            {
                return Err("social action already bound".into());
            }
            scene.actions[i] = Some(action.intent().id().clone());
            Ok(())
        })
    }
    pub(crate) fn observe(
        &mut self,
        id: &str,
        confirmed: &LifeState,
        now: u64,
    ) -> Result<(), String> {
        self.transact(now,|next|{let scene=next.scenes.get_mut(id).ok_or("unknown social scene")?;if scene.view.phase.terminal(){return Ok(());}let i=scene.actors.iter().position(|a|a.actor==*confirmed.actor_id()&&a.world==*confirmed.world_id()).ok_or("social actor changed")?;let action=confirmed.current_action().ok_or("missing social action")?;if scene.actions[i].as_ref()!=Some(action.intent().id()){return Ok(());}if action.phase()==ActionPhase::Cancelled{return next.abort(id,Reason::Unavailable,now);}if action.phase()!=ActionPhase::Completed{return Ok(());}let event=confirmed.recent_events().find(|record|matches!(record.event(),CommittedEvent::Completed{action_id} if action_id==action.intent().id())).ok_or("completed scene evidence unavailable")?;scene.source_events[i]=Some(event.id().clone());scene.done[i]=true;next.advance(id,now)})
    }
    fn advance(&mut self, id: &str, now: u64) -> Result<(), String> {
        let scene = self.scenes.get_mut(id).ok_or("unknown social scene")?;
        if !scene.done.iter().all(|done| *done) {
            return Ok(());
        }
        if scene.view.phase == Phase::Approach {
            scene.view.phase = Phase::Execution;
            scene.execute_at = now;
            scene.actions = [None, None];
            scene.source_events = [None, None];
            scene.done = [false; 2];
            scene.view.deadline_ms = now
                .checked_add(duration(scene.view.kind) + WAIT_MS)
                .filter(|n| *n <= MAX_COUNTER)
                .ok_or("social clock exhausted")?;
            self.outbox.push_back(Directive::Perform {
                scene: scene.view.clone(),
                slots: scene.slots.clone(),
                minimum_ms: duration(scene.view.kind),
            });
        } else if scene.view.phase == Phase::Execution
            && now.saturating_sub(scene.execute_at) >= duration(scene.view.kind)
        {
            scene.view.phase = Phase::Completed;
            scene.cleanup_pending = true;
            let mut event = SocialEvent::from_scene(&scene.view, Outcome::Completed, now);
            event.actions = Some([
                scene.actions[0].clone().ok_or("missing scene action")?,
                scene.actions[1].clone().ok_or("missing scene action")?,
            ]);
            event.source_events = Some([
                scene.source_events[0]
                    .clone()
                    .ok_or("missing scene event")?,
                scene.source_events[1]
                    .clone()
                    .ok_or("missing scene event")?,
            ]);
            self.outbox.push_back(Directive::Terminal {
                scene: scene.view.clone(),
                event,
                slots: scene.slots.clone(),
            });
        }
        Ok(())
    }
    fn abort(&mut self, id: &str, reason: Reason, now: u64) -> Result<(), String> {
        let scene = self.scenes.get_mut(id).ok_or("unknown social scene")?;
        if scene.view.phase.terminal() {
            return Ok(());
        }
        scene.view.phase = if reason == Reason::Declined {
            Phase::Declined
        } else {
            Phase::Cancelled
        };
        scene.view.reason = Some(reason);
        scene.cleanup_pending = true;
        let until = now
            .checked_add(COOLDOWN_MS)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or("social clock exhausted")?;
        if self.cooldown.len() >= 64 && !self.cooldown.contains_key(&pair(&scene.view.participants))
        {
            return Err("social cooldown capacity".into());
        }
        self.cooldown.insert(pair(&scene.view.participants), until);
        self.outbox.push_back(Directive::Terminal {
            scene: scene.view.clone(),
            event: SocialEvent::from_scene(
                &scene.view,
                if reason == Reason::Declined {
                    Outcome::Declined
                } else {
                    Outcome::Cancelled
                },
                now,
            ),
            slots: scene.slots.clone(),
        });
        Ok(())
    }
    pub(crate) fn cancel(&mut self, id: &str, reason: Reason, now: u64) -> Result<(), String> {
        self.transact(now, |next| next.abort(id, reason, now))
    }
    pub(crate) fn reconcile(&mut self, actors: &[Participant], now: u64) -> Result<(), String> {
        self.transact(now, |next| {
            let ids = next.scenes.keys().cloned().collect::<Vec<_>>();
            for id in ids {
                let scene = next.scenes.get(&id).unwrap();
                if scene.view.phase.terminal() {
                    continue;
                }
                let invalid = scene.actors.iter().any(|old| {
                    actors
                        .iter()
                        .find(|new| new.actor == old.actor)
                        .is_none_or(|new| !old.current(new))
                });
                if invalid {
                    next.abort(&id, Reason::Unavailable, now)?;
                } else if now >= scene.view.deadline_ms {
                    next.abort(&id, Reason::Timeout, now)?;
                } else {
                    next.advance(&id, now)?;
                }
            }
            Ok(())
        })
    }
    /// Call only after real action cancellation and all exact World leases released.
    pub(crate) fn released(&mut self, id: &str) -> Result<(), String> {
        let scene = self.scenes.get_mut(id).ok_or("unknown social scene")?;
        if !scene.view.phase.terminal() {
            return Err("scene still running".into());
        }
        scene.cleanup_pending = false;
        for actor in &scene.view.participants {
            if self.busy.get(actor).is_some_and(|current| current == id) {
                self.busy.remove(actor);
            }
        }
        Ok(())
    }
}
fn pair(ids: &[String; 2]) -> (String, String) {
    if ids[0] < ids[1] {
        (ids[0].clone(), ids[1].clone())
    } else {
        (ids[1].clone(), ids[0].clone())
    }
}
fn duration(kind: SceneKind) -> u64 {
    match kind {
        SceneKind::Greeting => 1000,
        SceneKind::Play => 1000,
        SceneKind::Rest => 10000,
    }
}

#[cfg(test)]
#[path = "social/tests.rs"]
pub(crate) mod tests;
