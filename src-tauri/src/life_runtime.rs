//! One application-owned native autonomy loop. Restart never restores activation.
#[path = "life_runtime/core.rs"]
pub(crate) mod core;
use self::core::{Claim, Core, Mode, RuntimeSnapshot, SpeechConfig};
use crate::{
    life_availability::{
        Availability, NativeMonitor, Reason as SystemReason, Snapshot as SystemSnapshot, Source,
    },
    life_domain::{ActionPhase, CancelReason, LifeEvent, Needs, RandomState, WorldId},
    storage::{
        life_store::{LifeAvailability, LifeSnapshot, WriteOutcome},
        Store,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
const UNAVAILABLE: &str = "Native Autonomie ist nicht verfügbar. Lokales Life und Stop bleiben verfügbar.";
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(crate::life_domain::MAX_COUNTER as u128) as u64
}
fn speech_config(config: &crate::autopilot::Config) -> SpeechConfig {
    SpeechConfig {
        speak: config.speak,
        min_ms: config.min_seconds * 1000,
        max_ms: config.max_seconds * 1000,
    }
}
fn has_model(profile:&Value)->bool{crate::providers::dialog_model_ready(&profile["model"])}
fn output_available(app: &tauri::AppHandle, profile: &Value) -> bool {
    match profile["output"]["mode"].as_str().unwrap_or("studio") {
        "studio" => app.get_webview_window("main").is_some(),
        "overlay" => crate::overlay::is_open(app, profile["id"].as_str().unwrap_or_default()),
        "obs" => profile["output"]["streamingEnabled"].as_bool() == Some(true),
        _ => false,
    }
}
fn visible(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}
fn main_only(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err("Life-Sitzungen können nur im Studio gesteuert werden.".into())
    }
}

/// Main-window IPC that can open a dialog, edit state or start foreground I/O
/// revokes the clean hidden barrier before its handler executes. Runtime status,
/// playback acknowledgements and the runtime-owned audio path never revoke
/// their own claim. These exemptions grant no session or OS authority.
pub fn operator_command(command:&str)->bool{!matches!(command,
 "get_app_state"|"get_life_runtime"|"get_life_bot"|"connect_life_ui"|"disconnect_life_ui"|"report_life_ui"|"set_life_mode"|"set_autopilot"|
 "send_automatic_message"|"get_speech_audio"|"report_playback"|"cancel_speech_job"|"get_emote_sound"|"report_emote_playback"|
 "get_emote_capabilities"|"get_extension_library"|"get_library_skill"|"get_control_mcp_config"|"list_bots"|"get_avatar_info"|
 "get_ui_locale"|"get_credential_session"|"get_update_settings"|"get_usage_budget"|"get_screen_session"|"get_screen_capture_permission"|"get_microphone_permission"|
 "list_output_displays"|"list_capture_displays"|"save_profile"|"save_document")}
pub fn operator_invoke<R:tauri::Runtime>(invoke:&tauri::ipc::Invoke<R>){let webview=invoke.message.webview_ref();if webview.label()=="main"&&operator_command(invoke.message.command()){if let Some(state)=webview.try_state::<crate::AppState>(){state.life_runtime.operator_activity();}}}

