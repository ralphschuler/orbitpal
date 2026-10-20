//! Bot-bound SQLite persistence for the pure life domain. No runtime/session,
//! provider or presentation entry point is installed by this module.
use super::{valid_id, Store};
use crate::life_domain::{
    self, ActionIntent, ActionPhase, ActorId, CancelReason, LifeError, LifeEvent, LifeRecord,
    LifeState, Needs, Outcome, RandomState, WorldId,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::{
    collections::BTreeMap,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

pub const STORAGE_SCHEMA_VERSION: u16 = 1;
pub const MAX_STATE_BYTES: usize = 256 * 1024;
pub const MAX_EVENT_BYTES: usize = 8 * 1024;
pub const MAX_LIFE_BOTS: usize = 1024;
pub const SNAPSHOT_INTERVAL_MS: i64 = 30_000;
pub const MAX_OFFLINE_MS: u64 = life_domain::MAX_ELAPSED_MS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifeStoreError {
    InvalidInput,
    WrongBot,
    UnknownBot,
    AlreadyExists,
    MissingState,
    RevisionConflict,
    UnsupportedSchema,
    CorruptData,
    LimitExceeded,
    StorageUnavailable,
    Domain(LifeError),
}
impl fmt::Display for LifeStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid life storage input",
            Self::WrongBot => "life storage context belongs to another bot",
            Self::UnknownBot => "life storage bot does not exist",
            Self::AlreadyExists => "life state already exists",
            Self::MissingState => "life state has not been initialized",
            Self::RevisionConflict => "life state changed; reload before retrying",
            Self::UnsupportedSchema => "unsupported life storage schema",
            Self::CorruptData => "stored life data is invalid",
            Self::LimitExceeded => "life storage limit exceeded",
            Self::StorageUnavailable => "life storage is unavailable",
            Self::Domain(_) => "life state transition was rejected",
        })
    }
}
impl std::error::Error for LifeStoreError {}
impl From<LifeError> for LifeStoreError {
    fn from(error: LifeError) -> Self {
        Self::Domain(error)
    }
}
fn database(_: rusqlite::Error) -> LifeStoreError {
    LifeStoreError::StorageUnavailable
}

