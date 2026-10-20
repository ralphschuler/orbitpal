//! The studio may discover updates automatically. Package download, installation
//! and restart are a separate, explicitly confirmed operator action. Models, MCP
//! clients and avatar webviews never receive these commands or candidate IDs.
use serde::Serialize;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
#[cfg(target_os = "linux")]
use tauri::Manager;
use tauri_plugin_updater::{Update, UpdaterExt};

const ENDPOINT: &str = "https://github.com/ralphschuler/orbitpal/releases/latest/download/latest.json";
const REPOSITORY: &str = "/ralphschuler/orbitpal";
// Only this same-owner former slug is allowed for installed 0.6/0.7 clients.
const LEGACY_REPOSITORY: &str = "/ralphschuler/streamer-ai-cohost";
const CANDIDATE_LIFETIME: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub struct UpdateState {
    operation: tokio::sync::Mutex<()>,
    lifecycle: Mutex<UpdateLifecycle<Update>>,
    installing: AtomicBool,
}
impl UpdateState {
    pub fn ensure_idle(&self) -> Result<(), String> {
        if self.installing.load(Ordering::SeqCst) {
            Err("Ein App-Update wird installiert. Bitte den Neustart abwarten.".into())
        } else { Ok(()) }
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum DownloadPhase { Downloading, Cancelled, Installing }
struct DownloadControl { id:String, phase:Mutex<DownloadPhase>, cancelled:tokio::sync::Notify }
impl DownloadControl {
    fn new(id:&str)->Self{Self{id:id.into(),phase:Mutex::new(DownloadPhase::Downloading),cancelled:tokio::sync::Notify::new()}}
    fn cancel(&self)->Result<bool,String>{
        let mut phase=self.phase.lock().map_err(|_|"Update-Status ist nicht verfügbar.")?;
        if *phase!=DownloadPhase::Downloading{return Ok(*phase==DownloadPhase::Cancelled);}
        *phase=DownloadPhase::Cancelled;self.cancelled.notify_one();Ok(true)
    }
    fn begin_install(&self)->Result<bool,String>{
        let mut phase=self.phase.lock().map_err(|_|"Update-Status ist nicht verfügbar.")?;
        if *phase==DownloadPhase::Cancelled{return Ok(false);}
        if *phase!=DownloadPhase::Downloading{return Err("Die Update-Installation läuft bereits.".into());}
        *phase=DownloadPhase::Installing;Ok(true)
    }
    fn while_downloading(&self,publish:impl FnOnce()){
        if let Ok(phase)=self.phase.lock(){if *phase==DownloadPhase::Downloading{publish();}}
    }
    async fn wait_cancelled(&self){
        loop{if self.phase.lock().map(|phase|*phase==DownloadPhase::Cancelled).unwrap_or(true){return;}self.cancelled.notified().await;}
    }
}
struct UpdateLifecycle<T>{candidate:Option<Candidate<T>>,active:Option<Arc<DownloadControl>>,cancelled_id:Option<String>}
impl<T> Default for UpdateLifecycle<T>{fn default()->Self{Self{candidate:None,active:None,cancelled_id:None}}}
impl<T> UpdateLifecycle<T>{
    fn start(&mut self,id:&str,confirmed:bool,now:Instant)->Result<Option<(T,Arc<DownloadControl>)>,String>{
        if !confirmed{return Err("Download und Installation benötigen deine ausdrückliche Bestätigung.".into());}
        if self.cancelled_id.as_deref()==Some(id){return Ok(None);}
        approved_candidate(&self.candidate,id,confirmed,now)?;
        let update=self.candidate.take().ok_or("Bitte zuerst erneut nach Updates suchen.")?.update;
        let control=Arc::new(DownloadControl::new(id));self.active=Some(control.clone());Ok(Some((update,control)))
    }
    fn cancel(&mut self,id:&str)->Result<bool,String>{
        if let Some(active)=&self.active{if active.id==id{let cancelled=active.cancel()?;if cancelled{self.cancelled_id=Some(id.to_owned());}return Ok(cancelled);}return Ok(false);}
        if self.candidate.as_ref().is_some_and(|candidate|candidate.id==id){self.candidate=None;self.cancelled_id=Some(id.to_owned());return Ok(true);}
        Ok(self.cancelled_id.as_deref()==Some(id))
    }
}
struct ActiveDownload<'a>{state:&'a UpdateState,control:Arc<DownloadControl>}
impl Drop for ActiveDownload<'_>{fn drop(&mut self){if let Ok(mut lifecycle)=self.state.lifecycle.lock(){if lifecycle.active.as_ref().is_some_and(|active|Arc::ptr_eq(active,&self.control)){lifecycle.active=None;}}}}
#[derive(Serialize)]
pub struct InstallOutcome { cancelled:bool }
async fn download_until_cancelled<T>(control:&DownloadControl,download:impl std::future::Future<Output=Result<T,String>>)->Result<Option<T>,String>{
    // Dropping the plugin download future drops its request/stream and buffers.
    // Cancellation wins when both futures are already ready. A second atomic
    // phase check below also covers cancellation during synchronous verification.
    tokio::select!{biased;_=control.wait_cancelled()=>Ok(None),result=download=>result.map(Some)}
}

struct Candidate<T> { id: String, created: Instant, update: T }
struct Installing<'a>(&'a AtomicBool);
impl Drop for Installing<'_> {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings { automatic_checks: bool }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    id: String,
    current_version: String,
    version: String,
    notes: String,
    date: Option<String>,
    release_url: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    current_version: String,
    update: Option<AvailableUpdate>,
    install_supported: bool,
    install_unavailable_reason: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    candidate_id: &'a str,
    phase: &'a str,
    downloaded: u64,
    total: Option<u64>,
    message: Option<&'a str>,
}