pub struct LifeRuntime {
    core: Mutex<Core>,
    monitor: OnceLock<NativeMonitor>,
    started: Instant,
    running: AtomicBool,
    stopping: AtomicBool,
}
impl Default for LifeRuntime {
    fn default() -> Self {
        Self {
            core: Mutex::new(Core::new(SystemSnapshot {
                state: Availability::Unknown,
                reason: SystemReason::Initializing,
                generation: 1,
                source: Source::Unsupported,
            })),
            monitor: OnceLock::new(),
            started: Instant::now(),
            running: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
        }
    }
}
impl LifeRuntime {
    pub fn now(&self) -> u64 {
        self.started
            .elapsed()
            .as_millis()
            .min(crate::life_domain::MAX_COUNTER as u128) as u64
    }
    pub fn snapshot(&self) -> Result<RuntimeSnapshot, String> {
        Ok(self
            .core
            .lock()
            .map_err(|_| UNAVAILABLE)?
            .snapshot(self.now()))
    }
    pub fn snapshot_for(&self,hub:&crate::autopilot::AutopilotHub)->Result<RuntimeSnapshot,String>{let core=self.core.lock().map_err(|_|UNAVAILABLE)?;let mut snapshot=core.snapshot(self.now());for session in &mut snapshot.sessions{let budget=hub.speech_budget(&session.bot_id)?;session.speech.attempts=budget.attempts;session.speech.limit=budget.limit;session.speech.budget_limited=budget.budget_limited;session.speech.remaining_seconds=session.speech.remaining_seconds.max(budget.remaining_seconds);if budget.budget_limited{session.speech.eligible=false;}}Ok(snapshot)}
    pub fn any_active(&self) -> bool {
        self.core
            .lock()
            .map(|core| core.any_active())
            .unwrap_or(true)
    }
    pub fn activity(&self) {
        if let Ok(mut core) = self.core.lock() {
            core.activity(self.now());
        }
    }
    pub fn operator_activity(&self) {
        if let Ok(mut core) = self.core.lock() {
            core.operator_activity(self.now());
        }
    }
    pub fn present(&self, mut runtime: crate::autopilot::Runtime) -> crate::autopilot::Runtime {
        let Ok(core) = self.core.lock() else {
            runtime.active = false;
            runtime.wander = false;
            runtime.gestures = false;
            return runtime;
        };
        match core.sessions.get(&runtime.bot_id) {
            Some(session) if session.mode == Mode::Life => {
                runtime.wander = false;
                runtime.gestures = false;
            }
            Some(_) => (),
            None => {
                runtime.active = false;
                runtime.wander = false;
                runtime.gestures = false;
            }
        }
        runtime
    }
    pub fn invalidate_bot(&self,bot:&str){if let Ok(mut core)=self.core.lock(){core.invalidate_bot(bot,self.now());core.operator_activity(self.now());}}
    pub fn current(&self, claim: &Claim) -> bool {
        self.core
            .lock()
            .map(|core| core.current(claim, self.now()))
            .unwrap_or(false)
    }
    pub fn bind(&self, claim: &Claim, cancel: Arc<AtomicBool>) -> Result<(), String> {
        let monitor = self.monitor.get().ok_or(UNAVAILABLE)?;
        if monitor.gate.watch(&cancel).is_none() {
            return Err(UNAVAILABLE.into());
        }
        if !self
            .core
            .lock()
            .map_err(|_| UNAVAILABLE)?
            .bind(claim, cancel, self.now())
        {
            return Err(UNAVAILABLE.into());
        }
        Ok(())
    }
    pub async fn check_cloud(&self, app: &tauri::AppHandle, claim: &Claim) -> Result<(), String> {
        let system = self.monitor.get().ok_or(UNAVAILABLE)?.refresh().await;
        let is_visible=visible(app);
        let mut core = self.core.lock().map_err(|_| UNAVAILABLE)?;
        let now = self.now();
        core.observe_system(system, now);
        core.reconcile_visibility(is_visible, now);
        if !core.current(claim, now) {
            return Err(UNAVAILABLE.into());
        }
        app.state::<crate::AppState>().updater.ensure_idle()?;
        Ok(())
    }
    /// Queue follow-up work must still belong to this live session, even after the
    /// dialog claim itself completed. External OS state is sampled again.
    pub async fn check_session_cloud(
        &self,
        app: &tauri::AppHandle,
        claim: &Claim,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let system = self.monitor.get().ok_or(UNAVAILABLE)?.refresh().await;
        let is_visible=visible(app);
        let mut core = self.core.lock().map_err(|_| UNAVAILABLE)?;
        let now = self.now();
        core.observe_system(system, now);
        core.reconcile_visibility(is_visible, now);
        if cancel.load(Ordering::SeqCst)
            || system.state != Availability::AwakeUnlocked
            || !core.session_current(claim)
        {
            return Err(UNAVAILABLE.into());
        }
        app.state::<crate::AppState>().updater.ensure_idle()?;
        Ok(())
    }
    pub fn commit<T>(
        &self,
        claim: &Claim,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let core = self.core.lock().map_err(|_| UNAVAILABLE)?;
        if !core.current(claim, self.now()) {
            return Err(UNAVAILABLE.into());
        }
        operation()
    }
    pub fn change_profile(
        &self,
        app: &tauri::AppHandle,
        bot: &str,
        write: impl FnOnce() -> Result<Value, String>,
    ) -> Result<(Value, Option<crate::autopilot::Runtime>), String> {
        let mut core = self.core.lock().map_err(|_| UNAVAILABLE)?;
        core.invalidate_bot(bot,self.now());
        core.operator_activity(self.now());
        let result = app
            .state::<crate::AppState>()
            .autopilot
            .change_profile(bot, write);
        if let Ok((profile, _)) = &result {
            let config = crate::autopilot::config(profile)?;
            core.reconfigure(
                bot,
                speech_config(&config),
                has_model(profile),
                output_available(app, profile),
                self.now(),
            )
            .map_err(str::to_owned)?;
        }
        result
    }
    pub fn stop(&self, app: &tauri::AppHandle, bot: Option<&str>) {
        let state = app.state::<crate::AppState>();
        // Runtime -> AutopilotHub -> Store is the admitted mutation/commit order.
        // Keep Stop and a concurrent new Start indivisible; emit only after unlock.
        let mut core = self
            .core
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let removed = if let Some(bot) = bot {
            core.stop(bot, self.now())
                .map(|session| vec![(bot.to_owned(), session)])
                .unwrap_or_default()
        } else {
            core.stop_all(self.now())
        };
        let mut publications = Vec::new();
        for (bot, session) in removed {
            let config = state
                .store
                .bot_profile(&bot)
                .ok()
                .and_then(|profile| crate::autopilot::config(&profile).ok())
                .unwrap_or_default();
            if let Ok(runtime) = state.autopilot.set(&bot, false, config) {
                publications.push(runtime);
            }
            cancel_saved_action(&state.store, &bot, session.state.as_ref());
        }
        if let Some(bot) = bot {
            let _ = state.autopilot.invalidate(bot);
        } else {
            publications.extend(state.autopilot.stop_all());
        }
        drop(core);
        for runtime in publications {
            crate::autopilot::publish(app, runtime);
        }
    }

