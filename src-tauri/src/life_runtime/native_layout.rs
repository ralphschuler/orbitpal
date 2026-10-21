//! Private constructor-bound native layout driver. Preparation is not reachable
//! from ordinary overlay opening, IPC, model tools or renderer messages.
//! Preparation stays non-authorizing. One-use promotion follows real Registry
//! calibration; only the root transport may publish Ready or admit an action.
use super::{action_state::LocalFence,core::Core,native_output_route::{LayoutRoute,LayoutRouteState,LayoutUi,PreparationTicket},LifeRuntime};
use crate::{overlay_presentation::{Bounds,LayoutError,LayoutHandle,LayoutMain,LayoutOwner,LayoutPhase,LayoutSample,NativeLayoutMain},storage::Store,world_hub::{ActionContext,WorldHub},world_state::SessionStamp};
use serde_json::Value;
use std::{sync::{atomic::{AtomicBool,Ordering},Arc,Mutex,Weak},time::Duration};
use tauri::Manager;

pub(crate) struct NativeLayoutIntent {host:Arc<dyn LayoutHost>,ticket:PreparationTicket,handle:LayoutHandle}
pub(crate) struct NativeLayoutPrepared {
    pub(crate) bounds:Bounds,
    pub(crate) scale:f64,
    pub(crate) reference_physical_height:u32,
    release:NativeLayoutRelease,
}
/// Drop requests cleanup; it does not clear the guard before Main work drains.
pub(crate) struct NativeLayoutRelease {owner:Arc<LayoutOwner>,host:Arc<dyn LayoutHost>,route:LayoutRoute,completed:Arc<CleanupCompletion>}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum NativeLayoutRestoreOutcome {ReleasedWithoutGeometry,Restored,Retired,GuardedUnavailable}
impl Drop for NativeLayoutRelease {fn drop(&mut self){self.owner.cancelled();}}
impl NativeLayoutPrepared {
    pub(crate) fn current(&self,now:u64)->bool{self.release.owner.valid(now)&&self.release.owner.phase()==LayoutPhase::Prepared&&self.release.owner.getter_fence().is_ok()&&self.release.host.route_current(&self.release.route)}
    pub(crate) async fn release(self)->NativeLayoutRestoreOutcome{self.release.owner.cancelled();loop{let notified=self.release.completed.changed.notified();if let Some(result)=self.release.completed.result(){return result;}notified.await;}}
}
/// The cell is shared by the existing owner and route; it owns no Normal,
/// cleanup or independent cancellation identity. Only the owning live handle
/// can advance successful-frame evidence after Registry validation.
const LIVE_FRAME_MS:u64=5_000;
const ADMISSION_FRAME_MS:u64=250;
#[derive(Clone,Copy,PartialEq,Eq)]
enum SurfacePhase{Pending,Live,Revoked}
struct SurfaceState{phase:SurfacePhase,frame:u64,received:u64,admitted:u64,geometry_current:bool}
pub(crate) struct SurfaceCell{cancel:Arc<AtomicBool>,issued:u64,deadline:u64,state:Mutex<SurfaceState>}
impl SurfaceCell{
    fn new(owner:&LayoutOwner)->Arc<Self>{Arc::new(Self{cancel:owner.cancel.clone(),issued:owner.issued,deadline:owner.deadline,state:Mutex::new(SurfaceState{phase:SurfacePhase::Pending,frame:0,received:0,admitted:0,geometry_current:true})})}
    fn lock(&self)->Result<std::sync::MutexGuard<'_,SurfaceState>,LayoutError>{self.state.lock().map_err(|_|{self.cancel.store(true,Ordering::SeqCst);LayoutError::Unavailable})}
    pub(crate) fn matches(&self,cancel:&Arc<AtomicBool>,issued:u64,deadline:u64)->bool{Arc::ptr_eq(&self.cancel,cancel)&&self.issued==issued&&self.deadline==deadline}
    pub(crate) fn retained_current(&self,now:u64)->bool{
        if self.cancel.load(Ordering::SeqCst)||now<self.issued{return false;}
        self.lock().is_ok_and(|s|match s.phase{SurfacePhase::Pending=>now<=self.deadline,SurfacePhase::Live=>now>=s.admitted&&now>=s.received&&now-s.received<=LIVE_FRAME_MS,SurfacePhase::Revoked=>false})
    }
    pub(crate) fn pending_current(&self,now:u64)->bool{self.retained_current(now)&&self.lock().is_ok_and(|s|s.phase==SurfacePhase::Pending&&s.geometry_current)}
    pub(crate) fn live_current(&self,now:u64)->bool{self.retained_current(now)&&self.lock().is_ok_and(|s|s.phase==SurfacePhase::Live&&s.geometry_current)}
    pub(crate) fn invalidate_geometry(&self){if let Ok(mut s)=self.lock(){s.geometry_current=false;if s.phase==SurfacePhase::Live{s.phase=SurfacePhase::Revoked;self.cancel.store(true,Ordering::SeqCst);}}}
    pub(crate) fn confirm_geometry(&self,now:u64)->Result<(),LayoutError>{if !self.retained_current(now){return Err(LayoutError::Expired);}let mut s=self.lock()?;if s.phase==SurfacePhase::Revoked{return Err(LayoutError::Stale);}s.geometry_current=true;Ok(())}
    pub(crate) fn revoke(&self){self.cancel.store(true,Ordering::SeqCst);if let Ok(mut s)=self.state.lock(){s.phase=SurfacePhase::Revoked;}}
    pub(crate) fn commit(&self,frame:u64,received:u64,now:u64,clock:&impl Fn()->u64)->Result<(),LayoutError>{
        let mut s=self.lock()?;let now=now.max(clock());
        if s.phase!=SurfacePhase::Pending||!s.geometry_current||self.cancel.load(Ordering::SeqCst)||now<self.issued||now>self.deadline||received<self.issued||received>now||now-received>ADMISSION_FRAME_MS||frame==0||frame>crate::life_domain::MAX_COUNTER{return Err(LayoutError::Expired);}
        s.phase=SurfacePhase::Live;s.frame=frame;s.received=received;s.admitted=now;Ok(())
    }
    fn accepted(&self,frame:u64,received:u64,now:u64,clock:&impl Fn()->u64)->Result<(),LayoutError>{
        let mut s=self.lock()?;let now=now.max(clock());
        if s.phase!=SurfacePhase::Live||self.cancel.load(Ordering::SeqCst)||now<s.admitted||now<s.received||now-s.received>LIVE_FRAME_MS||frame<=s.frame||frame>crate::life_domain::MAX_COUNTER||received<=s.received||received>now||now-received>ADMISSION_FRAME_MS{return Err(LayoutError::Expired);}
        s.frame=frame;s.received=received;s.admitted=now;Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct NativeSurfaceProof{owner:Weak<LayoutOwner>,host:Weak<dyn LayoutHost>,route:LayoutRoute,cell:Arc<SurfaceCell>}
impl std::fmt::Debug for NativeSurfaceProof{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("NativeSurfaceProof")}}
impl NativeSurfaceProof{
    pub(crate) fn owned_by(&self,owner:&Arc<LayoutOwner>)->bool{Weak::ptr_eq(&self.owner,&Arc::downgrade(owner))}
    pub(crate) fn same_surface(&self,other:&Self)->bool{Weak::ptr_eq(&self.owner,&other.owner)&&Arc::ptr_eq(&self.cell,&other.cell)}
    pub(crate) fn preparing_current(&self,now:u64)->bool{self.owner.upgrade().is_some_and(|owner|owner.promotion_current(&self.cell,now))&&self.host.upgrade().is_some_and(|host|host.route_current(&self.route))}
    pub(crate) fn live_current(&self,now:u64)->bool{self.owner.upgrade().is_some_and(|owner|owner.live_current(&self.cell,now))&&self.host.upgrade().is_some_and(|host|host.route_current(&self.route))}
    pub(crate) fn current(&self,now:u64)->bool{self.live_current(now)}
}
pub(crate) struct PromotionGuard{pub(crate) bounds:Bounds,pub(crate) scale:f64,pub(crate) reference_physical_height:u32,release:NativeLayoutRelease,cell:Arc<SurfaceCell>}
pub(crate) struct LiveNativeSurface{pub(crate) bounds:Bounds,pub(crate) scale:f64,pub(crate) reference_physical_height:u32,release:NativeLayoutRelease,cell:Arc<SurfaceCell>}
fn surface_proof(release:&NativeLayoutRelease,cell:&Arc<SurfaceCell>)->NativeSurfaceProof{NativeSurfaceProof{owner:Arc::downgrade(&release.owner),host:Arc::downgrade(&release.host),route:release.route.clone(),cell:cell.clone()}}
impl NativeLayoutPrepared{
    pub(crate) fn begin_promotion(self,now:u64)->Result<PromotionGuard,LayoutError>{
        let now=now.max(self.release.host.now());
        if !self.release.owner.valid(now)||self.release.owner.phase()!=LayoutPhase::Prepared||self.release.host.route_state(&self.release.route)==LayoutRouteState::Revoked{return Err(LayoutError::Stale);}
        let cell=SurfaceCell::new(&self.release.owner);
        self.release.owner.begin_promotion(&cell,now)?;
        self.release.host.stage_surface(&self.release.route,&cell)?;
        if !self.release.owner.pending_retained(&cell,now)||self.release.host.route_state(&self.release.route)==LayoutRouteState::Revoked{return Err(LayoutError::Stale);}
        let Self{bounds,scale,reference_physical_height,release}=self;
        Ok(PromotionGuard{bounds,scale,reference_physical_height,release,cell})
    }
}
impl PromotionGuard{
    pub(crate) async fn release(self)->NativeLayoutRestoreOutcome{self.release.owner.cancelled();loop{let notified=self.release.completed.changed.notified();if let Some(result)=self.release.completed.result(){return result;}notified.await;}}
    pub(crate) fn proof(&self)->NativeSurfaceProof{surface_proof(&self.release,&self.cell)}
    /// Caller has accepted the genuine calibration in Registry and rechecked
    /// actual context/time under Core. No public/action Ready before this call.
    pub(crate) fn commit_live(self,frame_sequence:u64,received:u64,now:u64)->Result<LiveNativeSurface,LayoutError>{
        let now=now.max(self.release.host.now());
        if !self.release.host.route_current(&self.release.route){return Err(LayoutError::Stale);}
        self.release.owner.commit_live(&self.cell,frame_sequence,received,now,&||self.release.host.now())?;
        if !self.release.host.route_current(&self.release.route){return Err(LayoutError::Stale);}
        let Self{bounds,scale,reference_physical_height,release,cell}=self;
        Ok(LiveNativeSurface{bounds,scale,reference_physical_height,release,cell})
    }
}
impl LiveNativeSurface{
    pub(crate) fn proof(&self)->NativeSurfaceProof{surface_proof(&self.release,&self.cell)}
    pub(crate) fn current(&self,now:u64)->bool{self.proof().live_current(now)}
    /// Called only AFTER exact Registry accepted a newer successful frame.
    /// A failure leaves previous evidence unchanged, never resurrects expiry.
    pub(crate) fn accepted_frame(&self,frame_sequence:u64,received:u64,now:u64)->Result<(),LayoutError>{
        let now=now.max(self.release.host.now());
        if !self.current(now){return Err(LayoutError::Stale);}
        self.cell.accepted(frame_sequence,received,now,&||self.release.host.now())?;
        if !self.current(now){return Err(LayoutError::Stale);}Ok(())
    }
    pub(crate) async fn release(self)->NativeLayoutRestoreOutcome{self.release.owner.cancelled();loop{let notified=self.release.completed.changed.notified();if let Some(result)=self.release.completed.result(){return result;}notified.await;}}
}
/// Phase Released is only a Main observation; the permit/Hub/route can still
/// be retained then. Only cleanup publishes the first terminal acknowledgement.
#[derive(Default)]
struct CleanupCompletion {outcome:std::sync::Mutex<Option<NativeLayoutRestoreOutcome>>,changed:tokio::sync::Notify}
impl CleanupCompletion {
    fn result(&self)->Option<NativeLayoutRestoreOutcome>{self.outcome.lock().map(|value|*value).unwrap_or(Some(NativeLayoutRestoreOutcome::GuardedUnavailable))}
    fn complete(&self,result:NativeLayoutRestoreOutcome){if let Ok(mut value)=self.outcome.lock(){if value.is_none(){*value=Some(result);}}self.changed.notify_waiters();}
}
pub(crate) struct LayoutContext {bot:String,profile:Value,local:LocalFence,world:ActionContext,ui:LayoutUi,monitor:String}
impl LayoutContext {pub(crate) fn monitor(&self)->&str{&self.monitor}}
impl NativeLayoutIntent {pub(crate) fn new(host:Arc<dyn LayoutHost>,ticket:PreparationTicket,handle:LayoutHandle)->Self{Self{host,ticket,handle}}}
/// Shared real Core -> WorldHub -> Store context path, also used by fixtures.
pub(crate) fn context(core:&Core,store:&Store,world:&WorldHub,bot:&str,ui:LayoutUi,ui_connection:&str,now:u64)->Result<LayoutContext,LayoutError>{
    if core.local_action_reason(bot,now).is_some()||core.world_ui_connection(now)!=Some(ui_connection){return Err(LayoutError::Stale);}
    let profile=store.bot_profile(bot).map_err(|_|LayoutError::Persistence)?;
    if profile["output"]["mode"]!="overlay"||profile["output"]["streamingEnabled"]!=true||store.profile().map_err(|_|LayoutError::Persistence)?["id"]!=bot{return Err(LayoutError::Stale);}
    let local=core.local_fence(bot,now).ok_or(LayoutError::Stale)?;
    let session=core.sessions.get(bot).ok_or(LayoutError::Stale)?;
    let life=session.state.as_ref().ok_or(LayoutError::Stale)?;
    let stamp=SessionStamp::new(crate::life_domain::ActorId::new(bot).map_err(|_|LayoutError::Stale)?,&session.session_id,session.revision).map_err(|_|LayoutError::Stale)?;
    let (world,_)=world.registration_context(store,&stamp,life.world_id()).map_err(|_|LayoutError::Stale)?;
    let monitor=world.record["outputBinding"]["monitorId"].as_str().filter(|id|!id.is_empty()).ok_or(LayoutError::Geometry)?.to_owned();
    Ok(LayoutContext{bot:bot.into(),profile,local,world,ui,monitor})
}
pub(crate) fn context_current(expected:&LayoutContext,core:&Core,store:&Store,world:&WorldHub,now:u64)->bool{
    if !core.local_current(&expected.bot,&expected.local,now)||core.local_action_reason(&expected.bot,now).is_some(){return false;}
    let Ok(profile)=store.bot_profile(&expected.bot)else{return false;};
    if profile!=expected.profile||!store.profile().is_ok_and(|p|p["id"]==expected.bot){return false;}
    world.registration_context(store,&expected.world.session,&expected.world.source_world).is_ok_and(|(current,_)|current==expected.world)
}
/// Only now() may run under owner/cell guards: it MUST be a nonblocking native
/// monotonic clock read, with no Core/Store/GUI/registry access. Other methods
/// are never called under those guards. Implementations do not put GUI
/// calls in validate/persist; all GUI work uses the separately scheduled runner.
pub(crate) trait LayoutHost:Send+Sync {
    fn now(&self)->u64;
    fn main(&self)->Arc<dyn LayoutMain>;
    fn refresh(&self)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<(),LayoutError>>+Send+'_>>;
    fn validate(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>;
    fn reserve(&self,handle:&LayoutHandle,sample:&LayoutSample,issued:u64,deadline:u64,cancel:Arc<AtomicBool>)->Result<Arc<LayoutOwner>,LayoutError>;
    fn persist(&self,bounds:Bounds,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>;
    fn attach(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>;
    fn retain(&self,ticket:&PreparationTicket,handle:&LayoutHandle,cancel:&Arc<AtomicBool>)->Result<LayoutRoute,LayoutError>;
    fn handoff(&self,ticket:&PreparationTicket,owner:&Arc<LayoutOwner>)->Result<LayoutRoute,LayoutError>;
    fn route_state(&self,route:&LayoutRoute)->LayoutRouteState;
    fn route_current(&self,route:&LayoutRoute)->bool{self.route_state(route)==LayoutRouteState::Current}
    fn stage_surface(&self,route:&LayoutRoute,cell:&Arc<SurfaceCell>)->Result<(),LayoutError>;
    fn validate_live(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>;
    fn release_route(&self,route:&LayoutRoute)->bool;
    fn finish(&self,owner:&Arc<LayoutOwner>)->bool;
    fn policy(&self)->Option<crate::storage::profile_mutation::NativeLayoutPolicy>;
    fn monitor(&self)->&str;
    fn save_current(&self,handle:&LayoutHandle,bounds:Bounds);
}
struct NativeHost {app:tauri::AppHandle,expected:LayoutContext}
impl LayoutHost for NativeHost {
    fn now(&self)->u64{self.app.state::<crate::AppState>().life_runtime.now()}
    fn main(&self)->Arc<dyn LayoutMain>{Arc::new(NativeLayoutMain(self.app.clone()))}
    fn refresh(&self)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<(),LayoutError>>+Send+'_>>{Box::pin(async move{let state=self.app.state::<crate::AppState>();let monitor=state.life_runtime.monitor.get().ok_or(LayoutError::Unavailable)?;let sample=monitor.refresh().await;if sample.state!=crate::life_availability::Availability::AwakeUnlocked||sample.generation!=self.expected.local.system_generation{return Err(LayoutError::Stale);}Ok(())})}
    fn validate(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{
        let state=self.app.state::<crate::AppState>();
        state.updater.ensure_idle().map_err(|_|LayoutError::Busy)?;
        if cancel.load(Ordering::SeqCst)||state.overlay.has_any_pending_speech()||state.generation.try_lock().is_err(){return Err(LayoutError::Stale);}
        let core=state.life_runtime.core.lock().map_err(|_|LayoutError::Unavailable)?;
        if !context_current(&self.expected,&core,&state.store,&state.world,self.now())||cancel.load(Ordering::SeqCst)||state.generation.try_lock().is_err(){return Err(LayoutError::Stale);}
        Ok(())
    }
    fn reserve(&self,handle:&LayoutHandle,sample:&LayoutSample,issued:u64,deadline:u64,cancel:Arc<AtomicBool>)->Result<Arc<LayoutOwner>,LayoutError>{self.app.state::<crate::AppState>().presentation.reserve_native(handle,sample,issued,deadline,cancel)}
    fn persist(&self,bounds:Bounds,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{
        self.validate(cancel)?;
        let state=self.app.state::<crate::AppState>();
        state.store.scoped(&self.expected.bot).map_err(|_|LayoutError::Persistence)?.save_window_bounds(&serde_json::to_value(bounds).map_err(|_|LayoutError::Persistence)?).map_err(|_|LayoutError::Persistence)?;
        self.validate(cancel)
    }
    fn attach(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{self.app.state::<crate::AppState>().life_runtime.monitor.get().and_then(|monitor|monitor.gate.watch(cancel)).map(|_|()).ok_or(LayoutError::Unavailable)}
    fn retain(&self,ticket:&PreparationTicket,handle:&LayoutHandle,cancel:&Arc<AtomicBool>)->Result<LayoutRoute,LayoutError>{self.app.state::<crate::AppState>().life_runtime.native_routes.retain_preparation(ticket,handle.signals(),cancel,&self.expected.ui,self.now()).map_err(|_|LayoutError::Stale)}
    fn handoff(&self,ticket:&PreparationTicket,owner:&Arc<LayoutOwner>)->Result<LayoutRoute,LayoutError>{self.app.state::<crate::AppState>().life_runtime.native_routes.handoff_preparation(ticket,owner.handle().signals(),&owner.cancel,&self.expected.ui,self.now()).map_err(|_|LayoutError::Stale)}
    fn route_state(&self,route:&LayoutRoute)->LayoutRouteState{self.app.state::<crate::AppState>().life_runtime.native_routes.layout_state(route,self.now())}
    fn stage_surface(&self,route:&LayoutRoute,cell:&Arc<SurfaceCell>)->Result<(),LayoutError>{self.app.state::<crate::AppState>().life_runtime.native_routes.stage_surface(route,cell,self.now()).map_err(|_|LayoutError::Stale)}
    fn validate_live(&self,cancel:&Arc<AtomicBool>)->Result<(),LayoutError>{self.validate(cancel)}
    fn release_route(&self,route:&LayoutRoute)->bool{self.app.state::<crate::AppState>().life_runtime.native_routes.release_layout(route)}
    fn finish(&self,owner:&Arc<LayoutOwner>)->bool{let state=self.app.state::<crate::AppState>();let finished=state.presentation.finish_native(owner);if finished&&owner.handle().current(){crate::overlay::configure_after_native_release(&self.app,owner.handle().clone());crate::overlay_presentation::sync_after_native_release(&self.app,owner.handle().bot(),owner.handle().signals().clone());}finished}
    fn policy(&self)->Option<crate::storage::profile_mutation::NativeLayoutPolicy>{self.app.state::<crate::AppState>().store.native_layout_policy(&self.expected.bot)}
    fn monitor(&self)->&str{&self.expected.monitor}
    fn save_current(&self,handle:&LayoutHandle,bounds:Bounds){let state=self.app.state::<crate::AppState>();if handle.current()&&!state.presentation.suppress_bounds(handle.bot()){if let Ok(store)=state.store.scoped(handle.bot()){let _=store.save_window_bounds(&serde_json::to_value(bounds).unwrap_or(Value::Null));}}}
}
impl LifeRuntime {
    /// Future explicit native main-operator boundary only; deliberately no IPC
    /// or call from ordinary open_for_bot. Ticket/time precede metadata waits.
    pub(crate) fn native_layout_intent(&self,app:&tauri::AppHandle,bot:&str)->Result<NativeLayoutIntent,LayoutError>{
        let issued=self.now();let state=app.state::<crate::AppState>();
        let handle=state.presentation.native_handle(bot)?;
        let witness=self.native_routes.witness(bot,&format!("avatar-{bot}"),handle.signals(),issued).map_err(|_|LayoutError::Stale)?;
        let ticket=self.native_routes.begin_preparation(witness,issued).map_err(|_|LayoutError::Busy)?;
        let ui=self.native_routes.layout_ui().map_err(|_|LayoutError::Stale)?;
        let expected={let core=self.core.lock().map_err(|_|LayoutError::Unavailable)?;context(&core,&state.store,&state.world,bot,ui.clone(),self.native_routes.layout_ui_connection(&ui),self.now())?};
        if self.now()>ticket.deadline(){return Err(LayoutError::Expired);}
        state.updater.ensure_idle().map_err(|_|LayoutError::Busy)?;
        Ok(NativeLayoutIntent::new(Arc::new(NativeHost{app:app.clone(),expected}),ticket,handle))
    }
}
pub(crate) async fn prepare_layout(intent:NativeLayoutIntent)->Result<NativeLayoutPrepared,LayoutError>{
    // Always begin on a worker. Wry run_on_main_thread may run inline on Main.
    tauri::async_runtime::spawn(run_driver(intent)).await.map_err(|_|LayoutError::Unavailable)?
}
async fn initial_sample(host:Arc<dyn LayoutHost>,handle:LayoutHandle,cancel:Arc<AtomicBool>,deadline:u64)->Result<LayoutSample,LayoutError>{
    let (tx,rx)=tokio::sync::oneshot::channel();let current=host.clone();
    host.main().enqueue(Box::new(move||{let result=if current.now()>deadline||cancel.load(Ordering::SeqCst)||!handle.current(){Err(LayoutError::Expired)}else{handle.sample()};let _=tx.send(result);}))?;
    let remaining=deadline.checked_sub(host.now()).ok_or(LayoutError::Expired)?;
    tokio::time::timeout(Duration::from_millis(remaining),rx).await.map_err(|_|LayoutError::Expired)?.map_err(|_|LayoutError::Dispatch)?
}
async fn main_job<T:Send+'static>(host:Arc<dyn LayoutHost>,owner:Arc<LayoutOwner>,cleanup:bool,operation:impl FnOnce(&Arc<LayoutOwner>)->Result<T,LayoutError>+Send+'static)->Result<T,LayoutError>{
    let permit=owner.begin_main(cleanup,host.now())?;
    let (tx,rx)=tokio::sync::oneshot::channel();let current=host.clone();let job_owner=owner.clone();
    host.main().enqueue(Box::new(move||{let result=if permit.allowed(cleanup,current.now()){operation(&job_owner)}else{Err(LayoutError::Expired)};drop(permit);let _=tx.send(result);}))?;
    let wait=if cleanup{1000}else{owner.observation_wait(host.now()).ok_or(LayoutError::Expired)?};
    tokio::time::timeout(Duration::from_millis(wait),rx).await.map_err(|_|LayoutError::Expired)?.map_err(|_|LayoutError::Dispatch)?
}
struct CleanupState {route:std::sync::Mutex<Option<LayoutRoute>>,preparing:AtomicBool,ready:std::sync::Mutex<Option<LayoutSample>>}
/// Charge route capacity before a Hub owner can exist. A failed Hub admission
/// releases only this exact inactive record, without a geometry operation.
struct RetainGuard {host:Arc<dyn LayoutHost>,route:Option<LayoutRoute>}
impl Drop for RetainGuard {fn drop(&mut self){if let Some(route)=self.route.take(){self.host.release_route(&route);}}}
struct PrepareGuard {owner:Arc<LayoutOwner>,cleanup:Arc<CleanupState>,armed:bool}
impl Drop for PrepareGuard {fn drop(&mut self){if self.armed{self.owner.cancelled();}self.cleanup.preparing.store(false,Ordering::SeqCst);}}
pub(crate) async fn run_driver(intent:NativeLayoutIntent)->Result<NativeLayoutPrepared,LayoutError>{
    let NativeLayoutIntent{host,ticket,handle}=intent;
    let initial_cancel=ticket.cancel_atom();
    host.refresh().await?;
    host.validate(&initial_cancel)?;
    let initial=initial_sample(host.clone(),handle.clone(),initial_cancel.clone(),ticket.deadline()).await?;
    let target=initial.target(host.monitor())?;
    let cancel=Arc::new(AtomicBool::new(false));
    host.attach(&cancel)?;
    let route=host.retain(&ticket,&handle,&cancel)?;
    let mut retained=RetainGuard{host:host.clone(),route:Some(route)};
    let owner=host.reserve(&handle,&initial,ticket.issued_at(),ticket.deadline(),cancel)?;
    let cleanup_done=Arc::new(CleanupCompletion::default());
    let cleanup_state=Arc::new(CleanupState{route:std::sync::Mutex::new(retained.route.take()),preparing:AtomicBool::new(true),ready:std::sync::Mutex::new(None)});
    let mut guard=PrepareGuard{owner:owner.clone(),cleanup:cleanup_state.clone(),armed:true};
    // Registered owner/cleanup lease precedes every metadata wait and handoff.
    tokio::spawn(cleanup(host.clone(),owner.clone(),cleanup_state.clone(),cleanup_done.clone()));
    if !owner.valid(host.now()){return Err(LayoutError::Expired);}
    host.persist(initial.bounds,&owner.cancel)?;
    let sampled=main_job(host.clone(),owner.clone(),false,|o|o.handle().sample()).await?;
    if sampled.bounds!=initial.bounds||!sampled.same_topology(&initial)||sampled.target(host.monitor())?!=target{return Err(LayoutError::Geometry);}
    host.validate(&owner.cancel)?;
    if !owner.valid(host.now()){return Err(LayoutError::Expired);}
    let registered=host.handoff(&ticket,&owner)?;
    *cleanup_state.route.lock().map_err(|_|LayoutError::Unavailable)?=Some(registered.clone());
    owner.install_fields(target)?;
    let now_host=host.clone();let expected=initial.clone();
    main_job(host.clone(),owner.clone(),false,move|o|{let sample=o.handle().sample()?;if sample.bounds!=expected.bounds||!sample.same_topology(&expected)||sample.monitor!=expected.monitor||sample.target(now_host.monitor())?!=target{return Err(LayoutError::Geometry);}o.normalize_target(target)?;o.protect_clicks(now_host.now())}).await?;
    for resize in [true,false]{let now_host=host.clone();let expected=initial.clone();main_job(host.clone(),owner.clone(),false,move|o|{if !o.geometry_queued(){let sample=o.handle().sample()?;if sample.bounds!=expected.bounds||sample.monitor!=expected.monitor||!sample.same_topology(&expected)||sample.target(now_host.monitor())?!=target{return Err(LayoutError::Geometry);}}o.expand_field(resize,target,now_host.now())}).await?;}
    loop {
        if !owner.valid(host.now()){return Err(LayoutError::Expired);}
        host.validate(&owner.cancel)?;
        match host.route_state(&registered) {
            LayoutRouteState::Revoked => return Err(LayoutError::Stale),
            LayoutRouteState::Classifying => { wait_for_observation(&host,&owner).await?; continue; }
            LayoutRouteState::Current => {}
        }
        if let Ok(fence)=owner.getter_fence(){
            let sample=main_job(host.clone(),owner.clone(),false,|o|o.handle().sample()).await?;
            if !owner.valid(host.now()){return Err(LayoutError::Expired);}
            host.validate(&owner.cancel)?;
            match host.route_state(&registered) {
                LayoutRouteState::Revoked => return Err(LayoutError::Stale),
                // A real owned event can be between raw invalidation and Hub
                // classification. This sample/fence cannot publish anything;
                // obtain both anew after classification, without renewing time.
                LayoutRouteState::Classifying => { wait_for_observation(&host,&owner).await?; continue; }
                LayoutRouteState::Current => {}
            }
            if !sample.same_topology(&initial)||sample.monitor!=host.monitor()||sample.target(host.monitor())?!=target||sample.bounds!=target{return Err(LayoutError::Geometry);}
            owner.accept_getter(fence,sample.bounds,host.now())?;
            *cleanup_state.ready.lock().map_err(|_|LayoutError::Unavailable)?=Some(sample.clone());
            // MoveTo/Ready authority cannot be built from this private value.
            guard.armed=false;
            return Ok(NativeLayoutPrepared{bounds:sample.bounds,scale:sample.scale,reference_physical_height:initial.inner.height,release:NativeLayoutRelease{owner,host,route:registered,completed:cleanup_done}});
        }
        wait_for_observation(&host,&owner).await?;
    }
}
/// Waiting never changes an atom, route or deadline. In particular, a pending
/// classification at the absolute deadline cannot manufacture another probe.
async fn wait_for_observation(host:&Arc<dyn LayoutHost>,owner:&Arc<LayoutOwner>)->Result<(),LayoutError>{
    if !owner.valid(host.now()){return Err(LayoutError::Expired);}
    host.validate(&owner.cancel)?;
    let remaining=owner.deadline.checked_sub(host.now()).filter(|remaining|*remaining>0).ok_or(LayoutError::Expired)?;
    tokio::time::sleep(Duration::from_millis(remaining.min(50))).await;
    Ok(())
}

async fn cleanup(host:Arc<dyn LayoutHost>,owner:Arc<LayoutOwner>,state:Arc<CleanupState>,completed:Arc<CleanupCompletion>){
    loop {
        if owner.cleanup_required(host.now()){break;}
        let phase=owner.phase();
        if matches!(phase,LayoutPhase::Live|LayoutPhase::PromotionPending) {
            let Some(route)=state.route.lock().ok().and_then(|route|route.clone())else{owner.cancelled();break;};
            match host.route_state(&route){
                LayoutRouteState::Revoked=>{owner.cancelled();break;},
                LayoutRouteState::Classifying=>{tokio::time::sleep(Duration::from_millis(25)).await;continue;},
                LayoutRouteState::Current=>{}
            }
            let fence=match owner.getter_fence(){Ok(fence)=>fence,Err(_)=>{owner.cancelled();break;}};
            let expected=state.ready.lock().ok().and_then(|ready|ready.clone());
            // This queued operation ONLY reads the exact retained window. It
            // owns no setter permit and cannot delay or undo actual cleanup.
            // A phase/cancel change discards its result below.
            let observation_deadline=host.now().checked_add(250).map(|deadline|if phase==LayoutPhase::PromotionPending{deadline.min(owner.deadline)}else{deadline});
            let observed=match observation_deadline{Some(deadline)=>initial_sample(host.clone(),owner.handle().clone(),owner.cancel.clone(),deadline).await,None=>Err(LayoutError::Expired)};
            if owner.phase()!=phase{continue;}
            match host.route_state(&route){LayoutRouteState::Revoked=>{owner.cancelled();break;},LayoutRouteState::Classifying=>continue,LayoutRouteState::Current=>{}}
            match (observed,expected){
                (Ok(sample),Some(expected)) if sample.visible&&sample.bounds==expected.bounds&&sample.inner==sample.outer&&sample.monitor==expected.monitor&&sample.same_topology(&expected)=>{
                    if owner.verify_surface_getter(fence,sample.bounds,host.now()).is_err()||host.validate_live(&owner.cancel).is_err(){owner.cancelled();break;}
                },
                _=>{owner.cancelled();break;}
            }
        } else if phase==LayoutPhase::Prepared {
            if owner.getter_fence().is_err(){owner.cancelled();break;}
            let expected=state.ready.lock().ok().and_then(|ready|ready.clone());
            if let Some(expected)=expected {
                // Observation only: neither native getters nor metadata renew
                // preparation D5 or the separate successful-frame lease.
                let observed=main_job(host.clone(),owner.clone(),false,|o|o.handle().sample()).await;
                if owner.phase()!=LayoutPhase::Prepared{continue;}
                if observed.is_err()||observed.as_ref().is_ok_and(|sample|!sample.visible||sample.bounds!=expected.bounds||sample.inner!=sample.outer||!sample.same_topology(&expected))||host.validate(&owner.cancel).is_err(){owner.cancelled();break;}
            }
        }
        // PromotionPending retains D5. Read-only native samples never renew
        // time or a successful-frame lease, and no setters are issued here.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    owner.cancelled();
    // The queued closure owns its permit after timeout/drop. Never enqueue a
    // restoring setter over an expansion closure that has not completed.
    while state.preparing.load(Ordering::SeqCst)||owner.in_flight(){tokio::time::sleep(Duration::from_millis(25)).await;}
    if owner.begin_restore().is_err(){owner.guarded_failure();completed.complete(NativeLayoutRestoreOutcome::GuardedUnavailable);return;}
    let mut hub_finished=false;
    for _ in 0..20 {
        let policy=host.policy();
        let (clicks,top)=policy.as_ref().map(|p|(p.clicks,p.top)).unwrap_or((true,true));
        if main_job(host.clone(),owner.clone(),true,move|o|o.restore_once(clicks,top,||policy.as_ref().is_some_and(|p|p.current()))).await==Ok(true){
            // Keep route capacity charged until the exact sole Hub owner is
            // actually released. A failed/poisoned finish never frees a slot.
            if !host.finish(&owner){break;}
            hub_finished=true;
            let released=state.route.lock().map(|route|route.as_ref().is_some_and(|route|host.release_route(route))).unwrap_or(false);
            if !released{break;}
            // No-own-setter drift is saved from a LATER actual getter after
            // release; it is never rolled back to the stale saved tuple.
            if !owner.geometry_queued()&&owner.handle().current(){if let Ok(sample)=initial_sample(host.clone(),owner.handle().clone(),Arc::new(AtomicBool::new(false)),host.now().saturating_add(1000)).await{host.save_current(owner.handle(),sample.bounds);}}
            completed.complete(if owner.destroyed(){NativeLayoutRestoreOutcome::Retired}else if owner.geometry_queued(){NativeLayoutRestoreOutcome::Restored}else{NativeLayoutRestoreOutcome::ReleasedWithoutGeometry});return;
        }
        while owner.in_flight(){tokio::time::sleep(Duration::from_millis(25)).await;}
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Failed geometry/Hub confirmation retains its real guard/Normal. If Hub
    // release already succeeded but route release failed, geometry/policy are
    // confirmed and the physical guard is cleared; only unavailable exact
    // route bookkeeping/capacity remains. Neither path acknowledges Restored.
    owner.guarded_failure();completed.complete(NativeLayoutRestoreOutcome::GuardedUnavailable);
    // At most eight retained cleanup workers/slots. Failed restore never frees
    // capacity optimistically, but exact destruction can retire it later.
    while !owner.destroyed()||owner.in_flight(){tokio::time::sleep(Duration::from_millis(250)).await;}
    owner.finish_destroyed();
    if hub_finished||host.finish(&owner){if let Ok(route)=state.route.lock(){if let Some(route)=route.as_ref(){host.release_route(route);}}}
}

#[cfg(test)]
mod surface_cell_tests{
    use super::*;
    #[test]
    fn poisoned_live_cell_cancels_retained_proofs_and_never_recovers_authority(){
        let cancel=Arc::new(AtomicBool::new(false));
        let cell=SurfaceCell{cancel:cancel.clone(),issued:0,deadline:5000,state:Mutex::new(SurfaceState{phase:SurfacePhase::Live,frame:1,received:0,admitted:0,geometry_current:true})};
        let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{let _held=cell.state.lock().unwrap();panic!("synthetic surface poison");}));
        assert!(!cell.retained_current(0));assert!(cancel.load(Ordering::SeqCst));assert!(!cell.live_current(0));assert!(cell.accepted(2,1,1,&||1).is_err());assert!(cell.commit(2,1,1,&||1).is_err());assert!(cell.state.is_poisoned());
    }
}
