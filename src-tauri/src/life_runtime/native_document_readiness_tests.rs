//! Real document/readiness transitions with synthetic cancellation identities.
//! These are not a native constructor, completed load, rendered frame or grant.
use super::*;
use serde_json::json;

fn fixture()->(Documents,Arc<WindowSignals>,DocumentGrant,LoadedDocumentReceipt){
    let documents=Documents::default();let signals=Arc::new(WindowSignals::default());
    assert!(documents.track("nova",&signals));assert!(documents.started("nova",&signals));
    let finished=documents.finished("nova",&signals,0).ok().unwrap();
    let loaded=documents.loaded("nova",&signals).ok().unwrap();(documents,signals,finished,loaded)
}
fn binding()->Value{crate::emote_capabilities::binding(&json!({"id":"nova","avatar":{"assetPath":"nova"},"motions":[]}))}
fn probe(d:&Documents,l:&LoadedDocumentReceipt,at:u64)->(AssetProbeGrant,Arc<AtomicBool>,Arc<AtomicBool>){
    let cancel=Arc::new(AtomicBool::new(false));let lease=Arc::new(AtomicBool::new(false));
    (d.begin_probe(l,binding(),cancel.clone(),lease.clone(),at).ok().unwrap(),cancel,lease)
}
fn report(p:&AssetProbeGrant,status:AssetReadinessStatus)->AssetReadinessReport{AssetReadinessReport{version:1,probe:p.probe().into(),mount_id:"actual-mount".into(),status}}
fn ready(d:&Documents,p:&AssetProbeGrant,at:u64)->AssetReadyReceipt{
    assert_eq!(d.report_probe("avatar-nova",&report(p,AssetReadinessStatus::Ready),at).unwrap(),AssetProbeState::Ready);
    d.consume_ready(p,at).ok().unwrap()
}

#[test]
fn loaded_lineage_requires_finished_and_survives_semantic_rotation_only(){
    let d=Documents::default();let signals=Arc::new(WindowSignals::default());assert!(d.track("nova",&signals));
    assert!(matches!(d.loaded("nova",&signals),Err(DocumentError::Pending)));
    assert!(d.started("nova",&signals));assert!(matches!(d.loaded("nova",&signals),Err(DocumentError::Pending)));
    let old=d.finished("nova",&signals,0).ok().unwrap();let loaded=d.loaded("nova",&signals).ok().unwrap();
    let next=d.rotate("nova",&signals,1).ok().unwrap();assert!(!old.proof().current());
    assert!(loaded.current());assert!(loaded.same_loaded(&d.loaded("nova",&signals).ok().unwrap()));
    assert!(next.proof().current());assert!(d.started("nova",&signals));assert!(!loaded.current());
    d.finished("nova",&signals,2).ok().unwrap();let reloaded=d.loaded("nova",&signals).ok().unwrap();
    assert!(reloaded.current());assert_ne!(loaded.load_id(),reloaded.load_id());assert!(!loaded.same_loaded(&reloaded));
}

#[test]
fn readiness_alone_never_authenticates_any_operational_document(){
    let (d,_s,finished,loaded)=fixture();let (p,_cancel,_lease)=probe(&d,&loaded,100);
    assert!(matches!(d.probe_status(&p,100),Ok(AssetProbeState::AwaitingMount)));
    assert!(d.authenticate("avatar-nova",p.probe(),"actual-mount",Some(1),101).is_err());
    assert!(d.authenticate("avatar-nova",finished.capability(),"actual-mount",Some(1),101).is_err());
    assert!(d.consume_ready(&p,101).is_err());
    d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Loading),101).unwrap();
    assert!(d.consume_ready(&p,102).is_err());
    let ready=ready(&d,&p,6000);assert!(ready.current(6000));
    assert!(d.authenticate("avatar-nova",p.probe(),"actual-mount",Some(1),6000).is_err());drop(ready);
}

