//! Actual native document callbacks and bounded ephemeral bootstrap. This
//! evaluates only a retained constructor; no capability is persisted or logged.
use super::{
    native_document::{AssetProbeGrant, AssetProbeState, DocumentGrant},
    native_scene_lifecycle::SceneAttempt,
};
use crate::window_signals::WindowSignals;
use std::{
    sync::Arc,
    time::Duration,
};
use tauri::Manager;

const ERROR: &str = super::native_scene::ERROR;
const BOOTSTRAP_MS: u64 = 5_000;

#[derive(Clone)]
pub(crate) struct NavigationPolicy {
    expected: url::Url,
    bot: String,
}
impl NavigationPolicy {
    pub(crate) fn for_app(app: &tauri::AppHandle, bot: &str) -> Result<Self, String> {
        if !crate::storage::valid_id(bot) {
            return Err(ERROR.into());
        }
        // Match Tauri 2.12's App URL selection for this embedded frontend. The
        // ordinary builder uses its default HTTP custom-protocol scheme.
        let base = if tauri::is_dev() {
            app.config().build.dev_url.clone().ok_or(ERROR)?
        } else {
            url::Url::parse(if cfg!(windows) {
                "http://tauri.localhost/"
            } else {
                "tauri://localhost/"
            })
            .map_err(|_| ERROR)?
        };
        Ok(Self {
            expected: base.join("index.html").map_err(|_| ERROR)?,
            bot: bot.into(),
        })
    }
    fn permits(&self, url: &url::Url) -> bool {
        if url.as_str().len() > 2048
            || url.scheme() != self.expected.scheme()
            || url.host_str() != self.expected.host_str()
            || url.port_or_known_default() != self.expected.port_or_known_default()
            || url.path() != self.expected.path()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return false;
        }
        let mut overlay = false;
        let mut bot = false;
        let mut count = 0;
        for (key, value) in url.query_pairs() {
            count += 1;
            if count > 2 {
                return false;
            }
            match key.as_ref() {
                "overlay" if !overlay && value == "1" => overlay = true,
                "bot" if !bot && value.as_ref() == self.bot.as_str() => bot = true,
                _ => return false,
            }
        }
        overlay && bot && count == 2
    }
}

/// Pre-build tracking is retired if construction fails before Destroyed hooks
/// are handed ownership. A pre-existing unrelated window is never touched.
pub(crate) struct TrackingGuard {
    app: tauri::AppHandle,
    bot: String,
    signals: Arc<WindowSignals>,
    tracked: bool,
    handed: bool,
}
impl TrackingGuard {
    pub(crate) fn complete(&mut self) {
        self.handed = true;
    }
}
impl Drop for TrackingGuard {
    fn drop(&mut self) {
        if self.tracked && !self.handed {
            retired(&self.app, &self.bot, &self.signals);
        }
    }
}
pub(crate) fn track(
    app: &tauri::AppHandle,
    bot: &str,
    signals: &Arc<WindowSignals>,
) -> TrackingGuard {
    let tracked = app
        .state::<crate::AppState>()
        .life_runtime
        .native_documents
        .track(bot, signals);
    TrackingGuard {
        app: app.clone(),
        bot: bot.into(),
        signals: signals.clone(),
        tracked,
        handed: false,
    }
}
fn cancel_exact(app: &tauri::AppHandle, bot: &str, signals: &Arc<WindowSignals>) {
    let state = app.state::<crate::AppState>();
    for member in state.life_runtime.native_scenes.members(Some(bot)) {
        if Arc::ptr_eq(member.constructor.signals(), signals) {
            member.cancel();
        }
    }
    state
        .life_runtime
        .native_document_route_revoked(bot, signals);
    state.life_runtime.native_scene_wake(Some(bot));
}
pub(crate) fn navigation(
    app: &tauri::AppHandle,
    policy: &NavigationPolicy,
    signals: &Arc<WindowSignals>,
    url: &url::Url,
) -> bool {
    let allowed = policy.permits(url);
    if app
        .state::<crate::AppState>()
        .life_runtime
        .native_documents
        .invalidate(&policy.bot, signals)
    {
        cancel_exact(app, &policy.bot, signals);
    }
    allowed
}
pub(crate) fn page_load(
    window: &tauri::WebviewWindow,
    policy: &NavigationPolicy,
    signals: &Arc<WindowSignals>,
    payload: tauri::webview::PageLoadPayload<'_>,
) {
    let app = window.app_handle();
    let state = app.state::<crate::AppState>();
    if !policy.permits(payload.url()) {
        retired(app, &policy.bot, signals);
        return;
    }
    match payload.event() {
        tauri::webview::PageLoadEvent::Started => {
            if state
                .life_runtime
                .native_documents
                .started(&policy.bot, signals)
            {
                cancel_exact(app, &policy.bot, signals);
            }
        }
        tauri::webview::PageLoadEvent::Finished => {
            if state.life_runtime.native_documents.finished(
                &policy.bot,
                signals,
                state.life_runtime.now(),
            ).is_ok() {
                // Finished is a physical document fact. The lifecycle owns the
                // cold asset probe and issues operational auth only afterward.
                state.life_runtime.native_scene_wake(Some(&policy.bot));
            }
        }
    }
}
pub(crate) fn retired(app: &tauri::AppHandle, bot: &str, signals: &Arc<WindowSignals>) {
    if app
        .state::<crate::AppState>()
        .life_runtime
        .native_documents
        .retire(bot, signals)
    {
        cancel_exact(app, bot, signals);
    }
}

