//! Installation-owned autonomous dispatch accounting. No provider, credential,
//! render, session-start or filesystem access belongs to this module.
use rusqlite::{params, Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const VERSION: i64 = 1;
const MAX_COUNTER: u64 = 9_007_199_254_740_991;
const MAX_ROWS: usize = 50_000;
const MAX_BOTS: usize = 1024;
const MAX_DAY: i64 = 2_932_896; // 9999-12-31; public dates retain YYYY-MM-DD.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Denial {
    DialogLimit,
    TokenLimit,
    TokenUnsupported,
    RequestAccountingUnsupported,
    RequestHistoryUnbounded,
    TokenHistoryUnbounded,
    TtsLimit,
    HindsightLimit,
    ImageCountLimit,
    ImageBytesLimit,
    Capacity,
    AccountingUnavailable,
}
impl Denial {
    pub fn message(self) -> &'static str {
        match self {
        Self::DialogLimit => "Autonomes Tageslimit für Dialoganfragen erreicht.",
        Self::TokenLimit => "Autonomes Tageslimit für Tokens erreicht.",
        Self::TokenUnsupported => "Für diesen Anbieter oder dieses Modell kann OrbitPal kein hartes Tokenlimit garantieren. Verwende ein unterstütztes direktes Modell oder nur das Anfragelimit.",
        Self::RequestAccountingUnsupported => "Direkte Anfragezählung unterstützt keine autonomen CLI-Aufträge. Wähle eine direkte HTTP-Integration oder den Modus OrbitPal-Aufrufe.",
        Self::RequestHistoryUnbounded => "Frühere CLI-Aufträge dieses UTC-Tags haben unbekannte interne Anfragezahlen. Direkte Anfragezählung sperrt weitere autonome Dialoge bis zum nächsten UTC-Tag.",
        Self::TokenHistoryUnbounded => "Frühere Dialoganfragen haben keine verlässliche Tokenobergrenze. Das harte Tages-Tokenlimit sperrt weitere autonome Dialoge bis zum nächsten UTC-Tag.",
        Self::TtsLimit => "Autonomes Tageslimit für Sprachzeichen erreicht.",
        Self::HindsightLimit => "Autonomes Tageslimit für Hindsight-Anfragen erreicht.",
        Self::ImageCountLimit => "Autonomes Tageslimit für Bildanzahl erreicht.",
        Self::ImageBytesLimit => "Autonomes Tageslimit für Bildbytes erreicht.",
        Self::Capacity => "Zu viele Budgetvorgänge für diesen UTC-Tag.",
        Self::AccountingUnavailable => "Das autonome Verbrauchsbudget ist nicht verfügbar. Manuelle Anfragen und Stop bleiben möglich.",
    }
    }
    fn code(self) -> &'static str {
        match self {
            Self::DialogLimit => "dialog_limit",
            Self::TokenLimit => "token_limit",
            Self::TokenUnsupported => "token_unsupported",
            Self::RequestAccountingUnsupported => "request_accounting_unsupported",
            Self::RequestHistoryUnbounded => "request_history_unbounded",
            Self::TokenHistoryUnbounded => "token_history_unbounded",
            Self::TtsLimit => "tts_limit",
            Self::HindsightLimit => "hindsight_limit",
            Self::ImageCountLimit => "image_count_limit",
            Self::ImageBytesLimit => "image_bytes_limit",
            Self::Capacity => "capacity",
            Self::AccountingUnavailable => "accounting_unavailable",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        [
            Self::DialogLimit,
            Self::TokenLimit,
            Self::TokenUnsupported,
            Self::RequestAccountingUnsupported,
            Self::RequestHistoryUnbounded,
            Self::TokenHistoryUnbounded,
            Self::TtsLimit,
            Self::HindsightLimit,
            Self::ImageCountLimit,
            Self::ImageBytesLimit,
            Self::Capacity,
            Self::AccountingUnavailable,
        ]
        .into_iter()
        .find(|d| d.code() == value)
        .ok_or(Self::AccountingUnavailable)
    }
}
type Result<T> = std::result::Result<T, Denial>;
fn unavailable<T>(_: T) -> Denial {
    Denial::AccountingUnavailable
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestAccounting {
    #[default]
    Dispatch,
    Direct,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub daily_dialog_requests: u64,
    pub daily_tts_characters: u64,
    pub daily_hindsight_requests: u64,
    pub daily_image_count: u64,
    pub daily_image_bytes: u64,
    pub hard_token_limit: Option<u64>,
    #[serde(default)]
    pub request_accounting: RequestAccounting,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            daily_dialog_requests: 100,
            daily_tts_characters: 20_000,
            daily_hindsight_requests: 200,
            daily_image_count: 100,
            daily_image_bytes: 50 * 1024 * 1024,
            hard_token_limit: None,
            request_accounting: RequestAccounting::Dispatch,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if [
            self.daily_dialog_requests,
            self.daily_tts_characters,
            self.daily_hindsight_requests,
            self.daily_image_count,
        ]
        .into_iter()
        .any(|n| n > 1_000_000)
            || self.daily_image_bytes > 1024 * 1024 * 1024
            || self
                .hard_token_limit
                .is_some_and(|n| n == 0 || n > 1_000_000_000)
        {
            return Err(Denial::AccountingUnavailable);
        }
        Ok(())
    }
}