#[test]
fn first_pin_deadline_is_original_and_loading_has_one_fixed_sixty_second_deadline(){
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,100);
    let again=d.begin_probe(&l,binding(),cancel.clone(),p.0.delivery.lease.get().unwrap().clone(),4999).ok().unwrap();
    assert_eq!(again.probe(),p.probe());assert_eq!(again.issued_at(),100);
    d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Loading),5099).unwrap();
    assert_eq!(p.0.lock().unwrap().deadline,65099);
    d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Loading),60000).unwrap();
    assert_eq!(p.0.lock().unwrap().deadline,65099,"loading must not renew time");
    assert!(matches!(d.probe_status(&p,65098),Ok(AssetProbeState::Loading)));
    assert!(matches!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),65099),Err(DocumentError::Expired)));
    assert!(cancel.load(Ordering::SeqCst));assert!(d.consume_ready(&p,65099).is_err());
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,100);
    assert!(matches!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),5100),Err(DocumentError::Expired)));
    assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn malformed_foreign_and_changed_mount_reports_cannot_pin_or_replace_a_probe(){
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);
    let r=report(&p,AssetReadinessStatus::Loading);
    assert!(d.report_probe("main",&r,1).is_err());assert!(d.report_probe("avatar-other",&r,1).is_err());
    let mut wrong=report(&p,AssetReadinessStatus::Ready);wrong.version=2;assert!(d.report_probe("avatar-nova",&wrong,1).is_err());
    wrong.version=1;wrong.probe="a".repeat(63);assert!(d.report_probe("avatar-nova",&wrong,1).is_err());
    assert_eq!(d.probe_status(&p,1).unwrap(),AssetProbeState::AwaitingMount);
    d.report_probe("avatar-nova",&r,2).unwrap();let deadline=p.0.lock().unwrap().deadline;
    let mut changed=report(&p,AssetReadinessStatus::Ready);changed.mount_id="different-mount".into();assert!(d.report_probe("avatar-nova",&changed,3).is_err());
    assert_eq!(p.0.lock().unwrap().deadline,deadline);assert!(!cancel.load(Ordering::SeqCst));assert_eq!(d.probe_status(&p,3).unwrap(),AssetProbeState::Loading);
}

#[test]
fn only_exact_avatar_binding_cancel_and_original_lease_are_idempotent(){
    let (d,_s,_f,l)=fixture();let (p,cancel,lease)=probe(&d,&l,0);
    assert_eq!(p.avatar_binding(),&binding());assert!(p.loaded().same_loaded(&l));
    assert!(d.begin_probe(&l,binding(),cancel.clone(),Arc::new(AtomicBool::new(false)),1).is_err());
    let mut changed=binding();changed[1]=json!("different-model");assert!(d.begin_probe(&l,changed,cancel.clone(),lease.clone(),1).is_err());
    assert!(d.begin_probe(&l,binding(),Arc::new(AtomicBool::new(false)),lease.clone(),1).is_err());
    assert!(!cancel.load(Ordering::SeqCst));lease.store(true,Ordering::SeqCst);
    assert!(!p.current(2));assert!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),2).is_err());
    assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn ready_consumption_is_once_and_dropped_receipt_cancels_the_original_attempt(){
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);let receipt=ready(&d,&p,1);
    assert!(!p.current(1));assert!(receipt.current(1));assert!(d.consume_ready(&p,1).is_err());
    assert!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),1).is_err());
    assert!(!cancel.load(Ordering::SeqCst));drop(receipt);assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn transfer_keeps_probe_delivery_and_lease_after_old_unused_document_is_dropped(){
    let (d,s,old,l)=fixture();let (p,cancel,lease)=probe(&d,&l,0);
    d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Loading),1).unwrap();
    let receipt=ready(&d,&p,10000);assert!(Arc::ptr_eq(&receipt.lease_cancel(),&lease));
    let grant=d.rotate_for_attempt(receipt,10001).ok().unwrap();
    assert!(!old.proof().current());assert!(!old.delivery_current());assert!(l.current());assert!(l.same_loaded(&d.loaded("nova",&s).ok().unwrap()));
    assert!(Arc::ptr_eq(&grant.delivery_cancel(),&cancel));assert!(Arc::ptr_eq(grant.delivery.lease.get().unwrap(),&lease));assert!(grant.delivery_current());
    let auth=d.authenticate("avatar-nova",grant.capability(),"normal-warm-mount",Some(9),10002).ok().unwrap();assert!(auth.current());
    assert!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Failed),10003).is_err());
    assert!(d.consume_ready(&p,10003).is_err());assert!(d.probe_status(&p,10003).is_err());drop(p);
    assert!(!cancel.load(Ordering::SeqCst));assert!(auth.current());lease.store(true,Ordering::SeqCst);assert!(!auth.current());
}

