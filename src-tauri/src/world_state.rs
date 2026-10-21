//! Shared stage contracts. Existing per-actor Life world IDs remain lineages;
//! joining this stage never rewrites, advances or rewards a LifeState.
//! No runtime, storage, OS, capture, renderer or network authority lives here.
#[path = "world_state/coordinates.rs"]
pub(crate) mod coordinates;
use crate::life_domain::{
    ActionId, ActionPhase, ActorId, LifeState, PublicActor, WorldId, MAX_COUNTER,
};
use coordinates::{Point, Rect};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    marker::PhantomData,
};

pub const SCHEMA_VERSION: u16 = 1;
pub const MAX_ACTORS: usize = 64;
pub const MAX_ANCHORS: usize = 64;
pub const MAX_SLOTS: usize = 128;
pub const MAX_LEASE_MS: u64 = 120_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    InvalidId,
    InvalidGeometry,
    InvalidConfig,
    UnsupportedSchema,
    LimitExceeded,
    StaleRevision,
    WrongStage,
    InvalidOwner,
    WrongLineage,
    UnknownSlot,
    Occupied,
    Expired,
    CounterExhausted,
    InvalidClock,
}
impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidId => "invalid stage identifier",
            Self::InvalidGeometry => "invalid stage geometry",
            Self::InvalidConfig => "inconsistent stage configuration",
            Self::UnsupportedSchema => "unsupported stage schema",
            Self::LimitExceeded => "stage capacity exceeded",
            Self::StaleRevision => "stage configuration changed",
            Self::WrongStage => "operation belongs to another stage",
            Self::InvalidOwner => "stage owner is no longer current",
            Self::WrongLineage => "action belongs to another actor lineage",
            Self::UnknownSlot => "stage slot is unavailable",
            Self::Occupied => "stage slot is occupied",
            Self::Expired => "stage reservation expired",
            Self::CounterExhausted => "stage sequence exhausted",
            Self::InvalidClock => "invalid stage clock",
        })
    }
}
impl std::error::Error for WorldError {}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}
macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl AsRef<str>) -> Result<Self, WorldError> {
                let value = value.as_ref();
                if !valid_id(value) {
                    return Err(WorldError::InvalidId);
                }
                Ok(Self(value.to_owned()))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Self::new(String::deserialize(d)?).map_err(de::Error::custom)
            }
        }
    };
}
identifier!(StageId);
identifier!(AnchorId);
identifier!(SlotId);

