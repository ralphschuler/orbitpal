//! Main-only native action intent. Public IDs select a one-use operator request;
//! the actual retained OwnedSurface remains the sole NativeActionPermit factory.
use super::{action_host, action_state::{LocalFence},
    native_scene::{NativeActionPermit, NativeMember}, LifeRuntime};
use crate::{world_output::OutputProof, world_state::{coordinates::Point, SlotId}};
use serde::Serialize;
use serde_json::Value;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use tauri::Manager;
const ERROR:&str="Die Desktopaktion benötigt eine aktuelle, freie native Figur. Bitte erneut versuchen.";
pub(super) const INTENT_MS:u64=2_000;
pub(super) fn intent_fresh(issued:u64,now:u64)->bool{now>=issued&&now-issued<INTENT_MS}

#[derive(Clone)]
pub(super) struct NativeIntent {
    pub id:String,
    pub member:Arc<NativeMember>,
    pub permit:NativeActionPermit,
    pub fence:LocalFence,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct PreparedNativeAction { version:u16, intent_id:String, expires_in_ms:u64 }
pub(super) enum NativeMoveGoal { Point(Point), Slot(SlotId) }

/// No actual native constructor means the existing Studio path may be used.
/// A pending/restoring constructor is an explicit error, never a Studio fallback.
#[tauri::command(async)]
pub(crate) fn prepare_native_life_action(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String)->Result<Option<PreparedNativeAction>,String>{
    if window.label()!="main"||!crate::storage::valid_id(&bot_id){return Err(ERROR.into());}
    let state=app.state::<crate::AppState>();state.updater.ensure_idle()?;
    let members=state.life_runtime.native_scenes.members(Some(&bot_id));
    if members.is_empty(){
        if state.life_runtime.session_outputs.retained_constructors()?.iter().any(|c|c.bot()==bot_id){return Err(ERROR.into());}
        return Ok(None);
    }
    if members.len()!=1{return Err(ERROR.into());}
    let member=members.into_iter().next().ok_or(ERROR)?;
    if !state.life_runtime.session_outputs.constructor_current(&member.constructor){return Err(ERROR.into());}
    // Cancellation is requested before acquiring the generation turn. An in-flight
    // provider remains responsible for observing its real cancellation atom.
    if let Ok(cancel)=state.cancel.lock(){if let Some(cancel)=cancel.as_ref(){cancel.store(true,Ordering::SeqCst);}}
    state.overlay.invalidate_speech(None);
    let _generation=state.generation.try_lock().map_err(|_|ERROR)?;
    // Existing measurement/delivery lock order: Owned -> Core -> World -> Store.
    let owned_guard=member.owned.lock().map_err(|_|ERROR)?;
    let owned=owned_guard.as_ref().ok_or(ERROR)?;
    let mut core=state.life_runtime.core.lock().map_err(|_|ERROR)?;
    let now=state.life_runtime.now();
    if !member.current()||!owned.context.current(&core,&state,now){return Err(ERROR.into());}
    let permit=owned.action_permit(now).ok_or(ERROR)?;
    let output=core.world_outputs.ready(&bot_id,now).map_err(|_|ERROR)?;
    if !permit.current(&core,now)||!permit.matches_output(&output){return Err(ERROR.into());}
    // Ordinary operator cancellation and quieting remain real; this narrow
    // operation preserves the exact native owner's clean UI generation.
    core.activity(now);
    action_host::poll(&mut core,&state.store,&state.world,||state.life_runtime.now(),super::wall_ms)?;
    let fence=core.local_fence(&bot_id,state.life_runtime.now()).ok_or(ERROR)?;
    let generation=state.life_runtime.outputs.ticket().ok_or(ERROR)?;
    let intent=core.prepare_native_action_intent(&bot_id,owned.context.profile.clone(),
        NativeIntent{id:uuid::Uuid::new_v4().to_string(),member:member.clone(),permit,fence},generation,state.life_runtime.now()).ok_or(ERROR)?;
    let native=intent.native.as_ref().ok_or(ERROR)?;
    let result=PreparedNativeAction{version:1,intent_id:native.id.clone(),expires_in_ms:INTENT_MS};
    drop(core);drop(owned_guard);drop(_generation);
    super::world_output::publish(&app);super::actions::publish(&app);
    // Core's monotonic clock expires this exact one-use map entry. There is
    // no detached timer per click; repeated preparations retain bounded state.
    Ok(Some(result))
}

struct IntentGuard<'a>{runtime:&'a LifeRuntime,bot:String,serial:u64}
impl Drop for IntentGuard<'_>{fn drop(&mut self){if let Ok(mut core)=self.runtime.core.lock(){core.abandon_rest_intent(&self.bot,self.serial);}}}

