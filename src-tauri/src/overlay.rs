use axum::{extract::{Path,Query,State},http::{header,StatusCode},response::{sse::{Event,KeepAlive},IntoResponse,Response,Sse},routing::{get,post},Json,Router};
use futures_util::{stream,StreamExt};
use serde_json::{json,Value};
use std::{collections::{HashMap,HashSet,VecDeque},convert::Infallible,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use tauri::{Emitter,Manager};
use tokio::sync::{broadcast,Mutex as AsyncMutex};

const PLAYBACK_TIMEOUT:Duration=Duration::from_secs(180);
const AUDIO_CACHE_BYTES:usize=64*1024*1024;
const MAX_ENCODED_AUDIO:usize=4*((25*1024*1024+2)/3);

#[derive(Default)]
struct AudioCache { entries:HashMap<String,(Value,usize)>, order:VecDeque<String>, bytes:usize }
impl AudioCache {
    fn remove(&mut self,id:&str){if let Some((_,bytes))=self.entries.remove(id){self.bytes-=bytes;}self.order.retain(|key|key!=id);}
    fn get(&mut self,id:&str)->Option<Value>{
        let value=self.entries.get(id)?.0.clone();
        self.order.retain(|key|key!=id);self.order.push_back(id.into());Some(value)
    }
    fn insert(&mut self,id:&str,value:Value,protected:&HashSet<String>,budget:usize)->Result<(),String>{
        let encoded=value["audioBase64"].as_str().ok_or("Sprachausgabe enthält keine Audiodaten")?;
        if encoded.is_empty()||encoded.len()>MAX_ENCODED_AUDIO{return Err("Sprachausgabe überschreitet die Audiogrößengrenze".into());}
        let bytes=encoded.len()+value["mimeType"].as_str().unwrap_or_default().len()+id.len();
        if bytes>budget{return Err("Sprachausgabe überschreitet das Audio-Cachebudget".into());}
        let replaced=self.entries.get(id).map(|(_,bytes)|*bytes).unwrap_or(0);
        let mut needed=self.bytes-replaced+bytes;let mut evict=Vec::new();
        for candidate in &self.order{
            if needed<=budget{break;}
            if candidate!=id&&!protected.contains(candidate){needed-=self.entries[candidate].1;evict.push(candidate.clone());}
        }
        if needed>budget{return Err("Audio-Cache ist durch laufende Wiedergaben belegt. Bitte eine Wiedergabe beenden.".into());}
        for candidate in evict{self.remove(&candidate);}
        self.remove(id);self.bytes+=bytes;self.order.push_back(id.into());self.entries.insert(id.into(),(value,bytes));Ok(())
    }
}

#[derive(Clone)]
struct SpeechEntry {
    id:String, bot_id:String, text:String, speech_text:String, voice:Value, emotion:String,
    source:String, user:String, emotes:Vec<Value>, output:String,
    user_text:String, hindsight:Value, assistant_name:String, chat_route:Option<crate::chat::ChatRoute>,
    started:bool, finished:bool, cancel:Arc<AtomicBool>,
    usage_origin:crate::usage_budget::Origin,
}
impl SpeechEntry{
    fn synthesis_text(&self)->&str{if self.voice["provider"]=="elevenlabs"{&self.speech_text}else{&self.text}}
    fn speech_event(&self)->Value{json!({"kind":"speech","botId":self.bot_id,"id":self.id,"text":self.text,"speechText":self.speech_text,"voice":public_voice(&self.voice),"emotion":self.emotion,"source":self.source,"user":self.user,"emotes":self.emotes})}
    fn presentation_event(&self)->Value{json!({"kind":"started","id":self.id,"botId":self.bot_id,"text":self.text,"emotion":self.emotion,"emotes":self.emotes,"source":self.source,"user":self.user})}
    fn start(&mut self,persist:impl FnOnce(&Self)->Result<(),String>)->Result<bool,String>{
        if self.cancel.load(Ordering::Relaxed)||self.finished{return Err("Sprachausgabe wurde abgebrochen oder bereits beendet".into());}
        if self.started{return Ok(false);}
        persist(self)?;self.started=true;Ok(true)
    }
}
pub struct OverlayHub {
    pub sender:broadcast::Sender<Value>,
    pub token:String,
    pub port:std::sync::atomic::AtomicU16,
    speech:Mutex<VecDeque<SpeechEntry>>,
    audio:Mutex<AudioCache>,
    synthesis:AsyncMutex<()>,
    recent_ids:Mutex<HashMap<String,Instant>>,
}
impl Default for OverlayHub {
    fn default()->Self {let (sender,_)=broadcast::channel(256);Self {sender,token:uuid::Uuid::new_v4().to_string(),port:std::sync::atomic::AtomicU16::new(0),speech:Mutex::new(VecDeque::new()),audio:Mutex::new(AudioCache::default()),synthesis:AsyncMutex::new(()),recent_ids:Mutex::new(HashMap::new())}}
}
impl OverlayHub {
    pub fn claim_reply_id(&self,requested:Option<&str>)->Result<String,String>{self.claim_reply_id_at(requested,Instant::now())}
    fn claim_reply_id_at(&self,requested:Option<&str>,now:Instant)->Result<String,String>{
        let id=match requested{
            None=>uuid::Uuid::new_v4().to_string(),
            Some(value)=>{let uuid=uuid::Uuid::parse_str(value).map_err(|_|"Anfrage-ID muss eine kanonische UUID sein")?;if uuid.to_string()!=value{return Err("Anfrage-ID muss eine kanonische UUID sein".into());}value.to_owned()},
        };
        let entries=self.speech.lock().map_err(|_|"Sprachausgabewarteschlange nicht verfügbar")?;
        let mut recent=self.recent_ids.lock().map_err(|_|"Anfrage-IDs nicht verfügbar")?;
        recent.retain(|_,expiry|*expiry>now);
        if recent.contains_key(&id)||entries.iter().any(|entry|entry.id==id){return Err("Anfrage-ID wurde bereits verwendet".into());}
        if recent.len()>=4096{return Err("Zu viele aktuelle Anfrage-IDs. Bitte kurz warten.".into());}
        // Outlive external playback timers, including for cancelled jobs. A
        // retry uses a fresh ID rather than letting an old timer hit a new job.
        recent.insert(id.clone(),now+PLAYBACK_TIMEOUT+Duration::from_secs(60));Ok(id)
    }
    pub fn url(&self)->String {format!("http://127.0.0.1:{}/overlay?session={}",self.port.load(std::sync::atomic::Ordering::Relaxed),self.token)}
    pub fn url_for_bot(&self,bot_id:&str)->String {format!("{}&bot={bot_id}",self.url())}
    pub fn publish(&self,app:&tauri::AppHandle,mut event:Value) {
        if event["botId"].as_str().is_none() {
            if let Some(id)=event["profile"]["id"].as_str().map(str::to_owned) {event["botId"]=json!(id);}
            else if matches!(event["kind"].as_str(),Some("profile"|"speech"|"emote")) {
                if let Ok(profile)=app.state::<crate::AppState>().store.profile(){event["botId"]=profile["id"].clone();}
            }
        }
        if event["kind"]=="profile"{if let Some(profile)=event.get("profile"){event["presentation"]=app.state::<crate::AppState>().presentation.snapshot(profile);}if let Some(bot_id)=event["botId"].as_str(){let state=app.state::<crate::AppState>();let config=state.store.bot_profile(bot_id).ok().and_then(|profile|crate::autopilot::config(&profile).ok()).unwrap_or_default();event["autopilotRuntime"]=json!(state.autopilot.runtime(bot_id,&config));}}
        if event["kind"]=="stop" {
            // Tray, Studio, Control-MCP and the global overlay stop all reach
            // this boundary. No background session survives an emergency stop.
            if event["botId"].is_null(){for runtime in app.state::<crate::AppState>().autopilot.stop_all(){crate::autopilot::publish(app,runtime);}}
            else if let Some(bot_id)=event["botId"].as_str(){let _=app.state::<crate::AppState>().autopilot.invalidate(bot_id);}
            self.invalidate_speech(event["botId"].as_str());
        }
        // An unscoped stop is intentionally broadcast to every character.
        let _=self.sender.send(event.clone());let _=app.emit("cohost://overlay",event);
    }
    pub fn invalidate_speech(&self,bot_id:Option<&str>) {
        if let Ok(mut entries)=self.speech.lock(){
            let mut audio=self.audio.lock().unwrap_or_else(|error|error.into_inner());
            for entry in entries.iter().filter(|entry|bot_id.is_none_or(|id|entry.bot_id==id)){entry.cancel.store(true,Ordering::Relaxed);audio.remove(&entry.id);}
            entries.retain(|entry|bot_id.is_some_and(|id|entry.bot_id!=id));
        }
    }
    fn remove_job(&self,id:&str,bot_id:&str,cancel:bool)->Result<Option<SpeechEntry>,String>{
        self.remove_job_if(id,bot_id,cancel,None)
    }
    fn remove_job_if(&self,id:&str,bot_id:&str,cancel:bool,expected:Option<&Arc<AtomicBool>>)->Result<Option<SpeechEntry>,String>{
        let mut entries=self.speech.lock().map_err(|_|"Sprachausgabewarteschlange nicht verfügbar")?;
        let Some(index)=entries.iter().position(|entry|entry.id==id&&entry.bot_id==bot_id&&expected.is_none_or(|token|Arc::ptr_eq(token,&entry.cancel)))else{return Ok(None);};
        let mut entry=entries.remove(index).ok_or("Sprachausgabe nicht verfügbar")?;
        entry.finished=true;if cancel||!entry.started{entry.cancel.store(true,Ordering::Relaxed);}
        self.audio.lock().map_err(|_|"Audio-Cache nicht verfügbar")?.remove(id);Ok(Some(entry))
    }
    pub fn has_pending_speech(&self,bot_id:&str)->bool{
        // Fail closed if the in-memory queue is unavailable. The generation
        // lock alone no longer covers a reply once its audio is being prepared.
        self.speech.lock().map(|entries|entries.iter().any(|entry|entry.bot_id==bot_id&&!entry.finished&&!entry.cancel.load(Ordering::Relaxed))).unwrap_or(true)
    }
    pub fn has_any_pending_speech(&self)->bool{
        self.speech.lock().map(|entries|entries.iter().any(|entry|!entry.finished&&!entry.cancel.load(Ordering::Relaxed))).unwrap_or(true)
    }
    pub fn has_autopilot_speech(&self,bot_id:&str)->bool{
        self.speech.lock().map(|entries|entries.iter().any(|entry|entry.bot_id==bot_id&&entry.source=="autopilot"&&!entry.finished)).unwrap_or(true)
    }
    pub fn say_profile(&self,app:&tauri::AppHandle,profile:&Value,id:&str,text:&str,speech_text:&str,emotion:&str,source:&str,user:&str,user_text:&str,emotes:&[Value],cancel:Arc<AtomicBool>,chat_route:Option<crate::chat::ChatRoute>,usage_origin:crate::usage_budget::Origin)->Result<(),String> {
        let bot_id=profile["id"].as_str().ok_or("Bot-ID fehlt")?;
        let mut speech=self.speech.lock().map_err(|_|"Sprachausgabewarteschlange nicht verfügbar")?;
        if cancel.load(Ordering::Relaxed){return Err("Antwort abgebrochen".into());}
        if speech.len()>=64{return Err("Zu viele ausstehende Sprachausgaben. Bitte Wiedergaben beenden oder stoppen.".into());}
        let entry=SpeechEntry{id:id.into(),bot_id:bot_id.into(),text:text.into(),speech_text:speech_text.into(),voice:profile["voice"].clone(),emotion:emotion.into(),source:source.into(),user:user.into(),user_text:user_text.into(),hindsight:profile["hindsight"].clone(),assistant_name:profile["name"].as_str().unwrap_or("Cohost").into(),emotes:emotes.to_vec(),output:profile["output"]["mode"].as_str().unwrap_or("studio").into(),started:false,finished:false,cancel,chat_route,usage_origin};
        let event=entry.speech_event();let timer_cancel=entry.cancel.clone();speech.push_back(entry);
        drop(speech);
        // A disconnected Studio/OBS client cannot keep the host busy forever.
        // Only identifiers and the cancellation token survive in the timer.
        if matches!(profile["output"]["mode"].as_str(),Some("overlay"|"obs")){
            let handle=app.clone();let job_id=id.to_owned();let target=bot_id.to_owned();
            tauri::async_runtime::spawn(async move{tokio::time::sleep(PLAYBACK_TIMEOUT).await;let _=cancel_playback_if(&handle,&job_id,&target,Some(&timer_cancel),Some("Keine Abschlussmeldung der Sprachausgabe. Automatische Antworten pausiert."));});
        }
        // Route by the answered bot, independently of the selected editor.
        match profile["output"]["mode"].as_str().unwrap_or("studio"){
            "overlay"=>{let _=app.emit("cohost://overlay",event);},
            "obs"=>{let _=self.sender.send(event);},
            _=>(),
        }
        Ok(())
    }
}
pub fn public_voice(voice:&Value)->Value{
    let provider=voice["provider"].as_str().unwrap_or("system");
    json!({"provider":provider,"voiceId":if matches!(provider,"system"|"browser"){json!(voice["voiceId"].as_str().unwrap_or_default())}else{Value::Null},"language":voice["language"].as_str().unwrap_or("de-DE"),"rate":voice["rate"].as_f64().unwrap_or(1.0).clamp(0.5,2.0),"volume":voice["volume"].as_f64().unwrap_or(0.85).clamp(0.0,1.0),"pitch":voice["pitch"].as_f64().unwrap_or(1.0).clamp(0.5,2.0),"enabled":voice["enabled"].as_bool().unwrap_or(true)})
}
pub fn public_profile(profile:&Value)->Value {
    json!({"id":profile["id"],"name":profile["name"],"tagline":profile["tagline"],"avatar":profile["avatar"],"output":profile["output"],"emotes":profile["emotes"],"motions":profile["motions"],"voice":public_voice(&profile["voice"])})
}
#[derive(serde::Deserialize)]
struct Access {session:Option<String>,bot:Option<String>}
fn allowed(app:&tauri::AppHandle,q:&Access)->bool {q.session.as_deref()==Some(app.state::<crate::AppState>().overlay.token.as_str())}
fn requested_bot(app:&tauri::AppHandle,q:&Access)->Result<String,String>{
    let id=if let Some(id)=q.bot.as_ref(){id.clone()}else{app.state::<crate::AppState>().store.profile()?["id"].as_str().ok_or("Bot-ID fehlt")?.to_owned()};
    if !crate::storage::valid_id(&id){return Err("Ungültige Bot-ID".into());}
    app.state::<crate::AppState>().store.bot_profile(&id)?;Ok(id)
}
fn belongs(event:&Value,bot_id:&str)->bool{
    let target=event["botId"].as_str().or_else(||event["profile"]["id"].as_str());
    target==Some(bot_id)||(target.is_none()&&event["kind"]=="stop")
}

pub async fn start(app:tauri::AppHandle)->Result<(),String> {
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|e|e.to_string())?;
    app.state::<crate::AppState>().overlay.port.store(listener.local_addr().map_err(|e|e.to_string())?.port(),std::sync::atomic::Ordering::Relaxed);
    let control=post(crate::control_mcp::http).get(crate::control_mcp::no_stream).delete(crate::control_mcp::no_stream).layer(axum::extract::DefaultBodyLimit::max(crate::control_mcp::MAX_REQUEST_BYTES));
    let routes=Router::new().route("/mcp",control).route("/events",get(events)).route("/speech/{id}",get(speech)).route("/emote-sound/{id}",get(emote_sound)).route("/motions/{id}",get(motion)).route("/emote-playback",post(emote_playback)).route("/playback",post(playback)).route("/assets/{name}",get(asset)).route("/",get(index)).route("/overlay",get(index)).route("/{*path}",get(frontend_asset)).with_state(app);
    tauri::async_runtime::spawn(async move {let _=axum::serve(listener,crate::overlay_http::with_security(routes)).await;});Ok(())
}
async fn playback(State(app):State<tauri::AppHandle>,Query(q):Query<Access>,Json(body):Json<Value>)->Response {
    if !allowed(&app,&q){return StatusCode::UNAUTHORIZED.into_response();}
    let Ok(bot_id)=requested_bot(&app,&q)else{return StatusCode::NOT_FOUND.into_response();};
    if body["botId"].as_str().is_some_and(|id|id!=bot_id){return StatusCode::BAD_REQUEST.into_response();}
    let Some(id)=body["id"].as_str()else{return StatusCode::BAD_REQUEST.into_response();};
    let Some(speaking)=body["speaking"].as_bool()else{return StatusCode::BAD_REQUEST.into_response();};
    match report_playback(&app,id,&bot_id,speaking,body["error"].as_str()){
        Ok(result)=>Json(result).into_response(),
        Err(_)=>(StatusCode::CONFLICT,"Sprachausgabe ist nicht mehr gültig oder bereits beendet.").into_response(),
    }
}
async fn emote_playback(State(app):State<tauri::AppHandle>,Query(q):Query<Access>,Json(body):Json<Value>)->Response{
    if !allowed(&app,&q){return StatusCode::UNAUTHORIZED.into_response();}
    let Ok(bot_id)=requested_bot(&app,&q)else{return StatusCode::NOT_FOUND.into_response();};
    if body["botId"].as_str()!=Some(bot_id.as_str())||body["output"]!="obs"{return StatusCode::BAD_REQUEST.into_response();}
    let (Some(id),Some(playing))=(body["id"].as_str(),body["playing"].as_bool())else{return StatusCode::BAD_REQUEST.into_response();};
    let activity=match body.get("activity"){None=>None,Some(Value::String(value))=>Some(value.as_str()),_=>return StatusCode::BAD_REQUEST.into_response()};
    match report_emote_playback(&app,&bot_id,"obs",id,playing,activity){Ok(result)=>Json(result).into_response(),Err(_)=>(StatusCode::CONFLICT,"Emote-Audiostatus ist für diesen Bot oder Ausgabemodus nicht gültig.").into_response()}
}
fn emote_playback_event(profile:&Value,output:&str,id:&str,playing:bool,activity:Option<&str>)->Result<Value,String>{
    let activity=activity.unwrap_or("foreground");
    if !matches!(activity,"foreground"|"ambient"){return Err("Ungültige Emote-Audioaktivität".into());}
    let bot_id=profile["id"].as_str().filter(|id|crate::storage::valid_id(id)).ok_or("Bot-ID fehlt oder ist ungültig")?;
    if !matches!(output,"overlay"|"obs"){return Err("Emote-Audioausgabe muss overlay oder obs sein".into());}
    let parsed=uuid::Uuid::parse_str(id).map_err(|_|"Emote-Audiostatus benötigt eine UUID")?;
    if parsed.to_string()!=id{return Err("Emote-Audiostatus benötigt eine kanonische UUID".into());}
    // Ends are accepted after reconfiguration, so an old output can release
    // its wakeword block even when the operator has already selected a new one.
    if playing&&profile["output"]["mode"].as_str()!=Some(output){return Err("Diese Emote-Audioausgabe ist für den Bot nicht mehr ausgewählt".into());}
    Ok(json!({"botId":bot_id,"output":output,"id":id,"playing":playing,"activity":activity}))
}
pub fn report_emote_playback(app:&tauri::AppHandle,bot_id:&str,output:&str,id:&str,playing:bool,activity:Option<&str>)->Result<Value,String>{
    let profile=app.state::<crate::AppState>().store.bot_profile(bot_id)?;
    let event=emote_playback_event(&profile,output,id,playing,activity)?;
    app.emit("cohost://emote-playback",event).map_err(|_|"Emote-Audiostatus konnte nicht übermittelt werden".to_owned())?;
    Ok(json!({"accepted":true}))
}
pub fn report_playback(app:&tauri::AppHandle,id:&str,bot_id:&str,speaking:bool,error:Option<&str>)->Result<Value,String>{
    let state=app.state::<crate::AppState>();
    if !speaking{
        let entry=state.overlay.remove_job(id,bot_id,error.is_some())?.ok_or("Sprachausgabe ist nicht mehr gültig")?;
        let mut event=json!({"id":entry.id,"botId":entry.bot_id,"speaking":false,"output":entry.output});
        if let Some(error)=error.filter(|error|!error.trim().is_empty()){event["error"]=json!(error.chars().take(500).collect::<String>());}
        let _=app.emit("cohost://playback",event);return Ok(json!({"accepted":true,"started":false}));
    }
    let entry={
        let mut entries=state.overlay.speech.lock().map_err(|_|"Sprachausgabewarteschlange nicht verfügbar")?;
        let entry=entries.iter_mut().find(|entry|entry.id==id&&entry.bot_id==bot_id).ok_or("Sprachausgabe ist nicht mehr gültig")?;
        // Asset/motion admission can read files. Recheck cancellation in start
        // after that work, before committing history and publishing output.
        if !entry.started{
            if let Ok(profile)=state.store.bot_profile(bot_id){entry.emotes=crate::emote_capabilities::pending(&state.store.root,&profile,&entry.emotes);}else{entry.emotes.clear();}
        }
        if !entry.start(|entry|{
            state.store.scoped(&entry.bot_id)?.add_message_with_id(&entry.id,"assistant",&entry.text,&entry.source,&entry.assistant_name).map(|_|())
        })?{return Ok(json!({"accepted":true,"started":false}));}
        // History persistence, acknowledgement and invalidation share this
        // lock. Stop cannot publish or persist a late start, and retries commit once.
        let _=app.emit("cohost://presentation",entry.presentation_event());
        let mut event=json!({"id":entry.id,"botId":entry.bot_id,"speaking":speaking,"output":entry.output});
        if let Some(error)=error.filter(|error|!error.trim().is_empty()){event["error"]=json!(error.chars().take(500).collect::<String>());}
        let _=app.emit("cohost://playback",event);
        entry.clone()
    };
    if speaking&&entry.hindsight["enabled"].as_bool()==Some(true)&&entry.hindsight["autoRetain"].as_bool()!=Some(false){
        let handle=app.clone();let entry=entry.clone();
        tauri::async_runtime::spawn(async move{
            if entry.cancel.load(Ordering::Relaxed){return;}
            let state=handle.state::<crate::AppState>();
            let result=async{
                let store=state.store.scoped(&entry.bot_id)?;
                let usage=crate::usage_budget::Context::new(&entry.bot_id,entry.usage_origin);
                crate::memory::retain_metered(&store,&entry.hindsight,crate::memory::Conversation{id:&entry.id,bot_id:&entry.bot_id,user_text:&entry.user_text,assistant_text:&entry.text,source:&entry.source,user:&entry.user},&usage,entry.cancel.clone()).await
            }.await;
            match result{
                Ok(Some(operation))=>{let _=handle.emit("cohost://memory",json!({"kind":"retain","status":"queued","id":entry.id,"botId":entry.bot_id,"operationId":operation}));},
                Ok(None)=>(),
                Err(error)=>{let _=handle.emit("cohost://agent",json!({"kind":"tool","turnId":entry.id,"botId":entry.bot_id,"text":format!("Hindsight: Erinnerung konnte nicht zur Verarbeitung eingereiht werden: {error}")}));},
            }
        });
    }
    if speaking&&["twitch","youtube"].contains(&entry.source.as_str()){
        let handle=app.clone();
        tauri::async_runtime::spawn(async move{
            let state=handle.state::<crate::AppState>();
            if entry.cancel.load(Ordering::Relaxed){return;}
            let result=async{
                let store=state.store.scoped(&entry.bot_id)?;
                let current=store.profile()?;
                if current["chat"][&entry.source]["sendReplies"].as_bool()!=Some(true)||entry.cancel.load(Ordering::Relaxed){return Ok(());}
                let route=entry.chat_route.as_ref().ok_or("Chatantwort ohne ursprüngliche Verbindung wurde verworfen.")?;
                if entry.cancel.load(Ordering::Relaxed){return Ok(());}
                state.chat.send(route,&state.store,&entry.text,&entry.cancel).await
            }.await;
            if let Err(error)=result{let _=handle.emit("cohost://agent",json!({"kind":"tool","turnId":entry.id,"botId":entry.bot_id,"text":format!("Die gesprochene Antwort bleibt erhalten; die Chat-Nachricht konnte nicht gesendet werden: {error}")}));}
        });
    }
    Ok(json!({"accepted":true,"started":speaking}))
}
pub fn cancel_playback(app:&tauri::AppHandle,id:&str,bot_id:&str,reason:Option<&str>)->Result<Value,String>{
    cancel_playback_if(app,id,bot_id,None,reason)
}
fn cancel_playback_if(app:&tauri::AppHandle,id:&str,bot_id:&str,expected:Option<&Arc<AtomicBool>>,reason:Option<&str>)->Result<Value,String>{
    let state=app.state::<crate::AppState>();
    let Some(entry)=state.overlay.remove_job_if(id,bot_id,true,expected)?else{return Ok(json!({"cancelled":false}));};
    state.overlay.publish(app,json!({"kind":"cancel-speech","id":id,"botId":bot_id}));
    let mut event=json!({"id":id,"botId":bot_id,"speaking":false,"output":entry.output,"cancelled":true});
    if let Some(reason)=reason{event["error"]=json!(reason);}
    let _=app.emit("cohost://playback",event);Ok(json!({"cancelled":true}))
}
pub fn prepare_emote_profile(root:&std::path::Path,profile:&Value,id:&str,viewer:bool)->Result<Value,String>{
    let emotes:Vec<_>=profile["emotes"].as_array().into_iter().flatten().filter(|emote|emote["id"].as_str()==Some(id)).collect();
    let [emote]=emotes.as_slice() else{return Err("Emote wurde im Charakterprofil nicht gefunden oder ist nicht eindeutig".into());};
    if viewer&&emote["allowChat"].as_bool()!=Some(true){return Err("Dieses Emote ist für Zuschauer nicht freigegeben".into());}
    let (resolved,avatar_binding)=crate::emote_capabilities::admit(root,profile,emote)?;
    let action=&resolved.action;
    let sound=if emote["sound"].is_null(){""}else{emote["sound"].as_str().ok_or("Emote-Sound muss eine Sound-ID sein")?};
    if !crate::emote_audio::valid_sound_id(sound){return Err("Emote-Sound-ID ist ungültig".into());}
    let duration=if action.starts_with("vrma:"){resolved.duration.ok_or("Die VRMA-Bewegung ist nicht verfügbar")?}else{emote["duration"].as_f64().unwrap_or(2.5).clamp(0.2,15.0)};
    Ok(json!({"botId":profile["id"],"id":id,"action":action,"avatarBinding":avatar_binding,"emoteBinding":crate::emote_capabilities::emote_binding(emote),"nonce":uuid::Uuid::new_v4().to_string(),"duration":duration,"intensity":emote["intensity"].as_f64().unwrap_or(1.0).clamp(0.0,2.0),"sound":sound,"soundVolume":emote["soundVolume"].as_f64().unwrap_or(0.7).clamp(0.0,1.0)}))
}
pub fn trigger_emote_profile(app:&tauri::AppHandle,profile:&Value,id:&str,viewer:bool)->Result<Value,String>{
    let payload=prepare_emote_profile(&app.state::<crate::AppState>().store.root,profile,id,viewer)?;
    let _=app.emit("cohost://emote",payload.clone());let mut event=payload.clone();event["kind"]=json!("emote");app.state::<crate::AppState>().overlay.publish(app,event);Ok(payload)
}
async fn events(State(app):State<tauri::AppHandle>,Query(q):Query<Access>)->Response {
    if !allowed(&app,&q){return StatusCode::UNAUTHORIZED.into_response();}
    let Ok(bot_id)=requested_bot(&app,&q)else{return StatusCode::NOT_FOUND.into_response();};
    let state=app.state::<crate::AppState>();let receiver=state.overlay.sender.subscribe();
    let initial=match state.store.scoped(&bot_id).and_then(|store|store.profile()){
        Ok(profile)=>json!({"kind":"profile","botId":bot_id,"profile":public_profile(&profile),"presentation":state.presentation.snapshot(&profile),"autopilotRuntime":state.autopilot.runtime(&bot_id,&crate::autopilot::config(&profile).unwrap_or_default())}),
        Err(_)=>return StatusCode::NOT_FOUND.into_response(),
    };
    let first=stream::once(async move{Ok::<Event,Infallible>(Event::default().data(initial.to_string()))});
    let next=tokio_stream::wrappers::BroadcastStream::new(receiver).filter_map(move|result|{
        let bot_id=bot_id.clone();async move{result.ok().filter(|event|belongs(event,&bot_id)).map(|event|Ok::<Event,Infallible>(Event::default().data(event.to_string())))}
    });
    Sse::new(first.chain(next)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response()
}
async fn speech(State(app):State<tauri::AppHandle>,Path(id):Path<String>,Query(q):Query<Access>)->Response {
    if !allowed(&app,&q){return StatusCode::UNAUTHORIZED.into_response();}
    let Ok(bot_id)=requested_bot(&app,&q)else{return StatusCode::NOT_FOUND.into_response();};
    match speech_audio(&app,&id,&bot_id).await{
        Ok(audio)=>Json(audio).into_response(),
        Err(error)=>(StatusCode::BAD_GATEWAY,error).into_response(),
    }
}
async fn motion(State(app):State<tauri::AppHandle>,Path(id):Path<String>,Query(q):Query<Access>)->Response{
    if !allowed(&app,&q){return StatusCode::UNAUTHORIZED.into_response();}
    let Some(bot_id)=q.bot.clone() else{return StatusCode::BAD_REQUEST.into_response();};
    match tokio::task::spawn_blocking(move||{let state=app.state::<crate::AppState>();state.store.bot_profile(&bot_id).and_then(|profile|crate::motions::read(&state.store.root,&profile,&id))}).await{
        Ok(Ok(bytes))=>([(header::CONTENT_TYPE,"model/gltf-binary"),(header::ACCESS_CONTROL_ALLOW_ORIGIN,"*"),(header::CACHE_CONTROL,"no-store")],bytes).into_response(),
        _=>StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    }
}
async fn emote_sound(State(app):State<tauri::AppHandle>,Path(id):Path<String>,Query(q):Query<Access>)->Response{
    if !allowed(&app,&q){return StatusCode::UNAUTHORIZED.into_response();}
    let Ok(bot_id)=requested_bot(&app,&q)else{return StatusCode::NOT_FOUND.into_response();};
    let state=app.state::<crate::AppState>();
    let result=state.store.bot_profile(&bot_id).and_then(|profile|crate::emote_audio::read(&state.store.root,&profile,&id));
    match result{Ok(audio)=>([(header::CACHE_CONTROL,"no-store")],Json(audio)).into_response(),Err(_)=>(StatusCode::NOT_FOUND,"Sound ist für diesen Bot nicht verfügbar.").into_response()}
}
pub async fn speech_audio(app:&tauri::AppHandle,id:&str,bot_id:&str)->Result<Value,String>{
    let state=app.state::<crate::AppState>();
    let entry=state.overlay.speech.lock().map_err(|_|"Sprachausgabewarteschlange nicht verfügbar")?.iter().find(|entry|entry.id==id&&entry.bot_id==bot_id).cloned().ok_or("Sprachausgabe ist nicht mehr gültig")?;
    if entry.cancel.load(Ordering::Relaxed)||entry.finished{return Err("Sprachausgabe wurde abgebrochen oder bereits beendet".into());}
    // Deduplicate synthesis without holding the cache lock across network I/O.
    // Stop/completion can immediately release this bot's buffers.
    let _synthesis=tokio::select!{
        lock=state.overlay.synthesis.lock()=>lock,
        _=async{while !entry.cancel.load(Ordering::Relaxed){tokio::time::sleep(Duration::from_millis(75)).await;}}=>return Err("Sprachausgabe wurde abgebrochen".into()),
    };
    let still_pending=||state.overlay.speech.lock().map(|entries|entries.iter().any(|current|current.id==id&&current.bot_id==bot_id&&!current.finished&&!current.cancel.load(Ordering::Relaxed))).unwrap_or(false);
    if entry.cancel.load(Ordering::Relaxed)||!still_pending(){return Err("Sprachausgabe wurde abgebrochen oder bereits beendet".into());}
    if let Some(audio)=state.overlay.audio.lock().map_err(|_|"Audio-Cache nicht verfügbar")?.get(id){return Ok(audio);}
    let usage=crate::usage_budget::Context::new(&entry.bot_id,entry.usage_origin);
    state.store.preflight_usage_cost(&usage,crate::usage_budget::Cost::tts(entry.synthesis_text().chars().count() as u64))?;
    let key=state.store.key_for(&entry.voice)?;
    if entry.cancel.load(Ordering::Relaxed)||!still_pending(){return Err("Sprachausgabe wurde abgebrochen oder bereits beendet".into());}
    let audio=crate::providers::synthesize_metered(&entry.voice,entry.synthesis_text(),key,&state.store,&usage,entry.cancel.clone()).await?;
    let entries=state.overlay.speech.lock().map_err(|_|"Sprachausgabewarteschlange nicht verfügbar")?;
    if entry.cancel.load(Ordering::Relaxed)||!entries.iter().any(|current|current.id==id&&current.bot_id==bot_id&&!current.finished&&!current.cancel.load(Ordering::Relaxed)){return Err("Sprachausgabe wurde abgebrochen oder bereits beendet".into());}
    let protected=entries.iter().filter(|entry|entry.started&&!entry.finished).map(|entry|entry.id.clone()).collect();
    state.overlay.audio.lock().map_err(|_|"Audio-Cache nicht verfügbar")?.insert(id,audio.clone(),&protected,AUDIO_CACHE_BYTES)?;Ok(audio)
}
#[derive(Debug,PartialEq)]
enum AssetRoute { ImportedAvatar, BundledFrontend }
fn classify_asset(name:&str)->Option<AssetRoute>{
    if name.is_empty()||name.len()>256||name.contains('/')||name.contains('\\')||name.contains("..")||!name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'.'||b==b'_'){return None;}
    Some(if crate::avatar_metadata::imported_asset_name(name){AssetRoute::ImportedAvatar}else{AssetRoute::BundledFrontend})
}
fn authorize_asset(route:Option<AssetRoute>,session_allowed:impl FnOnce()->bool)->Result<AssetRoute,StatusCode>{
    match route{
        Some(AssetRoute::ImportedAvatar) if !session_allowed()=>Err(StatusCode::UNAUTHORIZED),
        Some(route)=>Ok(route),
        None=>Err(StatusCode::NOT_FOUND),
    }
}
async fn asset(State(app):State<tauri::AppHandle>,Path(name):Path<String>,Query(q):Query<Access>)->Response {
    match authorize_asset(classify_asset(&name),||allowed(&app,&q)){
        Ok(AssetRoute::ImportedAvatar)=>(),
        // Vite emits its JavaScript, CSS and fonts under /assets too. They must
        // resolve from the bundle, never from the private avatar directory.
        Ok(AssetRoute::BundledFrontend)=>return serve_asset(&app,&format!("assets/{name}")),
        Err(status)=>return status.into_response(),
    }
    // Recheck legacy imports as well: the OBS browser must never receive an
    // unchecked model that could ask GLTFLoader to fetch external resources.
    // This happens per model HTTP request, never in the animation frame loop.
    let root=app.state::<crate::AppState>().store.root.clone();let requested=name.clone();
    match tokio::task::spawn_blocking(move||crate::avatar_metadata::for_asset(&root,Some(&requested))).await{
        Ok(Ok(_))=>(),
        Ok(Err(message))=>return (StatusCode::UNPROCESSABLE_ENTITY,[(header::CACHE_CONTROL,"no-store")],message).into_response(),
        Err(_)=>return (StatusCode::INTERNAL_SERVER_ERROR,"Die Modellprüfung konnte nicht abgeschlossen werden.").into_response(),
    }
    let path=app.state::<crate::AppState>().store.root.join("avatars").join(&name);
    match tokio::fs::read(path).await{
        Ok(bytes)=>([(header::CONTENT_TYPE,"model/gltf-binary"),(header::ACCESS_CONTROL_ALLOW_ORIGIN,"*"),(header::CACHE_CONTROL,"public, max-age=3600")],bytes).into_response(),
        Err(_)=>StatusCode::NOT_FOUND.into_response(),
    }
}
async fn index(State(app):State<tauri::AppHandle>,Query(q):Query<Access>)->Response{
    #[cfg(debug_assertions)] if app.asset_resolver().get("index.html".to_owned()).is_none(){
        let mut url=reqwest::Url::parse("http://localhost:1420/?overlay=1").unwrap();
        {let mut pairs=url.query_pairs_mut();if let Some(session)=q.session{pairs.append_pair("session",&session);}if let Some(bot)=q.bot{pairs.append_pair("bot",&bot);}}
        return (StatusCode::TEMPORARY_REDIRECT,[(header::LOCATION,url.to_string())]).into_response();
    }
    let _=q;serve_asset(&app,"index.html")
}
async fn frontend_asset(State(app):State<tauri::AppHandle>,Path(path):Path<String>)->Response{serve_asset(&app,&path)}
fn serve_asset(app:&tauri::AppHandle,path:&str)->Response{
    if path.split('/').any(|segment|segment==".."){return StatusCode::NOT_FOUND.into_response();}
    if let Some(asset)=app.asset_resolver().get(path.to_owned()){return ([(header::CONTENT_TYPE,asset.mime_type),(header::ACCESS_CONTROL_ALLOW_ORIGIN,"*".to_string())],asset.bytes).into_response();}
    (StatusCode::NOT_FOUND,"Frontend fehlt. Bitte zuerst pnpm build ausführen.").into_response()
}

