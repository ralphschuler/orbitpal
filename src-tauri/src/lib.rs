mod storage;
mod providers;
mod transcription;
mod cli_provider;
mod chat;
mod mcp;
mod agent;
mod overlay;
mod memory;
mod knowledge;

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
}

#[tauri::command]
fn get_app_state(state:State<'_,AppState>,app:tauri::AppHandle,bot_id:Option<String>)->Result<Value,String> {
    let id=bot_id.unwrap_or_else(||state.store.profile().ok().and_then(|p|p["id"].as_str().map(str::to_owned)).unwrap_or("nova".into()));
    let store=state.store.scoped(&id)?;
    Ok(json!({"profile":store.profile()?,"bots":bot_summaries(&state,&app)?,"documents":store.documents()?,"skills":store.skills()?,"knowledge":store.knowledge()?,"history":store.history()?,"secretIds":store.secret_ids()?,"overlayUrl":state.overlay.url_for_bot(&id),"overlayActive":overlay::is_open(&app,&id),"appDataDir":store.root.to_string_lossy()}))
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
    let profile=store.save_profile(profile)?;
    if let Some(servers)=old["mcpServers"].as_array(){for server in servers {if let Some(id)=server["id"].as_str(){
        let unchanged=profile["mcpServers"].as_array().and_then(|list|list.iter().find(|s|s["id"].as_str()==Some(id))).map(|s|mcp_transport_signature(s)==mcp_transport_signature(server)).unwrap_or(false);
        if !unchanged{state.mcp.disconnect(id).await;}
    }}}
    if old["output"]["mode"]!=profile["output"]["mode"]{state.overlay.publish(&app,json!({"kind":"stop","botId":profile["id"]}));}
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));
    Ok(profile)
}

