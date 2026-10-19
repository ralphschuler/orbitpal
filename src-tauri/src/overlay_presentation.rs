//! Output geometry and field-scoped, session-only previews. No capture APIs.
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::{Arc, Mutex}};
use crate::window_signals::{WindowSignals, RestoreFence};
pub use crate::window_signals::Bounds;
use tauri::Manager;

const ERROR: &str = "Die Desktop-Darstellung konnte nicht angepasst werden.";
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Display { pub id:String, pub name:String, pub bounds:Bounds, pub scale_factor:f64 }
#[derive(Clone, Debug)]
struct Normal {
    bounds: Bounds,
    expanded: bool,
    changed_move: bool,
    changed_size: bool,
    restore: Option<(Bounds, RestoreFence)>,
}
#[derive(Default)]
struct Bot {
    normal: Option<Normal>,
    scale: Option<f64>,
    monitor: Option<String>,
    revision: u64,
    signals: Arc<WindowSignals>,
    replacing: bool,
    initial_bounds: Option<Bounds>,
}
#[derive(Default)]
pub struct PresentationHub { bots: Mutex<HashMap<String, Bot>> }
fn advance(bot:&mut Bot){bot.revision=bot.revision.saturating_add(1).min(9_007_199_254_740_991);}
fn scale(profile:&Value)->f64{profile["avatar"]["scale"].as_f64().filter(|v|v.is_finite()&&(0.3..=2.5).contains(v)).unwrap_or(1.0)}
pub fn validate_scale(value:f64)->Result<(),String>{if value.is_finite()&&(0.3..=2.5).contains(&value){Ok(())}else{Err("Die Figurgröße muss zwischen 0,3 und 2,5 liegen.".into())}}
fn main_only(label:&str)->Result<(),String>{if label=="main"{Ok(())}else{Err("Darstellungsvorschauen werden ausschließlich im Studio gesteuert.".into())}}
fn display(monitor:tauri::Monitor)->Display{
    let p=monitor.position();let s=monitor.size();let name=monitor.name().cloned().unwrap_or_else(||"Monitor".into());
    Display{id:format!("{}:{}:{}",p.x,p.y,name),name,bounds:Bounds{x:p.x,y:p.y,width:s.width,height:s.height},scale_factor:monitor.scale_factor()}
}
fn displays(window:&tauri::WebviewWindow)->Result<Vec<Display>,String>{window.available_monitors().map(|items|items.into_iter().map(display).collect()).map_err(|_|ERROR.into())}
fn overlaps(a:Bounds,b:Bounds)->bool{(a.x as i64+a.width as i64)>b.x as i64&&(a.y as i64+a.height as i64)>b.y as i64&&(a.x as i64)<b.x as i64+b.width as i64&&(a.y as i64)<b.y as i64+b.height as i64}
fn intersection(a:Bounds,b:Bounds)->u64{let width=((a.x as i64+a.width as i64).min(b.x as i64+b.width as i64)-(a.x as i64).max(b.x as i64)).max(0)as u64;let height=((a.y as i64+a.height as i64).min(b.y as i64+b.height as i64)-(a.y as i64).max(b.y as i64)).max(0)as u64;width*height}
fn choose<'a>(items:&'a[Display],preferred:&str,normal:Bounds)->Option<&'a Display>{items.iter().find(|item|item.id==preferred).or_else(||items.iter().max_by_key(|item|intersection(item.bounds,normal)))}
trait WindowOps {
    fn bounds(&self) -> Result<Bounds, String>;
    fn size(&self, bounds: Bounds) -> Result<(), String>;
    fn position(&self, bounds: Bounds) -> Result<(), String>;
    fn clicks(&self, ignore: bool) -> Result<(), String>;
    fn signals(&self) -> &WindowSignals;
    fn normalize(&self, bounds: Bounds) -> Result<Bounds, String> { Ok(bounds) }
}
struct NativeWindow<'a> { window: &'a tauri::WebviewWindow, signals: &'a WindowSignals }
impl WindowOps for NativeWindow<'_> {
    fn bounds(&self)->Result<Bounds,String>{let p=self.window.outer_position().map_err(|_|ERROR)?;let s=self.window.inner_size().map_err(|_|ERROR)?;Ok(Bounds{x:p.x,y:p.y,width:s.width,height:s.height})}
    fn size(&self,b:Bounds)->Result<(),String>{
        let current=self.bounds()?;if (current.width,current.height)==(b.width,b.height)&&!self.signals.has_pending_resize(){return Ok(());}
        if !self.signals.request_resize(b.width,b.height){return Err(ERROR.into());}
        self.window.set_size(tauri::PhysicalSize::new(b.width,b.height)).map_err(|_|ERROR.into())
    }
    fn position(&self,b:Bounds)->Result<(),String>{
        let current=self.bounds()?;if (current.x,current.y)==(b.x,b.y)&&!self.signals.has_pending_move(){return Ok(());}
        if !self.signals.request_move(b.x,b.y){return Err(ERROR.into());}
        self.window.set_position(tauri::PhysicalPosition::new(b.x,b.y)).map_err(|_|ERROR.into())
    }
    fn clicks(&self,value:bool)->Result<(),String>{self.window.set_ignore_cursor_events(value).map_err(|_|ERROR.into())}
    fn signals(&self)->&WindowSignals{self.signals}
    fn normalize(&self,bounds:Bounds)->Result<Bounds,String>{
        #[cfg(target_os="linux")]
        {Ok(crate::window_signals::normalize_physical_bounds(bounds,self.window.scale_factor().map_err(|_|ERROR)?))}
        #[cfg(not(target_os="linux"))]
        {Ok(bounds)}
    }
}
fn restore(normal: &mut Normal, clicks: bool, window: &impl WindowOps) -> Result<bool, String> {
    window.clicks(true)?;
    let target = window.normalize(normal.bounds)?;
    let current = window.bounds()?;
    let moved = (current.x,current.y)!=(target.x,target.y)||window.signals().has_pending_move();
    let resized = (current.width,current.height)!=(target.width,target.height)||window.signals().has_pending_resize();
    let added = (moved&&!normal.changed_move)||(resized&&!normal.changed_size);
    normal.changed_move |= moved; normal.changed_size |= resized;
    let fresh = added || normal.restore.as_ref().is_none_or(|(previous,_)|*previous != target);
    if fresh {
        let fence = window.signals().restore_fence(target, normal.changed_move, normal.changed_size);
        normal.restore = Some((target, fence));
    }
    let fence = normal.restore.as_ref().expect("restore fence registered").1;
    // Always request changed fields on the first restore, even when stale
    // getters still show normal bounds before the queued expansion arrives.
    if fresh || !fence.confirmed(window.signals().snapshot(), window.bounds()?) {
        if normal.changed_size { window.size(target)?; }
        if normal.changed_move { window.position(target)?; }
    }
    if !fence.confirmed(window.signals().snapshot(), window.bounds()?) { return Ok(false); }
    window.clicks(clicks)?;
    Ok(true)
}
/// All callers serialize these operations on Main. Keep the normal snapshot
/// and click-through until field-specific request/event/getter evidence agrees.
fn transition(bot:&mut Bot,target:Option<Bounds>,clicks:bool,window:&impl WindowOps,save:impl FnOnce(Bounds)->Result<(),String>)->Result<(),String>{
    if let Some(target)=target {
        if bot.normal.is_none(){
            let bounds=bot.initial_bounds.take().unwrap_or(window.bounds()?); save(bounds)?;
            bot.normal=Some(Normal{bounds,expanded:false,changed_move:false,changed_size:false,restore:None});
        }
        window.clicks(true)?;
        let normal=bot.normal.as_mut().expect("normal snapshot exists");
        normal.restore=None;
        // Conservatively publish desktop framing even after a partial setter
        // failure; native setters may have queued work before returning.
        normal.expanded=true;
        let result=window.bounds().and_then(|current|{
            if current==target&&!window.signals().has_pending_move()&&!window.signals().has_pending_resize() { Ok(()) } else {
                normal.changed_size |= (target.width,target.height)!=(normal.bounds.width,normal.bounds.height)||window.signals().has_pending_resize();
                window.size(target)?;
                normal.changed_move |= (target.x,target.y)!=(normal.bounds.x,normal.bounds.y)||window.signals().has_pending_move();
                window.position(target)
            }
        });
        if result.is_err() {
            if restore(normal,clicks,window).unwrap_or(false) { normal.expanded=false; }
        }
        advance(bot);
        result
    } else if let Some(normal)=bot.normal.as_mut() {
        if restore(normal,clicks,window)? { bot.normal=None; advance(bot); }
        Ok(())
    } else { Ok(()) }
}
/// No monitor geometry was requested in this state. User movement during
/// initialization can safely expire only its normal-save guard.
fn finish_initial(bot:&mut Bot,window:&impl WindowOps,expired:bool)->Result<(),String>{
    if let Some(initial)=bot.initial_bounds {
        if expired || window.bounds()?==window.normalize(initial)? {bot.initial_bounds=None;}
    }Ok(())
}
fn needs_replacement(bot:&Bot,target:Option<Bounds>,pending:bool,attempt:u8)->bool{
    target.is_none()&&bot.normal.is_some()&&pending&&attempt>=20
}
impl PresentationHub {
    pub fn suppress_bounds(&self,id:&str)->bool{self.bots.try_lock().map(|bots|bots.get(id).is_some_and(|bot|bot.normal.is_some()||bot.initial_bounds.is_some())).unwrap_or(true)}
    pub fn guarded_window(&self,id:&str)->bool{self.bots.lock().map(|bots|bots.get(id).is_some_and(|bot|bot.normal.is_some())).unwrap_or(true)}
    pub fn register_window(&self,id:&str,signals:Arc<WindowSignals>,initial_bounds:Option<Bounds>)->Result<(),String>{
        let mut bots=self.bots.lock().map_err(|_|ERROR)?;let bot=bots.entry(id.into()).or_default();
        bot.signals=signals;bot.normal=None;bot.replacing=false;bot.initial_bounds=initial_bounds;advance(bot);Ok(())
    }
    pub fn current_window(&self,id:&str,signals:&Arc<WindowSignals>)->bool{
        self.bots.try_lock().is_ok_and(|bots|bots.get(id).is_some_and(|bot|Arc::ptr_eq(&bot.signals,signals)))
    }
    /// Called on a deferred Main task, never inline from a setter callback.
    pub fn forget_window(&self,id:&str,signals:&Arc<WindowSignals>)->Option<bool>{
        let mut bots=self.bots.lock().ok()?;let bot=bots.get_mut(id)?;
        if !Arc::ptr_eq(&bot.signals,signals){return None;}
        let replacing=bot.replacing;bot.normal=None;bot.replacing=false;advance(bot);Some(replacing)
    }
    pub fn snapshot(&self,profile:&Value)->Value{
        let id=profile["id"].as_str().unwrap_or_default();let Ok(bots)=self.bots.lock()else{return Value::Null;};let bot=bots.get(id);
        json!({"kind":"presentation","botId":id,"revision":bot.map_or(0,|bot|bot.revision),"scale":bot.and_then(|bot|bot.scale).unwrap_or_else(||scale(profile)),"desktop":{"active":bot.and_then(|bot|bot.normal.as_ref()).is_some_and(|normal|normal.expanded),"referencePhysicalHeight":bot.and_then(|bot|bot.normal.as_ref()).map(|normal|normal.bounds.height)}})
    }
    pub fn committed(&self,profile:&Value){if let Ok(mut bots)=self.bots.lock(){let bot=bots.entry(profile["id"].as_str().unwrap_or_default().into()).or_default();bot.scale=Some(scale(profile));bot.monitor=Some(profile["output"]["desktopMonitorId"].as_str().unwrap_or_default().into());advance(bot);}}
}
pub fn publish(app:&tauri::AppHandle,profile:&Value){let state=app.state::<crate::AppState>();state.overlay.publish(app,state.presentation.snapshot(profile));}
fn sync_on_main(app:&tauri::AppHandle,id:&str,attempt:u8,expected:Option<&Arc<WindowSignals>>)->Result<(),String>{
    let state=app.state::<crate::AppState>();
    let Some(window)=app.get_webview_window(&format!("avatar-{id}"))else{return Ok(());};
    let mut bots=state.presentation.bots.lock().map_err(|_|ERROR)?;let bot=bots.entry(id.into()).or_default();
    if expected.is_some_and(|signals|!Arc::ptr_eq(signals,&bot.signals)){return Ok(());}
    // Read authoritative activity after serializing layout changes. A delayed
    // start/profile callback cannot expand a window after a newer Stop restored it.
    let profile=state.store.bot_profile(id)?;let config=crate::autopilot::config(&profile)?;let runtime=state.autopilot.runtime(id,&config);
    let signals=bot.signals.clone();let native=NativeWindow{window:&window,signals:&signals};
    let available=if runtime.active&&runtime.wander{displays(&window)?}else{displays(&window).unwrap_or_default()};
    let target=if runtime.active&&runtime.wander&&profile["output"]["mode"]=="overlay"{
        let normal=bot.normal.as_ref().map(|normal|normal.bounds).or(bot.initial_bounds).unwrap_or(native.bounds()?);
        let preferred=bot.monitor.as_deref().unwrap_or_else(||profile["output"]["desktopMonitorId"].as_str().unwrap_or_default());
        Some(choose(&available,preferred,normal).ok_or("Kein Ausgabemonitor verfügbar.")?.bounds)
    }else{None};
    if target.is_none()&&bot.normal.is_none(){finish_initial(bot,&native,attempt>=20)?;}
    // A disconnected monitor must not restore the fixed window off screen.
    if target.is_none(){if let Some(normal)=&mut bot.normal{if !available.iter().any(|item|overlaps(item.bounds,normal.bounds)){if let Some(item)=available.first(){normal.bounds.x=item.bounds.x;normal.bounds.y=item.bounds.y;normal.restore=None;}}}}
    let result=transition(bot,target,profile["output"]["clickThrough"].as_bool().unwrap_or(false),&native,|bounds|state.store.scoped(id)?.save_window_bounds(&serde_json::to_value(bounds).map_err(|_|ERROR)?));
    let pending=if let Some(target)=target{native.bounds().map(|bounds|bounds!=target).unwrap_or(true)||signals.has_pending_move()||signals.has_pending_resize()}else{bot.normal.is_some()||bot.initial_bounds.is_some()};
    // Coalescing GTK requests may never acknowledge an intermediate expansion.
    // Retire that still-click-through handle and reopen from the persisted
    // fixed bounds instead of claiming an unconfirmed restore succeeded.
    let replace=needs_replacement(bot,target,pending,attempt);
    if replace {bot.replacing=true;}
    drop(bots);
    // Publication is mandatory even when a partial native mutation failed.
    publish(app,&profile);
    if replace {
        window.set_ignore_cursor_events(true).map_err(|_|ERROR)?;
        window.close().map_err(|_|ERROR)?;
        return result;
    }
    if pending {
        if attempt>=20{return Err("Das Betriebssystem hat die Fensterposition oder Größe nicht bestätigt. Die Klickdurchleitung bleibt zum Schutz des Desktops aktiv.".into());}
        schedule(app,id,attempt+1,50,Some(signals));
    }
    result?;
    Ok(())
}
/// Never hold a Hub lock in a background thread while a Wry getter waits for
/// the main thread. Window event callbacks run on that same serialized thread.
pub fn sync(app:&tauri::AppHandle,id:&str)->Result<(),String>{
    schedule(app,id,0,0,None);Ok(())
}
fn schedule(app:&tauri::AppHandle,id:&str,attempt:u8,delay:u64,expected:Option<Arc<WindowSignals>>){
    let handle=app.clone();let id=id.to_owned();
    // Wry executes run_on_main_thread inline when already on Main. Submit from
    // the async worker instead, so a setter's ScaleFactorChanged callback can
    // never reenter the still-locked transition on the same stack.
    tauri::async_runtime::spawn(async move{
        if delay>0{tokio::time::sleep(std::time::Duration::from_millis(delay)).await;}
        let main=handle.clone();let target=id.clone();
        if handle.run_on_main_thread(move||{if let Err(error)=sync_on_main(&main,&target,attempt,expected.as_ref()){use tauri::Emitter;let _=main.emit("cohost://agent",json!({"kind":"error","botId":target,"text":error}));}}).is_err(){use tauri::Emitter;let _=handle.emit("cohost://agent",json!({"kind":"error","botId":id,"text":ERROR}));}
    });
}
pub fn sync_or_report(app:&tauri::AppHandle,id:&str){if let Err(error)=sync(app,id){use tauri::Emitter;let _=app.emit("cohost://agent",json!({"kind":"error","botId":id,"text":error}));}}
#[tauri::command]
pub fn list_output_displays(window:tauri::WebviewWindow)->Result<Vec<Display>,String>{main_only(window.label())?;displays(&window)}
#[tauri::command]
pub fn preview_avatar_scale(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String,value:f64)->Result<Value,String>{
    main_only(window.label())?;validate_scale(value)?;let state=app.state::<crate::AppState>();state.updater.ensure_idle()?;let profile=state.store.bot_profile(&bot_id)?;
    {let mut bots=state.presentation.bots.lock().map_err(|_|ERROR)?;let bot=bots.entry(bot_id).or_default();bot.scale=Some(value);advance(bot);}
    publish(&app,&profile);Ok(state.presentation.snapshot(&profile))
}
#[tauri::command]
pub fn preview_output_monitor(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String,monitor_id:String)->Result<(),String>{
    main_only(window.label())?;let state=app.state::<crate::AppState>();state.updater.ensure_idle()?;state.store.bot_profile(&bot_id)?;
    if !monitor_id.is_empty()&&!displays(&window)?.iter().any(|item|item.id==monitor_id){return Err("Der gewählte Ausgabemonitor ist nicht mehr verfügbar.".into());}
    {let mut bots=state.presentation.bots.lock().map_err(|_|ERROR)?;let bot=bots.entry(bot_id.clone()).or_default();bot.monitor=Some(monitor_id);advance(bot);}
    sync(&app,&bot_id)
}