fn main_only(label: &str) -> Result<(), String> {
    if label != "main" { return Err("App-Updates sind ausschließlich für den Streamer im Studio verfügbar.".into()); }
    Ok(())
}

#[tauri::command]
pub fn get_update_settings(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>) -> Result<UpdateSettings, String> {
    main_only(window.label())?;
    Ok(UpdateSettings { automatic_checks: state.store.automatic_update_checks()? })
}
#[tauri::command]
pub fn set_update_settings(window: tauri::WebviewWindow, state: tauri::State<'_, crate::AppState>, automatic_checks: bool) -> Result<UpdateSettings, String> {
    main_only(window.label())?;
    state.store.save_automatic_update_checks(automatic_checks)?;
    Ok(UpdateSettings { automatic_checks })
}

fn newer_stable(current: &str, announced: &str) -> Result<bool, String> {
    let current = semver::Version::parse(current).map_err(|_| "Die installierte App-Version ist ungültig.".to_owned())?;
    let announced = semver::Version::parse(announced).map_err(|_| "Der Update-Katalog enthält eine ungültige Version.".to_owned())?;
    Ok(announced.cmp_precedence(&current).is_gt() && announced.pre.is_empty())
}

// A signature authenticates package contents. Pin the metadata's initial asset
// URL as well, so a release cannot send the downloader to an arbitrary host or
// pair a newer announcement with an asset from an older release tag.
fn trusted_asset(url: &url::Url, version: &str) -> Result<(), String> {
    let name = [REPOSITORY, LEGACY_REPOSITORY].iter().find_map(|repository| {
        let prefix = format!("{repository}/releases/download/v{version}/");
        url.path().strip_prefix(&prefix)
    }).unwrap_or_default();
    if url.scheme() != "https" || url.host_str() != Some("github.com") || !url.username().is_empty() || url.password().is_some() || url.port().is_some() || url.query().is_some() || url.fragment().is_some() || name.is_empty() || name.contains('/') || !name.is_ascii() || name.contains('%') {
        return Err("Der Update-Katalog verweist nicht auf ein freigegebenes GitHub-Release-Paket.".into());
    }
    if !name.contains(version) || ![".app.tar.gz", ".AppImage", ".exe", ".msi"].iter().any(|suffix| name.ends_with(suffix)) {
        return Err("Das Update-Paket hat keinen gültigen Versionsnamen oder Pakettyp.".into());
    }
    Ok(())
}

fn support(app: &tauri::AppHandle) -> Option<String> {
    if cfg!(debug_assertions) {
        return Some("Die Entwicklungsversion installiert keine Updates. Bitte eine veröffentlichte App verwenden.".into());
    }
    #[cfg(target_os = "linux")]
    if app.env().appimage.is_none() {
        return Some("Automatische Installation ist unter Linux für AppImages verfügbar. Bitte dieses Paket über GitHub Releases aktualisieren.".into());
    }
    let _ = app;
    None
}

