//! Owner/capacity/interleaving tests. No native window or session APIs.
use super::*;
struct View(Mutex<Bounds>);
fn bounds()->Bounds{Bounds{x:-900,y:100,width:520,height:720}}
impl LayoutWindow for View{
    fn sample(&self)->Result<LayoutSample,LayoutError>{let b=*self.0.lock().unwrap();Ok(LayoutSample{bounds:b,inner:b,outer:b,visible:true,scale:1.,monitor:"m".into(),displays:vec![Display{id:"m".into(),name:"fixture".into(),bounds:Bounds{x:-1000,y:0,width:1000,height:1000},scale_factor:1.}]})}
    fn bounds(&self)->Result<Bounds,LayoutError>{Ok(*self.0.lock().unwrap())}
    fn size_direct(&self,b:Bounds)->Result<(),LayoutError>{let mut current=self.0.lock().unwrap();current.width=b.width;current.height=b.height;Ok(())}
    fn position_direct(&self,b:Bounds)->Result<(),LayoutError>{let mut current=self.0.lock().unwrap();current.x=b.x;current.y=b.y;Ok(())}
    fn clicks(&self,_:bool)->Result<(),LayoutError>{Ok(())}
    fn top(&self,_:bool)->Result<(),LayoutError>{Ok(())}
    fn normalize(&self,b:Bounds)->Result<Bounds,LayoutError>{Ok(b)}
}
fn register(hub:&NativePresentations,id:&str)->LayoutHandle{hub.register(id,Arc::new(View(Mutex::new(bounds()))),Arc::new(WindowSignals::default())).unwrap();hub.handle(id).unwrap()}
fn reserve(hub:&NativePresentations,handle:&LayoutHandle)->Arc<LayoutOwner>{hub.reserve(handle,&handle.sample().unwrap(),0,5000,Arc::new(AtomicBool::new(false))).unwrap()}
fn release(hub:&NativePresentations,owner:&Arc<LayoutOwner>){owner.cancelled();owner.begin_restore().unwrap();assert!(owner.restore_once(false,false,||true).unwrap());assert!(hub.finish(owner));}

#[test]
fn sixty_four_retained_constructors_count_old_queued_handles_until_drained(){
    let hub=NativePresentations::default();let mut pending=Vec::new();for _ in 0..64{pending.push(register(&hub,"nova"));}
    assert!(matches!(hub.register("nova",Arc::new(View(Mutex::new(bounds()))),Arc::new(WindowSignals::default())),Err(LayoutError::Capacity)));
    assert!(hub.handle("nova").is_err(),"failed successor cannot keep an old current handle");
    assert_eq!(hub.state.lock().unwrap().records.len(),64);drop(pending);let next=register(&hub,"nova");assert!(next.current());assert_eq!(hub.state.lock().unwrap().records.len(),1);
}
#[test]
fn eight_cleanup_owners_stay_charged_through_cancel_and_release_exactly(){
    let hub=NativePresentations::default();let mut owners=Vec::new();for i in 0..8{let h=register(&hub,&format!("a{i}"));owners.push(reserve(&hub,&h));}
    let extra=register(&hub,"extra");owners[0].cancelled();assert!(matches!(hub.reserve(&extra,&extra.sample().unwrap(),0,5000,Arc::new(AtomicBool::new(false))),Err(LayoutError::Busy)));
    release(&hub,&owners[0]);let admitted=reserve(&hub,&extra);release(&hub,&admitted);for owner in owners.iter().skip(1){release(&hub,owner);}
}
#[test]
fn replacement_keeps_old_normal_and_cleanup_separate_from_successor(){
    let hub=NativePresentations::default();let a=register(&hub,"nova");let old=reserve(&hub,&a);let b=register(&hub,"nova");let current=reserve(&hub,&b);
    assert!(old.cancel.load(Ordering::SeqCst));assert!(!current.cancel.load(Ordering::SeqCst));assert!(!a.current());assert!(b.current());
    hub.retire("nova",a.signals());old.finish_destroyed();assert!(hub.finish(&old));assert!(current.valid(0));assert_eq!(current.handle().sample().unwrap().bounds,bounds());release(&hub,&current);
}
#[test]
fn queued_main_permit_outlives_waiter_and_prevents_restore_until_closure_drops(){
    let hub=NativePresentations::default();let h=register(&hub,"nova");let owner=reserve(&hub,&h);let permit=owner.begin_main(false,0).unwrap();owner.cancelled();assert!(!permit.allowed(false,0));assert_eq!(owner.begin_restore(),Err(LayoutError::Busy));assert!(owner.in_flight());drop(permit);release(&hub,&owner);
}
#[test]
fn typed_geometry_is_not_double_enqueued_by_the_direct_setter_adapter(){
    let hub=NativePresentations::default();let h=register(&hub,"nova");let owner=reserve(&hub,&h);let target=Bounds{width:900,..bounds()};owner.install_fields(target).unwrap();owner.expand_field(true,target,0).unwrap();
    assert!(h.signals().has_pending_resize());assert!(hub.observe("nova",h.signals(),None,Some((target.width,target.height))));assert!(!h.signals().has_pending_resize());let fence=owner.getter_fence().unwrap();owner.accept_getter(fence,target,0).unwrap();
    // Exact destruction retires the owner without pretending this fake window
    // supplied a restoring native event.
    hub.retire("nova",h.signals());owner.finish_destroyed();assert!(hub.finish(&owner));
}
#[test]
fn counter_exhaustion_and_poison_cancel_retained_atoms_without_successor(){
    let hub=NativePresentations::default();let h=register(&hub,"nova");let owner=reserve(&hub,&h);hub.state.lock().unwrap().serial=crate::life_domain::MAX_COUNTER;
    assert_eq!(hub.register("nova",h.0.window.clone(),h.signals().clone()),Err(LayoutError::Unavailable));assert!(owner.cancel.load(Ordering::SeqCst));release(&hub,&owner);
    let hub=NativePresentations::default();let h=register(&hub,"nova");let owner=reserve(&hub,&h);
    let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{let _guard=hub.state.lock().unwrap();panic!("synthetic poison");}));
    assert!(hub.handle("nova").is_err());assert!(owner.cancel.load(Ordering::SeqCst));assert!(hub.register("b",Arc::new(View(Mutex::new(bounds()))),Arc::new(WindowSignals::default())).is_err());
    // Cancel-only recovery is not a grant/clear-poison path. Break this test's
    // private Arc cycle after checking denial; no production recovery exists.
    h.0.owner.lock().unwrap().take();
}