fn mcp_transport_signature(server:&Value)->Value {
    json!({"id":server["id"],"enabled":server["enabled"],"transport":server["transport"],"command":server["command"],"args":server["args"],"env":server["env"],"cwd":server["cwd"],"url":server["url"],"apiKeyId":server["apiKeyId"],"secretId":server["secretId"],"tokenEnv":server["tokenEnv"]})
}
#[tauri::command]
fn save_document(state:State<'_,AppState>,name:String,content:String)->Result<(),String>{state.store.save_document(&name,&content)}
#[tauri::command]
fn save_skill(state:State<'_,AppState>,id:String,content:String)->Result<(),String>{state.store.save_skill(&id,&content)}
#[tauri::command]
fn delete_skill(state:State<'_,AppState>,id:String)->Result<(),String>{state.store.delete_skill(&id)}
#[tauri::command]
fn import_skill(state:State<'_,AppState>,source_path:String)->Result<Value,String>{state.store.import_skill(std::path::Path::new(&source_path))}
#[tauri::command]
fn trigger_emote(app:tauri::AppHandle,emote_id:String,bot_id:Option<String>)->Result<Value,String>{
    let state=app.state::<AppState>();let id=bot_id.unwrap_or(state.store.profile()?["id"].as_str().unwrap_or("nova").to_owned());let profile=state.store.scoped(&id)?.profile()?;
    overlay::trigger_emote_profile(&app,&profile,&emote_id,false)
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
async fn send_message(app:tauri::AppHandle,text:String,source:Option<String>,user:Option<String>,skill_ids:Option<Vec<String>>,bot_id:Option<String>)->Result<Value,String>{agent::send(app,text,source.unwrap_or("operator".into()),user.unwrap_or("Streamer".into()),skill_ids.unwrap_or_default(),bot_id).await}
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
#[tauri::command]
async fn mcp_connect(state:State<'_,AppState>,server_id:String)->Result<Vec<Value>,String>{
    let profile=state.store.profile()?;
    let config=profile["mcpServers"].as_array().and_then(|s|s.iter().find(|s|s["id"].as_str()==Some(&server_id))).cloned().ok_or("MCP-Server nicht gefunden")?;
    if config["enabled"].as_bool()!=Some(true){return Err("Bitte den MCP-Server zuerst aktivieren".into());}
    let key=state.store.key_for(&config)?;
    state.mcp.connect(&resolve_mcp_config(&state.store,config)?,key).await
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
    state.chat.connect(&platform,&profile,key,app).await
}
#[tauri::command]
async fn disconnect_platform(state:State<'_,AppState>,platform:String)->Result<(),String>{state.chat.disconnect(&platform).await}
#[tauri::command]
fn report_playback(app:tauri::AppHandle,id:String,bot_id:String,speaking:bool,error:Option<String>)->Result<Value,String>{overlay::report_playback(&app,&id,&bot_id,speaking,error.as_deref())}
#[tauri::command]
async fn get_speech_audio(app:tauri::AppHandle,id:String,bot_id:String)->Result<Value,String>{overlay::speech_audio(&app,&id,&bot_id).await}
#[tauri::command]
async fn synthesize_speech(state:State<'_,AppState>,text:String,bot_id:Option<String>)->Result<Value,String>{
    if text.len()>20_000{return Err("Sprachtext ist zu lang".into());}
    let scoped=bot_id.as_deref().map(|id|state.store.scoped(id)).transpose()?;let store=scoped.as_ref().unwrap_or(&state.store);
    let profile=store.profile()?;let config=&profile["voice"];providers::synthesize(config,&text,store.key_for(config)?).await
}
#[tauri::command]
async fn transcribe_audio(state:State<'_,AppState>,audio_base64:String,mime_type:String,bot_id:Option<String>)->Result<String,String>{
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
fn export_profile(state:State<'_,AppState>,destination:String)->Result<(),String>{state.store.export(std::path::Path::new(&destination))}
#[tauri::command]
async fn import_profile(state:State<'_,AppState>,app:tauri::AppHandle,source_path:String)->Result<Value,String>{
    let _idle=state.generation.try_lock().map_err(|_|"Bitte die aktuelle Antwort vor dem Import beenden".to_owned())?;
    let profile=state.store.import(std::path::Path::new(&source_path))?;
    state.overlay.publish(&app,json!({"kind":"profile","profile":overlay::public_profile(&profile)}));Ok(profile)
}

#[cfg_attr(mobile,tauri::mobile_entry_point)]
pub fn run(){
    tauri::Builder::default().plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_opener::init())
        .setup(|app|{
            let root=app.path().app_data_dir()?;
            let store=storage::Store::new(root).map_err(std::io::Error::other)?;
            app.manage(AppState {store,mcp:mcp::McpHub::default(),chat:chat::ChatHub::default(),cancel:Mutex::new(None),generation:tokio::sync::Mutex::new(()),overlay:overlay::OverlayHub::default()});
            tauri::async_runtime::block_on(overlay::start(app.handle().clone())).map_err(std::io::Error::other)?;
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_app_state,list_bots,create_bot,select_bot,save_profile,save_document,save_skill,delete_skill,import_skill,trigger_emote,import_knowledge,delete_knowledge,set_secret,delete_secret,list_models,check_cli_provider,list_cli_models,send_message,stop_generation,clear_memory,mcp_connect,mcp_disconnect,mcp_tools,connect_platform,disconnect_platform,report_playback,get_speech_audio,synthesize_speech,transcribe_audio,open_overlay,close_overlay,configure_overlay,import_avatar,export_profile,import_profile])
        .run(tauri::generate_context!()).expect("Streamer AI Cohost konnte nicht gestartet werden");
}

fn setup_tray(app:&mut tauri::App)->Result<(),Box<dyn std::error::Error>>{
    use tauri::{menu::{Menu,MenuItem},tray::TrayIconBuilder};
    let studio=MenuItem::with_id(app,"studio","Studio anzeigen",true,None::<&str>)?;
    let character=MenuItem::with_id(app,"character","Charakter anzeigen",true,None::<&str>)?;
    let stop=MenuItem::with_id(app,"stop","Sprechen abbrechen",true,None::<&str>)?;
    let quit=MenuItem::with_id(app,"quit","Streamer AI Cohost beenden",true,None::<&str>)?;
    let menu=Menu::with_items(app,&[&studio,&character,&stop,&quit])?;
    let mut tray=TrayIconBuilder::with_id("cohost").menu(&menu).tooltip("Streamer AI Cohost").show_menu_on_left_click(true);
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
