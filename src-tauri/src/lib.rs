mod storage;
mod providers;
mod transcription;
mod cli_provider;
mod bounded_io;
mod chat;
mod chat_route;
mod mcp;
mod agent;
mod overlay;
mod overlay_http;
mod memory;
mod knowledge;
mod runtime_command;
mod speech_text;
mod extensions;
mod skill_library;
mod emote_audio;
mod vision;
mod control_mcp;
mod updater;
mod avatar_metadata;
mod avatar_audio;
mod gltf_import;
mod autopilot;
mod screen_capture;
mod screen_context;
mod microphone_permission;
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
    control_mcp:control_mcp::ControlMcp,
    updater:updater::UpdateState,
    autopilot:autopilot::AutopilotHub,
    screen_context:screen_context::ScreenState,
}

#[tauri::command]
fn get_app_state(state:State<'_,AppState>,app:tauri::AppHandle,bot_id:Option<String>)->Result<Value,String> {
    let id=bot_id.unwrap_or_else(||state.store.profile().ok().and_then(|p|p["id"].as_str().map(str::to_owned)).unwrap_or("nova".into()));
    let store=state.store.scoped(&id)?;
    Ok(json!({"profile":store.profile()?,"bots":bot_summaries(&state,&app)?,"documents":store.documents()?,"skills":store.skills()?,"library":store.extension_library()?,"knowledge":store.knowledge()?,"history":store.history()?,"secretIds":store.secret_ids()?,"overlayUrl":state.overlay.url_for_bot(&id),"overlayActive":overlay::is_open(&app,&id),"appDataDir":store.root.to_string_lossy(),"autopilot":state.autopilot.all()?}))
}
#[tauri::command]
fn get_extension_library(state:State<'_,AppState>)->Result<Value,String>{state.store.extension_library()}
#[tauri::command]
fn get_library_skill(state:State<'_,AppState>,id:String)->Result<Value,String>{skill_library::SkillLibrary::new(&state.store.root)?.read(&id)}
#[tauri::command]
fn install_control_extension(state:State<'_,AppState>,app:tauri::AppHandle,window:tauri::WebviewWindow)->Result<Value,String>{
    if window.label()!="main"{return Err("Control-Erweiterungen werden ausschließlich im Studio installiert".into());}
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Installieren des Control-MCP beenden".to_owned())?;
    let installed=state.store.install_control_extension(control_mcp::CONTROL_SKILL)?;
    let _=app.emit("cohost://library-changed",json!({"kind":"control"}));Ok(installed)
}
#[tauri::command]
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
#[tauri::command]
fn list_bots(state:State<'_,AppState>,app:tauri::AppHandle)->Result<Vec<Value>,String>{bot_summaries(&state,&app)}
#[tauri::command]
async fn create_bot(state:State<'_,AppState>,app:tauri::AppHandle,name:String,copy_current:bool)->Result<Value,String>{
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Anlegen eines Bots beenden".to_owned())?;
    let profile=state.store.create_bot(&name,copy_current)?;
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}
#[tauri::command]
async fn select_bot(state:State<'_,AppState>,app:tauri::AppHandle,id:String)->Result<Value,String>{
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Botwechsel beenden".to_owned())?;
    let profile=state.store.select_bot(&id)?;
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}
#[tauri::command]
async fn save_profile(state:State<'_,AppState>,app:tauri::AppHandle,profile:Value,bot_id:Option<String>)->Result<Value,String> {
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;
    let store=scoped.as_ref().unwrap_or(&state.store);
    let old=store.profile()?;
    let _profile_switch=if old["id"]!=profile["id"]{Some(state.generation.try_lock().map_err(|_|"Profilwechsel während einer laufenden Antwort nicht möglich. Bitte zuerst abbrechen.".to_owned())?)}else{None};
    // An identical save (also used before ordinary messages) does not disturb
    // autonomous sessions. Changed settings revoke any in-flight snapshot.
    let write=||state.screen_context.change_profile(old["id"].as_str().ok_or("Bot-ID fehlt")?,||store.profile(),&profile,||store.save_profile(profile.clone()));
    let (profile,runtime)=if old!=profile{state.autopilot.change_profile(old["id"].as_str().ok_or("Bot-ID fehlt")?,write)?}else{(write()?,None)};
    if old!=profile&&state.overlay.has_autopilot_speech(old["id"].as_str().ok_or("Bot-ID fehlt")?){state.overlay.publish(&app,json!({"kind":"stop","botId":old["id"]}));}
    if let Some(runtime)=runtime{autopilot::publish(&app,runtime);}
    // Bindings are per bot; revoking one bot must not disconnect another bot's
    // shared transport. Runtime calls recheck the answered bot's current grants.
    if old["output"]["mode"]!=profile["output"]["mode"]{state.overlay.publish(&app,json!({"kind":"stop","botId":profile["id"]}));}
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));
    Ok(profile)
}

#[tauri::command]
fn save_document(state:State<'_,AppState>,app:tauri::AppHandle,name:String,content:String,bot_id:Option<String>)->Result<(),String>{
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);let profile=store.profile()?;let bot_id=profile["id"].as_str().ok_or("Bot-ID fehlt")?;
    let (_,runtime)=state.autopilot.change_profile(bot_id,||{store.save_document(&name,&content)?;store.profile()})?;
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
async fn list_models(state:State<'_,AppState>,config:Value)->Result<Vec<String>,String>{let key=state.store.key_for(&config)?;providers::models(&config,key).await}
#[tauri::command]
async fn check_cli_provider(config:Value)->Result<Value,String>{cli_provider::status(&config).await}
#[tauri::command]
async fn list_cli_models(config:Value)->Result<Value,String>{cli_provider::models(&config).await}
#[tauri::command]
async fn send_message(app:tauri::AppHandle,text:String,source:Option<String>,user:Option<String>,skill_ids:Option<Vec<String>>,bot_id:Option<String>,request_id:Option<String>,reply_route_id:Option<String>)->Result<Value,String>{agent::send(app,text,source.unwrap_or("operator".into()),user.unwrap_or("Streamer".into()),skill_ids.unwrap_or_default(),bot_id,request_id,reply_route_id).await}
#[tauri::command]
fn stop_generation(state:State<'_,AppState>,app:tauri::AppHandle)->Result<(),String>{
    if let Some(c)=state.cancel.lock().map_err(|e|e.to_string())?.as_ref(){c.store(true,Ordering::Relaxed);}
    state.overlay.publish(&app,json!({"kind":"stop"}));Ok(())
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
    let key=state.store.key_for(config)?;
    let write_key=if platform=="youtube" && config["sendReplies"]==true {state.store.secret(config["sendSecretId"].as_str().unwrap_or("youtube-write"))?} else {None};
    state.chat.connect(&platform,&profile,key,write_key,app).await
}
#[tauri::command]
async fn disconnect_platform(state:State<'_,AppState>,platform:String)->Result<(),String>{state.chat.disconnect(&platform).await}
#[tauri::command]
fn report_playback(app:tauri::AppHandle,id:String,bot_id:String,speaking:bool,error:Option<String>)->Result<Value,String>{overlay::report_playback(&app,&id,&bot_id,speaking,error.as_deref())}
#[tauri::command]
fn cancel_speech_job(app:tauri::AppHandle,window:tauri::WebviewWindow,id:String,bot_id:String)->Result<Value,String>{
    if window.label()!="main"{return Err("Ausstehende Wiedergaben werden ausschließlich im Studio abgebrochen".into());}
    overlay::cancel_playback(&app,&id,&bot_id,Some("Keine Abschlussmeldung der Sprachausgabe. Automatische Antworten pausiert."))
}
#[tauri::command]
fn report_emote_playback(app:tauri::AppHandle,bot_id:String,output:String,id:String,playing:bool)->Result<Value,String>{overlay::report_emote_playback(&app,&bot_id,&output,&id,playing)}
#[tauri::command]
async fn get_speech_audio(app:tauri::AppHandle,id:String,bot_id:String)->Result<Value,String>{overlay::speech_audio(&app,&id,&bot_id).await}
#[tauri::command]
async fn synthesize_speech(state:State<'_,AppState>,text:String,bot_id:Option<String>)->Result<Value,String>{
    state.updater.ensure_idle()?;
    if text.len()>20_000{return Err("Sprachtext ist zu lang".into());}
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);
    let profile=store.profile()?;let config=&profile["voice"];providers::synthesize(config,&text,store.key_for(config)?).await
}
#[tauri::command]
async fn transcribe_audio(state:State<'_,AppState>,audio_base64:String,mime_type:String,bot_id:Option<String>)->Result<String,String>{
    state.updater.ensure_idle()?;
    if audio_base64.len()>40*1024*1024{return Err("Audioaufnahme ist zu groß".into());}
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);
    let profile=store.profile()?;let config=&profile["stt"];providers::transcribe(config,&audio_base64,&mime_type,store.key_for(config)?).await
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
fn import_avatar(state:State<'_,AppState>,source_path:String)->Result<String,String>{state.store.import_avatar(std::path::Path::new(&source_path))}
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
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Import beenden".to_owned())?;
    let profile=state.store.import(std::path::Path::new(&source_path))?;
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}

#[cfg_attr(mobile,tauri::mobile_entry_point)]
pub fn run(){
    tauri::Builder::default().plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_opener::init()).plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app|{
            app.manage(ui_locale::UiLocaleState::default());
            let root=app.path().app_data_dir()?;
            let store=storage::Store::new(root).map_err(std::io::Error::other)?;
            app.manage(AppState {store,mcp:mcp::McpHub::default(),chat:chat::ChatHub::default(),cancel:Mutex::new(None),generation:tokio::sync::Mutex::new(()),overlay:overlay::OverlayHub::default(),control_mcp:control_mcp::ControlMcp::default(),updater:updater::UpdateState::default(),autopilot:autopilot::AutopilotHub::default(),screen_context:screen_context::ScreenState::default()});
            tauri::async_runtime::block_on(overlay::start(app.handle().clone())).map_err(std::io::Error::other)?;
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ui_locale::get_ui_locale,ui_locale::set_ui_locale,microphone_permission::get_microphone_permission,microphone_permission::request_microphone_permission,microphone_permission::open_microphone_settings,get_app_state,get_extension_library,get_library_skill,install_control_extension,get_control_mcp_config,save_library_mcp,delete_library_mcp,list_bots,create_bot,select_bot,save_profile,save_document,save_skill,delete_skill,import_skill,trigger_emote,import_emote_sound,get_emote_sound,import_knowledge,delete_knowledge,set_secret,delete_secret,list_models,check_cli_provider,list_cli_models,send_message,stop_generation,autopilot::set_autopilot,autopilot::send_autopilot,screen_context::list_capture_displays,clear_memory,mcp_connect,mcp_disconnect,mcp_tools,connect_platform,disconnect_platform,report_playback,cancel_speech_job,report_emote_playback,get_speech_audio,synthesize_speech,transcribe_audio,open_overlay,close_overlay,configure_overlay,import_avatar,get_avatar_info,export_profile,import_profile,updater::get_update_settings,updater::set_update_settings,updater::check_update,updater::install_update])
        .run(tauri::generate_context!()).expect("OrbitPal konnte nicht gestartet werden");
}

