mod storage;
mod usage_budget;
mod usage_settings;
#[cfg(feature="native-world-fixture")]
pub mod native_world_fixture;
mod credentials;
mod credential_policy;
mod api_connections;
mod api_connection_settings;
mod credential_session;
mod credential_session_settings;
mod providers;
mod transcription;
mod cli_provider;
mod bounded_io;
mod chat;
mod chat_route;
mod mcp;
mod agent;
mod overlay;
mod overlay_presentation;
mod window_signals;
mod overlay_http;
mod memory;
mod knowledge;
mod runtime_command;
mod speech_text;
mod extensions;
mod skill_library;
mod emote_audio;
mod emote_capabilities;
mod vision;
mod control_mcp;
mod updater;
mod avatar_metadata;
mod motions;
mod avatar_audio;
mod gltf_import;
mod autopilot;
mod life_availability;
mod life_runtime;
pub mod life_domain;
mod life_self_care;
mod social;
mod social_commands;
mod social_dialogue;
mod autonomy_context;
mod character_development;
mod experience;
mod platform_impulses;
mod native_interaction;
mod experience_commands;
mod content_packs;
mod desktop_affordances;
mod action_executor;
mod world_state;
mod world_hub;
mod world_output;
mod screen_capture;
mod screen_context;
mod screen_portal;
mod microphone_permission;
mod screen_permission;
#[cfg(any(target_os = "macos", test))]
mod permission_request;
mod ui_locale;

use serde_json::{json,Value};
use std::sync::{Mutex,Arc,atomic::{AtomicBool,Ordering}};
use tauri::{Emitter,Manager,State};

pub struct AppState {
    store:storage::Store,
    mcp:mcp::McpHub,
    chat:chat::ChatHub,
    cancel:Mutex<Option<Arc<AtomicBool>>>,
    generation:tokio::sync::Mutex<()>,
    overlay:overlay::OverlayHub,
    presentation:overlay_presentation::PresentationHub,
    control_mcp:control_mcp::ControlMcp,
    updater:updater::UpdateState,
    autopilot:autopilot::AutopilotHub,
    life_runtime:life_runtime::LifeRuntime,
    world:world_hub::WorldHub,
    screen_context:screen_context::ScreenState,
}