#[test]
fn stop_or_semantic_cancel_between_ready_and_rotation_cannot_issue_an_operational_capability(){
    for stop in [false,true]{let (d,_s,_f,l)=fixture();let (p,cancel,lease)=probe(&d,&l,0);let receipt=ready(&d,&p,1);
        if stop{lease.store(true,Ordering::SeqCst);}else{cancel.store(true,Ordering::SeqCst);}
        assert!(!receipt.current(2));assert!(d.rotate_for_attempt(receipt,2).is_err());assert!(cancel.load(Ordering::SeqCst));assert!(l.current());
    }
}

#[test]
fn real_navigation_replacement_and_retirement_cancel_old_probes_and_loaded_receipts(){
    for event in [0,1,2]{let (d,s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);let receipt=ready(&d,&p,1);
        let replacement=Arc::new(WindowSignals::default());
        match event{0=>assert!(d.started("nova",&s)),1=>assert!(d.track("nova",&replacement)),_=>assert!(d.retire("nova",&s))};
        assert!(!l.current());assert!(cancel.load(Ordering::SeqCst));assert!(d.rotate_for_attempt(receipt,2).is_err());
    }
}

#[test]
fn stale_probe_reports_and_drops_cannot_cancel_a_successor_attempt(){
    let (d,_s,_f,l)=fixture();let (old,cancel,_lease)=probe(&d,&l,0);let receipt=ready(&d,&old,1);cancel.store(true,Ordering::SeqCst);
    let (new,current,_lease)=probe(&d,&l,2);assert_ne!(new.probe(),old.probe());
    assert!(d.report_probe("avatar-nova",&report(&old,AssetReadinessStatus::Failed),3).is_err());
    assert!(d.probe_status(&old,3).is_err());assert!(d.rotate_for_attempt(receipt,3).is_err());
    assert!(!current.load(Ordering::SeqCst));assert_eq!(d.probe_status(&new,3).unwrap(),AssetProbeState::AwaitingMount);
}

#[test]
fn ready_never_rewinds_to_loading_and_failed_is_terminal(){
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);
    d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),1).unwrap();
    assert!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Loading),2).is_err());assert!(!cancel.load(Ordering::SeqCst));
    assert_eq!(d.probe_status(&p,2).unwrap(),AssetProbeState::Ready);
    assert_eq!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Failed),3).unwrap(),AssetProbeState::Failed);
    assert!(cancel.load(Ordering::SeqCst));assert!(d.consume_ready(&p,4).is_err());
}

#[test]
fn closed_readiness_schema_rejects_extra_status_or_authority_fields(){
    let good=json!({"version":1,"probe":"a".repeat(64),"mountId":"mount","status":"ready"});
    assert!(serde_json::from_value::<AssetReadinessReport>(good.clone()).is_ok());
    for status in ["rendered","live","completed",""]{let mut v=good.clone();v["status"]=json!(status);assert!(serde_json::from_value::<AssetReadinessReport>(v).is_err());}
    for field in ["botId","avatarBinding","sessionId","warmFrame","ready"]{let mut v=good.clone();v[field]=json!(true);assert!(serde_json::from_value::<AssetReadinessReport>(v).is_err());}
}

