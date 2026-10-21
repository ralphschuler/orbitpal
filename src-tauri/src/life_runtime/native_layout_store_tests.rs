//! Actual driver/Hub/Core/SQLite scenarios with controlled Main and WindowOps.
//! No native monitor/session/window APIs, renderer, provider or real OS data.
use super::Fixture;
use crate::{life_runtime::{action_output::OutputGate,native_layout::{self,LayoutContext,LayoutHost,NativeLayoutIntent,NativeLayoutPrepared},native_output_route::{GeometryClassification,LayoutRoute,LayoutRouteState,NativeOutputRoutes,PreparationTicket}},overlay_presentation::{Bounds,Display,LayoutError,LayoutHandle,LayoutMain,LayoutOwner,LayoutSample,LayoutWindow,PresentationHub},window_signals::WindowSignals};
use std::{collections::VecDeque,sync::{atomic::{AtomicBool,AtomicU64,Ordering},Arc,Mutex},time::Duration};
type Task=Box<dyn FnOnce()+Send>;
#[derive(Default)]struct Main{queue:Mutex<VecDeque<Task>>}
impl LayoutMain for Main{fn enqueue(&self,task:Task)->Result<(),LayoutError>{self.queue.lock().unwrap().push_back(task);Ok(())}}
impl Main{fn step(&self)->bool{let task=self.queue.lock().unwrap().pop_front();if let Some(task)=task{task();true}else{false}}}
#[derive(Clone,Copy)]enum Geometry{Move(i32,i32),Size(u32,u32)}
struct Window{
    bounds:Mutex<Bounds>,visible:AtomicBool,scale:Mutex<f64>,displays:Mutex<Vec<Display>>,calls:Mutex<Vec<&'static str>>,
    observer:Mutex<Option<Arc<dyn Fn(Geometry)+Send+Sync>>>,delayed:AtomicBool,pending:Mutex<VecDeque<Geometry>>,failure:Mutex<Option<&'static str>>,
    policy_calls:Mutex<Vec<(&'static str,bool)>>,after_top:Mutex<Option<Box<dyn FnOnce()+Send>>>,samples:AtomicU64,
}
fn normal()->Bounds{Bounds{x:-1300,y:100,width:520,height:720}}
fn arena()->Bounds{Bounds{x:-1600,y:0,width:1600,height:1000}}
fn displays()->Vec<Display>{vec![Display{id:"fixture-monitor".into(),name:"synthetic".into(),bounds:arena(),scale_factor:1.}]}
impl Window{
    fn new()->Self{Self{bounds:Mutex::new(normal()),visible:AtomicBool::new(true),scale:Mutex::new(1.),displays:Mutex::new(displays()),calls:Mutex::new(vec![]),observer:Mutex::new(None),delayed:AtomicBool::new(false),pending:Mutex::new(VecDeque::new()),failure:Mutex::new(None),policy_calls:Mutex::new(vec![]),after_top:Mutex::new(None),samples:AtomicU64::new(0)}}
    fn event(&self,event:Geometry){let mut bounds=self.bounds.lock().unwrap();match event{Geometry::Move(x,y)=>{bounds.x=x;bounds.y=y},Geometry::Size(w,h)=>{bounds.width=w;bounds.height=h}}drop(bounds);let callback=self.observer.lock().unwrap().clone();if let Some(callback)=callback{callback(event);}}
    fn signal_only(&self,event:Geometry){let callback=self.observer.lock().unwrap().clone();if let Some(callback)=callback{callback(event);}}
    fn issue(&self,name:&'static str,event:Geometry)->Result<(),LayoutError>{self.calls.lock().unwrap().push(name);if self.delayed.load(Ordering::SeqCst){self.pending.lock().unwrap().push_back(event);}else{self.event(event);}let mut failure=self.failure.lock().unwrap();if *failure==Some(name){failure.take();Err(LayoutError::Geometry)}else{Ok(())}}
    fn flush(&self){loop{let next=self.pending.lock().unwrap().pop_front();if let Some(next)=next{self.event(next)}else{break}}}
    fn geometry_calls(&self)->usize{self.calls.lock().unwrap().iter().filter(|c|matches!(**c,"size"|"move")).count()}
}
impl LayoutWindow for Window{
    fn sample(&self)->Result<LayoutSample,LayoutError>{self.samples.fetch_add(1,Ordering::SeqCst);let bounds=*self.bounds.lock().unwrap();Ok(LayoutSample{bounds,inner:bounds,outer:bounds,visible:self.visible.load(Ordering::SeqCst),scale:*self.scale.lock().unwrap(),monitor:"fixture-monitor".into(),displays:self.displays.lock().unwrap().clone()})}
    fn bounds(&self)->Result<Bounds,LayoutError>{Ok(*self.bounds.lock().unwrap())}
    fn size_direct(&self,b:Bounds)->Result<(),LayoutError>{self.issue("size",Geometry::Size(b.width,b.height))}
    fn position_direct(&self,b:Bounds)->Result<(),LayoutError>{self.issue("move",Geometry::Move(b.x,b.y))}
    fn clicks(&self,value:bool)->Result<(),LayoutError>{self.calls.lock().unwrap().push(if value{"guard"}else{"clicks"});self.policy_calls.lock().unwrap().push(("clicks",value));Ok(())}
    fn top(&self,value:bool)->Result<(),LayoutError>{self.calls.lock().unwrap().push("top");self.policy_calls.lock().unwrap().push(("top",value));let hook=self.after_top.lock().unwrap().take();if let Some(hook)=hook{hook();}Ok(())}
    fn normalize(&self,b:Bounds)->Result<Bounds,LayoutError>{Ok(b)}
}
struct Host{
    f:Mutex<Fixture>,hub:PresentationHub,routes:NativeOutputRoutes,outputs:OutputGate,main:Arc<Main>,window:Arc<Window>,signals:Arc<WindowSignals>,
    now:AtomicU64,expected:LayoutContext,ui:crate::life_runtime::native_output_route::LayoutUi,
    persist_entered:AtomicBool,fail_persist:AtomicBool,before_persist:Mutex<Option<Box<dyn FnOnce()+Send>>>,
    gate:crate::life_availability::AvailabilityGate,
    policy_reads:AtomicU64,before_reserve:Mutex<Option<Box<dyn FnOnce(&Arc<AtomicBool>)+Send>>>,
    before_finish:Mutex<Option<Box<dyn FnOnce()+Send>>>,fail_finish:AtomicBool,
    fail_release:AtomicBool,finish_calls:AtomicU64,release_calls:AtomicU64,
    owner:Mutex<Option<Arc<LayoutOwner>>>,hold_classification:AtomicBool,
    classifications:Mutex<Vec<(GeometryClassification,bool)>>,classifying_reads:AtomicU64,
    after_route_read:Mutex<Option<Box<dyn FnOnce()+Send>>>,
}
impl Host{
    fn new()->Arc<Self>{
        let mut f=Fixture::new(&["nova"]);let mut profile=f.store.bot_profile("nova").unwrap();profile["output"]["mode"]=serde_json::json!("overlay");profile["output"]["streamingEnabled"]=serde_json::json!(true);f.store.save_profile(profile).unwrap();
        let record=f.store.load_world_config().unwrap().unwrap();f.store.save_world_config(Some(record.config.revision()),&record.config,&crate::storage::world_store::OutputBinding{monitor_id:Some("fixture-monitor".into())}).unwrap();f.context("nova");
        let routes=NativeOutputRoutes::default();routes.ui_connect("studio");routes.ui_report("studio",1,false);
        let ui=routes.layout_ui().unwrap();let expected=native_layout::context(&f.core,&f.store,&f.world,"nova",ui.clone(),"studio",0).unwrap();
        let hub=PresentationHub::default();let window=Arc::new(Window::new());let signals=Arc::new(WindowSignals::default());let outputs=OutputGate::default();
        hub.register_window("nova",signals.clone(),None).unwrap();routes.track_window_with_output("nova","avatar-nova",&signals,0,&outputs).unwrap();assert!(hub.retain_layout_window("nova",window.clone(),signals.clone(),true));
        let gate=crate::life_availability::AvailabilityGate::new(crate::life_availability::Source::Unsupported);gate.apply_sample(gate.sample_fence(),crate::life_availability::Observation::ready());
        let host=Arc::new(Self{f:Mutex::new(f),hub,routes,outputs,main:Arc::new(Main::default()),window,signals,now:AtomicU64::new(0),expected,ui,persist_entered:AtomicBool::new(false),fail_persist:AtomicBool::new(false),before_persist:Mutex::new(None),gate,policy_reads:AtomicU64::new(0),before_reserve:Mutex::new(None),before_finish:Mutex::new(None),fail_finish:AtomicBool::new(false),fail_release:AtomicBool::new(false),finish_calls:AtomicU64::new(0),release_calls:AtomicU64::new(0),owner:Mutex::new(None),hold_classification:AtomicBool::new(false),classifications:Mutex::new(vec![]),classifying_reads:AtomicU64::new(0),after_route_read:Mutex::new(None)});
        let weak=Arc::downgrade(&host);*host.window.observer.lock().unwrap()=Some(Arc::new(move|event|{let host=weak.upgrade().unwrap();let classification=host.routes.begin_geometry("nova",&host.signals,&host.outputs);let accepted=match event{Geometry::Move(x,y)=>host.hub.native_moved("nova",&host.signals,x,y),Geometry::Size(w,h)=>host.hub.native_resized("nova",&host.signals,w,h)};if let Some(classification)=classification{if host.hold_classification.load(Ordering::SeqCst){host.classifications.lock().unwrap().push((classification,accepted));}else{host.routes.finish_geometry(classification,accepted);}}}));host
    }
    fn intent(self:&Arc<Self>)->NativeLayoutIntent{let handle=self.hub.native_handle("nova").unwrap();let witness=self.routes.witness("nova","avatar-nova",&self.signals,self.now()).unwrap();let ticket=self.routes.begin_preparation(witness,self.now()).unwrap();NativeLayoutIntent::new(self.clone(),ticket,handle)}
    async fn pump_until(&self,mut predicate:impl FnMut()->bool){for _ in 0..3000{while self.main.step(){}if predicate(){return;}tokio::time::sleep(Duration::from_millis(1)).await;}panic!("controlled driver did not settle");}
    async fn wait_until(&self,mut predicate:impl FnMut()->bool){for _ in 0..1500{if predicate(){return;}tokio::time::sleep(Duration::from_millis(1)).await;}panic!("controlled worker did not settle");}
    async fn step_one(&self){self.wait_until(||!self.main.queue.lock().unwrap().is_empty()).await;assert!(self.main.step());}
    async fn prepared(self:&Arc<Self>)->NativeLayoutPrepared{let task=tokio::spawn(native_layout::run_driver(self.intent()));self.pump_until(||task.is_finished()).await;task.await.unwrap().unwrap()}
    async fn released(&self){self.pump_until(||!self.hub.any_native_work()).await;}
    fn stop(&self){self.routes.revoke(Some("nova"));self.f.lock().unwrap().core.stop("nova",self.now());}
    fn finish_classification(&self,accepted:bool){self.hold_classification.store(false,Ordering::SeqCst);for (fact,observed) in self.classifications.lock().unwrap().drain(..){self.routes.finish_geometry(fact,accepted&&observed);}}
    fn owned(&self)->Arc<LayoutOwner>{self.owner.lock().unwrap().as_ref().unwrap().clone()}
}
impl LayoutHost for Host{
    fn now(&self)->u64{self.now.load(Ordering::SeqCst)}
    fn main(&self)->Arc<dyn LayoutMain>{self.main.clone()}
    fn refresh(&self)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<(),LayoutError>>+Send+'_>>{Box::pin(async{Ok(())})}
    fn validate(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{let f=self.f.lock().unwrap();if cancel.load(Ordering::SeqCst)||!native_layout::context_current(&self.expected,&f.core,&f.store,&f.world,self.now()){Err(LayoutError::Stale)}else{Ok(())}}
    fn reserve(&self,h:&LayoutHandle,s:&LayoutSample,i:u64,d:u64,c:Arc<AtomicBool>)->Result<Arc<LayoutOwner>,LayoutError>{let hook=self.before_reserve.lock().unwrap().take();if let Some(hook)=hook{hook(&c);}let owner=self.hub.reserve_native(h,s,i,d,c)?;*self.owner.lock().unwrap()=Some(owner.clone());Ok(owner)}
    fn persist(&self,b:Bounds,c:&Arc<AtomicBool>)->Result<(),LayoutError>{self.persist_entered.store(true,Ordering::SeqCst);if let Some(hook)=self.before_persist.lock().unwrap().take(){hook();}if self.fail_persist.load(Ordering::SeqCst){return Err(LayoutError::Persistence);}let f=self.f.lock().unwrap();f.store.save_window_bounds(&serde_json::to_value(b).unwrap()).map_err(|_|LayoutError::Persistence)?;drop(f);self.validate(c)}
    fn attach(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{self.gate.watch(cancel).map(|_|()).ok_or(LayoutError::Unavailable)}
    fn retain(&self,t:&PreparationTicket,h:&LayoutHandle,c:&Arc<AtomicBool>)->Result<LayoutRoute,LayoutError>{self.routes.retain_preparation(t,h.signals(),c,&self.ui,self.now()).map_err(|_|LayoutError::Stale)}
    fn handoff(&self,t:&PreparationTicket,o:&Arc<LayoutOwner>)->Result<LayoutRoute,LayoutError>{self.routes.handoff_preparation(t,o.handle().signals(),&o.cancel,&self.ui,self.now()).map_err(|_|LayoutError::Stale)}
    fn route_state(&self,r:&LayoutRoute)->LayoutRouteState{
        let observed=self.routes.layout_state(r,self.now());
        // Model a native callback beginning immediately AFTER the route read,
        // before the driver acquires its getter fence. No production hook.
        if observed==LayoutRouteState::Current{let hook=self.after_route_read.lock().unwrap().take();if let Some(hook)=hook{hook();}}
        if observed==LayoutRouteState::Classifying{self.classifying_reads.fetch_add(1,Ordering::SeqCst);}
        observed
    }
    fn stage_surface(&self,r:&LayoutRoute,c:&Arc<native_layout::SurfaceCell>)->Result<(),LayoutError>{self.routes.stage_surface(r,c,self.now()).map_err(|_|LayoutError::Stale)}
    fn validate_live(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{if cancel.load(Ordering::SeqCst)||self.gate.snapshot().state!=crate::life_availability::Availability::AwakeUnlocked{Err(LayoutError::Stale)}else{Ok(())}}
    fn release_route(&self,r:&LayoutRoute)->bool{let released=if self.fail_release.load(Ordering::SeqCst){false}else{self.routes.release_layout(r)};self.release_calls.fetch_add(1,Ordering::SeqCst);released}
    fn finish(&self,o:&Arc<LayoutOwner>)->bool{self.finish_calls.fetch_add(1,Ordering::SeqCst);let hook=self.before_finish.lock().unwrap().take();if let Some(hook)=hook{hook();}if self.fail_finish.load(Ordering::SeqCst){return false;}assert!(self.hub.finish_native(o));true}
    fn policy(&self)->Option<crate::storage::profile_mutation::NativeLayoutPolicy>{let policy=self.f.lock().unwrap().store.native_layout_policy("nova");self.policy_reads.fetch_add(1,Ordering::SeqCst);policy}
    fn monitor(&self)->&str{self.expected.monitor()}
    fn save_current(&self,h:&LayoutHandle,b:Bounds){if h.current()&&!self.hub.suppress_bounds("nova"){self.f.lock().unwrap().store.save_window_bounds(&serde_json::to_value(b).unwrap()).unwrap();}}
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn real_driver_prepares_then_restores_without_life_or_renderer_grants(){
    let h=Host::new();let before=h.f.lock().unwrap().store.load_life("nova").unwrap().unwrap().state().clone();let prepared=h.prepared().await;
    assert_eq!(prepared.bounds,arena());assert_eq!(prepared.reference_physical_height,720);assert!(prepared.current(0));assert!(h.hub.suppress_bounds("nova"));
    assert_eq!(h.window.geometry_calls(),2,"one typed direct setter per changed field");
    assert!(!h.f.lock().unwrap().core.world_output_current("nova",0));
    drop(prepared);h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
    assert_eq!(h.f.lock().unwrap().store.load_life("nova").unwrap().unwrap().state(),&before);
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn no_op_layout_requires_no_geometry_event_or_setter(){let h=Host::new();*h.window.bounds.lock().unwrap()=arena();let p=h.prepared().await;assert_eq!(h.window.geometry_calls(),0);drop(p);h.released().await;assert_eq!(h.window.geometry_calls(),0);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn pre_setter_operator_drift_aborts_without_geometry_rollback(){let h=Host::new();let weak=Arc::downgrade(&h);*h.before_persist.lock().unwrap()=Some(Box::new(move||{weak.upgrade().unwrap().window.event(Geometry::Move(-1200,130));}));let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());h.released().await;assert_eq!(h.window.geometry_calls(),0);assert_eq!(h.window.bounds().unwrap().x,-1200);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn persistence_failure_releases_guard_without_click_or_geometry_mutation(){let h=Host::new();h.fail_persist.store(true,Ordering::SeqCst);let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||task.is_finished()).await;assert!(matches!(task.await.unwrap(),Err(LayoutError::Persistence)));h.released().await;assert_eq!(h.window.geometry_calls(),0);assert!(!h.window.calls.lock().unwrap().contains(&"guard"));}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn stop_precedes_cleanup_and_old_prepared_value_never_revives(){let h=Host::new();let p=h.prepared().await;h.stop();assert!(!p.current(0));h.released().await;assert!(!p.current(0));assert_eq!(h.window.bounds().unwrap(),normal());drop(p);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn periodic_hidden_observation_cancels_without_a_visibility_event(){let h=Host::new();let p=h.prepared().await;h.window.visible.store(false,Ordering::SeqCst);h.released().await;assert!(!p.current(0));assert_eq!(h.window.bounds().unwrap(),normal());drop(p);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn held_main_deadline_and_dropped_prepare_do_not_overtake_queued_work(){
    let h=Host::new();let task=tokio::spawn(native_layout::run_driver(h.intent()));
    h.step_one().await; // initial sample
    h.wait_until(||h.persist_entered.load(Ordering::SeqCst)).await;
    h.step_one().await; // fresh sample after persistence
    h.step_one().await; // click guard, before the first geometry task
    // Capture the next queued closure without running it; aborting the waiter
    // cannot clear its MainPermit. The late closure must decline expansion.
    h.wait_until(||!h.main.queue.lock().unwrap().is_empty()).await;
    let held=h.main.queue.lock().unwrap().pop_front().unwrap();
    task.abort();let _=task.await;h.now.store(6000,Ordering::SeqCst);assert!(h.hub.native_guarded("nova"));
    h.routes.revoke(None);assert!(h.hub.admit_dialog().is_err(),"the production dialog gate must not mistake revoke for restore");
    held();h.released().await;assert_eq!(h.window.geometry_calls(),0);assert!(h.hub.admit_dialog().is_ok());
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn partial_setter_error_uses_legacy_restore_after_real_expansion(){let h=Host::new();*h.window.failure.lock().unwrap()=Some("size");let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());assert!(h.window.geometry_calls()>=2);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn delayed_geometry_in_either_order_needs_events_then_later_getter(){
    for reverse in [false,true]{let h=Host::new();h.window.delayed.store(true,Ordering::SeqCst);let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||h.window.pending.lock().unwrap().len()==2).await;assert!(!task.is_finished());let mut events=h.window.pending.lock().unwrap().drain(..).collect::<Vec<_>>();if reverse{events.reverse();}for event in events{h.window.event(event);}h.pump_until(||task.is_finished()).await;let p=task.await.unwrap().unwrap();h.window.delayed.store(false,Ordering::SeqCst);drop(p);h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());}
}
#[test]
fn canonical_monitor_validation_rejects_ambiguity_truncation_and_cross_dpi(){let h=Host::new();let sample=h.window.sample().unwrap();assert_eq!(sample.target("fixture-monitor").unwrap(),arena());let mut bad=sample.clone();bad.displays.push(Display{bounds:Bounds{width:1200,..arena()},..bad.displays[0].clone()});assert_eq!(bad.target("fixture-monitor"),Err(LayoutError::Geometry));let mut bad=sample.clone();bad.scale=2.;assert_eq!(bad.target("fixture-monitor"),Err(LayoutError::Geometry));let mut bad=sample;bad.displays=vec![bad.displays[0].clone();65];assert_eq!(bad.target("fixture-monitor"),Err(LayoutError::Geometry));}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn sqlite_wait_expiry_rechecks_after_write_and_never_queues_geometry(){
    let h=Host::new();let path=h.f.lock().unwrap().root.join("cohost.sqlite3");
    let (ready_tx,ready_rx)=std::sync::mpsc::channel();let (release_tx,release_rx)=std::sync::mpsc::channel();
    let blocker=std::thread::spawn(move||{let db=rusqlite::Connection::open(path).unwrap();db.execute_batch("BEGIN IMMEDIATE").unwrap();ready_tx.send(()).unwrap();release_rx.recv().unwrap();db.execute_batch("ROLLBACK").unwrap();});ready_rx.recv().unwrap();
    let task=tokio::spawn(native_layout::run_driver(h.intent()));h.step_one().await;h.wait_until(||h.persist_entered.load(Ordering::SeqCst)).await;
    h.now.store(6001,Ordering::SeqCst);h.routes.revoke(Some("nova"));release_tx.send(()).unwrap();blocker.join().unwrap();h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());h.released().await;assert_eq!(h.window.geometry_calls(),0);
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn current_ui_activity_and_system_block_cancel_but_old_reporter_does_not(){
    for system in [false,true]{let h=Host::new();let p=h.prepared().await;h.routes.ui_disconnect("retired");h.routes.ui_report("retired",99,true);h.routes.ui_report("studio",0,true);assert!(p.current(0));if system{h.gate.block(crate::life_availability::Reason::Sleeping);}else{h.routes.ui_report("studio",2,true);}assert!(!p.current(0));h.released().await;drop(p);}
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn foreign_legacy_request_cannot_keep_prepared_observation_valid(){let h=Host::new();let p=h.prepared().await;assert!(h.signals.request_resize(arena().width,arena().height));assert!(!p.current(0));h.window.event(Geometry::Size(arena().width,arena().height));h.released().await;drop(p);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn real_profile_change_is_rechecked_after_preparation_without_life_effect(){let h=Host::new();let p=h.prepared().await;{let f=h.f.lock().unwrap();let mut changed=f.store.bot_profile("nova").unwrap();changed["avatar"]["scale"]=serde_json::json!(1.2);f.store.save_profile(changed).unwrap();}h.released().await;assert!(!p.current(0));drop(p);}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn matching_old_event_does_not_promote_unchanged_stale_getter(){
    let h=Host::new();h.window.delayed.store(true,Ordering::SeqCst);let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||h.window.pending.lock().unwrap().len()==2).await;
    h.window.signal_only(Geometry::Size(arena().width,arena().height));h.window.signal_only(Geometry::Move(arena().x,arena().y));
    assert_eq!(h.window.bounds().unwrap(),normal());h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());
    assert!(h.hub.native_guarded("nova"),"possibly queued geometry still owns cleanup");h.window.delayed.store(false,Ordering::SeqCst);h.window.flush();h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn failed_restore_is_guarded_until_exact_retirement_and_not_success(){
    let h=Host::new();let p=h.prepared().await;h.window.delayed.store(true,Ordering::SeqCst);let release=tokio::spawn(p.release());h.pump_until(||release.is_finished()).await;
    assert_eq!(release.await.unwrap(),native_layout::NativeLayoutRestoreOutcome::GuardedUnavailable);assert!(h.hub.native_guarded("nova"));assert!(h.hub.admit_dialog().is_err());
    h.routes.window_changed_with_output("nova",&h.signals,crate::life_runtime::NativeWindowChange::Destroyed,&h.outputs);h.hub.forget_window("nova",&h.signals);h.released().await;
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn delayed_os_event_cannot_hide_drift_between_fresh_sample_and_first_setter(){
    for after_click_guard in [false,true]{let h=Host::new();let task=tokio::spawn(native_layout::run_driver(h.intent()));h.step_one().await;h.wait_until(||h.persist_entered.load(Ordering::SeqCst)).await;h.step_one().await;if after_click_guard{h.step_one().await;}
        h.wait_until(||!h.main.queue.lock().unwrap().is_empty()).await;
        // Real getter changed, while its corresponding OS notification has not
        // arrived. An event-only cancellation test would miss this ordering.
        *h.window.bounds.lock().unwrap()=Bounds{x:-1100,y:140,..normal()};
        h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());h.released().await;assert_eq!(h.window.geometry_calls(),0);assert_eq!(h.window.bounds().unwrap().x,-1100);
    }
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn route_cleanup_is_registered_before_hub_admission_and_released_on_failure(){
    let h=Host::new();let weak=Arc::downgrade(&h);*h.before_reserve.lock().unwrap()=Some(Box::new(move|cancel|{let h=weak.upgrade().unwrap();h.routes.revoke(Some("nova"));assert!(cancel.load(Ordering::SeqCst),"pre-Hub registered cancellation must already observe Stop");assert!(!h.hub.any_native_work());}));
    let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());assert!(!h.hub.any_native_work());assert_eq!(h.window.geometry_calls(),0);
    // The failed reservation cannot leave one invisible cleanup slot charged.
    let ui=h.routes.layout_ui().unwrap();let mut held=Vec::new();
    for i in 0..8{let bot=format!("other{i}");let signals=Arc::new(WindowSignals::default());h.routes.track_window_with_output(&bot,&format!("avatar-{bot}"),&signals,0,&h.outputs).unwrap();let witness=h.routes.witness(&bot,&format!("avatar-{bot}"),&signals,0).unwrap();let ticket=h.routes.begin_preparation(witness,0).unwrap();let cancel=Arc::new(AtomicBool::new(false));let route=h.routes.retain_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();held.push((signals,ticket,route));}
    for (_,_,route) in &held{h.routes.release_layout(route);}
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn policy_saved_behind_queued_main_cannot_release_with_stale_click_or_top_values(){
    let h=Host::new();let p=h.prepared().await;drop(p);
    // Execute only earlier observations. Stop before the restore-policy task
    // whose receipt was read on the worker can run on our controlled Main.
    for _ in 0..1500{if h.policy_reads.load(Ordering::SeqCst)>0{break;}h.main.step();tokio::time::sleep(Duration::from_millis(1)).await;}
    h.wait_until(||h.policy_reads.load(Ordering::SeqCst)>0&&!h.main.queue.lock().unwrap().is_empty()).await;
    let held=h.main.queue.lock().unwrap().pop_front().unwrap();let before=h.window.policy_calls.lock().unwrap().len();
    {let f=h.f.lock().unwrap();let mut profile=f.store.bot_profile("nova").unwrap();profile["output"]["clickThrough"]=serde_json::json!(false);profile["output"]["alwaysOnTop"]=serde_json::json!(true);f.store.scoped("nova").unwrap().save_profile(profile).unwrap();}
    held();assert!(h.hub.any_native_work());assert!(h.window.policy_calls.lock().unwrap()[before..].iter().all(|&(name,value)|name!="top"&&(name!="clicks"||value)),"stale task may only keep click protection");
    h.released().await;let calls=h.window.policy_calls.lock().unwrap();assert_eq!(calls.iter().rev().find(|&&(kind,_)|kind=="top"),Some(&("top",true)));assert_eq!(calls.last(),Some(&("clicks",false)));
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn real_profile_commit_during_top_setter_reprotects_and_retries_latest_policy(){
    let h=Host::new();let p=h.prepared().await;let (store,mut profile)={let f=h.f.lock().unwrap();(f.store.scoped("nova").unwrap(),f.store.bot_profile("nova").unwrap())};profile["output"]["clickThrough"]=serde_json::json!(true);profile["output"]["alwaysOnTop"]=serde_json::json!(true);
    let (start_tx,start_rx)=std::sync::mpsc::channel();let (done_tx,done_rx)=std::sync::mpsc::channel();
    let writer=std::thread::spawn(move||{start_rx.recv_timeout(Duration::from_secs(3)).unwrap();store.save_profile(profile).unwrap();done_tx.send(()).unwrap();});
    *h.window.after_top.lock().unwrap()=Some(Box::new(move||{start_tx.send(()).unwrap();done_rx.recv_timeout(Duration::from_secs(3)).unwrap();}));drop(p);h.released().await;writer.join().unwrap();
    let calls=h.window.policy_calls.lock().unwrap();let first_top=calls.iter().position(|&(kind,_)|kind=="top").unwrap();assert_eq!(calls[first_top+1],("clicks",true),"after-setter mutation must immediately restore protection");assert_eq!(calls.iter().rev().find(|&&(kind,_)|kind=="top"),Some(&("top",true)));assert_eq!(calls.last(),Some(&("clicks",true)));
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn post_release_policy_resync_is_exact_instance_and_never_reuses_a_stale_receipt(){
    let h=Host::new();let p=h.prepared().await;drop(p);h.released().await;
    let exact=h.hub.native_handle("nova").unwrap();let old=h.policy().unwrap();
    {let f=h.f.lock().unwrap();let mut profile=f.store.bot_profile("nova").unwrap();profile["output"]["alwaysOnTop"]=serde_json::json!(!old.top);f.store.save_profile(profile).unwrap();}
    let before=h.window.policy_calls.lock().unwrap().len();assert_eq!(exact.resync_policy(old.clicks,old.top,||old.current()),Err(LayoutError::Stale));assert_eq!(&h.window.policy_calls.lock().unwrap()[before..],&[("clicks",true)]);
    let latest=h.policy().unwrap();exact.resync_policy(latest.clicks,latest.top,||latest.current()).unwrap();
    let successor=Arc::new(Window::new());let signals=Arc::new(WindowSignals::default());h.hub.register_window("nova",signals.clone(),None).unwrap();assert!(h.hub.retain_layout_window("nova",successor.clone(),signals,true));
    let before=h.window.policy_calls.lock().unwrap().len();assert_eq!(exact.resync_policy(latest.clicks,latest.top,||latest.current()),Err(LayoutError::Stale));assert_eq!(h.window.policy_calls.lock().unwrap().len(),before);assert!(successor.policy_calls.lock().unwrap().is_empty(),"retained old task cannot reach a successor through its label");
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn reserve_wait_cannot_extend_the_original_deadline_or_persist_after_it(){
    let h=Host::new();let weak=Arc::downgrade(&h);*h.before_reserve.lock().unwrap()=Some(Box::new(move|_|{weak.upgrade().unwrap().now.store(5001,Ordering::SeqCst);}));
    let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||task.is_finished()).await;assert!(matches!(task.await.unwrap(),Err(LayoutError::Expired)));h.released().await;assert!(!h.persist_entered.load(Ordering::SeqCst));assert_eq!(h.window.geometry_calls(),0);assert!(h.window.policy_calls.lock().unwrap().is_empty());
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn release_waits_for_confirmed_cleanup_not_the_main_released_phase(){
    let h=Host::new();let p=h.prepared().await;let entered=Arc::new(AtomicBool::new(false));let marker=entered.clone();let (resume_tx,resume_rx)=std::sync::mpsc::channel();
    *h.before_finish.lock().unwrap()=Some(Box::new(move||{marker.store(true,Ordering::SeqCst);resume_rx.recv_timeout(Duration::from_secs(3)).unwrap();}));
    let release=tokio::spawn(p.release());h.pump_until(||entered.load(Ordering::SeqCst)).await;
    assert!(h.hub.any_native_work());assert!(!release.is_finished(),"a Released Main phase is not yet an exact Hub/route cleanup acknowledgement");
    resume_tx.send(()).unwrap();h.pump_until(||release.is_finished()).await;assert_eq!(release.await.unwrap(),native_layout::NativeLayoutRestoreOutcome::Restored);assert!(!h.hub.any_native_work());
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn failed_finish_acknowledges_guarded_and_later_retirement_cannot_rewrite_it(){
    let h=Host::new();let p=h.prepared().await;h.fail_finish.store(true,Ordering::SeqCst);let release=tokio::spawn(p.release());h.pump_until(||release.is_finished()).await;
    assert_eq!(release.await.unwrap(),native_layout::NativeLayoutRestoreOutcome::GuardedUnavailable);assert!(h.hub.any_native_work());assert!(h.hub.admit_dialog().is_err());
    h.fail_finish.store(false,Ordering::SeqCst);h.routes.window_changed_with_output("nova",&h.signals,crate::life_runtime::NativeWindowChange::Destroyed,&h.outputs);h.hub.forget_window("nova",&h.signals);h.released().await;
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn prepared_currentness_blocks_during_raw_geometry_classification(){
    let h=Host::new();let p=h.prepared().await;let fact=h.routes.begin_geometry("nova",&h.signals,&h.outputs).unwrap();assert!(!p.current(0),"unchanged owner/getter cannot publish during the raw-event interval");
    h.routes.finish_geometry(fact,true);assert!(p.current(0));let fact=h.routes.begin_geometry("nova",&h.signals,&h.outputs).unwrap();h.routes.revoke(Some("nova"));h.routes.finish_geometry(fact,true);assert!(!p.current(0));drop(p);h.released().await;
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn route_release_failure_keeps_only_unavailable_bookkeeping_and_never_refinishes_hub(){
    let h=Host::new();let p=h.prepared().await;h.fail_release.store(true,Ordering::SeqCst);let release=tokio::spawn(p.release());h.pump_until(||release.is_finished()).await;
    let result=release.await.unwrap();assert_eq!(result,native_layout::NativeLayoutRestoreOutcome::GuardedUnavailable);assert_eq!(h.window.bounds().unwrap(),normal());assert!(!h.hub.any_native_work(),"successful physical/Hub release is not an active geometry guard");assert!(h.hub.admit_dialog().is_ok());assert_eq!(h.finish_calls.load(Ordering::SeqCst),1);
    let ui=h.routes.layout_ui().unwrap();let mut held=Vec::new();
    for i in 0..8{let bot=format!("remaining{i}");let signals=Arc::new(WindowSignals::default());h.routes.track_window_with_output(&bot,&format!("avatar-{bot}"),&signals,0,&h.outputs).unwrap();let witness=h.routes.witness(&bot,&format!("avatar-{bot}"),&signals,0).unwrap();let ticket=h.routes.begin_preparation(witness,0);if i==7{assert!(matches!(ticket,Err(crate::life_runtime::native_output_route::RouteError::Capacity)),"unreleased original route still occupies the eighth capacity slot");held.push((signals,None,None));}else{let ticket=ticket.unwrap();let cancel=Arc::new(AtomicBool::new(false));let route=h.routes.retain_preparation(&ticket,&signals,&cancel,&ui,0).unwrap();held.push((signals,Some(ticket),Some(route)));}}
    h.fail_release.store(false,Ordering::SeqCst);h.routes.window_changed_with_output("nova",&h.signals,crate::life_runtime::NativeWindowChange::Destroyed,&h.outputs);h.hub.forget_window("nova",&h.signals);
    h.wait_until(||h.release_calls.load(Ordering::SeqCst)>=2).await;assert_eq!(h.finish_calls.load(Ordering::SeqCst),1,"retirement retries only the retained route, never an already finished Hub owner");
    let signals=&held[7].0;let witness=h.routes.witness("remaining7","avatar-remaining7",signals,0).unwrap();let ticket=h.routes.begin_preparation(witness,0).unwrap();h.routes.cancel_preparation(&ticket);for (_,_,route) in held{if let Some(route)=route{assert!(h.routes.release_layout(&route));}}
    assert_eq!(result,native_layout::NativeLayoutRestoreOutcome::GuardedUnavailable,"later bookkeeping retirement does not rewrite the first unavailable result");
}


#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn held_owned_classification_before_getter_waits_in_both_event_orders(){
    for reverse in [false,true]{
        let h=Host::new();h.window.delayed.store(true,Ordering::SeqCst);
        let task=tokio::spawn(native_layout::run_driver(h.intent()));
        h.pump_until(||h.window.pending.lock().unwrap().len()==2).await;
        let mut events=h.window.pending.lock().unwrap().drain(..).collect::<Vec<_>>();if reverse{events.reverse();}
        h.window.event(events[0]);h.hold_classification.store(true,Ordering::SeqCst);h.window.event(events[1]);
        h.pump_until(||h.classifying_reads.load(Ordering::SeqCst)>0).await;
        assert_eq!(h.classifications.lock().unwrap().len(),1,"hold is after the genuine Hub field observation");
        assert_eq!(h.owned().phase(),crate::overlay_presentation::LayoutPhase::Expanding);
        assert!(!h.owned().cancel.load(Ordering::SeqCst));assert!(!task.is_finished(),"the real driver reached Classifying and must neither prepare nor abort");
        assert!(!h.f.lock().unwrap().core.world_output_current("nova",0));
        let samples=h.window.samples.load(Ordering::SeqCst);h.finish_classification(true);
        h.wait_until(||!h.main.queue.lock().unwrap().is_empty()).await;
        assert_eq!(h.window.samples.load(Ordering::SeqCst),samples,"finishing an event is not a getter");
        assert!(!task.is_finished());h.pump_until(||task.is_finished()).await;
        let p=task.await.unwrap().unwrap();assert!(h.window.samples.load(Ordering::SeqCst)>samples);assert!(p.current(0));
        h.window.delayed.store(false,Ordering::SeqCst);drop(p);h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
    }
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn classification_after_current_read_discards_sample_and_requires_later_getter(){
    let h=Host::new();h.window.delayed.store(true,Ordering::SeqCst);
    let task=tokio::spawn(native_layout::run_driver(h.intent()));
    h.pump_until(||h.window.pending.lock().unwrap().len()==2).await;
    let events=h.window.pending.lock().unwrap().drain(..).collect::<Vec<_>>();h.window.event(events[0]);
    let weak=Arc::downgrade(&h);let last=events[1];
    *h.after_route_read.lock().unwrap()=Some(Box::new(move||{let h=weak.upgrade().unwrap();h.hold_classification.store(true,Ordering::SeqCst);h.window.event(last);}));
    let before=h.window.samples.load(Ordering::SeqCst);
    h.pump_until(||h.classifying_reads.load(Ordering::SeqCst)>0).await;
    assert!(h.window.samples.load(Ordering::SeqCst)>before,"the first getter actually ran before the post-getter Classifying check");
    assert_eq!(h.owned().phase(),crate::overlay_presentation::LayoutPhase::Expanding);
    assert!(!task.is_finished());assert!(!h.owned().cancel.load(Ordering::SeqCst));
    let discarded=h.window.samples.load(Ordering::SeqCst);h.finish_classification(true);
    h.wait_until(||!h.main.queue.lock().unwrap().is_empty()).await;
    assert!(!task.is_finished(),"a held fresh Main getter prevents acceptance of the discarded sample");
    assert_eq!(h.window.samples.load(Ordering::SeqCst),discarded);
    h.pump_until(||task.is_finished()).await;let p=task.await.unwrap().unwrap();
    assert!(h.window.samples.load(Ordering::SeqCst)>discarded);assert!(p.current(0));
    h.window.delayed.store(false,Ordering::SeqCst);drop(p);h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn held_classification_rejection_stop_and_absolute_expiry_cannot_resume(){
    for reason in ["rejected","stop","expired"]{
        let h=Host::new();h.window.delayed.store(true,Ordering::SeqCst);
        let task=tokio::spawn(native_layout::run_driver(h.intent()));
        h.pump_until(||h.window.pending.lock().unwrap().len()==2).await;
        let events=h.window.pending.lock().unwrap().drain(..).collect::<Vec<_>>();h.window.event(events[0]);
        h.hold_classification.store(true,Ordering::SeqCst);h.window.event(events[1]);
        h.pump_until(||h.classifying_reads.load(Ordering::SeqCst)>0).await;
        assert!(!task.is_finished());assert!(!h.owned().cancel.load(Ordering::SeqCst));
        match reason{"stop"=>h.stop(),"expired"=>h.now.store(5001,Ordering::SeqCst),_=>{}}
        h.finish_classification(reason!="rejected");h.window.delayed.store(false,Ordering::SeqCst);
        h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());
        assert!(h.owned().cancel.load(Ordering::SeqCst));assert_ne!(h.owned().phase(),crate::overlay_presentation::LayoutPhase::Prepared);
        h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
    }
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn unmatched_geometry_is_terminal_even_if_classification_is_later_finished(){
    let h=Host::new();h.window.delayed.store(true,Ordering::SeqCst);
    let task=tokio::spawn(native_layout::run_driver(h.intent()));h.pump_until(||h.window.pending.lock().unwrap().len()==2).await;
    h.hold_classification.store(true,Ordering::SeqCst);h.window.signal_only(Geometry::Move(arena().x+1,arena().y));
    assert!(h.owned().cancel.load(Ordering::SeqCst));h.finish_classification(true);
    h.pump_until(||task.is_finished()).await;assert!(task.await.unwrap().is_err());
    assert!(h.owned().cancel.load(Ordering::SeqCst));h.window.delayed.store(false,Ordering::SeqCst);h.window.flush();h.released().await;
}


// Durable physical ownership/leases; Registry, document IPC and actual renderer
// admission remain root-owned. These genuine driver/Store fixtures mint no
// renderer measurement or action completion on behalf of a public caller.
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn pending_surface_proof_calibrates_only_and_drop_retains_exact_cleanup(){
    let h=Host::new();let before=h.f.lock().unwrap().store.load_life("nova").unwrap().unwrap().state().clone();
    let p=h.prepared().await;let g=p.begin_promotion(0).unwrap();let proof=g.proof();
    assert!(proof.preparing_current(0));assert!(!proof.live_current(0));assert!(!proof.current(0));assert!(proof.same_surface(&proof.clone()));
    assert_eq!(h.owned().phase(),crate::overlay_presentation::LayoutPhase::PromotionPending);
    assert!(!h.f.lock().unwrap().core.world_output_current("nova",0));
    drop(g);assert!(!proof.preparing_current(0));assert!(!proof.live_current(0));assert!(h.hub.any_native_work());
    h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
    assert_eq!(h.f.lock().unwrap().store.load_life("nova").unwrap().unwrap().state(),&before);
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn live_surface_passes_original_d5_only_with_strictly_new_successful_frame_leases(){
    let h=Host::new();let p=h.prepared().await;let g=p.begin_promotion(0).unwrap();let pending=g.proof();
    let live=g.commit_live(1,0,0).unwrap();let proof=live.proof();assert!(pending.same_surface(&proof));assert!(!pending.preparing_current(0));
    for n in 1..=12{let now=n*1000;h.now.store(now,Ordering::SeqCst);live.accepted_frame(n+1,now,now).unwrap();while h.main.step(){}assert!(proof.live_current(now));}
    assert_eq!(h.owned().deadline,5000,"Live does not rewrite preparation D5");assert!(h.hub.any_native_work());assert_eq!(h.window.bounds().unwrap(),arena());
    assert!(!h.f.lock().unwrap().core.world_output_current("nova",0),"physical promotion alone installs no Registry/action grant");
    h.now.store(17001,Ordering::SeqCst);assert!(!proof.live_current(17001));assert!(live.accepted_frame(99,17001,17001).is_err());
    drop(live);h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn repeated_backward_future_and_delayed_frames_never_renew_or_revive_live(){
    let h=Host::new();let live=h.prepared().await.begin_promotion(0).unwrap().commit_live(10,0,0).unwrap();
    h.now.store(100,Ordering::SeqCst);live.accepted_frame(11,100,100).unwrap();
    assert!(live.accepted_frame(11,101,101).is_err());assert!(live.accepted_frame(12,100,100).is_err());assert!(live.accepted_frame(12,99,100).is_err());
    assert!(live.accepted_frame(12,101,100).is_err());assert!(live.accepted_frame(12,101,500).is_err());
    h.now.store(5100,Ordering::SeqCst);assert!(live.current(5100));
    h.now.store(5101,Ordering::SeqCst);assert!(!live.current(5101));assert!(live.accepted_frame(12,5101,5101).is_err());
    drop(live);h.released().await;
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn expired_pending_or_stale_first_calibration_cannot_win_cleanup_publication(){
    for stale in [false,true]{
        let h=Host::new();let g=h.prepared().await.begin_promotion(0).unwrap();let proof=g.proof();
        let now=if stale{251}else{5001};h.now.store(now,Ordering::SeqCst);
        if !stale{assert!(h.owned().cleanup_required(now));}
        assert!(g.commit_live(1,0,now).is_err());assert!(!proof.live_current(now));assert!(!proof.preparing_current(now));
        h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
    }
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn live_stop_and_owner_drop_invalidate_all_read_only_clones_without_freeing_guard_early(){
    for explicit in [false,true]{
        let h=Host::new();let live=h.prepared().await.begin_promotion(0).unwrap().commit_live(1,0,0).unwrap();let proof=live.proof();let retained=proof.clone();
        if explicit{h.stop();assert!(!proof.current(0));}
        drop(live);assert!(!proof.current(0));assert!(!retained.current(0));assert!(h.hub.any_native_work());assert!(h.hub.admit_dialog().is_err());
        h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());assert!(!retained.current(0));
    }
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn raw_geometry_after_live_revokes_even_if_a_late_classifier_claims_success(){
    let h=Host::new();let live=h.prepared().await.begin_promotion(0).unwrap().commit_live(1,0,0).unwrap();let proof=live.proof();
    let fact=h.routes.begin_geometry("nova",&h.signals,&h.outputs).unwrap();assert!(!proof.current(0));
    h.routes.finish_geometry(fact,true);assert!(!proof.current(0));assert!(live.accepted_frame(2,1,1).is_err());
    drop(live);h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn live_release_ack_waits_for_real_finish_and_keeps_failed_finish_guarded(){
    let h=Host::new();let live=h.prepared().await.begin_promotion(0).unwrap().commit_live(1,0,0).unwrap();
    let (entered_tx,entered_rx)=std::sync::mpsc::channel();let barrier=Arc::new(std::sync::Barrier::new(2));let held=barrier.clone();
    *h.before_finish.lock().unwrap()=Some(Box::new(move||{entered_tx.send(()).unwrap();held.wait();}));
    let release=tokio::spawn(live.release());
    h.pump_until(||entered_rx.try_recv().is_ok()).await;
    assert!(!release.is_finished());assert!(h.hub.any_native_work());barrier.wait();
    h.pump_until(||release.is_finished()).await;assert_eq!(release.await.unwrap(),native_layout::NativeLayoutRestoreOutcome::Restored);
    let h=Host::new();let live=h.prepared().await.begin_promotion(0).unwrap().commit_live(1,0,0).unwrap();h.fail_finish.store(true,Ordering::SeqCst);
    let release=tokio::spawn(live.release());h.pump_until(||release.is_finished()).await;assert_eq!(release.await.unwrap(),native_layout::NativeLayoutRestoreOutcome::GuardedUnavailable);assert!(h.hub.any_native_work());
    h.fail_finish.store(false,Ordering::SeqCst);h.hub.forget_window("nova",&h.signals);h.released().await;
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn session_layout_permission_needs_exact_arc_but_not_studio_connection(){
    let h=Host::new();h.routes.ui_disconnect("studio");let session=Arc::new(AtomicBool::new(false));let foreign=Arc::new(AtomicBool::new(false));
    let ui=h.routes.session_layout(&session).unwrap();let wrong=h.routes.session_layout(&foreign).unwrap();let handle=h.hub.native_handle("nova").unwrap();
    let witness=h.routes.witness("nova","avatar-nova",&h.signals,0).unwrap();let ticket=h.routes.begin_preparation(witness,0).unwrap();let cancel=Arc::new(AtomicBool::new(false));
    let route=h.routes.retain_preparation(&ticket,&h.signals,&cancel,&ui,0).unwrap();let owner=h.hub.reserve_native(&handle,&handle.sample().unwrap(),0,5000,cancel.clone()).unwrap();
    assert!(h.routes.handoff_preparation(&ticket,&h.signals,&cancel,&wrong,0).is_err());
    h.routes.handoff_preparation(&ticket,&h.signals,&cancel,&ui,0).unwrap();assert!(h.routes.layout_current(&route,0));
    session.store(true,Ordering::SeqCst);assert!(!h.routes.layout_current(&route,0));assert!(cancel.load(Ordering::SeqCst));assert!(!foreign.load(Ordering::SeqCst));
    owner.begin_restore().unwrap();assert!(owner.restore_once(false,false,||true).unwrap());assert!(h.hub.finish_native(&owner));assert!(h.routes.release_layout(&route));
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn pending_promotion_release_waits_for_exact_hub_and_route_completion(){
    let h=Host::new();let g=h.prepared().await.begin_promotion(0).unwrap();let proof=g.proof();
    let release=tokio::spawn(g.release());h.pump_until(||h.owned().cancel.load(Ordering::SeqCst)).await;
    assert!(!proof.preparing_current(0));assert!(!proof.live_current(0));
    h.pump_until(||release.is_finished()).await;
    assert_eq!(release.await.unwrap(),native_layout::NativeLayoutRestoreOutcome::Restored);
    assert!(!h.hub.any_native_work());assert_eq!(h.window.bounds().unwrap(),normal());assert_eq!(h.finish_calls.load(Ordering::SeqCst),1);assert_eq!(h.release_calls.load(Ordering::SeqCst),1);
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn promotion_keeps_the_identical_normal_owner_and_never_frees_capacity_at_live(){
    let h=Host::new();let p=h.prepared().await;let original=h.owned();let handle=h.hub.native_handle("nova").unwrap();
    let g=p.begin_promotion(0).unwrap();assert!(Arc::ptr_eq(&original,&h.owned()));
    let live=g.commit_live(1,0,0).unwrap();assert!(Arc::ptr_eq(&original,&h.owned()));
    assert!(matches!(h.hub.reserve_native(&handle,&handle.sample().unwrap(),0,5000,Arc::new(AtomicBool::new(false))),Err(LayoutError::Busy)));
    assert!(!h.hub.finish_native(&original),"a Live owner has not acknowledged restoration");
    assert!(live.current(0));drop(live);assert!(h.hub.any_native_work());h.released().await;assert_eq!(h.window.bounds().unwrap(),normal());
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn queued_prepared_observation_drains_through_pending_without_false_abort_or_grant(){
    let h=Host::new();let p=h.prepared().await;
    h.wait_until(||h.owned().in_flight()).await;assert!(!h.main.queue.lock().unwrap().is_empty());
    let g=p.begin_promotion(0).unwrap();let proof=g.proof();assert!(!proof.preparing_current(0));assert!(!proof.live_current(0));
    assert!(h.main.step(),"drain the actual previously queued read-only getter");h.wait_until(||!h.owned().in_flight()).await;
    assert!(proof.preparing_current(0));assert!(!h.owned().cancel.load(Ordering::SeqCst));
    let live=g.commit_live(1,0,0).unwrap();assert!(live.current(0));drop(live);h.released().await;
}