#[derive(Clone, Debug, Default)]
pub(super) struct RecoveryStatus {
    schema_error: Option<LifeStoreError>,
    bot_errors: BTreeMap<String, LifeStoreError>,
    unidentified_rows: usize,
}
impl RecoveryStatus {
    pub(super) fn new(schema_error: Option<LifeStoreError>) -> Self {
        Self {
            schema_error,
            bot_errors: BTreeMap::new(),
            unidentified_rows: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifeRecoveryReport {
    pub schema_error: Option<LifeStoreError>,
    pub unavailable_bots: usize,
    pub unidentified_rows: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifeAvailability {
    Ready,
    NotInitialized,
    Unavailable(LifeStoreError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventOrigin {
    Live,
    Restart,
    Offline,
}
impl EventOrigin {
    fn encoded(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Restart => "restart",
            Self::Offline => "offline",
        }
    }
    fn parse(value: &str) -> Result<Self, LifeStoreError> {
        match value {
            "live" => Ok(Self::Live),
            "restart" => Ok(Self::Restart),
            "offline" => Ok(Self::Offline),
            _ => Err(LifeStoreError::CorruptData),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredLifeEvent {
    record: LifeRecord,
    origin: EventOrigin,
}
impl StoredLifeEvent {
    pub fn record(&self) -> &LifeRecord {
        &self.record
    }
    pub fn origin(&self) -> EventOrigin {
        self.origin
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifeSnapshot {
    state: LifeState,
    random: RandomState,
    saved_at_ms: u64,
    events: Vec<StoredLifeEvent>,
}
impl LifeSnapshot {
    pub fn state(&self) -> &LifeState {
        &self.state
    }
    pub fn random(&self) -> RandomState {
        self.random
    }
    pub fn saved_at_ms(&self) -> u64 {
        self.saved_at_ms
    }
    pub fn events(&self) -> &[StoredLifeEvent] {
        &self.events
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteOutcome {
    Applied,
    Deferred,
    NoChange,
    Duplicate,
    Stale,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifeCommit {
    pub snapshot: LifeSnapshot,
    pub outcome: WriteOutcome,
    pub intents: Vec<ActionIntent>,
}

pub(super) fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(life_domain::MAX_COUNTER)) as u64
}
fn validate_time(value: u64) -> Result<(), LifeStoreError> {
    if value > life_domain::MAX_COUNTER {
        Err(LifeStoreError::InvalidInput)
    } else {
        Ok(())
    }
}
fn table_exists(db: &Connection, name: &str) -> Result<bool, LifeStoreError> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )
    .map_err(database)
}
/// The only supported old layout is the real pre-life database (no life tables).
/// Partial/unversioned prototypes and unknown future schemas are never guessed.
pub(super) fn initialize(db: &mut Connection) -> Result<(), LifeStoreError> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database)?;
    let meta = table_exists(&tx, "life_schema")?;
    let states = table_exists(&tx, "bot_life_states")?;
    let events = table_exists(&tx, "bot_life_events")?;
    if meta {
        let versions = {
            let mut stmt = tx
                .prepare("SELECT singleton,version FROM life_schema LIMIT 2")
                .map_err(database)?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
                .map_err(database)?;
            let values = rows.collect::<Result<Vec<_>, _>>().map_err(database)?;
            values
        };
        if versions.len() != 1 || versions[0].0 != 1 {
            return Err(LifeStoreError::CorruptData);
        }
        if versions[0].1 != i64::from(STORAGE_SCHEMA_VERSION) {
            return Err(LifeStoreError::UnsupportedSchema);
        }
        if !states || !events {
            return Err(LifeStoreError::CorruptData);
        }
    } else {
        if states || events {
            return Err(LifeStoreError::UnsupportedSchema);
        }
        tx.execute_batch("CREATE TABLE life_schema(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL);
            INSERT INTO life_schema VALUES(1,1);
            CREATE TABLE bot_life_states(bot_id TEXT PRIMARY KEY NOT NULL REFERENCES bot_profiles(id),schema_version INTEGER NOT NULL,revision INTEGER NOT NULL,saved_at_ms INTEGER NOT NULL,random_state TEXT NOT NULL,state_json TEXT NOT NULL CHECK(length(CAST(state_json AS BLOB))<=262144));
            CREATE TABLE bot_life_events(bot_id TEXT NOT NULL REFERENCES bot_life_states(bot_id),revision INTEGER NOT NULL,origin TEXT NOT NULL CHECK(origin IN ('live','restart','offline')),event_json TEXT NOT NULL CHECK(length(CAST(event_json AS BLOB))<=8192),PRIMARY KEY(bot_id,revision));").map_err(database)?;
    }
    tx.commit().map_err(database)
}

fn encode<T: serde::Serialize>(value: &T, limit: usize) -> Result<String, LifeStoreError> {
    let value = serde_json::to_string(value).map_err(|_| LifeStoreError::CorruptData)?;
    if value.len() > limit {
        return Err(LifeStoreError::LimitExceeded);
    }
    Ok(value)
}
fn parse_random(value: &str) -> Result<RandomState, LifeStoreError> {
    if value.len() != 16
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(LifeStoreError::CorruptData);
    }
    Ok(RandomState::seeded(
        u64::from_str_radix(value, 16).map_err(|_| LifeStoreError::CorruptData)?,
    ))
}
fn check_bot(db: &Connection, bot_id: &str) -> Result<(), LifeStoreError> {
    if !db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",
            [bot_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database)?
    {
        return Err(LifeStoreError::UnknownBot);
    }
    Ok(())
}
fn read(db: &Connection, bot_id: &str) -> Result<Option<LifeSnapshot>, LifeStoreError> {
    // Recheck live metadata: a future schema introduced by another connection
    // cannot be read or overwritten through a previously opened Store.
    let metadata = {
        let mut query = db
            .prepare("SELECT singleton,version FROM life_schema LIMIT 2")
            .map_err(database)?;
        let rows = query
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .map_err(database)?;
        let values = rows.collect::<Result<Vec<_>, _>>().map_err(database)?;
        values
    };
    if metadata.len() != 1 || metadata[0].0 != 1 {
        return Err(LifeStoreError::CorruptData);
    }
    if metadata[0].1 != i64::from(STORAGE_SCHEMA_VERSION) {
        return Err(LifeStoreError::UnsupportedSchema);
    }
    // CASE limits what SQLite hands to Rust; an oversized/corrupt DB value is
    // rejected before allocating/deserializing its JSON in the application.
    let row = db.query_row("SELECT schema_version,revision,saved_at_ms,CASE WHEN length(CAST(random_state AS BLOB))=16 THEN random_state ELSE NULL END,length(CAST(state_json AS BLOB)),CASE WHEN length(CAST(state_json AS BLOB))<=262144 THEN state_json ELSE NULL END FROM bot_life_states WHERE bot_id=?1",[bot_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,i64>(4)?,row.get::<_,Option<String>>(5)?))).optional().map_err(database)?;
    let Some((schema, revision, saved_at, random, bytes, encoded)) = row else {
        let orphan: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bot_life_events WHERE bot_id=?1)",
                [bot_id],
                |row| row.get(0),
            )
            .map_err(database)?;
        return if orphan {
            Err(LifeStoreError::CorruptData)
        } else {
            Ok(None)
        };
    };
    if schema != i64::from(STORAGE_SCHEMA_VERSION) {
        return Err(LifeStoreError::UnsupportedSchema);
    }
    if revision < 0
        || saved_at < 0
        || saved_at as u64 > life_domain::MAX_COUNTER
        || !(1..=MAX_STATE_BYTES as i64).contains(&bytes)
    {
        return Err(LifeStoreError::CorruptData);
    }
    let encoded = encoded.as_deref().ok_or(LifeStoreError::LimitExceeded)?;
    #[derive(serde::Deserialize)]
    struct DomainSchema {
        #[serde(rename = "schemaVersion")]
        schema_version: u16,
    }
    let schema: DomainSchema =
        serde_json::from_str(encoded).map_err(|_| LifeStoreError::CorruptData)?;
    if schema.schema_version != life_domain::SCHEMA_VERSION {
        return Err(LifeStoreError::UnsupportedSchema);
    }
    let state: LifeState =
        serde_json::from_str(encoded).map_err(|_| LifeStoreError::CorruptData)?;
    if state.actor_id().as_str() != bot_id || state.revision() != revision as u64 {
        return Err(LifeStoreError::CorruptData);
    }
    let random = parse_random(random.as_deref().ok_or(LifeStoreError::CorruptData)?)?;
    let mut stmt = db.prepare("SELECT revision,CASE WHEN length(CAST(origin AS BLOB))<=7 THEN origin ELSE NULL END,length(CAST(event_json AS BLOB)),CASE WHEN length(CAST(event_json AS BLOB))<=8192 THEN event_json ELSE NULL END FROM bot_life_events WHERE bot_id=?1 ORDER BY revision LIMIT ?2").map_err(database)?;
    let mut rows = stmt
        .query(params![bot_id, (life_domain::MAX_RECENT_EVENTS + 1) as i64])
        .map_err(database)?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().map_err(database)? {
        if events.len() == life_domain::MAX_RECENT_EVENTS {
            return Err(LifeStoreError::CorruptData);
        }
        let sequence: i64 = row.get(0).map_err(database)?;
        let origin: Option<String> = row.get(1).map_err(database)?;
        let bytes: i64 = row.get(2).map_err(database)?;
        if !(1..=MAX_EVENT_BYTES as i64).contains(&bytes) {
            return Err(LifeStoreError::CorruptData);
        }
        let encoded: Option<String> = row.get(3).map_err(database)?;
        let record: LifeRecord =
            serde_json::from_str(encoded.as_deref().ok_or(LifeStoreError::CorruptData)?)
                .map_err(|_| LifeStoreError::CorruptData)?;
        if sequence < 1
            || record.id().sequence() != sequence as u64
            || record.id().actor_id().as_str() != bot_id
        {
            return Err(LifeStoreError::CorruptData);
        }
        let origin = EventOrigin::parse(origin.as_deref().ok_or(LifeStoreError::CorruptData)?)?;
        let valid_origin = match (origin, record.event()) {
            (EventOrigin::Offline, life_domain::CommittedEvent::Advanced { .. }) => true,
            (
                EventOrigin::Restart,
                life_domain::CommittedEvent::Cancelled {
                    reason: CancelReason::Restart,
                    ..
                },
            ) => true,
            (
                EventOrigin::Live,
                life_domain::CommittedEvent::Cancelled {
                    reason: CancelReason::Restart,
                    ..
                },
            ) => false,
            (EventOrigin::Live, _) => true,
            _ => false,
        };
        if !valid_origin {
            return Err(LifeStoreError::CorruptData);
        }
        events.push(StoredLifeEvent { record, origin });
    }
    if events.len() != state.recent_events().len()
        || events
            .iter()
            .zip(state.recent_events())
            .any(|(saved, current)| &saved.record != current)
    {
        return Err(LifeStoreError::CorruptData);
    }
    Ok(Some(LifeSnapshot {
        state,
        random,
        saved_at_ms: saved_at as u64,
        events,
    }))
}
fn append_transition(
    snapshot: &mut LifeSnapshot,
    transition: life_domain::Transition,
    origin: EventOrigin,
) -> Result<(), LifeStoreError> {
    if transition.outcome != Outcome::Applied {
        return Err(LifeStoreError::CorruptData);
    }
    let record = transition
        .state
        .recent_events()
        .last()
        .ok_or(LifeStoreError::CorruptData)?
        .clone();
    snapshot.state = transition.state;
    snapshot.random = transition.random;
    snapshot.events.push(StoredLifeEvent { record, origin });
    if snapshot.events.len() > life_domain::MAX_RECENT_EVENTS {
        snapshot.events.remove(0);
    }
    Ok(())
}
fn save(
    tx: &Transaction<'_>,
    bot_id: &str,
    expected_revision: u64,
    snapshot: &LifeSnapshot,
) -> Result<(), LifeStoreError> {
    snapshot.state.validate()?;
    if snapshot.state.actor_id().as_str() != bot_id || snapshot.state.revision() < expected_revision
    {
        return Err(LifeStoreError::CorruptData);
    }
    validate_time(snapshot.saved_at_ms)?;
    let encoded = encode(&snapshot.state, MAX_STATE_BYTES)?;
    let changed = tx.execute("UPDATE bot_life_states SET schema_version=?1,revision=?2,saved_at_ms=?3,random_state=?4,state_json=?5 WHERE bot_id=?6 AND revision=?7",params![STORAGE_SCHEMA_VERSION,snapshot.state.revision() as i64,snapshot.saved_at_ms as i64,format!("{:016x}",snapshot.random.state()),encoded,bot_id,expected_revision as i64]).map_err(database)?;
    if changed != 1 {
        return Err(LifeStoreError::RevisionConflict);
    }
    for event in snapshot
        .events
        .iter()
        .filter(|event| event.record.id().sequence() > expected_revision)
    {
        tx.execute(
            "INSERT INTO bot_life_events(bot_id,revision,origin,event_json) VALUES(?1,?2,?3,?4)",
            params![
                bot_id,
                event.record.id().sequence() as i64,
                event.origin.encoded(),
                encode(&event.record, MAX_EVENT_BYTES)?
            ],
        )
        .map_err(database)?;
    }
    let threshold = snapshot
        .state
        .revision()
        .saturating_sub(life_domain::MAX_RECENT_EVENTS as u64);
    tx.execute(
        "DELETE FROM bot_life_events WHERE bot_id=?1 AND revision<=?2",
        params![bot_id, threshold as i64],
    )
    .map_err(database)?;
    // Validate the actual transaction contents too, including altered SQL
    // triggers/partial writes. Failure drops the entire transaction unchanged.
    if read(tx, bot_id)?.as_ref() != Some(snapshot) {
        return Err(LifeStoreError::CorruptData);
    }
    Ok(())
}

impl Store {
    fn life_scope(&self, bot_id: &str) -> Result<(), LifeStoreError> {
        if !valid_id(bot_id) {
            return Err(LifeStoreError::InvalidInput);
        }
        if self.fixed_profile.as_deref().is_some_and(|id| id != bot_id) {
            return Err(LifeStoreError::WrongBot);
        }
        Ok(())
    }
    fn life_ready(&self, bot_id: &str) -> Result<(), LifeStoreError> {
        let status = self
            .life_recovery
            .lock()
            .map_err(|_| LifeStoreError::StorageUnavailable)?;
        if let Some(error) = status
            .schema_error
            .or_else(|| status.bot_errors.get(bot_id).copied())
        {
            return Err(error);
        }
        Ok(())
    }
    /// Bounded startup diagnostics contain counts/fixed categories only. Scoped
    /// stores report their own identified bot errors, without other bot IDs.
    pub fn life_recovery_report(&self) -> Result<LifeRecoveryReport, LifeStoreError> {
        let status = self
            .life_recovery
            .lock()
            .map_err(|_| LifeStoreError::StorageUnavailable)?;
        let unavailable_bots = if let Some(bot_id) = self.fixed_profile.as_deref() {
            usize::from(status.bot_errors.contains_key(bot_id))
        } else {
            status.bot_errors.len()
        };
        Ok(LifeRecoveryReport {
            schema_error: status.schema_error,
            unavailable_bots,
            unidentified_rows: status.unidentified_rows,
        })
    }
    /// Native discovery for #69/#78. Unavailable life data does not make normal
    /// profile/chat/settings access fail and is never treated as an empty state.
    pub fn life_availability(&self, bot_id: &str) -> Result<LifeAvailability, LifeStoreError> {
        self.life_scope(bot_id)?;
        {
            let db = self
                .db
                .lock()
                .map_err(|_| LifeStoreError::StorageUnavailable)?;
            check_bot(&db, bot_id)?;
        }
        if let Err(error) = self.life_ready(bot_id) {
            return Ok(LifeAvailability::Unavailable(error));
        }
        match self.load_life(bot_id) {
            Ok(Some(_)) => Ok(LifeAvailability::Ready),
            Ok(None) => Ok(LifeAvailability::NotInitialized),
            Err(error) => Ok(LifeAvailability::Unavailable(error)),
        }
    }
    pub fn initialize_life(
        &self,
        bot_id: &str,
        world_id: WorldId,
        needs: Needs,
        random: RandomState,
        now_ms: u64,
    ) -> Result<LifeSnapshot, LifeStoreError> {
        self.life_scope(bot_id)?;
        self.life_ready(bot_id)?;
        validate_time(now_ms)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| LifeStoreError::StorageUnavailable)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        check_bot(&tx, bot_id)?;
        if read(&tx, bot_id)?.is_some() {
            return Err(LifeStoreError::AlreadyExists);
        }
        let count: i64 = tx
            .query_row("SELECT count(*) FROM bot_life_states", [], |row| row.get(0))
            .map_err(database)?;
        if count >= MAX_LIFE_BOTS as i64 {
            return Err(LifeStoreError::LimitExceeded);
        }
        let state = LifeState::new(ActorId::new(bot_id)?, world_id, needs);
        state.validate()?;
        tx.execute("INSERT INTO bot_life_states(bot_id,schema_version,revision,saved_at_ms,random_state,state_json) VALUES(?1,?2,0,?3,?4,?5)",params![bot_id,STORAGE_SCHEMA_VERSION,now_ms as i64,format!("{:016x}",random.state()),encode(&state,MAX_STATE_BYTES)?]).map_err(database)?;
        let snapshot = read(&tx, bot_id)?.ok_or(LifeStoreError::MissingState)?;
        tx.commit().map_err(database)?;
        Ok(snapshot)
    }
    pub fn load_life(&self, bot_id: &str) -> Result<Option<LifeSnapshot>, LifeStoreError> {
        self.life_scope(bot_id)?;
        self.life_ready(bot_id)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| LifeStoreError::StorageUnavailable)?;
        let tx = db.transaction().map_err(database)?;
        check_bot(&tx, bot_id)?;
        let snapshot = read(&tx, bot_id)?;
        tx.commit().map_err(database)?;
        Ok(snapshot)
    }
    pub fn apply_life_event(
        &self,
        bot_id: &str,
        expected_revision: u64,
        elapsed_ms: i64,
        event: LifeEvent,
        now_ms: u64,
    ) -> Result<LifeCommit, LifeStoreError> {
        self.life_scope(bot_id)?;
        self.life_ready(bot_id)?;
        validate_time(now_ms)?;
        if matches!(
            &event,
            LifeEvent::Cancel {
                reason: CancelReason::Restart,
                ..
            }
        ) {
            return Err(LifeStoreError::InvalidInput);
        }
        if expected_revision > life_domain::MAX_COUNTER {
            return Err(LifeStoreError::InvalidInput);
        }
        let mut db = self
            .db
            .lock()
            .map_err(|_| LifeStoreError::StorageUnavailable)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        check_bot(&tx, bot_id)?;
        let mut snapshot = read(&tx, bot_id)?.ok_or(LifeStoreError::MissingState)?;
        if snapshot.state.revision() != expected_revision {
            return Err(LifeStoreError::RevisionConflict);
        }
        if matches!(&event, LifeEvent::Tick {}) && elapsed_ms < SNAPSHOT_INTERVAL_MS {
            return Ok(LifeCommit {
                snapshot,
                outcome: WriteOutcome::Deferred,
                intents: Vec::new(),
            });
        }
        let transition = life_domain::reduce(&snapshot.state, elapsed_ms, snapshot.random, event)?;
        let intents = transition.intents.clone();
        let outcome = match transition.outcome {
            Outcome::Applied => WriteOutcome::Applied,
            Outcome::NoChange => WriteOutcome::NoChange,
            Outcome::Duplicate => WriteOutcome::Duplicate,
            Outcome::Stale => WriteOutcome::Stale,
        };
        if transition.outcome == Outcome::Applied {
            append_transition(&mut snapshot, transition, EventOrigin::Live)?;
            snapshot.saved_at_ms = now_ms;
            save(&tx, bot_id, expected_revision, &snapshot)?;
        }
        tx.commit().map_err(database)?;
        Ok(LifeCommit {
            snapshot,
            outcome,
            intents,
        })
    }
    /// Native action admission linearizes at `authorize`, after acquiring the
    /// IMMEDIATE transaction and loading the actual current validated snapshot.
    /// The callback must be synchronous, with no Core/Hub/Store reentry or GUI.
    /// Its opaque token never enters SQLite; a committed state remains final
    /// even if later acknowledgement/publication fails. Existing event API stays
    /// unchanged for ticks/manual compatibility.
    pub(crate) fn apply_life_action_checked<T>(
        &self,
        bot_id: &str,
        now_ms: u64,
        authorize: impl FnOnce(
            &LifeSnapshot,
            &Connection,
        ) -> Result<(LifeEvent, u64, T), LifeStoreError>,
    ) -> Result<(LifeCommit, T), LifeStoreError> {
        self.life_scope(bot_id)?;
        self.life_ready(bot_id)?;
        validate_time(now_ms)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| LifeStoreError::StorageUnavailable)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        check_bot(&tx, bot_id)?;
        let mut snapshot = read(&tx, bot_id)?.ok_or(LifeStoreError::MissingState)?;
        // Do not move this callback before the potentially blocking DB acquire.
        let (event, expected_revision, permit) = authorize(&snapshot, &tx)?;
        if expected_revision > life_domain::MAX_COUNTER
            || !matches!(
                &event,
                LifeEvent::Plan { .. }
                    | LifeEvent::Start { .. }
                    | LifeEvent::Complete { .. }
                    | LifeEvent::Cancel { .. }
            )
            || matches!(
                &event,
                LifeEvent::Cancel {
                    reason: CancelReason::Restart,
                    ..
                }
            )
        {
            return Err(LifeStoreError::InvalidInput);
        }
        if snapshot.state.revision() != expected_revision {
            return Err(LifeStoreError::RevisionConflict);
        }
        let transition = life_domain::reduce(&snapshot.state, 0, snapshot.random, event)?;
        let intents = transition.intents.clone();
        let outcome = match transition.outcome {
            Outcome::Applied => WriteOutcome::Applied,
            Outcome::NoChange => WriteOutcome::NoChange,
            Outcome::Duplicate => WriteOutcome::Duplicate,
            Outcome::Stale => WriteOutcome::Stale,
        };
        if transition.outcome == Outcome::Applied {
            append_transition(&mut snapshot, transition, EventOrigin::Live)?;
            snapshot.saved_at_ms = now_ms;
            save(&tx, bot_id, expected_revision, &snapshot)?;
        }
        tx.commit().map_err(database)?;
        Ok((
            LifeCommit {
                snapshot,
                outcome,
                intents,
            },
            permit,
        ))
    }
    pub(super) fn recover_life_after_restart(&self, now_ms: u64) {
        let schema_error = match self.life_recovery.lock() {
            Ok(status) => status.schema_error,
            Err(_) => return, // Poisoned availability makes every Life API fail closed.
        };
        if schema_error.is_some() {
            return;
        }
        let mut status = RecoveryStatus::default();
        let recover = || -> Result<RecoveryStatus, LifeStoreError> {
            validate_time(now_ms)?;
            let mut db = self
                .db
                .lock()
                .map_err(|_| LifeStoreError::StorageUnavailable)?;
            let bots = {
                let mut stmt = db.prepare("SELECT CASE WHEN typeof(bot_id)='text' AND length(CAST(bot_id AS BLOB)) BETWEEN 1 AND 96 THEN CAST(bot_id AS BLOB) ELSE NULL END FROM bot_life_states ORDER BY bot_id LIMIT ?1").map_err(database)?;
                let rows = stmt
                    .query_map([(MAX_LIFE_BOTS + 1) as i64], |row| {
                        row.get::<_, Option<Vec<u8>>>(0)
                    })
                    .map_err(database)?;
                let values = rows.collect::<Result<Vec<_>, _>>().map_err(database)?;
                values
            };
            if bots.len() > MAX_LIFE_BOTS {
                return Err(LifeStoreError::LimitExceeded);
            }
            let mut report = RecoveryStatus::default();
            for bot_id in bots {
                let Some(bot_id) = bot_id
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .filter(|id| valid_id(id))
                else {
                    report.unidentified_rows += 1;
                    continue;
                };
                let mut one = || -> Result<(), LifeStoreError> {
                    let tx = db
                        .transaction_with_behavior(TransactionBehavior::Immediate)
                        .map_err(database)?;
                    check_bot(&tx, &bot_id)?;
                    let mut snapshot = read(&tx, &bot_id)?.ok_or(LifeStoreError::MissingState)?;
                    let revision = snapshot.state.revision();
                    let elapsed = now_ms
                        .saturating_sub(snapshot.saved_at_ms)
                        .min(MAX_OFFLINE_MS);
                    if let Some(action) = snapshot.state.current_action().filter(|action| {
                        matches!(action.phase(), ActionPhase::Planned | ActionPhase::Started)
                    }) {
                        let transition = life_domain::reduce(
                            &snapshot.state,
                            0,
                            snapshot.random,
                            LifeEvent::Cancel {
                                action_id: action.intent().id().clone(),
                                reason: CancelReason::Restart,
                            },
                        )?;
                        append_transition(&mut snapshot, transition, EventOrigin::Restart)?;
                    }
                    if elapsed > 0 {
                        let transition = life_domain::reduce(
                            &snapshot.state,
                            elapsed as i64,
                            snapshot.random,
                            LifeEvent::Tick {},
                        )?;
                        append_transition(&mut snapshot, transition, EventOrigin::Offline)?;
                    }
                    if snapshot.state.revision() != revision || snapshot.saved_at_ms != now_ms {
                        snapshot.saved_at_ms = now_ms;
                        save(&tx, &bot_id, revision, &snapshot)?;
                    }
                    tx.commit().map_err(database)
                };
                if let Err(error) = one() {
                    report.bot_errors.insert(bot_id, error);
                }
            }
            Ok(report)
        };
        match recover() {
            Ok(report) => status = report,
            Err(error) => status.schema_error = Some(error),
        }
        if let Ok(mut current) = self.life_recovery.lock() {
            *current = status;
        }
    }
}

/// An imported profile may not inherit an orphaned local life lineage. If an
/// unsupported layout cannot be inspected, conservatively allocate a new bot
/// identity while leaving optional life data untouched and unavailable.
pub(super) fn profile_id_has_life(db: &Connection, bot_id: &str) -> bool {
    for table in ["bot_life_states", "bot_life_events"] {
        match table_exists(db, table) {
            Ok(false) => continue,
            Ok(true) => (),
            Err(_) => return true,
        }
        let sql = if table == "bot_life_states" {
            "SELECT EXISTS(SELECT 1 FROM bot_life_states WHERE bot_id=?1)"
        } else {
            "SELECT EXISTS(SELECT 1 FROM bot_life_events WHERE bot_id=?1)"
        };
        match db.query_row(sql, [bot_id], |row| row.get::<_, bool>(0)) {
            Ok(false) => (),
            Ok(true) | Err(_) => return true,
        }
    }
    false
}

/// Profile packages do not carry the local simulation's state/history/runtime.
/// Life configuration is deliberately separate; these names are payload-only.
pub(super) fn strip_profile_payload(profile: &mut serde_json::Value) {
    if let Some(fields) = profile.as_object_mut() {
        for key in ["lifeState", "lifeEvents", "lifeSnapshot", "lifeSessions"] {
            fields.remove(key);
        }
    }
}

#[cfg(test)]
#[path = "life_store/tests.rs"]
mod tests;
