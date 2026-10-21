//! Optional short dialogue over confirmed scenes. Provider output is data only.
use crate::{
    life_domain::MAX_COUNTER,
    social::{Outcome, SceneKind, SocialEvent},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Policy {
    pub model_enabled: bool,
    /// User-declared conservative rate; a planning estimate, never an invoice guarantee.
    pub estimated_micros_per_million: Option<u64>,
    pub max_turns: u8,
    pub duration_ms: u64,
    pub token_limit: u64,
    pub cost_limit_micros: u64,
    pub daily_token_limit: u64,
    pub daily_cost_limit_micros: u64,
    pub cooldown_ms: u64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            model_enabled: false,
            estimated_micros_per_million: None,
            max_turns: 2,
            duration_ms: 30000,
            token_limit: 300000,
            cost_limit_micros: 10000,
            daily_token_limit: 1000000,
            daily_cost_limit_micros: 100000,
            cooldown_ms: 60000,
        }
    }
}
impl Policy {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self
            .estimated_micros_per_million
            .is_some_and(|rate| rate == 0 || rate > 1_000_000_000)
            || !(1..=4).contains(&self.max_turns)
            || !(1000..=60000).contains(&self.duration_ms)
            || self.token_limit > 1000000
            || self.cost_limit_micros > 1000000
            || self.daily_token_limit > 10000000
            || self.daily_cost_limit_micros > 10000000
            || !(1000..=3600000).contains(&self.cooldown_ms)
        {
            return Err("invalid social dialogue policy".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Admission {
    pub text: bool,
    pub voice: bool,
    pub model: bool,
    pub preset_generation: u64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Quote {
    pub tokens: u64,
    pub cost_micros: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FallbackReason {
    Disabled,
    Unavailable,
    Budget,
    Timeout,
    InvalidReply,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reaction {
    None,
    Happy,
    Wave,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reply {
    pub text: String,
    pub reaction: Reaction,
}
impl Reply {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() > 2048 {
            return Err("social reply too long".into());
        }
        let reply: Self = serde_json::from_str(raw).map_err(|_| "invalid social reply")?;
        if reply.text.trim().is_empty()
            || reply.text.chars().count() > 240
            || reply.text.chars().any(|c| c.is_control())
        {
            return Err("invalid social text".into());
        }
        Ok(reply)
    }
}
#[derive(Clone)]
pub(crate) struct Turn {
    pub conversation_id: String,
    pub event_id: String,
    pub speaker: String,
    pub participants: [String; 2],
    pub kind: SceneKind,
    pub index: u8,
    pub deadline_ms: u64,
    pub preset_generation: u64,
    pub model: bool,
    pub fallback_reason: Option<FallbackReason>,
    cancel: Arc<AtomicBool>,
    bound: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
}
impl std::fmt::Debug for Turn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Turn")
            .field("conversation_id", &self.conversation_id)
            .field("index", &self.index)
            .finish()
    }
}
impl Turn {
    pub(crate) fn current(&self, now: u64, generation: u64) -> bool {
        !self.cancel.load(Ordering::SeqCst)
            && self
                .bound
                .lock()
                .is_ok_and(|bound| bound.iter().all(|token| !token.load(Ordering::SeqCst)))
            && now <= self.deadline_ms
            && generation == self.preset_generation
    }
    pub(crate) fn bind(&self, cancel: Arc<AtomicBool>) -> Result<(), String> {
        let mut bound = self
            .bound
            .lock()
            .map_err(|_| "social cancellation unavailable")?;
        if self.cancel.load(Ordering::SeqCst) || bound.len() >= 2 {
            cancel.store(true, Ordering::SeqCst);
            return Err("social turn cancelled".into());
        }
        bound.push(cancel);
        Ok(())
    }
    pub(crate) fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(bound) = self.bound.lock() {
            for cancel in bound.iter() {
                cancel.store(true, Ordering::SeqCst);
            }
        }
    }
    pub(crate) fn local(&self, locale: &str) -> Reply {
        let second = self.index % 2 == 1;
        let text = match (locale, self.kind, second) {
            ("de", SceneKind::Play, false) => "Das gemeinsame Spiel hat Spaß gemacht.",
            ("de", SceneKind::Play, true) => "Ja, danke fürs Mitspielen!",
            ("de", SceneKind::Rest, _) => "Die gemeinsame Pause war angenehm.",
            ("de", SceneKind::Greeting, _) => "Schön, dich zu sehen!",
            ("fr", SceneKind::Play, _) => "C’était agréable de jouer ensemble.",
            ("fr", SceneKind::Rest, _) => "Cette pause ensemble était agréable.",
            ("fr", SceneKind::Greeting, _) => "Content de te voir !",
            ("es", SceneKind::Play, _) => "Ha sido divertido jugar juntos.",
            ("es", SceneKind::Rest, _) => "La pausa juntos ha sido agradable.",
            ("es", SceneKind::Greeting, _) => "¡Qué gusto verte!",
            (_, SceneKind::Play, false) => "That was a fun game together.",
            (_, SceneKind::Play, true) => "Yes, thanks for playing!",
            (_, SceneKind::Rest, _) => "That was a pleasant break together.",
            _ => "Good to see you!",
        };
        Reply {
            text: text.into(),
            reaction: Reaction::None,
        }
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub(crate) fn cancel_token(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub(crate) fn prompt(&self) -> String {
        format!("Reply as {} in one short line about the confirmed {:?} event {} with {} and {}. Only JSON {{\"text\":\"...\",\"reaction\":\"none\"|\"happy\"|\"wave\"}}; at most240 characters. No tools, actions, state changes or new participants.",self.speaker,self.kind,self.event_id,self.participants[0],self.participants[1])
    }
}
#[derive(Clone)]
struct Conversation {
    event_id: String,
    participants: [String; 2],
    kind: SceneKind,
    policy: Policy,
    next: u8,
    deadline: u64,
    preset: u64,
    tokens: u64,
    cost: u64,
    pending: Option<Turn>,
}
#[derive(Default)]
pub(crate) struct DialogueHub {
    conversations: BTreeMap<String, Conversation>,
    cooldowns: BTreeMap<(String, String), u64>,
    seen: BTreeMap<(String, String), u64>,
    day: u64,
    tokens: u64,
    cost: u64,
    last_ms: u64,
}
impl DialogueHub {
    pub(crate) fn next_speaker(&self, id: &str) -> Option<&str> {
        let c = self.conversations.get(id)?;
        (c.pending.is_none() && c.next < c.policy.max_turns)
            .then(|| c.participants[c.next as usize % 2].as_str())
    }

    pub(crate) fn start(
        &mut self,
        event: &SocialEvent,
        policy: Policy,
        admission: Admission,
        now: u64,
    ) -> Result<String, String> {
        policy.validate()?;
        if now < self.last_ms
            || now > MAX_COUNTER
            || event.outcome() != Outcome::Completed
            || event.actions().is_none()
            || event.source_events().is_none()
            || !admission.text
            || self.conversations.len() >= 16
        {
            return Err("social conversation unavailable".into());
        }
        let id = format!("dialogue_{}", event.sequence());
        if self.conversations.contains_key(&id) {
            return Ok(id);
        }
        self.cooldowns.retain(|_, until| *until > now);
        let mut ids = event.participants().clone();
        ids.sort();
        let pair = (ids[0].clone(), ids[1].clone());
        if self
            .seen
            .get(&pair)
            .is_some_and(|sequence| *sequence >= event.sequence())
        {
            return Err("social event already discussed".into());
        }
        if self.seen.len() >= 64 && !self.seen.contains_key(&pair) {
            return Err("social history capacity".into());
        }
        if self.cooldowns.contains_key(&pair) {
            return Err("social conversation cooling down".into());
        }
        if self.cooldowns.len() >= 64 {
            return Err("social cooldown capacity".into());
        }
        let deadline = now
            .checked_add(policy.duration_ms)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or("social clock exhausted")?;
        let cooldown = now
            .checked_add(policy.cooldown_ms)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or("social clock exhausted")?;
        self.last_ms = now;
        self.seen.insert(pair.clone(), event.sequence());
        self.cooldowns.insert(pair, cooldown);
        self.conversations.insert(
            id.clone(),
            Conversation {
                event_id: event.id().into(),
                participants: event.participants().clone(),
                kind: event.kind(),
                policy,
                next: 0,
                deadline,
                preset: admission.preset_generation,
                tokens: 0,
                cost: 0,
                pending: None,
            },
        );
        Ok(id)
    }
    pub(crate) fn next(
        &mut self,
        id: &str,
        admission: Admission,
        quote: Quote,
        now: u64,
    ) -> Result<Option<Turn>, String> {
        if now < self.last_ms || now > MAX_COUNTER {
            return Err("invalid dialogue clock".into());
        }
        self.last_ms = now;
        let day = now / 86400000;
        if day > self.day {
            self.day = day;
            self.tokens = 0;
            self.cost = 0;
        }
        let c = self
            .conversations
            .get_mut(id)
            .ok_or("unknown social conversation")?;
        if !admission.text || now >= c.deadline || admission.preset_generation != c.preset {
            if let Some(turn) = c.pending.take() {
                turn.stop();
            }
            self.conversations.remove(id);
            return Ok(None);
        }
        if c.pending.is_some() {
            return Ok(None);
        }
        if c.next >= c.policy.max_turns {
            self.conversations.remove(id);
            return Ok(None);
        }
        let cost = quote.cost_micros;
        let budget = quote.tokens > 0
            && cost.is_some()
            && c.tokens
                .checked_add(quote.tokens)
                .is_some_and(|v| v <= c.policy.token_limit)
            && self
                .tokens
                .checked_add(quote.tokens)
                .is_some_and(|v| v <= c.policy.daily_token_limit)
            && c.cost
                .checked_add(cost.unwrap_or(0))
                .is_some_and(|v| v <= c.policy.cost_limit_micros)
            && self
                .cost
                .checked_add(cost.unwrap_or(0))
                .is_some_and(|v| v <= c.policy.daily_cost_limit_micros);
        let model = c.policy.model_enabled && admission.model && budget;
        let reason = if !c.policy.model_enabled {
            Some(FallbackReason::Disabled)
        } else if !admission.model {
            Some(FallbackReason::Unavailable)
        } else if !budget {
            Some(FallbackReason::Budget)
        } else {
            None
        };
        if model {
            c.tokens += quote.tokens;
            c.cost += cost.unwrap();
            self.tokens += quote.tokens;
            self.cost += cost.unwrap();
        }
        let turn = Turn {
            conversation_id: id.into(),
            event_id: c.event_id.clone(),
            speaker: c.participants[c.next as usize % 2].clone(),
            participants: c.participants.clone(),
            kind: c.kind,
            index: c.next,
            deadline_ms: c.deadline.min(now.saturating_add(10000)),
            preset_generation: c.preset,
            model,
            fallback_reason: reason,
            cancel: Arc::new(AtomicBool::new(false)),
            bound: Arc::new(Mutex::new(vec![])),
        };
        c.pending = Some(turn.clone());
        Ok(Some(turn))
    }
    /// Call after actual presentation start/completion, not after model text arrives.
    pub(crate) fn presented(
        &mut self,
        turn: &Turn,
        now: u64,
        generation: u64,
    ) -> Result<(), String> {
        let c = self
            .conversations
            .get_mut(&turn.conversation_id)
            .ok_or("social conversation ended")?;
        if !turn.current(now, generation)
            || c.pending
                .as_ref()
                .is_none_or(|p| p.index != turn.index || !Arc::ptr_eq(&p.cancel, &turn.cancel))
        {
            return Err("late social turn".into());
        }
        c.pending.take();
        c.next += 1;
        Ok(())
    }
    pub(crate) fn finish(&mut self, id: &str) {
        if let Some(c) = self.conversations.remove(id) {
            if let Some(turn) = c.pending {
                turn.stop();
            }
        }
    }
    pub(crate) fn cancel_all(&mut self) {
        for (_, c) in std::mem::take(&mut self.conversations) {
            if let Some(turn) = c.pending {
                turn.stop();
            }
        }
    }
    pub(crate) fn cancel_actor(&mut self, actor: &str) {
        let ids: Vec<_> = self
            .conversations
            .iter()
            .filter(|(_, c)| c.participants.iter().any(|id| id == actor))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(c) = self.conversations.remove(&id) {
                if let Some(turn) = c.pending {
                    turn.stop();
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_reply_is_closed_bounded_data() {
        assert!(Reply::parse("{\"text\":\"hello\",\"reaction\":\"wave\"}").is_ok());
        for bad in [
            "{\"text\":\"hi\",\"reaction\":\"run_tool\"}",
            "{\"text\":\"hi\",\"reaction\":\"none\",\"action\":\"move\"}",
            "{\"text\":\"\",\"reaction\":\"none\"}",
        ] {
            assert!(Reply::parse(bad).is_err());
        }
    }
    #[test]
    fn policy_limits_are_real_and_opt_in_is_default_off() {
        let mut p = Policy::default();
        assert!(!p.model_enabled);
        p.max_turns = 5;
        assert!(p.validate().is_err());
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    fn event() -> SocialEvent {
        let ids = ["nova", "blue"];
        let (_, _, events, _) =
            crate::social::tests::completed(1, ids, crate::social::tests::states(ids));
        events.last().unwrap().clone()
    }
    fn admission() -> Admission {
        Admission {
            text: true,
            voice: false,
            model: true,
            preset_generation: 7,
        }
    }
    #[test]
    fn only_real_completed_scene_starts_and_each_turn_needs_presentation() {
        let event = event();
        let mut hub = DialogueHub::default();
        let now = 200000;
        let id = hub
            .start(&event, Policy::default(), admission(), now)
            .unwrap();
        assert_eq!(hub.next_speaker(&id), Some("nova"));
        let turn = hub
            .next(
                &id,
                admission(),
                Quote {
                    tokens: 100,
                    cost_micros: Some(1),
                },
                now,
            )
            .unwrap()
            .unwrap();
        assert!(!turn.model);
        assert_eq!(turn.fallback_reason, Some(FallbackReason::Disabled));
        assert!(hub
            .next(
                &id,
                admission(),
                Quote {
                    tokens: 1,
                    cost_micros: Some(1)
                },
                now
            )
            .unwrap()
            .is_none());
        for locale in ["de", "en", "fr", "es"] {
            assert!(turn.local(locale).text.chars().count() <= 240);
        }
        hub.presented(&turn, now + 1, 7).unwrap();
        assert_eq!(hub.next_speaker(&id), Some("blue"));
        assert!(hub.presented(&turn, now + 1, 7).is_err());
        let next = hub
            .next(
                &id,
                admission(),
                Quote {
                    tokens: 100,
                    cost_micros: Some(1),
                },
                now + 2,
            )
            .unwrap()
            .unwrap();
        hub.presented(&next, now + 3, 7).unwrap();
        assert!(hub.next_speaker(&id).is_none());
        hub.finish(&id);
        assert!(hub
            .start(&event, Policy::default(), admission(), now + 4)
            .is_err());
    }
    #[test]
    fn unknown_or_exhausted_quote_uses_local_and_native_cancel_stops_bound_output() {
        let event = event();
        let mut hub = DialogueHub::default();
        let mut policy = Policy::default();
        policy.model_enabled = true;
        policy.token_limit = 100;
        policy.daily_token_limit = 100;
        let id = hub.start(&event, policy, admission(), 200000).unwrap();
        let turn = hub
            .next(
                &id,
                admission(),
                Quote {
                    tokens: 100,
                    cost_micros: None,
                },
                200000,
            )
            .unwrap()
            .unwrap();
        assert!(!turn.model);
        assert_eq!(turn.fallback_reason, Some(FallbackReason::Budget));
        let bound = Arc::new(AtomicBool::new(false));
        turn.bind(bound.clone()).unwrap();
        hub.cancel_actor("blue");
        assert!(bound.load(Ordering::SeqCst));
        assert!(!turn.current(200001, 7));
        assert!(hub.presented(&turn, 200001, 7).is_err());
    }
    #[test]
    fn model_turns_reserve_before_dispatch_and_preset_or_deadline_invalidates_pending() {
        let event = event();
        let mut hub = DialogueHub::default();
        let mut policy = Policy::default();
        policy.model_enabled = true;
        policy.token_limit = 100;
        policy.daily_token_limit = 100;
        let id = hub.start(&event, policy, admission(), 200000).unwrap();
        let turn = hub
            .next(
                &id,
                admission(),
                Quote {
                    tokens: 100,
                    cost_micros: Some(1),
                },
                200000,
            )
            .unwrap()
            .unwrap();
        assert!(turn.model);
        hub.presented(&turn, 200001, 7).unwrap();
        let second = hub
            .next(
                &id,
                admission(),
                Quote {
                    tokens: 1,
                    cost_micros: Some(1),
                },
                200002,
            )
            .unwrap()
            .unwrap();
        assert!(!second.model);
        assert_eq!(second.fallback_reason, Some(FallbackReason::Budget));
        assert!(hub.presented(&second, 200003, 8).is_err());
        assert!(!second.current(second.deadline_ms + 1, 7));
        let muted = Admission {
            text: false,
            ..admission()
        };
        assert!(hub
            .next(
                &id,
                muted,
                Quote {
                    tokens: 0,
                    cost_micros: None
                },
                200003
            )
            .unwrap()
            .is_none());
        assert!(second.cancelled());
    }
}
