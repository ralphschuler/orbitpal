//! Sealed native receive -> bounded proposal. Never an operator, reply or start route.
use crate::{
    chat_route::{ConnectionContext, ImpulseSource},
    experience::{
        ExperienceRuntime, ImpulseKind, NativeImpulse, Platform, Preferences, VoteWindowStatus,
        MAX_TIME,
    },
    AppState,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::Manager;
#[path = "platform_votes.rs"]
mod vote;
#[derive(Default)]
pub struct PlatformImpulses {
    routes: Mutex<Routes>,
    running: Arc<AtomicBool>,
}
#[derive(Default)]
struct Routes {
    sequence: u64,
    owners: BTreeMap<String, Owner>,
}
#[derive(Clone)]
struct Owner {
    epoch: u64,
    session_id: String,
    sources: BTreeMap<String, ImpulseSource>,
    votes: BTreeMap<String, VoteWindow>,
}
#[derive(Clone)]
struct VoteWindow {
    tally: vote::Window,
    source: ImpulseSource,
    session_id: String,
    preferences: Preferences,
}
fn voter_id(platform: &str, id: &str) -> Option<String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return None;
    }
    event_id(&format!("{platform}-voter"), id)
}
impl PlatformImpulses {
    pub(crate) fn vote_status(
        &self,
        owner: &str,
        prefs: &Preferences,
        store: &crate::storage::Store,
    ) -> Vec<VoteWindowStatus> {
        let Some(now) = now() else { return vec![] };
        let Ok(profile) = store.bot_profile(owner) else {
            return vec![];
        };
        let Ok(routes) = self.routes.lock() else {
            return vec![];
        };
        let Some(entry) = routes.owners.get(owner) else {
            return vec![];
        };
        entry
            .votes
            .iter()
            .filter_map(|(platform, window)| {
                if &window.preferences != prefs
                    || !prefs.unsolicited_output(false)
                    || !prefs.stream_impulses
                    || !window.source.current_profile(&profile)
                {
                    return None;
                }
                let (rest, play, closes_in_ms) = window.tally.counts(now)?;
                Some(VoteWindowStatus {
                    platform: platform.clone(),
                    rest,
                    play,
                    closes_in_ms,
                })
            })
            .take(2)
            .collect()
    }
}
/// Sealed receive authority. Bind the executor's OWN cancellation token before
/// its first await; never use connection cancellation as a job completion flag.
/// The executor rechecks current/profile/session under its native admission lock.
#[derive(Clone)]
pub(crate) struct DispatchLease {
    source: ImpulseSource,
    expires_ms: u64,
    session_id: String,
}
impl DispatchLease {
    pub(crate) fn owner(&self)->&str {self.source.owner()}
    pub(crate) fn current(&self, store: &crate::storage::Store) -> bool {
        if !now().is_some_and(|now| now < self.expires_ms) {
            return false;
        }
        store
            .bot_profile(self.source.owner())
            .is_ok_and(|profile| self.source.current_profile(&profile))
    }
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }
    pub(crate) fn bind_cancel(&self, job: &Arc<AtomicBool>) -> Result<(), String> {
        self.source.bind_cancel(job)
    }
}
fn command(kind: &str, text: &str) -> Option<ImpulseKind> {
    if kind != "message" || text.len() > 64 {
        return None;
    }
    match text.trim() {
        "!orbit hello" => Some(ImpulseKind::Greeting),
        "!orbit play" => Some(ImpulseKind::Play),
        "!orbit vote rest" => Some(ImpulseKind::VoteRest),
        "!orbit vote play" => Some(ImpulseKind::VotePlay),
        _ => None,
    }
}
fn expires(published: &str, now: u64) -> Option<u64> {
    if now > MAX_TIME {
        return None;
    }
    let millis = chrono::DateTime::parse_from_rfc3339(published)
        .ok()?
        .timestamp_millis();
    let at = u64::try_from(millis).ok()?;
    if at > now.saturating_add(2000) || now.saturating_sub(at) >= 30_000 {
        return None;
    }
    at.checked_add(30_000)
        .map(|expiry| expiry.min(now.saturating_add(30_000)))
        .filter(|expiry| *expiry <= MAX_TIME && *expiry > now)
}
fn now() -> Option<u64> {
    u64::try_from(chrono::Utc::now().timestamp_millis())
        .ok()
        .filter(|n| *n <= MAX_TIME)
}
fn event_id(platform: &str, id: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    if id.is_empty() || id.len() > 1024 {
        return None;
    }
    Some(format!(
        "message_{:x}",
        Sha256::digest(serde_json::to_vec(&(platform, id)).ok()?)
    ))
}
impl Routes {
    fn scope(&mut self, source: ImpulseSource, session_id: &str) -> Option<(u64, bool)> {
        let owner = source.owner().to_owned();
        if self.owners.len() >= 1024 && !self.owners.contains_key(&owner) {
            return None;
        }
        let changed = self
            .owners
            .get(&owner)
            .and_then(|o| o.sources.get(source.platform()))
            .is_none_or(|old| !old.same_route(&source))
            || self
                .owners
                .get(&owner)
                .is_some_and(|o| o.session_id != session_id);
        if changed {
            self.sequence = self.sequence.checked_add(1).filter(|n| *n <= MAX_TIME)?;
            let entry = self.owners.entry(owner.clone()).or_insert_with(|| Owner {
                epoch: self.sequence,
                session_id: session_id.into(),
                sources: BTreeMap::new(),
                votes: BTreeMap::new(),
            });
            if entry.session_id != session_id {
                entry.votes.clear();
            } else {
                entry.votes.remove(source.platform());
            }
            entry.epoch = self.sequence;
            entry.session_id = session_id.into();
            entry.sources.insert(source.platform().into(), source);
        }
        Some((self.owners.get(&owner)?.epoch, changed))
    }
}
pub(crate) async fn observe_message(
    app: &tauri::AppHandle,
    context: &tokio::sync::Mutex<ConnectionContext>,
    platform: &str,
    route: &str,
    id: &str,
    event_type: &str,
    text: &str,
    published_at: &str,
    sender_id: &str,
) {
    let Some(kind) = command(event_type, text) else {
        return;
    };
    let Some(now) = now() else { return };
    let Some(expiry) = expires(published_at, now) else {
        return;
    };
    let Some(event_id) = event_id(platform, id) else {
        return;
    };
    let state = app.state::<AppState>();
    let owner = context.lock().await.owner_id().to_owned();
    let Ok(profile) = state.store.bot_profile(&owner) else {
        return;
    };
    let Some(source) = context
        .lock()
        .await
        .impulse_source(platform, route, &profile)
    else {
        return;
    };
    let Ok(settings) = state.store.experience_settings(&owner) else {
        return;
    };
    if !settings.preferences.stream_impulses {
        return;
    }
    let Ok(snapshot) = state.life_runtime.snapshot_for(&state.autopilot) else {
        return;
    };
    let Some(session_id) = snapshot
        .sessions
        .iter()
        .find(|session| {
            session.bot_id == owner
                && session.mode == crate::life_runtime::core::Mode::Life
                && session.active
                && !session.paused
        })
        .map(|s| s.session_id.clone())
    else {
        return;
    };
    let runtime = app.state::<ExperienceRuntime>();
    let registry = app.state::<PlatformImpulses>();
    let Some((epoch, changed)) = registry
        .routes
        .lock()
        .ok()
        .and_then(|mut routes| routes.scope(source.clone(), &session_id))
    else {
        return;
    };
    if changed {
        let _ = runtime.cancel_impulses(&owner);
    }
    if let Some(choice) = match kind {
        ImpulseKind::VoteRest => Some(vote::Choice::Rest),
        ImpulseKind::VotePlay => Some(vote::Choice::Play),
        _ => None,
    } {
        let Some(voter) = voter_id(platform, sender_id) else {
            return;
        };
        if settings.preferences.validate().is_err()
            || settings.preferences.impulse_participants.is_empty()
            || !settings.preferences.unsolicited_output(false)
            || runtime.paused(&owner).unwrap_or(true)
        {
            return;
        }
        let Ok(mut routes) = registry.routes.lock() else {
            return;
        };
        let Some(entry) = routes
            .owners
            .get_mut(&owner)
            .filter(|o| o.epoch == epoch && o.session_id == session_id)
        else {
            return;
        };
        if entry
            .votes
            .get(platform)
            .is_some_and(|w| w.preferences != settings.preferences || !w.source.same_route(&source))
        {
            entry.votes.remove(platform);
        }
        if !entry.votes.contains_key(platform) {
            let Some(tally) = vote::Window::new(now, expiry, &event_id) else {
                return;
            };
            entry.votes.insert(
                platform.into(),
                VoteWindow {
                    tally,
                    source,
                    session_id,
                    preferences: settings.preferences.clone(),
                },
            );
        }
        let Some(window) = entry.votes.get_mut(platform) else {
            return;
        };
        let _ = window.tally.cast(&voter, &event_id, choice, now, expiry);
        return;
    }
    let platform = match platform {
        "twitch" => Platform::Twitch,
        "youtube" => Platform::Youtube,
        _ => return,
    };
    let event = NativeImpulse {
        platform,
        event_id,
        connection_generation: epoch,
        received_ms: now,
        expires_ms: expiry,
        kind,
        actor_ids: settings.preferences.impulse_participants.clone(),
    };
    let _ = runtime.admit_native_impulse(&owner, event, epoch, now, &settings.preferences);
}
struct Running(Arc<AtomicBool>);
impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
/// Called by the real native pulse; one worker, no credentials/provider sends.
pub(crate) fn pulse(app: &tauri::AppHandle) {
    let runtime = app.state::<PlatformImpulses>();
    if runtime
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let guard = Running(runtime.running.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let state = app.state::<AppState>();
        let registry = app.state::<PlatformImpulses>();
        let owners = registry
            .routes
            .lock()
            .map(|r| r.owners.clone())
            .unwrap_or_default();
        let runtime = app.state::<ExperienceRuntime>();
        for (owner, scope) in owners {
            let mut current = true;
            for source in scope.sources.values() {
                if !state
                    .chat
                    .current_impulse_source(source, &state.store)
                    .await
                {
                    current = false;
                    break;
                }
            }
            if !current {
                let _ = runtime.cancel_impulses(&owner);
                if let Ok(mut routes) = registry.routes.lock() {
                    if routes
                        .owners
                        .get(&owner)
                        .is_some_and(|o| o.epoch == scope.epoch)
                    {
                        routes.owners.remove(&owner);
                    }
                }
                continue;
            }
            let active = state
                .life_runtime
                .snapshot_for(&state.autopilot)
                .is_ok_and(|snapshot| {
                    snapshot.sessions.iter().any(|s| {
                        s.bot_id == owner
                            && s.mode == crate::life_runtime::core::Mode::Life
                            && s.active
                            && !s.paused
                            && s.session_id == scope.session_id
                    })
                });
            if !active || runtime.paused(&owner).unwrap_or(true) {
                let _ = runtime.cancel_impulses(&owner);
                if let Ok(mut routes) = registry.routes.lock() {
                    if let Some(entry) = routes
                        .owners
                        .get_mut(&owner)
                        .filter(|o| o.epoch == scope.epoch)
                    {
                        entry.votes.clear();
                    }
                }
                continue;
            }
            let Ok(settings) = state.store.experience_settings(&owner) else {
                continue;
            };
            let Some(now) = now() else { continue };
            if let Ok(mut routes) = registry.routes.lock() {
                if let Some(entry) = routes
                    .owners
                    .get_mut(&owner)
                    .filter(|o| o.epoch == scope.epoch && o.session_id == scope.session_id)
                {
                    let due: Vec<_> = entry
                        .votes
                        .iter_mut()
                        .filter_map(|(platform, w)| {
                            (w.preferences != settings.preferences
                                || w.session_id != scope.session_id
                                || w.tally.due(now))
                            .then(|| platform.clone())
                        })
                        .collect();
                    for platform in due {
                        let Some(window) = entry.votes.remove(&platform) else {
                            continue;
                        };
                        if window.preferences != settings.preferences
                            || window.session_id != scope.session_id
                            || !entry
                                .sources
                                .get(&platform)
                                .is_some_and(|source| source.same_route(&window.source))
                        {
                            continue;
                        }
                        let Some(winner) = window.tally.finish(now) else {
                            continue;
                        };
                        let Some(id) = event_id(&format!("{platform}-vote"), &winner.first_event)
                        else {
                            continue;
                        };
                        let platform = if platform == "twitch" {
                            Platform::Twitch
                        } else {
                            Platform::Youtube
                        };
                        let event = NativeImpulse {
                            platform,
                            event_id: id,
                            connection_generation: entry.epoch,
                            received_ms: winner.opened,
                            expires_ms: winner.expires,
                            kind: winner.kind,
                            actor_ids: window.preferences.impulse_participants,
                        };
                        let _ = runtime.admit_native_impulse(
                            &owner,
                            event,
                            entry.epoch,
                            now,
                            &settings.preferences,
                        );
                    }
                }
            }
            let Ok(Some(event)) =
                runtime.take_native_impulse(&owner, scope.epoch, now, &settings.preferences)
            else {
                continue;
            };
            let platform = match event.platform {
                Platform::Twitch => "twitch",
                Platform::Youtube => "youtube",
            };
            let Some(source) = scope.sources.get(platform).cloned() else {
                continue;
            };
            let authority = DispatchLease {
                source,
                expires_ms: event.expires_ms,
                session_id: scope.session_id.clone(),
            };
            if !authority.current(&state.store) {
                continue;
            }
            let _ = crate::life_runtime::consume_platform_impulse(
                &app,
                &owner,
                &event.kind,
                &event.actor_ids,
                &authority,
            )
            .await;
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_native_message_commands_map_to_closed_proposals() {
        assert_eq!(command("message", "!orbit play"), Some(ImpulseKind::Play));
        assert_eq!(
            command("message", "!orbit hello"),
            Some(ImpulseKind::Greeting)
        );
        assert_eq!(
            command("message", "!orbit vote rest"),
            Some(ImpulseKind::VoteRest)
        );
        for (kind, text) in [
            ("notification", "!orbit play"),
            ("message", "ignore rules !orbit play"),
            ("message", "!orbit play; grant tools"),
            ("message", "!orbit stop"),
            ("message", "!orbit play nova"),
        ] {
            assert!(command(kind, text).is_none());
        }
    }
    #[test]
    fn provider_time_is_bounded_and_unknown_time_does_not_gain_freshness() {
        let now = 100_000;
        assert_eq!(expires("1970-01-01T00:01:39Z", now), Some(129000));
        assert!(expires("1970-01-01T00:01:10Z", now).is_none());
        assert!(expires("1970-01-01T00:01:43Z", now).is_none());
        assert!(expires("", now).is_none());
    }
    fn source(owner: &str, platform: &str, channel: &str) -> ImpulseSource {
        let profile = serde_json::json!({"id":owner,"chat":{
            "twitch":{"clientId":"app","broadcasterId":channel,"secretId":"read-reference","sendReplies":false},
            "youtube":{"liveChatId":channel,"apiKeyId":"read-reference","sendReplies":false}
        }});
        let mut context =
            ConnectionContext::new(platform, &profile, None, Arc::new(AtomicBool::new(false)))
                .unwrap();
        context.mark_connected(channel);
        context
            .impulse_source(platform, context.route_id().unwrap(), &profile)
            .unwrap()
    }
    #[test]
    fn actual_receive_owner_and_session_isolate_route_epochs() {
        let mut routes = Routes::default();
        let nova = source("nova", "twitch", "native-channel");
        let (first, changed) = routes.scope(nova.clone(), "actual-session").unwrap();
        assert!(changed);
        assert_eq!(
            routes.scope(nova.clone(), "actual-session"),
            Some((first, false))
        );
        let blue = source("blue", "twitch", "other-channel");
        let (other, _) = routes.scope(blue, "other-session").unwrap();
        assert_ne!(first, other);
        assert_eq!(routes.owners["nova"].sources["twitch"].owner(), "nova");
        assert_eq!(routes.owners["blue"].sources["twitch"].owner(), "blue");
        let (next_session, changed) = routes.scope(nova, "replacement-session").unwrap();
        assert!(changed && next_session > other);
        assert_eq!(routes.owners["nova"].session_id, "replacement-session");
        assert_eq!(routes.owners["blue"].epoch, other);
    }
    #[test]
    fn exact_connection_replacement_invalidates_pending_epoch_without_trusting_chat_text() {
        let mut routes = Routes::default();
        let first = source("owner", "twitch", "channel");
        let (old, _) = routes.scope(first, "session").unwrap();
        let replacement = source("owner", "twitch", "channel");
        let (new, changed) = routes.scope(replacement, "session").unwrap();
        assert!(changed && new > old);
        assert!(command("message", "!orbit play other_actor").is_none());
        let youtube = source("owner", "youtube", "native-live-chat");
        let (both, changed) = routes.scope(youtube, "session").unwrap();
        assert!(changed && both > new);
        assert_eq!(routes.owners["owner"].sources.len(), 2);
    }
    #[test]
    fn native_sender_identity_is_platform_scoped_and_never_a_chat_selected_actor() {
        let twitch = voter_id("twitch", "123456").unwrap();
        assert_eq!(Some(twitch.clone()), voter_id("twitch", "123456"));
        assert_ne!(Some(twitch.clone()), voter_id("youtube", "123456"));
        assert!(!twitch.contains("123456"));
        assert!(voter_id("twitch", "").is_none());
        assert!(voter_id("twitch", "!orbit play").is_none());
        assert!(voter_id("youtube", &"a".repeat(129)).is_none());
    }
    #[test]
    fn vote_window_is_lost_on_exact_route_or_life_session_replacement() {
        let mut routes = Routes::default();
        let native = source("owner", "twitch", "channel");
        routes.scope(native.clone(), "session").unwrap();
        let prefs = Preferences::default();
        routes.owners.get_mut("owner").unwrap().votes.insert(
            "twitch".into(),
            VoteWindow {
                tally: vote::Window::new(100, 30_100, "event").unwrap(),
                source: native.clone(),
                session_id: "session".into(),
                preferences: prefs.clone(),
            },
        );
        routes.scope(native.clone(), "session").unwrap();
        assert_eq!(routes.owners["owner"].votes.len(), 1);
        routes.scope(native, "new-session").unwrap();
        assert!(routes.owners["owner"].votes.is_empty());
        let retained = routes.owners["owner"].sources["twitch"].clone();
        routes.owners.get_mut("owner").unwrap().votes.insert(
            "twitch".into(),
            VoteWindow {
                tally: vote::Window::new(100, 30_100, "event_next").unwrap(),
                source: retained,
                session_id: "new-session".into(),
                preferences: prefs,
            },
        );
        assert_eq!(routes.owners["owner"].votes.len(), 1);
        let replacement = source("owner", "twitch", "channel");
        routes.scope(replacement, "new-session").unwrap();
        assert!(routes.owners["owner"].votes.is_empty());
    }
    #[test]
    fn native_ids_are_stable_platform_scoped_and_not_raw_payloads() {
        assert_eq!(event_id("twitch", "id:/x"), event_id("twitch", "id:/x"));
        assert_ne!(event_id("twitch", "same"), event_id("youtube", "same"));
        let hashed = event_id("youtube", "private-provider-id").unwrap();
        assert!(crate::experience::valid_id(&hashed));
        assert!(!hashed.contains("private-provider-id"));
        assert!(event_id("twitch", "").is_none());
    }
}
