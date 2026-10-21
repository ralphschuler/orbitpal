//! Exact native World command delivery. Evaluation is transport only: only the
//! existing authenticated report_native_world_action may acknowledge delivery,
//! Start, progress or completion. No proof or permit is manufactured here.
use super::{action_host, native_scene::{NativeMember, NativeRouteProof}, native_scene_document::NavigationPolicy};
use crate::world_output::MotionCommand;
use std::{collections::VecDeque, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}};
use tauri::Manager;

const LIMIT: usize = 8;
const DELIVERY_MS: u64 = 2_000;
const PAYLOAD_BYTES: usize = 16 * 1024;

struct Work { command: MotionCommand, route: NativeRouteProof, deadline: u64 }
struct State<T> { queued: VecDeque<T>, running: bool, in_flight: bool }
impl<T> Default for State<T> {
    fn default() -> Self { Self { queued: VecDeque::new(), running:false, in_flight:false } }
}
impl<T> State<T> {
    fn push(&mut self, value:T) -> Result<bool, ()> {
        if self.queued.len()+usize::from(self.in_flight) >= LIMIT { return Err(()); }
        self.queued.push_back(value);
        let spawn=!self.running;self.running=true;Ok(spawn)
    }
    fn pop(&mut self) -> Result<Option<T>, ()> {
        if self.in_flight { return Err(()); }
        let next=self.queued.pop_front();
        if next.is_some() { self.in_flight=true; } else { self.running=false; }
        Ok(next)
    }
    fn drained(&mut self) { self.in_flight=false; }
}
#[derive(Default)]
pub(super) struct DispatchQueue { state: Mutex<State<Work>> }

/// Called after the Registry/Core drain, never with Core held. Looking up the
/// exact retained constructor cannot transfer an old command to a new member.
pub(super) fn enqueue(app:&tauri::AppHandle, command:MotionCommand, route:NativeRouteProof, issued_at:u64) {
    let state=app.state::<crate::AppState>();
    let Some(member)=state.life_runtime.native_scenes.for_constructor(route.constructor()) else {
        reject(app,&command);return;
    };
    if !member.current() || !route.matches_auth(&member.auth) || !member.constructor.same_constructor(route.constructor()) {
        reject(app,&command);return;
    }
    let Some(deadline)=issued_at.checked_add(DELIVERY_MS).filter(|n| *n<=crate::life_domain::MAX_COUNTER) else {
        reject(app,&command);return;
    };
    let work=Work { command:command.clone(), route, deadline };
    let spawn=member.dispatch.state.lock().ok().and_then(|mut queue|queue.push(work).ok());
    match spawn {
        Some(true)=>{let handle=app.clone();tauri::async_runtime::spawn(async move {run(&handle,member).await;});}
        Some(false)=>(),
        None=>{member.cancel();reject(app,&command);}
    }
}

struct ActionWatch {
    action: Arc<AtomicBool>,
    output: Arc<AtomicBool>,
    stream_source: Option<crate::platform_impulses::StreamActionFence>,
    window_rest: Option<WindowRestWatch>,
}
struct WindowRestWatch {
    guard: super::desktop_places::WindowRestGuard,
    bot: String,
    output: crate::world_output::OutputProof,
}
impl ActionWatch {
    /// Main checks retained cancellation/deadline/geometry evidence only. The
    /// rest guard's short freshness lock is released before any native getter.
    /// Terminals retire exact existing work even after its source was revoked.
    fn current(&self, terminal: bool, now: u64) -> bool {
        terminal || (!self.action.load(Ordering::SeqCst)
            && !self.output.load(Ordering::SeqCst)
            && self.stream_source.as_ref().is_none_or(|source| source.current())
            && self.window_rest.as_ref().is_none_or(|rest| rest.guard.current(&rest.bot, &rest.output, now)))
    }
}
fn validate(app:&tauri::AppHandle, member:&NativeMember, work:&Work) -> Option<ActionWatch> {
    let state=app.state::<crate::AppState>();
    let now=state.life_runtime.now();
    if now>=work.deadline || !member.current() || !work.route.matches_auth(&member.auth)
        || !member.constructor.same_constructor(work.route.constructor()) { return None; }
    // Preserve the existing exact physical owner order: Owned -> Core ->
    // World/Store. Never acquire Owned while a Core drain is still held.
    let owned=member.owned.lock().ok()?;
    let owned=owned.as_ref()?;
    let core=state.life_runtime.core.lock().ok()?;
    if !owned.context.current(&core,&state,state.life_runtime.now()) { return None; }
    // World/Store read-through may wait. Recompute freshness after those waits.
    let now=state.life_runtime.now();
    if now>=work.deadline || !member.current()
        || !owned.route.same_route(&work.route) || !work.route.live_current(now)
        || !work.route.context_current(&core,now)
        || !core.world_outputs.native_command_current(&work.command,&work.route) { return None; }
    let fence=work.command.fence();
    let job=core.actions.as_ref()?.jobs.get(&fence.actor_id)?;
    let visual=job.visual.as_ref()?;
    if job.key.action_id()!=&fence.action_id || visual.fence.as_ref()!=Some(fence)
        || !job.native_permit.as_ref().is_some_and(|permit|permit.current(&core,now)&&permit.matches_output(&visual.output))
        || !matches!(&visual.output.route, crate::world_output::SourceRoute::Native(route) if route.same_route(&work.route)) { return None; }
    let watch=ActionWatch { action:job.cancel.clone(),output:visual.output.cancel.clone(),stream_source:job.stream_source.clone(),
        window_rest:job.window_rest.as_ref().map(|guard|WindowRestWatch {guard:guard.clone(),bot:fence.actor_id.clone(),output:visual.output.clone()}) };
    watch.current(work.command.terminal(),state.life_runtime.now()).then_some(watch)
}