fn setup_tray(app:&mut tauri::App)->Result<(),Box<dyn std::error::Error>>{
    use tauri::tray::TrayIconBuilder;
    let menu=ui_locale::tray_menu(app.handle(),ui_locale::UiLocale::En)?;
    if let Some(window)=app.get_webview_window("main"){window.set_title("OrbitPal")?;}
    let mut tray=TrayIconBuilder::with_id("cohost").menu(&menu).tooltip("OrbitPal").show_menu_on_left_click(true);
    if let Some(icon)=app.default_window_icon(){tray=tray.icon(icon.clone());}
    let tray=tray.on_menu_event(|app,event|match event.id.as_ref(){
        "studio"=>{if let Some(w)=app.get_webview_window("main"){let _=w.unminimize();let _=w.show();let _=w.set_focus();}},
        "character"=>{if let Err(e)=overlay::open(app){let _=app.emit("cohost://agent",json!({"kind":"error","text":e}));}},
        "stop"=>{let state=app.state::<AppState>();if let Ok(current)=state.cancel.lock(){if let Some(cancel)=current.as_ref(){cancel.store(true,Ordering::Relaxed);}}state.overlay.publish(app,json!({"kind":"stop"}));},
        "quit"=>app.exit(0),
        _=>(),
    }).build(app);
    // A desktop without a tray keeps normal close behavior rather than hiding all controls.
    if tray.is_ok(){if let Some(window)=app.get_webview_window("main"){
        let hidden=window.clone();window.on_window_event(move|event|{if let tauri::WindowEvent::CloseRequested {api,..}=event{api.prevent_close();let _=hidden.hide();}});
    }}
    Ok(())
}
