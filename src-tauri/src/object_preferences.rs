//! Private per-bot learned object affinities. References are data, never leases.
use super::{valid_id, Store};
use crate::{content_packs::ContentRef, life_domain::{ActionPhase, Activity, CommittedEvent, MAX_COUNTER},
    life_runtime::ObjectUseWitness, storage::life_store::{EventOrigin, LifeSnapshot},
    world_hub::ActionSlotTarget, world_state::AnchorKind};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
const ERROR: &str = "social object preferences unavailable";
const MAX_BYTES: usize = 65536;
pub(crate) const MAX_OBJECT_PREFERENCES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub(crate) struct ObjectReference {
    pub stage_id: String,
    pub object_id: String,
    pub kind: AnchorKind,
    pub content_ref: Option<ContentRef>,
}
impl ObjectReference {
    pub(crate) fn from_slot(stage: &str, slot: &ActionSlotTarget) -> Self {
        Self { stage_id:stage.into(), object_id:slot.anchor_id.as_str().into(), kind:slot.kind, content_ref:slot.content_ref.clone() }
    }
    fn validate(&self) -> Result<(),String> {
        if !valid_id(&self.stage_id) || !valid_id(&self.object_id) { return Err(ERROR.into()); }
        if let Some(reference)=&self.content_ref { reference.validate().map_err(|_|ERROR)?; }
        Ok(())
    }
    fn key(&self) -> String { serde_json::to_string(self).unwrap_or_default() }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub(crate) struct ObjectAffinity {
    pub object: ObjectReference,
    pub affinity: u16,
    pub successful_uses: u16,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
struct Preferences {
    version: u8,
    action_world: String,
    action_ordinal: u64,
    entries: Vec<ObjectAffinity>,
}
impl Default for Preferences {
    fn default()->Self { Self{version:1,action_world:String::new(),action_ordinal:0,entries:vec![]} }
}
fn key(bot:&str)->String {format!("social-object-preferences-v1:{bot}")}
fn failure(_:impl std::fmt::Debug)->String {ERROR.into()}
fn read(db:&Connection,bot:&str)->Result<Preferences,String>{
    let raw:Option<String>=db.query_row("SELECT CASE WHEN typeof(value)='text' AND length(CAST(value AS BLOB)) BETWEEN 1 AND ?2 THEN value END FROM settings WHERE key=?1",
        params![key(bot),MAX_BYTES as i64],|row|row.get(0)).optional().map_err(failure)?;
    let state:Preferences=raw.map(|raw|serde_json::from_str(&raw).map_err(failure)).transpose()?.unwrap_or_default();
    if state.version!=1 || state.action_ordinal>MAX_COUNTER
        || !(state.action_world.is_empty() && state.action_ordinal==0 || valid_id(&state.action_world) && state.action_ordinal>0)
        || state.entries.len()>MAX_OBJECT_PREFERENCES {return Err(ERROR.into());}
    let mut seen=std::collections::BTreeSet::new();
    for entry in &state.entries {
        entry.object.validate()?;
        if entry.affinity>1000 || entry.successful_uses==0 || entry.successful_uses>10000
            || !seen.insert(entry.object.key()) {return Err(ERROR.into());}
    }
    Ok(state)
}
fn write(db:&Connection,bot:&str,state:&Preferences)->Result<(),String>{
    let raw=serde_json::to_string(state).map_err(failure)?;
    if raw.len()>MAX_BYTES {return Err(ERROR.into());}
    db.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key(bot),raw]).map_err(failure)?;
    Ok(())
}
pub(super) fn entries(db:&Connection,bot:&str)->Result<Vec<ObjectAffinity>,String>{ Ok(read(db,bot)?.entries) }
/// Planner data is read only from the owner's validated projection. A missing,
/// moved or replaced object cannot confer any action or reservation authority.
pub(crate) fn affinity(entries:&[ObjectAffinity],stage:&str,slot:&ActionSlotTarget)->u16{
    let reference=ObjectReference::from_slot(stage,slot);
    entries.iter().find(|entry|entry.object==reference).map_or(0,|entry|entry.affinity)
}
impl Store {
    pub(crate) fn social_object_preferences(&self,bot:&str)->Result<Vec<ObjectAffinity>,String>{
        self.social_ready()?;
        let db=self.db.lock().map_err(failure)?;
        super::social_store::owner(&db,self.fixed_profile.as_deref(),bot)?;
        Ok(read(&db,bot)?.entries)
    }
    /// Only the actual retained native job can construct this non-IPC witness.
    /// The immutable completion source is revalidated in the same credit TX.
    pub(crate) fn social_credit_object_use(&self,bot:&str,snapshot:&LifeSnapshot,witness:&ObjectUseWitness)->Result<bool,String>{
        self.social_ready()?;
        let source=snapshot.state();
        let action=source.current_action().filter(|action|action.phase()==ActionPhase::Completed
            && action.intent().id()==witness.action()).ok_or(ERROR)?;
        if source.actor_id().as_str()!=bot || !matches!(action.intent().activity(),
            Activity::Rest|Activity::Recharge|Activity::Snack|Activity::Play|Activity::Socialize) {return Err(ERROR.into());}
        let reference=witness.object();reference.validate()?;
        if !matches!((action.intent().activity(),reference.kind),
            (Activity::Rest|Activity::Recharge,AnchorKind::Rest)|(Activity::Snack,AnchorKind::Snack)
            |(Activity::Play,AnchorKind::Play)|(Activity::Socialize,AnchorKind::Meeting)) {return Err(ERROR.into());}
        let mut db=self.db.lock().map_err(failure)?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(failure)?;
        super::social_store::owner(&tx,self.fixed_profile.as_deref(),bot)?;
        let actual=super::life_store::read(&tx,bot).map_err(failure)?.ok_or(ERROR)?;
        if actual.state().actor_id()!=source.actor_id() || actual.state().world_id()!=source.world_id()
            || actual.state().revision()<source.revision() || actual.saved_at_ms()<snapshot.saved_at_ms()
            || !snapshot.events().iter().any(|event|event.origin()==EventOrigin::Live
                && source.recent_events().any(|record|record==event.record())
                && matches!(event.record().event(),CommittedEvent::Completed{action_id}if action_id==witness.action())
                && actual.events().iter().any(|retained|retained.origin()==EventOrigin::Live && retained.record()==event.record())
                && actual.state().recent_events().any(|record|record==event.record())) {return Err(ERROR.into());}
        let mut state=read(&tx,bot)?;
        let world=source.world_id().as_str();let ordinal=witness.action().ordinal();
        if state.action_world==world && ordinal<=state.action_ordinal {return Ok(false);}
        state.action_world=world.into();state.action_ordinal=ordinal;
        if let Some(entry)=state.entries.iter_mut().find(|entry|&entry.object==reference){
            entry.affinity=entry.affinity.saturating_add(25).min(1000);
            entry.successful_uses=entry.successful_uses.saturating_add(1).min(10000);
        }else{
            if state.entries.len()==MAX_OBJECT_PREFERENCES {
                let remove=state.entries.iter().enumerate().min_by_key(|(_,entry)|(entry.affinity,entry.object.key())).map(|(index,_)|index).ok_or(ERROR)?;
                state.entries.remove(remove);
            }
            state.entries.push(ObjectAffinity{object:reference.clone(),affinity:25,successful_uses:1});
        }
        state.entries.sort_by_key(|entry|entry.object.key());
        write(&tx,bot,&state)?;
        super::social_store::bump(&tx)?;
        tx.commit().map_err(failure)?;Ok(true)
    }
    pub(crate) fn social_reset_object_preferences(&self,bot:&str)->Result<(),String>{
        self.social_ready()?;
        let mut db=self.db.lock().map_err(failure)?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(failure)?;
        super::social_store::owner(&tx,self.fixed_profile.as_deref(),bot)?;
        let mut state=read(&tx,bot).unwrap_or_default();
        // Every explicit reset includes even a not-yet-credited retained job.
        // A validated current Life snapshot supplies a conservative barrier;
        // resetting optional preferences never creates an experienced event.
        if let Some(actual)=super::life_store::read(&tx,bot).map_err(failure)? {
            if actual.state().actor_id().as_str()!=bot {return Err(ERROR.into());}
            if let Some(action)=actual.state().current_action(){
                let world=actual.state().world_id().as_str();let ordinal=action.intent().id().ordinal();
                if state.action_world!=world || state.action_ordinal<ordinal {
                    state.action_world=world.into();state.action_ordinal=ordinal;
                }
            }
        }
        // Keep replay cursor: clearing a preference is never a new experience.
        state.entries.clear();write(&tx,bot,&state)?;
        super::social_store::bump(&tx)?;
        tx.commit().map_err(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{life_domain::{ActorId,LifeEvent,NeedEffect,Needs,RandomState,WorldId},world_state::{AnchorId,SlotId}};
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new()->Self{let path=std::env::temp_dir().join(format!("orbitpal-object-prefs-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&path).unwrap();Self(path)}
        fn store(&self)->Store{Store::new(self.0.clone()).unwrap()}
    }
    impl Drop for Fixture{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    fn reference(id:&str)->ObjectReference{ObjectReference{stage_id:"stage".into(),object_id:id.into(),kind:AnchorKind::Play,content_ref:None}}
    fn complete(store:&Store,activity:Activity)->LifeSnapshot{
        if store.load_life("nova").unwrap().is_none(){store.initialize_life("nova",WorldId::new("object-world").unwrap(),Needs::new(5000,5000,5000,5000).unwrap(),RandomState::seeded(1),1000).unwrap();}
        let old=store.load_life("nova").unwrap().unwrap();let now=old.saved_at_ms()+10;
        let plan=store.apply_life_event("nova",old.state().revision(),0,LifeEvent::Plan{activity,completion_effect:NeedEffect::default()},now).unwrap().snapshot;
        let action=plan.state().current_action().unwrap().intent().id().clone();
        let started=store.apply_life_event("nova",plan.state().revision(),0,LifeEvent::Start{action_id:action.clone()},now).unwrap().snapshot;
        store.apply_life_event("nova",started.state().revision(),100,LifeEvent::Complete{action_id:action},now+100).unwrap().snapshot
    }
    fn witness(snapshot:&LifeSnapshot,object:ObjectReference)->ObjectUseWitness{
        ObjectUseWitness::fixture(snapshot.state().current_action().unwrap().intent().id().clone(),object)
    }
    #[test]
    fn object_affinity_persists_per_owner_and_reset_never_recredits_the_same_completed_use(){
        let fixture=Fixture::new();let store=fixture.store();
        let completed=complete(&store,Activity::Play);let proof=witness(&completed,reference("toy"));
        assert!(store.social_credit_object_use("nova",&completed,&proof).unwrap());
        assert!(!store.social_credit_object_use("nova",&completed,&proof).unwrap());
        let learned=store.social_object_preferences("nova").unwrap();
        assert_eq!(learned[0].affinity,25);assert_eq!(learned[0].successful_uses,1);
        let copied=store.create_bot("Copy",true).unwrap();let copied=copied["id"].as_str().unwrap();
        assert!(store.social_object_preferences(copied).unwrap().is_empty());
        assert!(store.scoped(copied).unwrap().social_object_preferences("nova").is_err());
        let exported=fixture.0.join("profile.orbitpal");store.export(&exported).unwrap();
        let imported=store.import(&exported).unwrap();let imported=imported["id"].as_str().unwrap();
        assert!(store.social_object_preferences(imported).unwrap().is_empty());
        drop(store);let store=fixture.store();
        assert_eq!(store.social_object_preferences("nova").unwrap(),learned);
        store.social_reset_object_preferences("nova").unwrap();
        assert!(!store.social_credit_object_use("nova",&completed,&proof).unwrap());
        assert!(store.social_object_preferences("nova").unwrap().is_empty());
        assert_eq!(store.load_life("nova").unwrap().unwrap().state().current_action().unwrap().phase(),ActionPhase::Completed);
    }
    #[test]
    fn object_reference_survives_motion_but_never_changes_kind_stage_or_package_identity(){
        let slot=ActionSlotTarget{id:SlotId::new("seat").unwrap(),anchor_id:AnchorId::new("toy").unwrap(),kind:AnchorKind::Play,
            position:crate::world_state::coordinates::Point::new(100,100).unwrap(),content_ref:None};
        let entries=vec![ObjectAffinity{object:ObjectReference::from_slot("stage",&slot),affinity:100,successful_uses:4}];
        let mut changed=slot.clone();changed.position=crate::world_state::coordinates::Point::new(900000,900000).unwrap();
        assert_eq!(affinity(&entries,"stage",&changed),100);
        assert_eq!(affinity(&entries,"other_stage",&changed),0);
        changed.kind=AnchorKind::Rest;assert_eq!(affinity(&entries,"stage",&changed),0);
        changed=slot.clone();changed.content_ref=Some(ContentRef{package_id:"garden".into(),package_version:1,template_id:"garden:toy".into()});
        assert_eq!(affinity(&entries,"stage",&changed),0);
        let packaged=vec![ObjectAffinity{object:ObjectReference::from_slot("stage",&changed),affinity:100,successful_uses:4}];
        changed.content_ref.as_mut().unwrap().package_version=2;
        assert_eq!(affinity(&packaged,"stage",&changed),0);
    }
    #[test]
    fn corrupt_optional_object_preferences_recover_without_rollback_or_replay(){
        for raw in [rusqlite::types::Value::Text("invalid".into()),rusqlite::types::Value::Text(String::new()),
            rusqlite::types::Value::Text(" ".repeat(MAX_BYTES+1)),rusqlite::types::Value::Blob(b"{}".to_vec())]{
            let fixture=Fixture::new();let store=fixture.store();let completed=complete(&store,Activity::Play);
            let proof=witness(&completed,reference("toy"));
            store.db.lock().unwrap().execute("INSERT INTO settings(key,value) VALUES(?1,?2)",params![key("nova"),raw]).unwrap();
            assert!(store.social_credit_object_use("nova",&completed,&proof).is_err());
            assert_eq!(store.load_life("nova").unwrap().unwrap().state(),completed.state());
            assert!(store.social_object_preferences("nova").is_err());
            store.social_reset_object_preferences("nova").unwrap();
            assert!(store.social_object_preferences("nova").unwrap().is_empty());
            assert!(!store.social_credit_object_use("nova",&completed,&proof).unwrap());
        }
    }
    #[test]
    fn reset_before_first_credit_and_after_a_later_uncredited_completion_keeps_a_replay_barrier(){
        let fixture=Fixture::new();let store=fixture.store();
        let first=complete(&store,Activity::Play);let first_proof=witness(&first,reference("toy"));
        store.social_reset_object_preferences("nova").unwrap();
        assert!(!store.social_credit_object_use("nova",&first,&first_proof).unwrap());
        let next=complete(&store,Activity::Play);let next_proof=witness(&next,reference("toy"));
        assert!(store.social_credit_object_use("nova",&next,&next_proof).unwrap());
        let pending=complete(&store,Activity::Play);let pending_proof=witness(&pending,reference("toy"));
        store.social_reset_object_preferences("nova").unwrap();
        assert!(!store.social_credit_object_use("nova",&pending,&pending_proof).unwrap());
        assert!(store.social_object_preferences("nova").unwrap().is_empty());
    }
    #[test]
    fn planned_cancelled_and_unretained_sources_never_create_an_object_preference(){
        let fixture=Fixture::new();let store=fixture.store();
        let before=complete(&store,Activity::Play);
        let plan=store.apply_life_event("nova",before.state().revision(),0,LifeEvent::Plan{
            activity:Activity::Play,completion_effect:NeedEffect::default()},before.saved_at_ms()+1).unwrap().snapshot;
        let proof=witness(&plan,reference("toy"));
        assert!(store.social_credit_object_use("nova",&plan,&proof).is_err());
        let cancelled=store.apply_life_event("nova",plan.state().revision(),0,LifeEvent::Cancel{
            action_id:proof.action().clone(),reason:crate::life_domain::CancelReason::Operator},plan.saved_at_ms()+1).unwrap().snapshot;
        assert!(store.social_credit_object_use("nova",&cancelled,&proof).is_err());
        // Even a genuine former completed snapshot loses credit authority when
        // its exact retained LIVE row is unavailable in the actual Store.
        store.db.lock().unwrap().execute("DELETE FROM bot_life_events WHERE bot_id=?1 AND revision=?2",
            params!["nova",before.state().revision() as i64]).unwrap();
        let proof=witness(&before,reference("toy"));
        assert!(store.social_credit_object_use("nova",&before,&proof).is_err());
        assert!(store.social_object_preferences("nova").unwrap().is_empty());
    }
    #[test]
    fn only_retained_live_successful_object_use_can_learn_and_eviction_is_bounded(){
        let fixture=Fixture::new();let store=fixture.store();
        let mut earliest=None;
        for index in 0..65{
            let completed=complete(&store,Activity::Play);let proof=witness(&completed,reference(&format!("toy_{index:02}")));
            assert!(store.social_credit_object_use("nova",&completed,&proof).unwrap());
            if earliest.is_none(){earliest=Some((completed,proof));}
        }
        let entries=store.social_object_preferences("nova").unwrap();assert_eq!(entries.len(),64);
        assert!(!entries.iter().any(|entry|entry.object.object_id=="toy_00"));
        let (old_source,old_proof)=earliest.unwrap();
        assert!(store.social_credit_object_use("nova",&old_source,&old_proof).is_err(),"lost retained LIVE history fails closed");
        let completed=complete(&store,Activity::Pet);let proof=witness(&completed,reference("pet"));
        assert!(store.social_credit_object_use("nova",&completed,&proof).is_err());
        let completed=complete(&store,Activity::MoveTo);let proof=witness(&completed,reference("move"));
        assert!(store.social_credit_object_use("nova",&completed,&proof).is_err());
        assert_eq!(store.social_object_preferences("nova").unwrap(),entries);
        let mut state=crate::life_domain::LifeState::new(ActorId::new("nova").unwrap(),WorldId::new("offline").unwrap(),Needs::new(5000,5000,5000,5000).unwrap());
        let transition=crate::life_domain::reduce(&state,0,RandomState::seeded(1),LifeEvent::Plan{activity:Activity::Play,completion_effect:NeedEffect::default()}).unwrap();state=transition.state;
        let action=state.current_action().unwrap().intent().id().clone();
        let wrong=ObjectUseWitness::fixture(action,reference("forged"));
        assert!(store.social_credit_object_use("nova",&completed,&wrong).is_err());
    }
}