// These identities are deliberately NOT serializable/deserializable. The
// future native owner mints the epoch/session/operation; inputs cannot grant it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeEpoch(String);
impl RuntimeEpoch {
    pub fn new(value: impl AsRef<str>) -> Result<Self, WorldError> {
        if !valid_id(value.as_ref()) {
            return Err(WorldError::InvalidId);
        }
        Ok(Self(value.as_ref().to_owned()))
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionStamp {
    actor: ActorId,
    id: String,
    revision: u64,
}
impl SessionStamp {
    pub fn new(actor: ActorId, id: impl AsRef<str>, revision: u64) -> Result<Self, WorldError> {
        if !valid_id(actor.as_str())
            || !valid_id(id.as_ref())
            || !(1..=MAX_COUNTER).contains(&revision)
        {
            return Err(WorldError::InvalidOwner);
        }
        Ok(Self {
            actor,
            id: id.as_ref().to_owned(),
            revision,
        })
    }
    pub fn actor_id(&self) -> &ActorId {
        &self.actor
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkIdentity {
    Life(ActionId),
    SessionOperation(String),
}
impl WorkIdentity {
    pub fn operation(id: impl AsRef<str>) -> Result<Self, WorldError> {
        if !valid_id(id.as_ref()) {
            return Err(WorldError::InvalidId);
        }
        Ok(Self::SessionOperation(id.as_ref().to_owned()))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorKind {
    Rest,
    Snack,
    Meeting,
    Play,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slot {
    id: SlotId,
    position: Point,
}
impl Slot {
    pub fn new(id: SlotId, position: Point) -> Self {
        Self { id, position }
    }
    pub fn id(&self) -> &SlotId {
        &self.id
    }
    pub fn position(&self) -> Point {
        self.position
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Anchor {
    id: AnchorId,
    kind: AnchorKind,
    position: Point,
    #[serde(deserialize_with = "bounded::<_, _, MAX_SLOTS>")]
    slots: Vec<Slot>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    content_ref: Option<crate::content_packs::ContentRef>,
}
impl Anchor {
    pub fn id(&self)->&AnchorId { &self.id }
    pub fn content_ref(&self)->Option<&crate::content_packs::ContentRef> {self.content_ref.as_ref()}
    pub fn kind(&self) -> AnchorKind { self.kind }
    pub fn slots(&self) -> &[Slot] { &self.slots }
    pub fn new(
        id: AnchorId,
        kind: AnchorKind,
        position: Point,
        slots: Vec<Slot>,
    ) -> Result<Self, WorldError> {
        if slots.is_empty() || slots.len() > MAX_SLOTS {
            return Err(WorldError::LimitExceeded);
        }
        Ok(Self {
            id,
            kind,
            position,
            slots,
            content_ref: None,
        })
    }
}
fn bounded<'de, D, T, const N: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "at most {N} entries")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element()? {
                if items.len() == N {
                    return Err(de::Error::custom("stage capacity exceeded"));
                }
                items.push(item);
            }
            Ok(items)
        }
    }
    d.deserialize_seq(Visitor::<T, N>(PhantomData))
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigWire {
    schema_version: u16,
    stage_id: StageId,
    revision: u64,
    safe_region: Rect,
    #[serde(deserialize_with = "bounded::<_, _, MAX_ACTORS>")]
    members: Vec<ActorId>,
    #[serde(deserialize_with = "bounded::<_, _, MAX_ANCHORS>")]
    anchors: Vec<Anchor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ConfigWire", into = "ConfigWire")]
pub struct WorldConfig(ConfigWire);
impl TryFrom<ConfigWire> for WorldConfig {
    type Error = WorldError;
    fn try_from(wire: ConfigWire) -> Result<Self, Self::Error> {
        let result = Self(wire);
        result.validate()?;
        Ok(result)
    }
}
impl From<WorldConfig> for ConfigWire {
    fn from(config: WorldConfig) -> Self {
        config.0
    }
}
impl WorldConfig {
    pub fn new(
        stage: StageId,
        safe: Rect,
        members: Vec<ActorId>,
        anchors: Vec<Anchor>,
    ) -> Result<Self, WorldError> {
        Self::try_from(ConfigWire {
            schema_version: SCHEMA_VERSION,
            stage_id: stage,
            revision: 0,
            safe_region: safe,
            members,
            anchors,
        })
    }
    pub fn stage_id(&self) -> &StageId {
        &self.0.stage_id
    }
    pub fn revision(&self) -> u64 {
        self.0.revision
    }
    pub fn members(&self) -> &[ActorId] {
        &self.0.members
    }
    pub fn anchors(&self) -> &[Anchor] { &self.0.anchors }
    pub fn safe_region(&self) -> Rect {
        self.0.safe_region
    }
    /// Native storage construction only. The Store must perform its own atomic
    /// expected-revision CAS; this validated value grants no write authority.
    pub fn with_revision(&self, revision: u64) -> Result<Self, WorldError> {
        let mut config = self.clone();
        config.0.revision = revision;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), WorldError> {
        let wire = &self.0;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(WorldError::UnsupportedSchema);
        }
        if wire.revision > MAX_COUNTER || !valid_id(wire.stage_id.as_str()) {
            return Err(WorldError::InvalidConfig);
        }
        wire.safe_region.validate()?;
        if wire.members.len() > MAX_ACTORS || wire.anchors.len() > MAX_ANCHORS {
            return Err(WorldError::LimitExceeded);
        }
        let mut actors = BTreeSet::new();
        let mut anchors = BTreeSet::new();
        let mut slots = BTreeSet::new();
        for actor in &wire.members {
            if !valid_id(actor.as_str()) || !actors.insert(actor.as_str()) {
                return Err(WorldError::InvalidConfig);
            }
        }
        for anchor in &wire.anchors {
            if anchor.content_ref.as_ref().is_some_and(|reference|reference.validate().is_err()) {return Err(WorldError::InvalidConfig);}
            if !valid_id(anchor.id.as_str())
                || !anchors.insert(&anchor.id)
                || !wire.safe_region.contains(anchor.position)
                || anchor.slots.is_empty()
            {
                return Err(WorldError::InvalidConfig);
            }
            for slot in &anchor.slots {
                if !valid_id(slot.id.as_str())
                    || !slots.insert(&slot.id)
                    || !wire.safe_region.contains(slot.position)
                {
                    return Err(WorldError::InvalidConfig);
                }
                if slots.len() > MAX_SLOTS {
                    return Err(WorldError::LimitExceeded);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberHandle {
    stage: StageId,
    epoch: RuntimeEpoch,
    generation: u64,
    session: SessionStamp,
    source_world: Option<WorldId>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotLease {
    stage: StageId,
    epoch: RuntimeEpoch,
    serial: u64,
    slot: SlotId,
    resource_generation: u64,
    member: MemberHandle,
    work: WorkIdentity,
}
impl SlotLease {
    pub(crate) fn slot_id(&self) -> &SlotId { &self.slot }
}
#[derive(Clone)]
struct Participant {
    id: ActorId,
    position: Point,
    handle: Option<MemberHandle>,
    last_session: Option<SessionStamp>,
    public: Option<PublicActor>,
    observed_world: Option<WorldId>,
    life_revision: Option<u64>,
    action: Option<ActionId>,
}
#[derive(Clone, Eq, PartialEq)]
struct Resource {
    anchor: AnchorId,
    kind: AnchorKind,
    position: Point,
    generation: u64,
}
#[derive(Clone)]
struct Reservation {
    lease: SlotLease,
    expires_at: u64,
}
#[derive(Clone)]
pub struct WorldState {
    config: WorldConfig,
    epoch: RuntimeEpoch,
    sequence: u64,
    next_identity: u64,
    now: u64,
    exhausted: bool,
    actors: BTreeMap<String, Participant>,
    resources: BTreeMap<SlotId, Resource>,
    reservations: BTreeMap<SlotId, Reservation>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSnapshot {
    pub schema_version: u16,
    pub stage_id: StageId,
    pub config_revision: u64,
    pub sequence: u64,
    pub safe_region: Rect,
    pub anchors: Vec<Anchor>,
    pub actors: Vec<StageActor>,
    pub occupied: Vec<OccupiedSlot>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageActor {
    pub actor_id: ActorId,
    pub position: Point,
    pub life: Option<PublicActor>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OccupiedSlot {
    pub slot_id: SlotId,
    pub actor_id: ActorId,
}
impl WorldState {
    pub fn new(config: WorldConfig, epoch: RuntimeEpoch) -> Result<Self, WorldError> {
        config.validate()?;
        let mut state = Self {
            config,
            epoch,
            sequence: 0,
            next_identity: 1,
            now: 0,
            exhausted: false,
            actors: BTreeMap::new(),
            resources: BTreeMap::new(),
            reservations: BTreeMap::new(),
        };
        for actor in state.config.0.members.clone() {
            state.actors.insert(
                actor.as_str().to_owned(),
                Participant {
                    id: actor,
                    position: state.config.safe_region().center(),
                    handle: None,
                    last_session: None,
                    public: None,
                    observed_world: None,
                    life_revision: None,
                    action: None,
                },
            );
        }
        state.rebuild_resources(None)?;
        Ok(state)
    }
    pub fn config(&self) -> &WorldConfig {
        &self.config
    }
    pub fn is_participant(&self, actor: &ActorId) -> bool {
        self.actors.contains_key(actor.as_str())
    }
    /// Native-only lookup. Caller must still hold/check actual Core authority;
    /// the opaque returned handle must never be exposed through IPC.
    pub fn member_handle(&self, actor: &ActorId) -> Option<MemberHandle> {
        self.actors
            .get(actor.as_str())
            .and_then(|actor| actor.handle.clone())
    }
    /// Current validated resource geometry only; this grants no reservation.
    pub(crate) fn slot_geometry(&self, slot: &SlotId) -> Option<(AnchorKind, Point)> {
        self.resources.get(slot).map(|resource| (resource.kind, resource.position))
    }
    fn identity(&mut self) -> Result<u64, WorldError> {
        let id = self.next_identity;
        if id >= MAX_COUNTER {
            return Err(WorldError::CounterExhausted);
        }
        self.next_identity += 1;
        Ok(id)
    }
    fn changed(&mut self) -> Result<(), WorldError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .filter(|v| *v <= MAX_COUNTER)
            .ok_or(WorldError::CounterExhausted)?;
        Ok(())
    }
    fn transaction<T>(
        &mut self,
        op: impl FnOnce(&mut Self) -> Result<T, WorldError>,
    ) -> Result<T, WorldError> {
        if self.exhausted {
            return Err(WorldError::CounterExhausted);
        }
        let mut next = self.clone();
        match op(&mut next) {
            Ok(value) => {
                *self = next;
                Ok(value)
            }
            Err(WorldError::CounterExhausted) => {
                self.revoke_all();
                self.exhausted = true;
                Err(WorldError::CounterExhausted)
            }
            Err(error) => Err(error),
        }
    }
    fn revoke_all(&mut self) {
        self.reservations.clear();
        for actor in self.actors.values_mut() {
            actor.handle = None;
        }
    }
    pub fn advance(&mut self, raw: u64) -> Result<(), WorldError> {
        if raw > MAX_COUNTER {
            self.revoke_all();
            self.exhausted = true;
            return Err(WorldError::InvalidClock);
        }
        self.now = self.now.max(raw);
        let before = self.reservations.len();
        self.reservations
            .retain(|_, reservation| reservation.expires_at > self.now);
        if before != self.reservations.len() {
            if let Err(error) = self.changed() {
                self.revoke_all();
                self.exhausted = true;
                return Err(error);
            }
        }
        Ok(())
    }
    fn rebuild_resources(
        &mut self,
        previous: Option<(&WorldConfig, &BTreeMap<SlotId, Resource>)>,
    ) -> Result<(), WorldError> {
        let mut resources = BTreeMap::new();
        for anchor in self.config.0.anchors.clone() {
            for slot in anchor.slots {
                let mut resource = Resource {
                    anchor: anchor.id.clone(),
                    kind: anchor.kind,
                    position: slot.position,
                    generation: 0,
                };
                let retained_generation = if let Some((config, old)) = previous {
                    old.get(&slot.id)
                        .filter(|old| {
                            config.safe_region() == self.config.safe_region()
                                && old.anchor == resource.anchor
                                && old.kind == resource.kind
                                && old.position == resource.position
                        })
                        .map(|old| old.generation)
                } else {
                    None
                };
                resource.generation = match retained_generation {
                    Some(generation) => generation,
                    None => self.identity()?,
                };
                resources.insert(slot.id, resource);
            }
        }
        self.resources = resources;
        Ok(())
    }
    pub fn replace_config(
        &mut self,
        expected: u64,
        mut config: WorldConfig,
    ) -> Result<(), WorldError> {
        config.validate()?;
        if self.config.stage_id() != config.stage_id() {
            return Err(WorldError::WrongStage);
        }
        if self.config.revision() != expected {
            return Err(WorldError::StaleRevision);
        }
        self.transaction(|next| {
            config.0.revision = expected
                .checked_add(1)
                .filter(|v| *v <= MAX_COUNTER)
                .ok_or(WorldError::CounterExhausted)?;
            let previous = next.config.clone();
            let resources = next.resources.clone();
            next.config = config;
            next.rebuild_resources(Some((&previous, &resources)))?;
            let members = next
                .config
                .0
                .members
                .iter()
                .map(|actor| actor.as_str().to_owned())
                .collect::<BTreeSet<_>>();
            next.actors.retain(|id, _| members.contains(id));
            for id in next.config.0.members.clone() {
                next.actors
                    .entry(id.as_str().to_owned())
                    .or_insert_with(|| Participant {
                        id,
                        position: next.config.safe_region().center(),
                        handle: None,
                        last_session: None,
                        public: None,
                        observed_world: None,
                        life_revision: None,
                        action: None,
                    });
            }
            for actor in next.actors.values_mut() {
                actor.position = next.config.safe_region().clamp(actor.position);
            }
            next.reservations.retain(|slot, r| {
                next.resources
                    .get(slot)
                    .is_some_and(|resource| resource.generation == r.lease.resource_generation)
                    && next
                        .actors
                        .get(r.lease.member.session.actor.as_str())
                        .is_some_and(|actor| actor.handle.as_ref() == Some(&r.lease.member))
            });
            next.changed()
        })
    }
    pub fn bind_actor(
        &mut self,
        session: SessionStamp,
        life: Option<&LifeState>,
        now: u64,
    ) -> Result<MemberHandle, WorldError> {
        self.advance(now)?;
        if !self.actors.contains_key(session.actor.as_str()) {
            return Err(WorldError::InvalidOwner);
        }
        let source_world = validate_life(&session.actor, life)?;
        let previous = &self.actors[session.actor.as_str()];
        if let Some(last) = &previous.last_session {
            if last.id == session.id && session.revision < last.revision {
                return Err(WorldError::InvalidOwner);
            }
            if last == &session {
                let old_source = previous.observed_world.as_ref();
                if old_source != source_world.as_ref() {
                    return Err(WorldError::WrongLineage);
                }
                if previous.handle.is_none() {
                    return Err(WorldError::InvalidOwner);
                }
            }
        }
        if previous.observed_world.is_some() && life.is_none() {
            return Err(WorldError::WrongLineage);
        }
        if let Some(life) = life {
            if previous.observed_world.as_ref() == Some(life.world_id())
                && previous
                    .life_revision
                    .is_some_and(|revision| life.revision() < revision)
            {
                return Err(WorldError::StaleRevision);
            }
        }
        if let Some(handle) = self.actors[session.actor.as_str()]
            .handle
            .as_ref()
            .filter(|h| h.session == session && h.source_world == source_world)
            .cloned()
        {
            if let Some(life) = life {
                self.refresh_actor(&handle, life)?;
            }
            return Ok(handle);
        }
        self.transaction(|next| {
            let handle = MemberHandle {
                stage: next.config.stage_id().clone(),
                epoch: next.epoch.clone(),
                generation: next.identity()?,
                session: session.clone(),
                source_world,
            };
            next.reservations
                .retain(|_, r| r.lease.member.session.actor != session.actor);
            let actor = next
                .actors
                .get_mut(session.actor.as_str())
                .ok_or(WorldError::InvalidOwner)?;
            actor.handle = Some(handle.clone());
            actor.last_session = Some(session);
            set_life(actor, life);
            next.changed()?;
            Ok(handle)
        })
    }
    fn member(&self, handle: &MemberHandle) -> Result<&Participant, WorldError> {
        if self.exhausted || handle.stage != *self.config.stage_id() || handle.epoch != self.epoch {
            return Err(WorldError::InvalidOwner);
        }
        self.actors
            .get(handle.session.actor.as_str())
            .filter(|actor| actor.handle.as_ref() == Some(handle))
            .ok_or(WorldError::InvalidOwner)
    }
    pub fn refresh_actor(
        &mut self,
        handle: &MemberHandle,
        life: &LifeState,
    ) -> Result<(), WorldError> {
        let actor = self.member(handle)?;
        if validate_life(&handle.session.actor, Some(life))? != handle.source_world {
            return Err(WorldError::WrongLineage);
        }
        if actor
            .life_revision
            .is_some_and(|revision| life.revision() < revision)
        {
            return Err(WorldError::StaleRevision);
        }
        self.transaction(|next| {
            let actor = next.actors.get_mut(handle.session.actor.as_str()).ok_or(WorldError::InvalidOwner)?;
            set_life(actor, Some(life)); let action = actor.action.clone();
            next.reservations.retain(|_, r| r.lease.member != *handle || !matches!(&r.lease.work, WorkIdentity::Life(id) if Some(id) != action.as_ref()));
            next.changed()
        })
    }
    /// Fresh validated Store observation, including inactive actors and a Stop's
    /// committed cancellation. It never binds a session or grants a place.
    pub fn observe_actor(
        &mut self,
        actor_id: &ActorId,
        life: &LifeState,
    ) -> Result<(), WorldError> {
        let source = validate_life(actor_id, Some(life))?;
        let previous = self
            .actors
            .get(actor_id.as_str())
            .ok_or(WorldError::InvalidOwner)?;
        if previous.observed_world.is_some() && previous.observed_world != source {
            let actor = self
                .actors
                .get_mut(actor_id.as_str())
                .ok_or(WorldError::InvalidOwner)?;
            actor.handle = None;
            actor.public = None;
            actor.action = None;
            self.reservations
                .retain(|_, r| r.lease.member.session.actor != *actor_id);
            if self.changed().is_err() {
                self.revoke_all();
                self.exhausted = true;
            }
            return Err(WorldError::WrongLineage);
        }
        if previous
            .life_revision
            .is_some_and(|revision| life.revision() < revision)
        {
            return Err(WorldError::StaleRevision);
        }
        self.transaction(|next| {
            let actor = next.actors.get_mut(actor_id.as_str()).ok_or(WorldError::InvalidOwner)?;
            set_life(actor, Some(life)); let action = actor.action.clone();
            next.reservations.retain(|_, r| r.lease.member.session.actor != *actor_id || !matches!(&r.lease.work, WorkIdentity::Life(id) if Some(id) != action.as_ref()));
            next.changed()
        })
    }
    pub fn move_actor(&mut self, handle: &MemberHandle, position: Point) -> Result<(), WorldError> {
        self.member(handle)?;
        if !self.config.safe_region().contains(position) {
            return Err(WorldError::InvalidGeometry);
        }
        self.transaction(|next| {
            next.actors
                .get_mut(handle.session.actor.as_str())
                .ok_or(WorldError::InvalidOwner)?
                .position = position;
            next.changed()
        })
    }
    /// Explicit main-window placement is separate from autonomous session
    /// movement. It invalidates that actor's arrival leases, grants no session
    /// or action, and never mutates the actor's LifeState.
    pub fn place_actor(
        &mut self,
        expected: u64,
        actor: &ActorId,
        position: Point,
    ) -> Result<(), WorldError> {
        if expected != self.config.revision() {
            return Err(WorldError::StaleRevision);
        }
        if !self.is_participant(actor) {
            return Err(WorldError::InvalidOwner);
        }
        if !self.config.safe_region().contains(position) {
            return Err(WorldError::InvalidGeometry);
        }
        self.transaction(|next| {
            next.actors
                .get_mut(actor.as_str())
                .ok_or(WorldError::InvalidOwner)?
                .position = position;
            next.reservations
                .retain(|_, reservation| reservation.lease.member.session.actor != *actor);
            next.changed()
        })
    }
    fn owner(&self, member: &MemberHandle, work: &WorkIdentity) -> Result<(), WorldError> {
        let actor = self.member(member)?;
        match work {
            WorkIdentity::Life(id)
                if id.actor_id() != &member.session.actor
                    || member.source_world.as_ref() != Some(id.world_id())
                    || actor.action.as_ref() != Some(id) =>
            {
                Err(WorldError::WrongLineage)
            }
            WorkIdentity::SessionOperation(id) if !valid_id(id) => Err(WorldError::InvalidOwner),
            _ => Ok(()),
        }
    }
    fn expiry(&self, ttl: u64) -> Result<u64, WorldError> {
        if !(1..=MAX_LEASE_MS).contains(&ttl) {
            return Err(WorldError::InvalidClock);
        }
        self.now
            .checked_add(ttl)
            .filter(|v| *v <= MAX_COUNTER)
            .ok_or(WorldError::InvalidClock)
    }
    pub fn reserve(
        &mut self,
        expected: u64,
        member: &MemberHandle,
        work: WorkIdentity,
        slot: &SlotId,
        ttl: u64,
        now: u64,
    ) -> Result<SlotLease, WorldError> {
        self.advance(now)?;
        if expected != self.config.revision() {
            return Err(WorldError::StaleRevision);
        }
        self.owner(member, &work)?;
        let generation = self
            .resources
            .get(slot)
            .ok_or(WorldError::UnknownSlot)?
            .generation;
        let expires_at = self.expiry(ttl)?;
        if let Some(current) = self.reservations.get(slot) {
            return if current.lease.member == *member && current.lease.work == work {
                Ok(current.lease.clone())
            } else {
                Err(WorldError::Occupied)
            };
        }
        self.transaction(|next| {
            let lease = SlotLease {
                stage: next.config.stage_id().clone(),
                epoch: next.epoch.clone(),
                serial: next.identity()?,
                slot: slot.clone(),
                resource_generation: generation,
                member: member.clone(),
                work,
            };
            next.reservations.insert(
                slot.clone(),
                Reservation {
                    lease: lease.clone(),
                    expires_at,
                },
            );
            next.changed()?;
            Ok(lease)
        })
    }
    fn current_lease(&self, lease: &SlotLease, member: &MemberHandle) -> Result<(), WorldError> {
        self.owner(member, &lease.work)?;
        if lease.member != *member
            || self
                .reservations
                .get(&lease.slot)
                .is_none_or(|r| r.lease != *lease || r.expires_at <= self.now)
        {
            return Err(WorldError::Expired);
        }
        Ok(())
    }
    /// Place authority only. The later executor still validates Core and commits
    /// the unchanged Life ActionId through Store CAS; this grants no reward.
    pub fn validate_reservation(
        &mut self,
        lease: &SlotLease,
        member: &MemberHandle,
        now: u64,
    ) -> Result<Point, WorldError> {
        self.advance(now)?;
        self.current_lease(lease, member)?;
        self.resources
            .get(&lease.slot)
            .map(|r| r.position)
            .ok_or(WorldError::UnknownSlot)
    }
    pub fn renew(
        &mut self,
        lease: &SlotLease,
        member: &MemberHandle,
        ttl: u64,
        now: u64,
    ) -> Result<(), WorldError> {
        self.advance(now)?;
        self.current_lease(lease, member)?;
        let expires = self.expiry(ttl)?;
        self.transaction(|next| {
            next.reservations
                .get_mut(&lease.slot)
                .ok_or(WorldError::Expired)?
                .expires_at = expires;
            next.changed()
        })
    }
    pub fn release(
        &mut self,
        lease: &SlotLease,
        member: &MemberHandle,
        now: u64,
    ) -> Result<bool, WorldError> {
        self.advance(now)?;
        self.current_lease(lease, member)?;
        self.transaction(|next| {
            next.reservations.remove(&lease.slot);
            next.changed()?;
            Ok(true)
        })
    }
    pub fn unbind_session(&mut self, session: &SessionStamp) -> bool {
        let Some(actor) = self.actors.get_mut(session.actor.as_str()).filter(|actor| {
            actor
                .handle
                .as_ref()
                .is_some_and(|handle| handle.session == *session)
        }) else {
            return false;
        };
        actor.handle = None;
        self.reservations
            .retain(|_, r| r.lease.member.session != *session);
        if self.changed().is_err() {
            self.revoke_all();
            self.exhausted = true;
        }
        true
    }
    pub fn stop_all_sessions(&mut self) {
        let changed = !self.reservations.is_empty()
            || self.actors.values().any(|actor| actor.handle.is_some());
        self.revoke_all();
        if changed && self.changed().is_err() {
            self.exhausted = true;
        }
    }
    /// Native output geometry changed. Old arrival/release callbacks cannot
    /// reuse a slot on its new mapping, even when canonical anchors stay put.
    pub fn invalidate_geometry(&mut self) -> Result<(), WorldError> {
        self.transaction(|next| {
            next.reservations.clear();
            let slots = next.resources.keys().cloned().collect::<Vec<_>>();
            for slot in slots {
                let generation = next.identity()?;
                next.resources
                    .get_mut(&slot)
                    .ok_or(WorldError::UnknownSlot)?
                    .generation = generation;
            }
            next.changed()
        })
    }
    pub fn snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            schema_version: SCHEMA_VERSION,
            stage_id: self.config.stage_id().clone(),
            config_revision: self.config.revision(),
            sequence: self.sequence,
            safe_region: self.config.safe_region(),
            anchors: self.config.0.anchors.clone(),
            actors: self
                .actors
                .values()
                .map(|actor| StageActor {
                    actor_id: actor.id.clone(),
                    position: actor.position,
                    life: actor.public.clone(),
                })
                .collect(),
            occupied: self
                .reservations
                .values()
                .filter(|r| r.expires_at > self.now)
                .map(|r| OccupiedSlot {
                    slot_id: r.lease.slot.clone(),
                    actor_id: r.lease.member.session.actor.clone(),
                })
                .collect(),
        }
    }
    /// An output only gets the common scene if its own actor explicitly
    /// participates. Other selected actors contribute public observations only.
    pub fn snapshot_for_actor(&self, actor: &ActorId) -> Result<WorldSnapshot, WorldError> {
        if !self.is_participant(actor) {
            return Err(WorldError::InvalidOwner);
        }
        Ok(self.snapshot())
    }
}
fn validate_life(actor: &ActorId, life: Option<&LifeState>) -> Result<Option<WorldId>, WorldError> {
    match life {
        Some(life) => {
            life.validate().map_err(|_| WorldError::WrongLineage)?;
            if life.actor_id() != actor {
                return Err(WorldError::WrongLineage);
            }
            Ok(Some(life.world_id().clone()))
        }
        None => Ok(None),
    }
}
fn set_life(actor: &mut Participant, life: Option<&LifeState>) {
    actor.public = life.map(LifeState::public_projection);
    actor.observed_world = life.map(|life| life.world_id().clone());
    actor.life_revision = life.map(LifeState::revision);
    actor.action = life
        .and_then(|life| life.current_action())
        .filter(|action| matches!(action.phase(), ActionPhase::Planned | ActionPhase::Started))
        .map(|action| action.intent().id().clone());
}

#[cfg(test)]
#[path = "world_state/tests.rs"]
mod tests;