/// Constructed by native entry points. Never deserialized from model content,
/// profile fields or a renderer-supplied conversational source string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    Manual,
    Autopilot,
    AutomaticChat,
}
impl Origin {
    pub fn autonomous(self) -> bool {
        self != Self::Manual
    }
    fn code(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Autopilot => "autopilot",
            Self::AutomaticChat => "automatic_chat",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Context {
    pub bot_id: String,
    pub origin: Origin,
}
impl Context {
    pub fn new(bot_id: &str, origin: Origin) -> Self {
        Self {
            bot_id: bot_id.into(),
            origin,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Dialog,
    Tts,
    Recall,
    Retain,
}
impl Kind {
    fn code(self) -> &'static str {
        match self {
            Self::Dialog => "dialog",
            Self::Tts => "tts",
            Self::Recall => "recall",
            Self::Retain => "retain",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "dialog" => Ok(Self::Dialog),
            "tts" => Ok(Self::Tts),
            "recall" => Ok(Self::Recall),
            "retain" => Ok(Self::Retain),
            _ => Err(Denial::AccountingUnavailable),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cost {
    pub kind: Kind,
    pub characters: u64,
    pub images: u64,
    pub image_bytes: u64,
    pub token_upper: Option<u64>,
    pub direct_request: bool,
}
impl Cost {
    pub fn dialog(images: u64, image_bytes: u64, token_upper: Option<u64>) -> Self {
        Self {
            kind: Kind::Dialog,
            characters: 0,
            images,
            image_bytes,
            token_upper,
            direct_request: true,
        }
    }
    pub fn cli_dialog(images: u64, image_bytes: u64) -> Self {
        Self {
            direct_request: false,
            ..Self::dialog(images, image_bytes, None)
        }
    }
    pub fn tts(characters: u64) -> Self {
        Self {
            kind: Kind::Tts,
            characters,
            images: 0,
            image_bytes: 0,
            token_upper: None,
            direct_request: false,
        }
    }
    pub fn memory(retain: bool) -> Self {
        Self {
            kind: if retain { Kind::Retain } else { Kind::Recall },
            characters: 0,
            images: 0,
            image_bytes: 0,
            token_upper: None,
            direct_request: false,
        }
    }
    fn validate(self) -> Result<()> {
        if (self.kind == Kind::Tts && self.characters == 0)
            || (self.images == 0) != (self.image_bytes == 0)
            || self.characters > 20_000
            || self.images > 4
            || self.image_bytes > 12 * 1024 * 1024
            || self
                .token_upper
                .is_some_and(|n| n == 0 || n > 1_000_000_000)
            || (self.kind != Kind::Dialog
                && (self.images != 0 || self.image_bytes != 0 || self.token_upper.is_some()))
            || (self.kind != Kind::Tts && self.characters != 0)
            || (self.direct_request && self.kind != Kind::Dialog)
            || (!self.direct_request && self.token_upper.is_some())
        {
            return Err(Denial::AccountingUnavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Reservation {
    id: String,
    bot_id: String,
    day: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub dialog_requests: u64,
    pub tts_requests: u64,
    pub tts_characters: u64,
    pub hindsight_recall_requests: u64,
    pub hindsight_retain_requests: u64,
    pub image_count: u64,
    pub image_bytes: u64,
    /// Includes unknown and pending upper reservations. Do not add reserved_tokens.
    pub charged_tokens: u64,
    pub observed_tokens: u64,
    pub reserved_tokens: u64,
    pub unknown_token_requests: u64,
    pub pending_requests: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotTotal {
    pub bot_id: String,
    pub total: Totals,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub limits: Limits,
    pub day: String,
    pub clock_adjusted: bool,
    pub total: Totals,
    pub bots: Vec<BotTotal>,
    pub last_denial: Option<Denial>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn today() -> i64 {
    chrono::Utc::now()
        .timestamp()
        .div_euclid(86_400)
        .clamp(0, MAX_DAY)
}
fn check_day(day: i64) -> Result<()> {
    if !(0..=MAX_DAY).contains(&day) {
        Err(Denial::AccountingUnavailable)
    } else {
        Ok(())
    }
}
fn date(day: i64) -> Result<String> {
    chrono::DateTime::from_timestamp(
        day.checked_mul(86_400)
            .ok_or(Denial::AccountingUnavailable)?,
        0,
    )
    .map(|d| d.format("%Y-%m-%d").to_string())
    .ok_or(Denial::AccountingUnavailable)
}
fn table(db: &Connection, name: &str) -> Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )
    .map_err(unavailable)
}

struct Metadata {
    day: i64,
    denial: Option<Denial>,
    denial_day: Option<i64>,
    breach_day: Option<i64>,
}
fn metadata(db: &Connection) -> Result<Metadata> {
    let mut q=db.prepare("SELECT singleton,version,last_day,CASE WHEN last_denial IS NULL THEN NULL WHEN typeof(last_denial)='text' AND length(CAST(last_denial AS BLOB)) BETWEEN 1 AND 32 THEN last_denial ELSE '' END,last_denial_day,breach_day FROM usage_budget_meta LIMIT 2").map_err(unavailable)?;
    let values = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })
        .map_err(unavailable)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(unavailable)?;
    if values.len() != 1 || values[0].0 != 1 || values[0].1 != VERSION {
        return Err(Denial::AccountingUnavailable);
    }
    let (_, _, day, denial, denial_day, breach_day) = values.into_iter().next().unwrap();
    check_day(day)?;
    if let Some(value) = denial_day {
        check_day(value)?;
    }
    if let Some(value) = breach_day {
        check_day(value)?;
    }
    if denial_day.is_some_and(|n| n > day)
        || breach_day.is_some_and(|n| n > day)
        || denial.is_some() != denial_day.is_some()
    {
        return Err(Denial::AccountingUnavailable);
    }
    Ok(Metadata {
        day,
        denial: denial.map(|value| Denial::parse(&value)).transpose()?,
        denial_day,
        breach_day,
    })
}
fn limits(db: &Connection) -> Result<Limits> {
    let mut q=db.prepare("SELECT singleton,CASE WHEN typeof(settings_json)='text' AND length(CAST(settings_json AS BLOB)) BETWEEN 1 AND 1024 THEN settings_json ELSE NULL END FROM usage_budget_settings LIMIT 2").map_err(unavailable)?;
    let rows = q
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(unavailable)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(unavailable)?;
    if rows.len() != 1 || rows[0].0 != 1 {
        return Err(Denial::AccountingUnavailable);
    }
    let result: Limits =
        serde_json::from_str(rows[0].1.as_deref().ok_or(Denial::AccountingUnavailable)?)
            .map_err(unavailable)?;
    result.validate()?;
    Ok(result)
}
pub fn initialize(db: &mut Connection, day: i64) -> Result<()> {
    check_day(day)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let names = [
        "usage_budget_meta",
        "usage_budget_settings",
        "usage_budget_reservations",
    ];
    let existing = names
        .map(|n| table(&tx, n))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    if existing.iter().all(|v| !*v) {
        tx.execute_batch("CREATE TABLE usage_budget_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL,last_day INTEGER NOT NULL,last_denial TEXT,last_denial_day INTEGER,breach_day INTEGER);
            CREATE TABLE usage_budget_settings(singleton INTEGER PRIMARY KEY CHECK(singleton=1),settings_json TEXT NOT NULL CHECK(length(CAST(settings_json AS BLOB))<=1024));
            CREATE TABLE usage_budget_reservations(id TEXT PRIMARY KEY NOT NULL,day INTEGER NOT NULL,bot_id TEXT NOT NULL REFERENCES bot_profiles(id),origin TEXT NOT NULL,kind TEXT NOT NULL,state TEXT NOT NULL,characters INTEGER NOT NULL,image_count INTEGER NOT NULL,image_bytes INTEGER NOT NULL,token_upper INTEGER,token_charge INTEGER NOT NULL,observed_tokens INTEGER,direct_request INTEGER NOT NULL);
            CREATE INDEX usage_budget_day ON usage_budget_reservations(day);").map_err(unavailable)?;
        tx.execute(
            "INSERT INTO usage_budget_meta(singleton,version,last_day) VALUES(1,?1,?2)",
            params![VERSION, day],
        )
        .map_err(unavailable)?;
        tx.execute(
            "INSERT INTO usage_budget_settings VALUES(1,?1)",
            [serde_json::to_string(&Limits::default()).map_err(unavailable)?],
        )
        .map_err(unavailable)?;
    } else if existing.iter().any(|v| !*v) {
        return Err(Denial::AccountingUnavailable);
    }
    let meta = metadata(&tx)?;
    validate_days(&tx, meta.day)?;
    // A malformed optional setting fails admission closed, but the operator may
    // still explicitly replace it through configure(). Do not latch that as an
    // unsupported database layout or make normal app startup fail.
    // Retain their full conservative charge. No operation or session is resumed.
    tx.execute(
        "UPDATE usage_budget_reservations SET state='unknown' WHERE state='pending'",
        [],
    )
    .map_err(unavailable)?;
    tx.commit().map_err(unavailable)
}

#[derive(Clone)]
struct Row {
    id: String,
    bot: String,
    kind: Kind,
    state: String,
    characters: u64,
    images: u64,
    bytes: u64,
    upper: Option<u64>,
    charge: u64,
    observed: Option<u64>,
    direct: bool,
}
fn number(value: i64) -> Result<u64> {
    if value < 0 || value as u64 > MAX_COUNTER {
        Err(Denial::AccountingUnavailable)
    } else {
        Ok(value as u64)
    }
}
// A malformed/future bucket must never hide a possibly-sent charge from the
// active-day WHERE clause. Check before advancing the clock or pruning history.
fn validate_days(db: &Connection, last_day: i64) -> Result<()> {
    let invalid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM usage_budget_reservations WHERE typeof(day)!='integer' OR day<0 OR day>?1)",[last_day],|r|r.get(0)).map_err(unavailable)?;
    if invalid {
        Err(Denial::AccountingUnavailable)
    } else {
        Ok(())
    }
}
fn rows(db: &Connection, day: i64) -> Result<Vec<Row>> {
    let mut q=db.prepare("SELECT CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE NULL END,CASE WHEN length(CAST(bot_id AS BLOB)) BETWEEN 1 AND 96 THEN bot_id ELSE NULL END,CASE WHEN length(CAST(origin AS BLOB))<=16 THEN origin ELSE NULL END,CASE WHEN length(CAST(kind AS BLOB))<=8 THEN kind ELSE NULL END,CASE WHEN length(CAST(state AS BLOB))<=12 THEN state ELSE NULL END,characters,image_count,image_bytes,token_upper,token_charge,observed_tokens,direct_request FROM usage_budget_reservations WHERE day=?1 ORDER BY id LIMIT ?2").map_err(unavailable)?;
    let mut values = q
        .query(params![day, (MAX_ROWS + 1) as i64])
        .map_err(unavailable)?;
    let mut result = Vec::new();
    while let Some(r) = values.next().map_err(unavailable)? {
        if result.len() == MAX_ROWS {
            return Err(Denial::Capacity);
        }
        let text = |i| -> Result<String> {
            r.get::<_, Option<String>>(i)
                .map_err(unavailable)?
                .ok_or(Denial::AccountingUnavailable)
        };
        let id = text(0)?;
        if uuid::Uuid::parse_str(&id).map_err(unavailable)?.to_string() != id {
            return Err(Denial::AccountingUnavailable);
        }
        let bot = text(1)?;
        if !valid_id(&bot) {
            return Err(Denial::AccountingUnavailable);
        }
        if !["autopilot", "automatic_chat"].contains(&text(2)?.as_str()) {
            return Err(Denial::AccountingUnavailable);
        }
        let kind = Kind::parse(&text(3)?)?;
        let state = text(4)?;
        if !["pending", "unknown", "known"].contains(&state.as_str()) {
            return Err(Denial::AccountingUnavailable);
        }
        let get = |i| -> Result<u64> { number(r.get(i).map_err(unavailable)?) };
        let optional = |i| -> Result<Option<u64>> {
            r.get::<_, Option<i64>>(i)
                .map_err(unavailable)?
                .map(number)
                .transpose()
        };
        let row = Row {
            id,
            bot,
            kind,
            state,
            characters: get(5)?,
            images: get(6)?,
            bytes: get(7)?,
            upper: optional(8)?,
            charge: get(9)?,
            observed: optional(10)?,
            direct: match get(11)? {
                0 => false,
                1 => true,
                _ => return Err(Denial::AccountingUnavailable),
            },
        };
        Cost {
            kind,
            characters: row.characters,
            images: row.images,
            image_bytes: row.bytes,
            token_upper: row.upper,
            direct_request: row.direct,
        }
        .validate()?;
        if row.observed.is_some_and(|n| n > 1_000_000_000)
            || row.charge > 1_000_000_000
            || (kind != Kind::Dialog && (row.charge != 0 || row.observed.is_some()))
            || (row.state == "known" && kind == Kind::Dialog && row.observed != Some(row.charge))
            || (kind == Kind::Dialog
                && row.state != "known"
                && (row.observed.is_some() || row.charge != row.upper.unwrap_or(0)))
        {
            return Err(Denial::AccountingUnavailable);
        }
        result.push(row);
    }
    Ok(result)
}
fn add(target: &mut u64, value: u64) -> Result<()> {
    *target = target
        .checked_add(value)
        .filter(|n| *n <= MAX_COUNTER)
        .ok_or(Denial::AccountingUnavailable)?;
    Ok(())
}
fn include(total: &mut Totals, row: &Row) -> Result<()> {
    match row.kind {
        Kind::Dialog => {
            add(&mut total.dialog_requests, 1)?;
            add(&mut total.charged_tokens, row.charge)?;
            add(&mut total.observed_tokens, row.observed.unwrap_or(0))?;
            if row.observed.is_none() {
                add(&mut total.unknown_token_requests, 1)?;
            }
            if row.state == "pending" {
                add(&mut total.reserved_tokens, row.charge)?;
            }
        }
        Kind::Tts => {
            add(&mut total.tts_requests, 1)?;
            add(&mut total.tts_characters, row.characters)?;
        }
        Kind::Recall => add(&mut total.hindsight_recall_requests, 1)?,
        Kind::Retain => add(&mut total.hindsight_retain_requests, 1)?,
    }
    add(&mut total.image_count, row.images)?;
    add(&mut total.image_bytes, row.bytes)?;
    if row.state == "pending" {
        add(&mut total.pending_requests, 1)?;
    }
    Ok(())
}
fn aggregate(values: &[Row]) -> Result<(Totals, BTreeMap<String, Totals>)> {
    let mut total = Totals::default();
    let mut bots = BTreeMap::new();
    for row in values {
        include(&mut total, row)?;
        if !bots.contains_key(&row.bot) && bots.len() == MAX_BOTS {
            return Err(Denial::Capacity);
        }
        include(bots.entry(row.bot.clone()).or_default(), row)?;
    }
    Ok((total, bots))
}
fn effective_day(db: &Connection, today: i64) -> Result<(Metadata, i64)> {
    check_day(today)?;
    let meta = metadata(db)?;
    validate_days(db, meta.day)?;
    let day = meta.day.max(today);
    if day != meta.day {
        db.execute(
            "UPDATE usage_budget_meta SET last_day=?1 WHERE singleton=1",
            [day],
        )
        .map_err(unavailable)?;
    }
    Ok((meta, day))
}
fn reject(db: &Connection, day: i64, reason: Denial) -> Result<()> {
    db.execute(
        "UPDATE usage_budget_meta SET last_denial=?1,last_denial_day=?2 WHERE singleton=1",
        params![reason.code(), day],
    )
    .map_err(unavailable)?;
    Ok(())
}

fn admission_denial(
    meta: &Metadata,
    day: i64,
    limits: &Limits,
    values: &[Row],
    total: &Totals,
    bots: &BTreeMap<String, Totals>,
    context: &Context,
    cost: Cost,
) -> Option<Denial> {
    let dialog_reason = if cost.kind == Kind::Dialog {
        dialog_denial(limits, total, values, cost.direct_request, cost.token_upper)
    } else {
        None
    };
    if meta.breach_day == Some(day)
        || values.iter().any(|row| {
            row.observed
                .zip(row.upper)
                .is_some_and(|(actual, bound)| actual > bound)
        })
    {
        Some(Denial::AccountingUnavailable)
    } else if values.len() >= MAX_ROWS
        || (!bots.contains_key(&context.bot_id) && bots.len() >= MAX_BOTS)
    {
        Some(Denial::Capacity)
    } else if dialog_reason.is_some() {
        dialog_reason
    } else if cost.kind == Kind::Tts
        && total.tts_characters.saturating_add(cost.characters) > limits.daily_tts_characters
    {
        Some(Denial::TtsLimit)
    } else if matches!(cost.kind, Kind::Recall | Kind::Retain)
        && total
            .hindsight_recall_requests
            .saturating_add(total.hindsight_retain_requests)
            >= limits.daily_hindsight_requests
    {
        Some(Denial::HindsightLimit)
    } else if cost.images > 0
        && total.image_count.saturating_add(cost.images) > limits.daily_image_count
    {
        Some(Denial::ImageCountLimit)
    } else if cost.image_bytes > 0
        && total.image_bytes.saturating_add(cost.image_bytes) > limits.daily_image_bytes
    {
        Some(Denial::ImageBytesLimit)
    } else {
        None
    }
}

pub fn reserve(
    db: &mut Connection,
    context: &Context,
    cost: Cost,
    today: i64,
) -> Result<Reservation> {
    if !context.origin.autonomous() || !valid_id(&context.bot_id) {
        return Err(Denial::AccountingUnavailable);
    }
    cost.validate()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let (meta, day) = effective_day(&tx, today)?;
    let limits = limits(&tx)?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",
            [&context.bot_id],
            |r| r.get(0),
        )
        .map_err(unavailable)?;
    if !exists {
        return Err(Denial::AccountingUnavailable);
    }
    let values = rows(&tx, day)?;
    let (total, bots) = aggregate(&values)?;
    let reason = admission_denial(&meta, day, &limits, &values, &total, &bots, context, cost);
    if let Some(reason) = reason {
        reject(&tx, day, reason)?;
        tx.commit().map_err(unavailable)?;
        return Err(reason);
    }
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO usage_budget_reservations(id,day,bot_id,origin,kind,state,characters,image_count,image_bytes,token_upper,token_charge,direct_request) VALUES(?1,?2,?3,?4,?5,'pending',?6,?7,?8,?9,?10,?11)",params![id,day,context.bot_id,context.origin.code(),cost.kind.code(),cost.characters as i64,cost.images as i64,cost.image_bytes as i64,cost.token_upper.map(|n|n as i64),cost.token_upper.unwrap_or(0) as i64,cost.direct_request as i64]).map_err(unavailable)?;
    // The durable possibly-sent charge exists before a transport receives input.
    tx.execute(
        "DELETE FROM usage_budget_reservations WHERE day<?1",
        [day.saturating_sub(31)],
    )
    .map_err(unavailable)?;
    tx.commit().map_err(unavailable)?;
    Ok(Reservation {
        id,
        bot_id: context.bot_id.clone(),
        day,
    })
}

/// Missing/invalid receipts retain the full conservative charge. A dropped
/// caller leaves a pending charge; startup converts it to unknown without send.
pub fn settle(db: &mut Connection, ticket: &Reservation, observed: Option<u64>) -> Result<()> {
    if observed.is_some_and(|n| n > 1_000_000_000) {
        return Err(Denial::AccountingUnavailable);
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let meta = metadata(&tx)?;
    validate_days(&tx, meta.day)?;
    let values = rows(&tx, ticket.day)?;
    let row = values
        .iter()
        .find(|r| r.id == ticket.id && r.bot == ticket.bot_id)
        .ok_or(Denial::AccountingUnavailable)?;
    if row.state != "pending" {
        tx.commit().map_err(unavailable)?;
        return Ok(());
    }
    let observed = if row.kind == Kind::Dialog {
        observed
    } else {
        None
    };
    let breach = observed
        .zip(row.upper)
        .is_some_and(|(actual, bound)| actual > bound);
    tx.execute("UPDATE usage_budget_reservations SET state=?1,observed_tokens=?2,token_charge=?3 WHERE id=?4 AND state='pending'",params![if observed.is_some()||row.kind!=Kind::Dialog{"known"}else{"unknown"},observed.map(|n|n as i64),observed.unwrap_or(row.charge) as i64,ticket.id]).map_err(unavailable)?;
    if breach {
        tx.execute("UPDATE usage_budget_meta SET breach_day=?1,last_denial='accounting_unavailable',last_denial_day=?1 WHERE singleton=1",[ticket.day]).map_err(unavailable)?;
    }
    tx.commit().map_err(unavailable)?;
    if breach {
        Err(Denial::AccountingUnavailable)
    } else {
        Ok(())
    }
}
pub fn snapshot(db: &mut Connection, today: i64) -> Result<Snapshot> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let (meta, day) = effective_day(&tx, today)?;
    let limits = limits(&tx)?;
    let (total, bots) = aggregate(&rows(&tx, day)?)?;
    let result = Snapshot {
        limits,
        day: date(day)?,
        clock_adjusted: day > today,
        total,
        bots: bots
            .into_iter()
            .map(|(bot_id, total)| BotTotal { bot_id, total })
            .collect(),
        last_denial: meta.denial.filter(|_| meta.denial_day == Some(day)),
    };
    tx.commit().map_err(unavailable)?;
    Ok(result)
}
pub fn configure(db: &mut Connection, value: &Limits, today: i64) -> Result<Snapshot> {
    value.validate()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    metadata(&tx)?;
    let changed = tx
        .execute(
            "UPDATE usage_budget_settings SET settings_json=?1 WHERE singleton=1",
            [serde_json::to_string(value).map_err(unavailable)?],
        )
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(Denial::AccountingUnavailable);
    }
    tx.commit().map_err(unavailable)?;
    snapshot(db, today)
}

/// Convenience only: prevents optional retrieval/capture when a whole new turn
/// is already impossible. Each actual send still needs atomic reserve().
fn dialog_denial(
    limits: &Limits,
    total: &Totals,
    values: &[Row],
    direct: bool,
    upper: Option<u64>,
) -> Option<Denial> {
    if limits.request_accounting == RequestAccounting::Direct && !direct {
        Some(Denial::RequestAccountingUnsupported)
    } else if limits.request_accounting == RequestAccounting::Direct
        && values.iter().any(|r| r.kind == Kind::Dialog && !r.direct)
    {
        Some(Denial::RequestHistoryUnbounded)
    } else if limits.hard_token_limit.is_some()
        && values
            .iter()
            .any(|r| r.kind == Kind::Dialog && r.upper.is_none())
    {
        Some(Denial::TokenHistoryUnbounded)
    } else if total.dialog_requests >= limits.daily_dialog_requests {
        Some(Denial::DialogLimit)
    } else if limits.hard_token_limit.is_some() && upper.is_none() {
        Some(Denial::TokenUnsupported)
    } else if limits
        .hard_token_limit
        .is_some_and(|limit| total.charged_tokens.saturating_add(upper.unwrap_or(0)) > limit)
    {
        Some(Denial::TokenLimit)
    } else {
        None
    }
}
pub fn preflight(
    db: &mut Connection,
    context: &Context,
    direct: bool,
    upper: Option<u64>,
    today: i64,
) -> Result<()> {
    preflight_cost(
        db,
        context,
        Cost {
            direct_request: direct,
            ..Cost::dialog(0, 0, upper)
        },
        today,
    )
}
/// Cheap check before potentially blocking credential lookup. This creates no
/// permission to send: reserve() repeats the same checks at the actual boundary.
pub fn preflight_cost(
    db: &mut Connection,
    context: &Context,
    cost: Cost,
    today: i64,
) -> Result<()> {
    if !context.origin.autonomous() {
        return Ok(());
    }
    if !valid_id(&context.bot_id) {
        return Err(Denial::AccountingUnavailable);
    }
    cost.validate()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let (meta, day) = effective_day(&tx, today)?;
    let limits = limits(&tx)?;
    let values = rows(&tx, day)?;
    let (total, bots) = aggregate(&values)?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",
            [&context.bot_id],
            |r| r.get(0),
        )
        .map_err(unavailable)?;
    if !exists {
        return Err(Denial::AccountingUnavailable);
    }
    if let Some(reason) =
        admission_denial(&meta, day, &limits, &values, &total, &bots, context, cost)
    {
        reject(&tx, day, reason)?;
        tx.commit().map_err(unavailable)?;
        Err(reason)
    } else {
        tx.commit().map_err(unavailable)
    }
}

#[cfg(test)]
#[path = "usage_budget/tests.rs"]
mod tests;
