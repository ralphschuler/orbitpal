//! Bounded experience policy. Proposals and facts never grant native action authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

pub const MAX_FACTS: usize = 256;
pub const MAX_TIME: u64 = 9_007_199_254_740_991;
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Quiet,
    #[default]
    Focus,
    Stream,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    pub version: u8,
    pub preset: Preset,
    pub volume: u8,
    pub initiative_cooldown_ms: u32,
    pub movement_radius: u16,
    pub allowed_animations: Vec<String>,
    pub intent_visible: bool,
    pub desktop_anchors: bool,
    pub cursor_play: bool,
    pub stream_impulses: bool,
    pub impulse_participants: Vec<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            preset: Preset::Focus,
            volume: 35,
            initiative_cooldown_ms: 60_000,
            movement_radius: 250,
            allowed_animations: vec!["idle".into(), "walk".into(), "wave".into(), "play".into()],
            intent_visible: true,
            desktop_anchors: false,
            cursor_play: false,
            stream_impulses: false,
            impulse_participants: vec![],
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        let motions = ["idle", "walk", "wave", "play", "rest", "pet", "jump", "sit"];
        if self.version != 1
            || self.volume > 100
            || !(5_000..=3_600_000).contains(&self.initiative_cooldown_ms)
            || self.movement_radius > 1000
            || self.allowed_animations.len() > motions.len()
            || self.impulse_participants.len() > 8
            || self
                .allowed_animations
                .iter()
                .any(|s| !motions.contains(&s.as_str()))
            || self.impulse_participants.iter().any(|s| !valid_id(s))
            || self
                .allowed_animations
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.allowed_animations.len()
            || self
                .impulse_participants
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.impulse_participants.len()
        {
            return Err("experience_invalid".into());
        }
        Ok(())
    }
    /// Quiet/Focus suppress every unsolicited conversational output, including model output.
    pub fn unsolicited_output(&self, paused: bool) -> bool {
        !paused && self.preset == Preset::Stream
    }
    pub fn effective_volume(&self, paused: bool) -> u8 {
        if self.unsolicited_output(paused) {
            self.volume
        } else {
            0
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub version: u8,
    pub revision: u64,
    pub preferences: Preferences,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            preferences: Preferences::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Rest,
    Snack,
    Play,
    Greeting,
    Invitation,
    Home,
    Pet,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactPhase {
    Planned,
    Started,
    Completed,
    Declined,
    Cancelled,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityFact {
    pub id: String,
    pub actor_ids: Vec<String>,
    pub kind: FactKind,
    pub phase: FactPhase,
    pub at_ms: u64,
    pub source_event_ids: Vec<String>,
    pub shared: bool,
}
impl ActivityFact {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id)
            || self.actor_ids.is_empty()
            || self.actor_ids.len() > 8
            || self.actor_ids.iter().any(|s| !valid_id(s))
            || self.actor_ids.iter().collect::<BTreeSet<_>>().len() != self.actor_ids.len()
            || self.source_event_ids.is_empty()
            || self.source_event_ids.len() > 8
            || self.source_event_ids.iter().any(|s| !valid_id(s))
            || self.at_ms > MAX_TIME
        {
            return Err("experience_invalid_fact".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    Pet,
    Play,
    Home,
    Pause,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InteractionRequest {
    pub kind: InteractionKind,
    pub target_actor_id: Option<String>,
}
impl InteractionRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .target_actor_id
            .as_deref()
            .is_some_and(|id| !valid_id(id))
        {
            Err("experience_invalid".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: u8,
    pub bot_id: String,
    pub settings: Settings,
    pub paused: bool,
    pub facts: Vec<ActivityFact>,
    pub desktop_capabilities: DesktopCapabilities,
    pub impulses: ImpulseStatus,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpulseStatus {
    pub queued: u8,
    pub accepted: u64,
    pub dropped: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCapabilities {
    pub external_windows: bool,
    pub cursor: bool,
    pub reason: &'static str,
}
impl Default for DesktopCapabilities {
    fn default() -> Self {
        Self {
            external_windows: false,
            cursor: false,
            reason: "no_verified_source",
        }
    }
}
#[derive(Default)]
pub struct ExperienceRuntime {
    paused: Mutex<BTreeSet<String>>,
    impulses: Mutex<BTreeMap<String, ImpulseQueue>>,
}
impl ExperienceRuntime {
    pub fn paused(&self, id: &str) -> Result<bool, String> {
        Ok(self
            .paused
            .lock()
            .map_err(|_| "experience_unavailable")?
            .contains(id))
    }
    pub fn admit_native_impulse(
        &self,
        owner: &str,
        event: NativeImpulse,
        generation: u64,
        now: u64,
        prefs: &Preferences,
    ) -> Result<(), ImpulseDenial> {
        if !valid_id(owner) {
            return Err(ImpulseDenial::Invalid);
        }
        let paused = self.paused(owner).map_err(|_| ImpulseDenial::Disabled)?;
        let mut queues = self.impulses.lock().map_err(|_| ImpulseDenial::Disabled)?;
        if queues.len() >= 1024 && !queues.contains_key(owner) {
            return Err(ImpulseDenial::Full);
        }
        queues
            .entry(owner.into())
            .or_default()
            .admit(event, generation, now, prefs, paused)
    }
    pub fn take_native_impulse(
        &self,
        owner: &str,
        generation: u64,
        now: u64,
        prefs: &Preferences,
    ) -> Result<Option<NativeImpulse>, String> {
        let paused = self.paused(owner)?;
        let mut queues = self.impulses.lock().map_err(|_| "experience_unavailable")?;
        Ok(queues
            .get_mut(owner)
            .and_then(|q| q.take(generation, now, prefs, paused)))
    }
    pub fn impulse_status(&self, owner: &str) -> Result<ImpulseStatus, String> {
        let queues = self.impulses.lock().map_err(|_| "experience_unavailable")?;
        Ok(queues
            .get(owner)
            .map(|q| ImpulseStatus {
                queued: q.pending.len() as u8,
                accepted: q.accepted,
                dropped: q.dropped,
            })
            .unwrap_or_default())
    }
    pub fn cancel_impulses(&self, owner: &str) -> Result<(), String> {
        if let Some(q) = self
            .impulses
            .lock()
            .map_err(|_| "experience_unavailable")?
            .get_mut(owner)
        {
            q.cancel_pending();
        }
        Ok(())
    }
    pub fn set_paused(&self, id: &str, paused: bool) -> Result<(), String> {
        if !valid_id(id) {
            return Err("experience_invalid".into());
        }
        let mut ids = self.paused.lock().map_err(|_| "experience_unavailable")?;
        if paused {
            self.cancel_impulses(id)?;
            if ids.len() >= 1024 && !ids.contains(id) {
                return Err("experience_capacity".into());
            }
            ids.insert(id.into());
        } else {
            ids.remove(id);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpulseKind {
    Greeting,
    Play,
    VoteRest,
    VotePlay,
}
#[derive(Clone, Debug)]
pub struct NativeImpulse {
    pub platform: Platform,
    pub event_id: String,
    pub connection_generation: u64,
    pub received_ms: u64,
    pub expires_ms: u64,
    pub kind: ImpulseKind,
    pub actor_ids: Vec<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Twitch,
    Youtube,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImpulseDenial {
    Invalid,
    Disabled,
    Expired,
    StaleRoute,
    Duplicate,
    Cooldown,
    Full,
}
#[derive(Default)]
pub struct ImpulseQueue {
    pending: VecDeque<NativeImpulse>,
    seen: VecDeque<(Platform, String, u64)>,
    last: BTreeMap<String, u64>,
    accepted: u64,
    dropped: u64,
}
impl ImpulseQueue {
    /// Called only by the native authenticated platform adapter, never by a viewer IPC.
    pub fn admit(
        &mut self,
        event: NativeImpulse,
        current_generation: u64,
        now: u64,
        prefs: &Preferences,
        paused: bool,
    ) -> Result<(), ImpulseDenial> {
        let result = self.admit_inner(event, current_generation, now, prefs, paused);
        if result.is_ok() {
            self.accepted = self.accepted.saturating_add(1).min(MAX_TIME)
        } else {
            self.dropped = self.dropped.saturating_add(1).min(MAX_TIME)
        }
        result
    }
    fn admit_inner(
        &mut self,
        event: NativeImpulse,
        current_generation: u64,
        now: u64,
        prefs: &Preferences,
        paused: bool,
    ) -> Result<(), ImpulseDenial> {
        if current_generation == 0
            || event.expires_ms > MAX_TIME
            || prefs.validate().is_err()
            || now > MAX_TIME
            || !valid_id(&event.event_id)
            || event.actor_ids.is_empty()
            || event.actor_ids.len() > 8
            || event
                .actor_ids
                .iter()
                .any(|id| !valid_id(id) || !prefs.impulse_participants.contains(id))
            || event.actor_ids.iter().collect::<BTreeSet<_>>().len() != event.actor_ids.len()
        {
            return Err(ImpulseDenial::Invalid);
        }
        if !prefs.stream_impulses || !prefs.unsolicited_output(paused) {
            return Err(ImpulseDenial::Disabled);
        }
        if event.connection_generation != current_generation {
            return Err(ImpulseDenial::StaleRoute);
        }
        if event.received_ms > now
            || event.expires_ms <= now
            || event.expires_ms < event.received_ms
            || event.expires_ms - event.received_ms > 30_000
        {
            return Err(ImpulseDenial::Expired);
        }
        self.pending
            .retain(|e| e.connection_generation == current_generation && e.expires_ms > now);
        if self
            .seen
            .iter()
            .any(|(p, id, _)| *p == event.platform && id == &event.event_id)
        {
            return Err(ImpulseDenial::Duplicate);
        }
        if event.actor_ids.iter().any(|id| {
            self.last.get(id).is_some_and(|last| {
                now.saturating_sub(*last) < u64::from(prefs.initiative_cooldown_ms)
            })
        }) {
            return Err(ImpulseDenial::Cooldown);
        }
        if self.pending.len() >= 16 {
            return Err(ImpulseDenial::Full);
        }
        for id in &event.actor_ids {
            self.last.insert(id.clone(), now);
        }
        self.last
            .retain(|id, _| prefs.impulse_participants.contains(id));
        self.seen
            .push_back((event.platform, event.event_id.clone(), current_generation));
        while self.seen.len() > 256 {
            self.seen.pop_front();
        }
        self.pending.push_back(event);
        Ok(())
    }
    pub fn take(
        &mut self,
        generation: u64,
        now: u64,
        prefs: &Preferences,
        paused: bool,
    ) -> Option<NativeImpulse> {
        if !prefs.stream_impulses || !prefs.unsolicited_output(paused) {
            self.pending.clear();
            return None;
        }
        while let Some(e) = self.pending.pop_front() {
            if e.connection_generation == generation
                && e.expires_ms > now
                && e.actor_ids
                    .iter()
                    .all(|id| prefs.impulse_participants.contains(id))
            {
                return Some(e);
            }
        }
        None
    }
    pub fn cancel_pending(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prefs() -> Preferences {
        Preferences {
            preset: Preset::Stream,
            stream_impulses: true,
            impulse_participants: vec!["nova".into()],
            ..Preferences::default()
        }
    }
    fn event() -> NativeImpulse {
        NativeImpulse {
            platform: Platform::Twitch,
            event_id: "event1".into(),
            connection_generation: 4,
            received_ms: 100,
            expires_ms: 20_000,
            kind: ImpulseKind::Play,
            actor_ids: vec!["nova".into()],
        }
    }
    #[test]
    fn preferences_do_not_save_activation() {
        let p = Preferences::default();
        assert!(!p.unsolicited_output(false));
        assert_eq!(p.effective_volume(false), 0);
        let json = serde_json::to_value(p).unwrap();
        assert!(json.get("active").is_none());
        assert!(json.get("paused").is_none());
    }
    #[test]
    fn preference_bounds_and_closed_fields() {
        let mut p = prefs();
        p.volume = 101;
        assert!(p.validate().is_err());
        p.volume = 50;
        p.allowed_animations.push("shell".into());
        assert!(p.validate().is_err());
        assert!(serde_json::from_str::<Preferences>(r#"{"active":true}"#).is_err());
    }
    #[test]
    fn native_impulses_recheck_pause_and_route() {
        let mut q = ImpulseQueue::default();
        let p = prefs();
        assert_eq!(
            q.admit(event(), 4, 100, &p, true),
            Err(ImpulseDenial::Disabled)
        );
        q.admit(event(), 4, 100, &p, false).unwrap();
        assert!(q.take(5, 101, &p, false).is_none());
    }
    #[test]
    fn impulse_duplicates_cooldown_and_expiry() {
        let mut q = ImpulseQueue::default();
        let p = prefs();
        q.admit(event(), 4, 100, &p, false).unwrap();
        assert_eq!(
            q.admit(event(), 4, 101, &p, false),
            Err(ImpulseDenial::Duplicate)
        );
        let mut reconnect = event();
        reconnect.connection_generation = 5;
        assert_eq!(
            q.admit(reconnect, 5, 101, &p, false),
            Err(ImpulseDenial::Duplicate)
        );
        let mut e = event();
        e.event_id = "other".into();
        assert_eq!(
            q.admit(e.clone(), 4, 102, &p, false),
            Err(ImpulseDenial::Cooldown)
        );
        e.expires_ms = 102;
        assert_eq!(q.admit(e, 4, 102, &p, false), Err(ImpulseDenial::Expired));
    }
    #[test]
    fn quiet_clears_pending_without_catchup() {
        let mut q = ImpulseQueue::default();
        q.admit(event(), 4, 100, &prefs(), false).unwrap();
        let p = Preferences::default();
        assert!(q.take(4, 101, &p, false).is_none());
        assert!(q.take(4, 102, &prefs(), false).is_none());
    }
    #[test]
    fn fact_needs_real_source_reference() {
        let f = ActivityFact {
            id: "fact".into(),
            actor_ids: vec!["nova".into()],
            kind: FactKind::Rest,
            phase: FactPhase::Completed,
            at_ms: 100,
            source_event_ids: vec![],
            shared: false,
        };
        assert!(f.validate().is_err());
    }
}