#[cfg(test)]
mod tests{
    use super::*;use std::cell::{Cell,RefCell};
    struct Fake{bounds:Cell<Bounds>,clicks:Cell<bool>,failures:RefCell<std::collections::VecDeque<&'static str>>,signals:WindowSignals}
    impl Fake{fn acknowledge_late_geometry(&self){for b in [monitor(),normal()]{self.bounds.set(b);self.signals.observe_resize(b.width,b.height);self.signals.observe_move(b.x,b.y);}}fn check(&self,operation:&str)->Result<(),String>{let mut failures=self.failures.borrow_mut();if failures.front().copied()==Some(operation){failures.pop_front();Err(format!("fixture {operation}"))}else{Ok(())}}}
    impl WindowOps for Fake{
        fn bounds(&self)->Result<Bounds,String>{Ok(self.bounds.get())}
        fn size(&self,b:Bounds)->Result<(),String>{let c=self.bounds.get();if(c.width,c.height)==(b.width,b.height)&&!self.signals.has_pending_resize(){return Ok(());}self.signals.request_resize(b.width,b.height);self.check("size")?;self.bounds.set(Bounds{width:b.width,height:b.height,..self.bounds.get()});self.signals.observe_resize(b.width,b.height);Ok(())}
        fn position(&self,b:Bounds)->Result<(),String>{let c=self.bounds.get();if(c.x,c.y)==(b.x,b.y)&&!self.signals.has_pending_move(){return Ok(());}self.signals.request_move(b.x,b.y);self.check("position")?;self.bounds.set(Bounds{x:b.x,y:b.y,..self.bounds.get()});self.signals.observe_move(b.x,b.y);Ok(())}
        fn clicks(&self,b:bool)->Result<(),String>{self.check("clicks")?;self.clicks.set(b);Ok(())}
        fn signals(&self)->&WindowSignals{&self.signals}
    }
    fn normal()->Bounds{Bounds{x:-2100,y:120,width:1040,height:1440}}
    fn monitor()->Bounds{Bounds{x:-2560,y:0,width:2560,height:1600}}
    fn fake()->Fake{Fake{bounds:Cell::new(normal()),clicks:Cell::new(false),failures:RefCell::new(std::collections::VecDeque::new()),signals:WindowSignals::default()}}
    #[test]fn physical_negative_origin_transition_is_reversible_and_saves_only_normal(){let window=fake();let mut bot=Bot::default();let saved=Cell::new(None);transition(&mut bot,Some(monitor()),false,&window,|b|{saved.set(Some(b));Ok(())}).unwrap();assert_eq!(saved.get(),Some(normal()));assert_eq!(window.bounds.get(),monitor());assert!(window.clicks.get());transition(&mut bot,Some(Bounds{x:0,..monitor()}),true,&window,|_|panic!("never snapshot twice")).unwrap();transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();assert_eq!(window.bounds.get(),normal());assert!(!window.clicks.get());assert!(bot.normal.is_none());}
    #[test]fn failed_native_mutations_keep_restore_snapshot_and_bots_are_isolated(){let first=fake();let second=fake();let mut a=Bot::default();let mut b=Bot::default();first.failures.borrow_mut().push_back("size");assert!(transition(&mut a,Some(monitor()),false,&first,|_|Ok(())).is_err());assert!(a.normal.is_some());assert!(b.normal.is_none());transition(&mut b,Some(monitor()),true,&second,|_|Ok(())).unwrap();first.acknowledge_late_geometry();transition(&mut a,None,false,&first,|_|Ok(())).unwrap();assert!(!a.normal.is_some());assert!(b.normal.as_ref().unwrap().expanded);}
    #[test]fn partial_expansion_and_failed_rollback_never_unmask_monitor_sized_window(){
        let window=fake();let mut bot=Bot::default();window.failures.borrow_mut().extend(["position","size"]);
        assert!(transition(&mut bot,Some(monitor()),false,&window,|_|Ok(())).is_err());
        assert_eq!(window.bounds.get().width,monitor().width);assert!(window.clicks.get());assert!(bot.normal.as_ref().unwrap().expanded);
        window.acknowledge_late_geometry();transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();
        // Failed rollback never issued one field's restoring request. Getter
        // equality cannot manufacture its missing acknowledgement: replacement
        // is required while the old handle remains safely click-through.
        assert_eq!(window.bounds.get(),normal());assert!(window.clicks.get());assert!(bot.normal.is_some());
        assert!(!needs_replacement(&bot,None,true,19));assert!(needs_replacement(&bot,None,true,20));assert_eq!(bot.normal.as_ref().unwrap().bounds,normal());
    }
    #[test]fn restore_failures_keep_guard_and_retry_uses_latest_saved_cursor_setting(){
        for failure in ["size","position","clicks"]{let window=fake();let mut bot=Bot::default();transition(&mut bot,Some(monitor()),false,&window,|_|Ok(())).unwrap();window.failures.borrow_mut().push_back(failure);assert!(transition(&mut bot,None,false,&window,|_|unreachable!()).is_err());assert!(window.clicks.get());assert!(bot.normal.is_some());if failure!="clicks"{window.acknowledge_late_geometry();}transition(&mut bot,None,true,&window,|_|unreachable!()).unwrap();assert!(window.clicks.get());assert_eq!(window.bounds.get(),normal());
            if failure=="size"{assert!(bot.normal.is_some());assert!(needs_replacement(&bot,None,true,20));}else{assert!(bot.normal.is_none(),"restore after {failure}: {:?}",bot.normal);}
        }
    }
    struct Delayed { inner:Fake, pending:RefCell<std::collections::VecDeque<(bool,Bounds)>>, grid:f64 }
    impl Delayed {
        fn flush_one(&self){if let Some((resize,b))=self.pending.borrow_mut().pop_front(){if resize{self.inner.bounds.set(Bounds{width:b.width,height:b.height,..self.inner.bounds.get()});self.inner.signals.observe_resize(b.width,b.height);}else{self.inner.bounds.set(Bounds{x:b.x,y:b.y,..self.inner.bounds.get()});self.inner.signals.observe_move(b.x,b.y);}}}
        fn flush(&self){while !self.pending.borrow().is_empty(){self.flush_one();}}
    }
    impl WindowOps for Delayed {
        fn bounds(&self)->Result<Bounds,String>{self.inner.bounds()}
        fn size(&self,b:Bounds)->Result<(),String>{self.inner.signals.request_resize(b.width,b.height);self.pending.borrow_mut().push_back((true,b));Ok(())}
        fn position(&self,b:Bounds)->Result<(),String>{self.inner.signals.request_move(b.x,b.y);self.pending.borrow_mut().push_back((false,b));Ok(())}
        fn clicks(&self,b:bool)->Result<(),String>{self.inner.clicks(b)}
        fn signals(&self)->&WindowSignals{&self.inner.signals}
        fn normalize(&self,b:Bounds)->Result<Bounds,String>{Ok(crate::window_signals::normalize_physical_bounds(b,self.grid))}
    }
    #[test]fn delayed_expansion_then_stop_keeps_clicks_guarded_until_actual_restore_ack(){
        let window=Delayed{inner:fake(),pending:RefCell::default(),grid:1.0};let mut bot=Bot::default();
        transition(&mut bot,Some(monitor()),false,&window,|_|Ok(())).unwrap();
        // Getter still equals normal, but pending expansion cannot be ignored.
        transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();
        assert!(bot.normal.is_some());assert!(window.inner.clicks.get());
        // An old normal event after the fence cannot jump the expansion queue.
        window.inner.signals.observe_resize(normal().width,normal().height);window.inner.signals.observe_move(normal().x,normal().y);
        transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();assert!(bot.normal.is_some());
        window.flush();transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();
        assert!(bot.normal.is_none());assert!(!window.inner.clicks.get());assert_eq!(window.inner.bounds.get(),normal());
    }
    #[test]fn dpi_grid_restore_finishes_without_a_broad_geometry_tolerance(){
        let window=Delayed{inner:fake(),pending:RefCell::default(),grid:2.0};let mut bot=Bot::default();
        window.inner.bounds.set(Bounds{x:-2101,y:121,width:1041,height:1441});let original=window.inner.bounds.get();
        transition(&mut bot,Some(monitor()),false,&window,|_|Ok(())).unwrap();window.flush();
        transition(&mut bot,None,true,&window,|_|unreachable!()).unwrap();assert!(bot.normal.is_some());
        window.flush();transition(&mut bot,None,true,&window,|_|unreachable!()).unwrap();
        assert!(bot.normal.is_none());assert_eq!(window.inner.bounds.get(),crate::window_signals::normalize_physical_bounds(original,2.0));assert!(window.inner.clicks.get());
    }
    #[test]fn window_generation_prevents_old_destroyed_callbacks_from_clearing_new_state(){
        let hub=PresentationHub::default();let old=Arc::new(WindowSignals::default());let new=Arc::new(WindowSignals::default());
        hub.register_window("nova",old.clone(),None).unwrap();hub.register_window("nova",new.clone(),None).unwrap();
        assert_eq!(hub.forget_window("nova",&old),None);assert!(hub.current_window("nova",&new));
        assert_eq!(hub.forget_window("nova",&new),Some(false));
    }
    #[test]fn init_only_manual_move_releases_save_guard_after_bounded_wait(){
        let window=fake();let mut bot=Bot{initial_bounds:Some(Bounds{x:100,..normal()}),..Bot::default()};
        finish_initial(&mut bot,&window,false).unwrap();assert!(bot.initial_bounds.is_some());
        finish_initial(&mut bot,&window,true).unwrap();assert!(bot.initial_bounds.is_none());assert!(bot.normal.is_none());assert!(!window.clicks.get());
    }
    #[test]fn active_restart_reasserts_monitor_after_pending_stop_restore(){
        let window=Delayed{inner:fake(),pending:RefCell::default(),grid:1.0};let mut bot=Bot::default();
        transition(&mut bot,Some(monitor()),false,&window,|_|Ok(())).unwrap();window.flush();
        transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();assert_eq!(window.inner.bounds.get(),monitor());
        transition(&mut bot,Some(monitor()),false,&window,|_|unreachable!()).unwrap();window.flush();
        assert_eq!(window.inner.bounds.get(),monitor());assert!(bot.normal.as_ref().unwrap().expanded);assert!(window.inner.clicks.get());
        transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();window.flush();transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();assert!(bot.normal.is_none());
    }
    #[test]fn opening_active_overlay_keeps_saved_normal_bounds_despite_stale_builder_getters(){
        let window=fake();window.bounds.set(Bounds{x:0,y:0,width:520,height:720});
        let mut bot=Bot{initial_bounds:Some(normal()),..Bot::default()};let saved=Cell::new(None);
        transition(&mut bot,Some(monitor()),false,&window,|b|{saved.set(Some(b));Ok(())}).unwrap();
        assert_eq!(saved.get(),Some(normal()));assert_eq!(bot.normal.as_ref().unwrap().bounds,normal());
        transition(&mut bot,None,false,&window,|_|unreachable!()).unwrap();assert_eq!(window.bounds.get(),normal());
    }
    #[test]fn preview_values_and_main_window_gate_fail_closed(){for value in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,0.0,0.299,2.501]{assert!(validate_scale(value).is_err());}for value in [0.3,1.0,2.5]{assert!(validate_scale(value).is_ok());}for label in ["avatar","avatar-nova",""]{assert!(main_only(label).is_err());}assert!(main_only("main").is_ok());}
    #[test]fn snapshot_survives_new_receivers_without_mutating_profile_or_grants(){let hub=PresentationHub::default();let profile=json!({"id":"nova","avatar":{"scale":1.0},"model":{"visionEnabled":false}});{let mut bots=hub.bots.lock().unwrap();let bot=bots.entry("nova".into()).or_default();bot.scale=Some(1.7);bot.normal=Some(Normal{bounds:normal(),expanded:true,changed_move:true,changed_size:true,restore:None});advance(bot);}let value=hub.snapshot(&profile);assert_eq!(value["scale"],1.7);assert_eq!(value["desktop"]["referencePhysicalHeight"],1440);assert_eq!(profile["avatar"]["scale"],1.0);assert_eq!(profile["model"]["visionEnabled"],false);assert_eq!(hub.snapshot(&json!({"id":"other"}))["desktop"]["active"],false);}
    #[test]fn automatic_monitor_uses_largest_physical_overlap(){let make=|id:&str,bounds|Display{id:id.into(),name:id.into(),bounds,scale_factor:2.0};let monitors=[make("left",Bounds{x:-2560,y:0,width:2560,height:1600}),make("right",Bounds{x:0,y:0,width:1920,height:1080})];let normal=Bounds{x:-8,y:100,width:1000,height:700};assert_eq!(choose(&monitors,"",normal).unwrap().id,"right");assert_eq!(choose(&monitors,"left",normal).unwrap().id,"left");}
}
