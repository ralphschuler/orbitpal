//! Installation-only metadata. Migration and snapshots never touch native
//! credentials. Provider keys are read only after exact purpose/scope resolution.
use super::Store;
use crate::api_connections::{self as api, Binding, Purpose, MAX_COUNTER, MAX_ENTRIES};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
const TABLE: &str = "global_api_connections";
const MAX_BYTES: usize = 24 * 1024 * 1024;
const MAX_PROFILE: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Error {
    Database,
    UnsupportedSchema,
    CorruptRegistry,
    MigrationIncomplete,
    Capacity,
}
impl Error {
    fn reason(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::UnsupportedSchema => "unsupported_schema",
            Self::CorruptRegistry => "corrupt_registry",
            Self::MigrationIncomplete | Self::Capacity => "migration_incomplete",
        }
    }
}
type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Source {
    Legacy,
    Operator,
}
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    id: String,
    label: String,
    binding: Binding,
    secret_id: String,
    source: Source,
}
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DefaultEntry {
    id: String,
    binding: Binding,
    connection_id: Option<String>,
}
fn bounded<'de, D, T>(d: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct V<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for V<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded API metadata list")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = Vec::new();
            while let Some(value) = seq.next_element()? {
                if result.len() >= MAX_ENTRIES {
                    return Err(serde::de::Error::custom("API metadata capacity"));
                }
                result.push(value);
            }
            Ok(result)
        }
    }
    d.deserialize_seq(V(std::marker::PhantomData))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    #[serde(deserialize_with = "bounded")]
    connections: Vec<Entry>,
    #[serde(deserialize_with = "bounded")]
    defaults: Vec<DefaultEntry>,
    legacy_unbound_count: u64,
}
#[derive(Clone)]
struct Record {
    revision: u64,
    catalog: Catalog,
}
impl Catalog {
    fn new() -> Self {
        Self {
            connections: vec![],
            defaults: vec![],
            legacy_unbound_count: 0,
        }
    }
    fn validate(&self) -> Result<()> {
        if self.connections.len() > MAX_ENTRIES
            || self.defaults.len() > MAX_ENTRIES
            || self.legacy_unbound_count > MAX_COUNTER
        {
            return Err(Error::CorruptRegistry);
        }
        let mut ids = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for entry in &self.connections {
            if !api::valid_id(&entry.id)
                || !api::valid_id(&entry.secret_id)
                || !ids.insert(&entry.id)
                || api::label(&entry.label).as_ref() != Ok(&entry.label)
                || entry.binding.validate().is_err()
            {
                return Err(Error::CorruptRegistry);
            }
        }
        for entry in &self.defaults {
            if !api::valid_id(&entry.id)
                || !ids.insert(&entry.id)
                || !bindings.insert(&entry.binding)
                || entry.binding.validate().is_err()
                || entry.connection_id.as_ref().is_some_and(|id| {
                    !self
                        .connections
                        .iter()
                        .any(|c| &c.id == id && c.binding == entry.binding)
                })
            {
                return Err(Error::CorruptRegistry);
            }
        }
        Ok(())
    }
    fn alias(&mut self, binding: &Binding) -> Result<&mut DefaultEntry> {
        if !self.defaults.iter().any(|d| &d.binding == binding) {
            if self.defaults.len() >= MAX_ENTRIES {
                return Err(Error::Capacity);
            }
            self.defaults.push(DefaultEntry {
                id: format!("api-default-{}", uuid::Uuid::new_v4()),
                binding: binding.clone(),
                connection_id: None,
            });
        }
        Ok(self
            .defaults
            .iter_mut()
            .find(|d| &d.binding == binding)
            .unwrap())
    }
    fn adopt(&mut self, binding: Binding, secret_id: &str) -> Result<String> {
        if let Some(entry) = self
            .connections
            .iter()
            .find(|e| e.binding == binding && e.secret_id == secret_id)
        {
            return Ok(entry.id.clone());
        }
        if self.connections.len() >= MAX_ENTRIES {
            return Err(Error::Capacity);
        }
        let id = format!("api-{}", uuid::Uuid::new_v4());
        self.connections.push(Entry {
            id: id.clone(),
            label: format!("Übernommen · {}", self.connections.len() + 1),
            binding: binding.clone(),
            secret_id: secret_id.into(),
            source: Source::Legacy,
        });
        self.alias(&binding)?;
        Ok(id)
    }
    fn resolve(&self, reference: &str, binding: &Binding) -> Option<&Entry> {
        if let Some(default) = self.defaults.iter().find(|d| d.id == reference) {
            if &default.binding != binding {
                return None;
            }
            let target = default.connection_id.as_ref()?;
            return self
                .connections
                .iter()
                .find(|e| &e.id == target && &e.binding == binding);
        }
        self.connections
            .iter()
            .find(|e| e.id == reference && &e.binding == binding)
    }
}
fn db_error(_: rusqlite::Error) -> Error {
    Error::Database
}
fn schema(db: &Connection) -> Result<()> {
    let kind: Option<String> = db
        .query_row(
            "SELECT type FROM sqlite_master WHERE name=?1",
            [TABLE],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if kind.as_deref() != Some("table") {
        return Err(Error::UnsupportedSchema);
    }
    let mut s=db.prepare("SELECT CASE WHEN typeof(name)='text' AND length(CAST(name AS BLOB)) BETWEEN 1 AND 64 THEN name ELSE NULL END,CASE WHEN typeof(type)='text' AND length(CAST(type AS BLOB)) BETWEEN 1 AND 16 THEN upper(type) ELSE NULL END,pk FROM pragma_table_info('global_api_connections') LIMIT 5").map_err(db_error)?;
    let cols = s
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| Error::UnsupportedSchema)?;
    let expected = [
        ("singleton", "INTEGER", 1),
        ("schema_version", "INTEGER", 0),
        ("revision", "INTEGER", 0),
        ("metadata", "TEXT", 0),
    ];
    if cols.len() != expected.len()
        || cols
            .iter()
            .zip(expected)
            .any(|((n, t, p), (en, et, ep))| n != en || t != et || *p != ep)
    {
        return Err(Error::UnsupportedSchema);
    }
    Ok(())
}
fn load(db: &Connection) -> Result<Record> {
    schema(db)?;
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM (SELECT 1 FROM global_api_connections LIMIT 2)",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if count != 1 {
        return Err(Error::CorruptRegistry);
    }
    let row=db.query_row("SELECT singleton,typeof(singleton),schema_version,typeof(schema_version),revision,typeof(revision),CASE WHEN typeof(metadata)='text' AND length(CAST(metadata AS BLOB)) BETWEEN 1 AND ?1 THEN metadata ELSE NULL END FROM global_api_connections",[MAX_BYTES as i64],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?))).map_err(|_|Error::CorruptRegistry)?;
    if row.0 != 1
        || row.1 != "integer"
        || row.3 != "integer"
        || row.5 != "integer"
        || row.4 < 0
        || row.4 as u64 > MAX_COUNTER
    {
        return Err(Error::CorruptRegistry);
    }
    if row.2 != 1 {
        return Err(Error::UnsupportedSchema);
    }
    let catalog: Catalog = serde_json::from_str(row.6.as_deref().ok_or(Error::CorruptRegistry)?)
        .map_err(|_| Error::CorruptRegistry)?;
    catalog.validate()?;
    Ok(Record {
        revision: row.4 as u64,
        catalog,
    })
}
fn encode(catalog: &Catalog) -> Result<String> {
    catalog.validate()?;
    let text = serde_json::to_string(catalog).map_err(|_| Error::CorruptRegistry)?;
    if text.len() > MAX_BYTES {
        return Err(Error::CorruptRegistry);
    }
    Ok(text)
}
fn save(db: &Connection, expected: u64, catalog: &Catalog) -> std::result::Result<(), String> {
    if expected >= MAX_COUNTER {
        return Err(api::UNAVAILABLE.into());
    }
    let text = encode(catalog).map_err(|_| api::UNAVAILABLE)?;
    let n=db.execute("UPDATE global_api_connections SET revision=?1,metadata=?2 WHERE singleton=1 AND revision=?3",params![(expected+1)as i64,text,expected as i64]).map_err(|_|api::UNAVAILABLE)?;
    if n != 1 {
        return Err(api::STALE.into());
    }
    Ok(())
}
fn registered(db: &Connection) -> Result<BTreeSet<String>> {
    let mut s=db.prepare("SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND 96 THEN CAST(id AS BLOB) ELSE NULL END FROM secret_ids LIMIT 16385").map_err(db_error)?;
    let values = s
        .query_map([], |r| r.get::<_, Option<Vec<u8>>>(0))
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    if values.len() > 16384 {
        return Err(Error::CorruptRegistry);
    }
    Ok(values
        .into_iter()
        .flatten()
        .filter_map(|id| String::from_utf8(id).ok())
        .filter(|id| api::valid_id(id))
        .collect())
}
fn sections() -> [(&'static str, Purpose); 5] {
    [
        ("/model", Purpose::Dialog),
        ("/voice", Purpose::Speech),
        ("/stt", Purpose::Transcription),
        ("/hindsight", Purpose::Hindsight),
        ("/chat/youtube", Purpose::YoutubeData),
    ]
}
fn profile_migration(
    profile: &Value,
    catalog: &Catalog,
    ids: &BTreeSet<String>,
) -> Result<(Value, Catalog)> {
    let mut profile = profile.clone();
    let mut next = catalog.clone();
    for (path, purpose) in sections() {
        let Some(config) = profile.pointer_mut(path) else {
            continue;
        };
        if !config.is_object() {
            return Err(Error::MigrationIncomplete);
        }
        let binding = Binding::request(config, purpose).map_err(|_| Error::MigrationIncomplete)?;
        let id = match config.get("apiKeyId") {
            None => "",
            Some(v) => v.as_str().ok_or(Error::MigrationIncomplete)?,
        }
        .to_owned();
        if !id.is_empty() && !api::valid_id(&id) {
            return Err(Error::MigrationIncomplete);
        }
        let reference = match binding {
            None => String::new(),
            Some(binding) if !id.is_empty() && ids.contains(&id) => next.adopt(binding, &id)?,
            Some(_) => {
                if !id.is_empty() {
                    next.legacy_unbound_count =
                        next.legacy_unbound_count.saturating_add(1).min(MAX_COUNTER);
                }
                String::new()
            }
        };
        config["apiKeyId"] = json!(reference);
    }
    Ok((profile, next))
}
fn migrate(db: &mut Connection) -> Result<()> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT type FROM sqlite_master WHERE name=?1",
            [TABLE],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if existing.is_some() {
        load(&tx)?;
        tx.commit().map_err(db_error)?;
        return Ok(());
    }
    tx.execute_batch("CREATE TABLE global_api_connections(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,revision INTEGER NOT NULL,metadata TEXT NOT NULL CHECK(length(CAST(metadata AS BLOB))<=25165824));").map_err(db_error)?;
    schema(&tx)?;
    let ids = registered(&tx)?;
    let profiles = {
        let mut s=tx.prepare("SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND 96 THEN CAST(id AS BLOB) ELSE NULL END,CASE WHEN typeof(profile)='text' AND length(CAST(profile AS BLOB)) BETWEEN 1 AND ?1 THEN CAST(profile AS BLOB) ELSE NULL END FROM bot_profiles ORDER BY rowid LIMIT 1025").map_err(db_error)?;
        let rows = s
            .query_map([MAX_PROFILE as i64], |r| {
                Ok((
                    r.get::<_, Option<Vec<u8>>>(0)?,
                    r.get::<_, Option<Vec<u8>>>(1)?,
                ))
            })
            .map_err(db_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error)?;
        rows
    };
    if profiles.len() > 1024 {
        return Err(Error::MigrationIncomplete);
    }
    let mut catalog = Catalog::new();
    let mut updated = BTreeMap::new();
    for (id, text) in profiles {
        let id = id.and_then(|value| String::from_utf8(value).ok());
        let text = text.and_then(|value| String::from_utf8(value).ok());
        let result = id
            .as_ref()
            .filter(|id| api::valid_id(id))
            .zip(text.as_ref())
            .and_then(|(id, t)| {
                serde_json::from_str::<Value>(t)
                    .ok()
                    .filter(|p| p["id"].as_str() == Some(id))
            });
        let result = match result {
            Some(profile) => match profile_migration(&profile, &catalog, &ids) {
                Ok(value) => Some(value),
                Err(Error::Capacity) => return Err(Error::Capacity),
                Err(_) => None,
            },
            None => None,
        };
        if let Some((profile, next)) = result {
            catalog = next;
            let id = id.unwrap();
            let text = serde_json::to_string(&profile).map_err(|_| Error::MigrationIncomplete)?;
            tx.execute(
                "UPDATE bot_profiles SET profile=?1 WHERE id=?2",
                params![text, id],
            )
            .map_err(db_error)?;
            updated.insert(id, text);
        } else {
            catalog.legacy_unbound_count = catalog
                .legacy_unbound_count
                .saturating_add(1)
                .min(MAX_COUNTER);
        }
    }
    let bindings = catalog
        .defaults
        .iter()
        .map(|d| d.binding.clone())
        .collect::<Vec<_>>();
    for binding in bindings {
        let entries = catalog
            .connections
            .iter()
            .filter(|e| e.binding == binding)
            .map(|e| e.id.clone())
            .collect::<Vec<_>>();
        if entries.len() == 1 {
            catalog.alias(&binding)?.connection_id = Some(entries[0].clone());
        }
    }
    let active:Option<String>=tx.query_row("SELECT CASE WHEN typeof(value)='text' AND length(CAST(value AS BLOB)) BETWEEN 1 AND ?1 THEN value ELSE NULL END FROM settings WHERE key='profile'",[MAX_PROFILE as i64],|r|r.get::<_,Option<String>>(0)).optional().map_err(db_error)?.flatten();
    if let Some(id) = active
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v["id"].as_str().map(str::to_owned))
    {
        if let Some(text) = updated.get(&id) {
            tx.execute("UPDATE settings SET value=?1 WHERE key='profile'", [text])
                .map_err(db_error)?;
        }
    }
    tx.execute(
        "INSERT INTO global_api_connections VALUES(1,1,0,?1)",
        [encode(&catalog)?],
    )
    .map_err(db_error)?;
    tx.commit().map_err(db_error)
}
fn refs(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "apiKeyId" | "secretId" | "sendSecretId") {
                    if let Some(id) = value.as_str().filter(|id| api::valid_id(id)) {
                        ids.insert(id.into());
                    }
                } else {
                    refs(value, ids);
                }
            }
        }
        Value::Array(v) => {
            for value in v {
                refs(value, ids);
            }
        }
        Value::String(s) => {
            if let Some(id) = s
                .strip_prefix("${secret:")
                .and_then(|s| s.strip_suffix('}'))
                .filter(|id| api::valid_id(id))
            {
                ids.insert(id.into());
            }
        }
        _ => (),
    }
}
fn external(db: &Connection, catalog: &Catalog) -> (BTreeSet<String>, bool) {
    let mut ids = BTreeSet::new();
    let mut uncertain = false;
    for (table, column, limit) in [
        ("bot_profiles", "profile", 1024),
        ("mcp_library", "server", 4096),
    ] {
        let sql=format!("SELECT CASE WHEN typeof({column})='text' AND length(CAST({column} AS BLOB)) BETWEEN 1 AND {MAX_PROFILE} THEN {column} ELSE NULL END FROM {table} LIMIT {}",limit+1);
        let rows = (|| {
            let mut s = db.prepare(&sql)?;
            let values = s
                .query_map([], |r| r.get::<_, Option<String>>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok::<_, rusqlite::Error>(values)
        })();
        match rows {
            Ok(rows) => {
                if rows.len() > limit {
                    uncertain = true;
                }
                for row in rows.into_iter().take(limit) {
                    if let Some(mut value) =
                        row.and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    {
                        if table == "bot_profiles" {
                            for (path, _) in sections() {
                                if let Some(config) =
                                    value.pointer_mut(path).and_then(Value::as_object_mut)
                                {
                                    if config.get("apiKeyId").and_then(Value::as_str).is_some_and(
                                        |id| {
                                            id.is_empty()
                                                || catalog.connections.iter().any(|e| e.id == id)
                                                || catalog.defaults.iter().any(|d| d.id == id)
                                        },
                                    ) {
                                        config.remove("apiKeyId");
                                    }
                                }
                            }
                        }
                        refs(&value, &mut ids);
                    } else {
                        uncertain = true;
                    }
                }
            }
            Err(_) => uncertain = true,
        }
    }
    (ids, uncertain)
}
fn policies(db: &Connection, catalog: &Catalog) -> BTreeMap<String, (usize, bool)> {
    let (ids, uncertain) = external(db, catalog);
    catalog
        .connections
        .iter()
        .map(|entry| {
            let count = catalog
                .connections
                .iter()
                .filter(|e| e.secret_id == entry.secret_id)
                .count();
            (
                entry.id.clone(),
                (
                    count,
                    ids.contains(&entry.secret_id) || (uncertain && entry.source == Source::Legacy),
                ),
            )
        })
        .collect()
}
fn public(db: &Connection, record: &Record) -> Result<Value> {
    let ids = registered(db)?;
    let policies = policies(db, &record.catalog);
    let connections=record.catalog.connections.iter().map(|e|{let(count,external)=policies[&e.id];json!({"id":e.id,"label":e.label,"service":e.binding.service,"destination":e.binding.destination,"authStyle":e.binding.auth_style,"keyStatus":if ids.contains(&e.secret_id){"registered"}else{"missing"},"source":e.source,"sharedConnectionCount":count,"externalReferences":external,"writePolicy":if count>1||external{"shared_legacy_read_only"}else{"replaceable"}})}).collect::<Vec<_>>();
    Ok(
        json!({"version":1,"availability":"ready","reason":null,"revision":record.revision,"connections":connections,"defaults":record.catalog.defaults.iter().map(|d|json!({"id":d.id,"service":d.binding.service,"destination":d.binding.destination,"authStyle":d.binding.auth_style,"connectionId":d.connection_id})).collect::<Vec<_>>(),"legacyUnboundCount":record.catalog.legacy_unbound_count}),
    )
}
fn failed(error: Error) -> Value {
    json!({"version":1,"availability":"unavailable","reason":error.reason(),"revision":null,"connections":[],"defaults":[],"legacyUnboundCount":0})
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NewConnection {
    pub label: String,
    pub service: api::Service,
    pub destination: String,
    pub auth_style: api::AuthStyle,
    pub existing_secret_id: Option<String>,
}
impl Store {
    pub(crate) fn migrate_api_connections(&self) -> Result<()> {
        let mut db = self.db.lock().map_err(|_| Error::Database)?;
        migrate(&mut db)
    }
    fn api_scope(&self) -> std::result::Result<(), String> {
        if self.fixed_profile.is_some() {
            Err(
                "Globale API-Zugänge werden ausschließlich für diese Installation verwaltet."
                    .into(),
            )
        } else {
            Ok(())
        }
    }
    fn api_record(&self, db: &Connection) -> Result<Record> {
        if let Some(error) = self.api_error {
            return Err(error);
        }
        load(db)
    }
    pub(crate) fn api_snapshot(&self) -> Value {
        match self.db.lock() {
            Ok(db) => match self.api_record(&db).and_then(|r| public(&db, &r)) {
                Ok(value) => value,
                Err(error) => failed(error),
            },
            Err(_) => failed(Error::Database),
        }
    }
    fn edit_api(
        &self,
        expected: u64,
        change: impl FnOnce(&mut Catalog, &Connection) -> std::result::Result<(), String>,
    ) -> std::result::Result<Value, String> {
        self.api_scope()?;
        let _management = self.api_management.lock().map_err(|_| api::UNAVAILABLE)?;
        let mut db = self.db.lock().map_err(|_| api::UNAVAILABLE)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| api::UNAVAILABLE)?;
        let mut current = self.api_record(&tx).map_err(|_| api::UNAVAILABLE)?;
        if current.revision != expected {
            return Err(api::STALE.into());
        }
        change(&mut current.catalog, &tx)?;
        save(&tx, expected, &current.catalog)?;
        tx.commit().map_err(|_| api::UNAVAILABLE)?;
        drop(db);
        Ok(self.api_snapshot())
    }
    pub(crate) fn create_api_connection(
        &self,
        expected: u64,
        connection: NewConnection,
    ) -> std::result::Result<Value, String> {
        let binding = Binding::normalized(
            connection.service,
            &connection.destination,
            connection.auth_style,
        )?;
        let label = api::label(&connection.label)?;
        let id = format!("api-{}", uuid::Uuid::new_v4());
        let result = self.edit_api(expected, |catalog, db| {
            if catalog.connections.len() >= MAX_ENTRIES {
                return Err(api::UNAVAILABLE.into());
            }
            let secret_id = match connection.existing_secret_id {
                Some(ref id)
                    if api::valid_id(id)
                        && registered(db).map_err(|_| api::UNAVAILABLE)?.contains(id) =>
                {
                    id.clone()
                }
                Some(_) => {
                    return Err(
                        "Der vorhandene Schlüsselbund-Verweis ist nicht registriert.".into(),
                    )
                }
                None => format!("api-secret-{}", uuid::Uuid::new_v4()),
            };
            catalog.alias(&binding).map_err(|_| api::UNAVAILABLE)?;
            catalog.connections.push(Entry {
                id: id.clone(),
                label,
                binding,
                secret_id,
                source: if connection.existing_secret_id.is_some() {
                    Source::Legacy
                } else {
                    Source::Operator
                },
            });
            Ok(())
        })?;
        Ok(json!({"snapshot":result,"connectionId":id}))
    }
    pub(crate) fn rename_api_connection(
        &self,
        expected: u64,
        id: &str,
        label: &str,
    ) -> std::result::Result<Value, String> {
        let label = api::label(label)?;
        self.edit_api(expected, |c, _| {
            let e = c
                .connections
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or(api::INVALID)?;
            e.label = label;
            Ok(())
        })
    }
    pub(crate) fn default_api_connection(
        &self,
        expected: u64,
        binding: Binding,
        id: Option<&str>,
    ) -> std::result::Result<Value, String> {
        binding.validate()?;
        self.edit_api(expected, |c, _| {
            if id.is_some_and(|id| {
                !c.connections
                    .iter()
                    .any(|e| e.id == id && e.binding == binding)
            }) {
                return Err(api::INVALID.into());
            }
            let default = c
                .defaults
                .iter_mut()
                .find(|d| d.binding == binding)
                .ok_or(api::INVALID)?;
            default.connection_id = id.map(str::to_owned);
            Ok(())
        })
    }
    pub(crate) fn remove_api_connection(
        &self,
        expected: u64,
        id: &str,
    ) -> std::result::Result<Value, String> {
        self.edit_api(expected, |c, _| {
            if !c.connections.iter().any(|e| e.id == id) {
                return Err(api::INVALID.into());
            }
            c.connections.retain(|e| e.id != id);
            for d in &mut c.defaults {
                if d.connection_id.as_deref() == Some(id) {
                    d.connection_id = None;
                }
            }
            Ok(())
        })
    }
    pub(crate) fn set_api_connection_secret(
        &self,
        expected: u64,
        id: &str,
        value: &str,
    ) -> std::result::Result<Value, String> {
        self.api_scope()?;
        if value.is_empty() || value.len() > 65536 {
            return Err("Ein API-Schlüssel muss zwischen 1 und 65536 Bytes enthalten.".into());
        }
        let _management = self.api_management.lock().map_err(|_| api::UNAVAILABLE)?;
        let target = {
            let db = self.db.lock().map_err(|_| api::UNAVAILABLE)?;
            let record = self.api_record(&db).map_err(|_| api::UNAVAILABLE)?;
            if record.revision != expected {
                return Err(api::STALE.into());
            }
            let entry = record
                .catalog
                .connections
                .iter()
                .find(|e| e.id == id)
                .ok_or(api::INVALID)?;
            let (count, external) = policies(&db, &record.catalog)[id];
            if count > 1 || external {
                return Err(api::SHARED.into());
            }
            entry.secret_id.clone()
        };
        // No SQLite or catalog read lock is held across the OS authorization dialog.
        self.set_secret(&target, value).map_err(|_| {
            "Der API-Schlüssel konnte nicht im sicheren Systemspeicher gespeichert werden."
        })?;
        let mut db = self.db.lock().map_err(|_| api::UNAVAILABLE)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| api::UNAVAILABLE)?;
        let current = self.api_record(&tx).map_err(|_| api::UNAVAILABLE)?;
        if current.revision != expected
            || !current
                .catalog
                .connections
                .iter()
                .any(|e| e.id == id && e.secret_id == target)
        {
            return Err(api::STALE.into());
        }
        save(&tx, expected, &current.catalog)?;
        tx.commit().map_err(|_| api::UNAVAILABLE)?;
        drop(db);
        Ok(self.api_snapshot())
    }
    pub(crate) fn api_binding(&self, config: &Value, purpose: Purpose) -> Value {
        let reference = config["apiKeyId"]
            .as_str()
            .filter(|id| api::valid_id(id))
            .map(str::to_owned);
        let binding = Binding::request(config, purpose);
        let unused = matches!(binding, Ok(None));
        let mut result = json!({"binding":null,"unused":unused,"selectedRef":reference,"defaultRef":null,"defaultConnectionId":null,"compatibleIds":[],"status":if unused{"unused"}else{"unbound"},"reason":null});
        let binding = match binding {
            Ok(Some(b)) => b,
            Ok(None) => return result,
            Err(_) => {
                result["status"] = json!("invalid");
                result["reason"] = json!("invalid_destination");
                return result;
            }
        };
        result["binding"] = json!(binding);
        let load = (|| {
            let db = self.db.lock().map_err(|_| Error::Database)?;
            let record = self.api_record(&db)?;
            let ids = registered(&db)?;
            Ok::<_, Error>((record, ids))
        })();
        let (record, ids) = match load {
            Ok(v) => v,
            Err(_) => {
                if reference.is_some() {
                    result["status"] = json!("invalid");
                }
                result["reason"] = json!("registry_unavailable");
                return result;
            }
        };
        let c = &record.catalog;
        result["compatibleIds"] = json!(c
            .connections
            .iter()
            .filter(|e| e.binding == binding)
            .map(|e| &e.id)
            .collect::<Vec<_>>());
        if let Some(d) = c.defaults.iter().find(|d| d.binding == binding) {
            result["defaultRef"] = json!(d.id);
            result["defaultConnectionId"] = json!(d.connection_id);
        }
        let raw = match config.get("apiKeyId") {
            None => "",
            Some(Value::String(value)) => value.as_str(),
            _ => {
                result["status"] = json!("invalid");
                result["reason"] = json!("invalid_reference");
                return result;
            }
        };
        if raw.is_empty() {
            return result;
        }
        if !api::valid_id(raw) {
            result["status"] = json!("invalid");
            result["reason"] = json!("invalid_reference");
            return result;
        }
        let exists =
            c.connections.iter().any(|e| e.id == raw) || c.defaults.iter().any(|d| d.id == raw);
        let same_alias = c
            .defaults
            .iter()
            .find(|d| d.id == raw && d.binding == binding);
        if same_alias.is_some() {
            result["status"] = json!("default");
        } else {
            result["status"] = json!("named");
        }
        if let Some(e) = c.resolve(raw, &binding) {
            if !ids.contains(&e.secret_id) {
                result["reason"] = json!("missing_secret");
            }
        } else if same_alias.is_some() {
            result["reason"] = json!("default_missing");
        } else {
            result["status"] = json!("invalid");
            result["reason"] = json!(if exists {
                "scope_mismatch"
            } else {
                "unknown_connection"
            });
        }
        result
    }
    pub(crate) fn api_key_for(
        &self,
        config: &Value,
        purpose: Purpose,
    ) -> std::result::Result<Option<String>, String> {
        self.api_key_with(config, purpose, |id| self.secret(id))
    }
    pub(crate) fn api_key_with(
        &self,
        config: &Value,
        purpose: Purpose,
        read: impl FnOnce(&str) -> std::result::Result<Option<String>, String>,
    ) -> std::result::Result<Option<String>, String> {
        let Some(binding) = Binding::request(config, purpose)? else {
            return Ok(None);
        };
        let reference = match config.get("apiKeyId") {
            None => "",
            Some(Value::String(value)) => value.as_str(),
            _ => return Err(api::INVALID.into()),
        };
        if reference.is_empty() {
            return Ok(None);
        }
        if !api::valid_id(reference) {
            return Err(api::INVALID.into());
        }
        let (revision, entry) = {
            let db = self.db.lock().map_err(|_| api::UNAVAILABLE)?;
            let record = self.api_record(&db).map_err(|_| api::UNAVAILABLE)?;
            let e = record
                .catalog
                .resolve(reference, &binding)
                .ok_or(api::INVALID)?;
            if !registered(&db)
                .map_err(|_| api::UNAVAILABLE)?
                .contains(&e.secret_id)
            {
                return Err(
                    "Der globale API-Zugang enthält keinen registrierten Schlüssel.".into(),
                );
            }
            (record.revision, e.clone())
        };
        let mut key = read(&entry.secret_id)
            .map_err(|_| {
                "Der API-Schlüssel konnte nicht aus dem sicheren Systemspeicher gelesen werden."
            })?
            .ok_or("Der globale API-Zugang enthält keinen verfügbaren Schlüssel.")?;
        let valid = (|| {
            let db = self.db.lock().ok()?;
            let record = self.api_record(&db).ok()?;
            Some(
                record.revision == revision
                    && record.catalog.resolve(reference, &binding) == Some(&entry)
                    && registered(&db).ok()?.contains(&entry.secret_id),
            )
        })()
        .unwrap_or(false);
        if !valid {
            use zeroize::Zeroize;
            key.zeroize();
            return Err(api::INVALID.into());
        }
        Ok(Some(key))
    }
    pub(crate) fn assign_api_defaults(&self, profile: &mut Value, keep_existing: bool) {
        // Copy is not a new authentication choice. Preserve blank and unavailable
        // references as well as valid ones; only an explicit new bot gets defaults.
        if keep_existing {
            return;
        }
        let db = match self.db.lock() {
            Ok(db) => db,
            Err(_) => return,
        };
        let record = match self.api_record(&db) {
            Ok(v) => v,
            Err(_) => return,
        };
        for (path, purpose) in sections() {
            let Some(config) = profile.pointer_mut(path) else {
                continue;
            };
            if !config.is_object() {
                continue;
            }
            let binding = match Binding::request(config, purpose) {
                Ok(Some(b)) => b,
                _ => {
                    config["apiKeyId"] = json!("");
                    continue;
                }
            };
            let default = record
                .catalog
                .defaults
                .iter()
                .find(|d| d.binding == binding && d.connection_id.is_some());
            config["apiKeyId"] = json!(default.map(|d| d.id.as_str()).unwrap_or_default());
        }
    }
}
pub(crate) fn strip_exported_api_refs(profile: &mut Value) {
    for (path, _) in sections() {
        if let Some(section) = profile.pointer_mut(path).and_then(Value::as_object_mut) {
            section.insert("apiKeyId".into(), json!(""));
        }
    }
}
#[cfg(test)]
#[path = "api_connection_store/tests.rs"]
mod tests;