/// Evaluate only the actual retained constructor. The caller retains this
/// future until the Main job really drains, even after cancellation/deadline;
/// a local timeout may not free a worker while its old eval is still queued.
async fn evaluate(
    app: &tauri::AppHandle,
    attempt: &SceneAttempt,
    deadline: u64,
    script: String,
    document: Option<super::native_document::DocumentProof>,
) -> Result<bool, String> {
    if !attempt.current(app) { return Err(ERROR.into()); }
    let policy = NavigationPolicy::for_app(app, attempt.constructor().bot())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let main = app.clone();
    let attempt = attempt.clone();
    app.run_on_main_thread(move || {
        let state = main.state::<crate::AppState>();
        // No Core/World/Store locks or native blocking getter under those locks.
        // Once the original delivery watch is attached, other actors' owned
        // geometry must not invalidate it through a global ticket comparison.
        let receipt = attempt.constructor();
        let current = state.life_runtime.now() < deadline
            && attempt.atoms_current()
            && document.as_ref().is_none_or(|proof| proof.current())
            && state.life_runtime.session_outputs.constructor_current(receipt)
            && !state.overlay.has_any_pending_speech()
            && state.updater.ensure_idle().is_ok()
            && state.generation.try_lock().is_ok();
        let result = current
            && receipt.window().url().is_ok_and(|url| policy.permits(&url))
            && receipt.window().is_visible().ok() == Some(true)
            && receipt.window().eval(&script).is_ok();
        let _ = tx.send(result);
    }).map_err(|_| ERROR)?;
    // Deliberately no timeout which would detach this one queued Main job.
    // The closure itself checks the immutable deadline and original atoms.
    rx.await.map_err(|_| ERROR.into())
}

