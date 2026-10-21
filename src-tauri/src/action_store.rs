//! Transaction-scoped native action fences. Tokens are process-only, never
//! serialized, logged, exported, or accepted from IPC. No runtime dependency.
use super::{life_store::LifeStoreError, valid_id, world_store, Store};
use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};
pub(crate) type ProfileToken = [u8; 32];
fn token(value: &Value) -> Result<ProfileToken, LifeStoreError> {
    Ok(Sha256::digest(serde_json::to_vec(value).map_err(|_| LifeStoreError::CorruptData)?).into())
}
const MAX_PROFILE_BYTES: usize = 1_048_576;
fn profile(db: &Connection, bot: &str) -> Result<Value, LifeStoreError> {
    if !valid_id(bot) {
        return Err(LifeStoreError::InvalidInput);
    }
    let text: Option<String> = db.query_row(
        "SELECT CASE WHEN typeof(profile)='text' AND length(CAST(profile AS BLOB)) BETWEEN 1 AND ?2 THEN profile ELSE NULL END FROM bot_profiles WHERE id=?1",
        rusqlite::params![bot,MAX_PROFILE_BYTES as i64], |row| row.get(0)
    ).map_err(|_| LifeStoreError::StorageUnavailable)?;
    serde_json::from_str(text.as_deref().ok_or(LifeStoreError::CorruptData)?)
        .map_err(|_| LifeStoreError::CorruptData)
}
pub(crate) fn matches(
    db: &Connection,
    bot: &str,
    expected_profile: &ProfileToken,
    expected_world: &Value,
    studio: bool,
) -> bool {
    (!studio || selected(db, bot))
        && profile_current(db, bot, expected_profile)
        && world_store::load(db, true)
            .ok()
            .flatten()
            .and_then(|record| serde_json::to_value(record).ok())
            .as_ref()
            == Some(expected_world)
}
/// Reads only the caller's already-held connection/transaction. In particular,
/// a stream owner may differ from the actor whose action is being committed.
pub(crate) fn profile_current(db:&Connection,bot:&str,expected:&ProfileToken)->bool {
    profile(db,bot).and_then(|value|token(&value)).as_ref()==Ok(expected)
}
impl Store {
    pub(crate) fn action_profile_token(
        &self,
        bot: &str,
        expected_effective: &Value,
    ) -> Result<ProfileToken, LifeStoreError> {
        let raw = {
            let db = self
                .db
                .lock()
                .map_err(|_| LifeStoreError::StorageUnavailable)?;
            profile(&db, bot)?
        };
        if self
            .effective_profile(raw.clone())
            .map_err(|_| LifeStoreError::InvalidInput)?
            != *expected_effective
        {
            return Err(LifeStoreError::InvalidInput);
        }
        token(&raw)
    }
}

fn selected(db: &Connection, bot: &str) -> bool {
    db.query_row("SELECT CASE WHEN typeof(value)='text' AND length(CAST(value AS BLOB)) BETWEEN 1 AND 1048576 AND json_valid(value) THEN json_extract(value,'$.id') ELSE NULL END FROM settings WHERE key='profile'",[],|row|row.get::<_,Option<String>>(0)).ok().flatten().as_deref()==Some(bot)
}
