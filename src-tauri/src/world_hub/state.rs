//! Native shared-stage owner. Call mutation/synchronization under Life Core,
//! then acquire WorldHub -> Store. GUI queries and event emission stay outside.
use crate::{
    life_domain::{ActorId, WorldId, MAX_COUNTER},
    storage::{
        world_store::{OutputBinding, WorldRecord, WorldStoreError},
        Store,
    },
    world_state::{
        coordinates::{self, LogicalSize, OutputGeometry, Point},
        AnchorKind, RuntimeEpoch, SessionStamp, SlotId, SlotLease, WorkIdentity, WorldConfig, WorldSnapshot,
        WorldState,
    },
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{atomic::AtomicU64, Mutex},
    time::{Duration, Instant},
};
const UNAVAILABLE: &str = "Die gemeinsame Bühne ist vorübergehend nicht verfügbar.";
const INVALID_PLACE: &str = "Die Position liegt außerhalb des sicheren Bühnenbereichs.";
pub(crate) struct NativeActor {
    pub stamp: SessionStamp,
    pub lineage: Option<WorldId>,
}
#[derive(Clone)]
pub(super) struct DisplaySample {
    pub(super) geometry: OutputGeometry,
}
#[derive(Clone)]
pub(crate) struct TopologySample {
    pub(super) ticket: u64,
    pub(super) observed: Instant,
    pub(super) displays: Vec<DisplaySample>,
    pub(super) fingerprint: Vec<(String, i32, i32, u32, u32, u64)>,
}
impl TopologySample {
    /// Origin/name IDs may alias mirrored displays. Conflicting geometry for
    /// one ID is unusable, while byte-identical duplicates are coalesced. Both
    /// selection and its fingerprint then have the same deterministic order.
    fn canonicalize(mut self) -> Self {
        self.fingerprint.sort();
        self.fingerprint.dedup();
        let mut ambiguous = BTreeSet::new();
        for pair in self.fingerprint.windows(2) {
            if pair[0].0 == pair[1].0 {
                ambiguous.insert(pair[0].0.clone());
            }
        }
        self.fingerprint
            .retain(|entry| !ambiguous.contains(&entry.0));
        let valid = self
            .fingerprint
            .iter()
            .map(|entry| entry.0.as_str())
            .collect::<BTreeSet<_>>();
        self.displays
            .retain(|display| valid.contains(display.geometry.id()));
        self.displays
            .sort_by(|a, b| a.geometry.id().cmp(b.geometry.id()));
        self.displays
            .dedup_by(|a, b| a.geometry.id() == b.geometry.id());
        self.fingerprint.retain(|entry| {
            self.displays
                .iter()
                .any(|display| display.geometry.id() == entry.0)
        });
        self
    }
}
#[derive(Clone)]
struct Hub {
    instance_id: String,
    record: Option<WorldRecord>,
    world: Option<WorldState>,
    error: Option<WorldStoreError>,
    bound: BTreeMap<String, SessionStamp>,
    unavailable: BTreeSet<String>,
    published: BTreeSet<String>,
    topology: Option<TopologySample>,
    topology_revision: u64,
}
pub(crate) use crate::life_runtime::action_state::WorldActionFence as ActionContext;
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActionSlotTarget {
    pub(crate) id: SlotId,
    pub(crate) kind: AnchorKind,
    pub(crate) position: Point,
    pub(crate) content_ref: Option<crate::content_packs::ContentRef>,
}
/// Constructed only from the actual World reservation for a current Life Plan.
/// The opaque lease and its target cannot be independently replaced by callers.
#[derive(Clone)]
pub(crate) struct SlotReservation {
    target: ActionSlotTarget,
    lease: SlotLease,
    valid_from: u64,
    valid_until: u64,
}
impl SlotReservation {
    pub(crate) fn lease(&self) -> SlotLease { self.lease.clone() }
    pub(crate) fn target(&self) -> &ActionSlotTarget { &self.target }
    pub(crate) fn fresh(&self, now: u64) -> bool {
        now >= self.valid_from && now < self.valid_until
    }
}
pub struct WorldHub {
    inner: Mutex<Hub>,
    pub(super) sample_ticket: AtomicU64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicWorld {
    version: u16,
    instance_id: String,
    availability: &'static str,
    reason: Option<WorldStoreError>,
    snapshot: Option<WorldSnapshot>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSettings {
    version: u16,
    instance_id: String,
    availability: &'static str,
    reason: Option<WorldStoreError>,
    record: Option<WorldRecord>,
}
fn public(hub: &Hub, scope: Option<&ActorId>) -> PublicWorld {
    let mut snapshot = hub.world.as_ref().and_then(|world| match scope {
        Some(actor) => world.snapshot_for_actor(actor).ok(),
        None => Some(world.snapshot()),
    });
    if let Some(snapshot) = &mut snapshot {
        for actor in &mut snapshot.actors {
            if hub.unavailable.contains(actor.actor_id.as_str()) {
                actor.life = None;
            }
        }
    }
    PublicWorld {
        version: 1,
        instance_id: hub.instance_id.clone(),
        availability: if hub.error.is_some() {
            "unavailable"
        } else if hub.world.is_some() {
            "ready"
        } else {
            "not_initialized"
        },
        reason: hub.error,
        snapshot,
    }
}
/// World projections only address the matching native avatar. The main
/// operator gets its separate aggregate event, and SSE checks bot scope again.
pub(crate) fn output_window(event: &Value) -> Option<String> {
    if event["kind"] != "world-state" {
        return None;
    }
    let actor = ActorId::new(event["botId"].as_str()?).ok()?;
    Some(format!("avatar-{}", actor.as_str()))
}
/// Called only while the actual native Core lock is held; no renderer input
/// can supply this set. Re-derived for every capability operation after Stop,
/// profile mutation and output loss, rather than retaining an earlier stamp.
pub(crate) fn current_actors(core: &crate::life_runtime::core::Core) -> Vec<NativeActor> {
    core.sessions
        .iter()
        .filter(|(_, session)| !session.storage_failed && session.output)
        .filter_map(|(bot, session)| {
            Some(NativeActor {
                stamp: SessionStamp::new(
                    ActorId::new(bot).ok()?,
                    &session.session_id,
                    session.revision,
                )
                .ok()?,
                lineage: session.state.as_ref().map(|life| life.world_id().clone()),
            })
        })
        .collect()
}
impl WorldHub {
    pub fn new(store: &Store) -> Self {
        let loaded = store.load_world_config();
        let (record, error) = match loaded {
            Ok(record) => (record, None),
            Err(error) => (None, Some(error)),
        };
        let epoch = RuntimeEpoch::new(uuid::Uuid::new_v4().to_string()).expect("native UUID");
        let world = record
            .as_ref()
            .and_then(|record| WorldState::new(record.config.clone(), epoch).ok());
        Self {
            inner: Mutex::new(Hub {
                instance_id: uuid::Uuid::new_v4().to_string(),
                record,
                world,
                error,
                bound: BTreeMap::new(),
                unavailable: BTreeSet::new(),
                published: BTreeSet::new(),
                topology: None,
                topology_revision: 0,
            }),
            sample_ticket: AtomicU64::new(0),
        }
    }
    pub fn configured(&self) -> bool {
        self.inner
            .lock()
            .map(|hub| hub.record.is_some())
            .unwrap_or(false)
    }
    pub fn snapshot(&self) -> Result<PublicWorld, String> {
        Ok(public(&*self.inner.lock().map_err(|_| UNAVAILABLE)?, None))
    }
    pub fn snapshot_for(&self, id: &str) -> Option<PublicWorld> {
        let id = ActorId::new(id).ok()?;
        let hub = self.inner.lock().ok()?;
        if !hub.world.as_ref()?.is_participant(&id) {
            return None;
        }
        Some(public(&hub, Some(&id)))
    }
    pub fn settings(&self, store: &Store) -> WorldSettings {
        let instance_id = self
            .inner
            .lock()
            .map(|hub| hub.instance_id.clone())
            .unwrap_or_default();
        match store.load_world_config() {
            Ok(record) => WorldSettings {
                version: 1,
                instance_id,
                availability: if record.is_some() {
                    "ready"
                } else {
                    "not_initialized"
                },
                reason: None,
                record,
            },
            Err(error) => WorldSettings {
                version: 1,
                instance_id,
                availability: "unavailable",
                reason: Some(error),
                record: if error == WorldStoreError::UnknownActor {
                    store.world_settings_record().ok().flatten()
                } else {
                    None
                },
            },
        }
    }
    fn apply_topology(hub: &mut Hub, sample: TopologySample) -> Result<(), String> {
        let sample = sample.canonicalize();
        if hub
            .topology
            .as_ref()
            .is_some_and(|old| old.ticket > sample.ticket)
        {
            return Ok(());
        }
        let changed = hub
            .topology
            .as_ref()
            .is_none_or(|old| old.fingerprint != sample.fingerprint);
        if changed {
            if let Some(world) = &mut hub.world {
                world.invalidate_geometry().map_err(|_| UNAVAILABLE)?;
            }
            hub.topology_revision = hub
                .topology_revision
                .checked_add(1)
                .filter(|v| *v <= MAX_COUNTER)
                .ok_or(UNAVAILABLE)?;
        }
        hub.topology = Some(sample);
        Ok(())
    }
    fn refresh_record(hub: &mut Hub, store: &Store) -> Result<(), String> {
        match store.load_world_config() {
            Ok(record) => {
                let unchanged = match (&hub.record, &record) {
                    (Some(old), Some(new)) => {
                        old.config == new.config && old.output_binding == new.output_binding
                    }
                    (None, None) => true,
                    _ => false,
                };
                if !unchanged || hub.error.is_some() {
                    let world = record
                        .as_ref()
                        .map(|record| {
                            WorldState::new(
                                record.config.clone(),
                                RuntimeEpoch::new(uuid::Uuid::new_v4().to_string())
                                    .map_err(|_| UNAVAILABLE)?,
                            )
                            .map_err(|_| UNAVAILABLE)
                        })
                        .transpose()?;
                    hub.instance_id = uuid::Uuid::new_v4().to_string();
                    hub.world = world;
                    hub.bound.clear();
                    hub.unavailable.clear();
                    hub.record = record;
                }
                hub.error = None;
            }
            Err(error) => {
                if let Some(world) = &mut hub.world {
                    world.stop_all_sessions();
                }
                hub.world = None;
                hub.bound.clear();
                hub.unavailable.clear();
                hub.error = Some(error);
                hub.record = if error == WorldStoreError::UnknownActor {
                    store.world_settings_record().ok().flatten()
                } else {
                    None
                };
            }
        }
        Ok(())
    }
    fn synchronize_locked(
        hub: &mut Hub,
        store: &Store,
        sessions: &[NativeActor],
        now: u64,
    ) -> Result<(), String> {
        let Some(world) = &mut hub.world else {
            return Ok(());
        };
        world.advance(now).map_err(|_| UNAVAILABLE)?;
        let current = sessions
            .iter()
            .map(|actor| (actor.stamp.actor_id().as_str(), &actor.stamp))
            .collect::<BTreeMap<_, _>>();
        let old = hub
            .bound
            .iter()
            .filter(|(id, stamp)| {
                current
                    .get(id.as_str())
                    .is_none_or(|current| *current != *stamp)
            })
            .map(|(id, stamp)| (id.clone(), stamp.clone()))
            .collect::<Vec<_>>();
        for (id, stamp) in old {
            world.unbind_session(&stamp);
            hub.bound.remove(&id);
        }
        let members = world.config().members().to_vec();
        hub.unavailable.clear();
        for id in members {
            let life = match store.load_life(id.as_str()) {
                Ok(life) => life,
                Err(_) => {
                    if let Some(stamp) = hub.bound.remove(id.as_str()) {
                        world.unbind_session(&stamp);
                    }
                    hub.unavailable.insert(id.as_str().to_owned());
                    continue;
                }
            };
            if let Some(life) = &life {
                if world.observe_actor(&id, life.state()).is_err() {
                    if let Some(stamp) = hub.bound.remove(id.as_str()) {
                        world.unbind_session(&stamp);
                    }
                    hub.unavailable.insert(id.as_str().to_owned());
                    continue;
                }
            }
            if let Some(session) = sessions
                .iter()
                .find(|session| session.stamp.actor_id() == &id)
            {
                if session.lineage.as_ref().is_some_and(|expected| {
                    life.as_ref()
                        .is_none_or(|life| life.state().world_id() != expected)
                }) {
                    if let Some(stamp) = hub.bound.remove(id.as_str()) {
                        world.unbind_session(&stamp);
                    }
                    hub.unavailable.insert(id.as_str().to_owned());
                    continue;
                }
                match world.bind_actor(
                    session.stamp.clone(),
                    life.as_ref().map(|life| life.state()),
                    now,
                ) {
                    Ok(_) => {
                        hub.bound
                            .insert(id.as_str().to_owned(), session.stamp.clone());
                    }
                    Err(_) => {
                        if let Some(stamp) = hub.bound.remove(id.as_str()) {
                            world.unbind_session(&stamp);
                        }
                        hub.unavailable.insert(id.as_str().to_owned());
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn synchronize(
        &self,
        store: &Store,
        sessions: &[NativeActor],
        sample: Option<TopologySample>,
        now: u64,
    ) -> Result<(), String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        if let Some(sample) = sample {
            Self::apply_topology(&mut hub, sample)?;
        }
        Self::synchronize_locked(&mut hub, store, sessions, now)
    }
    pub(crate) fn save(
        &self,
        store: &Store,
        expected: Option<u64>,
        draft: &WorldConfig,
        binding: &OutputBinding,
        sessions: &[NativeActor],
        sample: TopologySample,
        now: u64,
    ) -> Result<WorldSettings, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        let mut staged = if let Some(world) = &hub.world {
            let mut staged = world.clone();
            staged
                .replace_config(
                    expected.ok_or(WorldStoreError::StaleRevision.message())?,
                    draft.clone(),
                )
                .map_err(|_| WorldStoreError::StaleRevision.message())?;
            staged
        } else {
            WorldState::new(
                draft
                    .with_revision(expected.unwrap_or(0).checked_add(1).ok_or(UNAVAILABLE)?)
                    .map_err(|_| WorldStoreError::CorruptConfig.message())?,
                RuntimeEpoch::new(uuid::Uuid::new_v4().to_string()).map_err(|_| UNAVAILABLE)?,
            )
            .map_err(|_| UNAVAILABLE)?
        };
        if hub
            .record
            .as_ref()
            .is_none_or(|old| old.output_binding != *binding)
        {
            staged.invalidate_geometry().map_err(|_| UNAVAILABLE)?;
        }
        let mut next = hub.clone();
        if hub.world.is_none() {
            next.instance_id = uuid::Uuid::new_v4().to_string();
        }
        next.record = Some(WorldRecord {
            config: staged.config().clone(),
            output_binding: binding.clone(),
        });
        next.world = Some(staged);
        next.error = None;
        next.bound
            .retain(|id, _| draft.members().iter().any(|actor| actor.as_str() == id));
        Self::apply_topology(&mut next, sample)?;
        Self::synchronize_locked(&mut next, store, sessions, now)?;
        let confirmed = store
            .save_world_config(expected, draft, binding)
            .map_err(|error| error.message())?;
        next.record = Some(confirmed.clone());
        *hub = next;
        Ok(WorldSettings {
            version: 1,
            instance_id: hub.instance_id.clone(),
            availability: "ready",
            reason: None,
            record: Some(confirmed),
        })
    }
    pub(crate) fn place_current(
        &self,
        store: &Store,
        sessions: &[NativeActor],
        now: u64,
        expected: u64,
        id: &ActorId,
        position: Point,
    ) -> Result<PublicWorld, String> {
        self.synchronize(store, sessions, None, now)?;
        self.place(expected, id, position)
    }
    pub(crate) fn place(
        &self,
        expected: u64,
        id: &ActorId,
        position: Point,
    ) -> Result<PublicWorld, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        world.place_actor(expected, id, position).map_err(|error| {
            if error == crate::world_state::WorldError::StaleRevision {
                WorldStoreError::StaleRevision.message()
            } else {
                INVALID_PLACE
            }
        })?;
        Ok(public(&hub, None))
    }
    pub(crate) fn revoke_actor(&self, store: &Store, id: &str) {
        if let Ok(mut hub) = self.inner.lock() {
            if let Some(stamp) = hub.bound.remove(id) {
                if let Some(world) = &mut hub.world {
                    world.unbind_session(&stamp);
                }
            }
            if let Ok(Some(life)) = store.load_life(id) {
                if let (Some(world), Ok(actor)) = (&mut hub.world, ActorId::new(id)) {
                    let _ = world.observe_actor(&actor, life.state());
                }
            }
        }
    }
    pub(crate) fn stop_all(&self) {
        if let Ok(mut hub) = self.inner.lock() {
            if let Some(world) = &mut hub.world {
                world.stop_all_sessions();
            }
            hub.bound.clear();
        }
    }
    /// Core-held native bridge used by the runtime and the bounded fixture.
    /// Derive the current session only after entering the Core critical section.
    pub(crate) fn action_context(
        &self,
        store: &Store,
        sessions: &[NativeActor],
        bot: &str,
        now: u64,
    ) -> Result<ActionContext, String> {
        self.synchronize(store, sessions, None, now)?;
        let hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        let native = sessions
            .iter()
            .find(|s| s.stamp.actor_id().as_str() == bot)
            .ok_or(UNAVAILABLE)?;
        let world = hub.world.as_ref().ok_or(UNAVAILABLE)?;
        if hub.unavailable.contains(bot)
            || hub.bound.get(bot) != Some(&native.stamp)
            || hub.topology_revision == 0
            || hub
                .topology
                .as_ref()
                .is_none_or(|t| t.observed.elapsed() > Duration::from_secs(5))
        {
            return Err(UNAVAILABLE.into());
        }
        let position = world
            .snapshot()
            .actors
            .into_iter()
            .find(|a| a.actor_id.as_str() == bot)
            .ok_or(UNAVAILABLE)?
            .position;
        Ok(ActionContext {
            session: native.stamp.clone(),
            source_world: native.lineage.clone().ok_or(UNAVAILABLE)?,
            stage: world.config().stage_id().clone(),
            config_revision: world.config().revision(),
            topology_revision: hub.topology_revision,
            position,
            instance_id: hub.instance_id.clone(),
            record: serde_json::to_value(hub.record.as_ref().ok_or(UNAVAILABLE)?)
                .map_err(|_| UNAVAILABLE)?,
        })
    }
    /// Registration observes explicit participants without minting a MemberHandle,
    /// output grant or Life session. Nonselected Studio layers are mirrors only.
    pub(crate) fn registration_context(
        &self,
        store: &Store,
        session: &SessionStamp,
        lineage: &WorldId,
    ) -> Result<(ActionContext, coordinates::Rect), String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        let world = hub.world.as_ref().ok_or(UNAVAILABLE)?;
        if !world.is_participant(session.actor_id())
            || hub.unavailable.contains(session.actor_id().as_str())
            || hub.topology_revision == 0
            || hub
                .topology
                .as_ref()
                .is_none_or(|t| t.observed.elapsed() > Duration::from_secs(5))
        {
            return Err(UNAVAILABLE.into());
        }
        let position = world
            .snapshot()
            .actors
            .into_iter()
            .find(|a| &a.actor_id == session.actor_id())
            .ok_or(UNAVAILABLE)?
            .position;
        Ok((
            ActionContext {
                session: session.clone(),
                source_world: lineage.clone(),
                stage: world.config().stage_id().clone(),
                config_revision: world.config().revision(),
                topology_revision: hub.topology_revision,
                position,
                instance_id: hub.instance_id.clone(),
                record: serde_json::to_value(hub.record.as_ref().ok_or(UNAVAILABLE)?)
                    .map_err(|_| UNAVAILABLE)?,
            },
            world.config().safe_region(),
        ))
    }
    /// Currentness is rechecked after the actual read-through Store wait. The
    /// caller already holds Core; its pure admission closure must not lock or I/O.
    pub(crate) fn apply_visual_progress(
        &self,
        store: &Store,
        expected: &ActionContext,
        position: Point,
        admit: impl FnOnce() -> bool,
    ) -> Result<ActionContext, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        let bot = expected.session.actor_id().as_str();
        let world = hub.world.as_ref().ok_or(UNAVAILABLE)?;
        if hub.instance_id != expected.instance_id
            || hub.bound.get(bot) != Some(&expected.session)
            || hub.unavailable.contains(bot)
            || world.config().stage_id() != &expected.stage
            || world.config().revision() != expected.config_revision
            || hub.topology_revision != expected.topology_revision
            || hub
                .topology
                .as_ref()
                .is_none_or(|t| t.observed.elapsed() > Duration::from_secs(5))
            || world
                .snapshot()
                .actors
                .iter()
                .find(|a| a.actor_id.as_str() == bot)
                .is_none_or(|a| {
                    a.position != expected.position
                        || a.life
                            .as_ref()
                            .is_none_or(|life| life.world_id() != &expected.source_world)
                })
            || !admit()
        {
            return Err(UNAVAILABLE.into());
        }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let handle = world
            .member_handle(expected.session.actor_id())
            .ok_or(UNAVAILABLE)?;
        world
            .move_actor(&handle, position)
            .map_err(|_| UNAVAILABLE)?;
        let mut next = expected.clone();
        next.position = position;
        Ok(next)
    }
    /// Caller holds Core. The closure may enter Store only; no Core/WorldHub
    /// reentry, GUI getter, await or publication while this guard is held.
    pub(crate) fn with_action_context<T>(
        &self,
        store: &Store,
        expected: &ActionContext,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        if !Self::action_context_matches(&hub, expected) {
            return Err(UNAVAILABLE.into());
        }
        operation()
    }
    fn action_context_matches(hub: &Hub, expected: &ActionContext) -> bool {
        let Some(world) = hub.world.as_ref() else { return false; };
        let bot = expected.session.actor_id().as_str();
        hub.instance_id == expected.instance_id
            && hub.bound.get(bot) == Some(&expected.session)
            && !hub.unavailable.contains(bot)
            && world.config().stage_id() == &expected.stage
            && world.config().revision() == expected.config_revision
            && hub.topology_revision == expected.topology_revision
            && hub.topology.as_ref().is_some_and(|t| t.observed.elapsed() <= Duration::from_secs(5))
            && world.snapshot().actors.iter().find(|a| a.actor_id.as_str() == bot).is_some_and(|a|
                a.position == expected.position && a.life.as_ref().is_some_and(|life| life.world_id() == &expected.source_world))
    }
    pub(crate) fn slot_target(&self, store: &Store, expected: &ActionContext, slot: &SlotId) -> Result<ActionSlotTarget, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        if !Self::action_context_matches(&hub, expected) { return Err(UNAVAILABLE.into()); }
        let world = hub.world.as_ref().ok_or(UNAVAILABLE)?;
        let (kind, position) = world.slot_geometry(slot).ok_or(UNAVAILABLE)?;
        let anchor = world.config().anchors().iter().find(|anchor|anchor.slots().iter().any(|candidate|candidate.id()==slot)).ok_or(UNAVAILABLE)?;
        let content_ref = anchor.content_ref().cloned();
        if let Some(reference) = &content_ref {
            let packs = store.content_packs().map_err(|_|UNAVAILABLE)?;
            if crate::content_packs::resolve_object(&packs, reference, kind).is_none() {return Err(UNAVAILABLE.into());}
        }
        Ok(ActionSlotTarget { id: slot.clone(), kind, position, content_ref })
    }
    /// Offers are current observations; admission still reserves the exact slot
    /// in the existing World transaction and may lose a race to another actor.
    pub(crate) fn action_slots(&self, store: &Store, expected: &ActionContext) -> Result<Vec<ActionSlotTarget>, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        if !Self::action_context_matches(&hub, expected) { return Err(UNAVAILABLE.into()); }
        let snapshot = hub.world.as_ref().ok_or(UNAVAILABLE)?.snapshot();
        // Invalid optional content has no offers. Ordinary configured anchors
        // remain usable, and saved package anchors stay visible as fallbacks.
        let packs = snapshot.anchors.iter().any(|anchor|anchor.content_ref().is_some())
            .then(||store.content_packs().ok()).flatten();
        let mut result = Vec::new();
        for anchor in &snapshot.anchors {
            if anchor.content_ref().is_some_and(|reference|packs.as_deref()
                .and_then(|packs|crate::content_packs::resolve_object(packs,reference,anchor.kind())).is_none()) {continue;}
            for slot in anchor.slots() {
                if snapshot.occupied.iter().any(|occupied| occupied.slot_id == *slot.id()) { continue; }
                result.push(ActionSlotTarget { id: slot.id().clone(), kind: anchor.kind(), position: slot.position(), content_ref:anchor.content_ref().cloned() });
            }
        }
        result.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(result)
    }
    /// Current selected display observation, never a window/layout proof. The
    /// host must compare this with its fresh actual Prepared geometry.
    pub(crate) fn native_output_geometry(&self, store: &Store, expected: &ActionContext) -> Result<OutputGeometry, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        if !Self::action_context_matches(&hub, expected) { return Err(UNAVAILABLE.into()); }
        let topology = hub.topology.as_ref().ok_or(UNAVAILABLE)?;
        let record = hub.record.as_ref().ok_or(UNAVAILABLE)?;
        let preferred = record.output_binding.monitor_id.as_deref().unwrap_or_default();
        select(topology, preferred).map(|display| display.geometry.clone()).ok_or(UNAVAILABLE.into())
    }
    fn check_action_slot(hub: &mut Hub, expected: &ActionContext, slot: &SlotReservation, now: u64) -> Result<(), String> {
        // `synchronize` may have waited in Store after sampling its clock. Age
        // the actual World again before rejecting a locally expired lease so
        // its reservation cannot remain publicly occupied at that old sample.
        hub.world.as_mut().ok_or(UNAVAILABLE)?.advance(now).map_err(|_| UNAVAILABLE)?;
        if !slot.fresh(now) || !Self::action_context_matches(hub, expected) { return Err(UNAVAILABLE.into()); }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        if world.slot_geometry(&slot.target.id) != Some((slot.target.kind, slot.target.position)) { return Err(UNAVAILABLE.into()); }
        let member = world.member_handle(expected.session.actor_id()).ok_or(UNAVAILABLE)?;
        if world.validate_reservation(&slot.lease, &member, now).map_err(|_| UNAVAILABLE)? != slot.target.position { return Err(UNAVAILABLE.into()); }
        Ok(())
    }
    /// Core is held by the caller. This must run after the real Plan and after
    /// the earlier with_action_context closure has released this Hub guard.
    pub(crate) fn reserve_action_slot_current(
        &self, core: &crate::life_runtime::core::Core, store: &Store, expected: &ActionContext,
        target: ActionSlotTarget, action: crate::life_domain::ActionId, ttl: u64, now: impl Fn() -> u64,
    ) -> Result<SlotReservation, String> {
        let issued = now();
        let valid_until = issued.checked_add(ttl).filter(|value| *value <= MAX_COUNTER).ok_or(UNAVAILABLE)?;
        let lease = self.reserve_current(core, store, expected.session.actor_id().as_str(), WorkIdentity::Life(action), &target.id, ttl, issued)?;
        let slot = SlotReservation { target, lease, valid_from: issued, valid_until };
        if let Err(error) = self.validate_action_slot_current(core, store, expected, &slot, &now) {
            let _ = self.release(&expected.session, &slot.lease, now());
            return Err(error);
        }
        Ok(slot)
    }
    pub(crate) fn validate_action_slot_current(
        &self, core: &crate::life_runtime::core::Core, store: &Store, expected: &ActionContext, slot: &SlotReservation, now: impl Fn() -> u64,
    ) -> Result<(), String> {
        self.synchronize(store, &current_actors(core), None, now())?;
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        Self::check_action_slot(&mut hub, expected, slot, now())
    }
    pub(crate) fn renew_action_slot_current(
        &self, core: &crate::life_runtime::core::Core, store: &Store, expected: &ActionContext, slot: &mut SlotReservation, ttl: u64, now: impl Fn() -> u64,
    ) -> Result<(), String> {
        self.synchronize(store, &current_actors(core), None, now())?;
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        let renewed = now();
        let valid_until = renewed.checked_add(ttl).filter(|value| *value <= MAX_COUNTER).ok_or(UNAVAILABLE)?;
        Self::check_action_slot(&mut hub, expected, slot, renewed)?;
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(expected.session.actor_id()).ok_or(UNAVAILABLE)?;
        world.renew(&slot.lease, &member, ttl, renewed).map_err(|_| UNAVAILABLE)?;
        slot.valid_from = renewed; slot.valid_until = valid_until;
        Ok(())
    }
    /// Exact lease ownership is held stable by this same Hub guard through
    /// Store CAS. The callback must additionally check slot.fresh(now()) after
    /// SQLite waits; no GUI, Core or World reentry is permitted here.
    pub(crate) fn with_action_slot_context<T>(
        &self, store: &Store, expected: &ActionContext, slot: &SlotReservation, now: impl Fn() -> u64,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        Self::check_action_slot(&mut hub, expected, slot, now())?;
        operation()
    }
    pub(crate) fn apply_slot_progress(
        &self, store: &Store, expected: &ActionContext, slot: &SlotReservation, position: Point,
        now: impl Fn() -> u64, admit: impl FnOnce() -> bool,
    ) -> Result<ActionContext, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        Self::refresh_record(&mut hub, store)?;
        Self::check_action_slot(&mut hub, expected, slot, now())?;
        if !slot.fresh(now()) || !admit() { return Err(UNAVAILABLE.into()); }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(expected.session.actor_id()).ok_or(UNAVAILABLE)?;
        world.move_actor(&member, position).map_err(|_| UNAVAILABLE)?;
        let mut next = expected.clone(); next.position = position; Ok(next)
    }
    /// Exact native revocation proof for failed local work. A new session is
    /// untouched. Pure unbind atomically removes its old session's every lease.
    pub(crate) fn revoke_action_context(&self, expected: &ActionContext) -> Result<(), String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        let bot = expected.session.actor_id().as_str();
        if hub.bound.get(bot) == Some(&expected.session) {
            hub.bound.remove(bot);
            if let Some(world) = &mut hub.world {
                world.unbind_session(&expected.session);
            }
        }
        if hub.bound.get(bot) == Some(&expected.session) {
            Err(UNAVAILABLE.into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn reserve_current(
        &self,
        core: &crate::life_runtime::core::Core,
        store: &Store,
        bot: &str,
        work: WorkIdentity,
        slot: &SlotId,
        ttl: u64,
        now: u64,
    ) -> Result<SlotLease, String> {
        let sessions = current_actors(core);
        self.synchronize(store, &sessions, None, now)?;
        let session = sessions
            .iter()
            .find(|session| session.stamp.actor_id().as_str() == bot)
            .ok_or(UNAVAILABLE)?;
        if let WorkIdentity::Life(requested) = &work {
            let life = store.load_life(bot).map_err(|_| UNAVAILABLE)?;
            let current = life
                .as_ref()
                .and_then(|life| life.state().current_action())
                .filter(|action| {
                    !matches!(
                        action.phase(),
                        crate::life_domain::ActionPhase::Completed
                            | crate::life_domain::ActionPhase::Cancelled
                    )
                });
            if current.is_none_or(|action| action.intent().id() != requested) {
                return Err(UNAVAILABLE.into());
            }
        }
        self.reserve(&session.stamp, work, slot, ttl, now)
    }
    pub(crate) fn reserve(
        &self,
        session: &SessionStamp,
        work: WorkIdentity,
        slot: &SlotId,
        ttl: u64,
        now: u64,
    ) -> Result<SlotLease, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        if hub.topology.as_ref().is_none_or(|topology| {
            topology.observed.elapsed() > Duration::from_secs(5) || topology.displays.is_empty()
        }) {
            return Err(UNAVAILABLE.into());
        }
        if hub.bound.get(session.actor_id().as_str()) != Some(session) {
            return Err(UNAVAILABLE.into());
        }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(session.actor_id()).ok_or(UNAVAILABLE)?;
        world
            .reserve(world.config().revision(), &member, work, slot, ttl, now)
            .map_err(|_| UNAVAILABLE.into())
    }
    pub(crate) fn renew(
        &self,
        session: &SessionStamp,
        lease: &SlotLease,
        ttl: u64,
        now: u64,
    ) -> Result<(), String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        if hub.topology.as_ref().is_none_or(|topology| {
            topology.observed.elapsed() > Duration::from_secs(5) || topology.displays.is_empty()
        }) || hub.bound.get(session.actor_id().as_str()) != Some(session)
        {
            return Err(UNAVAILABLE.into());
        }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(session.actor_id()).ok_or(UNAVAILABLE)?;
        world
            .renew(lease, &member, ttl, now)
            .map_err(|_| UNAVAILABLE.into())
    }
    pub(crate) fn release(
        &self,
        session: &SessionStamp,
        lease: &SlotLease,
        now: u64,
    ) -> Result<bool, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        if hub.bound.get(session.actor_id().as_str()) != Some(session) {
            return Err(UNAVAILABLE.into());
        }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(session.actor_id()).ok_or(UNAVAILABLE)?;
        world
            .release(lease, &member, now)
            .map_err(|_| UNAVAILABLE.into())
    }
    /// Validates an opaque scene reservation against its current actual actor.
    /// The caller holds Core through validation, release and action admission.
    pub(crate) fn validate_current_lease(
        &self, core: &crate::life_runtime::core::Core, store: &Store, bot: &str,
        lease: &SlotLease, slot: &SlotId, now: u64,
    ) -> Result<SessionStamp, String> {
        if lease.slot_id() != slot { return Err(UNAVAILABLE.into()); }
        let sessions = current_actors(core);
        self.synchronize(store, &sessions, None, now)?;
        let session = sessions.iter().find(|actor| actor.stamp.actor_id().as_str() == bot).ok_or(UNAVAILABLE)?;
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        if hub.bound.get(bot) != Some(&session.stamp) { return Err(UNAVAILABLE.into()); }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(session.stamp.actor_id()).ok_or(UNAVAILABLE)?;
        world.validate_reservation(lease, &member, now).map_err(|_| UNAVAILABLE)?;
        Ok(session.stamp.clone())
    }
    /// Idempotent retirement of one opaque scene lease, never a slot-ID delete.
    /// A replacement lease is preserved; only typed expiry is an absent result.
    pub(crate) fn release_if_present(&self, session: &SessionStamp, lease: &SlotLease, now: u64) -> Result<bool, String> {
        let mut hub = self.inner.lock().map_err(|_| UNAVAILABLE)?;
        if hub.bound.get(session.actor_id().as_str()) != Some(session) { return Err(UNAVAILABLE.into()); }
        let world = hub.world.as_mut().ok_or(UNAVAILABLE)?;
        let member = world.member_handle(session.actor_id()).ok_or(UNAVAILABLE)?;
        match world.release(lease, &member, now) {
            Ok(released) => Ok(released),
            Err(crate::world_state::WorldError::Expired) => Ok(false),
            Err(_) => Err(UNAVAILABLE.into()),
        }
    }
    pub(super) fn output_targets(&self) -> Vec<String> {
        let Ok(mut hub) = self.inner.lock() else {
            return Vec::new();
        };
        let current = hub
            .record
            .as_ref()
            .map(|record| {
                record
                    .config
                    .members()
                    .iter()
                    .map(|actor| actor.as_str().to_owned())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let targets = current.union(&hub.published).cloned().collect();
        hub.published = current;
        targets
    }
    pub(crate) fn output_event(&self, id: &str, profile: &Value) -> Option<Value> {
        let hub = self.inner.lock().ok()?;
        let actor = ActorId::new(id).ok()?;
        let world = hub.world.as_ref()?;
        if !world.is_participant(&actor) {
            return None;
        }
        let arena = if profile["output"]["mode"] == "overlay" {
            hub.topology
                .as_ref()
                .filter(|topology| topology.observed.elapsed() <= Duration::from_secs(5))
                .and_then(|topology| {
                    select(
                        topology,
                        hub.record
                            .as_ref()?
                            .output_binding
                            .monitor_id
                            .as_deref()
                            .unwrap_or_default(),
                    )
                })
                .map(|display| display.geometry.logical_size())
        } else {
            let width = profile["output"]["width"].as_f64().unwrap_or(520.0);
            let height = profile["output"]["height"].as_f64().unwrap_or(780.0);
            if [width, height].into_iter().all(|value| {
                value.is_finite() && value > 0.0 && value <= coordinates::MAX_DIMENSION
            }) {
                Some(LogicalSize { width, height })
            } else {
                None
            }
        };
        // Arena metadata is observation only. Measured mesh placement and
        // physical presentation are deliberately unavailable until #72.
        Some(
            json!({"kind":"world-state","botId":id,"world":public(&hub,Some(&actor)),"arena":{"available":arena.is_some(),"topologyRevision":hub.topology_revision,"logicalSize":arena.map(|arena|json!({"width":arena.width,"height":arena.height})),"presentationAvailable":false}}),
        )
    }
}
fn select<'a>(topology: &'a TopologySample, preferred: &str) -> Option<&'a DisplaySample> {
    topology
        .displays
        .iter()
        .find(|display| display.geometry.id() == preferred)
        .or_else(|| {
            topology
                .displays
                .iter()
                .min_by(|a, b| a.geometry.id().cmp(b.geometry.id()))
        })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod content_offer_tests {
    // Actual SQLite/World offer lookup only. No renderer/native surface grant,
    // OS input, capture, provider or fabricated completion participates here.
    use super::*;
    use crate::{content_packs::{ContentRef,Manifest},life_domain::{Needs,RandomState},
        world_state::{coordinates::Rect,Anchor,AnchorId,Slot,StageId}};
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new()->Self {let path=std::env::temp_dir().join(format!("orbitpal-world-offers-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&path).unwrap();Self(path)}
    }
    impl Drop for Fixture {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    #[test]
    fn inactive_and_replaced_packages_keep_visible_anchors_but_have_no_native_targets() {
        let fixture=Fixture::new();let store=Store::new(fixture.0.clone()).unwrap();
        let manifest=Manifest::parse(include_str!("../../../public/content-packs/desk-garden.json")).unwrap();
        let reference=ContentRef{package_id:manifest.id.clone(),package_version:1,template_id:manifest.objects[0].id.clone()};
        store.install_content_pack(manifest.clone()).unwrap();store.set_content_pack_active(&reference.package_id,1,true).unwrap();
        let lineage=WorldId::new("content-lineage").unwrap();
        store.initialize_life("nova",lineage.clone(),Needs::new(5000,5000,5000,5000).unwrap(),RandomState::seeded(7),1000).unwrap();
        let config=WorldConfig::new(StageId::new("content-stage").unwrap(),Rect::full(),vec![ActorId::new("nova").unwrap()],vec![
            Anchor::new(AnchorId::new("seed").unwrap(),AnchorKind::Play,Point::new(200000,500000).unwrap(),vec![Slot::new(SlotId::new("seed-slot").unwrap(),Point::new(200000,500000).unwrap())]).unwrap(),
            Anchor::new(AnchorId::new("rest").unwrap(),AnchorKind::Rest,Point::new(800000,500000).unwrap(),vec![Slot::new(SlotId::new("rest-slot").unwrap(),Point::new(800000,500000).unwrap())]).unwrap(),
        ]).unwrap();
        let mut wire=serde_json::to_value(config).unwrap();wire["anchors"][0]["contentRef"]=serde_json::to_value(&reference).unwrap();
        let config:WorldConfig=serde_json::from_value(wire).unwrap();
        let actors=vec![NativeActor{stamp:SessionStamp::new(ActorId::new("nova").unwrap(),"content-session",1).unwrap(),lineage:Some(lineage)}];
        let topology=TopologySample {ticket:1,observed:Instant::now(),displays:vec![DisplaySample{geometry:OutputGeometry::new("fixture-display",0,0,1920,1200,1.0).unwrap()}],fingerprint:vec![("fixture-display".into(),0,0,1920,1200,1.0f64.to_bits())]};
        let hub=WorldHub::new(&store);
        hub.save(&store,None,&config,&OutputBinding{monitor_id:Some("fixture-display".into())},&actors,topology,0).unwrap();
        let context=hub.action_context(&store,&actors,"nova",0).unwrap();
        let offered=hub.action_slots(&store,&context).unwrap();assert_eq!(offered.len(),2);
        assert_eq!(hub.slot_target(&store,&context,&SlotId::new("seed-slot").unwrap()).unwrap().content_ref,Some(reference.clone()));
        assert!(hub.slot_target(&store,&context,&SlotId::new("rest-slot").unwrap()).unwrap().content_ref.is_none());
        store.set_content_pack_active(&reference.package_id,2,false).unwrap();
        let offered=hub.action_slots(&store,&context).unwrap();assert_eq!(offered.len(),1);assert_eq!(offered[0].id.as_str(),"rest-slot");
        assert!(hub.slot_target(&store,&context,&SlotId::new("seed-slot").unwrap()).is_err());
        let snapshot=hub.snapshot().unwrap().snapshot.unwrap();assert_eq!(snapshot.anchors.len(),2);
        assert!(snapshot.anchors.iter().any(|anchor|anchor.content_ref()==Some(&reference)));
        let mut upgraded=manifest;upgraded.version=2;
        store.install_content_pack(upgraded).unwrap();let revision=store.content_packs().unwrap()[0].revision;
        store.set_content_pack_active(&reference.package_id,revision,true).unwrap();
        assert!(hub.slot_target(&store,&context,&SlotId::new("seed-slot").unwrap()).is_err(),"version2 must not relabel the savedversion1 target");
        assert_eq!(hub.action_slots(&store,&context).unwrap().len(),1);
    }
}
