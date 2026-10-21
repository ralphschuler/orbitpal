//! The PresentationHub's retained constructor subset and sole native Normal
//! owner. No renderer, IPC, Ready, session or action authority is minted here.
use super::{Bounds, Display, Normal, WindowOps, WindowSignals, ERROR};
use crate::window_signals::{FieldRequest, GetterFence, LayoutFields};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

const MAX_RETAINED: usize = 64;
const MAX_ACTIVE: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutError { Unavailable, Stale, Busy, Expired, Geometry, Capacity, Dispatch, Persistence }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutPhase { Reserved, Expanding, Prepared, PromotionPending, Live, Restoring, Released, Guarded }
#[derive(Clone, Debug)]
pub(crate) struct LayoutSample {
    pub bounds: Bounds,
    pub inner: Bounds,
    pub outer: Bounds,
    pub visible: bool,
    pub scale: f64,
    pub monitor: String,
    pub displays: Vec<Display>,
}
impl LayoutSample {
    pub(crate) fn target(&self, id:&str)->Result<Bounds,LayoutError>{
        if !self.visible || self.inner!=self.outer || self.bounds!=self.inner
            || !self.scale.is_finite() || !(0.25..=8.).contains(&self.scale)
            || self.displays.is_empty() || self.displays.len()>64 {return Err(LayoutError::Geometry);}
        crate::world_state::coordinates::OutputGeometry::new("native-window",self.bounds.x,self.bounds.y,self.bounds.width,self.bounds.height,self.scale).map_err(|_|LayoutError::Geometry)?;
        let mut selected:Option<&Display>=None;
        let mut canonical=std::collections::BTreeMap::new();
        for display in &self.displays {
            crate::world_state::coordinates::OutputGeometry::new(&display.id,display.bounds.x,display.bounds.y,display.bounds.width,display.bounds.height,display.scale_factor).map_err(|_|LayoutError::Geometry)?;
            if canonical.insert(display.id.clone(),(display.bounds,display.scale_factor.to_bits())).is_some_and(|prior|prior!=(display.bounds,display.scale_factor.to_bits())){return Err(LayoutError::Geometry);}
            if display.id==id {
                if selected.is_some_and(|prior|prior.bounds!=display.bounds||prior.scale_factor.to_bits()!=display.scale_factor.to_bits()){return Err(LayoutError::Geometry);}
                selected=Some(display);
            }
        }
        let selected=selected.ok_or(LayoutError::Geometry)?;
        if selected.scale_factor.to_bits()!=self.scale.to_bits()||canonical.get(&self.monitor).is_none_or(|(_,scale)|*scale!=self.scale.to_bits()){return Err(LayoutError::Geometry);}
        Ok(selected.bounds)
    }
    pub(crate) fn same_topology(&self, other:&Self)->bool{
        fn fingerprint(sample:&LayoutSample)->Vec<(String,Bounds,u64)>{
            let mut values=sample.displays.iter().map(|d|(d.id.clone(),d.bounds,d.scale_factor.to_bits())).collect::<Vec<_>>();
            values.sort_by(|a,b|(&a.0,a.1.x,a.1.y,a.1.width,a.1.height,a.2).cmp(&(&b.0,b.1.x,b.1.y,b.1.width,b.1.height,b.2)));values.dedup();values
        }
        self.scale.to_bits()==other.scale.to_bits()&&fingerprint(self)==fingerprint(other)
    }
}
/// Calls are made only by the Main runner, with no Hub/Core/Store lock held.
pub(crate) trait LayoutWindow:Send+Sync {
    fn sample(&self)->Result<LayoutSample,LayoutError>;
    fn bounds(&self)->Result<Bounds,LayoutError>;
    fn size_direct(&self,bounds:Bounds)->Result<(),LayoutError>;
    fn position_direct(&self,bounds:Bounds)->Result<(),LayoutError>;
    fn clicks(&self,ignore:bool)->Result<(),LayoutError>;
    fn top(&self,value:bool)->Result<(),LayoutError>;
    fn normalize(&self,bounds:Bounds)->Result<Bounds,LayoutError>;
}
pub(super) struct TauriLayoutWindow {pub window:tauri::WebviewWindow}
impl LayoutWindow for TauriLayoutWindow {
    fn bounds(&self)->Result<Bounds,LayoutError>{let p=self.window.outer_position().map_err(|_|LayoutError::Geometry)?;let s=self.window.inner_size().map_err(|_|LayoutError::Geometry)?;Ok(Bounds{x:p.x,y:p.y,width:s.width,height:s.height})}
    fn sample(&self)->Result<LayoutSample,LayoutError>{
        let outer_p=self.window.outer_position().map_err(|_|LayoutError::Geometry)?;let outer_s=self.window.outer_size().map_err(|_|LayoutError::Geometry)?;
        let inner_p=self.window.inner_position().map_err(|_|LayoutError::Geometry)?;let inner_s=self.window.inner_size().map_err(|_|LayoutError::Geometry)?;
        let monitors=self.window.available_monitors().map_err(|_|LayoutError::Geometry)?;
        if monitors.len()>64{return Err(LayoutError::Capacity);}
        let monitor=self.window.current_monitor().map_err(|_|LayoutError::Geometry)?.ok_or(LayoutError::Geometry)?;
        let inner=Bounds{x:inner_p.x,y:inner_p.y,width:inner_s.width,height:inner_s.height};
        Ok(LayoutSample{bounds:self.bounds()?,inner,outer:Bounds{x:outer_p.x,y:outer_p.y,width:outer_s.width,height:outer_s.height},visible:self.window.is_visible().map_err(|_|LayoutError::Geometry)?,scale:self.window.scale_factor().map_err(|_|LayoutError::Geometry)?,monitor:super::display(monitor).id,displays:monitors.into_iter().map(super::display).collect()})
    }
    fn size_direct(&self,b:Bounds)->Result<(),LayoutError>{self.window.set_size(tauri::PhysicalSize::new(b.width,b.height)).map_err(|_|LayoutError::Geometry)}
    fn position_direct(&self,b:Bounds)->Result<(),LayoutError>{self.window.set_position(tauri::PhysicalPosition::new(b.x,b.y)).map_err(|_|LayoutError::Geometry)}
    fn clicks(&self,value:bool)->Result<(),LayoutError>{self.window.set_ignore_cursor_events(value).map_err(|_|LayoutError::Geometry)}
    fn top(&self,value:bool)->Result<(),LayoutError>{self.window.set_always_on_top(value).map_err(|_|LayoutError::Geometry)}
    fn normalize(&self,b:Bounds)->Result<Bounds,LayoutError>{
        #[cfg(target_os="linux")]{Ok(crate::window_signals::normalize_physical_bounds(b,self.window.scale_factor().map_err(|_|LayoutError::Geometry)?))}
        #[cfg(not(target_os="linux"))]{Ok(b)}
    }
}
pub(crate) trait LayoutMain:Send+Sync {fn enqueue(&self,operation:Box<dyn FnOnce()+Send>)->Result<(),LayoutError>;}
pub(crate) struct NativeLayoutMain(pub tauri::AppHandle);
impl LayoutMain for NativeLayoutMain {fn enqueue(&self,operation:Box<dyn FnOnce()+Send>)->Result<(),LayoutError>{self.0.run_on_main_thread(operation).map_err(|_|LayoutError::Dispatch)}}
struct Retained {
    id:u64,bot:String,signals:Arc<WindowSignals>,window:Arc<dyn LayoutWindow>,
    current:AtomicBool,destroyed:AtomicBool,owner:Mutex<Option<Arc<LayoutOwner>>>,
}
#[derive(Clone)]
pub(crate) struct LayoutHandle(Arc<Retained>);
impl LayoutHandle {
    pub(crate) fn bot(&self)->&str{&self.0.bot}
    pub(crate) fn signals(&self)->&Arc<WindowSignals>{&self.0.signals}
    pub(crate) fn current(&self)->bool{self.0.current.load(Ordering::SeqCst)&&!self.0.destroyed.load(Ordering::SeqCst)}
    pub(crate) fn sample(&self)->Result<LayoutSample,LayoutError>{self.0.window.sample()}
    /// A queued resync owns the actual constructor, never reacquires by label.
    /// A new owner/replacement or profile write invalidates its observation.
    pub(crate) fn resync_policy(&self,clicks:bool,top:bool,current:impl Fn()->bool)->Result<(),LayoutError>{
        let available=||self.current()&&self.0.owner.lock().map(|owner|owner.is_none()).unwrap_or(false)&&current();
        if !self.current(){return Err(LayoutError::Stale);}
        apply_window_policy(self.0.window.as_ref(),clicks,top,true,&available)
    }
}
#[derive(Default)]
struct Retention {serial:u64,records:Vec<Arc<Retained>>}
#[derive(Default)]
pub(super) struct NativePresentations {state:Mutex<Retention>}
impl NativePresentations {
    fn cancel_record(record:&Retained){let owner=record.owner.lock().unwrap_or_else(|poison|poison.into_inner());if let Some(owner)=owner.as_ref(){owner.cancelled();}}
    fn lock(&self)->Result<std::sync::MutexGuard<'_,Retention>,LayoutError>{
        self.state.lock().map_err(|poison|{for record in &poison.into_inner().records {Self::cancel_record(record);}LayoutError::Unavailable})
    }
    fn reap(state:&mut Retention){state.records.retain(|record|record.current.load(Ordering::SeqCst)||Arc::strong_count(record)>1||record.owner.lock().map(|owner|owner.is_some()).unwrap_or(true));}
    pub(super) fn register(&self,bot:&str,window:Arc<dyn LayoutWindow>,signals:Arc<WindowSignals>)->Result<(),LayoutError>{
        let mut state=self.lock()?;
        if state.serial>=crate::life_domain::MAX_COUNTER{for record in &state.records{Self::cancel_record(record);}return Err(LayoutError::Unavailable);}
        if state.records.iter().any(|r|r.bot==bot&&Arc::ptr_eq(&r.signals,&signals)&&r.current.load(Ordering::SeqCst)){return Ok(());}
        for record in state.records.iter().filter(|r|r.bot==bot){record.current.store(false,Ordering::SeqCst);Self::cancel_record(record);}
        Self::reap(&mut state);
        if state.records.len()>=MAX_RETAINED||state.serial>=crate::life_domain::MAX_COUNTER {
            for record in &state.records {Self::cancel_record(record);}return Err(LayoutError::Capacity);
        }
        state.serial+=1;let id=state.serial;
        state.records.push(Arc::new(Retained{id,bot:bot.into(),signals,window,current:AtomicBool::new(true),destroyed:AtomicBool::new(false),owner:Mutex::new(None)}));Ok(())
    }
    pub(super) fn handle(&self,bot:&str)->Result<LayoutHandle,LayoutError>{self.lock()?.records.iter().find(|r|r.bot==bot&&r.current.load(Ordering::SeqCst)&&!r.destroyed.load(Ordering::SeqCst)).cloned().map(LayoutHandle).ok_or(LayoutError::Stale)}
    pub(super) fn guarded(&self,bot:&str)->bool{self.lock().map(|state|state.records.iter().any(|r|r.bot==bot&&r.current.load(Ordering::SeqCst)&&r.owner.lock().map(|o|o.is_some()).unwrap_or(true))).unwrap_or(true)}
    pub(super) fn any_work(&self)->bool{self.lock().map(|state|state.records.iter().any(|r|r.owner.lock().map(|o|o.is_some()).unwrap_or(true))).unwrap_or(true)}
    /// Healthy Live owners may yield to a future dialog scheduling attempt.
    /// Every physical owner must have its exact native receipt; all preparing,
    /// restoring, unowned or stale states still block. This never releases work.
    pub(super) fn blocks_dialog_scheduling(&self,handoffs:&[crate::life_runtime::native_scene::NativeDialogueHandoff],now:u64)->bool{
        self.lock().map(|state|state.records.iter().any(|record|{
            record.owner.lock().map(|slot|slot.as_ref().is_some_and(|owner|
                !handoffs.iter().any(|receipt|receipt.matches_owner(owner,now)))).unwrap_or(true)
        })).unwrap_or(true)
    }
    pub(super) fn reserve(&self,handle:&LayoutHandle,sample:&LayoutSample,issued:u64,deadline:u64,cancel:Arc<AtomicBool>)->Result<Arc<LayoutOwner>,LayoutError>{
        let state=self.lock()?;
        if state.serial>=crate::life_domain::MAX_COUNTER{for record in &state.records{Self::cancel_record(record);}return Err(LayoutError::Unavailable);}
        if !handle.current()||!state.records.iter().any(|r|Arc::ptr_eq(r,&handle.0)){return Err(LayoutError::Stale);}
        let active=state.records.iter().filter(|r|r.owner.lock().map(|o|o.is_some()).unwrap_or(true)).count();
        if active>=MAX_ACTIVE||handle.signals().has_pending_move()||handle.signals().has_pending_resize(){return Err(LayoutError::Busy);}
        let mut slot=handle.0.owner.lock().map_err(|_|LayoutError::Unavailable)?;
        if slot.is_some()||cancel.load(Ordering::SeqCst)||deadline.checked_sub(issued)!=Some(5000){return Err(LayoutError::Busy);}
        let normal_monitor=sample.displays.iter().find(|display|display.id==sample.monitor).ok_or(LayoutError::Geometry)?.clone();
        let owner=Arc::new(LayoutOwner{handle:handle.clone(),cancel,issued,deadline,normal_monitor,state:Mutex::new(Owned{phase:LayoutPhase::Reserved,normal:Some(Normal{bounds:sample.bounds,expanded:false,changed_move:false,changed_size:false,restore:None}),fields:None,moved:None,resized:None,in_flight:false,main_serial:0,geometry_queued:false,click_dirty:false,surface:None}),wake:tokio::sync::Notify::new()});
        *slot=Some(owner.clone());Ok(owner)
    }
    pub(super) fn retire(&self,bot:&str,signals:&Arc<WindowSignals>){if let Ok(mut state)=self.lock(){for record in &state.records{if record.bot==bot&&Arc::ptr_eq(&record.signals,signals){record.current.store(false,Ordering::SeqCst);record.destroyed.store(true,Ordering::SeqCst);Self::cancel_record(record);}}Self::reap(&mut state);}}
    pub(super) fn finish(&self,owner:&Arc<LayoutOwner>)->bool{
        let Ok(mut state)=self.lock()else{return false;};
        let Ok(owned)=owner.state.lock()else{return false;};
        if owned.in_flight||owned.phase!=LayoutPhase::Released{return false;}drop(owned);
        let mut slot=match owner.handle.0.owner.lock(){Ok(slot)=>slot,Err(_)=>return false};
        if slot.as_ref().is_none_or(|current|!Arc::ptr_eq(current,owner)){return false;}
        slot.take();drop(slot);Self::reap(&mut state);true
    }
    pub(super) fn observe(&self,bot:&str,signals:&Arc<WindowSignals>,moved:Option<(i32,i32)>,resized:Option<(u32,u32)>)->bool{
        let owner=self.lock().ok().and_then(|state|state.records.iter().find(|r|r.bot==bot&&Arc::ptr_eq(&r.signals,signals)).and_then(|r|r.owner.lock().ok().and_then(|o|o.clone())));
        if let Some(owner)=owner{return owner.observe(moved,resized);}
        if let Some((x,y))=moved{signals.observe_move(x,y);}if let Some((w,h))=resized{signals.observe_resize(w,h);}false
    }
}
struct Owned {
    phase:LayoutPhase,normal:Option<Normal>,fields:Option<LayoutFields>,moved:Option<FieldRequest>,resized:Option<FieldRequest>,
    in_flight:bool,main_serial:u64,geometry_queued:bool,click_dirty:bool,
    surface:Option<Arc<crate::life_runtime::native_layout::SurfaceCell>>,
}
pub(crate) struct LayoutOwner {
    handle:LayoutHandle,pub(crate) cancel:Arc<AtomicBool>,pub(crate) issued:u64,pub(crate) deadline:u64,
    normal_monitor:Display,
    state:Mutex<Owned>,wake:tokio::sync::Notify,
}
impl LayoutOwner {
    fn lock(&self)->Result<std::sync::MutexGuard<'_,Owned>,LayoutError>{self.state.lock().map_err(|_|{self.cancelled();LayoutError::Unavailable})}
    pub(crate) fn handle(&self)->&LayoutHandle{&self.handle}
    pub(crate) fn valid(&self,now:u64)->bool{self.handle.current()&&!self.cancel.load(Ordering::SeqCst)&&now>=self.issued&&now<=self.deadline}
    pub(crate) fn begin_promotion(&self,cell:&Arc<crate::life_runtime::native_layout::SurfaceCell>,now:u64)->Result<(),LayoutError>{
        let mut state=self.lock()?;
        if state.phase!=LayoutPhase::Prepared||state.surface.is_some()||!self.valid(now)||!cell.matches(&self.cancel,self.issued,self.deadline){return Err(LayoutError::Stale);}
        if state.fields.is_none(){return Err(LayoutError::Stale);}
        state.surface=Some(cell.clone());state.phase=LayoutPhase::PromotionPending;Ok(())
    }
    pub(crate) fn pending_retained(&self,cell:&Arc<crate::life_runtime::native_layout::SurfaceCell>,now:u64)->bool{self.valid(now)&&self.lock().is_ok_and(|state|state.phase==LayoutPhase::PromotionPending&&state.surface.as_ref().is_some_and(|s|Arc::ptr_eq(s,cell))&&cell.retained_current(now))}
    pub(crate) fn promotion_current(&self,cell:&Arc<crate::life_runtime::native_layout::SurfaceCell>,now:u64)->bool{
        self.valid(now)&&self.lock().is_ok_and(|state|state.phase==LayoutPhase::PromotionPending&&!state.in_flight&&state.surface.as_ref().is_some_and(|s|Arc::ptr_eq(s,cell))&&cell.pending_current(now)&&state.fields.as_ref().is_some_and(|fields|self.handle.signals().begin_getter_check(fields).is_ok()))
    }
    pub(crate) fn commit_live(&self,cell:&Arc<crate::life_runtime::native_layout::SurfaceCell>,frame:u64,received:u64,now:u64,clock:&impl Fn()->u64)->Result<(),LayoutError>{
        let mut state=self.lock()?;
        if state.phase!=LayoutPhase::PromotionPending||state.in_flight||!self.valid(now)||state.surface.as_ref().is_none_or(|s|!Arc::ptr_eq(s,cell)){return Err(LayoutError::Stale);}
        self.handle.signals().begin_getter_check(state.fields.as_ref().ok_or(LayoutError::Stale)?).map_err(|_|LayoutError::Geometry)?;
        cell.commit(frame,received,now,clock)?;state.phase=LayoutPhase::Live;Ok(())
    }
    pub(crate) fn live_current(&self,cell:&Arc<crate::life_runtime::native_layout::SurfaceCell>,now:u64)->bool{
        self.handle.current()&&!self.cancel.load(Ordering::SeqCst)&&self.lock().is_ok_and(|state|state.phase==LayoutPhase::Live&&state.surface.as_ref().is_some_and(|s|Arc::ptr_eq(s,cell))&&cell.live_current(now)&&state.fields.as_ref().is_some_and(|fields|self.handle.signals().begin_getter_check(fields).is_ok()))
    }
    /// Serialize expiry with the promotion publication point. Once cleanup
    /// wins this decision, no late accepted calibration can make it Live.
    pub(crate) fn cleanup_required(&self,now:u64)->bool{
        let Ok(state)=self.lock()else{return true;};
        let expired=if state.phase==LayoutPhase::Live{state.surface.as_ref().is_none_or(|cell|!cell.retained_current(now))}else{now<self.issued||now>self.deadline};
        let required=expired||!self.handle.current()||self.cancel.load(Ordering::SeqCst)||self.destroyed();
        if required{if let Some(cell)=&state.surface{cell.revoke();}self.cancelled();}required
    }
    fn operation_current(&self,state:&Owned,now:u64)->bool{if state.phase==LayoutPhase::Live{self.handle.current()&&!self.cancel.load(Ordering::SeqCst)&&state.surface.as_ref().is_some_and(|cell|cell.retained_current(now))}else{self.valid(now)}}
    pub(crate) fn observation_wait(&self,now:u64)->Option<u64>{self.lock().ok().and_then(|state|if self.operation_current(&state,now){if state.phase==LayoutPhase::Live{Some(250)}else{self.deadline.checked_sub(now)}}else{None})}
    pub(crate) fn verify_surface_getter(&self,fence:GetterFence,bounds:Bounds,now:u64)->Result<(),LayoutError>{let state=self.lock()?;if !matches!(state.phase,LayoutPhase::Live|LayoutPhase::PromotionPending)||!self.operation_current(&state,now){return Err(LayoutError::Expired);}self.handle.signals().match_getter(state.fields.as_ref().ok_or(LayoutError::Stale)?,fence,bounds).map_err(|_|LayoutError::Geometry)?;state.surface.as_ref().ok_or(LayoutError::Stale)?.confirm_geometry(now)}
    pub(crate) fn cancelled(&self){self.cancel.store(true,Ordering::SeqCst);self.wake.notify_waiters();}
    pub(crate) fn in_flight(&self)->bool{self.lock().map(|s|s.in_flight).unwrap_or(true)}
    pub(crate) fn phase(&self)->LayoutPhase{self.lock().map(|s|s.phase).unwrap_or(LayoutPhase::Guarded)}
    pub(crate) fn destroyed(&self)->bool{self.handle.0.destroyed.load(Ordering::SeqCst)}
    pub(crate) fn geometry_queued(&self)->bool{self.lock().map(|s|s.geometry_queued).unwrap_or(true)}
    pub(crate) fn install_fields(&self,target:Bounds)->Result<(),LayoutError>{
        let mut state=self.lock()?;
        if state.phase!=LayoutPhase::Reserved||self.cancel.load(Ordering::SeqCst){return Err(LayoutError::Stale);}
        let initial=state.normal.as_ref().ok_or(LayoutError::Stale)?.bounds;
        state.fields=Some(self.handle.signals().begin_layout_fields(initial,target).map_err(|_|LayoutError::Geometry)?);state.phase=LayoutPhase::Expanding;Ok(())
    }
    pub(crate) fn begin_main(self:&Arc<Self>,cleanup:bool,now:u64)->Result<MainPermit,LayoutError>{
        let mut state=self.lock()?;
        if state.main_serial>=crate::life_domain::MAX_COUNTER{self.cancelled();return Err(LayoutError::Unavailable);}
        if state.in_flight||(!cleanup&&state.phase==LayoutPhase::PromotionPending){return Err(LayoutError::Busy);}
        if !cleanup&&!self.operation_current(&state,now){return Err(LayoutError::Expired);}
        state.main_serial+=1;state.in_flight=true;Ok(MainPermit{owner:self.clone(),serial:state.main_serial,phase:state.phase})
    }
    pub(crate) fn expand_field(&self,resize:bool,target:Bounds,now:u64)->Result<(),LayoutError>{
        let changed={
            let mut state=self.lock()?;
            if !self.valid(now)||state.phase!=LayoutPhase::Expanding{return Err(LayoutError::Stale);}
            let normal=state.normal.as_ref().ok_or(LayoutError::Stale)?.bounds;
            let changed=if resize{(normal.width,normal.height)!=(target.width,target.height)}else{(normal.x,normal.y)!=(target.x,target.y)};
            if !changed{return Ok(());}
            let fields=state.fields.as_ref().ok_or(LayoutError::Stale)?;
            let request=if resize{self.handle.signals().request_layout_resize(fields)}else{self.handle.signals().request_layout_move(fields)}.map_err(|_|LayoutError::Geometry)?;
            if resize{state.resized=Some(request);}else{state.moved=Some(request);}
            state.geometry_queued=true;let normal=state.normal.as_mut().ok_or(LayoutError::Stale)?;normal.expanded=true;
            if resize{normal.changed_size=true;}else{normal.changed_move=true;}changed
        };
        if changed {if resize{self.handle.0.window.size_direct(target)}else{self.handle.0.window.position_direct(target)}}else{Ok(())}
    }
    pub(crate) fn protect_clicks(&self,now:u64)->Result<(),LayoutError>{
        {let mut state=self.lock()?;if !self.valid(now){return Err(LayoutError::Expired);}state.click_dirty=true;}
        self.handle.0.window.clicks(true)
    }
    pub(crate) fn normalize_target(&self,target:Bounds)->Result<(),LayoutError>{if self.handle.0.window.normalize(target)?==target{Ok(())}else{Err(LayoutError::Geometry)}}
    fn observe(&self,moved:Option<(i32,i32)>,resized:Option<(u32,u32)>)->bool{
        let Ok(state)=self.lock()else{return false;};
        if matches!(state.phase,LayoutPhase::Expanding|LayoutPhase::Prepared|LayoutPhase::PromotionPending|LayoutPhase::Live)&&!self.cancel.load(Ordering::SeqCst){
            let fact=if let Some((x,y))=moved{self.handle.signals().observe_move_fact(x,y)}else if let Some((w,h))=resized{self.handle.signals().observe_resize_fact(w,h)}else{return false};
            let request=if moved.is_some(){state.moved.as_ref()}else{state.resized.as_ref()};
            let accepted=match (state.fields.as_ref(),request,fact){(Some(fields),Some(request),Ok(fact))=>self.handle.signals().matches_request(fields,request,&fact).unwrap_or(false),_=>false};
            if !accepted{self.cancelled();}accepted
        }else{drop(state);if let Some((x,y))=moved{self.handle.signals().observe_move(x,y);}if let Some((w,h))=resized{self.handle.signals().observe_resize(w,h);}if self.phase()==LayoutPhase::Reserved{self.cancelled();}false}
    }
    pub(crate) fn getter_fence(&self)->Result<GetterFence,LayoutError>{let state=self.lock()?;self.handle.signals().begin_getter_check(state.fields.as_ref().ok_or(LayoutError::Stale)?).map_err(|_|LayoutError::Busy)}
    pub(crate) fn accept_getter(&self,fence:GetterFence,bounds:Bounds,now:u64)->Result<(),LayoutError>{let mut state=self.lock()?;if !self.valid(now){return Err(LayoutError::Expired);}self.handle.signals().match_getter(state.fields.as_ref().ok_or(LayoutError::Stale)?,fence,bounds).map_err(|_|LayoutError::Geometry)?;state.phase=LayoutPhase::Prepared;Ok(())}
    pub(crate) fn begin_restore(&self)->Result<(),LayoutError>{let mut state=self.lock()?;if state.in_flight{return Err(LayoutError::Busy);}if let Some(fields)=state.fields.take(){self.handle.signals().cancel_layout_fields(&fields);}state.phase=LayoutPhase::Restoring;Ok(())}
    /// Sole Normal is moved into this Main operation; callbacks never borrow it.
    pub(crate) fn restore_once(self:&Arc<Self>,clicks:bool,top:bool,policy_current:impl Fn()->bool)->Result<bool,LayoutError>{
        let (normal,geometry,click_dirty)={let mut state=self.lock()?;if state.phase!=LayoutPhase::Restoring{return Err(LayoutError::Stale);}(state.normal.take().ok_or(LayoutError::Busy)?,state.geometry_queued,state.click_dirty)};
        let mut loan=NormalLoan{owner:self.clone(),normal:Some(normal)};
        let confirmed=if self.destroyed(){true}else if !geometry {
            // Never infer ownership of operator drift before an own setter.
            if click_dirty{self.apply_policy(clicks,top,false,&policy_current)?;}true
        }else{
            let sample=self.handle.0.window.sample()?;
            let matching=sample.displays.iter().filter(|display|display.id==self.normal_monitor.id).collect::<Vec<_>>();
            if sample.displays.len()>64||matching.is_empty()||matching.iter().any(|display|display.bounds!=self.normal_monitor.bounds||display.scale_factor.to_bits()!=self.normal_monitor.scale_factor.to_bits()){return Err(LayoutError::Geometry);}
            let ops=RetainedOps{window:self.handle.0.window.as_ref(),signals:self.handle.signals()};
            // Keep click protection until the full later native tuple agrees,
            // beyond the legacy field/FIFO/getter confirmation.
            let result=super::restore(loan.normal.as_mut().expect("sole normal loan"),true,&ops).map_err(|_|LayoutError::Geometry)?;
            if result{let sample=self.handle.0.window.sample()?;if sample.inner!=sample.outer||sample.scale.to_bits()!=self.normal_monitor.scale_factor.to_bits()||sample.monitor!=self.normal_monitor.id{return Err(LayoutError::Geometry);}self.apply_policy(clicks,top,true,&policy_current)?;}result
        };
        if confirmed {self.lock()?.phase=LayoutPhase::Released;}
        Ok(confirmed)
    }
    /// No Store/Core/Hub wait on Main. A receipt is cancellation-only: check
    /// before AND after each kernel call, re-protect and retry on interference.
    /// It does not attest that a policy can never change after this operation.
    fn apply_policy(&self,clicks:bool,top:bool,apply_top:bool,current:&impl Fn()->bool)->Result<(),LayoutError>{
        apply_window_policy(self.handle.0.window.as_ref(),clicks,top,apply_top,current)
    }
    pub(crate) fn guarded_failure(&self){if let Ok(mut state)=self.state.lock(){state.phase=LayoutPhase::Guarded;}self.cancelled();}
    pub(crate) fn finish_destroyed(&self){if self.destroyed(){if let Ok(mut state)=self.state.lock(){if !state.in_flight{state.phase=LayoutPhase::Released;}}}}
}
fn apply_window_policy(window:&dyn LayoutWindow,clicks:bool,top:bool,apply_top:bool,current:&impl Fn()->bool)->Result<(),LayoutError>{
    let stale=||{window.clicks(true)?;Err(LayoutError::Stale)};
    if !current(){return stale();}
    if apply_top{
        if let Err(error)=window.top(top){let _=window.clicks(true);return Err(error);}
        if !current(){return stale();}
    }
    if !current(){return stale();}
    if let Err(error)=window.clicks(clicks){let _=window.clicks(true);return Err(error);}
    if !current(){return stale();}
    Ok(())
}
struct NormalLoan {owner:Arc<LayoutOwner>,normal:Option<Normal>}
impl Drop for NormalLoan {fn drop(&mut self){if let Ok(mut state)=self.owner.state.lock(){state.normal=self.normal.take();}}}
pub(crate) struct MainPermit {owner:Arc<LayoutOwner>,serial:u64,phase:LayoutPhase}
impl MainPermit {pub(crate) fn allowed(&self,cleanup:bool,now:u64)->bool{self.owner.state.lock().map(|s|s.in_flight&&s.main_serial==self.serial&&(s.phase==self.phase||(!cleanup&&self.phase==LayoutPhase::Prepared&&s.phase==LayoutPhase::PromotionPending&&s.surface.is_some()))&&(cleanup||self.owner.operation_current(&s,now))).unwrap_or(false)}}
impl Drop for MainPermit {fn drop(&mut self){if let Ok(mut state)=self.owner.state.lock(){if state.main_serial==self.serial{state.in_flight=false;}}self.owner.wake.notify_waiters();}}
struct RetainedOps<'a>{window:&'a dyn LayoutWindow,signals:&'a Arc<WindowSignals>}
impl WindowOps for RetainedOps<'_>{
    fn bounds(&self)->Result<Bounds,String>{self.window.bounds().map_err(|_|ERROR.into())}
    fn size(&self,b:Bounds)->Result<(),String>{let c=self.bounds()?;if(c.width,c.height)==(b.width,b.height)&&!self.signals.has_pending_resize(){return Ok(());}if !self.signals.request_resize(b.width,b.height){return Err(ERROR.into());}self.window.size_direct(b).map_err(|_|ERROR.into())}
    fn position(&self,b:Bounds)->Result<(),String>{let c=self.bounds()?;if(c.x,c.y)==(b.x,b.y)&&!self.signals.has_pending_move(){return Ok(());}if !self.signals.request_move(b.x,b.y){return Err(ERROR.into());}self.window.position_direct(b).map_err(|_|ERROR.into())}
    fn clicks(&self,v:bool)->Result<(),String>{self.window.clicks(v).map_err(|_|ERROR.into())}
    fn signals(&self)->&WindowSignals{self.signals}
    fn normalize(&self,b:Bounds)->Result<Bounds,String>{self.window.normalize(b).map_err(|_|ERROR.into())}
}
#[cfg(test)]
#[path="native_layout_tests.rs"]
mod tests;