#[tauri::command]
pub async fn check_update(window: tauri::WebviewWindow, app: tauri::AppHandle, state: tauri::State<'_, crate::AppState>) -> Result<UpdateCheck, String> {
    main_only(window.label())?;
    let _operation = state.updater.operation.try_lock().map_err(|_| "Eine Update-Prüfung oder Installation läuft bereits.".to_owned())?;
    state.updater.ensure_idle()?;
    // Failed/replaced checks must invalidate the previously shown confirmation.
    {let mut lifecycle=state.updater.lifecycle.lock().map_err(|_| "Update-Status ist nicht verfügbar.")?;lifecycle.candidate=None;lifecycle.cancelled_id=None;}
    let unavailable = support(&app);
    let mut result = UpdateCheck {
        current_version: app.package_info().version.to_string(), update: None,
        install_supported: unavailable.is_none(), install_unavailable_reason: unavailable,
    };
    let updater = app.updater_builder()
        .endpoints(vec![ENDPOINT.parse().map_err(|_| "Die Update-Adresse ist ungültig.")?])
        .map_err(|_| "Die Update-Adresse ist nicht verfügbar.")?
        .timeout(Duration::from_secs(20))
        // Keep stable releases only, regardless of future configuration changes.
        .version_comparator(|current, release| release.version.cmp_precedence(&current).is_gt() && release.version.pre.is_empty())
        .build().map_err(|error| format!("Update-Prüfung konnte nicht gestartet werden: {error}"))?;
    let checked = updater.check().await.map_err(check_error)?;
    if let Some(mut update) = checked {
        if !newer_stable(&result.current_version, &update.version)? { return Ok(result); }
        trusted_asset(&update.download_url, &update.version)?;
        if update.signature.is_empty() { return Err("Das Update-Paket hat keine Signatur.".into()); }
        let id = uuid::Uuid::new_v4().to_string();
        result.update = Some(AvailableUpdate {
            id: id.clone(), current_version: result.current_version.clone(), version: update.version.clone(),
            // Rendered as plain text; release notes cannot become HTML or commands.
            notes: update.body.as_deref().unwrap_or_default().chars().take(16_000).collect(),
            date: update.raw_json["pub_date"].as_str().map(str::to_owned),
            release_url: format!("https://github.com{REPOSITORY}/releases/tag/v{}", update.version),
        });
        // Downloads can take longer than a small manifest request.
        update.timeout = Some(Duration::from_secs(10 * 60));
        state.updater.lifecycle.lock().map_err(|_| "Update-Status ist nicht verfügbar.")?.candidate = Some(Candidate { id, created: Instant::now(), update });
    }
    Ok(result)
}

fn check_error(error: tauri_plugin_updater::Error) -> String {
    match error {
        tauri_plugin_updater::Error::ReleaseNotFound => "Der GitHub-Update-Katalog ist noch nicht verfügbar oder wurde nicht erfolgreich geladen. Bitte später erneut prüfen.".into(),
        tauri_plugin_updater::Error::TargetNotFound(_) | tauri_plugin_updater::Error::TargetsNotFound(_) => "Für dieses Betriebssystem und diese Architektur fehlt ein Update-Paket im Release.".into(),
        error => format!("GitHub-Update-Prüfung fehlgeschlagen: {error}"),
    }
}

fn approved_candidate<T>(slot: &Option<Candidate<T>>, id: &str, confirmed: bool, now: Instant) -> Result<(), String> {
    if !confirmed { return Err("Download und Installation benötigen deine ausdrückliche Bestätigung.".into()); }
    let candidate = slot.as_ref().ok_or("Bitte zuerst erneut nach Updates suchen.")?;
    if candidate.id != id || now.saturating_duration_since(candidate.created) >= CANDIDATE_LIFETIME {
        return Err("Die Update-Bestätigung ist nicht mehr aktuell. Bitte erneut nach Updates suchen.".into());
    }
    Ok(())
}
fn progress(window: &tauri::WebviewWindow, id: &str, phase: &str, downloaded: u64, total: Option<u64>, message: Option<&str>) {
    let _ = window.emit("cohost://update", Progress { candidate_id: id, phase, downloaded, total, message });
}

fn cancel_download<T>(label:&str,lifecycle:&Mutex<UpdateLifecycle<T>>,id:&str)->Result<bool,String>{
    main_only(label)?;lifecycle.lock().map_err(|_|"Update-Status ist nicht verfügbar.")?.cancel(id)
}

#[tauri::command]
pub fn cancel_update(window:tauri::WebviewWindow,state:tauri::State<'_,crate::AppState>,candidate_id:String)->Result<InstallOutcome,String>{
    let cancelled=cancel_download(window.label(),&state.updater.lifecycle,&candidate_id)?;
    Ok(InstallOutcome{cancelled})
}

