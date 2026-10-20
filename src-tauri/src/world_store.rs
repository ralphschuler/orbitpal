//! Installation-only optional stage configuration. Actor Life lineages, capture
//! grants, profile packages and runtime capabilities never enter this table.
use super::Store;
use crate::world_state::{WorldConfig, SCHEMA_VERSION};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

pub const MAX_CONFIG_BYTES: usize = 262_144;
const MAX_BINDING_BYTES: usize = 4096;
const TABLE: &str = "shared_world_config";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStoreError {
    Database,
    UnsupportedSchema,
    CorruptConfig,
    UnknownActor,
    InvalidScope,
    StaleRevision,
    WrongStage,
    InvalidBinding,
    Capacity,
}
impl WorldStoreError {
    pub fn message(self) -> &'static str {
        match self{
        Self::Database=>"Die gemeinsame Bühne ist vorübergehend nicht verfügbar.",
        Self::UnsupportedSchema=>"Diese Version der Bühnenkonfiguration wird nicht unterstützt.",
        Self::CorruptConfig=>"Die gespeicherte Bühnenkonfiguration ist ungültig. Andere Bot-Daten bleiben verfügbar.",
        Self::UnknownActor=>"Ein ausgewählter Bot ist nicht mehr verfügbar.",
        Self::InvalidScope=>"Die gemeinsame Bühne kann nur für diese Installation verwaltet werden.",
        Self::StaleRevision=>"Die Bühne wurde inzwischen geändert. Bitte neu laden.",
        Self::WrongStage=>"Eine vorhandene Bühne kann nicht durch eine andere Bühnen-ID ersetzt werden.",
        Self::InvalidBinding=>"Die Monitorauswahl der Bühne ist ungültig.",
        Self::Capacity=>"Die Bühnenkonfiguration überschreitet die zulässige Größe.",
    }
    }
}
type Result<T> = std::result::Result<T, WorldStoreError>;
#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputBinding {
    pub monitor_id: Option<String>,
}
impl OutputBinding {
    pub fn validate(&self) -> Result<()> {
        if self
            .monitor_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 512 || id.chars().any(char::is_control))
        {
            Err(WorldStoreError::InvalidBinding)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldRecord {
    pub config: WorldConfig,
    pub output_binding: OutputBinding,
}
fn database(_: rusqlite::Error) -> WorldStoreError {
    WorldStoreError::Database
}
fn table_exists(db: &Connection) -> Result<bool> {
    let kind = db
        .query_row(
            "SELECT type FROM sqlite_master WHERE name=?1",
            [TABLE],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(database)?;
    match kind.as_deref() {
        None => Ok(false),
        Some("table") => Ok(true),
        _ => Err(WorldStoreError::UnsupportedSchema),
    }
}
fn schema(db: &Connection) -> Result<()> {
    let mut statement=db.prepare("SELECT CASE WHEN typeof(name)='text' AND length(CAST(name AS BLOB)) BETWEEN 1 AND 64 THEN name ELSE NULL END,CASE WHEN typeof(type)='text' AND length(CAST(type AS BLOB)) BETWEEN 1 AND 16 THEN upper(type) ELSE NULL END,pk FROM pragma_table_info('shared_world_config') LIMIT 6").map_err(database)?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(database)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(database)?;
    let expected = [
        ("singleton", "INTEGER", 1),
        ("schema_version", "INTEGER", 0),
        ("revision", "INTEGER", 0),
        ("config_json", "TEXT", 0),
        ("output_json", "TEXT", 0),
    ];
    if columns.len() != expected.len()
        || columns
            .iter()
            .zip(expected)
            .any(|((name, kind, pk), (n, k, p))| {
                name != n || kind.to_ascii_uppercase() != k || *pk != p
            })
    {
        return Err(WorldStoreError::UnsupportedSchema);
    }
    Ok(())
}
pub(super) fn initialize(db: &mut Connection) -> Result<()> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database)?;
    if !table_exists(&tx)? {
        tx.execute_batch("CREATE TABLE shared_world_config(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,revision INTEGER NOT NULL,config_json TEXT NOT NULL CHECK(length(CAST(config_json AS BLOB))<=262144),output_json TEXT NOT NULL CHECK(length(CAST(output_json AS BLOB))<=4096));").map_err(database)?;
    }
    schema(&tx)?;
    load(&tx, false)?;
    tx.commit().map_err(database)
}
fn members_exist(db: &Connection, config: &WorldConfig) -> Result<()> {
    for actor in config.members() {
        let exists = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",
                [actor.as_str()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(database)?;
        if !exists {
            return Err(WorldStoreError::UnknownActor);
        }
    }
    Ok(())
}
pub(super) fn load(db: &Connection, check_members: bool) -> Result<Option<WorldRecord>> {
    schema(db)?;
    let count = db
        .query_row(
            "SELECT count(*) FROM (SELECT 1 FROM shared_world_config LIMIT 2)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(database)?;
    if count == 0 {
        return Ok(None);
    }
    if count != 1 {
        return Err(WorldStoreError::CorruptConfig);
    }
    // SQLite bounds and type-checks payloads BEFORE returning them to Rust.
    let row=db.query_row("SELECT singleton,typeof(singleton),schema_version,typeof(schema_version),revision,typeof(revision),CASE WHEN typeof(config_json)='text' AND length(CAST(config_json AS BLOB)) BETWEEN 1 AND 262144 THEN config_json ELSE NULL END,CASE WHEN typeof(output_json)='text' AND length(CAST(output_json AS BLOB)) BETWEEN 1 AND 4096 THEN output_json ELSE NULL END FROM shared_world_config",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,String>(3)?,row.get::<_,i64>(4)?,row.get::<_,String>(5)?,row.get::<_,Option<String>>(6)?,row.get::<_,Option<String>>(7)?))).map_err(|_|WorldStoreError::CorruptConfig)?;
    if row.0 != 1
        || row.1 != "integer"
        || row.3 != "integer"
        || row.5 != "integer"
        || !(1..=crate::life_domain::MAX_COUNTER as i64).contains(&row.4)
    {
        return Err(WorldStoreError::CorruptConfig);
    }
    if row.2 != i64::from(SCHEMA_VERSION) {
        return Err(WorldStoreError::UnsupportedSchema);
    }
    let config: WorldConfig =
        serde_json::from_str(row.6.as_deref().ok_or(WorldStoreError::CorruptConfig)?)
            .map_err(|_| WorldStoreError::CorruptConfig)?;
    let output_binding: OutputBinding =
        serde_json::from_str(row.7.as_deref().ok_or(WorldStoreError::CorruptConfig)?)
            .map_err(|_| WorldStoreError::CorruptConfig)?;
    config
        .validate()
        .map_err(|_| WorldStoreError::CorruptConfig)?;
    output_binding
        .validate()
        .map_err(|_| WorldStoreError::CorruptConfig)?;
    if config.revision() != row.4 as u64 {
        return Err(WorldStoreError::CorruptConfig);
    }
    if check_members {
        members_exist(db, &config)?;
    }
    Ok(Some(WorldRecord {
        config,
        output_binding,
    }))
}
/// Profile packages cannot acquire an installation's orphaned membership.
/// An unknown/corrupt optional layout is conservatively treated as reserved;
/// importing can still proceed under a newly minted bot ID without resetting it.
pub(super) fn profile_id_reserved(db: &Connection, id: &str) -> bool {
    match table_exists(db) {
        Ok(false) => false,
        Ok(true) => match load(db, false) {
            Ok(Some(record)) => record
                .config
                .members()
                .iter()
                .any(|actor| actor.as_str() == id),
            Ok(None) => false,
            Err(_) => true,
        },
        Err(_) => true,
    }
}
impl Store {
    fn world_ready(&self) -> Result<()> {
        if self.fixed_profile.is_some() {
            return Err(WorldStoreError::InvalidScope);
        }
        self.world_error.map_or(Ok(()), Err)
    }
    pub(crate) fn load_world_config(&self) -> Result<Option<WorldRecord>> {
        self.world_ready()?;
        load(
            &*self.db.lock().map_err(|_| WorldStoreError::Database)?,
            true,
        )
    }
    pub(crate) fn world_settings_record(&self) -> Result<Option<WorldRecord>> {
        self.world_ready()?;
        load(
            &*self.db.lock().map_err(|_| WorldStoreError::Database)?,
            false,
        )
    }
    pub(crate) fn save_world_config(
        &self,
        expected: Option<u64>,
        draft: &WorldConfig,
        binding: &OutputBinding,
    ) -> Result<WorldRecord> {
        self.world_ready()?;
        draft
            .validate()
            .map_err(|_| WorldStoreError::CorruptConfig)?;
        binding.validate()?;
        if expected.is_some_and(|revision| revision >= crate::life_domain::MAX_COUNTER)
            || draft.revision() != expected.unwrap_or(0)
        {
            return Err(WorldStoreError::StaleRevision);
        }
        let mut db = self.db.lock().map_err(|_| WorldStoreError::Database)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        let current = load(&tx, false)?;
        match (&current, expected) {
            (None, None) => (),
            (Some(current), Some(revision)) if current.config.revision() == revision => {
                if current.config.stage_id() != draft.stage_id() {
                    return Err(WorldStoreError::WrongStage);
                }
            }
            _ => return Err(WorldStoreError::StaleRevision),
        }
        members_exist(&tx, draft)?;
        let config = draft
            .with_revision(
                expected
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(WorldStoreError::StaleRevision)?,
            )
            .map_err(|_| WorldStoreError::CorruptConfig)?;
        let encoded = serde_json::to_string(&config).map_err(|_| WorldStoreError::CorruptConfig)?;
        let output = serde_json::to_string(binding).map_err(|_| WorldStoreError::CorruptConfig)?;
        if encoded.len() > MAX_CONFIG_BYTES || output.len() > MAX_BINDING_BYTES {
            return Err(WorldStoreError::Capacity);
        }
        if current.is_some() {
            let changed=tx.execute("UPDATE shared_world_config SET schema_version=?1,revision=?2,config_json=?3,output_json=?4 WHERE singleton=1 AND revision=?5",params![SCHEMA_VERSION,config.revision() as i64,encoded,output,expected.map(|value|value as i64)]).map_err(database)?;
            if changed != 1 {
                return Err(WorldStoreError::StaleRevision);
            }
        } else {
            tx.execute("INSERT INTO shared_world_config(singleton,schema_version,revision,config_json,output_json) VALUES(1,?1,?2,?3,?4)",params![SCHEMA_VERSION,config.revision() as i64,encoded,output]).map_err(database)?;
        }
        tx.commit().map_err(database)?;
        Ok(WorldRecord {
            config,
            output_binding: binding.clone(),
        })
    }
}
#[cfg(test)]
#[path = "world_store/tests.rs"]
mod tests;