async fn run(app:&tauri::AppHandle, member:Arc<NativeMember>) {
    loop {
        let next=member.dispatch.state.lock().ok().and_then(|mut queue|queue.pop().ok());
        let work=match next {Some(Some(work))=>work,Some(None)=>return,None=>{member.cancel();return;}};
        let result=if let Some(watch)=validate(app,&member,&work) { deliver(app,&member,&work,watch).await } else { false };
        // The charge remains in_flight until its actual Main closure has drained.
        // No local timeout detaches a queued native operation or permits overlap.
        if let Ok(mut queue)=member.dispatch.state.lock(){queue.drained();}else{member.cancel();return;}
        if !result { reject(app,&work.command); }
    }
}

async fn deliver(app:&tauri::AppHandle, member:&Arc<NativeMember>, work:&Work, watch:ActionWatch) -> bool {
    let Ok(policy)=NavigationPolicy::for_app(app,member.constructor.bot()) else {return false;};
    let Ok(payload)=serde_json::to_string(&work.command) else {return false;};
    if payload.len()>PAYLOAD_BYTES {return false;}
    let script=format!("window.__orbitpalNativeWorldCommand?.({payload});");
    let deadline=work.deadline;let route=work.route.clone();let exact=member.clone();let terminal=work.command.terminal();
    let (tx,rx)=tokio::sync::oneshot::channel();let main=app.clone();
    if app.run_on_main_thread(move||{
        let state=main.state::<crate::AppState>();let now=state.life_runtime.now();
        // Main inspects retained identities, cancellation and freshness only. No Owned,
        // Core, World or Store lock is taken while calling native GUI getters.
        let current=now<deadline&&exact.current()&&route.live_current(now)&&route.matches_auth(&exact.auth)
            &&exact.constructor.same_constructor(route.constructor())
            &&watch.current(terminal,state.life_runtime.now())
            &&state.life_runtime.session_outputs.constructor_current(&exact.constructor)
            &&state.presentation.current_window(exact.constructor.bot(),exact.constructor.signals())
            &&!state.overlay.has_any_pending_speech()&&state.updater.ensure_idle().is_ok()
            &&state.generation.try_lock().is_ok();
        let observed=current&&watch.current(terminal,state.life_runtime.now())
            &&exact.constructor.window().url().is_ok_and(|url|policy.permits(&url))
            &&watch.current(terminal,state.life_runtime.now())
            &&exact.constructor.window().is_visible().ok()==Some(true);
        // Native getters are observations, not a lease on their earlier time.
        let evaluated=observed&&state.life_runtime.now()<deadline&&exact.current()
            &&route.live_current(state.life_runtime.now())
            &&watch.current(terminal,state.life_runtime.now())
            &&exact.constructor.window().eval(&script).is_ok();
        let _=tx.send(evaluated);
    }).is_err(){return false;}
    // The closure checks a fixed deadline; awaiting its completion retains one
    // worker/queue charge even if Main is delayed past that deadline.
    rx.await.unwrap_or(false)
}

