//! Loaded-page lineage and asset observations only. None of these receipts is
//! document authentication, layout, renderer readiness or an action permission.
use super::*;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Weak;

const ASSET_MS:u64=60_000;

pub(super) struct PhysicalLoad {
    id:u64,
    cancel:AtomicBool,
    finished_at:OnceLock<u64>,
    probe:Mutex<Option<Arc<AssetProbe>>>,
}
impl PhysicalLoad {
    pub(super) fn new(id:u64)->Self{Self{id,cancel:AtomicBool::new(false),finished_at:OnceLock::new(),probe:Mutex::new(None)}}
    pub(super) fn cancel(&self){
        self.cancel.store(true,Ordering::SeqCst);
        let slot=self.probe.lock().unwrap_or_else(|poison|poison.into_inner());
        if let Some(probe)=slot.as_ref(){probe.delivery.cancel.store(true,Ordering::SeqCst);}
    }
    pub(super) fn finished(&self,now:u64)->Result<(),DocumentError>{
        if self.cancel.load(Ordering::SeqCst){return Err(DocumentError::Stale);}
        if let Some(first)=self.finished_at.get(){if now<*first{return Err(DocumentError::Expired);}}
        else{self.finished_at.set(now).map_err(|_|DocumentError::Unavailable)?;}
        Ok(())
    }
    fn slot(&self)->Result<MutexGuard<'_,Option<Arc<AssetProbe>>>,DocumentError>{
        self.probe.lock().map_err(|poison|{
            self.cancel.store(true,Ordering::SeqCst);
            if let Some(probe)=poison.into_inner().as_ref(){probe.delivery.cancel.store(true,Ordering::SeqCst);}
            DocumentError::Unavailable
        })
    }
}