    pub fn hide_main(&self, window: &tauri::WebviewWindow) {
        let barrier = self
            .core
            .lock()
            .ok()
            .and_then(|core| core.prepare_hide(self.now()));
        let hidden = window.hide().is_ok() && window.is_visible().ok() == Some(false);
        if let Ok(mut core) = self.core.lock() {
            core.hidden(barrier, hidden, self.now());
        }
    }
    pub fn shown(&self) {
        if let Ok(mut core) = self.core.lock() {
            core.shown(self.now());
        }
    }
    pub fn shutdown(&self, app: &tauri::AppHandle) {
        self.stopping.store(true, Ordering::SeqCst);
        self.stop(app, None);
        if let Ok(mut core) = self.core.lock() {
            core.shutdown(self.now());
        }
        if let Some(monitor) = self.monitor.get() {
            monitor.stop();
        }
    }
    pub fn install(&self, app: &tauri::AppHandle) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.monitor.set(NativeMonitor::start(app.clone()));
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(1));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let state = handle.state::<crate::AppState>();
                if state.life_runtime.stopping.load(Ordering::SeqCst) {
                    break;
                }
                if !state.life_runtime.any_active() {
                    continue;
                }
                state.life_runtime.step(&handle).await;
            }
        });
    }
    async fn step(&self, app: &tauri::AppHandle) {
        let Some(monitor) = self.monitor.get() else {
            return;
        };
        let system = monitor.refresh().await;
        let state = app.state::<crate::AppState>();
        let now = self.now();
        let is_visible=visible(app);
        if let Ok(mut core) = self.core.lock() {
            core.observe_system(system, now);
            core.reconcile_visibility(is_visible, now);
        }
        // Native owner alone advances persisted simulation. Deferred writes never
        // move the checkpoint; storage errors pause only their own session.
        if let Ok(mut core) = self.core.lock() {
            let jobs = core.checkpoint_jobs(now);
            for (bot, id, revision, expected, elapsed) in jobs {
                if core
                    .sessions
                    .get(&bot)
                    .is_none_or(|session| session.session_id != id || session.revision != revision)
                {
                    continue;
                }
                match state.store.apply_life_event(
                    &bot,
                    expected,
                    elapsed as i64,
                    LifeEvent::Tick {},
                    wall_ms(),
                ) {
                    Ok(commit) => {
                        let checkpoint = if commit.outcome == WriteOutcome::Applied {
                            Some(now)
                        } else {
                            None
                        };
                        core.replace_confirmed(
                            &bot,
                            &id,
                            revision,
                            commit.snapshot.state().clone(),
                            checkpoint,
                        );
                    }
                    Err(crate::storage::life_store::LifeStoreError::RevisionConflict) => {
                        core.mark_storage_failure(&bot, now);
                        if let Ok(Some(snapshot)) = state.store.load_life(&bot) {
                            if let Some(session) = core.sessions.get(&bot) {
                                let revision = session.revision;
                                core.replace_confirmed(
                                    &bot,
                                    &id,
                                    revision,
                                    snapshot.state().clone(),
                                    Some(now),
                                );
                            }
                        }
                    }
                    Err(_) => core.mark_storage_failure(&bot, now),
                }
            }
        }
        let pending = self.core.lock().ok().and_then(|core| core.pending());
        if let Some((claim, generating)) = pending {
            if !self.current(&claim) {
                let _ = crate::overlay::cancel_playback(
                    app,
                    &claim.turn_id,
                    &claim.bot_id,
                    Some(UNAVAILABLE),
                );
                if let Ok(mut core) = self.core.lock() {
                    core.finish(&claim, self.now());
                }
                turn_event(app, "finished", &claim, None);
            } else if !generating && !state.overlay.has_pending_speech(&claim.bot_id) {
                if let Ok(mut core) = self.core.lock() {
                    core.finish(&claim, self.now());
                }
                turn_event(app, "finished", &claim, None);
            }
        }
        if let Ok(mut core)=self.core.lock(){let bots=core.sessions.keys().cloned().collect::<Vec<_>>();for bot in bots{if let Ok(budget)=state.autopilot.speech_budget(&bot){core.defer_speech(&bot,budget.remaining_seconds,self.now());}}}
        let has_pending = self
            .core
            .lock()
            .map(|core| core.pending().is_some())
            .unwrap_or(true);
        if !has_pending {
            let blocked = state.updater.ensure_idle().is_err()
                || state.overlay.has_any_pending_speech()
                || state.generation.try_lock().is_err();
            let claim = self.core.lock().ok().and_then(|mut core| {
                core.claim(self.now(), blocked, uuid::Uuid::new_v4().to_string())
            });
            if let Some(claim) = claim {
                if monitor.gate.watch(&claim.cancel).is_none() {
                    if let Ok(mut core) = self.core.lock() {
                        core.finish(&claim, self.now());
                    }
                } else {
                    turn_event(app, "started", &claim, None);
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let result =
                            crate::agent::send_runtime_autopilot(app.clone(), claim.clone()).await;
                        let state = app.state::<crate::AppState>();
                        if state.life_runtime.current(&claim) {
                            if let Ok(reply) = result {
                                turn_event(&app, "reply", &claim, Some(reply));
                            }
                            if let Ok(mut core) = state.life_runtime.core.lock() {
                                core.generated(&claim, state.life_runtime.now());
                            }
                        }
                    });
                }
            }
        }
        publish(app);
    }
}
fn cancel_saved_action(store:&Store,bot:&str,state:Option<&crate::life_domain::LifeState>){let _=cancel_saved_action_checked(store,bot,state);}
fn cancel_saved_action_checked(store:&Store,bot:&str,state:Option<&crate::life_domain::LifeState>)->Result<(),String>{let Some(state)=state else{return Ok(());};if let Some(action)=state.current_action().filter(|action|!matches!(action.phase(),ActionPhase::Completed|ActionPhase::Cancelled)){store.apply_life_event(bot,state.revision(),0,LifeEvent::Cancel{action_id:action.intent().id().clone(),reason:CancelReason::Operator},wall_ms()).map_err(|_|UNAVAILABLE)?;}Ok(())}
fn cancel_orphaned_action(store:&Store,bot:&str,snapshot:LifeSnapshot)->Result<LifeSnapshot,String>{if let Some(action)=snapshot.state().current_action().filter(|action|!matches!(action.phase(),ActionPhase::Completed|ActionPhase::Cancelled)){return store.apply_life_event(bot,snapshot.state().revision(),0,LifeEvent::Cancel{action_id:action.intent().id().clone(),reason:CancelReason::Interrupted},wall_ms()).map(|commit|commit.snapshot).map_err(|_|UNAVAILABLE.into());}Ok(snapshot)}

