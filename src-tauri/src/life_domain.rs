//! Pure, versioned character-life contracts. This module has no runtime, I/O,
//! permission or presentation owner. See docs/LIFE-DOMAIN.md for its boundaries.
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{collections::VecDeque, fmt};

pub const SCHEMA_VERSION: u16 = 1;
pub const MAX_ID_BYTES: usize = 96;
pub const NEED_SCALE: u16 = 10_000;
pub const MAX_ELAPSED_MS: u64 = 60_000;
pub const MAX_RECENT_EVENTS: usize = 128;
/// Exact in JavaScript as well as Rust; counters never wrap or saturate/reuse.
pub const MAX_COUNTER: u64 = 9_007_199_254_740_991;
pub const MAX_SIMULATION_TIME_MS: u64 = MAX_COUNTER;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifeError {
    InvalidId,
    InvalidNumber,
    OutOfRange,
    UnsupportedSchema,
    InvalidState,
    WrongActor,
    WrongWorld,
    UnknownAction,
    InvalidTransition,
    ActionBusy,
    CounterExhausted,
    ClockExhausted,
}
impl fmt::Display for LifeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Fixed categories; errors never interpolate an untrusted document.
        f.write_str(match self {
            Self::InvalidId => "invalid life identifier",
            Self::InvalidNumber => "non-finite life value",
            Self::OutOfRange => "life value outside its allowed range",
            Self::UnsupportedSchema => "unsupported life schema version",
            Self::InvalidState => "inconsistent life state",
            Self::WrongActor => "action belongs to another actor",
            Self::WrongWorld => "action belongs to another world",
            Self::UnknownAction => "action was not allocated by this actor",
            Self::InvalidTransition => "invalid action transition",
            Self::ActionBusy => "an unfinished action already exists",
            Self::CounterExhausted => "life sequence exhausted",
            Self::ClockExhausted => "simulation clock exhausted",
        })
    }
}
impl std::error::Error for LifeError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl AsRef<str>) -> Result<Self, LifeError> {
                let value = value.as_ref();
                if !valid_id(value) {
                    return Err(LifeError::InvalidId);
                }
                Ok(Self(value.to_owned()))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(de::Error::custom)
            }
        }
    };
}
identifier!(ActorId);
identifier!(WorldId);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct Sequence(u64);
impl Sequence {
    fn new(value: u64) -> Result<Self, LifeError> {
        if !(1..=MAX_COUNTER).contains(&value) {
            return Err(LifeError::OutOfRange);
        }
        Ok(Self(value))
    }
}
impl<'de> Deserialize<'de> for Sequence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionId {
    actor_id: ActorId,
    world_id: WorldId,
    ordinal: Sequence,
}
impl ActionId {
    pub fn new(actor_id: ActorId, world_id: WorldId, ordinal: u64) -> Result<Self, LifeError> {
        Ok(Self { actor_id, world_id, ordinal: Sequence::new(ordinal)? })
    }
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
    pub fn world_id(&self) -> &WorldId {
        &self.world_id
    }
    pub fn ordinal(&self) -> u64 {
        self.ordinal.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventId {
    actor_id: ActorId,
    sequence: Sequence,
}
impl EventId {
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
    pub fn sequence(&self) -> u64 {
        self.sequence.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SimulationTime(u64);
impl SimulationTime {
    pub fn new(milliseconds: u64) -> Result<Self, LifeError> {
        if milliseconds > MAX_SIMULATION_TIME_MS {
            return Err(LifeError::OutOfRange);
        }
        Ok(Self(milliseconds))
    }
    pub fn milliseconds(self) -> u64 {
        self.0
    }
}
impl<'de> Deserialize<'de> for SimulationTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Fixed-point satisfaction: 0..=10_000, not floating-point simulation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NeedLevel(u16);
impl NeedLevel {
    pub fn new(units: u16) -> Result<Self, LifeError> {
        if units > NEED_SCALE {
            return Err(LifeError::OutOfRange);
        }
        Ok(Self(units))
    }
    /// Optional numeric boundary for callers using normalized UI values.
    pub fn from_normalized(value: f64) -> Result<Self, LifeError> {
        if !value.is_finite() {
            return Err(LifeError::InvalidNumber);
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(LifeError::OutOfRange);
        }
        Self::new((value * f64::from(NEED_SCALE)).round() as u16)
    }
    pub fn units(self) -> u16 {
        self.0
    }
    fn apply(self, delta: i16) -> Self {
        Self((i32::from(self.0) + i32::from(delta)).clamp(0, i32::from(NEED_SCALE)) as u16)
    }
}
impl<'de> Deserialize<'de> for NeedLevel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u16::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Needs {
    energy: NeedLevel,
    nourishment: NeedLevel,
    engagement: NeedLevel,
    social: NeedLevel,
}
impl Needs {
    pub fn new(energy: u16, nourishment: u16, engagement: u16, social: u16) -> Result<Self, LifeError> {
        Ok(Self {
            energy: NeedLevel::new(energy)?,
            nourishment: NeedLevel::new(nourishment)?,
            engagement: NeedLevel::new(engagement)?,
            social: NeedLevel::new(social)?,
        })
    }
    pub fn energy(self) -> NeedLevel {
        self.energy
    }
    pub fn nourishment(self) -> NeedLevel {
        self.nourishment
    }
    pub fn engagement(self) -> NeedLevel {
        self.engagement
    }
    pub fn social(self) -> NeedLevel {
        self.social
    }
    fn apply(self, effect: NeedEffect) -> Self {
        Self {
            energy: self.energy.apply(effect.energy),
            nourishment: self.nourishment.apply(effect.nourishment),
            engagement: self.engagement.apply(effect.engagement),
            social: self.social.apply(effect.social),
        }
    }
    fn validate(self) -> Result<(), LifeError> {
        for level in [self.energy, self.nourishment, self.engagement, self.social] {
            NeedLevel::new(level.0)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "NeedEffectWire", into = "NeedEffectWire")]
pub struct NeedEffect {
    energy: i16,
    nourishment: i16,
    engagement: i16,
    social: i16,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NeedEffectWire {
    energy: i32,
    nourishment: i32,
    engagement: i32,
    social: i32,
}
impl NeedEffect {
    pub fn new(energy: i32, nourishment: i32, engagement: i32, social: i32) -> Result<Self, LifeError> {
        if [energy, nourishment, engagement, social]
            .iter()
            .any(|v| !(-i32::from(NEED_SCALE)..=i32::from(NEED_SCALE)).contains(v))
        {
            return Err(LifeError::OutOfRange);
        }
        Ok(Self {
            energy: energy as i16,
            nourishment: nourishment as i16,
            engagement: engagement as i16,
            social: social as i16,
        })
    }
    fn validate(self) -> Result<(), LifeError> {
        Self::new(
            i32::from(self.energy),
            i32::from(self.nourishment),
            i32::from(self.engagement),
            i32::from(self.social),
        )
        .map(|_| ())
    }
}
impl TryFrom<NeedEffectWire> for NeedEffect {
    type Error = LifeError;
    fn try_from(v: NeedEffectWire) -> Result<Self, Self::Error> {
        Self::new(v.energy, v.nourishment, v.engagement, v.social)
    }
}
impl From<NeedEffect> for NeedEffectWire {
    fn from(v: NeedEffect) -> Self {
        Self {
            energy: v.energy.into(),
            nourishment: v.nourishment.into(),
            engagement: v.engagement.into(),
            social: v.social.into(),
        }
    }
}

/// Closed vocabulary, not asset names, prompts, scripts or tool calls. Availability
/// and execution are later native-runtime/executor decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    MoveTo,
    Rest,
    Recharge,
    Snack,
    Play,
    Socialize,
    LookAt,
    Wave,
}
impl Activity {
    pub(crate) fn as_str(self) -> &'static str {
        match self { Self::MoveTo => "move_to", Self::Rest => "rest", Self::Recharge => "recharge", Self::Snack => "snack", Self::Play => "play", Self::Socialize => "socialize", Self::LookAt => "look_at", Self::Wave => "wave" }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionPhase {
    Planned,
    Started,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    Operator,
    Interrupted,
    Unavailable,
    Timeout,
    Restart,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionIntent {
    id: ActionId,
    world_id: WorldId,
    activity: Activity,
    planned_at: SimulationTime,
    variation: u16,
    completion_effect: NeedEffect,
}
impl ActionIntent {
    pub fn id(&self) -> &ActionId {
        &self.id
    }
    pub fn world_id(&self) -> &WorldId {
        &self.world_id
    }
    pub fn activity(&self) -> Activity {
        self.activity
    }
    pub fn planned_at(&self) -> SimulationTime {
        self.planned_at
    }
    pub fn variation(&self) -> u16 {
        self.variation
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ActionStateWire", into = "ActionStateWire")]
pub struct ActionState {
    intent: ActionIntent,
    phase: ActionPhase,
    started_at: Option<SimulationTime>,
    finished_at: Option<SimulationTime>,
    cancellation: Option<CancelReason>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionStateWire {
    intent: ActionIntent,
    phase: ActionPhase,
    started_at: Option<SimulationTime>,
    finished_at: Option<SimulationTime>,
    cancellation: Option<CancelReason>,
}
impl ActionState {
    pub fn intent(&self) -> &ActionIntent {
        &self.intent
    }
    pub fn phase(&self) -> ActionPhase {
        self.phase
    }
    pub fn started_at(&self) -> Option<SimulationTime> {
        self.started_at
    }
    pub fn finished_at(&self) -> Option<SimulationTime> {
        self.finished_at
    }
    pub fn cancellation(&self) -> Option<CancelReason> {
        self.cancellation
    }
    fn terminal(&self) -> bool {
        matches!(self.phase, ActionPhase::Completed | ActionPhase::Cancelled)
    }
    fn validate(&self) -> Result<(), LifeError> {
        self.intent.completion_effect.validate()?;
        let planned = self.intent.planned_at;
        if self.started_at.is_some_and(|t| t < planned)
            || self.finished_at.is_some_and(|t| t < self.started_at.unwrap_or(planned))
        {
            return Err(LifeError::InvalidState);
        }
        let valid = match self.phase {
            ActionPhase::Planned => {
                self.started_at.is_none() && self.finished_at.is_none() && self.cancellation.is_none()
            }
            ActionPhase::Started => {
                self.started_at.is_some() && self.finished_at.is_none() && self.cancellation.is_none()
            }
            ActionPhase::Completed => {
                self.started_at.is_some() && self.finished_at.is_some() && self.cancellation.is_none()
            }
            ActionPhase::Cancelled => self.finished_at.is_some() && self.cancellation.is_some(),
        };
        if valid {
            Ok(())
        } else {
            Err(LifeError::InvalidState)
        }
    }
}
impl TryFrom<ActionStateWire> for ActionState {
    type Error = LifeError;
    fn try_from(v: ActionStateWire) -> Result<Self, Self::Error> {
        let state = Self {
            intent: v.intent,
            phase: v.phase,
            started_at: v.started_at,
            finished_at: v.finished_at,
            cancellation: v.cancellation,
        };
        state.validate()?;
        Ok(state)
    }
}
impl From<ActionState> for ActionStateWire {
    fn from(v: ActionState) -> Self {
        Self {
            intent: v.intent,
            phase: v.phase,
            started_at: v.started_at,
            finished_at: v.finished_at,
            cancellation: v.cancellation,
        }
    }
}

/// Input events contain no authorization. Only a future native host may dispatch
/// them after its independent per-bot/session/capability checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LifeEvent {
    Tick {},
    ApplyNeedEffect { effect: NeedEffect },
    Plan { activity: Activity, completion_effect: NeedEffect },
    Start { action_id: ActionId },
    Complete { action_id: ActionId },
    Cancel { action_id: ActionId, reason: CancelReason },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommittedEvent {
    Advanced { elapsed_ms: u64 },
    NeedsChanged { effect: NeedEffect },
    Planned { intent: ActionIntent },
    Started { action_id: ActionId },
    Completed { action_id: ActionId },
    Cancelled { action_id: ActionId, reason: CancelReason },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeRecord {
    id: EventId,
    at: SimulationTime,
    event: CommittedEvent,
}
impl LifeRecord {
    pub fn id(&self) -> &EventId {
        &self.id
    }
    pub fn at(&self) -> SimulationTime {
        self.at
    }
    pub fn event(&self) -> &CommittedEvent {
        &self.event
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct RecentEvents(VecDeque<LifeRecord>);
impl RecentEvents {
    fn push(&mut self, event: LifeRecord) {
        if self.0.len() == MAX_RECENT_EVENTS {
            self.0.pop_front();
        }
        self.0.push_back(event);
    }
}
impl<'de> Deserialize<'de> for RecentEvents {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Bounded;
        impl<'de> de::Visitor<'de> for Bounded {
            type Value = RecentEvents;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a bounded life event history")
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = VecDeque::new();
                while let Some(value) = seq.next_element()? {
                    if values.len() == MAX_RECENT_EVENTS {
                        return Err(de::Error::custom("life event retention exceeded"));
                    }
                    values.push_back(value);
                }
                Ok(RecentEvents(values))
            }
        }
        deserializer.deserialize_seq(Bounded)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "LifeStateWire", into = "LifeStateWire")]
pub struct LifeState {
    schema_version: u16,
    actor_id: ActorId,
    world_id: WorldId,
    simulation_time: SimulationTime,
    revision: u64,
    next_action_ordinal: Sequence,
    resolved_action_ordinal: u64,
    needs: Needs,
    current_action: Option<ActionState>,
    recent_events: RecentEvents,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifeStateWire {
    schema_version: u16,
    actor_id: ActorId,
    world_id: WorldId,
    simulation_time: SimulationTime,
    revision: u64,
    next_action_ordinal: Sequence,
    resolved_action_ordinal: u64,
    needs: Needs,
    current_action: Option<ActionState>,
    recent_events: RecentEvents,
}
impl LifeState {
    pub fn new(actor_id: ActorId, world_id: WorldId, needs: Needs) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            actor_id,
            world_id,
            simulation_time: SimulationTime(0),
            revision: 0,
            next_action_ordinal: Sequence(1),
            resolved_action_ordinal: 0,
            needs,
            current_action: None,
            recent_events: RecentEvents::default(),
        }
    }
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
    pub fn world_id(&self) -> &WorldId {
        &self.world_id
    }
    pub fn simulation_time(&self) -> SimulationTime {
        self.simulation_time
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn needs(&self) -> Needs {
        self.needs
    }
    pub fn current_action(&self) -> Option<&ActionState> {
        self.current_action.as_ref()
    }
    pub fn recent_events(&self) -> impl ExactSizeIterator<Item = &LifeRecord> {
        self.recent_events.0.iter()
    }
    /// Deliberate whitelist: no needs, reward policy, RNG variation or history.
    pub fn public_projection(&self) -> PublicActor {
        PublicActor {
            schema_version: SCHEMA_VERSION,
            actor_id: self.actor_id.clone(),
            world_id: self.world_id.clone(),
            revision: self.revision,
            simulation_time: self.simulation_time,
            activity: self.current_action.as_ref().map(|a| PublicActivity {
                id: a.intent.id.clone(),
                activity: a.intent.activity,
                phase: a.phase,
            }),
        }
    }
    pub fn validate(&self) -> Result<(), LifeError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(LifeError::UnsupportedSchema);
        }
        if !valid_id(self.actor_id.as_str()) || !valid_id(self.world_id.as_str()) {
            return Err(LifeError::InvalidId);
        }
        self.needs.validate()?;
        SimulationTime::new(self.simulation_time.0)?;
        Sequence::new(self.next_action_ordinal.0)?;
        if self.resolved_action_ordinal >= self.next_action_ordinal.0
            || self.revision > MAX_COUNTER
            || self.revision < self.next_action_ordinal.0 - 1
            || self.recent_events.0.len() > MAX_RECENT_EVENTS
        {
            return Err(LifeError::InvalidState);
        }
        match &self.current_action {
            Some(action) => {
                action.validate()?;
                self.validate_intent(&action.intent)?;
                let resolved = action.intent.id.ordinal.0 - u64::from(!action.terminal());
                if self.resolved_action_ordinal != resolved {
                    return Err(LifeError::InvalidState);
                }
                if action.intent.id.ordinal.0 != self.next_action_ordinal.0 - 1
                    || action.started_at.is_some_and(|t| t > self.simulation_time)
                    || action.finished_at.is_some_and(|t| t > self.simulation_time)
                {
                    return Err(LifeError::InvalidState);
                }
            }
            None if self.next_action_ordinal.0 != 1 || self.resolved_action_ordinal != 0 => {
                return Err(LifeError::InvalidState)
            }
            None => (),
        }
        let events = &self.recent_events.0;
        if (self.revision == 0) != events.is_empty()
            || events.len() as u64 > self.revision
            || (self.revision == 0 && self.simulation_time.0 != 0)
        {
            return Err(LifeError::InvalidState);
        }
        let mut expected = self.revision - events.len() as u64;
        let mut previous_time = if expected == 0 { Some(SimulationTime(0)) } else { None };
        for record in events {
            expected += 1;
            if record.id.actor_id != self.actor_id
                || record.id.sequence.0 != expected
                || previous_time
                    .is_some_and(|previous| record.at < previous || record.at.0 - previous.0 > MAX_ELAPSED_MS)
                || record.at > self.simulation_time
            {
                return Err(LifeError::InvalidState);
            }
            match &record.event {
                CommittedEvent::Advanced { elapsed_ms }
                    if *elapsed_ms == 0
                        || *elapsed_ms > MAX_ELAPSED_MS
                        || previous_time
                            .is_some_and(|previous| previous.0.checked_add(*elapsed_ms) != Some(record.at.0))
                        || *elapsed_ms > record.at.0 =>
                {
                    return Err(LifeError::InvalidState)
                }
                CommittedEvent::NeedsChanged { effect } => effect.validate()?,
                CommittedEvent::Planned { intent } => {
                    self.validate_intent(intent)?;
                    if intent.planned_at != record.at {
                        return Err(LifeError::InvalidState);
                    }
                }
                CommittedEvent::Started { action_id }
                | CommittedEvent::Completed { action_id }
                | CommittedEvent::Cancelled { action_id, .. } => self.validate_allocated(action_id)?,
                _ => (),
            }
            previous_time = Some(record.at);
        }
        if events.back().is_some_and(|r| r.at != self.simulation_time) {
            return Err(LifeError::InvalidState);
        }
        self.validate_lifecycle_suffix()?;
        Ok(())
    }
    fn validate_lifecycle_suffix(&self) -> Result<(), LifeError> {
        // Retention may start in the middle of an action. From the first
        // retained lifecycle event onwards, every observed transition is strict.
        struct Observed<'a> {
            id: &'a ActionId,
            phase: ActionPhase,
            intent: Option<&'a ActionIntent>,
            started: Option<SimulationTime>,
            finished: Option<SimulationTime>,
            cancellation: Option<CancelReason>,
        }
        let full_history = self.recent_events.0.front().is_some_and(|record| record.id.sequence.0 == 1);
        let mut observed: Option<Observed<'_>> = None;
        for record in &self.recent_events.0 {
            match &record.event {
                CommittedEvent::Planned { intent } => {
                    if full_history && observed.is_none() && intent.id.ordinal.0 != 1 {
                        return Err(LifeError::InvalidState);
                    }
                    if observed.as_ref().is_some_and(|prior| {
                        !matches!(prior.phase, ActionPhase::Completed | ActionPhase::Cancelled)
                            || prior.id.ordinal.0.checked_add(1) != Some(intent.id.ordinal.0)
                    }) {
                        return Err(LifeError::InvalidState);
                    }
                    observed = Some(Observed {
                        id: &intent.id,
                        phase: ActionPhase::Planned,
                        intent: Some(intent),
                        started: None,
                        finished: None,
                        cancellation: None,
                    });
                }
                CommittedEvent::Started { action_id }
                | CommittedEvent::Completed { action_id }
                | CommittedEvent::Cancelled { action_id, .. } => {
                    if let Some(prior) = &observed {
                        if prior.id != action_id {
                            return Err(LifeError::InvalidState);
                        }
                        let valid = match &record.event {
                            CommittedEvent::Started { .. } => prior.phase == ActionPhase::Planned,
                            CommittedEvent::Completed { .. } => prior.phase == ActionPhase::Started,
                            CommittedEvent::Cancelled { .. } => {
                                matches!(prior.phase, ActionPhase::Planned | ActionPhase::Started)
                            }
                            _ => false,
                        };
                        if !valid {
                            return Err(LifeError::InvalidState);
                        }
                    }
                    if full_history && observed.is_none() {
                        return Err(LifeError::InvalidState);
                    }
                    let current = observed.get_or_insert(Observed {
                        id: action_id,
                        phase: ActionPhase::Planned,
                        intent: None,
                        started: None,
                        finished: None,
                        cancellation: None,
                    });
                    match &record.event {
                        CommittedEvent::Started { .. } => {
                            current.phase = ActionPhase::Started;
                            current.started = Some(record.at);
                        }
                        CommittedEvent::Completed { .. } => {
                            current.phase = ActionPhase::Completed;
                            current.finished = Some(record.at);
                        }
                        CommittedEvent::Cancelled { reason, .. } => {
                            current.phase = ActionPhase::Cancelled;
                            current.finished = Some(record.at);
                            current.cancellation = Some(*reason);
                        }
                        _ => (),
                    }
                }
                _ => (),
            }
        }
        if full_history && observed.is_none() && self.current_action.is_some() {
            return Err(LifeError::InvalidState);
        }
        if let Some(observed) = observed {
            let current = self.current_action.as_ref().ok_or(LifeError::InvalidState)?;
            if observed.id != &current.intent.id
                || observed.phase != current.phase
                || observed.intent.is_some_and(|intent| intent != &current.intent)
                || observed.started.is_some_and(|at| Some(at) != current.started_at)
                || observed.finished.is_some_and(|at| Some(at) != current.finished_at)
                || observed.cancellation.is_some_and(|reason| Some(reason) != current.cancellation)
            {
                return Err(LifeError::InvalidState);
            }
        }
        Ok(())
    }
    fn validate_allocated(&self, id: &ActionId) -> Result<(), LifeError> {
        if id.actor_id != self.actor_id {
            return Err(LifeError::WrongActor);
        }
        if id.world_id != self.world_id {
            return Err(LifeError::WrongWorld);
        }
        Sequence::new(id.ordinal.0)?;
        if id.ordinal.0 >= self.next_action_ordinal.0 {
            return Err(LifeError::UnknownAction);
        }
        Ok(())
    }
    fn validate_intent(&self, intent: &ActionIntent) -> Result<(), LifeError> {
        self.validate_allocated(&intent.id)?;
        if intent.world_id != self.world_id || intent.planned_at > self.simulation_time {
            return Err(LifeError::InvalidState);
        }
        intent.completion_effect.validate()
    }
}
impl TryFrom<LifeStateWire> for LifeState {
    type Error = LifeError;
    fn try_from(v: LifeStateWire) -> Result<Self, Self::Error> {
        let state = Self {
            schema_version: v.schema_version,
            actor_id: v.actor_id,
            world_id: v.world_id,
            simulation_time: v.simulation_time,
            revision: v.revision,
            next_action_ordinal: v.next_action_ordinal,
            resolved_action_ordinal: v.resolved_action_ordinal,
            needs: v.needs,
            current_action: v.current_action,
            recent_events: v.recent_events,
        };
        state.validate()?;
        Ok(state)
    }
}
impl From<LifeState> for LifeStateWire {
    fn from(v: LifeState) -> Self {
        Self {
            schema_version: v.schema_version,
            actor_id: v.actor_id,
            world_id: v.world_id,
            simulation_time: v.simulation_time,
            revision: v.revision,
            next_action_ordinal: v.next_action_ordinal,
            resolved_action_ordinal: v.resolved_action_ordinal,
            needs: v.needs,
            current_action: v.current_action,
            recent_events: v.recent_events,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicActivity {
    id: ActionId,
    activity: Activity,
    phase: ActionPhase,
}
impl PublicActivity {
    pub fn id(&self) -> &ActionId {
        &self.id
    }
    pub fn activity(&self) -> Activity {
        self.activity
    }
    pub fn phase(&self) -> ActionPhase {
        self.phase
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicActor {
    schema_version: u16,
    actor_id: ActorId,
    world_id: WorldId,
    revision: u64,
    simulation_time: SimulationTime,
    activity: Option<PublicActivity>,
}
impl PublicActor {
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
    pub fn world_id(&self) -> &WorldId {
        &self.world_id
    }
    pub fn activity(&self) -> Option<&PublicActivity> {
        self.activity.as_ref()
    }
}

/// SplitMix64 with an explicit caller-owned state, never an OS random source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RandomState(u64);
impl RandomState {
    pub fn seeded(seed: u64) -> Self {
        Self(seed)
    }
    pub fn state(self) -> u64 {
        self.0
    }
    fn draw(self) -> (Self, u64) {
        let state = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        (Self(state), value ^ (value >> 31))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Applied,
    NoChange,
    Duplicate,
    Stale,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub state: LifeState,
    /// At most one data intent. Emitting it neither dispatches nor starts it.
    pub intents: Vec<ActionIntent>,
    pub random: RandomState,
    pub outcome: Outcome,
    pub applied_elapsed_ms: u64,
    pub elapsed_was_limited: bool,
}
fn unchanged(state: &LifeState, random: RandomState, outcome: Outcome) -> Transition {
    Transition {
        state: state.clone(),
        intents: Vec::new(),
        random,
        outcome,
        applied_elapsed_ms: 0,
        elapsed_was_limited: false,
    }
}

/// One accepted event produces exactly one revision/record. Rejected and stale
/// events are atomic no-ops, including their elapsed time and random stream.
pub fn reduce(
    state: &LifeState,
    elapsed_ms: i64,
    random: RandomState,
    event: LifeEvent,
) -> Result<Transition, LifeError> {
    state.validate()?;
    match &event {
        LifeEvent::Plan { completion_effect, .. } => {
            completion_effect.validate()?;
            if state.current_action.as_ref().is_some_and(|a| !a.terminal()) {
                return Err(LifeError::ActionBusy);
            }
            if state.next_action_ordinal.0 == MAX_COUNTER {
                return Err(LifeError::CounterExhausted);
            }
        }
        LifeEvent::ApplyNeedEffect { effect } => effect.validate()?,
        LifeEvent::Start { action_id } | LifeEvent::Complete { action_id } | LifeEvent::Cancel { action_id, .. } => {
            state.validate_allocated(action_id)?;
            let Some(action) = &state.current_action else {
                return Err(LifeError::UnknownAction);
            };
            if &action.intent.id != action_id {
                return Ok(unchanged(state, random, Outcome::Stale));
            }
            match (&event, action.phase) {
                (LifeEvent::Start { .. }, ActionPhase::Started)
                | (LifeEvent::Complete { .. }, ActionPhase::Completed)
                | (LifeEvent::Cancel { .. }, ActionPhase::Cancelled) => {
                    return Ok(unchanged(state, random, Outcome::Duplicate))
                }
                (LifeEvent::Complete { .. }, ActionPhase::Cancelled)
                | (LifeEvent::Cancel { .. }, ActionPhase::Completed) => {
                    return Ok(unchanged(state, random, Outcome::Stale))
                }
                (LifeEvent::Start { .. }, ActionPhase::Planned)
                | (LifeEvent::Complete { .. }, ActionPhase::Started)
                | (LifeEvent::Cancel { .. }, ActionPhase::Planned | ActionPhase::Started) => (),
                _ => return Err(LifeError::InvalidTransition),
            }
        }
        LifeEvent::Tick {} => (),
    }
    let elapsed = elapsed_ms.max(0) as u64;
    let applied = elapsed.min(MAX_ELAPSED_MS);
    if applied == 0
        && (matches!(&event, LifeEvent::Tick {})
            || matches!(&event, LifeEvent::ApplyNeedEffect { effect } if state.needs.apply(*effect) == state.needs))
    {
        let mut result = unchanged(state, random, Outcome::NoChange);
        result.elapsed_was_limited = elapsed_ms < 0;
        return Ok(result);
    }
    let time = state
        .simulation_time
        .0
        .checked_add(applied)
        .filter(|t| *t <= MAX_SIMULATION_TIME_MS)
        .ok_or(LifeError::ClockExhausted)?;
    let revision = state.revision.checked_add(1).filter(|r| *r <= MAX_COUNTER).ok_or(LifeError::CounterExhausted)?;
    let mut next = state.clone();
    next.simulation_time = SimulationTime(time);
    next.revision = revision;
    let mut intents = Vec::new();
    let mut next_random = random;
    let committed = match event {
        LifeEvent::Tick {} => CommittedEvent::Advanced { elapsed_ms: applied },
        LifeEvent::ApplyNeedEffect { effect } => {
            next.needs = next.needs.apply(effect);
            CommittedEvent::NeedsChanged { effect }
        }
        LifeEvent::Plan { activity, completion_effect } => {
            let (rng, draw) = random.draw();
            next_random = rng;
            let intent = ActionIntent {
                id: ActionId {
                    actor_id: next.actor_id.clone(),
                    world_id: next.world_id.clone(),
                    ordinal: next.next_action_ordinal,
                },
                world_id: next.world_id.clone(),
                activity,
                planned_at: next.simulation_time,
                variation: draw as u16,
                completion_effect,
            };
            next.next_action_ordinal = Sequence(next.next_action_ordinal.0 + 1);
            next.current_action = Some(ActionState {
                intent: intent.clone(),
                phase: ActionPhase::Planned,
                started_at: None,
                finished_at: None,
                cancellation: None,
            });
            intents.push(intent.clone());
            CommittedEvent::Planned { intent }
        }
        LifeEvent::Start { action_id } => {
            let action = next.current_action.as_mut().ok_or(LifeError::UnknownAction)?;
            action.phase = ActionPhase::Started;
            action.started_at = Some(next.simulation_time);
            CommittedEvent::Started { action_id }
        }
        LifeEvent::Complete { action_id } => {
            let action = next.current_action.as_mut().ok_or(LifeError::UnknownAction)?;
            next.needs = next.needs.apply(action.intent.completion_effect);
            action.phase = ActionPhase::Completed;
            action.finished_at = Some(next.simulation_time);
            next.resolved_action_ordinal = action.intent.id.ordinal.0;
            CommittedEvent::Completed { action_id }
        }
        LifeEvent::Cancel { action_id, reason } => {
            let action = next.current_action.as_mut().ok_or(LifeError::UnknownAction)?;
            action.phase = ActionPhase::Cancelled;
            action.finished_at = Some(next.simulation_time);
            action.cancellation = Some(reason);
            next.resolved_action_ordinal = action.intent.id.ordinal.0;
            CommittedEvent::Cancelled { action_id, reason }
        }
    };
    next.recent_events.push(LifeRecord {
        id: EventId { actor_id: next.actor_id.clone(), sequence: Sequence(revision) },
        at: next.simulation_time,
        event: committed,
    });
    next.validate()?;
    Ok(Transition {
        state: next,
        intents,
        random: next_random,
        outcome: Outcome::Applied,
        applied_elapsed_ms: applied,
        elapsed_was_limited: elapsed_ms < 0 || elapsed > MAX_ELAPSED_MS,
    })
}

#[cfg(test)]
#[path = "life_domain/tests.rs"]
mod tests;
