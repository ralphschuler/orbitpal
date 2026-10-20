//! Actual native document callbacks and bounded ephemeral bootstrap. This
//! evaluates only a retained constructor; no capability is persisted or logged.
use super::{
    native_document::{DocumentError, DocumentGrant},
    session_output::{ConstructorReceipt, Lease},
};
use crate::window_signals::WindowSignals;
use std::{
    sync::{atomic::Ordering, Arc},
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
            if let Ok(grant) = state.life_runtime.native_documents.finished(
                &policy.bot,
                signals,
                state.life_runtime.now(),
            ) {
                bootstrap(app.clone(), policy.clone(), grant, false);
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

fn eligible(
    app: &tauri::AppHandle,
    policy: &NavigationPolicy,
    grant: &DocumentGrant,
    ticket: u64,
) -> Option<ConstructorReceipt> {
    let state = app.state::<crate::AppState>();
    if !grant.proof().current()
        || !grant.delivery_current()
        || grant.proof().bot() != policy.bot
        || state.updater.ensure_idle().is_err()
        || state.overlay.has_any_pending_speech()
    {
        return None;
    }
    let _generation = state.generation.try_lock().ok()?;
    let lease = state
        .life_runtime
        .session_outputs
        .resolve(&policy.bot)
        .ok()??;
    let receipt = state
        .life_runtime
        .session_outputs
        .constructor_for(&lease)
        .ok()?;
    if !state
        .life_runtime
        .session_outputs
        .constructor_current(&receipt)
        || !Arc::ptr_eq(receipt.signals(), grant.proof().signals())
    {
        return None;
    }
    if state.life_runtime.outputs.ticket() != Some(ticket) {
        return None;
    }
    let core = state.life_runtime.core.lock().ok()?;
    core.local_fence(&policy.bot, state.life_runtime.now())?;
    drop(core);
    if state.life_runtime.outputs.ticket() != Some(ticket)
        || !grant.proof().current()
        || !grant.delivery_current()
        || !receipt.lease_current()
    {
        return None;
    }
    Some(receipt)
}
fn bootstrap(
    app: tauri::AppHandle,
    policy: NavigationPolicy,
    grant: DocumentGrant,
    revoke_first: bool,
) {
    // Capture before spawning/waiting for constructor/Core readiness. Later
    // clear heartbeats cannot turn a cancelled bootstrap into a fresh intent.
    let state = app.state::<crate::AppState>();
    let Some(ticket) = state.life_runtime.outputs.ticket() else {
        return;
    };
    // This same cancellation atom belongs to the original Finished/rotate,
    // including duplicate callbacks. Attach before Core/constructor waits;
    // neither a retry nor a later clear UI report may rebind a cancelled cap.
    let delivery = grant.delivery_cancel();
    if !state.life_runtime.outputs.watch_delivery(
        ticket,
        &policy.bot,
        &format!("avatar-{}", policy.bot),
        &delivery,
    ) || state
        .life_runtime
        .monitor
        .get()
        .and_then(|monitor| monitor.gate.watch(&delivery))
        .is_none()
    {
        delivery.store(true, Ordering::SeqCst);
        return;
    }
    drop(state);
    tauri::async_runtime::spawn(async move {
        let Some(deadline) = grant
            .issued_at()
            .checked_add(BOOTSTRAP_MS)
            .filter(|n| *n <= crate::life_domain::MAX_COUNTER)
        else {
            return;
        };
        let mut first = revoke_first;
        let mut admitted: Option<ConstructorReceipt> = None;
        // A finite attempt count also bounds this worker if the monotonic host
        // clock is unavailable/exhausted. No grant time is refreshed on retry.
        for _ in 0..100 {
            let state = app.state::<crate::AppState>();
            let now = state.life_runtime.now();
            if now >= deadline
                || !grant.proof().current()
                || !grant.delivery_current()
                || state.life_runtime.outputs.ticket() != Some(ticket)
            {
                return;
            }
            if state
                .life_runtime
                .native_documents
                .authenticate_bound(&format!("avatar-{}", policy.bot), grant.capability(), now)
                .is_ok()
            {
                return;
            }
            if admitted.is_none() {
                if let Some(receipt) = eligible(&app, &policy, &grant, ticket) {
                    if state
                        .life_runtime
                        .native_documents
                        .bind_delivery(&grant, delivery.clone(), receipt.lease_cancel())
                        .is_err()
                    {
                        return;
                    }
                    admitted = Some(receipt);
                }
            }
            if let Some(receipt) = &admitted {
                if !receipt.lease_current()
                    || !state
                        .life_runtime
                        .session_outputs
                        .constructor_current(receipt)
                    || state.life_runtime.outputs.ticket() != Some(ticket)
                {
                    return;
                }
                let (tx, rx) = tokio::sync::oneshot::channel();
                let main = app.clone();
                let receipt = receipt.clone();
                let proof = grant.proof();
                let delivery = delivery.clone();
                let policy = policy.clone();
                let revoke = first;
                let payload =
                    serde_json::json!({"version":1,"capability":grant.capability()}).to_string();
                if app
                    .run_on_main_thread(move || {
                        let state = main.state::<crate::AppState>();
                        // These native-only latches/short state observations do not
                        // acquire Core/World/Store or run caller code under a lock.
                        let current = state.life_runtime.now() < deadline
                            && proof.current()
                            && !delivery.load(Ordering::SeqCst)
                            && receipt.lease_current()
                            && state.life_runtime.outputs.ticket() == Some(ticket)
                            && state
                                .life_runtime
                                .session_outputs
                                .constructor_current(&receipt)
                            && !state.overlay.has_any_pending_speech()
                            && state.updater.ensure_idle().is_ok();
                        let result = if current
                            && receipt.window().url().is_ok_and(|url| policy.permits(&url))
                            && receipt.window().is_visible().ok() == Some(true)
                        {
                            let revoke = if revoke {
                                "window.__orbitpalNativeWorldRevoke?.();"
                            } else {
                                ""
                            };
                            receipt
                                .window()
                                .eval(&format!(
                                    "{revoke}window.__orbitpalNativeWorldBootstrap?.({payload});"
                                ))
                                .is_ok()
                        } else {
                            false
                        };
                        let _ = tx.send(result);
                    })
                    .is_err()
                {
                    return;
                }
                // One queued Main job at a time. A timed-out job still checks
                // the same original deadline and exact cancellation identities.
                match tokio::time::timeout(Duration::from_millis((deadline - now).min(250)), rx)
                    .await
                {
                    // Queueing is not delivery. Keep the old-document revoke
                    // prefix until the retained constructor accepted eval;
                    // a temporarily unavailable view must retry both steps.
                    Ok(Ok(true)) => first = false,
                    Ok(Ok(false)) => (),
                    _ => return,
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
}

/// Explicit native post-start/post-dialog reconciliation only. Root calls this
/// after old physical cleanup; it is not an app-start or model-invoked trigger.
pub(crate) fn kick_for_lease(app: &tauri::AppHandle, lease: &Lease) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let receipt = state.life_runtime.session_outputs.constructor_for(lease)?;
    if !state
        .life_runtime
        .session_outputs
        .constructor_current(&receipt)
    {
        return Err(ERROR.into());
    }
    let now = state.life_runtime.now();
    match state
        .life_runtime
        .native_documents
        .current_grant(receipt.bot(), receipt.signals(), now)
    {
        Err(DocumentError::Pending) => return Ok(()), // the actual Finished callback will retry
        Err(_) => return Err(ERROR.into()),
        Ok(_) => (),
    }
    if state
        .life_runtime
        .native_scenes
        .members(Some(receipt.bot()))
        .iter()
        .any(|member| member.current() && member.constructor.same_constructor(&receipt))
    {
        return Ok(());
    }
    if state.presentation.native_guarded(receipt.bot()) {
        return Err(ERROR.into());
    }
    let grant = state.life_runtime.native_scenes.rotate_document_if_idle(
        &state.life_runtime.native_documents,
        &receipt,
        now,
    )?;
    let policy = NavigationPolicy::for_app(app, receipt.bot())?;
    bootstrap(app.clone(), policy, grant, true);
    Ok(())
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
