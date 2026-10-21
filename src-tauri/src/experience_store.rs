//! Child module of storage: bounded typed settings/facts and declarative manifests.
use super::Store;
use crate::{
    content_packs::{self, ContentRef, Installed, Manifest},
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
fn append_fact(db: &Connection, bot: &str, fact: ActivityFact) -> Result<bool, String> {
    fact.validate()?;
    if !fact.actor_ids.iter().any(|id| id == bot) {
        return Err("experience_invalid_fact".into());
    }
    let key = format!("experience-facts-v1:{bot}");
    let mut facts = read::<Vec<ActivityFact>>(db, &key)?.unwrap_or_default();
    if facts.len() > MAX_FACTS {
        return Err("experience_corrupt".into());
    }
    for old in &facts {
        old.validate()?;
        if !old.actor_ids.iter().any(|id| id == bot) {
            return Err("experience_corrupt".into());
        }
    }
    if facts.iter().any(|old| old.id == fact.id) {
        return Ok(false);
    }
    let oldest = fact.at_ms.saturating_sub(30 * 24 * 60 * 60 * 1000);
    facts.retain(|old| old.at_ms >= oldest);
    facts.push(fact);
    facts.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.id.cmp(&b.id)));
    facts.truncate(MAX_FACTS);
    write(db, &key, &facts)?;
    Ok(true)
}
fn source_id(id: &crate::life_domain::EventId) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(serde_json::to_vec(id).map_err(storage)?);
    Ok(format!("life_{digest:x}"))
}
fn pack_character<'a>(
    packs: &'a [Installed],
    reference: &ContentRef,
) -> Result<&'a content_packs::Character, String> {
    packs
        .iter()
        .find(|p| {
            p.active
                && p.manifest.id == reference.package_id
                && p.manifest.version == reference.package_version
        })
        .and_then(|p| {
            p.manifest
                .characters
                .iter()
                .find(|c| c.id == reference.template_id)
        })
        .ok_or_else(|| "pack_inactive_template".into())
}
/// Read through an existing native SQLite transaction. This never acquires
/// Store again, so World save can bind provenance atomically to its own CAS.
pub(super) fn read_content_catalog(db: &Connection) -> Result<Vec<Installed>, String> {
    let packs = read::<Vec<Installed>>(db, "content-packs-v1")?.unwrap_or_default();
    content_packs::validate_catalog(&packs)?;
    Ok(packs)
}
/// Only a fresh UUID directory created by this call is eligible for rollback.
struct NewCharacterDirectory {
    path: std::path::PathBuf,
    committed: bool,
}
impl Drop for NewCharacterDirectory {
    fn drop(&mut self) {
        if !self.committed
            && std::fs::symlink_metadata(&self.path)
                .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
impl Store {
    fn experience_owner(&self, bot: &str) -> Result<serde_json::Value, String> {
        if self.fixed_profile.as_deref().is_some_and(|id| id != bot) {
            return Err("experience_wrong_bot".into());
        }
        self.bot_profile(bot)
    }

    pub fn experience_settings(&self, bot: &str) -> Result<Settings, String> {
        let profile = self.experience_owner(bot)?;
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
        let profile = self.experience_owner(bot)?;
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
        self.experience_owner(bot)?;
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
    /// A real native observer calls after its actual transition/poll. No caller supplies a fact.
    pub(crate) fn record_experience_life_fact(
        &self,
        bot: &str,
        expected_revision: u64,
        event_sequence: u64,
    ) -> Result<bool, String> {
        self.record_life_fact(bot,expected_revision,event_sequence,None)
    }
    /// Retry the exact immutable native commit. Actual retained LIVE history is
    /// checked again in the write transaction; the snapshot supplies no grant.
    pub(crate) fn record_experience_native_snapshot(&self,bot:&str,expected:&super::life_store::LifeSnapshot)->Result<bool,String> {
        let sequence=expected.state().recent_events().last().ok_or("experience_missing_source")?.id().sequence();
        self.record_life_fact(bot,expected.state().revision(),sequence,Some(expected))
    }
    fn record_life_fact(&self,bot:&str,expected_revision:u64,event_sequence:u64,expected:Option<&super::life_store::LifeSnapshot>)->Result<bool,String> {
        use super::life_store::EventOrigin;
        use crate::experience::{FactKind, FactPhase};
        use crate::life_domain::{ActionPhase, CancelReason, CommittedEvent};
        self.experience_owner(bot)?;
        if expected_revision == 0
            || expected_revision > MAX_TIME
            || event_sequence != expected_revision
        {
            return Err("experience_stale_source".into());
        }
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let actual = super::life_store::read(&tx, bot)
            .map_err(storage)?
            .ok_or("experience_missing_source")?;
        let source=expected.unwrap_or(&actual);
        let state = source.state();
        if state.revision() != expected_revision
            || actual.state().revision()<expected_revision
            || actual.state().world_id()!=state.world_id()
            || state.actor_id().as_str() != bot
            || source.saved_at_ms()>actual.saved_at_ms()
            || actual.saved_at_ms() > MAX_TIME
        {
            return Err("experience_stale_source".into());
        }
        let stored = actual
            .events()
            .iter()
            .find(|event| event.record().id().sequence() == event_sequence)
            .ok_or("experience_missing_source")?;
        let record = stored.record();
        if stored.origin() != EventOrigin::Live
            || record.id().actor_id().as_str() != bot
            || record.at() != state.simulation_time()
            || !state.recent_events().any(|r| r == record)
            || !actual.state().recent_events().any(|r| r==record)
        {
            return Err("experience_not_live".into());
        }
        let Some(action) = state.current_action() else {
            return Ok(false);
        };
        let phase = match record.event() {
            CommittedEvent::Planned { intent }
                if intent == action.intent() && action.phase() == ActionPhase::Planned =>
            {
                FactPhase::Planned
            }
            CommittedEvent::Started { action_id }
                if action_id == action.intent().id() && action.phase() == ActionPhase::Started =>
            {
                FactPhase::Started
            }
            CommittedEvent::Completed { action_id }
                if action_id == action.intent().id()
                    && action.phase() == ActionPhase::Completed =>
            {
                FactPhase::Completed
            }
            CommittedEvent::Cancelled { action_id, reason }
                if action_id == action.intent().id()
                    && *reason != CancelReason::Restart
                    && action.phase() == ActionPhase::Cancelled =>
            {
                FactPhase::Cancelled
            }
            _ => return Ok(false),
        };
        let activity = serde_json::to_value(action.intent().activity()).map_err(storage)?;
        let kind = match activity.as_str() {
            Some("move_to") => FactKind::Move,
            Some("look_at") => FactKind::Look,
            Some("rest" | "recharge") => FactKind::Rest,
            Some("snack") => FactKind::Snack,
            Some("play") => FactKind::Play,
            Some("wave" | "socialize") => FactKind::Greeting,
            _ => return Ok(false),
        };
        let event_id = source_id(record.id())?;
        let fact = ActivityFact {
            id: event_id.clone(),
            actor_ids: vec![bot.into()],
            kind,
            phase,
            at_ms: source.saved_at_ms(),
            source_event_ids: vec![event_id],
            shared: false,
        };
        let inserted = append_fact(&tx, bot, fact)?;
        tx.commit().map_err(storage)?;
        Ok(inserted)
    }
    /// Revalidate the actual paired memory and both LIVE completed source events in one TX.
    pub(crate) fn record_experience_social_event(
        &self,
        bot: &str,
        event: &crate::social::SocialEvent,
    ) -> Result<bool, String> {
        use super::life_store::EventOrigin;
        use crate::{
            experience::{FactKind, FactPhase},
            life_domain::CommittedEvent,
            social::{Outcome, SceneKind},
        };
        self.experience_owner(bot)?;
        if event.outcome() != Outcome::Completed || !event.participants().iter().any(|id| id == bot)
        {
            return Ok(false);
        }
        let actions = event.actions().ok_or("experience_stale_source")?;
        let sources = event.source_events().ok_or("experience_stale_source")?;
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let (sequence,kind,outcome,participants,at):(i64,String,String,String,i64)=tx.query_row("SELECT sequence,CASE WHEN length(CAST(kind AS BLOB))<=16 THEN kind END,CASE WHEN length(CAST(outcome AS BLOB))<=16 THEN outcome END,CASE WHEN length(CAST(participants AS BLOB))<=512 THEN participants END,at_ms FROM social_memories WHERE owner=?1 AND event_id=?2",params![bot,event.id()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(storage)?;
        let sequence = u64::try_from(sequence)
            .ok()
            .filter(|n| *n <= MAX_TIME)
            .ok_or("experience_corrupt")?;
        let at = u64::try_from(at)
            .ok()
            .filter(|n| *n <= MAX_TIME)
            .ok_or("experience_corrupt")?;
        let members: Vec<String> =
            serde_json::from_str(&participants).map_err(|_| "experience_corrupt")?;
        let kind: SceneKind = serde_json::from_str(&kind).map_err(|_| "experience_corrupt")?;
        let outcome: Outcome = serde_json::from_str(&outcome).map_err(|_| "experience_corrupt")?;
        if sequence != event.sequence()
            || kind != event.kind()
            || outcome != Outcome::Completed
            || members.as_slice() != event.participants()
            || at != event.at_ms()
        {
            return Err("experience_stale_source".into());
        }
        for (index, id) in event.participants().iter().enumerate() {
            let actual = super::life_store::read(&tx, id)
                .map_err(storage)?
                .ok_or("experience_missing_source")?;
            let state = actual.state();
            if state.actor_id().as_str()!=id||actions[index].actor_id().as_str()!=id||actions[index].world_id()!=state.world_id()||sources[index].actor_id().as_str()!=id||!actual.events().iter().any(|stored|stored.origin()==EventOrigin::Live&&stored.record().id()==&sources[index]&&matches!(stored.record().event(),CommittedEvent::Completed{action_id}if action_id==&actions[index])&&state.recent_events().any(|r|r==stored.record())){return Err("experience_stale_source".into())}
        }
        let kind = match event.kind() {
            SceneKind::Greeting => FactKind::Greeting,
            SceneKind::Play => FactKind::Play,
            SceneKind::Rest => FactKind::Rest,
        };
        let fact = ActivityFact {
            id: event.id().into(),
            actor_ids: event.participants().to_vec(),
            kind,
            phase: FactPhase::Completed,
            at_ms: event.at_ms(),
            source_event_ids: sources.iter().map(source_id).collect::<Result<_, _>>()?,
            shared: false,
        };
        let inserted = append_fact(&tx, bot, fact)?;
        tx.commit().map_err(storage)?;
        Ok(inserted)
    }
    pub fn edit_experience_fact(
        &self,
        bot: &str,
        id: Option<&str>,
        shared: Option<bool>,
    ) -> Result<Vec<ActivityFact>, String> {
        self.experience_owner(bot)?;
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
    /// Explicit Main creation only. No current selection, credentials, API defaults or Life state are inherited.
    pub fn create_content_pack_character(
        &self,
        reference: &ContentRef,
    ) -> Result<serde_json::Value, String> {
        use serde_json::json;
        if self.fixed_profile.is_some() {
            return Err("experience_invalid_scope".into());
        }
        let template = pack_character(&self.content_packs()?, reference)?.clone();
        let mut profile: serde_json::Value =
            serde_json::from_str(super::DEFAULT_PROFILE).map_err(storage)?;
        let id = super::new_bot_id(&template.name);
        profile["id"] = json!(id);
        profile["name"] = json!(template.name);
        profile["tagline"] = json!("");
        profile["avatar"]["assetPath"] = json!(template.avatar);
        profile["contentRef"] = serde_json::to_value(reference).map_err(storage)?;
        for section in ["model", "voice", "stt", "hindsight"] {
            profile[section]["apiKeyId"] = json!("");
        }
        profile["model"]["toolsEnabled"] = json!(false);
        profile["voice"]["enabled"] = json!(false);
        profile["mcpServers"] = json!([]);
        profile["skillsEnabled"] = json!([]);
        profile["allowSkillScripts"] = json!(false);
        profile["screenContext"] = json!({"enabled":false,"displayId":""});
        profile["wakeWord"]["enabled"] = json!(false);
        profile["hindsight"]["enabled"] = json!(false);
        profile["hindsight"]["bankId"] = json!("");
        profile["autopilot"]["speak"] = json!(false);
        profile["autopilot"]["instructions"] = json!("");
        profile["chat"]["mode"] = json!("manual");
        profile["chat"]["autoReply"] = json!(false);
        profile["chat"]["trigger"] = json!(format!("@{}", super::bot_slug(&template.name)));
        for platform in ["twitch", "youtube"] {
            profile["chat"][platform]["sendReplies"] = json!(false);
        }
        for (platform, key) in [
            ("twitch", "secretId"),
            ("youtube", "apiKeyId"),
            ("youtube", "sendSecretId"),
        ] {
            profile["chat"][platform][key] = json!("");
        }
        profile["output"]["mode"] = json!("studio");
        profile["output"]["streamingEnabled"] = json!(false);
        if let Some(emotes) = profile["emotes"].as_array_mut() {
            for emote in emotes {
                emote["allowChat"] = json!(false);
            }
        }
        super::life_store::strip_profile_payload(&mut profile);
        super::validate_profile(&profile)?;
        // Reserve only this new path. Creating it first keeps ensure_profile's
        // legacy Skill migration outside our transaction and leaves no grants.
        let directory = self.profile_dir(&profile)?;
        std::fs::create_dir(&directory).map_err(storage)?;
        let mut created = NewCharacterDirectory {
            path: directory,
            committed: false,
        };
        self.ensure_profile(&profile)?;
        let mut db = self.db.lock().map_err(storage)?;
        super::social_store::initialize(&mut db)?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let packs = read::<Vec<Installed>>(&tx, "content-packs-v1")?.unwrap_or_default();
        content_packs::validate_catalog(&packs)?;
        let current = pack_character(&packs, reference)?;
        // Re-resolve the actual catalog after filesystem preparation, under the
        // same transaction that makes the new owner and initial traits visible.
        if current.name != template.name
            || current.avatar != template.avatar
            || current.sociability != template.sociability
            || current.rest_preference != template.rest_preference
        {
            return Err("experience_stale_revision".into());
        }
        tx.execute(
            "INSERT INTO bot_profiles(id,profile) VALUES(?1,?2)",
            params![id, profile.to_string()],
        )
        .map_err(storage)?;
        super::social_store::seed_template_character(
            &tx,
            &id,
            current.sociability,
            current.rest_preference,
        )?;
        tx.commit().map_err(storage)?;
        created.committed = true;
        Ok(profile)
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
    fn template_ref() -> ContentRef {
        ContentRef {
            package_id: "desk-garden".into(),
            package_version: 1,
            template_id: "desk-garden:sprout".into(),
        }
    }
    fn install_garden(store: &Store, active: bool) {
        let manifest =
            Manifest::parse(include_str!("../../public/content-packs/desk-garden.json")).unwrap();
        store.install_content_pack(manifest).unwrap();
        if active {
            store
                .set_content_pack_active("desk-garden", 1, true)
                .unwrap();
        }
    }
    #[test]
    fn character_template_is_explicit_fresh_and_never_selects_or_inherits_permissions() {
        let f = Fixture::new();
        let selected = f.0.profile().unwrap();
        install_garden(&f.0, true);
        let created = f.0.create_content_pack_character(&template_ref()).unwrap();
        let id = created["id"].as_str().unwrap();
        assert_ne!(created["id"], selected["id"]);
        assert_eq!(f.0.profile().unwrap(), selected);
        assert_eq!(created["name"], "Sprout");
        assert_eq!(created["avatar"]["assetPath"], "builtin:robot-expressive");
        assert_eq!(
            created["contentRef"],
            serde_json::to_value(template_ref()).unwrap()
        );
        for section in ["model", "voice", "stt", "hindsight"] {
            assert_eq!(created[section]["apiKeyId"], "");
        }
        assert_eq!(created["chat"]["twitch"]["secretId"], "");
        assert_eq!(created["chat"]["youtube"]["apiKeyId"], "");
        assert_eq!(created["chat"]["youtube"]["sendSecretId"], "");
        for (section, key) in [
            ("model", "toolsEnabled"),
            ("voice", "enabled"),
            ("wakeWord", "enabled"),
            ("screenContext", "enabled"),
            ("hindsight", "enabled"),
            ("chat", "autoReply"),
            ("autopilot", "speak"),
            ("output", "streamingEnabled"),
        ] {
            assert_eq!(created[section][key], false);
        }
        assert_eq!(created["mcpServers"], serde_json::json!([]));
        assert_eq!(created["skillsEnabled"], serde_json::json!([]));
        assert_eq!(created["allowSkillScripts"], false);
        assert!(f.0.load_life(id).unwrap().is_none());
        assert!(f.0.experience_facts(id).unwrap().is_empty());
        let social = f.0.social_snapshot(id).unwrap();
        assert_eq!(social.development.traits.sociability, 450);
        assert_eq!(social.development.traits.rest_preference, 600);
        assert!(social.memories.is_empty());
        assert!(social.relationships.is_empty());
    }
    #[test]
    fn inactive_missing_and_wrong_version_templates_never_create_profiles() {
        let f = Fixture::new();
        install_garden(&f.0, false);
        let count = f.0.list_bots().unwrap().len();
        assert!(f.0.create_content_pack_character(&template_ref()).is_err());
        f.0.set_content_pack_active("desk-garden", 1, true).unwrap();
        let mut wrong = template_ref();
        wrong.package_version = 2;
        assert!(f.0.create_content_pack_character(&wrong).is_err());
        wrong = template_ref();
        wrong.template_id = "desk-garden:unknown".into();
        assert!(f.0.create_content_pack_character(&wrong).is_err());
        f.0.set_content_pack_active("desk-garden", 2, false)
            .unwrap();
        assert!(f.0.create_content_pack_character(&template_ref()).is_err());
        assert_eq!(f.0.list_bots().unwrap().len(), count);
    }
    #[test]
    fn scoped_profile_context_cannot_create_a_template_owner() {
        let f = Fixture::new();
        install_garden(&f.0, true);
        let id = f.0.profile().unwrap()["id"].as_str().unwrap().to_owned();
        assert!(f
            .0
            .scoped(&id)
            .unwrap()
            .create_content_pack_character(&template_ref())
            .is_err());
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
    fn live_complete(store: &Store, id: &str, now: u64) -> u64 {
        use crate::life_domain::{Activity, LifeEvent, NeedEffect};
        let old = store.load_life(id).unwrap().unwrap();
        let planned = store
            .apply_life_event(
                id,
                old.state().revision(),
                0,
                LifeEvent::Plan {
                    activity: Activity::Rest,
                    completion_effect: NeedEffect::new(0, 0, 0, 0).unwrap(),
                },
                now,
            )
            .unwrap();
        let action = planned
            .snapshot
            .state()
            .current_action()
            .unwrap()
            .intent()
            .id()
            .clone();
        let started = store
            .apply_life_event(
                id,
                planned.snapshot.state().revision(),
                0,
                LifeEvent::Start {
                    action_id: action.clone(),
                },
                now,
            )
            .unwrap();
        store
            .apply_life_event(
                id,
                started.snapshot.state().revision(),
                0,
                LifeEvent::Complete { action_id: action },
                now,
            )
            .unwrap()
            .snapshot
            .state()
            .revision()
    }
    fn initialize(store: &Store, id: &str) {
        use crate::life_domain::{Needs, RandomState, WorldId};
        store
            .initialize_life(
                id,
                WorldId::new("experience_test").unwrap(),
                Needs::new(5000, 5000, 5000, 5000).unwrap(),
                RandomState::seeded(7),
                0,
            )
            .unwrap();
    }
    #[test]
    fn confirmed_facts_are_private_deduped_bounded_and_deletable() {
        let f = Fixture::new();
        let id = f.0.profile().unwrap()["id"].as_str().unwrap().to_owned();
        initialize(&f.0, &id);
        let mut revision = 0;
        for n in 0..260 {
            revision = live_complete(&f.0, &id, n);
            assert!(f
                .0
                .record_experience_life_fact(&id, revision, revision)
                .unwrap());
        }
        let facts = f.0.experience_facts(&id).unwrap();
        assert_eq!(facts.len(), 256);
        assert!(facts
            .iter()
            .all(|f| !f.shared && f.phase == crate::experience::FactPhase::Completed));
        assert!(!f
            .0
            .record_experience_life_fact(&id, revision, revision)
            .unwrap());
        let shared =
            f.0.edit_experience_fact(&id, Some(&facts[0].id), Some(true))
                .unwrap();
        assert!(shared[0].shared);
        f.0.edit_experience_fact(&id, None, None).unwrap();
        assert!(f.0.experience_facts(&id).unwrap().is_empty());
    }
    #[test]
    fn stale_offline_and_wrong_actor_sources_never_create_facts() {
        let f = Fixture::new();
        let id = f.0.profile().unwrap()["id"].as_str().unwrap().to_owned();
        initialize(&f.0, &id);
        let revision = live_complete(&f.0, &id, 100);
        assert!(f
            .0
            .record_experience_life_fact(&id, revision + 1, revision + 1)
            .is_err());
        assert!(f
            .0
            .record_experience_life_fact(&id, revision, revision - 1)
            .is_err());
        let other = f.0.create_bot("Other", false).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(f
            .0
            .record_experience_life_fact(&other, revision, revision)
            .is_err());
        {
            let db = f.0.db.lock().unwrap();
            db.execute(
                "UPDATE bot_life_events SET origin='offline' WHERE bot_id=?1 AND revision=?2",
                params![id, i64::try_from(revision).unwrap()],
            )
            .unwrap();
        }
        assert!(f
            .0
            .record_experience_life_fact(&id, revision, revision)
            .is_err());
        assert!(f.0.experience_facts(&id).unwrap().is_empty());
    }
    #[test]
    fn immutable_native_fact_retry_uses_retained_live_history_after_checkpoint() {
        let f=Fixture::new();let id=f.0.profile().unwrap()["id"].as_str().unwrap().to_owned();
        initialize(&f.0,&id);let revision=live_complete(&f.0,&id,100);
        let expected=f.0.load_life(&id).unwrap().unwrap();
        let checkpoint=f.0.apply_life_event(&id,revision,30_000,crate::life_domain::LifeEvent::Tick{},30_100).unwrap();
        assert_eq!(checkpoint.outcome,super::super::life_store::WriteOutcome::Applied);
        assert!(f.0.record_experience_life_fact(&id,revision,revision).is_err());
        assert!(f.0.record_experience_native_snapshot(&id,&expected).unwrap());
        let facts=f.0.experience_facts(&id).unwrap();assert_eq!(facts.len(),1);assert_eq!(facts[0].at_ms,100);
        assert!(!f.0.record_experience_native_snapshot(&id,&expected).unwrap());
        f.0.edit_experience_fact(&id,None,None).unwrap();
        {let db=f.0.db.lock().unwrap();db.execute("UPDATE bot_life_events SET origin='offline' WHERE bot_id=?1 AND revision=?2",params![id,i64::try_from(revision).unwrap()]).unwrap();}
        assert!(f.0.record_experience_native_snapshot(&id,&expected).is_err());assert!(f.0.experience_facts(&id).unwrap().is_empty());
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