#[tauri::command(async)]
fn get_app_state(window:tauri::WebviewWindow,state:State<'_,AppState>,app:tauri::AppHandle,bot_id:Option<String>)->Result<Value,String> {
    let id=bot_id.unwrap_or_else(||state.store.profile().ok().and_then(|p|p["id"].as_str().map(str::to_owned)).unwrap_or("nova".into()));
    let store=state.store.scoped(&id)?;let profile=store.profile()?;let presentation=state.presentation.snapshot(&profile);
    let life_outputs=state.store.list_bots()?.iter().filter_map(|bot|bot["id"].as_str()).map(|id|state.life_runtime.session_output(id)).collect::<Result<Vec<_>,_>>()?;
    Ok(json!({"profile":profile,"presentation":presentation,"renderSettings":store.render_settings(&id).ok(),"bots":bot_summaries(&state,&app)?,"documents":store.documents()?,"skills":store.skills()?,"library":store.extension_library()?,"knowledge":store.knowledge()?,"history":store.history()?,"secretIds":store.secret_ids()?,"overlayUrl":state.overlay.url_for_bot(&id),"overlayActive":overlay::is_open(&app,&id),"appDataDir":store.root.to_string_lossy(),"autopilot":state.autopilot.all()?.into_iter().map(|runtime|state.life_runtime.present(runtime)).collect::<Vec<_>>(),"lifeRuntime":state.life_runtime.snapshot_for(&state.autopilot)?,"lifeOutput":state.life_runtime.session_output(&id)?,"lifeOutputs":life_outputs,"world":if window.label()=="main"{Some(state.world.snapshot()?)}else if window.label().strip_prefix("avatar-")==Some(id.as_str()){state.world.snapshot_for(&id)}else{None}}))
}
#[tauri::command(async)]
fn stop_life_session_exact(window:tauri::WebviewWindow,app:tauri::AppHandle,bot_id:String,session_id:String,revision:u64)->Result<bool,String>{
    if window.label()!="main"{return Err("Life-Sitzungen können nur im Studio gesteuert werden.".into());}
    Ok(app.state::<AppState>().life_runtime.stop_exact_session(&app,&bot_id,&session_id,revision))
}
#[tauri::command]
async fn start_life_scene(window:tauri::WebviewWindow,app:tauri::AppHandle,expected_revision:Option<u64>)->Result<life_runtime::core::RuntimeSnapshot,String>{
    if window.label()!="main"{return Err("Life-Sitzungen können nur im Studio gesteuert werden.".into());}
    life_runtime::session_output::start_scene(&app,expected_revision).await
}
#[tauri::command(async)]
fn get_emote_capabilities(state:State<'_,AppState>,bot_id:String)->Result<Value,String>{
    let profile=state.store.bot_profile(&bot_id)?;
    Ok(emote_capabilities::configured(&emote_capabilities::inspect(&state.store.root,&profile)?,&profile))
}
#[tauri::command(async)]
fn get_extension_library(state:State<'_,AppState>)->Result<Value,String>{state.store.extension_library()}
#[tauri::command(async)]
fn get_library_skill(state:State<'_,AppState>,id:String)->Result<Value,String>{skill_library::SkillLibrary::new(&state.store.root)?.read(&id)}
#[tauri::command]
fn install_control_extension(state:State<'_,AppState>,app:tauri::AppHandle,window:tauri::WebviewWindow)->Result<Value,String>{
    if window.label()!="main"{return Err("Control-Erweiterungen werden ausschließlich im Studio installiert".into());}
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Installieren des Control-MCP beenden".to_owned())?;
    let installed=state.store.install_control_extension(control_mcp::CONTROL_SKILL)?;
    let _=app.emit("cohost://library-changed",json!({"kind":"control"}));Ok(installed)
}
#[tauri::command(async)]
fn get_control_mcp_config(app:tauri::AppHandle,window:tauri::WebviewWindow)->Result<Value,String>{if window.label()!="main"{return Err("Die Control-Verbindungskonfiguration ist nur für den Streamer im Studio verfügbar".into());}app.state::<AppState>().control_mcp.client_config(&app)}
#[tauri::command]
async fn save_library_mcp(state:State<'_,AppState>,app:tauri::AppHandle,server:Value)->Result<Value,String>{
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Ändern der gemeinsamen Bibliothek beenden".to_owned())?;
    let saved=state.store.save_library_mcp(server)?;
    state.mcp.disconnect(saved["id"].as_str().ok_or("MCP-ID fehlt")?).await;
    let _=app.emit("cohost://library-changed",json!({"kind":"mcp"}));Ok(saved)
}
#[tauri::command]
async fn delete_library_mcp(state:State<'_,AppState>,app:tauri::AppHandle,server_id:String)->Result<(),String>{
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Entfernen einer gemeinsamen Erweiterung beenden".to_owned())?;
    state.store.delete_library_mcp(&server_id)?;state.mcp.disconnect(&server_id).await;
    let _=app.emit("cohost://library-changed",json!({"kind":"mcp"}));Ok(())
}
fn bot_summaries(state:&AppState,app:&tauri::AppHandle)->Result<Vec<Value>,String>{
    let live=overlay::active_bots(app);let mut bots=state.store.list_bots()?;
    for bot in &mut bots{let id=bot["id"].as_str().unwrap_or_default().to_owned();bot["live"]=json!(live.contains(&id));bot["overlayUrl"]=json!(state.overlay.url_for_bot(&id));}
    Ok(bots)
}
#[tauri::command(async)]
fn list_bots(state:State<'_,AppState>,app:tauri::AppHandle)->Result<Vec<Value>,String>{bot_summaries(&state,&app)}
#[tauri::command]
async fn create_bot(state:State<'_,AppState>,app:tauri::AppHandle,name:String,copy_current:bool)->Result<Value,String>{
    state.life_runtime.operator_activity();
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Anlegen eines Bots beenden".to_owned())?;
    let previous=state.store.profile()?["id"].as_str().unwrap_or_default().to_owned();
    let profile=state.store.create_bot(&name,copy_current)?;
    state.screen_context.cancel_inflight(&previous)?;
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}
#[tauri::command]
async fn select_bot(state:State<'_,AppState>,app:tauri::AppHandle,id:String)->Result<Value,String>{
    state.life_runtime.operator_activity();
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Botwechsel beenden".to_owned())?;
    let previous=state.store.profile()?["id"].as_str().unwrap_or_default().to_owned();
    let profile=state.store.select_bot(&id)?;
    if previous!=id{state.screen_context.cancel_inflight(&previous)?;}
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}
#[tauri::command]
async fn save_profile(state:State<'_,AppState>,app:tauri::AppHandle,profile:Value,bot_id:Option<String>)->Result<Value,String> {
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;
    let store=scoped.as_ref().unwrap_or(&state.store);
    #[cfg(feature="native-world-fixture")] native_world_fixture::validate_profile_for_fixture(store,&profile)?;
    let old=store.profile()?;
    let _profile_switch=if old["id"]!=profile["id"]{Some(state.generation.try_lock().map_err(|_|"Profilwechsel während einer laufenden Antwort nicht möglich. Bitte zuerst abbrechen.".to_owned())?)}else{None};
    // An identical save (also used before ordinary messages) does not disturb
    // autonomous sessions. Changed settings revoke any in-flight snapshot.
    let write=||state.screen_context.change_profile(old["id"].as_str().ok_or("Bot-ID fehlt")?,||store.profile(),&profile,||store.save_profile(profile.clone()));
    let (profile,runtime)=if old!=profile{state.life_runtime.change_profile(&app,old["id"].as_str().ok_or("Bot-ID fehlt")?,write)?}else{(write()?,None)};
    if old!=profile&&state.overlay.has_autopilot_speech(old["id"].as_str().ok_or("Bot-ID fehlt")?){state.overlay.publish(&app,json!({"kind":"stop","botId":old["id"]}));}
    state.presentation.committed(&profile);
    if let Some(runtime)=runtime{autopilot::publish(&app,runtime);}else{overlay_presentation::sync_or_report(&app,profile["id"].as_str().ok_or("Bot-ID fehlt")?);}
    // Bindings are per bot; revoking one bot must not disconnect another bot's
    // shared transport. Runtime calls recheck the answered bot's current grants.
    if old["output"]["mode"]!=profile["output"]["mode"]{state.overlay.publish(&app,json!({"kind":"stop","botId":profile["id"]}));}
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));
    Ok(profile)
}

#[tauri::command]
fn save_document(state:State<'_,AppState>,app:tauri::AppHandle,name:String,content:String,bot_id:Option<String>)->Result<(),String>{
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);let profile=store.profile()?;let bot_id=profile["id"].as_str().ok_or("Bot-ID fehlt")?;
    let (_,runtime)=state.life_runtime.change_profile(&app,bot_id,||{store.save_document(&name,&content)?;store.profile()})?;
    if state.overlay.has_autopilot_speech(bot_id){state.overlay.publish(&app,json!({"kind":"stop","botId":bot_id}));}
    if let Some(runtime)=runtime{autopilot::publish(&app,runtime);}Ok(())
}
#[tauri::command]
fn save_skill(state:State<'_,AppState>,id:String,content:String)->Result<(),String>{let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor einer gemeinsamen Skilländerung beenden".to_owned())?;state.store.save_skill(&id,&content)}
#[tauri::command]
fn delete_skill(state:State<'_,AppState>,id:String)->Result<(),String>{let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Entfernen eines Skills beenden".to_owned())?;state.store.delete_skill(&id)}
#[tauri::command]
fn import_skill(state:State<'_,AppState>,source_path:String)->Result<Value,String>{let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Installieren eines Skills beenden".to_owned())?;state.store.import_skill(std::path::Path::new(&source_path))}
#[tauri::command]
fn trigger_emote(app:tauri::AppHandle,emote_id:String,bot_id:Option<String>)->Result<Value,String>{
    let state=app.state::<AppState>();state.updater.ensure_idle()?;let id=bot_id.unwrap_or(state.store.profile()?["id"].as_str().unwrap_or("nova").to_owned());let profile=state.store.scoped(&id)?.profile()?;
    overlay::trigger_emote_profile(&app,&profile,&emote_id,false)
}
#[tauri::command]
fn import_emote_sound(state:State<'_,AppState>,source_path:String,bot_id:Option<String>)->Result<String,String>{
    let id=bot_id.unwrap_or(state.store.profile()?["id"].as_str().ok_or("Bot-ID fehlt")?.to_owned());
    state.store.bot_profile(&id)?;
    let imported=emote_audio::import(&state.store.root,&id,std::path::Path::new(&source_path))?;
    imported["id"].as_str().map(str::to_owned).ok_or("Importierte Sound-ID fehlt".into())
}
#[tauri::command]
fn get_emote_sound(state:State<'_,AppState>,sound_id:String,bot_id:String)->Result<Value,String>{
    let profile=state.store.bot_profile(&bot_id)?;
    emote_audio::read(&state.store.root,&profile,&sound_id)
}
#[tauri::command]
async fn import_motion(app:tauri::AppHandle,window:tauri::Window,state:State<'_,AppState>,source_path:String,bot_id:String)->Result<motions::Motion,String>{
    if window.label()!="main"{return Err("VRMA-Import ist nur im Steuerungsfenster verfügbar.".into());}
    state.life_runtime.invalidate_bot(&app,&bot_id);
    state.updater.ensure_idle()?;
    let profile=state.store.bot_profile(&bot_id)?;
    if profile["motions"].as_array().is_some_and(|motions|motions.len()>=motions::MAX_MOTIONS){return Err("Maximal 32 Bewegungsdateien pro Bot.".into());}
    let root=state.store.root.clone();
    tauri::async_runtime::spawn_blocking(move||motions::import(&root,&bot_id,std::path::Path::new(&source_path))).await.map_err(|_|"VRMA-Import wurde unterbrochen.".to_owned())?
}
#[tauri::command]
async fn import_knowledge(state:State<'_,AppState>,source_path:String)->Result<Value,String>{
    let id=state.store.profile()?["id"].as_str().ok_or("Bot-ID fehlt")?.to_owned();
    let store=state.store.scoped(&id)?;
    tokio::task::spawn_blocking(move||store.import_knowledge(std::path::Path::new(&source_path))).await.map_err(|_|"Wissensimport wurde unerwartet beendet".to_owned())?
}
#[tauri::command]
fn delete_knowledge(state:State<'_,AppState>,name:String)->Result<(),String>{state.store.delete_knowledge(&name)}
#[tauri::command]
fn set_secret(state:State<'_,AppState>,id:String,value:String)->Result<(),String>{state.store.set_secret(&id,&value)}
#[tauri::command]
fn delete_secret(state:State<'_,AppState>,id:String)->Result<(),String>{state.store.delete_secret(&id)}
#[tauri::command]
async fn list_models(state:State<'_,AppState>,config:Value)->Result<Vec<String>,String>{let key=state.store.key_for_purpose(&config,credential_policy::Purpose::ModelCatalogue)?;providers::models(&config,key).await}
#[tauri::command]
async fn check_cli_provider(config:Value)->Result<Value,String>{cli_provider::status(&config).await}
#[tauri::command]
async fn list_cli_models(config:Value)->Result<Value,String>{cli_provider::models(&config).await}
#[tauri::command]
async fn send_message(window:tauri::WebviewWindow,app:tauri::AppHandle,text:String,source:Option<String>,user:Option<String>,skill_ids:Option<Vec<String>>,bot_id:Option<String>,request_id:Option<String>,reply_route_id:Option<String>)->Result<Value,String>{usage_settings::main_only(window.label())?;agent::send(app,text,source.unwrap_or("operator".into()),user.unwrap_or("Streamer".into()),skill_ids.unwrap_or_default(),bot_id,request_id,reply_route_id).await}
#[tauri::command]
async fn send_automatic_message(window:tauri::WebviewWindow,app:tauri::AppHandle,text:String,source:String,user:String,bot_id:String,request_id:Option<String>,reply_route_id:Option<String>)->Result<Value,String>{usage_settings::main_only(window.label())?;agent::send_automatic(app,text,source,user,bot_id,request_id,reply_route_id).await}
#[tauri::command]
fn stop_generation(state:State<'_,AppState>,app:tauri::AppHandle)->Result<(),String>{
    #[cfg(feature="native-world-fixture")]
    let fixture_receipts=life_runtime::native_scene::fixture_stop_receipts(&app);
    state.screen_context.portal.close_all();
    if let Some(c)=state.cancel.lock().map_err(|e|e.to_string())?.as_ref(){c.store(true,Ordering::Relaxed);}
    state.overlay.publish(&app,json!({"kind":"stop"}));
    #[cfg(feature="native-world-fixture")]
    life_runtime::native_scene::observe_fixture_stopped(&app,fixture_receipts);
    Ok(())
}
#[tauri::command]
fn clear_memory(state:State<'_,AppState>)->Result<(),String>{let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort zuerst beenden".to_owned())?;state.store.clear_history()}

pub fn resolve_mcp_config(store:&storage::Store,mut config:Value)->Result<Value,String>{
    if let Some(env)=config["env"].as_object_mut(){for (_,v) in env {
        if let Some(id)=v.as_str().and_then(|s|s.strip_prefix("${secret:")).and_then(|s|s.strip_suffix('}')) {
            *v=json!(store.secret(id)?.ok_or_else(||format!("Zugangsdaten-ID {id} für MCP-Umgebung fehlt"))?);
        }
    }}
    Ok(config)
}
pub async fn connect_mcp_shared(app:&tauri::AppHandle,store:&storage::Store,server:Value)->Result<Vec<Value>,String>{
    let state=app.state::<AppState>();
    let mut config=extensions::definition(&server)?;
    let id=config["id"].as_str().ok_or("MCP-ID fehlt")?.to_owned();
    if state.mcp.connected(&id).await{return state.mcp.tools(&id).await;}
    let key=if config["transport"]=="builtin"{
        // The library and exported profiles contain only the builtin marker.
        // Endpoint and independent bearer are resolved for this process only.
        config["transport"]=json!("http");
        config["url"]=json!(state.control_mcp.url(app)?);
        Some(state.control_mcp.bearer_token().to_owned())
    }else{config=resolve_mcp_config(store,config)?;store.key_for(&config)?};
    state.mcp.connect(&config,key).await
}
#[tauri::command]
async fn mcp_connect(state:State<'_,AppState>,app:tauri::AppHandle,server_id:String)->Result<Vec<Value>,String>{
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem manuellen Verbinden eines MCP-Servers beenden".to_owned())?;
    let config=state.store.library_mcp()?.into_iter().find(|s|s["id"].as_str()==Some(&server_id)).ok_or("MCP-Server ist nicht in der gemeinsamen Bibliothek installiert")?;
    connect_mcp_shared(&app,&state.store,config).await
}
#[tauri::command]
async fn mcp_disconnect(state:State<'_,AppState>,server_id:String)->Result<(),String>{state.mcp.disconnect(&server_id).await;Ok(())}
#[tauri::command]
async fn mcp_tools(state:State<'_,AppState>,server_id:String)->Result<Vec<Value>,String>{state.mcp.tools(&server_id).await}
#[tauri::command]
async fn connect_platform(state:State<'_,AppState>,app:tauri::AppHandle,platform:String)->Result<(),String>{
    let profile=state.store.profile()?;
    let config=&profile["chat"][&platform];
    let key=if platform=="youtube"{state.store.api_key_for(config,api_connections::Purpose::YoutubeData)?}else{state.store.key_for(config)?};
    let write_key=if platform=="youtube" && config["sendReplies"]==true {state.store.secret(config["sendSecretId"].as_str().unwrap_or("youtube-write"))?} else {None};
    state.chat.connect(&platform,&profile,key,write_key,app).await
}
#[tauri::command]
async fn disconnect_platform(state:State<'_,AppState>,platform:String)->Result<(),String>{state.chat.disconnect(&platform).await}
#[tauri::command(async)]
fn report_playback(app:tauri::AppHandle,id:String,bot_id:String,speaking:bool,error:Option<String>)->Result<Value,String>{overlay::report_playback(&app,&id,&bot_id,speaking,error.as_deref())}
#[tauri::command(async)]
fn cancel_speech_job(app:tauri::AppHandle,window:tauri::WebviewWindow,id:String,bot_id:String)->Result<Value,String>{
    if window.label()!="main"{return Err("Ausstehende Wiedergaben werden ausschließlich im Studio abgebrochen".into());}
    overlay::cancel_playback(&app,&id,&bot_id,Some("Keine Abschlussmeldung der Sprachausgabe. Automatische Antworten pausiert."))
}
#[tauri::command(async)]
fn report_emote_playback(app:tauri::AppHandle,bot_id:String,output:String,id:String,playing:bool,activity:Option<String>,speech_id:Option<String>)->Result<Value,String>{overlay::report_emote_playback(&app,&bot_id,&output,&id,playing,activity.as_deref(),speech_id.as_deref())}
#[tauri::command]
async fn get_speech_audio(app:tauri::AppHandle,id:String,bot_id:String)->Result<Value,String>{overlay::speech_audio(&app,&id,&bot_id).await}
#[tauri::command]
async fn synthesize_speech(window:tauri::WebviewWindow,state:State<'_,AppState>,text:String,bot_id:Option<String>)->Result<Value,String>{
    usage_settings::main_only(window.label())?;state.updater.ensure_idle()?;
    if text.len()>20_000{return Err("Sprachtext ist zu lang".into());}
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);
    let profile=store.profile()?;let config=&profile["voice"];providers::synthesize(config,&text,store.key_for_purpose(config,credential_policy::Purpose::Speech)?).await
}
#[tauri::command]
async fn transcribe_audio(window:tauri::WebviewWindow,state:State<'_,AppState>,audio_base64:String,mime_type:String,bot_id:Option<String>)->Result<String,String>{
    usage_settings::main_only(window.label())?;state.life_runtime.operator_activity();state.updater.ensure_idle()?;
    if audio_base64.len()>40*1024*1024{return Err("Audioaufnahme ist zu groß".into());}
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);
    let profile=store.profile()?;let config=&profile["stt"];providers::transcribe(config,&audio_base64,&mime_type,store.api_key_for(config,api_connections::Purpose::Transcription)?).await
}
#[tauri::command]
fn open_overlay(app:tauri::AppHandle,bot_id:Option<String>)->Result<(),String>{let id=bot_id.unwrap_or(app.state::<AppState>().store.profile()?["id"].as_str().unwrap_or("nova").to_owned());overlay::open_for_bot(&app,&id)}
#[tauri::command]
fn close_overlay(app:tauri::AppHandle,bot_id:Option<String>)->Result<(),String>{let id=bot_id.unwrap_or(app.state::<AppState>().store.profile()?["id"].as_str().unwrap_or("nova").to_owned());overlay::close_for_bot(&app,&id)}
#[tauri::command]
fn configure_overlay(app:tauri::AppHandle,always_on_top:bool,click_through:bool,bot_id:Option<String>)->Result<(),String>{
    let id=bot_id.unwrap_or(app.state::<AppState>().store.profile()?["id"].as_str().unwrap_or("nova").to_owned());overlay::configure_for_bot(&app,&id,always_on_top,click_through)
}
#[tauri::command]
fn import_avatar(app:tauri::AppHandle,state:State<'_,AppState>,source_path:String)->Result<String,String>{let id=state.store.profile()?["id"].as_str().ok_or("Bot-ID fehlt")?.to_owned();state.life_runtime.invalidate_bot(&app,&id);state.store.import_avatar(std::path::Path::new(&source_path))}
#[tauri::command]
async fn get_avatar_info(state:State<'_,AppState>,asset_path:Option<String>,bot_id:Option<String>)->Result<avatar_metadata::AvatarInfo,String>{
    if let Some(id)=bot_id.as_deref(){state.store.bot_profile(id)?;}
    let root=state.store.root.clone();
    tauri::async_runtime::spawn_blocking(move||avatar_metadata::for_asset(&root,asset_path.as_deref())).await.map_err(|_|"Die Modellprüfung wurde unerwartet beendet".to_owned())?
}
#[tauri::command]
fn export_profile(state:State<'_,AppState>,destination:String)->Result<(),String>{state.store.export(std::path::Path::new(&destination))}
#[tauri::command]
async fn import_profile(state:State<'_,AppState>,app:tauri::AppHandle,source_path:String)->Result<Value,String>{
    state.life_runtime.operator_activity();
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Import beenden".to_owned())?;
    let previous=state.store.profile()?["id"].as_str().unwrap_or_default().to_owned();
    let profile=state.store.import(std::path::Path::new(&source_path))?;
    state.screen_context.cancel_inflight(&previous)?;
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}

#[cfg_attr(mobile,tauri::mobile_entry_point)]
pub fn run(){
    #[cfg(feature="native-world-fixture")] native_world_fixture::refuse_shipping_entry();
    tauri::Builder::default().plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_opener::init()).plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app|{
            let root=app.path().app_data_dir()?;
            let store=storage::Store::new(root).map_err(std::io::Error::other)?;
            initialize_application(app,store)
        })
        .invoke_handler(production_invoke)
        .build(tauri::generate_context!()).expect("OrbitPal konnte nicht gestartet werden").run(application_exit);
}
fn initialize_application(app:&mut tauri::App,store:storage::Store)->Result<(),Box<dyn std::error::Error>>{
    app.manage(ui_locale::UiLocaleState::default());
    app.manage(experience::ExperienceRuntime::default());
    app.manage(platform_impulses::PlatformImpulses::default());
    app.manage(native_interaction::NativeInteraction::default());
    native_interaction::install(app.handle());
    let world=world_hub::WorldHub::new(&store);
    app.manage(AppState {world,store,mcp:mcp::McpHub::default(),chat:chat::ChatHub::default(),cancel:Mutex::new(None),generation:tokio::sync::Mutex::new(()),overlay:overlay::OverlayHub::default(),presentation:overlay_presentation::PresentationHub::default(),control_mcp:control_mcp::ControlMcp::default(),updater:updater::UpdateState::default(),autopilot:autopilot::AutopilotHub::default(),life_runtime:life_runtime::LifeRuntime::default(),screen_context:screen_context::ScreenState::default()});
    tauri::async_runtime::block_on(overlay::start(app.handle().clone())).map_err(std::io::Error::other)?;
    app.state::<AppState>().life_runtime.install(app.handle());
    setup_tray(app)?;
    Ok(())
}
fn production_invoke(invoke:tauri::ipc::Invoke)->bool{
    life_runtime::operator_invoke(&invoke);
    let handler:fn(tauri::ipc::Invoke)->bool=tauri::generate_handler![storage::render_settings::get_render_settings,storage::render_settings::set_render_quality,native_interaction::open_native_character_menu,native_interaction::close_native_character_menu,social_commands::get_social_snapshot,social_commands::invite_social_scene,social_commands::respond_social_invitation,social_commands::cancel_social_scene,social_commands::delete_social_memory,social_commands::reset_social_memory,social_commands::set_social_memory_shared,social_commands::set_social_development,social_commands::reset_social_development,social_commands::set_social_dialogue_policy,experience_commands::get_experience_snapshot,experience_commands::set_experience_preferences,experience_commands::set_experience_pause,experience_commands::request_character_interaction,experience_commands::delete_experience_fact,experience_commands::set_experience_fact_shared,experience_commands::list_content_packs,experience_commands::create_content_pack_character,experience_commands::preview_content_pack,experience_commands::install_content_pack,experience_commands::set_content_pack_active,experience_commands::delete_content_pack,storage::life_care_store::get_life_care_preferences,storage::life_care_store::save_life_care_preferences,life_runtime::native_scene_ipc::report_native_world_asset,life_runtime::native_scene_ipc::register_native_world_output,life_runtime::native_scene_ipc::report_native_world_measurement,life_runtime::native_scene_ipc::report_native_world_action,life_runtime::native_scene_ipc::unregister_native_world_output,start_life_scene,stop_life_session_exact,life_runtime::world_output::register_world_output,life_runtime::world_output::report_world_measurement,life_runtime::world_output::report_world_action,life_runtime::world_output::unregister_world_output,life_runtime::world_output::get_world_output_status,life_runtime::world_output::start_life_move,api_connection_settings::get_api_connections,api_connection_settings::get_api_binding,api_connection_settings::create_api_connection,api_connection_settings::rename_api_connection,api_connection_settings::set_default_api_connection,api_connection_settings::remove_api_connection,api_connection_settings::set_api_connection_secret,life_runtime::actions::get_life_actions,life_runtime::actions::start_life_rest,world_hub::get_world_settings,world_hub::get_world_state,life_runtime::semantic_mirrors::get_world_semantic_mirrors,life_runtime::native_scene_actions::prepare_native_life_action,world_hub::save_world_config,world_hub::place_world_actor,life_runtime::get_life_runtime,life_runtime::get_life_bot,life_runtime::set_life_mode,life_runtime::connect_life_ui,life_runtime::disconnect_life_ui,life_runtime::report_life_ui,credential_session_settings::get_credential_session,credential_session_settings::set_credential_session,credential_session_settings::lock_credential_session,usage_settings::get_usage_budget,usage_settings::set_usage_limits,overlay_presentation::list_output_displays,overlay_presentation::preview_avatar_scale,overlay_presentation::preview_output_monitor,ui_locale::get_ui_locale,ui_locale::set_ui_locale,microphone_permission::get_microphone_permission,microphone_permission::request_microphone_permission,microphone_permission::open_microphone_settings,get_app_state,get_emote_capabilities,get_extension_library,get_library_skill,install_control_extension,get_control_mcp_config,save_library_mcp,delete_library_mcp,list_bots,create_bot,select_bot,save_profile,save_document,save_skill,delete_skill,import_skill,trigger_emote,import_emote_sound,get_emote_sound,import_knowledge,delete_knowledge,set_secret,delete_secret,list_models,check_cli_provider,list_cli_models,send_message,send_automatic_message,stop_generation,autopilot::set_autopilot,autopilot::send_autopilot,screen_context::list_capture_displays,clear_memory,mcp_connect,mcp_disconnect,mcp_tools,connect_platform,disconnect_platform,report_playback,cancel_speech_job,report_emote_playback,get_speech_audio,synthesize_speech,transcribe_audio,open_overlay,close_overlay,configure_overlay,import_avatar,get_avatar_info,import_motion,export_profile,import_profile,updater::get_update_settings,updater::set_update_settings,updater::check_update,updater::install_update,updater::cancel_update,screen_permission::get_screen_capture_permission,screen_permission::request_screen_capture_permission,screen_permission::open_screen_capture_settings,screen_portal::get_screen_session,screen_portal::select_screen_source,screen_portal::disconnect_screen_source];
    handler(invoke)
}
fn application_exit(app:&tauri::AppHandle,event:tauri::RunEvent){
    if matches!(event,tauri::RunEvent::ExitRequested{..}|tauri::RunEvent::Exit){app.state::<AppState>().life_runtime.shutdown(app);app.state::<AppState>().screen_context.portal.close_all();}
}

fn setup_tray(app:&mut tauri::App)->Result<(),Box<dyn std::error::Error>>{
    use tauri::tray::TrayIconBuilder;
    let menu=ui_locale::tray_menu(app.handle(),ui_locale::UiLocale::En)?;
    if let Some(window)=app.get_webview_window("main"){window.set_title("OrbitPal")?;}
    let mut tray=TrayIconBuilder::with_id("cohost").menu(&menu).tooltip("OrbitPal").show_menu_on_left_click(true);
    if let Some(icon)=app.default_window_icon(){tray=tray.icon(icon.clone());}
    let tray=tray.on_menu_event(|app,event|match event.id.as_ref(){
        "studio"=>{app.state::<AppState>().life_runtime.shown();if let Some(w)=app.get_webview_window("main"){let _=w.unminimize();let _=w.show();let _=w.set_focus();}},
        "character"=>{if let Err(e)=overlay::open(app){let _=app.emit("cohost://agent",json!({"kind":"error","text":e}));}},
        "stop"=>{let state=app.state::<AppState>();state.screen_context.portal.close_all();if let Ok(current)=state.cancel.lock(){if let Some(cancel)=current.as_ref(){cancel.store(true,Ordering::Relaxed);}}state.overlay.publish(app,json!({"kind":"stop"}));},
        "quit"=>app.exit(0),
        _=>(),
    }).build(app);
    // A desktop without a tray keeps normal close behavior rather than hiding all controls.
    if let Some(window)=app.get_webview_window("main"){
        let hide_to_tray=tray.is_ok();let hidden=window.clone();window.on_window_event(move|event|{if matches!(event,tauri::WindowEvent::Destroyed|tauri::WindowEvent::CloseRequested{..}|tauri::WindowEvent::Resized(_)|tauri::WindowEvent::Moved(_)|tauri::WindowEvent::ScaleFactorChanged{..}){hidden.state::<AppState>().life_runtime.output_window_lost("main");}if hide_to_tray{if let tauri::WindowEvent::CloseRequested {api,..}=event{api.prevent_close();hidden.state::<AppState>().life_runtime.hide_main(&hidden);}}});
    }
    Ok(())
}