/// Cancellation only, matched to the exact job/fence. An obsolete descriptor
/// cannot fail a successor action, award an effect or fabricate an ACK.
pub(super) fn reject(app:&tauri::AppHandle,command:&MotionCommand) {
    let state=app.state::<crate::AppState>();
    if let Ok(mut core)=state.life_runtime.core.lock(){
        let now=state.life_runtime.now();
        let changed=if let Some(actions)=core.actions.as_mut(){
            let exact=actions.jobs.get(&command.fence().actor_id).filter(|job|
                job.key.action_id()==&command.fence().action_id
                    &&job.visual.as_ref().is_some_and(|visual|visual.fence.as_ref()==Some(command.fence())))
                .map(|job|(job.authority.clone(),job.key.clone(),job.cancel.clone()));
            if let Some((authority,key,cancel))=exact{
                cancel.store(true,Ordering::SeqCst);
                let _=actions.executor.fail(&authority,&key,crate::action_executor::Failure::OutputLost,now);
                true
            }else{false}
        }else{false};
        if changed { let _=action_host::poll(&mut core,&state.store,&state.world,||state.life_runtime.now(),super::wall_ms); }
        drop(core);
        if changed {super::world_output::publish(app);super::actions::publish(app);crate::world_hub::publish(app);}
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{platform_impulses::StreamActionFence, storage::Store};
    use std::{path::PathBuf, sync::atomic::AtomicU64};

    struct Fixture { root: PathBuf, store: Option<Store> }
    impl Fixture {
        fn new() -> Self {
            let root=std::env::temp_dir().join(format!("orbitpal-dispatch-source-{}",uuid::Uuid::new_v4()));
            let store=Store::new(root.clone()).unwrap();
            Self {root,store:Some(store)}
        }
        fn store(&self)->&Store {self.store.as_ref().unwrap()}
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.store.take());
            let _=std::fs::remove_dir_all(&self.root);
        }
    }
    fn watch(stream_source: Option<StreamActionFence>) -> ActionWatch {
        ActionWatch {action:Arc::new(AtomicBool::new(false)),output:Arc::new(AtomicBool::new(false)),stream_source,window_rest:None}
    }
    #[test]
    fn queued_delivery_rechecks_original_source_deadline_without_waiting_for_poll_cancellation() {
        let fixture=Fixture::new();let clock=Arc::new(AtomicU64::new(0));
        let (source,_context)=StreamActionFence::controlled_for_test(fixture.store(),clock.clone(),100).unwrap();
        let watch=watch(Some(source));
        assert!(watch.current(false,0),"validation before the Main queue is still current");
        clock.store(99,Ordering::SeqCst);
        assert!(watch.current(false,99),"the pre-getter boundary still has time");
        clock.store(100,Ordering::SeqCst);
        assert!(!watch.action.load(Ordering::SeqCst));assert!(!watch.output.load(Ordering::SeqCst));
        assert!(!watch.current(false,100),"post-getter/eval must refuse even before the next native poll");
        assert!(watch.current(true,100),"an exact terminal still retires expired work");
    }
    #[test]
    fn disconnected_source_is_rechecked_even_when_delivery_tokens_have_not_changed() {
        let fixture=Fixture::new();let clock=Arc::new(AtomicU64::new(0));
        let (source,mut context)=StreamActionFence::controlled_for_test(fixture.store(),clock,100).unwrap();
        let watch=watch(Some(source));assert!(watch.current(false,0));
        context.disconnected();
        assert!(!watch.action.load(Ordering::SeqCst));assert!(!watch.output.load(Ordering::SeqCst));
        assert!(!watch.current(false,0));assert!(watch.current(true,0));
    }
    #[test]
    fn ordinary_source_absence_preserves_existing_cancel_and_terminal_semantics() {
        let watch=watch(None);assert!(watch.current(false,0));
        watch.action.store(true,Ordering::SeqCst);
        assert!(!watch.current(false,0));assert!(watch.current(true,0));
        watch.action.store(false,Ordering::SeqCst);watch.output.store(true,Ordering::SeqCst);
        assert!(!watch.current(false,0));assert!(watch.current(true,0));
    }
    #[test]
    fn ordinary_actions_have_no_synthetic_window_sample_deadline() {
        let watch=watch(None);
        for now in [0,1500,1501,crate::life_domain::MAX_COUNTER] {
            assert!(watch.current(false,now));
        }
        watch.action.store(true,Ordering::SeqCst);
        assert!(!watch.current(false,1501));
        assert!(watch.current(true,1501),"terminal cleanup keeps its existing exception");
    }

    #[test]
    fn one_worker_retains_its_charge_until_main_really_drains_and_capacity_includes_it() {
        let mut queue=State::<u8>::default();assert_eq!(queue.push(0),Ok(true));
        assert_eq!(queue.pop(),Ok(Some(0)));assert!(queue.running&&queue.in_flight);
        for n in 1..LIMIT {assert_eq!(queue.push(n as u8),Ok(false));}
        assert_eq!(queue.push(99),Err(()));assert!(queue.pop().is_err());
        queue.drained();assert_eq!(queue.pop(),Ok(Some(1)));assert!(queue.running&&queue.in_flight);
    }
    #[test]
    fn fifo_worker_exits_only_after_every_original_charge_drains() {
        let mut queue=State::<u8>::default();assert_eq!(queue.push(1),Ok(true));assert_eq!(queue.push(2),Ok(false));
        assert_eq!(queue.pop(),Ok(Some(1)));queue.drained();assert_eq!(queue.pop(),Ok(Some(2)));queue.drained();
        assert_eq!(queue.pop(),Ok(None));assert!(!queue.running&&!queue.in_flight);
        assert_eq!(queue.push(3),Ok(true));
    }
}
