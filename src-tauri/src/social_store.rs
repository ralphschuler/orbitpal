//! Optional, bot-bound social data in the application's existing SQLite Store.
use super::{valid_id, Store};
use crate::{
    character_development::{Character, HobbyKind},
    life_domain::MAX_COUNTER,
    social::{Outcome, Phase, Reason, SceneKind, SceneView, SocialEvent},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
const FAIL: &str = "social data unavailable";
const MAX_JSON: usize = 32768;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Relationship {
    pub other_id: String,
    pub familiarity: u16,
    pub affection: i16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Memory {
    pub event_id: String,
    pub sequence: u64,
    pub kind: SceneKind,
    pub outcome: Outcome,
    pub participants: [String; 2],
    pub at_ms: u64,
    pub origin: String,
    pub shared: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Summary {
    pub completed: u32,
    pub cancelled: u32,
    pub declined: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SocialSnapshot {
    pub version: u8,
    pub bot_id: String,
    pub revision: u64,
    pub relationships: Vec<Relationship>,
    pub memories: Vec<Memory>,
    pub summary: Summary,
    pub development: Character,
    pub scenes: Vec<SceneView>,
}
struct Bot {
    revision: u64,
    sequence: u64,
    mask: u8,
    context_sequence: u64,
    context_mask: u8,
    context_goal: String,
    action_world: String,
    action_ordinal: u64,
    character: Character,
    summary: Summary,
}
fn err(_: impl std::fmt::Debug) -> String {
    FAIL.into()
}
pub(crate) fn initialize(db: &mut Connection) -> Result<(), String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS social_schema(version INTEGER NOT NULL); INSERT INTO social_schema(version) SELECT 1 WHERE NOT EXISTS(SELECT 1 FROM social_schema); CREATE TABLE IF NOT EXISTS social_meta(id INTEGER PRIMARY KEY CHECK(id=1),sequence INTEGER NOT NULL,epoch INTEGER NOT NULL); INSERT OR IGNORE INTO social_meta VALUES(1,0,0); CREATE TABLE IF NOT EXISTS social_bots(owner TEXT PRIMARY KEY,revision INTEGER NOT NULL,sequence INTEGER NOT NULL,mask INTEGER NOT NULL,context_sequence INTEGER NOT NULL,context_mask INTEGER NOT NULL,action_world TEXT NOT NULL,action_ordinal INTEGER NOT NULL,character TEXT NOT NULL,summary TEXT NOT NULL,context_goal TEXT NOT NULL); CREATE TABLE IF NOT EXISTS social_relationships(owner TEXT NOT NULL,other TEXT NOT NULL,familiarity INTEGER NOT NULL,affection INTEGER NOT NULL,PRIMARY KEY(owner,other)); CREATE TABLE IF NOT EXISTS social_memories(owner TEXT NOT NULL,event_id TEXT NOT NULL,sequence INTEGER NOT NULL,kind TEXT NOT NULL,outcome TEXT NOT NULL,participants TEXT NOT NULL,at_ms INTEGER NOT NULL,shared INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(owner,event_id)); CREATE TABLE IF NOT EXISTS social_scenes(id TEXT PRIMARY KEY,sequence INTEGER NOT NULL,actor_a TEXT NOT NULL,actor_b TEXT NOT NULL,terminal INTEGER NOT NULL,view TEXT NOT NULL);").map_err(err)?;
    let versions: Vec<i64> = {
        let mut s = tx
            .prepare("SELECT version FROM social_schema LIMIT 2")
            .map_err(err)?;
        let rows = s.query_map([], |r| r.get(0)).map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)?
    };
    if versions != vec![1] {
        return Err("unsupported social schema".into());
    }
    tx.commit().map_err(err)
}
fn owner(db: &Connection, fixed: Option<&str>, id: &str) -> Result<(), String> {
    if !valid_id(id)
        || fixed.is_some_and(|scope| scope != id)
        || !db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bot_profiles WHERE id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(err)?
    {
        return Err("social owner unavailable".into());
    }
    Ok(())
}
fn encode<T: Serialize>(value: &T) -> Result<String, String> {
    let s = serde_json::to_string(value).map_err(err)?;
    if s.len() > MAX_JSON {
        return Err("social data capacity".into());
    }
    Ok(s)
}
fn bot(db: &Connection, id: &str) -> Result<Bot, String> {
    let row=db.query_row("SELECT revision,sequence,mask,context_sequence,context_mask,CASE WHEN length(CAST(action_world AS BLOB))<=96 THEN action_world END,action_ordinal,CASE WHEN length(CAST(character AS BLOB))<=32768 THEN character END,CASE WHEN length(CAST(summary AS BLOB))<=1024 THEN summary END,CASE WHEN length(CAST(context_goal AS BLOB))<=128 THEN context_goal END FROM social_bots WHERE owner=?1",[id],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,u8>(2)?,r.get::<_,u64>(3)?,r.get::<_,u8>(4)?,r.get::<_,String>(5)?,r.get::<_,u64>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?))).optional().map_err(err)?;
    if let Some((
        revision,
        sequence,
        mask,
        context_sequence,
        context_mask,
        action_world,
        action_ordinal,
        character,
        summary,
        context_goal,
    )) = row
    {
        let character: Character = serde_json::from_str(&character).map_err(err)?;
        character.validate()?;
        let summary: Summary = serde_json::from_str(&summary).map_err(err)?;
        if [revision, sequence, context_sequence, action_ordinal]
            .iter()
            .any(|v| *v > MAX_COUNTER)
            || mask > 15
            || context_mask > 15
            || action_world.len() > 96
        {
            return Err(FAIL.into());
        }
        Ok(Bot {
            revision,
            sequence,
            mask,
            context_sequence,
            context_mask,
            action_world,
            action_ordinal,
            character,
            summary,
            context_goal,
        })
    } else {
        Ok(Bot {
            revision: 0,
            sequence: 0,
            mask: 0,
            context_sequence: 0,
            context_mask: 0,
            action_world: String::new(),
            action_ordinal: 0,
            character: Character::default(),
            summary: Summary::default(),
            context_goal: String::new(),
        })
    }
}
fn save_bot(tx: &Transaction, id: &str, bot: &Bot) -> Result<(), String> {
    if bot.revision > MAX_COUNTER {
        return Err("social revision exhausted".into());
    }
    tx.execute("INSERT INTO social_bots VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(owner) DO UPDATE SET revision=excluded.revision,sequence=excluded.sequence,mask=excluded.mask,context_sequence=excluded.context_sequence,context_mask=excluded.context_mask,action_world=excluded.action_world,action_ordinal=excluded.action_ordinal,character=excluded.character,summary=excluded.summary,context_goal=excluded.context_goal",params![id,bot.revision,bot.sequence,bot.mask,bot.context_sequence,bot.context_mask,bot.action_world,bot.action_ordinal,encode(&bot.character)?,encode(&bot.summary)?,bot.context_goal]).map_err(err)?;
    Ok(())
}
fn bump(tx: &Transaction) -> Result<(), String> {
    if tx
        .execute(
            "UPDATE social_meta SET epoch=epoch+1 WHERE id=1 AND epoch<?1",
            [MAX_COUNTER],
        )
        .map_err(err)?
        != 1
    {
        return Err("social epoch exhausted".into());
    }
    Ok(())
}
fn bit(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Invited => 1,
        Outcome::Declined => 2,
        Outcome::Cancelled => 4,
        Outcome::Completed => 8,
    }
}
fn apply(tx: &Transaction, event: &SocialEvent) -> Result<bool, String> {
    let allocated: u64 = tx
        .query_row("SELECT sequence FROM social_meta WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    if event.sequence() == 0 || event.sequence() > allocated {
        return Err("unallocated social event".into());
    }
    let mut pending = false;
    for id in event.participants() {
        owner(tx, None, id)?;
        let state = bot(tx, id)?;
        if event.sequence() > state.sequence
            || event.sequence() == state.sequence
                && state.mask & bit(event.outcome()) == 0
                && state.mask & 14 == 0
        {
            pending = true;
        }
    }
    if !pending {
        return Ok(false);
    }
    if event.outcome() == Outcome::Completed {
        let actions = event.actions().ok_or("missing completed scene actions")?;
        let sources = event
            .source_events()
            .ok_or("missing completed scene sources")?;
        for (index, id) in event.participants().iter().enumerate() {
            let actual = super::life_store::read(tx, id)
                .map_err(err)?
                .ok_or("missing confirmed actor")?;
            let action = actual
                .state()
                .current_action()
                .ok_or("missing confirmed action")?;
            if action.phase() != crate::life_domain::ActionPhase::Completed
                || action.intent().id() != &actions[index]
                || !actual.state().recent_events().any(|record| record.id() == &sources[index]
                    && matches!(record.event(), crate::life_domain::CommittedEvent::Completed { action_id } if action_id == &actions[index])) {
                return Err("social completion not confirmed in Store".into());
            }
        }
    }
    let mut changed = false;
    for (i, id) in event.participants().iter().enumerate() {
        owner(tx, None, id)?;
        let mut b = bot(tx, id)?;
        let mask = bit(event.outcome());
        if event.sequence() < b.sequence
            || event.sequence() == b.sequence && (b.mask & mask != 0 || b.mask & 14 != 0)
        {
            continue;
        }
        if event.sequence() > b.sequence {
            b.sequence = event.sequence();
            b.mask = 0;
        }
        b.mask |= mask;
        b.revision = b
            .revision
            .checked_add(1)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or("social revision exhausted")?;
        let other = &event.participants()[1 - i];
        let previous=tx.query_row("SELECT familiarity,affection FROM social_relationships WHERE owner=?1 AND other=?2",params![id,other],|r|Ok((r.get::<_,u16>(0)?,r.get::<_,i16>(1)?))).optional().map_err(err)?.unwrap_or((0,0));
        if previous.0 > 10000 || !(-10000..=10000).contains(&previous.1) {
            return Err(FAIL.into());
        }
        let count: u64 = tx
            .query_row(
                "SELECT count(*) FROM social_relationships WHERE owner=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(err)?;
        if count >= 64
            && !tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM social_relationships WHERE owner=?1 AND other=?2)",
                    params![id, other],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(err)?
        {
            return Err("relationship capacity".into());
        }
        let (familiarity, affection) = match event.outcome() {
            Outcome::Completed => (50, 5 + i32::from(b.character.traits.sociability) / 50),
            Outcome::Declined => (
                1,
                -(5 - i32::from(b.character.traits.patience) / 250).max(1),
            ),
            _ => (1, 0),
        };
        tx.execute("INSERT INTO social_relationships VALUES(?1,?2,?3,?4) ON CONFLICT(owner,other) DO UPDATE SET familiarity=excluded.familiarity,affection=excluded.affection",params![id,other,previous.0.saturating_add(familiarity).min(10000),(i32::from(previous.1)+affection).clamp(-10000,10000)]).map_err(err)?;
        match event.outcome() {
            Outcome::Completed => {
                b.summary.completed = b.summary.completed.saturating_add(1).min(1000000);
                let action = event
                    .actions()
                    .and_then(|actions| {
                        actions
                            .iter()
                            .find(|action| action.actor_id().as_str() == id)
                    })
                    .ok_or("missing completed scene action")?;
                if !b.action_world.is_empty() && b.action_world != action.world_id().as_str() {
                    return Err("character lineage changed".into());
                }
                if action.ordinal() > b.action_ordinal {
                    b.action_world = action.world_id().as_str().into();
                    b.action_ordinal = action.ordinal();
                    b.character.credit(if event.kind() == SceneKind::Play {
                        HobbyKind::Play
                    } else {
                        HobbyKind::Company
                    });
                }
            }
            Outcome::Cancelled => {
                b.summary.cancelled = b.summary.cancelled.saturating_add(1).min(1000000)
            }
            Outcome::Declined => {
                b.summary.declined = b.summary.declined.saturating_add(1).min(1000000)
            }
            _ => (),
        }
        tx.execute("INSERT INTO social_memories(owner,event_id,sequence,kind,outcome,participants,at_ms,shared) VALUES(?1,?2,?3,?4,?5,?6,?7,0)",params![id,event.id(),event.sequence(),encode(&event.kind())?,encode(&event.outcome())?,encode(event.participants())?,event.at_ms()]).map_err(err)?;
        tx.execute("DELETE FROM social_memories WHERE owner=?1 AND event_id NOT IN(SELECT event_id FROM social_memories WHERE owner=?1 ORDER BY sequence DESC,at_ms DESC,event_id DESC LIMIT 128)",[id]).map_err(err)?;
        save_bot(tx, id, &b)?;
        changed = true;
    }
    if changed {
        bump(tx)?;
    }
    Ok(changed)
}
fn memories(db: &Connection, id: &str, shared: bool) -> Result<Vec<Memory>, String> {
    let sql = if shared {
        "SELECT CASE WHEN length(CAST(event_id AS BLOB))<=96 THEN event_id END,sequence,CASE WHEN length(CAST(kind AS BLOB))<=32 THEN kind END,CASE WHEN length(CAST(outcome AS BLOB))<=32 THEN outcome END,CASE WHEN length(CAST(participants AS BLOB))<=256 THEN participants END,at_ms,shared,CASE WHEN length(CAST(owner AS BLOB))<=96 THEN owner END FROM social_memories WHERE owner=?1 OR shared=1 ORDER BY sequence DESC,at_ms DESC LIMIT 16"
    } else {
        "SELECT CASE WHEN length(CAST(event_id AS BLOB))<=96 THEN event_id END,sequence,CASE WHEN length(CAST(kind AS BLOB))<=32 THEN kind END,CASE WHEN length(CAST(outcome AS BLOB))<=32 THEN outcome END,CASE WHEN length(CAST(participants AS BLOB))<=256 THEN participants END,at_ms,shared,CASE WHEN length(CAST(owner AS BLOB))<=96 THEN owner END FROM social_memories WHERE owner=?1 ORDER BY sequence DESC,at_ms DESC LIMIT 128"
    };
    let mut s = db.prepare(sql).map_err(err)?;
    let rows = s
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, u64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(err)?;
    let mut result = vec![];
    for row in rows {
        let (event_id, sequence, kind, outcome, participants, at_ms, shared, source) =
            row.map_err(err)?;
        if !matches!(shared, 0 | 1)
            || event_id.len() > 96
            || kind.len() > 32
            || outcome.len() > 32
            || participants.len() > 256
            || sequence > MAX_COUNTER
            || at_ms > MAX_COUNTER
        {
            return Err(FAIL.into());
        }
        let participants: [String; 2] = serde_json::from_str(&participants).map_err(err)?;
        if participants.iter().any(|p| !valid_id(p)) || !participants.iter().any(|p| p == &source) {
            return Err(FAIL.into());
        }
        result.push(Memory {
            event_id,
            sequence,
            kind: serde_json::from_str(&kind).map_err(err)?,
            outcome: serde_json::from_str(&outcome).map_err(err)?,
            participants,
            at_ms,
            origin: if source == id { "own" } else { "shared" }.into(),
            shared: shared == 1,
        });
    }
    Ok(result)
}
impl Store {
    fn social_ready(&self) -> Result<(), String> {
        initialize(&mut self.db.lock().map_err(err)?)
    }
    pub(crate) fn social_next_scene_sequence(&self, actors: &[String; 2]) -> Result<u64, String> {
        self.social_ready()?;
        if self.fixed_profile.is_some() {
            return Err("paired social write requires native owner".into());
        }
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        for id in actors {
            owner(&tx, None, id)?;
        }
        let seq: u64 = tx
            .query_row("SELECT sequence FROM social_meta WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(err)?;
        let next = seq
            .checked_add(1)
            .filter(|v| *v <= MAX_COUNTER)
            .ok_or("social sequence exhausted")?;
        tx.execute("UPDATE social_meta SET sequence=?1 WHERE id=1", [next])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(next)
    }
    pub(crate) fn social_apply_event(&self, event: &SocialEvent) -> Result<bool, String> {
        self.social_ready()?;
        if self.fixed_profile.is_some() {
            return Err("paired social write requires native owner".into());
        }
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let changed = apply(&tx, event)?;
        tx.commit().map_err(err)?;
        Ok(changed)
    }
    pub(crate) fn social_save_scene(&self, scene: &SceneView) -> Result<(), String> {
        scene.validate()?;
        self.social_ready()?;
        if self.fixed_profile.is_some() {
            return Err("paired social write requires native owner".into());
        }
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        for id in &scene.participants {
            owner(&tx, None, id)?;
        }
        let count: u64 = tx
            .query_row(
                "SELECT count(*) FROM social_scenes WHERE terminal=0",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        if count >= 32
            && !tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM social_scenes WHERE id=?1)",
                    [&scene.id],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(err)?
        {
            return Err("social scene capacity".into());
        }
        tx.execute("INSERT INTO social_scenes VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET terminal=excluded.terminal,view=excluded.view",params![scene.id,scene.sequence,scene.participants[0],scene.participants[1],scene.phase.terminal(),encode(scene)?]).map_err(err)?;
        tx.execute("DELETE FROM social_scenes WHERE terminal=1 AND id NOT IN(SELECT id FROM social_scenes ORDER BY sequence DESC LIMIT 64)",[]).map_err(err)?;
        tx.commit().map_err(err)
    }
    /// Once on application startup, before any explicit Life start. No session/lease is restored.
    pub(crate) fn social_recover(&self, now: u64) -> Result<(), String> {
        if now > MAX_COUNTER {
            return Err(FAIL.into());
        }
        self.social_ready()?;
        if self.fixed_profile.is_some() {
            return Err(FAIL.into());
        }
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let scenes: Vec<String> = {
            let mut s=tx.prepare("SELECT CASE WHEN length(CAST(view AS BLOB))<=32768 THEN view END FROM social_scenes WHERE terminal=0 LIMIT 33").map_err(err)?;
            let rows = s.query_map([], |r| r.get(0)).map_err(err)?;
            rows.collect::<Result<_, _>>().map_err(err)?
        };
        if scenes.len() > 32 {
            return Err(FAIL.into());
        }
        for text in scenes {
            let mut scene: SceneView = serde_json::from_str(&text).map_err(err)?;
            scene.validate()?;
            let event = SocialEvent::interrupted(&scene, now);
            apply(&tx, &event)?;
            scene.phase = Phase::Cancelled;
            scene.reason = Some(Reason::Restart);
            tx.execute(
                "UPDATE social_scenes SET terminal=1,view=?2 WHERE id=?1",
                params![scene.id, encode(&scene)?],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)
    }
    pub(crate) fn social_snapshot(&self, id: &str) -> Result<SocialSnapshot, String> {
        self.social_ready()?;
        let db = self.db.lock().map_err(err)?;
        owner(&db, self.fixed_profile.as_deref(), id)?;
        let b = bot(&db, id)?;
        let mut s=db.prepare("SELECT other,familiarity,affection FROM social_relationships WHERE owner=?1 ORDER BY other LIMIT 65").map_err(err)?;
        let rows = s
            .query_map([id], |r| {
                Ok(Relationship {
                    other_id: r.get(0)?,
                    familiarity: r.get(1)?,
                    affection: r.get(2)?,
                })
            })
            .map_err(err)?;
        let relationships: Vec<_> = rows.collect::<Result<_, _>>().map_err(err)?;
        if relationships.len() > 64
            || relationships.iter().any(|r| {
                !valid_id(&r.other_id)
                    || r.familiarity > 10000
                    || !(-10000..=10000).contains(&r.affection)
            })
        {
            return Err(FAIL.into());
        }
        let mut s=db.prepare("SELECT CASE WHEN length(CAST(view AS BLOB))<=32768 THEN view END FROM social_scenes WHERE actor_a=?1 OR actor_b=?1 ORDER BY sequence DESC LIMIT 32").map_err(err)?;
        let rows = s.query_map([id], |r| r.get::<_, String>(0)).map_err(err)?;
        let mut scenes = vec![];
        for row in rows {
            let scene: SceneView = serde_json::from_str(&row.map_err(err)?).map_err(err)?;
            scene.validate()?;
            scenes.push(scene);
        }
        Ok(SocialSnapshot {
            version: 1,
            bot_id: id.into(),
            revision: b.revision,
            relationships,
            memories: memories(&db, id, false)?,
            summary: b.summary,
            development: b.character,
            scenes,
        })
    }
    pub(crate) fn social_set_development(
        &self,
        id: &str,
        expected: u64,
        requested: Character,
    ) -> Result<SocialSnapshot, String> {
        self.social_ready()?;
        {
            let mut db = self.db.lock().map_err(err)?;
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(err)?;
            owner(&tx, self.fixed_profile.as_deref(), id)?;
            let mut b = bot(&tx, id)?;
            if b.revision != expected {
                return Err("social data changed".into());
            }
            b.character.edit(requested)?;
            b.revision = b
                .revision
                .checked_add(1)
                .filter(|n| *n <= MAX_COUNTER)
                .ok_or(FAIL)?;
            save_bot(&tx, id, &b)?;
            bump(&tx)?;
            tx.commit().map_err(err)?;
        }
        self.social_snapshot(id)
    }
    pub(crate) fn social_memory_change(
        &self,
        id: &str,
        event: Option<&str>,
        shared: Option<bool>,
    ) -> Result<(), String> {
        self.social_ready()?;
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        owner(&tx, self.fixed_profile.as_deref(), id)?;
        let mut b = bot(&tx, id)?;
        if let Some(event) = event {
            if event.len() > 128 || !valid_id(event) {
                return Err(FAIL.into());
            }
            if let Some(shared) = shared {
                tx.execute(
                    "UPDATE social_memories SET shared=?3 WHERE owner=?1 AND event_id=?2",
                    params![id, event, shared],
                )
                .map_err(err)?;
            } else {
                tx.execute(
                    "DELETE FROM social_memories WHERE owner=?1 AND event_id=?2",
                    params![id, event],
                )
                .map_err(err)?;
            }
        } else {
            if shared.is_some() {
                return Err(FAIL.into());
            }
            tx.execute("DELETE FROM social_memories WHERE owner=?1", [id])
                .map_err(err)?;
            b.summary = Summary::default();
        }
        b.revision = b
            .revision
            .checked_add(1)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or(FAIL)?;
        save_bot(&tx, id, &b)?;
        bump(&tx)?;
        tx.commit().map_err(err)
    }
    pub(crate) fn social_reset_development(&self, id: &str) -> Result<(), String> {
        self.social_ready()?;
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        owner(&tx, self.fixed_profile.as_deref(), id)?;
        let mut b = bot(&tx, id)?;
        b.character.reset_development();
        b.revision = b
            .revision
            .checked_add(1)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or(FAIL)?;
        save_bot(&tx, id, &b)?;
        bump(&tx)?;
        tx.commit().map_err(err)
    }
    pub(crate) fn social_reconcile_goal(
        &self,
        id: &str,
        available: &[HobbyKind],
    ) -> Result<(), String> {
        if available.len() > 3 {
            return Err(FAIL.into());
        }
        self.social_ready()?;
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        owner(&tx, self.fixed_profile.as_deref(), id)?;
        let mut state = bot(&tx, id)?;
        let before = state.character.goal.clone();
        state.character.reconcile_goal(available);
        if state.character.goal != before {
            state.revision = state
                .revision
                .checked_add(1)
                .filter(|v| *v <= MAX_COUNTER)
                .ok_or(FAIL)?;
            save_bot(&tx, id, &state)?;
            bump(&tx)?;
        }
        tx.commit().map_err(err)
    }
    pub(crate) fn social_epoch(&self) -> Result<u64, String> {
        self.social_ready()?;
        self.db
            .lock()
            .map_err(err)?
            .query_row("SELECT epoch FROM social_meta WHERE id=1", [], |r| r.get(0))
            .map_err(err)
    }
    /// One-use event retrieval claim, not a provider admission or permission.
    pub(crate) fn social_take_context(
        &self,
        id: &str,
        now: u64,
    ) -> Result<Option<crate::autonomy_context::AutonomyContext>, String> {
        self.social_ready()?;
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        owner(&tx, self.fixed_profile.as_deref(), id)?;
        let mut b = bot(&tx, id)?;
        let found = memories(&tx, id, true)?.into_iter().find(|m| {
            m.at_ms <= now
                && now - m.at_ms <= 300000
                && (m.sequence > b.context_sequence
                    || m.sequence == b.context_sequence && b.context_mask & bit(m.outcome) == 0)
        });
        let epoch = tx
            .query_row("SELECT epoch FROM social_meta WHERE id=1", [], |r| r.get(0))
            .map_err(err)?;
        let context = if let Some(memory) = found {
            if memory.sequence > b.context_sequence {
                b.context_sequence = memory.sequence;
                b.context_mask = 0;
            }
            b.context_mask |= bit(memory.outcome);
            crate::autonomy_context::AutonomyContext::new(
                id,
                &memory,
                b.character.goal.as_ref(),
                epoch,
            )?
        } else if let Some(goal) = b
            .character
            .goal
            .as_ref()
            .filter(|goal| goal.status == crate::character_development::GoalStatus::Active)
        {
            let key = format!("{}_{}", goal.id, goal.progress);
            if b.context_goal == key {
                return Ok(None);
            }
            let context = crate::autonomy_context::AutonomyContext::from_goal(id, goal, epoch)?;
            b.context_goal = key;
            context
        } else {
            return Ok(None);
        };
        save_bot(&tx, id, &b)?;
        tx.commit().map_err(err)?;
        Ok(Some(context))
    }
    pub(crate) fn social_credit_activity(
        &self,
        id: &str,
        confirmed: &crate::life_domain::LifeState,
    ) -> Result<bool, String> {
        self.social_ready()?;
        let action = confirmed
            .current_action()
            .filter(|a| a.phase() == crate::life_domain::ActionPhase::Completed)
            .ok_or("activity not completed")?;
        if confirmed.actor_id().as_str() != id {
            return Err(FAIL.into());
        }
        let hobby = match serde_json::to_value(action.intent().activity())
            .map_err(err)?
            .as_str()
        {
            Some("play") => HobbyKind::Play,
            Some("move_to") => HobbyKind::Explore,
            Some("socialize") | Some("wave") => HobbyKind::Company,
            _ => return Ok(false),
        };
        let mut db = self.db.lock().map_err(err)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        owner(&tx, self.fixed_profile.as_deref(), id)?;
        let actual = super::life_store::read(&tx, id)
            .map_err(err)?
            .ok_or("missing confirmed actor")?;
        let current = actual
            .state()
            .current_action()
            .ok_or("missing confirmed action")?;
        if current.phase() != crate::life_domain::ActionPhase::Completed
            || current.intent().id() != action.intent().id()
            || current.intent().activity() != action.intent().activity()
        {
            return Err("development activity not confirmed in Store".into());
        }
        let mut b = bot(&tx, id)?;
        let world = confirmed.world_id().as_str();
        if !b.action_world.is_empty() && b.action_world != world {
            return Err("character lineage changed".into());
        }
        if action.intent().id().ordinal() <= b.action_ordinal {
            return Ok(false);
        }
        b.action_world = world.into();
        b.action_ordinal = action.intent().id().ordinal();
        b.character.credit(hobby);
        b.revision = b
            .revision
            .checked_add(1)
            .filter(|n| *n <= MAX_COUNTER)
            .ok_or(FAIL)?;
        save_bot(&tx, id, &b)?;
        bump(&tx)?;
        tx.commit().map_err(err)?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "social_store/tests.rs"]
mod tests;