#[derive(Clone)]
pub(crate) struct LoadedDocumentReceipt {bot:String,signals:Arc<WindowSignals>,load:Weak<PhysicalLoad>,load_id:u64}
impl LoadedDocumentReceipt {
    pub(crate) fn bot(&self)->&str{&self.bot}
    pub(crate) fn signals(&self)->&Arc<WindowSignals>{&self.signals}
    pub(crate) fn load_id(&self)->u64{self.load_id}
    pub(crate) fn current(&self)->bool{self.load.upgrade().is_some_and(|load|!load.cancel.load(Ordering::SeqCst)&&load.finished_at.get().is_some())}
    pub(crate) fn same_loaded(&self,other:&Self)->bool{self.bot==other.bot&&Arc::ptr_eq(&self.signals,&other.signals)&&Weak::ptr_eq(&self.load,&other.load)}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq,Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum AssetReadinessStatus{Loading,Ready,Failed}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct AssetReadinessReport {
    pub version:u16,
    pub probe:String,
    pub mount_id:String,
    pub status:AssetReadinessStatus,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum AssetProbeState{AwaitingMount,Loading,Ready,Failed}
#[derive(Clone,Copy,PartialEq,Eq)]
enum ProbePhase{Waiting,Loading,Ready,Consumed,Transferred,Failed}
struct Observation {phase:ProbePhase,mount:Option<String>,deadline:u64,last:u64}
struct AssetProbe {
    token:String,
    loaded:LoadedDocumentReceipt,
    avatar_binding:Value,
    delivery:Arc<Delivery>,
    issued:u64,
    state:Mutex<Observation>,
}
impl AssetProbe {
    fn lock(&self)->Result<MutexGuard<'_,Observation>,DocumentError>{self.state.lock().map_err(|_|{self.delivery.cancel.store(true,Ordering::SeqCst);DocumentError::Unavailable})}
    fn valid_at(&self,state:&Observation,now:u64)->bool{
        self.loaded.current()&&self.delivery.bound_current()&&now>=state.last&&now>=self.issued&&now<state.deadline
            &&!matches!(state.phase,ProbePhase::Transferred|ProbePhase::Failed)
    }
    fn status(&self,state:&Observation)->Result<AssetProbeState,DocumentError>{match state.phase{
        ProbePhase::Waiting=>Ok(AssetProbeState::AwaitingMount),ProbePhase::Loading=>Ok(AssetProbeState::Loading),
        ProbePhase::Ready=>Ok(AssetProbeState::Ready),ProbePhase::Failed=>Ok(AssetProbeState::Failed),
        ProbePhase::Consumed|ProbePhase::Transferred=>Err(DocumentError::Stale),
    }}
    fn check(&self,state:&mut Observation,now:u64)->Result<(),DocumentError>{
        // Old observations/receipt drops must never revoke the delivery after
        // it was transferred into the operational Document's ownership.
        if matches!(state.phase,ProbePhase::Consumed|ProbePhase::Transferred){return Err(DocumentError::Stale);}
        if !self.valid_at(state,now){
            state.phase=ProbePhase::Failed;self.delivery.cancel.store(true,Ordering::SeqCst);
            return Err(if now<state.last||now<self.issued||now>=state.deadline{DocumentError::Expired}else{DocumentError::Stale});
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct AssetProbeGrant(Arc<AssetProbe>);
impl AssetProbeGrant {
    pub(crate) fn probe(&self)->&str{&self.0.token}
    pub(crate) fn issued_at(&self)->u64{self.0.issued}
    pub(crate) fn avatar_binding(&self)->&Value{&self.0.avatar_binding}
    pub(crate) fn loaded(&self)->&LoadedDocumentReceipt{&self.0.loaded}
    pub(crate) fn delivery_cancel(&self)->Arc<AtomicBool>{self.0.delivery.cancel.clone()}
    pub(crate) fn current(&self,now:u64)->bool{self.0.lock().is_ok_and(|s|self.0.valid_at(&s,now)&&!matches!(s.phase,ProbePhase::Consumed))}
}
/// Not Clone/serde: only this one consumed observation may issue an operational
/// document. Dropping it before transfer cancels its original attempt.
pub(crate) struct AssetReadyReceipt {probe:Arc<AssetProbe>,armed:bool}
impl AssetReadyReceipt {
    pub(crate) fn loaded(&self)->&LoadedDocumentReceipt{&self.probe.loaded}
    pub(crate) fn avatar_binding(&self)->&Value{&self.probe.avatar_binding}
    pub(crate) fn delivery_cancel(&self)->Arc<AtomicBool>{self.probe.delivery.cancel.clone()}
    pub(crate) fn lease_cancel(&self)->Arc<AtomicBool>{self.probe.delivery.lease.get().expect("native readiness always binds original lease").clone()}
    pub(crate) fn mount_id(&self)->Option<String>{self.probe.lock().ok().and_then(|s|s.mount.clone())}
    pub(crate) fn current(&self,now:u64)->bool{self.probe.lock().is_ok_and(|s|s.phase==ProbePhase::Consumed&&self.probe.valid_at(&s,now))}
}
impl Drop for AssetReadyReceipt{fn drop(&mut self){if self.armed{self.probe.delivery.cancel.store(true,Ordering::SeqCst);}}}

impl Documents {
    pub(crate) fn loaded(&self,bot:&str,signals:&Arc<WindowSignals>)->Result<LoadedDocumentReceipt,DocumentError>{
        let state=self.lock()?;
        let entry=state.entries.get(bot).filter(|e|Arc::ptr_eq(&e.signals,signals)).ok_or(DocumentError::Stale)?;
        let load=entry.loaded.as_ref().ok_or(DocumentError::Pending)?;
        let receipt=LoadedDocumentReceipt{bot:bot.into(),signals:signals.clone(),load:Arc::downgrade(load),load_id:load.id};
        if !receipt.current(){return Err(DocumentError::Pending);}Ok(receipt)
    }
    pub(crate) fn begin_probe(&self,loaded:&LoadedDocumentReceipt,avatar_binding:Value,cancel:Arc<AtomicBool>,lease_cancel:Arc<AtomicBool>,now:u64)->Result<AssetProbeGrant,DocumentError>{
        let deadline=now.checked_add(FIRST_REGISTER_MS).filter(|n|*n<=MAX_COUNTER).ok_or(DocumentError::Expired)?;
        if !loaded.current()||avatar_binding.get(0).and_then(Value::as_str)!=Some(loaded.bot())||cancel.load(Ordering::SeqCst)||lease_cancel.load(Ordering::SeqCst){return Err(DocumentError::Stale);}
        let state=self.lock()?;
        let entry=state.entries.get(loaded.bot()).filter(|e|Arc::ptr_eq(&e.signals,loaded.signals())&&e.loaded.as_ref().is_some_and(|load|Weak::ptr_eq(&Arc::downgrade(load),&loaded.load))).ok_or(DocumentError::Stale)?;
        if entry.loaded.as_ref().and_then(|load|load.finished_at.get()).is_none_or(|finished|now<*finished){return Err(DocumentError::Expired);}
        // The unused Finished capability and the attempt have different owners.
        // Its eventual Document::drop must not cancel a transferred attempt.
        if entry.document.as_ref().is_some_and(|d|matches!(&d.phase,Phase::Finished{delivery,..}if Arc::ptr_eq(&delivery.cancel,&cancel))){return Err(DocumentError::Invalid);}
        let load=loaded.load.upgrade().ok_or(DocumentError::Stale)?;
        let mut slot=load.slot()?;
        if let Some(old)=slot.as_ref(){
            if Arc::ptr_eq(&old.delivery.cancel,&cancel){
                if old.avatar_binding!=avatar_binding||old.delivery.lease.get().is_none_or(|lease|!Arc::ptr_eq(lease,&lease_cancel)){return Err(DocumentError::Stale);}
                let s=old.lock()?;if !old.valid_at(&s,now)||matches!(s.phase,ProbePhase::Consumed|ProbePhase::Transferred){return Err(DocumentError::Stale);}
                return Ok(AssetProbeGrant(old.clone()));
            }
            if old.delivery.bound_current(){return Err(DocumentError::Pending);}
        }
        if !loaded.current()||cancel.load(Ordering::SeqCst)||lease_cancel.load(Ordering::SeqCst){return Err(DocumentError::Stale);}
        let delivery=Arc::new(Delivery{cancel,lease:OnceLock::new()});delivery.lease.set(lease_cancel).map_err(|_|DocumentError::Unavailable)?;
        let probe=Arc::new(AssetProbe{token:capability(),loaded:loaded.clone(),avatar_binding,delivery,issued:now,state:Mutex::new(Observation{phase:ProbePhase::Waiting,mount:None,deadline,last:now})});
        *slot=Some(probe.clone());Ok(AssetProbeGrant(probe))
    }
    pub(crate) fn probe_status(&self,grant:&AssetProbeGrant,now:u64)->Result<AssetProbeState,DocumentError>{
        let state=self.lock()?;Self::matches_probe(&state,&grant.0)?;
        let mut observed=grant.0.lock()?;
        if observed.phase==ProbePhase::Failed{return Ok(AssetProbeState::Failed);}
        grant.0.check(&mut observed,now)?;grant.0.status(&observed)
    }
    fn matches_probe(state:&State,probe:&Arc<AssetProbe>)->Result<(),DocumentError>{
        let loaded=&probe.loaded;
        let entry=state.entries.get(loaded.bot()).filter(|e|Arc::ptr_eq(&e.signals,loaded.signals())&&e.loaded.as_ref().is_some_and(|load|Weak::ptr_eq(&Arc::downgrade(load),&loaded.load))).ok_or(DocumentError::Stale)?;
        let slot=entry.loaded.as_ref().ok_or(DocumentError::Stale)?.slot()?;
        if slot.as_ref().is_none_or(|current|!Arc::ptr_eq(current,probe)){return Err(DocumentError::Stale);}Ok(())
    }
    pub(crate) fn report_probe(&self,label:&str,report:&AssetReadinessReport,now:u64)->Result<AssetProbeState,DocumentError>{
        let bot=label.strip_prefix("avatar-").filter(|bot|valid_bot(bot)).ok_or(DocumentError::Invalid)?;
        if report.version!=1||!valid_capability(&report.probe)||!valid_mount(&report.mount_id){return Err(DocumentError::Invalid);}
        let state=self.lock()?;
        let entry=state.entries.get(bot).ok_or(DocumentError::Stale)?;
        let load=entry.loaded.as_ref().ok_or(DocumentError::Pending)?;let slot=load.slot()?;
        let probe=slot.as_ref().filter(|p|same_capability(&p.token,&report.probe)).ok_or(DocumentError::Stale)?;
        let mut observed=probe.lock()?;probe.check(&mut observed,now)?;
        if let Some(mount)=&observed.mount{if mount!=&report.mount_id{return Err(DocumentError::Stale);}}
        else{
            let Some(deadline)=now.checked_add(ASSET_MS).filter(|n|*n<=MAX_COUNTER)else{
                observed.phase=ProbePhase::Failed;probe.delivery.cancel.store(true,Ordering::SeqCst);return Err(DocumentError::Expired);
            };
            observed.mount=Some(report.mount_id.clone());
            observed.deadline=deadline;
        }
        match (observed.phase,report.status){
            (_,AssetReadinessStatus::Failed)=>{observed.phase=ProbePhase::Failed;probe.delivery.cancel.store(true,Ordering::SeqCst);},
            (ProbePhase::Ready,AssetReadinessStatus::Loading)=>return Err(DocumentError::Stale),
            (_,AssetReadinessStatus::Ready)=>observed.phase=ProbePhase::Ready,
            (_,AssetReadinessStatus::Loading)=>observed.phase=ProbePhase::Loading,
        }
        observed.last=now;
        probe.status(&observed)
    }
    pub(crate) fn consume_ready(&self,grant:&AssetProbeGrant,now:u64)->Result<AssetReadyReceipt,DocumentError>{
        let state=self.lock()?;Self::matches_probe(&state,&grant.0)?;
        let mut observed=grant.0.lock()?;grant.0.check(&mut observed,now)?;
        if observed.phase!=ProbePhase::Ready{return Err(DocumentError::Pending);}
        observed.phase=ProbePhase::Consumed;observed.last=now;Ok(AssetReadyReceipt{probe:grant.0.clone(),armed:true})
    }
    pub(crate) fn rotate_for_attempt(&self,mut receipt:AssetReadyReceipt,now:u64)->Result<DocumentGrant,DocumentError>{
        let mut state=self.lock()?;Self::matches_probe(&state,&receipt.probe)?;
        let mut observed=receipt.probe.lock()?;
        if observed.phase!=ProbePhase::Consumed||!receipt.probe.valid_at(&observed,now){return Err(DocumentError::Stale);}
        let epoch=state.epoch()?;
        let entry=state.entries.get_mut(receipt.loaded().bot()).ok_or(DocumentError::Stale)?;
        entry.document.take();
        let document=Document{epoch,cancel:Arc::new(AtomicBool::new(false)),phase:Phase::Finished{issued_at:now,capability:capability(),delivery:receipt.probe.delivery.clone(),pinned:None}};
        let Phase::Finished{capability,delivery,..}=&document.phase else{unreachable!()};
        let grant=DocumentGrant{capability:capability.clone(),proof:proof(receipt.loaded().bot(),entry,&document),issued_at:now,delivery:delivery.clone()};
        entry.document=Some(document);observed.phase=ProbePhase::Transferred;receipt.armed=false;Ok(grant)
    }
}

#[cfg(test)]
#[path="native_document_readiness_tests.rs"]
mod tests;