#[tauri::command]
pub async fn install_update(window: tauri::WebviewWindow, app: tauri::AppHandle, state: tauri::State<'_, crate::AppState>, candidate_id: String, confirmed: bool) -> Result<InstallOutcome, String> {
    main_only(window.label())?;
    if let Some(reason) = support(&app) { return Err(reason); }
    let _operation = state.updater.operation.try_lock().map_err(|_| "Eine Update-Prüfung oder Installation läuft bereits.".to_owned())?;
    // Hold the generation lock for the complete download/install. A new model
    // turn, bot switch or import cannot race the final idle check and restart.
    let _generation = state.generation.try_lock().map_err(|_| "Bitte die aktuelle Antwort vor dem Update beenden.".to_owned())?;
    if state.autopilot.any_active() || state.life_runtime.any_active(){return Err("Bitte alle Autopiloten vor dem Update stoppen.".into());}
    if state.overlay.has_any_pending_speech() { return Err("Bitte die vorbereitete oder laufende Sprachausgabe vor dem Update beenden.".into()); }
    let (update,control) = {
        let mut lifecycle=state.updater.lifecycle.lock().map_err(|_| "Update-Status ist nicht verfügbar.")?;
        // Candidate consumption and publication of its cancellation generation
        // share one lock. An immediate Cancel cannot fall into a registration gap.
        let Some(value)=lifecycle.start(&candidate_id,confirmed,Instant::now())? else{return Ok(InstallOutcome{cancelled:true});};
        state.updater.installing.store(true,Ordering::SeqCst);value
    };
    let _installing = Installing(&state.updater.installing);
    let _active=ActiveDownload{state:&state.updater,control:control.clone()};
    trusted_asset(&update.download_url, &update.version)?;
    if !newer_stable(&app.package_info().version.to_string(), &update.version)? { return Err("Diese Version ist kein neueres stabiles Update.".into()); }
    // Invalidate gestures/sound preparation already dispatched to native/OBS
    // renderers before confirmation. Their late decodes must not start audio
    // behind the update dialog while the native runtime is locked.
    state.overlay.publish(&app, serde_json::json!({"kind":"stop"}));
    control.while_downloading(||progress(&window, &candidate_id, "downloading", 0, None, None));
    let mut downloaded = 0u64;
    let mut last_progress = Instant::now();
    let downloaded_bytes = download_until_cancelled(&control,async{update.download(|chunk, total| {
        downloaded = downloaded.saturating_add(chunk as u64);
        if last_progress.elapsed() >= Duration::from_millis(100) || total == Some(downloaded) {
            control.while_downloading(||progress(&window, &candidate_id, "downloading", downloaded, total, None));
            last_progress = Instant::now();
        }
    }, || {}).await.map_err(|error|format!("Update-Download oder Signaturprüfung fehlgeschlagen: {error}"))}).await;
    let bytes = match downloaded_bytes {
        Ok(Some(bytes)) => bytes,
        Ok(None)=>return Ok(InstallOutcome{cancelled:true}),
        Err(error) => {
            let message = error;
            progress(&window, &candidate_id, "error", downloaded, None, Some(&message));
            return Err(message);
        }
    };
    if state.overlay.has_any_pending_speech() || (state.autopilot.any_active() || state.life_runtime.any_active()) {
        let message = "Die Ausgabe ist wieder aktiv. Das Update wurde nicht installiert; bitte erneut prüfen.";
        progress(&window, &candidate_id, "error", downloaded, None, Some(message));
        return Err(message.into());
    }
    // This transition and Cancel contend for the same mutex. Once Installing
    // wins, replacement is deliberately non-cancellable on every platform.
    if !control.begin_install()?{return Ok(InstallOutcome{cancelled:true});}
    progress(&window, &candidate_id, "installing", downloaded, Some(downloaded), None);
    // The plugin verifies the signature again inside install(). On Windows it
    // exits only after launching the installer successfully (which relaunches).
    // On macOS/Linux install replaces the bundle, then we restart below.
    let installed = tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await;
    if let Err(message) = installed.map_err(|_| "Die Update-Installation wurde unerwartet beendet.".to_owned()).and_then(|value| value.map_err(|error| format!("Update konnte nicht installiert werden: {error}"))) {
        progress(&window, &candidate_id, "error", downloaded, None, Some(&message));
        return Err(message);
    }
    progress(&window, &candidate_id, "restarting", downloaded, Some(downloaded), None);
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatars_and_secondary_windows_cannot_check_or_confirm() {
        assert!(main_only("main").is_ok());
        for label in ["avatar", "avatar-nova", "main-child", "", "mcp"] { assert!(main_only(label).is_err()); }
    }
    #[test]
    fn stable_updates_reject_rollbacks_equal_versions_and_prereleases() {
        assert!(newer_stable("0.5.0", "0.6.0").unwrap());
        for version in ["0.5.0", "0.4.9", "0.6.0-beta.1", "0.5.0+rebuilt"] { assert!(!newer_stable("0.5.0", version).unwrap(), "{version}"); }
        assert!(newer_stable("0.5.0", "nightly").is_err());
    }
    #[test]
    fn declined_missing_stale_or_replaced_confirmation_never_consumes_package() {
        let now = Instant::now();
        let candidate = Some(Candidate { id: "confirmed-check".into(), created: now, update: "not downloaded" });
        assert!(approved_candidate(&candidate, "confirmed-check", false, now).is_err());
        assert!(approved_candidate::<()>(&None, "confirmed-check", true, now).is_err());
        assert!(approved_candidate(&candidate, "different-check", true, now).is_err());
        assert!(approved_candidate(&candidate, "confirmed-check", true, now + CANDIDATE_LIFETIME).is_err());
        assert!(approved_candidate(&candidate, "confirmed-check", true, now + Duration::from_secs(60)).is_ok());
        assert_eq!(candidate.unwrap().update, "not downloaded");
    }
    #[test]
    fn manifest_cannot_download_other_hosts_repositories_tags_or_package_types() {
        let base = "https://github.com/ralphschuler/streamer-ai-cohost/releases/download/v0.6.0/Streamer-AI-Cohost-v0.6.0";
        for suffix in [".app.tar.gz", ".AppImage", "-setup.exe", ".msi"] { assert!(trusted_asset(&format!("{base}{suffix}").parse().unwrap(), "0.6.0").is_ok()); }
        for address in [
            format!("{base}.exe?redirect=1"), format!("{base}.exe#fragment"), format!("{base}.sh"),
            format!("{base}.exe").replace("https://", "http://"),
            format!("{base}.exe").replace("github.com", "github.com.evil.invalid"),
            format!("{base}.exe").replace("ralphschuler", "attacker"),
            format!("{base}.exe").replace("download/v0.6.0/", "download/v0.4.0/"),
            format!("{base}.exe").replace("github.com/", "user@github.com/"),
            format!("{base}/nested.exe"), format!("{base}%2f.exe"),
        ] { assert!(trusted_asset(&address.parse().unwrap(), "0.6.0").is_err(), "{address}"); }
    }
    #[test]
    fn installer_rename_accepts_only_current_and_exact_legacy_repository_paths() {
        assert_eq!(ENDPOINT, "https://github.com/ralphschuler/orbitpal/releases/latest/download/latest.json");
        for repository in [REPOSITORY, LEGACY_REPOSITORY] {
            for suffix in [".app.tar.gz", ".AppImage", "-setup.exe", ".msi"] {
                let address = format!("https://github.com{repository}/releases/download/v0.8.0/OrbitPal-v0.8.0-windows-x64{suffix}");
                assert!(trusted_asset(&address.parse().unwrap(), "0.8.0").is_ok(), "{address}");
            }
        }
        for repository in ["/attacker/orbitpal", "/attacker/streamer-ai-cohost", "/ralphschuler/orbitpal-other", "/ralphschuler/other", "/ralphschuler/orbitpal/extra", "/ralphschuler/streamer-ai-cohost-other"] {
            let address = format!("https://github.com{repository}/releases/download/v0.8.0/OrbitPal-v0.8.0-linux-x64.AppImage");
            assert!(trusted_asset(&address.parse().unwrap(), "0.8.0").is_err(), "{address}");
        }
    }
    #[test]
    fn install_guard_blocks_new_activity_and_recovers_after_failure() {
        let state = UpdateState::default();
        assert!(state.ensure_idle().is_ok());
        state.installing.store(true, Ordering::SeqCst);
        { let _guard = Installing(&state.installing); assert!(state.ensure_idle().is_err()); }
        assert!(state.ensure_idle().is_ok());
    }
    #[tokio::test]
    async fn cancelled_slow_download_drops_its_future_and_late_bytes_never_install(){
        struct Dropped(Arc<AtomicBool>);impl Drop for Dropped{fn drop(&mut self){self.0.store(true,Ordering::SeqCst);}}
        let control=Arc::new(DownloadControl::new("slow-candidate"));let dropped=Arc::new(AtomicBool::new(false));
        let (sender,receiver)=tokio::sync::oneshot::channel::<Vec<u8>>();let guard=Dropped(dropped.clone());let downloading=control.clone();
        let task=tokio::spawn(async move{download_until_cancelled(&downloading,async move{let _guard=guard;let _=receiver.await;Ok(vec![1,2,3])}).await});
        tokio::task::yield_now().await;assert!(control.cancel().unwrap());assert!(task.await.unwrap().unwrap().is_none());
        assert!(dropped.load(Ordering::SeqCst));assert!(sender.send(vec![9;100]).is_err());assert!(!control.begin_install().unwrap());
        let progress=AtomicBool::new(false);control.while_downloading(||progress.store(true,Ordering::SeqCst));assert!(!progress.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn cancellation_after_verification_rejects_ready_bytes_before_install(){
        let control=DownloadControl::new("verified");let bytes=download_until_cancelled(&control,async{Ok(vec![1,2,3])}).await.unwrap().unwrap();
        assert_eq!(bytes.len(),3);assert!(control.cancel().unwrap());assert!(!control.begin_install().unwrap());
        let control=DownloadControl::new("simultaneously-ready");assert!(control.cancel().unwrap());
        assert!(download_until_cancelled(&control,async{Ok(vec![1])}).await.unwrap().is_none());
    }
    #[test]
    fn cancel_and_install_have_one_atomic_winner_and_installation_cannot_be_cancelled(){
        for _ in 0..64{
            let control=Arc::new(DownloadControl::new("race"));let barrier=Arc::new(std::sync::Barrier::new(2));
            let cancelled=control.clone();let ready=barrier.clone();let task=std::thread::spawn(move||{ready.wait();cancelled.cancel().unwrap()});
            barrier.wait();let installing=control.begin_install().unwrap();let cancelled=task.join().unwrap();assert_ne!(installing,cancelled);
            if installing{assert!(!control.cancel().unwrap());}else{assert!(!control.begin_install().unwrap());}
        }
    }
    #[test]
    fn cancellation_is_main_only_candidate_bound_and_requires_a_fresh_check(){
        let now=Instant::now();let lifecycle=Mutex::new(UpdateLifecycle{candidate:Some(Candidate{id:"old".into(),created:now,update:()}),..UpdateLifecycle::default()});
        for label in ["avatar","avatar-nova","mcp",""]{assert!(cancel_download(label,&lifecycle,"old").is_err());}
        assert!(lifecycle.lock().unwrap().candidate.is_some());assert!(!cancel_download("main",&lifecycle,"other").unwrap());
        assert!(cancel_download("main",&lifecycle,"old").unwrap());assert!(lifecycle.lock().unwrap().start("old",true,now).unwrap().is_none());
        {let mut state=lifecycle.lock().unwrap();state.cancelled_id=None;assert!(state.start("old",true,now).is_err());state.candidate=Some(Candidate{id:"new".into(),created:now,update:()});assert!(state.start("new",false,now).is_err());}
        let control=lifecycle.lock().unwrap().start("new",true,now).unwrap().unwrap().1;
        assert!(!cancel_download("main",&lifecycle,"old").unwrap());assert!(control.begin_install().unwrap());assert!(!cancel_download("main",&lifecycle,"new").unwrap());
    }
    #[test]
    fn automatic_checks_are_app_wide_persistent_and_absent_from_bot_exports() {
        let path = std::env::temp_dir().join(format!("cohost-updater-{}", uuid::Uuid::new_v4()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
        let _cleanup = Cleanup(path.clone());
        let store = crate::storage::Store::new(path.clone()).unwrap();
        assert!(store.automatic_update_checks().unwrap());
        store.save_automatic_update_checks(false).unwrap();
        let second = store.create_bot("Update fixture", false).unwrap();
        assert!(!store.scoped(second["id"].as_str().unwrap()).unwrap().automatic_update_checks().unwrap());
        drop(store);
        let store = crate::storage::Store::new(path.clone()).unwrap();
        assert!(!store.automatic_update_checks().unwrap());
        let destination = path.join("fixture-profile.json");
        store.export(&destination).unwrap();
        assert!(!std::fs::read_to_string(destination).unwrap().contains("automatic_update_checks"));
    }
}