#[test]
fn backward_clock_and_counter_overflow_are_terminal_without_deadline_revival(){
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,10);d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Loading),20).unwrap();
    assert!(matches!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),19),Err(DocumentError::Expired)));assert!(cancel.load(Ordering::SeqCst));
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,MAX_COUNTER-5000);
    assert!(matches!(d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),MAX_COUNTER-4999),Err(DocumentError::Expired)));
    assert!(cancel.load(Ordering::SeqCst));assert!(d.consume_ready(&p,MAX_COUNTER-4998).is_err());
}

#[test]
fn poison_and_registry_drop_cancel_readiness_without_recovering_or_retaining_load_cycles(){
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{let _guard=p.0.state.lock().unwrap();panic!("synthetic readiness poison");})).is_err());
    assert!(d.probe_status(&p,1).is_err());assert!(cancel.load(Ordering::SeqCst));assert!(p.0.state.is_poisoned());
    let (d,_s,_f,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);let physical=l.load.clone();drop(d);
    assert!(!l.current());assert!(cancel.load(Ordering::SeqCst));assert!(physical.upgrade().is_none(),"probe owns only a weak physical lineage receipt");assert!(!p.current(1));
}

#[test]
fn numerically_equal_foreign_load_lineage_cannot_create_or_consume_this_probe(){
    let (d,s,_f,l)=fixture();let foreign=Documents::default();assert!(foreign.track("nova",&s));assert!(foreign.started("nova",&s));foreign.finished("nova",&s,0).ok().unwrap();
    let other=foreign.loaded("nova",&s).ok().unwrap();assert_eq!(l.load_id(),other.load_id());assert!(!l.same_loaded(&other));
    assert!(foreign.begin_probe(&l,binding(),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false)),0).is_err());
    let (p,cancel,_lease)=probe(&d,&l,0);d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),1).unwrap();
    assert!(foreign.consume_ready(&p,1).is_err());assert!(!cancel.load(Ordering::SeqCst));assert_eq!(d.probe_status(&p,1).unwrap(),AssetProbeState::Ready);
}

#[test]
fn concurrent_consumers_obtain_one_receipt_and_loser_cannot_cancel_winner(){
    let (d,_s,_f,l)=fixture();let d=Arc::new(d);let (p,cancel,_lease)=probe(&d,&l,0);d.report_probe("avatar-nova",&report(&p,AssetReadinessStatus::Ready),1).unwrap();
    let barrier=Arc::new(std::sync::Barrier::new(2));
    let joins=[0,1].map(|_|{let d=d.clone();let p=p.clone();let barrier=barrier.clone();std::thread::spawn(move||{barrier.wait();d.consume_ready(&p,1).ok()})});
    let mut winners=joins.into_iter().filter_map(|join|join.join().unwrap()).collect::<Vec<_>>();
    assert_eq!(winners.len(),1);assert!(!cancel.load(Ordering::SeqCst));
    let grant=d.rotate_for_attempt(winners.pop().unwrap(),1).ok().unwrap();assert!(grant.delivery_current());
}

#[test]
fn final_rotation_cannot_admit_an_expired_ready_receipt_or_retime_asset_budget(){
    let (d,_s,finished,l)=fixture();let (p,cancel,_lease)=probe(&d,&l,0);let receipt=ready(&d,&p,1);
    assert!(receipt.current(60000));assert!(!receipt.current(60001));
    assert!(d.rotate_for_attempt(receipt,60001).is_err());assert!(cancel.load(Ordering::SeqCst));
    let unchanged=d.current_grant("nova",l.signals(),60001).ok().unwrap();assert_eq!(unchanged.capability(),finished.capability());
}
