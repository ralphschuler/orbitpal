//! Durable bounded preference data. No active session, lease or clock is saved.
use super::Store;
use crate::life_self_care::CareConfig;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

const ERROR: &str = "Life-Einstellungen sind nicht verfügbar.";
const CONFLICT: &str = "Life-Einstellungen wurden geändert. Bitte aktualisieren.";
const LIMIT: usize = 4096;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CareSnapshot {
    pub version: u16,
    pub bot_id: String,
    pub revision: u64,
    pub config: CareConfig,
}

fn initialize(db: &rusqlite::Connection) -> Result<(), String> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS life_care_preferences (bot_id TEXT PRIMARY KEY, version INTEGER NOT NULL CHECK(version=1), revision INTEGER NOT NULL CHECK(revision>=1), config TEXT NOT NULL)").map_err(|_| ERROR)?;
    Ok(())
}
impl Store {
    fn care_scope(&self, bot: &str) -> Result<(), String> {
        if !super::valid_id(bot) || self.fixed_profile.as_deref().is_some_and(|id| id != bot) {
            return Err(ERROR.into());
        }
        self.bot_profile(bot).map_err(|_| ERROR)?;
        Ok(())
    }
    pub(crate) fn life_care_preferences(&self, bot: &str) -> Result<CareSnapshot, String> {
        self.care_scope(bot)?;
        let db = self.db.lock().map_err(|_| ERROR)?;
        initialize(&db)?;
        let saved = db.query_row("SELECT version,revision,CASE WHEN typeof(config)='text' AND length(CAST(config AS BLOB))<=4096 THEN config ELSE NULL END FROM life_care_preferences WHERE bot_id=?1", [bot], |row| Ok((row.get::<_,u16>(0)?, row.get::<_,i64>(1)?, row.get::<_,String>(2)?))).optional().map_err(|_| ERROR)?;
        let (revision, config) = if let Some((version, revision, encoded)) = saved {
            let revision = u64::try_from(revision).map_err(|_| ERROR)?;
            if version != 1
                || !(1..=crate::life_domain::MAX_COUNTER).contains(&revision)
                || encoded.len() > LIMIT
            {
                return Err(ERROR.into());
            }
            let config: CareConfig = serde_json::from_str(&encoded).map_err(|_| ERROR)?;
            config.validate().map_err(|_| ERROR)?;
            (revision, config)
        } else {
            (0, CareConfig::default())
        };
        Ok(CareSnapshot {
            version: 1,
            bot_id: bot.into(),
            revision,
            config,
        })
    }
    pub(crate) fn save_life_care_preferences(
        &self,
        bot: &str,
        expected: u64,
        config: CareConfig,
    ) -> Result<CareSnapshot, String> {
        self.care_scope(bot)?;
        config.validate().map_err(|_| ERROR)?;
        if expected >= crate::life_domain::MAX_COUNTER {
            return Err(ERROR.into());
        }
        let encoded = serde_json::to_string(&config).map_err(|_| ERROR)?;
        if encoded.len() > LIMIT {
            return Err(ERROR.into());
        }
        let mut db = self.db.lock().map_err(|_| ERROR)?;
        initialize(&db)?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| ERROR)?;
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",
                [bot],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| ERROR)?;
        if !exists {
            return Err(ERROR.into());
        }
        let revision = expected + 1;
        let changed = if expected == 0 {
            tx.execute("INSERT OR IGNORE INTO life_care_preferences(bot_id,version,revision,config) VALUES(?1,1,1,?2)", params![bot,encoded])
        } else {
            tx.execute("UPDATE life_care_preferences SET revision=?1,config=?2 WHERE bot_id=?3 AND version=1 AND revision=?4", params![revision as i64,encoded,bot,expected as i64])
        }.map_err(|_| ERROR)?;
        if changed != 1 {
            return Err(CONFLICT.into());
        }
        tx.commit().map_err(|_| ERROR)?;
        Ok(CareSnapshot {
            version: 1,
            bot_id: bot.into(),
            revision,
            config,
        })
    }
}

#[tauri::command(async)]
pub(crate) fn get_life_care_preferences(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
    bot_id: String,
) -> Result<CareSnapshot, String> {
    if window.label() != "main" {
        return Err(ERROR.into());
    }
    state.store.life_care_preferences(&bot_id)
}
#[tauri::command(async)]
pub(crate) fn save_life_care_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    bot_id: String,
    expected_revision: u64,
    config: CareConfig,
) -> Result<CareSnapshot, String> {
    use tauri::Manager;
    if window.label() != "main" {
        return Err(ERROR.into());
    }
    let state = app.state::<crate::AppState>();
    state.updater.ensure_idle()?;
    state.store.care_scope(&bot_id)?;
    config.validate().map_err(|_| ERROR)?;
    state.life_runtime.invalidate_bot(&app, &bot_id);
    state
        .store
        .save_life_care_preferences(&bot_id, expected_revision, config)
}

#[cfg(test)]
#[path = "life_care_store/tests.rs"]
mod tests;
