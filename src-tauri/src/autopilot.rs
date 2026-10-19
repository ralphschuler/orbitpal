//! Explicit, session-only autonomy. Neither saved profiles nor model/MCP calls
//! can activate a session. Native admission limits apply even to failed turns.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use tauri::{Emitter, Manager};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub min_seconds: u64,
    pub max_seconds: u64,
    pub max_turns_per_hour: usize,
    pub speak: bool,
    pub gestures: bool,
    pub wander: bool,
    pub instructions: String,
}
impl Default for Config {
    fn default() -> Self { Self { min_seconds:60, max_seconds:180, max_turns_per_hour:12, speak:true, gestures:true, wander:true, instructions:String::new() } }
}
pub fn config(profile:&Value)->Result<Config,String> {
    let mut value=profile.get("autopilot").filter(|value|!value.is_null()).cloned().unwrap_or_else(||json!({}));
    if let Some(fields)=value.as_object_mut(){for key in ["screenContext","screenServerId","screenToolName","screenToolArguments"]{fields.remove(key);}}
    let config:Config=serde_json::from_value(value).map_err(|error|format!("Ungültige Autopilot-Einstellungen: {error}"))?;
    if !(15..=3600).contains(&config.min_seconds)||config.max_seconds<config.min_seconds||config.max_seconds>3600{return Err("Autopilot-Abstände müssen zwischen 15 und 3600 Sekunden liegen; das Maximum darf nicht kleiner als das Minimum sein".into());}
    if !(1..=120).contains(&config.max_turns_per_hour){return Err("Autopilot erlaubt 1 bis 120 Sprechversuche pro Stunde".into());}
    if config.instructions.len()>4000{return Err("Autopilot-Anweisungen dürfen höchstens 4000 Bytes umfassen".into());}
    Ok(config)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Runtime { pub bot_id:String, pub active:bool, pub wander:bool, pub gestures:bool, pub started_at:u64, pub seed:String, pub revision:u64 }
fn advance(runtime:&mut Runtime){
    // JavaScript consumers must compare an exact, safe integer. This counter
    // belongs only to the running process, never to an imported profile.
    runtime.revision=runtime.revision.saturating_add(1).min(9_007_199_254_740_991);
}
struct Session { runtime:Runtime, config:Config, started:Instant, attempts:VecDeque<Instant>, cancel:Arc<AtomicBool> }
#[derive(Default)]
pub struct AutopilotHub { sessions:Mutex<HashMap<String,Session>> }
#[derive(Clone)]
pub struct Ticket { pub cancel:Arc<AtomicBool>, pub config:Config }

impl AutopilotHub {
    pub fn runtime(&self,bot_id:&str,config:&Config)->Runtime {
        self.sessions.lock().ok().and_then(|sessions|sessions.get(bot_id).map(|session|session.runtime.clone())).unwrap_or_else(||Runtime{bot_id:bot_id.into(),active:false,wander:config.wander,gestures:config.gestures,started_at:0,seed:String::new(),revision:0})
    }
    pub fn all(&self)->Result<Vec<Runtime>,String>{let sessions=self.sessions.lock().map_err(|_|"Autopilot-Status nicht verfügbar")?;Ok(sessions.values().map(|session|session.runtime.clone()).collect())}
    pub fn any_active(&self)->bool{self.sessions.lock().map(|sessions|sessions.values().any(|session|session.runtime.active)).unwrap_or(true)}
    fn set_at(&self,bot_id:&str,active:bool,config:Config,now:Instant)->Result<Runtime,String>{
        let mut sessions=self.sessions.lock().map_err(|_|"Autopilot-Status nicht verfügbar")?;
        let session=sessions.entry(bot_id.to_owned()).or_insert_with(||Session{runtime:Runtime{bot_id:bot_id.into(),active:false,wander:config.wander,gestures:config.gestures,started_at:0,seed:String::new(),revision:0},config:config.clone(),started:now,attempts:VecDeque::new(),cancel:Arc::new(AtomicBool::new(false))});
        // Repeated start commands cannot reset the rate window or revive a turn.
        if session.runtime.active==active&&session.config==config{return Ok(session.runtime.clone());}
        session.cancel.store(true,Ordering::SeqCst);session.cancel=Arc::new(AtomicBool::new(false));
        if active&&!session.runtime.active {
            session.started=now;
            session.runtime.started_at=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64;
            session.runtime.seed=uuid::Uuid::new_v4().to_string();
        }
        session.runtime.active=active;session.runtime.wander=config.wander;session.runtime.gestures=config.gestures;session.config=config;
        advance(&mut session.runtime);
        Ok(session.runtime.clone())
    }
    pub fn set(&self,bot_id:&str,active:bool,config:Config)->Result<Runtime,String>{self.set_at(bot_id,active,config,Instant::now())}
    pub fn invalidate(&self,bot_id:&str)->Result<(),String>{
        if let Some(session)=self.sessions.lock().map_err(|_|"Autopilot-Status nicht verfügbar")?.get_mut(bot_id){session.cancel.store(true,Ordering::SeqCst);session.cancel=Arc::new(AtomicBool::new(false));}
        Ok(())
    }
    pub fn change_profile(&self,bot_id:&str,write:impl FnOnce()->Result<Value,String>)->Result<(Value,Option<Runtime>),String>{
        let mut sessions=self.sessions.lock().map_err(|_|"Autopilot-Status nicht verfügbar")?;
        if let Some(session)=sessions.get_mut(bot_id){session.cancel.store(true,Ordering::SeqCst);session.cancel=Arc::new(AtomicBool::new(false));}
        // Admission and final output commits cannot interleave a profile/doc
        // mutation. The callback only touches native storage, never emits UI.
        let profile=write()?;let config=config(&profile)?;
        let session=sessions.entry(bot_id.to_owned()).or_insert_with(||Session{runtime:Runtime{bot_id:bot_id.into(),active:false,wander:config.wander,gestures:config.gestures,started_at:0,seed:String::new(),revision:0},config:config.clone(),started:Instant::now(),attempts:VecDeque::new(),cancel:Arc::new(AtomicBool::new(false))});
        session.config=config.clone();session.runtime.wander=config.wander;session.runtime.gestures=config.gestures;advance(&mut session.runtime);
        let runtime=Some(session.runtime.clone());
        Ok((profile,runtime))
    }
    pub fn stop_all(&self)->Vec<Runtime>{
        let Ok(mut sessions)=self.sessions.lock()else{return Vec::new();};
        sessions.values_mut().filter_map(|session|{session.cancel.store(true,Ordering::SeqCst);if !session.runtime.active{return None;}session.runtime.active=false;advance(&mut session.runtime);Some(session.runtime.clone())}).collect()
    }
    fn admit_at(&self,bot_id:&str,current:&Config,now:Instant)->Result<Ticket,String>{
        let mut sessions=self.sessions.lock().map_err(|_|"Autopilot-Status nicht verfügbar")?;
        let session=sessions.get_mut(bot_id).filter(|session|session.runtime.active).ok_or("Autopilot ist für diesen Bot nicht gestartet")?;
        if &session.config!=current{return Err("Autopilot-Einstellungen wurden geändert; bitte den Sitzungsstatus aktualisieren".into());}
        if !current.speak{return Err("Selbstständige Sprachausgaben sind für diesen Autopilot ausgeschaltet".into());}
        session.attempts.retain(|attempt|now.saturating_duration_since(*attempt)<Duration::from_secs(3600));
        if session.attempts.len()>=current.max_turns_per_hour{return Err("Das stündliche Autopilot-Sprechlimit ist erreicht".into());}
        let previous=session.attempts.back().copied().unwrap_or(session.started).max(session.started);
        if now.saturating_duration_since(previous)<Duration::from_secs(current.min_seconds){return Err("Der minimale Autopilot-Sprechabstand ist noch nicht erreicht".into());}
        session.attempts.push_back(now);
        // Playback failures may cancel an individual speech entry. That must
        // never poison the active session's next attempt or revive the old one.
        session.cancel.store(true,Ordering::SeqCst);session.cancel=Arc::new(AtomicBool::new(false));
        Ok(Ticket{cancel:session.cancel.clone(),config:current.clone()})
    }
    pub fn admit(&self,bot_id:&str,current:&Config)->Result<Ticket,String>{self.admit_at(bot_id,current,Instant::now())}
    // Commit under the same lock as stop/refresh, so a completed model response
    // cannot publish speech after the session was already stopped/reconfigured.
    pub fn commit<T>(&self,bot_id:&str,ticket:&Ticket,f:impl FnOnce()->Result<T,String>)->Result<T,String>{
        let sessions=self.sessions.lock().map_err(|_|"Autopilot-Status nicht verfügbar")?;
        let session=sessions.get(bot_id).ok_or("Autopilot-Sitzung wurde beendet")?;
        if !session.runtime.active||!Arc::ptr_eq(&session.cancel,&ticket.cancel)||ticket.cancel.load(Ordering::SeqCst){return Err("Autopilot-Antwort wurde abgebrochen".into());}
        f()
    }
}
pub fn publish(app:&tauri::AppHandle,runtime:Runtime){
    let _=app.emit("cohost://autopilot",&runtime);
    let mut value=serde_json::to_value(runtime).unwrap_or_else(|_|json!({}));value["kind"]=json!("autopilot");app.state::<crate::AppState>().overlay.publish(app,value);
}
fn main_only(label:&str)->Result<(),String>{if label=="main"{Ok(())}else{Err("Autopilot wird ausschließlich vom Streamer im Studio gesteuert".into())}}
#[tauri::command]
pub fn set_autopilot(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String,active:bool)->Result<Runtime,String>{
    main_only(window.label())?;let state=app.state::<crate::AppState>();
    let profile=state.store.bot_profile(&bot_id)?;let config=config(&profile)?;
    if active&&config.speak&&profile["model"]["model"].as_str().unwrap_or_default().trim().is_empty(){return Err("Bitte zuerst ein Sprachmodell für diesen Bot auswählen".into());}
    let _idle=if active{state.updater.ensure_idle()?;Some(state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort oder das Update vor dem Autopilot-Start beenden")?)}else{None};
    let runtime=state.autopilot.set(&bot_id,active,config)?;
    if !active&&state.overlay.has_autopilot_speech(&bot_id){state.overlay.publish(&app,json!({"kind":"stop","botId":bot_id}));}
    publish(&app,runtime.clone());Ok(runtime)
}
#[tauri::command]
pub async fn send_autopilot(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String)->Result<Value,String>{main_only(window.label())?;crate::agent::send_autopilot(app,bot_id).await}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn configuration_is_optional_bounded_and_never_persists_activation(){
        assert_eq!(config(&json!({})).unwrap(),Config::default());
        for bad in [json!({"active":true}),json!({"enabled":true}),json!({"minSeconds":14}),json!({"maxSeconds":59}),json!({"maxTurnsPerHour":121})]{assert!(config(&json!({"autopilot":bad})).is_err(),"{bad}");}
        assert!(config(&json!({"autopilot":{"instructions":"x".repeat(4001)}})).is_err());
        assert_eq!(config(&json!({"autopilot":{"screenContext":true,"screenToolArguments":"old legacy data"}})).unwrap(),Config::default());
    }
    #[test]fn admission_enforces_start_spacing_hour_cap_and_keeps_failed_attempts(){
        let hub=AutopilotHub::default();let start=Instant::now();let config=Config{min_seconds:15,max_seconds:30,max_turns_per_hour:2,..Config::default()};
        assert!(hub.admit_at("nova",&config,start).is_err());hub.set_at("nova",true,config.clone(),start).unwrap();
        assert!(hub.admit_at("nova",&config,start+Duration::from_secs(14)).is_err());
        hub.admit_at("nova",&config,start+Duration::from_secs(15)).unwrap(); // no completion: still charged
        assert!(hub.admit_at("nova",&config,start+Duration::from_secs(29)).is_err());
        hub.admit_at("nova",&config,start+Duration::from_secs(30)).unwrap();
        assert!(hub.admit_at("nova",&config,start+Duration::from_secs(45)).is_err());
        hub.set_at("nova",false,config.clone(),start+Duration::from_secs(50)).unwrap();hub.set_at("nova",true,config.clone(),start+Duration::from_secs(51)).unwrap();
        assert!(hub.admit_at("nova",&config,start+Duration::from_secs(66)).is_err());
        assert!(hub.admit_at("nova",&config,start+Duration::from_secs(3615)).is_ok());
    }
    #[test]fn stop_refresh_and_other_bot_sessions_cannot_revive_late_results(){
        let hub=AutopilotHub::default();let now=Instant::now();let config=Config::default();
        let started=hub.set_at("nova",true,config.clone(),now).unwrap();assert!(!started.seed.is_empty());assert!(started.started_at>0);
        let first=hub.admit_at("nova",&config,now+Duration::from_secs(60)).unwrap();
        hub.set_at("other",true,config.clone(),now).unwrap();hub.invalidate("nova").unwrap();
        assert!(first.cancel.load(Ordering::SeqCst));assert!(hub.commit("nova",&first,||Ok(())).is_err());assert!(hub.runtime("other",&config).active);
        let next=hub.admit_at("nova",&config,now+Duration::from_secs(120)).unwrap();hub.stop_all();assert!(next.cancel.load(Ordering::SeqCst));assert!(!hub.any_active());assert!(hub.commit("nova",&next,||Ok(())).is_err());
        hub.set_at("nova",true,config.clone(),now+Duration::from_secs(121)).unwrap();assert_ne!(started.seed,hub.runtime("nova",&config).seed);assert!(hub.commit("nova",&next,||Ok(())).is_err());
    }
    #[test]fn only_the_studio_can_start_autopilot(){
        assert!(main_only("main").is_ok());for label in ["avatar-nova","main-spoof",""]{assert!(main_only(label).is_err());}
    }
    #[test]fn speech_failure_does_not_poison_next_ticket_and_profile_mutation_is_atomic(){
        let hub=AutopilotHub::default();let now=Instant::now();let config=Config::default();hub.set_at("nova",true,config.clone(),now).unwrap();
        let first=hub.admit_at("nova",&config,now+Duration::from_secs(60)).unwrap();first.cancel.store(true,Ordering::SeqCst);
        let second=hub.admit_at("nova",&config,now+Duration::from_secs(120)).unwrap();assert!(!second.cancel.load(Ordering::SeqCst));assert!(hub.commit("nova",&second,||Ok(())).is_ok());assert!(hub.commit("nova",&first,||Ok(())).is_err());
        let changed=Config{wander:false,..config.clone()};
        let (_,runtime)=hub.change_profile("nova",||{assert!(hub.sessions.try_lock().is_err());Ok(json!({"id":"nova","autopilot":changed}))}).unwrap();
        assert!(second.cancel.load(Ordering::SeqCst));assert!(hub.commit("nova",&second,||Ok(())).is_err());let runtime=runtime.unwrap();assert!(runtime.active);assert!(!runtime.wander);
        let third=hub.admit_at("nova",&changed,now+Duration::from_secs(180)).unwrap();assert!(!third.cancel.load(Ordering::SeqCst));
        let failure=hub.change_profile("nova",||Err("fixture disk error".into()));assert!(failure.is_err());assert!(third.cancel.load(Ordering::SeqCst));assert!(hub.admit_at("nova",&changed,now+Duration::from_secs(240)).is_ok());
    }
    #[test]fn captured_start_and_config_events_cannot_outrank_a_later_stop_or_restart(){
        let hub=AutopilotHub::default();let config=Config::default();let now=Instant::now();
        let start=hub.set_at("nova",true,config.clone(),now).unwrap();
        let (_,changed)=hub.change_profile("nova",||Ok(json!({"id":"nova","autopilot":{"wander":false}}))).unwrap();let changed=changed.unwrap();
        let stopped=hub.stop_all().pop().unwrap();assert!(!stopped.active);assert_eq!(stopped.seed,start.seed);assert!(start.revision<changed.revision&&changed.revision<stopped.revision);
        // Stop arrives first, followed by the captured older Start and config
        // events. Neither older version can replace the stopped tombstone.
        let delivered=[stopped.clone(),start.clone(),changed];let newest=delivered.iter().max_by_key(|runtime|runtime.revision).unwrap();assert!(!newest.active);
        assert_eq!(hub.all().unwrap()[0].revision,stopped.revision);assert_eq!(serde_json::to_value(&stopped).unwrap()["revision"],stopped.revision);
        let restart=hub.set_at("nova",true,config,now+Duration::from_secs(1)).unwrap();assert!(restart.revision>stopped.revision);assert_ne!(restart.seed,start.seed);
        let stopped_again=hub.stop_all().pop().unwrap();assert!(stopped_again.revision>restart.revision);assert!(!stopped_again.active);assert!(stopped_again.revision<=9_007_199_254_740_991);
    }
}