/// Cold assets have their own bounded observation phase, before operational
/// first-auth/D5. The probe is a fact-only token and cannot prepare a layout.
pub(super) async fn deliver_probe(
    app: &tauri::AppHandle,
    attempt: &SceneAttempt,
    grant: &AssetProbeGrant,
) -> Result<(), String> {
    if !grant.loaded().same_loaded(attempt.loaded())
        || grant.avatar_binding() != attempt.avatar_binding()
        || !Arc::ptr_eq(&grant.delivery_cancel(), &attempt.cancel())
    { return Err(ERROR.into()); }
    let deadline = grant.issued_at().checked_add(BOOTSTRAP_MS)
        .filter(|n| *n <= crate::life_domain::MAX_COUNTER).ok_or(ERROR)?;
    let payload = serde_json::json!({"version":1,"probe":grant.probe(),
        "avatarBinding":grant.avatar_binding()}).to_string();
    let mut first = true;
    for _ in 0..100 {
        let state = app.state::<crate::AppState>();
        let now = state.life_runtime.now();
        if !attempt.current(app) { return Err(ERROR.into()); }
        match state.life_runtime.native_documents.probe_status(grant, now).map_err(|_| ERROR)? {
            AssetProbeState::Loading | AssetProbeState::Ready => return Ok(()),
            AssetProbeState::Failed => return Err(ERROR.into()),
            AssetProbeState::AwaitingMount => (),
        }
        if now >= deadline || !grant.current(now) { return Err(ERROR.into()); }
        let revoke = if first { "window.__orbitpalNativeWorldRevoke?.();" } else { "" };
        if evaluate(app, attempt, deadline,
            format!("{revoke}window.__orbitpalNativeWorldProbe?.({payload});"), None).await?
        { first = false; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(ERROR.into())
}

/// A genuinely ready asset observation is rotated into this operational grant
/// by the same worker. Its exact original cancel/lease watches remain attached.
pub(super) async fn bootstrap_attempt(
    app: &tauri::AppHandle,
    attempt: &SceneAttempt,
    grant: DocumentGrant,
) -> Result<(), String> {
    let receipt = attempt.constructor();
    if grant.proof().bot() != receipt.bot()
        || !Arc::ptr_eq(grant.proof().signals(), receipt.signals())
        || !Arc::ptr_eq(&grant.delivery_cancel(), &attempt.cancel())
    { return Err(ERROR.into()); }
    let deadline = grant.issued_at().checked_add(BOOTSTRAP_MS)
        .filter(|n| *n <= crate::life_domain::MAX_COUNTER).ok_or(ERROR)?;
    let label = receipt.window().label();
    let payload = serde_json::json!({"version":1,"capability":grant.capability()}).to_string();
    let mut first = true;
    for _ in 0..100 {
        let state = app.state::<crate::AppState>();
        let now = state.life_runtime.now();
        if !attempt.current(app) || !grant.proof().current() || !grant.delivery_current() {
            return Err(ERROR.into());
        }
        // Authentication of this exact document, rather than successful queueing
        // or eval, acknowledges delivery. Own layout generation changes are inert.
        if state.life_runtime.native_documents.authenticate_bound(label, grant.capability(), now).is_ok() {
            return Ok(());
        }
        if now >= deadline { return Err(ERROR.into()); }
        let revoke = if first { "window.__orbitpalNativeWorldRevoke?.();" } else { "" };
        if evaluate(app, attempt, deadline,
            format!("{revoke}window.__orbitpalNativeWorldBootstrap?.({payload});"), Some(grant.proof())).await?
        { first = false; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(ERROR.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> NavigationPolicy {
        NavigationPolicy {
            expected: url::Url::parse("tauri://localhost/index.html").unwrap(),
            bot: "nova".into(),
        }
    }
    #[test]
    fn native_document_navigation_is_closed_to_exact_app_path_and_overlay_bot_query() {
        let policy = policy();
        for url in [
            "tauri://localhost/index.html?overlay=1&bot=nova",
            "tauri://localhost/index.html?bot=nova&overlay=1",
        ] {
            assert!(policy.permits(&url::Url::parse(url).unwrap()));
        }
        for url in [
            "https://example.org/index.html?overlay=1&bot=nova",
            "tauri://foreign/index.html?overlay=1&bot=nova",
            "tauri://localhost/other.html?overlay=1&bot=nova",
            "tauri://localhost/index.html?overlay=1&bot=other",
            "tauri://localhost/index.html?overlay=1&bot=nova&session=secret",
            "tauri://localhost/index.html?overlay=1&overlay=1&bot=nova",
            "tauri://localhost/index.html?overlay=1&bot=nova#other",
            "tauri://user@localhost/index.html?overlay=1&bot=nova",
            "tauri://localhost/index.html?overlay=0&bot=nova",
        ] {
            assert!(!policy.permits(&url::Url::parse(url).unwrap()));
        }
    }
    #[test]
    fn configured_development_origin_and_port_are_exact_without_loopback_alias_guessing() {
        let policy = NavigationPolicy {
            expected: url::Url::parse("http://127.0.0.1:1420/index.html").unwrap(),
            bot: "nova".into(),
        };
        assert!(policy.permits(
            &url::Url::parse("http://127.0.0.1:1420/index.html?overlay=1&bot=nova").unwrap()
        ));
        for url in [
            "http://localhost:1420/index.html?overlay=1&bot=nova",
            "http://127.0.0.1:1421/index.html?overlay=1&bot=nova",
            "https://127.0.0.1:1420/index.html?overlay=1&bot=nova",
        ] {
            assert!(!policy.permits(&url::Url::parse(url).unwrap()));
        }
    }
}