fn turn_event(app: &tauri::AppHandle, kind: &str, claim: &Claim, reply: Option<Value>) {
    let _=app.emit_to("main","cohost://life-turn",json!({"kind":kind,"botId":claim.bot_id,"sessionId":claim.session_id,"revision":claim.revision,"turnId":claim.turn_id,"reply":reply}));
}
pub fn publish(app: &tauri::AppHandle) {
    let state = app.state::<crate::AppState>();
    if let Ok(snapshot) = state.life_runtime.snapshot_for(&state.autopilot) {
        let _ = app.emit_to("main", "cohost://life-runtime", &snapshot);
        for session in snapshot.sessions {
            state.overlay.publish(app,json!({"kind":"life-runtime","botId":session.bot_id,"session":session,"system":snapshot.system,"version":1,"sequence":snapshot.sequence}));
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConnection {
    connection_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiReport {
    connection_id: String,
    sequence: u64,
    blocked: bool,
    activity: bool,
}
#[tauri::command]
pub fn get_life_runtime(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<RuntimeSnapshot, String> {
    main_only(window.label())?;
    state.life_runtime.snapshot_for(&state.autopilot)
}
#[tauri::command]
pub fn connect_life_ui(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<UiConnection, String> {
    main_only(window.label())?;
    let id = uuid::Uuid::new_v4().to_string();
    state
        .life_runtime
        .core
        .lock()
        .map_err(|_| UNAVAILABLE)?
        .connect_ui(id.clone(), state.life_runtime.now());
    Ok(UiConnection { connection_id: id })
}
#[tauri::command]
pub fn disconnect_life_ui(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
    connection_id: String,
) -> Result<(), String> {
    main_only(window.label())?;
    state
        .life_runtime
        .core
        .lock()
        .map_err(|_| UNAVAILABLE)?
        .disconnect_ui(&connection_id, state.life_runtime.now());
    Ok(())
}
#[tauri::command]
pub fn report_life_ui(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
    report: UiReport,
) -> Result<bool, String> {
    main_only(window.label())?;
    let is_visible=window.is_visible().unwrap_or(false);
    Ok(state
        .life_runtime
        .core
        .lock()
        .map_err(|_| UNAVAILABLE)?
        .report_ui(
            &report.connection_id,
            report.sequence,
            report.blocked,
            report.activity,
            is_visible,
            state.life_runtime.now(),
        ))
}
#[tauri::command]
pub fn get_life_bot(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
    bot_id: String,
) -> Result<Value, String> {
    main_only(window.label())?;
    Ok(
        match state
            .store
            .life_availability(&bot_id)
            .map_err(|_| UNAVAILABLE)?
        {
            LifeAvailability::Ready => {
                json!({"availability":"ready","actor":state.store.load_life(&bot_id).map_err(|_|UNAVAILABLE)?.map(|snapshot|snapshot.state().public_projection())})
            }
            LifeAvailability::NotInitialized => json!({"availability":"not_initialized"}),
            LifeAvailability::Unavailable(_) => json!({"availability":"unavailable"}),
        },
    )
}
#[tauri::command]
pub async fn set_life_mode(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    bot_id: String,
    mode: Mode,
    active: bool,
    initialize: Option<bool>,
) -> Result<RuntimeSnapshot, String> {
    main_only(window.label())?;
    set_mode(&app, &bot_id, mode, active, initialize == Some(true)).await
}
struct StartIntentGuard<'a> { runtime:&'a LifeRuntime, bot:String, intent:String }
impl Drop for StartIntentGuard<'_> {
    fn drop(&mut self) {
        // A cancelled command future must not leave a phantom active start.
        // finish_start holds no Core guard across an await; its local guards
        // are destroyed before this outer guard on cancellation/error.
        let mut core=self.runtime.core.lock().unwrap_or_else(|error|error.into_inner());
        core.cancel_start(&self.bot,&self.intent);
    }
}
pub async fn set_mode(
    app: &tauri::AppHandle,
    bot: &str,
    mode: Mode,
    active: bool,
    initialize: bool,
) -> Result<RuntimeSnapshot, String> {
    let state = app.state::<crate::AppState>();
    if !active {
        state.overlay.publish(app,json!({"kind":"stop","botId":bot}));
        publish(app);
        return state.life_runtime.snapshot_for(&state.autopilot);
    }
    state.life_runtime.operator_activity();
    state.updater.ensure_idle()?;
    let _idle = state
        .generation
        .try_lock()
        .map_err(|_| "Bitte die laufende Antwort oder Aktualisierung vor dem Start von Life beenden.")?;
    if state.overlay.has_any_pending_speech() {
        return Err("Bitte vorbereitete oder laufende Ausgaben vor dem Start von Life beenden.".into());
    }
    let profile = state.store.bot_profile(bot)?;
    let config = crate::autopilot::config(&profile)?;
    if mode == Mode::Ambient && config.speak && !has_model(&profile) {
        return Err("Bitte vor dem Start von Ambient mit Sprache ein Dialogmodell auswählen.".into());
    }
    let intent = uuid::Uuid::new_v4().to_string();
    state
        .life_runtime
        .core
        .lock()
        .map_err(|_| UNAVAILABLE)?
        .begin_start(bot, intent.clone())
        .map_err(str::to_owned)?;
    let _intent_guard=StartIntentGuard{runtime:&state.life_runtime,bot:bot.to_owned(),intent:intent.clone()};
    finish_start(app,bot,mode,initialize,&intent,&profile).await
}
async fn finish_start(app:&tauri::AppHandle,bot:&str,mode:Mode,initialize:bool,intent:&str,expected_profile:&Value)->Result<RuntimeSnapshot,String>{
    let state=app.state::<crate::AppState>();
    if let Some(monitor) = state.life_runtime.monitor.get() {
        let system = monitor.refresh().await;
        state
            .life_runtime
            .core
            .lock()
            .map_err(|_| UNAVAILABLE)?
            .observe_system(system, state.life_runtime.now());
    }
    let mut core = state.life_runtime.core.lock().map_err(|_| UNAVAILABLE)?;
    if !core.start_current(bot, intent) {
        return Err("Der Start von Life wurde abgebrochen.".into());
    }
    let profile=state.store.bot_profile(bot)?;
    if &profile!=expected_profile{return Err("Der Start von Life wurde abgebrochen.".into());}
    let config=crate::autopilot::config(&profile)?;
    // Repeated identical starts do not replace a running local activity or
    // reset session identity/rate windows. A deliberate mode switch first
    // cancels the previous confirmed action without granting completion credit.
    if core.sessions.get(bot).is_some_and(|session|session.mode==mode&&session.config==speech_config(&config)&&!session.storage_failed){drop(core);return state.life_runtime.snapshot_for(&state.autopilot);}
    if let Some(previous)=core.take_session(bot,state.life_runtime.now()){
        state.autopilot.set(bot,false,config.clone())?;
        cancel_saved_action_checked(&state.store,bot,previous.state.as_ref())?;
    }
    let confirmed: Option<LifeSnapshot> = if mode == Mode::Life {
        match state.store.load_life(bot).map_err(|_| UNAVAILABLE)? {
            Some(snapshot) => Some(cancel_orphaned_action(&state.store,bot,snapshot)?),
            None if initialize => Some(
                state
                    .store
                    .initialize_life(
                        bot,
                        WorldId::new(format!("world_{}", uuid::Uuid::new_v4().simple()))
                            .map_err(|_| UNAVAILABLE)?,
                        Needs::new(5000, 5000, 5000, 5000).map_err(|_| UNAVAILABLE)?,
                        RandomState::seeded(wall_ms()),
                        wall_ms(),
                    )
                    .map_err(|_| UNAVAILABLE)?,
            ),
            None => {
                core.stop(bot, state.life_runtime.now());
                return Err("Der Life-Zustand muss im Studio ausdrücklich initialisiert werden.".into());
            }
        }
    } else {
        None
    };
    let session = uuid::Uuid::new_v4();
    let seed = u64::from_le_bytes(
        session.as_bytes()[..8]
            .try_into()
            .map_err(|_| UNAVAILABLE)?,
    );
    core.start(
        bot,
        mode,
        session.to_string(),
        speech_config(&config),
        confirmed.map(|snapshot| snapshot.state().clone()),
        seed,
        has_model(&profile),
        output_available(app, &profile),
        state.life_runtime.now(),
    )
    .map_err(str::to_owned)?;
    if let Err(error)=state.autopilot.set(bot,true,config){core.stop(bot,state.life_runtime.now());return Err(error);}
    let runtime=match state.autopilot.refresh_runtime(bot){Ok(runtime)=>runtime,Err(error)=>{core.stop(bot,state.life_runtime.now());return Err(error);}};
    drop(core);
    crate::autopilot::publish(app, runtime);
    publish(app);
    state.life_runtime.snapshot_for(&state.autopilot)
}
