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
        RuntimeEpoch, SessionStamp, SlotId, SlotLease, WorkIdentity, WorldConfig, WorldSnapshot,
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
        let world = hub.world.as_ref().ok_or(UNAVAILABLE)?;
        let bot = expected.session.actor_id().as_str();
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
        {
            return Err(UNAVAILABLE.into());
        }
        operation()
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