async fn start(app:tauri::AppHandle,bot:String,id:String,goal:Option<NativeMoveGoal>)->Result<Value,String>{
    if id.len()!=36||uuid::Uuid::parse_str(&id).is_err(){return Err(ERROR.into());}
    let state=app.state::<crate::AppState>();state.updater.ensure_idle()?;
    let intent=state.life_runtime.core.lock().map_err(|_|ERROR)?.native_action_intent(&bot,&id).ok_or(ERROR)?;
    let _intent=IntentGuard{runtime:&state.life_runtime,bot:bot.clone(),serial:intent.serial};
    let native=intent.native.as_ref().ok_or(ERROR)?.clone();
    let monitor=state.life_runtime.monitor.get().ok_or(ERROR)?;
    let system=monitor.refresh().await;
    let topology=state.world.sample_topology(&app).await;
    state.updater.ensure_idle()?;
    if state.overlay.has_any_pending_speech(){return Err(ERROR.into());}
    let _generation=state.generation.try_lock().map_err(|_|ERROR)?;
    let cancel=Arc::new(AtomicBool::new(false));
    if !state.life_runtime.outputs.watch_delivery(intent.output_generation,&bot,native.member.constructor.window().label(),&cancel)
        || monitor.gate.watch(&cancel).is_none(){return Err(ERROR.into());}
    state.life_runtime.session_outputs.bind_surface(&native.member.constructor,&cancel)?;
    let owned_guard=native.member.owned.lock().map_err(|_|ERROR)?;
    let owned=owned_guard.as_ref().ok_or(ERROR)?;
    let mut core=state.life_runtime.core.lock().map_err(|_|ERROR)?;
    core.observe_system(system,state.life_runtime.now());
    state.world.synchronize(&state.store,&crate::world_hub::current_actors(&core),Some(topology),state.life_runtime.now())?;
    if !native.member.current()||!owned.context.current(&core,&state,state.life_runtime.now())
        || !state.life_runtime.session_outputs.constructor_current(&native.member.constructor){return Err(ERROR.into());}
    let source=owned.context.source(&core,&state,owned.route.clone(),owned.framing,state.life_runtime.now())?;
    if !core.world_outputs.matches(&bot,&source){return Err(ERROR.into());}
    let output:OutputProof=core.world_outputs.ready(&bot,state.life_runtime.now()).map_err(|_|ERROR)?;
    if !native.permit.current(&core,state.life_runtime.now())||!native.permit.matches_output(&output){return Err(ERROR.into());}
    let fence=core.consume_rest_intent(&intent,&owned.context.profile,state.life_runtime.now()).ok_or(ERROR)?;
    let context=source.context;
    let result=match goal{
        Some(NativeMoveGoal::Point(target))=>action_host::admit_native_move(&mut core,&state.store,&state.world,&bot,&intent.profile,fence,context,output,native.permit,target,cancel,||state.life_runtime.now(),super::wall_ms),
        Some(NativeMoveGoal::Slot(slot))=>action_host::admit_slot_move(&mut core,&state.store,&state.world,&bot,&intent.profile,fence,context,output,native.permit,slot,cancel,||state.life_runtime.now(),super::wall_ms),
        None=>action_host::admit_native_rest(&mut core,&state.store,&state.world,&bot,&intent.profile,fence,context,output,native.permit,cancel,||state.life_runtime.now(),super::wall_ms),
    };
    let _=action_host::poll(&mut core,&state.store,&state.world,||state.life_runtime.now(),super::wall_ms);
    let status=action_host::status(&core,&state.store,&state.world,&bot,state.life_runtime.now());
    drop(core);drop(owned_guard);drop(_generation);
    super::world_output::publish(&app);super::actions::publish(&app);crate::world_hub::publish(&app);
    result?;status
}
pub(super) async fn start_move(app:tauri::AppHandle,bot:String,id:String,target:Option<Point>,slot:Option<String>)->Result<Value,String>{
    let goal=match(target,slot){(Some(point),None)=>NativeMoveGoal::Point(point),(None,Some(slot))=>NativeMoveGoal::Slot(SlotId::new(&slot).map_err(|_|ERROR)?),_=>return Err(ERROR.into())};
    start(app,bot,id,Some(goal)).await
}
pub(super) async fn start_rest(app:tauri::AppHandle,bot:String,id:String)->Result<Value,String>{start(app,bot,id,None).await}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_use_native_intent_has_its_own_hard_monotonic_deadline(){
        assert!(intent_fresh(100,100));assert!(intent_fresh(100,2099));
        assert!(!intent_fresh(100,2100));assert!(!intent_fresh(100,2101));
        assert!(!intent_fresh(100,99));
    }
    #[test]
    fn public_native_intent_response_contains_no_permit_constructor_profile_or_action_grant(){
        let response=PreparedNativeAction{version:1,intent_id:uuid::Uuid::new_v4().to_string(),expires_in_ms:INTENT_MS};
        let data=serde_json::to_value(response).unwrap();
        let fields=data.as_object().unwrap();assert_eq!(fields.len(),3);
        assert_eq!(data["expiresInMs"],2000);assert!(data["intentId"].as_str().is_some());
        for field in ["nativeSource","capability","permit","owner","profile","actionId","sessionId"]{assert!(data.get(field).is_none());}
    }
}