pub fn active_bots(app:&tauri::AppHandle)->Vec<String>{
    let mut ids:Vec<String>=app.webview_windows().keys().filter_map(|label|label.strip_prefix("avatar-").filter(|id|crate::storage::valid_id(id)).map(str::to_owned)).collect();ids.sort();ids
}
pub fn is_open(app:&tauri::AppHandle,bot_id:&str)->bool{app.get_webview_window(&format!("avatar-{bot_id}")).is_some()}
pub fn open(app:&tauri::AppHandle)->Result<(),String>{
    let profile=app.state::<crate::AppState>().store.profile()?;open_for_bot(app,profile["id"].as_str().ok_or("Bot-ID fehlt")?)
}
pub fn close_for_bot(app:&tauri::AppHandle,bot_id:&str)->Result<(),String>{
    if !crate::storage::valid_id(bot_id){return Err("Ungültige Bot-ID".into());}
    let state=app.state::<crate::AppState>();let store=state.store.scoped(bot_id)?;
    let profile=store.patch_output(json!({"streamingEnabled":false,"mode":"studio"}))?;
    state.overlay.publish(app,json!({"kind":"stop","botId":bot_id}));state.overlay.publish(app,json!({"kind":"profile","botId":bot_id,"profile":public_profile(&profile)}));
    if let Some(window)=app.get_webview_window(&format!("avatar-{bot_id}")){window.close().map_err(|error|error.to_string())?;}
    let _=app.emit("cohost://overlay-state",json!({"botId":bot_id,"active":false}));Ok(())
}
pub fn configure_for_bot(app:&tauri::AppHandle,bot_id:&str,_always_on_top:bool,_click_through:bool)->Result<(),String>{
    if !crate::storage::valid_id(bot_id){return Err("Ungültige Bot-ID".into());}
    let handle=app.clone();let id=bot_id.to_owned();
    tauri::async_runtime::spawn(async move{
        let main=handle.clone();let _=handle.run_on_main_thread(move||{
            if let Some(window)=main.get_webview_window(&format!("avatar-{id}")){
                let state=main.state::<crate::AppState>();let Ok(profile)=state.store.bot_profile(&id)else{return;};
                let always_on_top=profile["output"]["alwaysOnTop"].as_bool().unwrap_or(false);let click_through=profile["output"]["clickThrough"].as_bool().unwrap_or(false);
                let result=window.set_always_on_top(always_on_top).and_then(|_|window.set_ignore_cursor_events(click_through||main.state::<crate::AppState>().presentation.guarded_window(&id)));
                if result.is_err(){let _=main.emit("cohost://agent",json!({"kind":"error","botId":id,"text":"Die Desktop-Darstellung konnte nicht angepasst werden."}));}
            }
        });
    });Ok(())
}
pub fn open_for_bot(app:&tauri::AppHandle,bot_id:&str)->Result<(),String>{
    let state=app.state::<crate::AppState>();let store=state.store.scoped(bot_id)?;let mut profile=store.profile()?;
    if profile["output"]["mode"]!="overlay"||profile["output"]["streamingEnabled"]!=true{
        profile=store.patch_output(json!({"mode":"overlay","streamingEnabled":true}))?;
        state.overlay.publish(app,json!({"kind":"stop","botId":bot_id}));state.overlay.publish(app,json!({"kind":"profile","botId":bot_id,"profile":public_profile(&profile)}));
    }
    let label=format!("avatar-{bot_id}");
    if let Some(window)=app.get_webview_window(&label){crate::overlay_presentation::sync(app,bot_id)?;window.show().map_err(|error|error.to_string())?;window.set_focus().map_err(|error|error.to_string())?;let _=app.emit("cohost://overlay-state",json!({"botId":bot_id,"active":true}));return Ok(());}
    if let Some(legacy)=app.get_webview_window("avatar"){let _=legacy.close();}
    let window=tauri::WebviewWindowBuilder::new(app,&label,tauri::WebviewUrl::App(format!("index.html?overlay=1&bot={bot_id}").into()))
        .title(format!("OrbitPal · {}",profile["name"].as_str().unwrap_or("Charakter"))).inner_size(520.,720.).transparent(true).decorations(false).shadow(false).resizable(true).visible(false)
        .always_on_top(profile["output"]["alwaysOnTop"].as_bool().unwrap_or(false)).build().map_err(|error|error.to_string())?;
    let signals=Arc::new(crate::window_signals::WindowSignals::default());
    let mut initial_bounds=None;
    if let Some(bounds)=store.window_bounds()?{
        let x=bounds["x"].as_i64().unwrap_or(0).clamp(-65535,65535)as i32;let y=bounds["y"].as_i64().unwrap_or(0).clamp(-65535,65535)as i32;
        let width=bounds["width"].as_u64().unwrap_or(520).clamp(160,8192)as u32;let height=bounds["height"].as_u64().unwrap_or(720).clamp(160,8192)as u32;
        let visible=window.available_monitors().map(|monitors|monitors.iter().any(|monitor|{let p=monitor.position();let s=monitor.size();(x as i64+width as i64)>p.x as i64&&(y as i64+height as i64)>p.y as i64&&(x as i64)<p.x as i64+s.width as i64&&(y as i64)<p.y as i64+s.height as i64})).unwrap_or(false);
        initial_bounds=if visible{Some(crate::window_signals::Bounds{x,y,width,height})}else{None};
        let _=window.set_size(tauri::PhysicalSize::new(width,height));if visible{let _=window.set_position(tauri::PhysicalPosition::new(x,y));}else{let _=window.center();}
    }
    state.presentation.register_window(bot_id,signals.clone(),initial_bounds)?;
    window.set_ignore_cursor_events(profile["output"]["clickThrough"].as_bool().unwrap_or(false)).map_err(|error|error.to_string())?;window.show().map_err(|error|error.to_string())?;
    let handle=app.clone();let bot_id=bot_id.to_owned();let measured=window.clone();let revision=Arc::new(std::sync::atomic::AtomicU64::new(0));
    window.on_window_event(move|event|{
        match event {
            tauri::WindowEvent::Moved(position)=>signals.observe_move(position.x,position.y),
            tauri::WindowEvent::Resized(size)=>signals.observe_resize(size.width,size.height),
            _=>(),
        }

        if matches!(event,tauri::WindowEvent::Moved(_)|tauri::WindowEvent::Resized(_)|tauri::WindowEvent::CloseRequested{..}){
            revision.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
            if handle.state::<crate::AppState>().presentation.current_window(&bot_id,&signals)&&!handle.state::<crate::AppState>().presentation.suppress_bounds(&bot_id){if let(Ok(position),Ok(size))=(measured.outer_position(),measured.inner_size()){
                let bounds=json!({"x":position.x,"y":position.y,"width":size.width,"height":size.height});let ticket=revision.fetch_add(1,std::sync::atomic::Ordering::Relaxed)+1;
                if matches!(event,tauri::WindowEvent::CloseRequested{..}){if let Ok(store)=handle.state::<crate::AppState>().store.scoped(&bot_id){let _=store.save_window_bounds(&bounds);}}
                else{let pending=revision.clone();let app=handle.clone();let bot=bot_id.clone();let current=measured.clone();let instance=signals.clone();tauri::async_runtime::spawn(async move{
                    tokio::time::sleep(Duration::from_millis(350)).await;let main=app.clone();
                    let _=app.run_on_main_thread(move||{
                        if pending.load(std::sync::atomic::Ordering::Relaxed)!=ticket||!main.state::<crate::AppState>().presentation.current_window(&bot,&instance)||main.state::<crate::AppState>().presentation.suppress_bounds(&bot){return;}
                        if let(Ok(position),Ok(size))=(current.outer_position(),current.inner_size()){
                            let bounds=json!({"x":position.x,"y":position.y,"width":size.width,"height":size.height});
                            if let Ok(store)=main.state::<crate::AppState>().store.scoped(&bot){let _=store.save_window_bounds(&bounds);}
                        }
                    });
                });}
            }
        }
        }
        if let tauri::WindowEvent::ScaleFactorChanged{..}=event{crate::overlay_presentation::sync_or_report(&handle,&bot_id);}
        if let tauri::WindowEvent::Destroyed=event{
            // Setters can dispatch Destroyed inline on Main. Defer cleanup,
            // never wait for the transition's Hub mutex from that callback.
            let app=handle.clone();let bot=bot_id.clone();let instance=signals.clone();
            tauri::async_runtime::spawn(async move{
                let main=app.clone();let _=app.run_on_main_thread(move||{
                    let state=main.state::<crate::AppState>();
                    let Some(replacing)=state.presentation.forget_window(&bot,&instance)else{return;};
                    if replacing {
                        // Retiring a Webview also loses its playing/prepared
                        // audio client. Cancel exact jobs before replacement;
                        // never leave host history/playback busy until timeout.
                        let jobs=state.overlay.speech.lock().map(|entries|entries.iter().filter(|entry|entry.bot_id==bot&&entry.output=="overlay").map(|entry|entry.id.clone()).collect::<Vec<_>>()).unwrap_or_default();
                        for id in jobs{let _=cancel_playback(&main,&id,&bot,None);}
                        // Closing/Stop or a profile change may have happened
                        // since timeout. Reopen only the still-selected output.
                        if state.store.bot_profile(&bot).is_ok_and(|profile|profile["output"]["mode"]=="overlay"&&profile["output"]["streamingEnabled"]==true){
                            if let Err(error)=open_for_bot(&main,&bot){let _=main.emit("cohost://agent",json!({"kind":"error","botId":bot,"text":error}));}
                        }
                        return;
                    }
                    if let Ok(store)=state.store.scoped(&bot){if let Ok(profile)=store.profile(){if profile["output"]["mode"]=="overlay"{
                        if let Ok(profile)=store.patch_output(json!({"streamingEnabled":false,"mode":"studio"})){state.overlay.publish(&main,json!({"kind":"stop","botId":bot}));state.overlay.publish(&main,json!({"kind":"profile","botId":bot,"profile":public_profile(&profile)}));}
                    }}}
                    let _=main.emit("cohost://overlay-state",json!({"botId":bot,"active":false}));let _=main.emit("cohost://playback",json!({"botId":bot,"speaking":false,"output":"overlay"}));
                });
            });
        }
    });
    crate::overlay_presentation::sync(app,profile["id"].as_str().ok_or("Bot-ID fehlt")?)?;
    let _=app.emit("cohost://overlay-state",json!({"botId":profile["id"],"active":true}));Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn public_profile_never_discloses_autopilot_screen_configuration_or_instructions(){
        let mut profile:Value=serde_json::from_str(include_str!("../../public/default-profile.json")).unwrap();
        profile["autopilot"]["instructions"]=json!("private instruction fixture");profile["autopilot"]["screenServerId"]=json!("private server fixture");profile["autopilot"]["screenToolArguments"]=json!("{\"private\":true}");
        profile["screenContext"]=json!({"enabled":true,"displayId":"private monitor fixture"});
        let public=public_profile(&profile);assert!(public.get("autopilot").is_none());assert!(!public.to_string().contains("private"));assert!(public.get("screenContext").is_none());assert!(public.get("model").is_none());assert!(public.get("mcpServers").is_none());
    }
    #[test]
    fn obs_frontend_bundles_and_imported_models_have_distinct_asset_routes(){
        for name in ["index-DA3KJSJI.js","index-BrBgwP07.css","chunk_ab-c12.js","manrope-latin-wght-normal.woff2"]{assert_eq!(classify_asset(name),Some(AssetRoute::BundledFrontend));}
        for extension in ["vrm","glb"]{let name=format!("{}.{extension}",uuid::Uuid::new_v4());assert_eq!(classify_asset(&name),Some(AssetRoute::ImportedAvatar));}
        for name in ["","../cohost.sqlite3","folder/model.glb","folder\\model.vrm","index.js?secret=1","%2e%2e.glb"]{assert_eq!(classify_asset(name),None);}
        // A guessed filename must never select an arbitrary app-data file.
        assert_eq!(classify_asset("cohost.sqlite3"),Some(AssetRoute::BundledFrontend));
        assert_eq!(classify_asset("model.vrm"),Some(AssetRoute::BundledFrontend));
    }
    #[test]
    fn private_avatar_assets_require_a_session_while_frontend_imports_are_public(){
        for extension in ["glb","vrm"]{
            let name=format!("{}.{extension}",uuid::Uuid::new_v4());
            assert_eq!(authorize_asset(classify_asset(&name),||false),Err(StatusCode::UNAUTHORIZED));
            assert_eq!(authorize_asset(classify_asset(&name),||true),Ok(AssetRoute::ImportedAvatar));
        }
        for name in ["index-DA3KJSJI.js","index-BrBgwP07.css","manrope-latin-wght-normal.woff2"]{
            assert_eq!(authorize_asset(classify_asset(name),||panic!("Public frontend imports must not require a session")),Ok(AssetRoute::BundledFrontend));
        }
        assert_eq!(authorize_asset(classify_asset("../secret.vrm"),||panic!("Malformed paths must fail before authorization")),Err(StatusCode::NOT_FOUND));
    }
    fn speech(provider:&str)->SpeechEntry{
        SpeechEntry{id:"speech-fixture".into(),bot_id:"nova".into(),text:"Hallo zusammen!".into(),speech_text:"[crying] Hallo [shouts] zusammen!".into(),voice:json!({"provider":provider}),emotion:"neutral".into(),source:"twitch".into(),user:"viewer".into(),emotes:Vec::new(),output:"studio".into(),user_text:"Grüße den Chat".into(),hindsight:json!({"private_config_marker":"not an event field"}),assistant_name:"Nova".into(),chat_route:None,started:false,finished:false,cancel:Arc::new(AtomicBool::new(false)),usage_origin:crate::usage_budget::Origin::Manual}
    }
    #[test]
    fn elevenlabs_keeps_audio_tags_in_synthesis_only(){
        let entry=speech("elevenlabs");
        assert_eq!(entry.synthesis_text(),"[crying] Hallo [shouts] zusammen!");
        assert_eq!(entry.presentation_event()["text"],"Hallo zusammen!");
    }
    #[test]
    fn other_speech_providers_receive_clean_text(){
        for provider in ["system","openai-tts","piper","custom",""]{assert_eq!(speech(provider).synthesis_text(),"Hallo zusammen!");}
    }
    #[test]
    fn speech_job_keeps_both_texts_while_presentation_omits_raw_tags_and_config(){
        let entry=speech("elevenlabs");let event=entry.speech_event();
        assert_eq!(event["text"],"Hallo zusammen!");assert_eq!(event["speechText"],entry.speech_text);
        assert!(event.get("hindsight").is_none());
        let presentation=entry.presentation_event();
        assert!(presentation.get("speechText").is_none());assert!(!presentation.to_string().contains("[crying]"));assert!(!presentation.to_string().contains("private_config_marker"));
    }
    #[test]
    fn queued_voice_survives_both_provider_change_directions_without_exposing_credentials(){
        for (first,next) in [("system","elevenlabs"),("elevenlabs","system")]{
            let mut profile=json!({"voice":{"provider":first,"voiceId":"chosen-voice","language":"fr-FR","rate":1.2,"volume":0.4,"pitch":1.1,"enabled":true,"apiKeyId":"private-binding","baseUrl":"https://private.invalid","model":"private-model"}});
            let mut entry=speech(first);entry.voice=profile["voice"].clone();
            profile["voice"]["provider"]=json!(next);profile["voice"]["language"]=json!("es-ES");
            let voice=entry.speech_event()["voice"].clone();
            assert_eq!(voice["provider"],first);assert_eq!(voice["language"],"fr-FR");assert_eq!(voice["rate"],1.2);assert_eq!(voice["volume"],0.4);assert_eq!(voice["pitch"],1.1);assert_eq!(voice["enabled"],true);
            assert_eq!(voice["voiceId"],if first=="system"{json!("chosen-voice")}else{Value::Null});
            assert!(!voice.to_string().contains("private-"));assert!(!voice.to_string().contains("private.invalid"));
        }
        let defaults=public_voice(&json!({"provider":"system"}));
        assert_eq!(defaults["pitch"],1.0);assert_eq!(defaults["rate"],1.0);assert_eq!(defaults["enabled"],true);
    }
    #[test]
    fn pending_cancelled_and_failed_speech_never_enters_history_even_after_restart(){
        let root=std::env::temp_dir().join(format!("orbitpal-pending-history-{}",uuid::Uuid::new_v4()));
        {
            let store=crate::storage::Store::new(root.clone()).unwrap();
            store.add_message("user","Eine Frage","operator","Streamer").unwrap();
            let hub=OverlayHub::default();let entry=speech("system");
            hub.speech.lock().unwrap().push_back(entry);
            assert_eq!(store.history().unwrap().len(),1);
            let mut cancelled=hub.remove_job("speech-fixture","nova",true).unwrap().unwrap();
            assert!(cancelled.start(|_|panic!("Stop must never persist an unheard answer")).is_err());
            let mut failed=speech("elevenlabs");failed.id="failed-tts".into();
            hub.speech.lock().unwrap().push_back(failed);
            hub.remove_job("failed-tts","nova",false).unwrap();
            assert!(!hub.has_any_pending_speech());
        }
        let store=crate::storage::Store::new(root.clone()).unwrap();let history=store.history().unwrap();assert_eq!(history.len(),1);assert_eq!(history[0]["role"],"user");drop(store);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn presentation_commits_once_to_its_bot_and_preserves_newer_successful_history(){
        let root=std::env::temp_dir().join(format!("orbitpal-presented-history-{}",uuid::Uuid::new_v4()));
        let store=crate::storage::Store::new(root.clone()).unwrap();
        let origin=store.scoped("nova").unwrap();let other=store.create_bot("Other",false).unwrap();
        let mut entry=speech("system");
        let persist=|entry:&SpeechEntry|origin.add_message_with_id(&entry.id,"assistant",&entry.text,&entry.source,&entry.assistant_name).map(|_|());
        assert!(entry.start(persist).unwrap());
        assert!(!entry.start(|_|panic!("Duplicate acknowledgement must not commit again")).unwrap());
        let first=origin.history().unwrap();assert_eq!(first.len(),1);assert_eq!(first[0]["id"],"speech-fixture");
        assert_eq!(origin.add_message_with_id(&entry.id,"assistant",&entry.text,&entry.source,&entry.assistant_name).unwrap(),first[0]);
        assert!(origin.add_message_with_id(&entry.id,"assistant","different","twitch","Nova").is_err());
        let mut next=speech("system");next.id="next-success".into();assert!(next.start(persist).unwrap());
        assert!(store.scoped(other["id"].as_str().unwrap()).unwrap().history().unwrap().is_empty());
        drop(origin);drop(store);
        let reloaded=crate::storage::Store::new(root.clone()).unwrap().scoped("nova").unwrap();
        assert_eq!(reloaded.history().unwrap().len(),2);drop(reloaded);std::fs::remove_dir_all(root).unwrap();
    }
    fn cached_audio(bytes:usize)->Value{json!({"audioBase64":"A".repeat(bytes),"mimeType":"audio/mpeg"})}
    #[test]
    fn audio_cache_uses_a_byte_budget_and_lru_while_pinning_playing_jobs(){
        let mut cache=AudioCache::default();let none=HashSet::new();let budget=100;
        for id in ["a","b"]{cache.insert(id,cached_audio(30),&none,budget).unwrap();}
        assert!(cache.get("a").is_some());cache.insert("c",cached_audio(30),&none,budget).unwrap();
        assert!(cache.get("b").is_none());assert!(cache.get("a").is_some());assert!(cache.bytes<=budget);
        let protected=HashSet::from(["a".into()]);cache.insert("d",cached_audio(45),&protected,budget).unwrap();
        assert!(cache.get("a").is_some());assert!(cache.get("c").is_none());
        let before=cache.bytes;assert!(cache.insert("too-large",cached_audio(80),&protected,budget).is_err());assert_eq!(cache.bytes,before);assert!(cache.get("a").is_some());
        for i in 0..200{cache.insert(&format!("bot-{i}"),cached_audio(8),&protected,budget).unwrap();assert!(cache.bytes<=budget);}
        assert!(cache.get("a").is_some());
        assert!(cache.insert("oversized",cached_audio(MAX_ENCODED_AUDIO+1),&none,AUDIO_CACHE_BYTES).is_err());
    }
    #[test]
    fn completion_and_exact_job_expiry_release_buffers_without_cancelling_other_bots_or_newer_jobs(){
        let hub=OverlayHub::default();let mut next=speech("system");next.id="newer".into();next.started=true;
        let mut other=speech("system");other.bot_id="other".into();other.id="other-job".into();
        hub.speech.lock().unwrap().extend([speech("system"),next,other]);
        for id in ["speech-fixture","newer","other-job"]{hub.audio.lock().unwrap().insert(id,cached_audio(16),&HashSet::new(),AUDIO_CACHE_BYTES).unwrap();}
        let expired=hub.remove_job("speech-fixture","nova",true).unwrap().unwrap();assert!(expired.cancel.load(Ordering::Relaxed));
        assert!(hub.remove_job("speech-fixture","nova",true).unwrap().is_none());
        assert!(hub.remove_job("newer","other",true).unwrap().is_none());
        assert!(hub.has_pending_speech("nova"));assert!(hub.has_pending_speech("other"));
        assert_eq!(hub.audio.lock().unwrap().entries.len(),2);
        let completed=hub.remove_job("newer","nova",false).unwrap().unwrap();assert!(!completed.cancel.load(Ordering::Relaxed));assert!(!hub.has_pending_speech("nova"));
        hub.invalidate_speech(Some("nova"));assert!(hub.has_pending_speech("other"));assert_eq!(hub.audio.lock().unwrap().entries.len(),1);
        hub.invalidate_speech(None);assert!(!hub.has_any_pending_speech());assert_eq!(hub.audio.lock().unwrap().bytes,0);
    }
    #[test]
    fn fresh_request_ids_are_canonical_bounded_and_cannot_reuse_a_live_or_recent_job(){
        let hub=OverlayHub::default();let now=Instant::now();let id="00112233-4455-4677-8899-aabbccddeeff".to_owned();
        assert_eq!(hub.claim_reply_id_at(Some(&id),now).unwrap(),id);
        assert!(hub.claim_reply_id_at(Some(&id),now+Duration::from_secs(180)).is_err());
        for invalid in ["not-a-uuid","",&id.to_uppercase()]{assert!(hub.claim_reply_id_at(Some(invalid),now).is_err());}
        let mut active=speech("system");active.id=id.clone();hub.speech.lock().unwrap().push_back(active);
        assert!(hub.claim_reply_id_at(Some(&id),now+Duration::from_secs(300)).is_err());
        hub.remove_job(&id,"nova",true).unwrap();
        assert_eq!(hub.claim_reply_id_at(Some(&id),now+Duration::from_secs(300)).unwrap(),id);
        assert!(uuid::Uuid::parse_str(&hub.claim_reply_id_at(None,now).unwrap()).is_ok());
        let bounded=OverlayHub::default();
        for _ in 0..4096{bounded.claim_reply_id_at(None,now).unwrap();}
        assert!(bounded.claim_reply_id_at(None,now).unwrap_err().contains("Zu viele"));
        assert!(bounded.claim_reply_id_at(None,now+Duration::from_secs(300)).is_ok());
    }
    #[test]
    fn delayed_native_watchdog_cannot_expire_a_replacement_with_a_reused_id(){
        let hub=OverlayHub::default();let old=speech("system");let old_token=old.cancel.clone();
        hub.speech.lock().unwrap().push_back(old);hub.remove_job("speech-fixture","nova",true).unwrap();
        let replacement=speech("system");let replacement_token=replacement.cancel.clone();hub.speech.lock().unwrap().push_back(replacement);
        assert!(hub.remove_job_if("speech-fixture","nova",true,Some(&old_token)).unwrap().is_none());
        assert!(!replacement_token.load(Ordering::Relaxed));assert!(hub.has_pending_speech("nova"));
    }
    #[test]
    fn emote_sound_payload_uses_only_known_ids_and_keeps_viewer_grants(){
        let mut profile=json!({"id":"nova","emotes":[{"id":"wave","action":"wave","allowChat":false,"sound":"builtin:wave","soundVolume":0.35}]});
        assert!(prepare_emote_profile(std::path::Path::new("unused"),&profile,"wave",true).is_err());
        let event=prepare_emote_profile(std::path::Path::new("unused"),&profile,"wave",false).unwrap();assert_eq!(event["sound"],"builtin:wave");assert_eq!(event["soundVolume"],0.35);assert_eq!(event["botId"],"nova");
        profile["emotes"][0]["sound"]=json!("https://untrusted.invalid/sound.mp3");assert!(prepare_emote_profile(std::path::Path::new("unused"),&profile,"wave",false).is_err());
        profile["emotes"][0]["sound"]=json!("custom:../../../private.wav");assert!(prepare_emote_profile(std::path::Path::new("unused"),&profile,"wave",false).is_err());
    }
    #[test]
    fn external_emote_playback_rejects_invalid_ids_and_output_modes(){
        let profile=json!({"id":"nova","output":{"mode":"obs"}});let id=uuid::Uuid::new_v4().to_string();
        for output in ["studio","","other"]{assert!(emote_playback_event(&profile,output,&id,true,None).is_err());assert!(emote_playback_event(&profile,output,&id,false,None).is_err());}
        for invalid in ["","../other","not-a-uuid"]{assert!(emote_playback_event(&profile,"obs",invalid,true,None).is_err());assert!(emote_playback_event(&profile,"obs",invalid,false,None).is_err());}
        let event=emote_playback_event(&profile,"obs",&id,true,None).unwrap();assert_eq!(event,json!({"botId":"nova","output":"obs","id":id,"playing":true,"activity":"foreground"}));
    }
    #[test]
    fn external_emote_playback_start_requires_current_mode_but_end_survives_reconfiguration(){
        let profile=json!({"id":"nova","output":{"mode":"studio"}});let id=uuid::Uuid::new_v4().to_string();
        for output in ["overlay","obs"]{assert!(emote_playback_event(&profile,output,&id,true,None).is_err());assert_eq!(emote_playback_event(&profile,output,&id,false,None).unwrap()["playing"],false);}
        assert!(emote_playback_event(&json!({"id":"../other","output":{"mode":"obs"}}),"obs",&id,false,None).is_err());
    }
    #[test]
    fn external_model_audio_classification_preserves_output_and_identity_validation(){
        let profile=json!({"id":"nova","output":{"mode":"obs"}});let id=uuid::Uuid::new_v4().to_string();
        for activity in ["foreground","ambient"]{
            let event=emote_playback_event(&profile,"obs",&id,true,Some(activity)).unwrap();
            assert_eq!(event,json!({"botId":"nova","output":"obs","id":id,"playing":true,"activity":activity}));
            assert!(emote_playback_event(&profile,"overlay",&id,true,Some(activity)).is_err());
            assert!(emote_playback_event(&profile,"obs","not-a-uuid",false,Some(activity)).is_err());
            assert!(emote_playback_event(&profile,"overlay",&id,false,Some(activity)).is_ok());
        }
        for activity in ["", "idle", "AMBIENT", "speech"]{
            assert!(emote_playback_event(&profile,"obs",&id,true,Some(activity)).is_err());
            assert!(emote_playback_event(&profile,"obs",&id,false,Some(activity)).is_err());
        }
    }
    #[test]
    fn output_changes_can_detect_pending_audio_without_blocking_other_bots(){
        let hub=OverlayHub::default();hub.speech.lock().unwrap().push_back(speech("system"));
        assert!(hub.has_pending_speech("nova"));assert!(!hub.has_pending_speech("other-bot"));
        hub.speech.lock().unwrap()[0].finished=true;assert!(!hub.has_pending_speech("nova"));
        let mut entries=hub.speech.lock().unwrap();entries[0].finished=false;entries[0].cancel.store(true,Ordering::Relaxed);drop(entries);assert!(!hub.has_pending_speech("nova"));
    }
}
