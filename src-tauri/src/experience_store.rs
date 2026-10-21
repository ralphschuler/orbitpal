//! Child module of storage: bounded typed settings/facts and declarative manifests.
use super::Store;
use crate::{
    content_packs::{self, Installed, Manifest},
    experience::{ActivityFact, Preferences, Settings, MAX_FACTS, MAX_TIME},
};
use rusqlite::{params, Connection, OptionalExtension};
const MAX_STORED: usize = 2 * 1024 * 1024;
fn storage(_: impl std::fmt::Display) -> String {
    "experience_storage_unavailable".into()
}
fn key(bot: &str) -> Result<String, String> {
    if !crate::experience::valid_id(bot) {
        return Err("experience_invalid".into());
    }
    Ok(format!("experience-v1:{bot}"))
}
fn read<T: serde::de::DeserializeOwned>(db: &Connection, key: &str) -> Result<Option<T>, String> {
    let row:Option<(bool,Option<String>)>=db.query_row("SELECT length(CAST(value AS BLOB))<=?2, CASE WHEN length(CAST(value AS BLOB))<=?2 THEN value ELSE NULL END FROM settings WHERE key=?1",params![key,MAX_STORED as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?;
    match row {
        None => Ok(None),
        Some((true, Some(v))) => serde_json::from_str(&v)
            .map(Some)
            .map_err(|_| "experience_corrupt".into()),
        _ => Err("experience_corrupt".into()),
    }
}
fn inherited(profile: &serde_json::Value) -> Settings {
    let mut settings = Settings::default();
    if profile["autopilot"]["speak"].as_bool() == Some(true) {
        settings.preferences.preset = crate::experience::Preset::Stream;
    }
    settings
}
fn write<T: serde::Serialize>(db: &Connection, key: &str, value: &T) -> Result<(), String> {
    let json = serde_json::to_string(value).map_err(storage)?;
    if json.len() > MAX_STORED {
        return Err("experience_capacity".into());
    }
    db.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,json]).map_err(storage)?;
    Ok(())
}
impl Store {
    pub fn experience_settings(&self, bot: &str) -> Result<Settings, String> {
        let profile = self.bot_profile(bot)?;
        let db = self.db.lock().map_err(storage)?;
        let settings = read::<Settings>(&db, &key(bot)?)?.unwrap_or_else(|| inherited(&profile));
        if settings.version != 1 || settings.revision > MAX_TIME {
            return Err("experience_corrupt".into());
        }
        settings.preferences.validate()?;
        Ok(settings)
    }
    pub fn set_experience_preferences(
        &self,
        bot: &str,
        expected: u64,
        preferences: Preferences,
    ) -> Result<Settings, String> {
        preferences.validate()?;
        let profile = self.bot_profile(bot)?;
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let k = key(bot)?;
        let old = read::<Settings>(&tx, &k)?.unwrap_or_else(|| inherited(&profile));
        old.preferences.validate()?;
        if old.version != 1 || old.revision != expected {
            return Err("experience_stale_revision".into());
        }
        let revision = expected
            .checked_add(1)
            .filter(|r| *r <= MAX_TIME)
            .ok_or("experience_capacity")?;
        let next = Settings {
            version: 1,
            revision,
            preferences,
        };
        write(&tx, &k, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
    pub fn experience_facts(&self, bot: &str) -> Result<Vec<ActivityFact>, String> {
        self.bot_profile(bot)?;
        let db = self.db.lock().map_err(storage)?;
        let facts = read::<Vec<ActivityFact>>(&db, &format!("experience-facts-v1:{bot}"))?
            .unwrap_or_default();
        if facts.len() > MAX_FACTS {
            return Err("experience_corrupt".into());
        }
        for f in &facts {
            f.validate()?;
            if !f.actor_ids.iter().any(|a| a == bot) {
                return Err("experience_corrupt".into());
            }
        }
        Ok(facts)
    }
    /// Native confirmed-outcome hook only. No renderer/import/IPC may supply facts.
    pub(crate) fn record_experience_fact(
        &self,
        bot: &str,
        mut fact: ActivityFact,
    ) -> Result<(), String> {
        fact.validate()?;
        self.bot_profile(bot)?;
        if !fact.actor_ids.iter().any(|a| a == bot) {
            return Err("experience_invalid_fact".into());
        }
        fact.shared = false;
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let k = format!("experience-facts-v1:{bot}");
        let mut facts = read::<Vec<ActivityFact>>(&tx, &k)?.unwrap_or_default();
        if facts.len() > MAX_FACTS {
            return Err("experience_corrupt".into());
        }
        for f in &facts {
            f.validate()?;
            if !f.actor_ids.iter().any(|id| id == bot) {
                return Err("experience_corrupt".into());
            }
        }
        if facts.iter().any(|f| f.id == fact.id) {
            return Ok(());
        }
        let oldest = fact.at_ms.saturating_sub(30 * 24 * 60 * 60 * 1000);
        facts.retain(|f| f.at_ms >= oldest);
        facts.push(fact);
        facts.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.id.cmp(&b.id)));
        facts.truncate(MAX_FACTS);
        write(&tx, &k, &facts)?;
        tx.commit().map_err(storage)
    }
    pub fn edit_experience_fact(
        &self,
        bot: &str,
        id: Option<&str>,
        shared: Option<bool>,
    ) -> Result<Vec<ActivityFact>, String> {
        self.bot_profile(bot)?;
        if id.is_some_and(|id| !crate::experience::valid_id(id)) {
            return Err("experience_invalid".into());
        }
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let k = format!("experience-facts-v1:{bot}");
        let mut facts = read::<Vec<ActivityFact>>(&tx, &k)?.unwrap_or_default();
        if facts.len() > MAX_FACTS {
            return Err("experience_corrupt".into());
        }
        for f in &facts {
            f.validate()?;
            if !f.actor_ids.iter().any(|id| id == bot) {
                return Err("experience_corrupt".into());
            }
        }
        match (id, shared) {
            (Some(id), Some(value)) => {
                let f = facts
                    .iter_mut()
                    .find(|f| f.id == id)
                    .ok_or("experience_unknown_fact")?;
                if f.phase != crate::experience::FactPhase::Completed {
                    return Err("experience_not_completed".into());
                }
                f.shared = value
            }
            (Some(id), None) => facts.retain(|f| f.id != id),
            (None, None) => facts.clear(),
            _ => return Err("experience_invalid".into()),
        }
        write(&tx, &k, &facts)?;
        tx.commit().map_err(storage)?;
        Ok(facts)
    }
    pub fn content_packs(&self) -> Result<Vec<Installed>, String> {
        let db = self.db.lock().map_err(storage)?;
        let packs = read::<Vec<Installed>>(&db, "content-packs-v1")?.unwrap_or_default();
        content_packs::validate_catalog(&packs)?;
        Ok(packs)
    }
    pub fn install_content_pack(&self, manifest: Manifest) -> Result<Vec<Installed>, String> {
        manifest.validate()?;
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let mut packs = read::<Vec<Installed>>(&tx, "content-packs-v1")?.unwrap_or_default();
        content_packs::validate_catalog(&packs)?;
        if let Some(old) = packs.iter_mut().find(|p| p.manifest.id == manifest.id) {
            if old.active {
                return Err("pack_deactivate_before_upgrade".into());
            }
            if manifest.version <= old.manifest.version {
                return Err("pack_version_not_newer".into());
            }
            old.revision = old
                .revision
                .checked_add(1)
                .filter(|r| *r <= MAX_TIME)
                .ok_or("pack_capacity")?;
            old.manifest = manifest;
        } else {
            if packs.len() >= content_packs::MAX_PACKS {
                return Err("pack_capacity".into());
            }
            packs.push(Installed {
                manifest,
                revision: 1,
                active: false,
            });
        }
        content_packs::validate_catalog(&packs)?;
        write(&tx, "content-packs-v1", &packs)?;
        tx.commit().map_err(storage)?;
        Ok(packs)
    }
    pub fn set_content_pack_active(
        &self,
        id: &str,
        expected: u64,
        active: bool,
    ) -> Result<Vec<Installed>, String> {
        if !crate::experience::valid_id(id) {
            return Err("pack_invalid_id".into());
        }
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let mut packs = read::<Vec<Installed>>(&tx, "content-packs-v1")?.unwrap_or_default();
        content_packs::validate_catalog(&packs)?;
        let p = packs
            .iter_mut()
            .find(|p| p.manifest.id == id)
            .ok_or("pack_missing")?;
        if p.revision != expected {
            return Err("experience_stale_revision".into());
        }
        p.revision = p
            .revision
            .checked_add(1)
            .filter(|r| *r <= MAX_TIME)
            .ok_or("pack_capacity")?;
        p.active = active;
        content_packs::validate_catalog(&packs)?;
        write(&tx, "content-packs-v1", &packs)?;
        tx.commit().map_err(storage)?;
        Ok(packs)
    }
    pub fn delete_content_pack(&self, id: &str) -> Result<Vec<Installed>, String> {
        if !crate::experience::valid_id(id) {
            return Err("pack_invalid_id".into());
        }
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let mut packs = read::<Vec<Installed>>(&tx, "content-packs-v1")?.unwrap_or_default();
        content_packs::validate_catalog(&packs)?;
        if packs.iter().any(|p| p.manifest.id == id && p.active) {
            return Err("pack_deactivate_before_delete".into());
        }
        packs.retain(|p| p.manifest.id != id);
        content_packs::validate_catalog(&packs)?;
        write(&tx, "content-packs-v1", &packs)?;
        tx.commit().map_err(storage)?;
        Ok(packs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(Store);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("orbitpal-experience-{}", uuid::Uuid::new_v4()));
            Self(Store::new(root).unwrap())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0.root);
        }
    }
    #[test]
    fn missing_settings_inherit_only_saved_speech_intent() {
        let profile = serde_json::json!({"autopilot":{"speak":true},"active":false});
        assert_eq!(
            inherited(&profile).preferences.preset,
            crate::experience::Preset::Stream
        );
        assert_eq!(
            inherited(&serde_json::json!({"autopilot":{"speak":false}}))
                .preferences
                .preset,
            crate::experience::Preset::Focus
        );
        assert!(serde_json::to_value(inherited(&profile))
            .unwrap()
            .get("active")
            .is_none());
    }
    #[test]
    fn preferences_persist_with_revision_not_activation() {
        let f = Fixture::new();
        let id = f.0.profile().unwrap()["id"].as_str().unwrap().to_owned();
        let mut p = Preferences::default();
        p.preset = crate::experience::Preset::Quiet;
        let s = f.0.set_experience_preferences(&id, 0, p.clone()).unwrap();
        assert_eq!(s.revision, 1);
        assert!(f.0.set_experience_preferences(&id, 0, p).is_err());
        assert_eq!(f.0.experience_settings(&id).unwrap().revision, 1);
        assert!(!crate::experience::ExperienceRuntime::default()
            .paused(&id)
            .unwrap());
    }
    #[test]
    fn confirmed_facts_are_private_deduped_bounded_and_deletable() {
        let f = Fixture::new();
        let id = f.0.profile().unwrap()["id"].as_str().unwrap().to_owned();
        for n in 0..260 {
            let fact = ActivityFact {
                id: format!("event_{n}"),
                actor_ids: vec![id.clone()],
                kind: crate::experience::FactKind::Rest,
                phase: crate::experience::FactPhase::Completed,
                at_ms: n,
                source_event_ids: vec![format!("source_{n}")],
                shared: true,
            };
            f.0.record_experience_fact(&id, fact).unwrap();
        }
        let facts = f.0.experience_facts(&id).unwrap();
        assert_eq!(facts.len(), 256);
        assert!(facts.iter().all(|f| !f.shared));
        f.0.record_experience_fact(&id, facts[0].clone()).unwrap();
        assert_eq!(f.0.experience_facts(&id).unwrap().len(), 256);
        let shared =
            f.0.edit_experience_fact(&id, Some(&facts[0].id), Some(true))
                .unwrap();
        assert!(shared[0].shared);
        f.0.edit_experience_fact(&id, None, None).unwrap();
        assert!(f.0.experience_facts(&id).unwrap().is_empty());
    }
    #[test]
    fn installed_manifest_is_inactive_until_explicit_cas() {
        let f = Fixture::new();
        let m =
            Manifest::parse(include_str!("../../public/content-packs/desk-garden.json")).unwrap();
        let packs = f.0.install_content_pack(m).unwrap();
        assert!(!packs[0].active);
        assert!(f.0.set_content_pack_active("desk-garden", 0, true).is_err());
        let packs = f.0.set_content_pack_active("desk-garden", 1, true).unwrap();
        assert!(packs[0].active);
        assert!(f.0.delete_content_pack("desk-garden").is_err());
        f.0.set_content_pack_active("desk-garden", 2, false)
            .unwrap();
        assert!(f.0.delete_content_pack("desk-garden").unwrap().is_empty());
    }
}
