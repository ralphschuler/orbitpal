//! The studio may discover updates automatically. Package download, installation
//! and restart are a separate, explicitly confirmed operator action. Models, MCP
//! clients and avatar webviews never receive these commands or candidate IDs.
use serde::Serialize;
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
#[cfg(target_os = "linux")]
use tauri::Manager;
use tauri_plugin_updater::{Update, UpdaterExt};

const ENDPOINT: &str = "https://github.com/ralphschuler/streamer-ai-cohost/releases/latest/download/latest.json";
const REPOSITORY: &str = "/ralphschuler/streamer-ai-cohost";
const CANDIDATE_LIFETIME: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub struct UpdateState {
    operation: tokio::sync::Mutex<()>,
    candidate: Mutex<Option<Candidate<Update>>>,
    installing: AtomicBool,
}
impl UpdateState {
    pub fn ensure_idle(&self) -> Result<(), String> {
        if self.installing.load(Ordering::SeqCst) {
            Err("Ein App-Update wird installiert. Bitte den Neustart abwarten.".into())
        } else { Ok(()) }
    }
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
    let prefix = format!("{REPOSITORY}/releases/download/v{version}/");
    let name = url.path().strip_prefix(&prefix).unwrap_or_default();
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
    *state.updater.candidate.lock().map_err(|_| "Update-Status ist nicht verfügbar.")? = None;
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
        *state.updater.candidate.lock().map_err(|_| "Update-Status ist nicht verfügbar.")? = Some(Candidate { id, created: Instant::now(), update });
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

#[tauri::command]
pub async fn install_update(window: tauri::WebviewWindow, app: tauri::AppHandle, state: tauri::State<'_, crate::AppState>, candidate_id: String, confirmed: bool) -> Result<(), String> {
    main_only(window.label())?;
    if let Some(reason) = support(&app) { return Err(reason); }
    let _operation = state.updater.operation.try_lock().map_err(|_| "Eine Update-Prüfung oder Installation läuft bereits.".to_owned())?;
    // Hold the generation lock for the complete download/install. A new model
    // turn, bot switch or import cannot race the final idle check and restart.
    let _generation = state.generation.try_lock().map_err(|_| "Bitte die aktuelle Antwort vor dem Update beenden.".to_owned())?;
    if state.overlay.has_any_pending_speech() { return Err("Bitte die vorbereitete oder laufende Sprachausgabe vor dem Update beenden.".into()); }
    let update = {
        let mut slot = state.updater.candidate.lock().map_err(|_| "Update-Status ist nicht verfügbar.")?;
        approved_candidate(&slot, &candidate_id, confirmed, Instant::now())?;
        // Consumed on confirmation: errors require a fresh check and a new click.
        slot.take().ok_or("Bitte zuerst erneut nach Updates suchen.")?.update
    };
    trusted_asset(&update.download_url, &update.version)?;
    if !newer_stable(&app.package_info().version.to_string(), &update.version)? { return Err("Diese Version ist kein neueres stabiles Update.".into()); }
    state.updater.installing.store(true, Ordering::SeqCst);
    let _installing = Installing(&state.updater.installing);
    // Invalidate gestures/sound preparation already dispatched to native/OBS
    // renderers before confirmation. Their late decodes must not start audio
    // behind the update dialog while the native runtime is locked.
    state.overlay.publish(&app, serde_json::json!({"kind":"stop"}));
    progress(&window, &candidate_id, "downloading", 0, None, None);
    let mut downloaded = 0u64;
    let mut last_progress = Instant::now();
    let downloaded_bytes = update.download(|chunk, total| {
        downloaded = downloaded.saturating_add(chunk as u64);
        if last_progress.elapsed() >= Duration::from_millis(100) || total == Some(downloaded) {
            progress(&window, &candidate_id, "downloading", downloaded, total, None);
            last_progress = Instant::now();
        }
    }, || {}).await;
    let bytes = match downloaded_bytes {
        Ok(bytes) => bytes,
        Err(error) => {
            let message = format!("Update-Download oder Signaturprüfung fehlgeschlagen: {error}");
            progress(&window, &candidate_id, "error", downloaded, None, Some(&message));
            return Err(message);
        }
    };
    if state.overlay.has_any_pending_speech() {
        let message = "Die Ausgabe ist wieder aktiv. Das Update wurde nicht installiert; bitte erneut prüfen.";
        progress(&window, &candidate_id, "error", downloaded, None, Some(message));
        return Err(message.into());
    }
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
    fn install_guard_blocks_new_activity_and_recovers_after_failure() {
        let state = UpdateState::default();
        assert!(state.ensure_idle().is_ok());
        state.installing.store(true, Ordering::SeqCst);
        { let _guard = Installing(&state.installing); assert!(state.ensure_idle().is_err()); }
        assert!(state.ensure_idle().is_ok());
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
