//! Nonshipping actual-Wry fixture. No alternate backend, proof factory or ACK.
//! The cfg feature is deliberately not enabled by any ordinary build target.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

const ERROR: &str = "native_world_fixture_refused";
static CREDENTIAL_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static DENIED_COMMANDS: AtomicU64 = AtomicU64::new(0);
fn increment(counter: &AtomicU64) { let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1)); }

// Closed, bounded debug counters. These are observations only: no identity,
// capability, command payload, window text, error string or readiness fact.
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum FixtureIpcPhase { RegisterEntered, RegisterAuthenticated, RegisterMember, RegisterAwaitingLayout, RegisterAwaitingCalibration, RegisterSource, RegisterRegistryAccepted, RegisterAwaitingMeasurement, RegisterReady, RegisterUnavailable, RegisterReplyPinned, MeasurementEntered, MeasurementAuthenticated, MeasurementValid, MeasurementOwned, MeasurementSourceMatches, MeasurementSource, MeasurementRegistryAccepted, MeasurementPromotionAccepted, MeasurementAccepted }
const DIAGNOSTIC_LIMIT: u64 = 10_000_000;
const PHASE_NAMES: [&str; 20] = ["register_entered", "register_authenticated", "register_member", "register_awaiting_layout", "register_awaiting_calibration", "register_source", "register_registry_accepted", "register_awaiting_measurement", "register_ready", "register_unavailable", "register_reply_pinned", "measurement_entered", "measurement_authenticated", "measurement_valid", "measurement_owned", "measurement_source_matches", "measurement_source", "measurement_registry_accepted", "measurement_promotion_accepted", "measurement_accepted"];
static IPC_PHASES: [AtomicU64; 20] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
const DENIED_NAMES: [&str; 11] = ["main_only", "get_document", "get_knowledge", "get_emote_sound", "get_speech_audio", "list_models", "check_cli_provider", "list_cli_models", "overlay_control", "plugin", "other"];
static DENIED_KINDS: [AtomicU64; 11] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
fn diagnostic_increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < DIAGNOSTIC_LIMIT).then(|| n + 1));
}
pub(crate) fn observe_native_ipc_phase(phase: FixtureIpcPhase) { diagnostic_increment(&IPC_PHASES[phase as usize]); }
fn observe_denied_command(command: &str, main_only: bool) {
    let index = if main_only { 0 } else { match command {
        "get_document" => 1, "get_knowledge" => 2, "get_emote_sound" => 3,
        "get_speech_audio" => 4, "list_models" => 5, "check_cli_provider" => 6,
        "list_cli_models" => 7, "open_overlay" | "close_overlay" | "configure_overlay" => 8,
        _ if command.starts_with("plugin:") => 9, _ => 10,
    }};
    diagnostic_increment(&DENIED_KINDS[index]);
}
#[derive(Serialize)]
struct FixtureDiagnostics {
    ipc_phases: std::collections::BTreeMap<&'static str, u64>,
    denied_kinds: std::collections::BTreeMap<&'static str, u64>,
}
fn diagnostics() -> FixtureDiagnostics {
    FixtureDiagnostics {
        ipc_phases: PHASE_NAMES.iter().copied().zip(IPC_PHASES.iter().map(|n| n.load(Ordering::SeqCst))).collect(),
        denied_kinds: DENIED_NAMES.iter().copied().zip(DENIED_KINDS.iter().map(|n| n.load(Ordering::SeqCst))).collect(),
    }
}

/// Called before constructing an OS credential-store backend, including read,
/// write and delete. A refusal is an error, never an invented missing key.
pub(crate) fn deny_credential() -> keyring_core::Error {
    increment(&CREDENTIAL_ATTEMPTS);
    keyring_core::Error::NoStorageAccess(Box::new(std::io::Error::new(std::io::ErrorKind::PermissionDenied, ERROR)))
}
/// Compile-feature hook at outbound provider/CLI/memory boundaries, before IO.
pub(crate) fn deny_outbound() -> Result<(), String> { increment(&OUTBOUND_ATTEMPTS); Err(ERROR.into()) }
/// Even an accidentally launched ordinary binary with this feature must not
/// discover installed app-data. The example is the only supported entry.
pub(crate) fn refuse_shipping_entry() -> ! { std::process::exit(2) }

pub fn run() -> i32 {
    #[cfg(target_os = "macos")]
    { mac::run().unwrap_or(2) }
    #[cfg(not(target_os = "macos"))]
    { 2 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FixtureRestore { Restored, Retired, WithoutGeometry, Guarded }

/// These functions observe only completed production linearization points.
/// They cannot admit a session, mutate layout, send a renderer ACK or persist
/// a Life effect. A caller cannot supply arbitrary JSON or a private proof.
pub(crate) fn observe_prepared(app: &tauri::AppHandle, bot: &str, signals: &std::sync::Arc<crate::window_signals::WindowSignals>, epoch: u64, normal_warm: u64) {
    #[cfg(target_os = "macos")]
    mac::observe(app, bot, signals, mac::Event::Prepared { epoch, warm: normal_warm });
}
pub(crate) fn observe_frame(app: &tauri::AppHandle, bot: &str, signals: &std::sync::Arc<crate::window_signals::WindowSignals>, epoch: u64, frame: u64, received: u64, admitted: u64, live_current: bool) {
    #[cfg(target_os = "macos")]
    mac::observe(app, bot, signals, mac::Event::Frame { epoch, frame, received, admitted, live_current });
}
pub(crate) fn observe_cleanup(app: &tauri::AppHandle, bot: &str, signals: &std::sync::Arc<crate::window_signals::WindowSignals>, epoch: u64, outcome: FixtureRestore) {
    #[cfg(target_os = "macos")]
    mac::observe(app, bot, signals, mac::Event::Cleanup { epoch, outcome });
}
pub(crate) fn observe_destroyed(app: &tauri::AppHandle, bot: &str, signals: &std::sync::Arc<crate::window_signals::WindowSignals>) {
    #[cfg(target_os = "macos")]
    mac::observe(app, bot, signals, mac::Event::Destroyed);
}
/// Only a successful main-only operator Stop may call this, with its captured
/// exact native member sequence. Generic shutdown/timeout is never a Stop fact.
pub(crate) fn observe_stop(app: &tauri::AppHandle, bot: &str, signals: &std::sync::Arc<crate::window_signals::WindowSignals>, epoch: u64) {
    #[cfg(target_os = "macos")]
    mac::observe(app, bot, signals, mac::Event::Stop { epoch });
}
/// Real save_profile calls this before profile mutation/revocation. Only the
/// unchanged, normalized seeded profile can be saved by the fixture's Main.
pub(crate) fn validate_profile_for_fixture(store: &crate::storage::Store, profile: &serde_json::Value) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { mac::validate_profile(store, profile).map_err(|_| ERROR.into()) }
    #[cfg(not(target_os = "macos"))]
    { let _ = (store, profile); Err(ERROR.into()) }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use crate::{storage::Store, window_signals::WindowSignals};
    use serde_json::{json, Value};
    use std::{fs::{self, File, OpenOptions}, io::Write, os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt}, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, AtomicI32}, Arc, Mutex, OnceLock}, time::{Duration, Instant}};
    use tauri::Manager;

    const APP_ID: &str = "studio.orbitpal.native-world-fixture";
    const LIMIT: u64 = 10_000_000;
    const DEADLINE_MS: u64 = 180_000;
    const REQUIRED_LIVE_MS: u64 = 35_000;
    const REQUIRED_HIDDEN_MS: u64 = 10_000;
    const MAX_FRAME_GAP_MS: u64 = 5_000;
    struct SaveSeal { root: PathBuf, profiles: [Value; 2] }
    static SAVE_SEAL: OnceLock<SaveSeal> = OnceLock::new();
    type Result<T> = std::result::Result<T, &'static str>;
    fn fixed<T>(_: T) -> &'static str { ERROR }
    fn normalized_profile(mut profile: Value) -> Value {
        if let Some(avatar) = profile.get_mut("avatar").and_then(Value::as_object_mut) { avatar.remove("emote"); }
        profile
    }
    fn safe_profile(profile: &Value) -> bool {
        profile["avatar"]["assetPath"].is_null() && profile["avatar"]["camera"] == "portrait"
            && profile["avatar"]["scale"] == 1 && profile["output"]["mode"] == "studio"
            && profile["output"]["streamingEnabled"] == false && profile["output"]["fps"] == 60
            && profile["model"]["provider"] == "compatible" && profile["model"]["model"] == ""
            && profile["model"]["command"] == "" && profile["model"]["apiKeyId"] == ""
            && profile["model"]["baseUrl"] == "http://127.0.0.1:9/v1"
            && profile["model"]["toolsEnabled"] == false && profile["model"]["visionEnabled"] == false
            && profile["voice"]["enabled"] == false && profile["voice"]["apiKeyId"] == ""
            && profile["stt"]["apiKeyId"] == "" && profile["autopilot"]["speak"] == false
            && profile["autopilot"]["gestures"] == false && profile["allowSkillScripts"] == false
            && ["mcpServers", "skillsEnabled", "motions"].iter().all(|key| profile[*key].as_array().is_some_and(Vec::is_empty))
            && profile["screenContext"]["enabled"] == false && profile["screenContext"]["displayId"] == ""
            && profile["wakeWord"]["enabled"] == false && profile["hindsight"]["enabled"] == false
            && profile["hindsight"]["apiKeyId"] == "" && profile["chat"]["autoReply"] == false
            && profile["chat"]["mode"] == "manual" && profile["chat"]["twitch"]["sendReplies"] == false
            && profile["chat"]["twitch"]["secretId"] == "" && profile["chat"]["youtube"]["sendReplies"] == false
            && profile["chat"]["youtube"]["apiKeyId"] == "" && profile["chat"]["youtube"]["sendSecretId"] == ""
    }
    pub(super) fn validate_profile(store: &Store, profile: &Value) -> Result<()> {
        let seal = SAVE_SEAL.get().ok_or(ERROR)?;
        // This comparison MUST precede even a stored-profile read. A different
        // Store/root can never turn the fixture into an installed-data reader.
        if store.root != seal.root { return Err(ERROR); }
        let expected = seal.profiles.iter().find(|p| p["id"] == profile["id"]).ok_or(ERROR)?;
        if !safe_profile(profile) || profile != expected { return Err(ERROR); }
        let current = normalized_profile(store.profile().map_err(fixed)?);
        if current != *expected || !safe_profile(&current) { return Err(ERROR); }
        Ok(())
    }

    struct Root { path: PathBuf, directory: File, store_directory: File, uid: u32 }
    impl Root {
        fn fresh() -> Result<Self> {
            // Explicit compile-time evidence directory; no runtime HOME/app-data
            // lookup, fallback path, user-supplied argv path or environment API.
            let parent = Path::new(option_env!("ORBITPAL_NATIVE_WORLD_FIXTURE_EVIDENCE").ok_or(ERROR)?);
            if !parent.is_absolute() || parent.canonicalize().map_err(fixed)? != parent { return Err(ERROR); }
            let uid = unsafe { libc::geteuid() };
            let parent_meta = fs::symlink_metadata(parent).map_err(fixed)?;
            if !parent_meta.is_dir() || parent_meta.uid() != uid || parent_meta.mode() & 0o022 != 0 { return Err(ERROR); }
            let path = parent.join(format!("world-native-macos-actual-run-{}", uuid::Uuid::new_v4().simple()));
            fs::DirBuilder::new().mode(0o700).create(&path).map_err(fixed)?;
            let directory = open_dir(&path, uid)?;
            fs::DirBuilder::new().mode(0o700).create(path.join("store")).map_err(fixed)?;
            let store_directory = open_dir(&path.join("store"), uid)?;
            let root = Self { path, directory, store_directory, uid };
            root.check()?;
            if fs::read_dir(root.path.join("store")).map_err(fixed)?.next().is_some() { return Err(ERROR); }
            Ok(root)
        }
        fn check(&self) -> Result<()> {
            same_dir(&self.path, &self.directory, self.uid)?;
            same_dir(&self.path.join("store"), &self.store_directory, self.uid)
        }
        fn write_result(&self, result: &Report) -> Result<()> {
            self.check()?;
            let bytes = serde_json::to_vec(result).map_err(fixed)?;
            if bytes.len() > 16_384 { return Err(ERROR); }
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(self.path.join("result.json")).map_err(fixed)?;
            file.write_all(&bytes).map_err(fixed)?; file.write_all(b"\n").map_err(fixed)?; file.sync_all().map_err(fixed)?;
            self.check()
        }
    }
    fn open_dir(path: &Path, uid: u32) -> Result<File> {
        let file = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path).map_err(fixed)?;
        same_dir(path, &file, uid)?; Ok(file)
    }
    fn same_dir(path: &Path, file: &File, uid: u32) -> Result<()> {
        let leaf = fs::symlink_metadata(path).map_err(fixed)?; let retained = file.metadata().map_err(fixed)?;
        if !leaf.is_dir() || !retained.is_dir() || leaf.uid() != uid || leaf.mode() & 0o077 != 0 || leaf.dev() != retained.dev() || leaf.ino() != retained.ino() { return Err(ERROR); }
        Ok(())
    }

    #[derive(Clone, Copy)]
    pub(super) enum Event {
        Prepared { epoch: u64, warm: u64 },
        Frame { epoch: u64, frame: u64, received: u64, admitted: u64, live_current: bool },
        Cleanup { epoch: u64, outcome: FixtureRestore }, Stop { epoch: u64 }, Destroyed,
    }
    #[derive(Clone)]
    struct Epoch {
        sequence: u64, warm: u64, first: Option<u64>, last: Option<u64>, frame: u64,
        accepted: u64, max_gap: u64, max_age: u64, hidden_first: Option<u64>,
        hidden_last: Option<u64>, hidden_frames: u64, stopped: bool, cleanup: Option<FixtureRestore>,
    }
    impl Epoch {
        fn new(sequence: u64, warm: u64) -> Self { Self { sequence, warm, first: None, last: None, frame: 0,
            accepted: 0, max_gap: 0, max_age: 0, hidden_first: None, hidden_last: None, hidden_frames: 0,
            stopped: false, cleanup: None } }
        fn clear_hidden(&mut self) { self.hidden_first = None; self.hidden_last = None; self.hidden_frames = 0; }
        fn span(&self, hidden: bool) -> Option<SpanReport> {
            let (first, last, frames) = if hidden { (self.hidden_first?, self.hidden_last?, self.hidden_frames) }
                else { (self.first?, self.last?, self.accepted) };
            Some(SpanReport { epoch: self.sequence, accepted_frames: frames, first_admitted_ms: first,
                last_admitted_ms: last, span_ms: last.checked_sub(first)?, last_frame_sequence: self.frame,
                max_frame_gap_ms: self.max_gap, max_frame_age_ms: self.max_age })
        }
        fn report(&self) -> EpochReport { EpochReport { epoch: self.sequence, normal_warm_sequence: self.warm,
            accepted_frames: self.accepted, first_admitted_ms: self.first, last_admitted_ms: self.last,
            last_frame_sequence: self.frame, live_span_ms: span(self.first, self.last), max_frame_gap_ms: self.max_gap,
            max_frame_age_ms: self.max_age, stopped: self.stopped, cleanup: self.cleanup } }
    }
    struct Actor {
        id: String, signals: Option<Arc<WindowSignals>>, current: Option<Epoch>, prepared_epochs: u64,
        best_live: Option<SpanReport>, best_hidden: Option<SpanReport>, destroyed: bool, invalid: bool,
    }
    impl Actor {
        fn new(id: String) -> Self { Self { id, signals: None, current: None, prepared_epochs: 0,
            best_live: None, best_hidden: None, destroyed: false, invalid: false } }
        fn clear_hidden(&mut self) { if let Some(epoch) = &mut self.current { epoch.clear_hidden(); } }
        fn record(&mut self, event: Event, hidden: bool) -> std::result::Result<(), ()> {
            if self.invalid { return Err(()); }
            let result = self.apply(event, hidden); if result.is_err() { self.invalid = true; } result
        }
        fn apply(&mut self, event: Event, hidden: bool) -> std::result::Result<(), ()> {
            if matches!(event, Event::Destroyed) { self.destroyed = true; return Ok(()); }
            let sequence = match event { Event::Prepared { epoch, .. } | Event::Frame { epoch, .. }
                | Event::Cleanup { epoch, .. } | Event::Stop { epoch } => epoch, Event::Destroyed => unreachable!() };
            if sequence == 0 || sequence > crate::life_domain::MAX_COUNTER { return Err(()); }
            // Guarded cleanup is sticky, including a late contradictory result.
            if let Event::Cleanup { outcome: FixtureRestore::Guarded, .. } = event {
                if let Some(current) = &mut self.current { if sequence == current.sequence { current.cleanup = Some(FixtureRestore::Guarded); } }
                return Err(());
            }
            if let Event::Prepared { warm, .. } = event {
                if warm == 0 || warm > LIMIT || self.destroyed { return Err(()); }
                if let Some(old) = &self.current {
                    if sequence < old.sequence { return Ok(()); }
                    if sequence == old.sequence { return if warm == old.warm { Ok(()) } else { Err(()) }; }
                    // Native member sequence can advance only after exact old
                    // physical release; hidden/show cycles never combine spans.
                    if !matches!(old.cleanup, Some(FixtureRestore::Restored | FixtureRestore::Retired | FixtureRestore::WithoutGeometry)) { return Err(()); }
                }
                if self.prepared_epochs >= LIMIT { return Err(()); }
                self.prepared_epochs += 1; self.current = Some(Epoch::new(sequence, warm)); return Ok(());
            }
            let current = self.current.as_mut().ok_or(())?;
            if sequence < current.sequence { return Ok(()); } // Late old work is inert.
            if sequence != current.sequence { return Err(()); }
            match event {
                Event::Frame { frame, received, admitted, live_current, .. } => {
                    if self.destroyed || current.stopped || current.cleanup.is_some() || !live_current
                        || frame <= current.frame || frame > LIMIT || received > LIMIT || admitted > LIMIT || admitted < received || admitted - received > 250
                        || current.accepted >= LIMIT || current.last.is_some_and(|last| admitted < last || admitted - last > MAX_FRAME_GAP_MS) { return Err(()); }
                    current.first.get_or_insert(admitted);
                    if let Some(last) = current.last { current.max_gap = current.max_gap.max(admitted - last); }
                    current.last = Some(admitted); current.frame = frame; current.accepted += 1;
                    current.max_age = current.max_age.max(admitted - received);
                    if hidden { current.hidden_first.get_or_insert(admitted); current.hidden_last = Some(admitted); current.hidden_frames += 1; }
                    else { current.clear_hidden(); }
                    if current.accepted >= 10 { keep_best(&mut self.best_live, current.span(false)); }
                    if current.hidden_frames >= 2 { keep_best(&mut self.best_hidden, current.span(true)); }
                },
                Event::Cleanup { outcome, .. } => {
                    if let Some(old) = current.cleanup { if old != outcome { return Err(()); } }
                    else { current.cleanup = Some(outcome); current.clear_hidden(); }
                },
                Event::Stop { .. } => current.stopped = true,
                _ => unreachable!(),
            }
            Ok(())
        }
        fn report(&self, index: u8) -> ActorReport { ActorReport { actor: index, constructor_observed: self.signals.is_some(),
            prepared_epochs: self.prepared_epochs, current: self.current.as_ref().map(Epoch::report), best_live: self.best_live.clone(),
            best_hidden: self.best_hidden.clone(), destroyed: self.destroyed, invalid: self.invalid } }
    }
    fn keep_best(slot: &mut Option<SpanReport>, candidate: Option<SpanReport>) {
        if let Some(candidate) = candidate { if slot.as_ref().is_none_or(|old| candidate.span_ms > old.span_ms || candidate.epoch == old.epoch && candidate.span_ms == old.span_ms) { *slot = Some(candidate); } }
    }
    fn span(first: Option<u64>, last: Option<u64>) -> u64 { first.zip(last).and_then(|(a,b)| b.checked_sub(a)).unwrap_or(0) }
    struct Facts { actors: [Actor; 2], invalid: bool, hidden_observed_at: Option<u64> }
    struct Fixture { root: Root, facts: Mutex<Facts>, started: Instant, finished: AtomicBool, main_in_flight: AtomicBool, exit_code: Arc<AtomicI32> }
    impl Fixture {
        fn now(&self) -> u64 { self.started.elapsed().as_millis().min(u64::MAX as u128) as u64 }
        fn finish(&self) {
            if self.finished.swap(true, Ordering::SeqCst) { return; }
            let mut report = match self.facts.lock() {
                Ok(facts) => Report::from_facts(&facts, self.now()),
                Err(_) => Report::invalid(self.now()),
            };
            report.main_getter_drained = !self.main_in_flight.load(Ordering::SeqCst);
            if !report.main_getter_drained { report.status = Status::Invalid; }
            let code = if report.status == Status::Passed { 0 } else if report.status == Status::Invalid { 2 } else { 32 };
            self.exit_code.store(if self.root.write_result(&report).is_ok() { code } else { 2 }, Ordering::SeqCst);
        }
    }
    pub(super) fn observe(app: &tauri::AppHandle, bot: &str, signals: &Arc<WindowSignals>, event: Event) {
        let Some(fixture) = app.try_state::<Fixture>() else { return; };
        if fixture.finished.load(Ordering::SeqCst) { return; }
        let Ok(mut facts) = fixture.facts.lock() else { return; };
        let hidden = facts.hidden_observed_at;
        let Some(actor) = facts.actors.iter_mut().find(|actor| actor.id == bot) else { facts.invalid = true; return; };
        match &actor.signals {
            Some(current) if !Arc::ptr_eq(current, signals) => { facts.invalid = true; return; },
            None if matches!(event, Event::Prepared { .. }) => actor.signals = Some(signals.clone()),
            None => { facts.invalid = true; return; }, _ => (),
        }
        if actor.record(event, hidden.is_some_and(|at| fixture.now().saturating_sub(at) <= 2_000)).is_err() { facts.invalid = true; }
    }

    #[derive(Clone, Copy, PartialEq, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum Status { Passed, QualificationIncomplete, Invalid }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct SpanReport { epoch: u64, accepted_frames: u64, first_admitted_ms: u64, last_admitted_ms: u64,
        span_ms: u64, last_frame_sequence: u64, max_frame_gap_ms: u64, max_frame_age_ms: u64 }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct EpochReport { epoch: u64, normal_warm_sequence: u64, accepted_frames: u64, first_admitted_ms: Option<u64>,
        last_admitted_ms: Option<u64>, last_frame_sequence: u64, live_span_ms: u64, max_frame_gap_ms: u64,
        max_frame_age_ms: u64, stopped: bool, cleanup: Option<FixtureRestore> }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct ActorReport { actor: u8, constructor_observed: bool, prepared_epochs: u64, current: Option<EpochReport>,
        best_live: Option<SpanReport>, best_hidden: Option<SpanReport>, destroyed: bool, invalid: bool }
    impl ActorReport {
        fn qualified(&self) -> bool {
            self.constructor_observed && !self.invalid && self.prepared_epochs > 0 && self.destroyed
                && self.current.as_ref().is_some_and(|e| e.stopped && matches!(e.cleanup, Some(FixtureRestore::Restored | FixtureRestore::Retired)))
                && self.best_live.as_ref().is_some_and(|s| s.accepted_frames >= 10 && s.span_ms >= REQUIRED_LIVE_MS && s.max_frame_gap_ms <= MAX_FRAME_GAP_MS && s.max_frame_age_ms <= 250)
                && self.best_hidden.as_ref().is_some_and(|s| s.accepted_frames >= 2 && s.span_ms >= REQUIRED_HIDDEN_MS && s.max_frame_gap_ms <= MAX_FRAME_GAP_MS && s.max_frame_age_ms <= 250)
        }
    }
    #[derive(Serialize)]
    struct Report { version: u16, scope: &'static str, status: Status, duration_ms: u64, credential_attempts: u64, outbound_attempts: u64, denied_commands: u64, diagnostics: FixtureDiagnostics, main_getter_drained: bool, actors: Vec<ActorReport>, remaining: [&'static str; 5] }
    impl Report {
        fn from_facts(facts: &Facts, duration: u64) -> Self {
            let actors = facts.actors.iter().enumerate().map(|(i,a)| a.report(i as u8)).collect::<Vec<_>>();
            let credentials = CREDENTIAL_ATTEMPTS.load(Ordering::SeqCst); let outbound = OUTBOUND_ATTEMPTS.load(Ordering::SeqCst);
            let passed = actors.iter().all(ActorReport::qualified);
            let denied = DENIED_COMMANDS.load(Ordering::SeqCst);
            Self { version: 3, scope: "actual_wry_native_render_lifecycle", status: if facts.invalid || actors.iter().any(|a| a.invalid) || credentials != 0 || outbound != 0 || denied > LIMIT || duration > DEADLINE_MS + 5_000 { Status::Invalid } else if passed { Status::Passed } else { Status::QualificationIncomplete }, duration_ms: duration, credential_attempts: credentials, outbound_attempts: outbound, denied_commands: denied, diagnostics: diagnostics(), main_getter_drained: false, actors, remaining: ["native_action_arrival", "speech_handoff", "obs_cef", "mixed_dpi_lock_suspend", "visible_pixel_attestation"] }
        }
        fn invalid(duration: u64) -> Self { let facts = Facts { actors: [Actor::new(String::new()), Actor::new(String::new())], invalid: true, hidden_observed_at: None }; Self::from_facts(&facts, duration) }
    }

    fn seed(store: &Store) -> Result<[String; 2]> {
        store.save_automatic_update_checks(false).map_err(fixed)?;
        let first = store.profile().map_err(fixed)?; let first_id = first["id"].as_str().ok_or(ERROR)?.to_owned();
        let second = store.create_bot("Fixture Blue", false).map_err(fixed)?; let second_id = second["id"].as_str().ok_or(ERROR)?.to_owned();
        for (id, name) in [(&first_id, "Fixture Nova"), (&second_id, "Fixture Blue")] {
            let scoped = store.scoped(id).map_err(fixed)?; let mut profile = scoped.profile().map_err(fixed)?;
            profile["name"] = json!(name); profile["avatar"]["assetPath"] = Value::Null;
            profile["avatar"]["camera"] = json!("portrait"); profile["avatar"]["scale"] = json!(1);
            profile["model"]["provider"] = json!("compatible"); profile["model"]["model"] = json!(""); profile["model"]["command"] = json!(""); profile["model"]["baseUrl"] = json!("http://127.0.0.1:9/v1"); profile["model"]["apiKeyId"] = json!(""); profile["model"]["toolsEnabled"] = json!(false); profile["model"]["visionEnabled"] = json!(false);
            profile["voice"]["enabled"] = json!(false); profile["voice"]["apiKeyId"] = json!(""); profile["stt"]["apiKeyId"] = json!("");
            profile["autopilot"]["speak"] = json!(false); profile["autopilot"]["gestures"] = json!(false);
            profile["allowSkillScripts"] = json!(false); profile["skillsEnabled"] = json!([]); profile["mcpServers"] = json!([]); profile["motions"] = json!([]);
            profile["screenContext"] = json!({"enabled":false,"displayId":""}); profile["wakeWord"]["enabled"] = json!(false); profile["hindsight"]["enabled"] = json!(false); profile["hindsight"]["apiKeyId"] = json!("");
            profile["chat"]["autoReply"] = json!(false); profile["chat"]["mode"] = json!("manual");
            for (platform, fields) in [("twitch", &["secretId"][..]), ("youtube", &["apiKeyId", "sendSecretId"][..])] { profile["chat"][platform]["sendReplies"] = json!(false); for field in fields { profile["chat"][platform][*field] = json!(""); } }
            profile["output"]["mode"] = json!("studio"); profile["output"]["streamingEnabled"] = json!(false); profile["output"]["fps"] = json!(60);
            scoped.save_profile(profile).map_err(fixed)?;
        }
        store.select_bot(&first_id).map_err(fixed)?;
        let profiles = [normalized_profile(store.bot_profile(&first_id).map_err(fixed)?), normalized_profile(store.bot_profile(&second_id).map_err(fixed)?)];
        if !profiles.iter().all(safe_profile) { return Err(ERROR); }
        SAVE_SEAL.set(SaveSeal { root: store.root.clone(), profiles }).map_err(fixed)?;
        // Participants and placement are deliberately chosen/saved through the
        // real visual editor. Seeding never starts Life or mints UI authority.
        Ok([first_id, second_id])
    }
    fn allowed(command: &str) -> bool {
        matches!(command, "get_app_state" | "list_bots" | "select_bot" | "get_emote_capabilities" | "get_avatar_info" | "get_extension_library" | "get_api_connections" | "get_api_binding" | "get_credential_session" | "get_usage_budget" | "get_ui_locale" | "set_ui_locale" | "get_update_settings" | "list_output_displays" | "get_world_settings" | "get_world_state" | "get_world_semantic_mirrors" | "prepare_native_life_action" | "save_world_config" | "place_world_actor" | "connect_life_ui" | "disconnect_life_ui" | "report_life_ui" | "get_life_runtime" | "get_life_bot" | "set_life_mode" | "start_life_scene" | "stop_life_session_exact" | "stop_generation" | "get_life_actions" | "start_life_rest" | "start_life_move" | "get_world_output_status" | "register_world_output" | "report_world_measurement" | "report_world_action" | "unregister_world_output" | "report_native_world_asset" | "register_native_world_output" | "report_native_world_measurement" | "report_native_world_action" | "unregister_native_world_output" | "report_playback" | "cancel_speech_job" | "report_emote_playback")
    }
    fn invoke(invoke: tauri::ipc::Invoke) -> bool {
        let command = invoke.message.command();
        let operator_stop = command == "stop_generation";
        let guarded_save = command == "save_profile";
        if !(allowed(command) || guarded_save) || (operator_stop || guarded_save) && invoke.message.webview_ref().label() != "main" {
            increment(&DENIED_COMMANDS);
            observe_denied_command(command, (operator_stop || guarded_save) && invoke.message.webview_ref().label() != "main");
            invoke.resolver.reject(ERROR); return true;
        }
        crate::production_invoke(invoke)
    }
    pub(super) fn run() -> Result<i32> {
        if tauri::is_dev() || std::env::args_os().skip(1).next().is_some() { return Err(ERROR); }
        let context = tauri::generate_context!("tauri.native-world-fixture.conf.json");
        // Validate before Builder can construct even the first WKWebView.
        let config = context.config();
        if config.identifier != APP_ID || config.bundle.active || config.build.dev_url.is_some() || config.app.windows.len() != 1 || config.app.windows[0].label != "main" || !config.app.windows[0].incognito || config.app.windows[0].visible { return Err(ERROR); }
        let root = Root::fresh()?; let exit_code = Arc::new(AtomicI32::new(32)); let final_code = exit_code.clone();
        let app = tauri::Builder::default().invoke_handler(invoke).setup(move |app| {
            if app.config().identifier != APP_ID || app.config().bundle.active || app.config().build.dev_url.is_some() || app.config().app.windows.len() != 1 || !app.config().app.windows[0].incognito { return Err(std::io::Error::other(ERROR).into()); }
            root.check().map_err(std::io::Error::other)?;
            let store = Store::new(root.path.join("store")).map_err(|_| std::io::Error::other(ERROR))?;
            let ids = seed(&store).map_err(std::io::Error::other)?;
            app.manage(Fixture { root, facts: Mutex::new(Facts { actors: ids.map(Actor::new), invalid: false, hidden_observed_at: None }), started: Instant::now(), finished: AtomicBool::new(false), main_in_flight: AtomicBool::new(false), exit_code });
            crate::initialize_application(app, store)?;
            let main = app.get_webview_window("main").ok_or_else(|| std::io::Error::other(ERROR))?;
            main.set_title("OrbitPal Native World Fixture")?; main.show()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let fixture = handle.state::<Fixture>();
                    if fixture.finished.load(Ordering::SeqCst) { break; }
                    if fixture.now() >= DEADLINE_MS { handle.exit(32); break; }
                    if fixture.main_in_flight.swap(true, Ordering::SeqCst) { continue; }
                    // finish may have won after the first observation. A
                    // post-finish winner must not queue work after a drained
                    // report; an earlier winner retains this actual charge.
                    if fixture.finished.load(Ordering::SeqCst) {
                        fixture.main_in_flight.store(false, Ordering::SeqCst);
                        break;
                    }
                    let own = handle.clone();
                    if handle.run_on_main_thread(move || {
                        let fixture = own.state::<Fixture>();
                        let hidden = own.get_webview_window("main").and_then(|main| main.is_visible().ok()) == Some(false);
                        if let Ok(mut facts) = fixture.facts.lock() {
                            facts.hidden_observed_at = hidden.then(|| fixture.now());
                            if !hidden { for actor in &mut facts.actors { actor.clear_hidden(); } }
                        }
                        fixture.main_in_flight.store(false, Ordering::SeqCst);
                    }).is_err() { fixture.main_in_flight.store(false, Ordering::SeqCst); if let Ok(mut facts) = fixture.facts.lock() { facts.invalid = true; } }
                }
            });
            Ok(())
        }).build(context).map_err(fixed)?;
        app.run(|app, event| {
            let exiting = matches!(event, tauri::RunEvent::Exit);
            crate::application_exit(app, event);
            if exiting { if let Some(fixture) = app.try_state::<Fixture>() { fixture.finish(); } }
        });
        Ok(final_code.load(Ordering::SeqCst))
    }

    #[cfg(test)]
    mod epoch_tests {
        use super::*;
        #[test]
        fn actual_native_mirror_and_action_preparation_use_production_handlers_without_io_commands() {
            assert!(allowed("get_world_semantic_mirrors"));
            assert!(allowed("prepare_native_life_action"));
            // Profile saves retain their separate sealed main-window guard;
            // credentials and provider/CLI IO remain outside this allowlist.
            for command in ["save_profile", "set_secret", "delete_secret", "set_api_connection_secret", "send_message", "send_automatic_message", "synthesize_speech", "transcribe_audio", "list_models", "check_cli_provider", "list_cli_models"] {
                assert!(!allowed(command), "{command}");
            }
        }
        // Pure local recorder state with authored numeric facts only. These
        // tests do not construct native auth/Source/UI/Store grants or Wry.
        fn actor() -> Actor {
            let mut actor = Actor::new("synthetic".into());
            actor.signals = Some(Arc::new(WindowSignals::default()));
            actor
        }
        fn prepare(actor: &mut Actor, epoch: u64) {
            actor.record(Event::Prepared { epoch, warm: epoch + 3 }, false).unwrap();
        }
        fn frame(actor: &mut Actor, epoch: u64, sequence: u64, at: u64, hidden: bool) {
            actor.record(Event::Frame { epoch, frame: sequence, received: at.saturating_sub(20), admitted: at, live_current: true }, hidden).unwrap();
        }
        fn cleanup(actor: &mut Actor, epoch: u64) {
            actor.record(Event::Cleanup { epoch, outcome: FixtureRestore::Restored }, false).unwrap();
        }
        #[test]
        fn real_owner_epochs_can_qualify_live_and_hidden_spans_separately_without_combining_them() {
            let mut actor = actor(); prepare(&mut actor, 1);
            for i in 0..=12 { frame(&mut actor, 1, i + 1, 100 + i * 3_000, false); }
            cleanup(&mut actor, 1); prepare(&mut actor, 4);
            for i in 0..=4 { frame(&mut actor, 4, i + 20, 50_100 + i * 3_000, true); }
            cleanup(&mut actor, 4); prepare(&mut actor, 9);
            actor.record(Event::Stop { epoch: 9 }, false).unwrap(); cleanup(&mut actor, 9);
            actor.record(Event::Destroyed, false).unwrap();
            let report = actor.report(0);
            assert!(report.qualified()); assert_eq!(report.prepared_epochs, 3);
            assert_eq!(report.best_live.as_ref().unwrap().epoch, 1);
            assert_eq!(report.best_live.as_ref().unwrap().span_ms, 36_000);
            assert_eq!(report.best_hidden.as_ref().unwrap().epoch, 4);
            assert_eq!(report.best_hidden.as_ref().unwrap().span_ms, 12_000);
            assert_eq!(report.current.unwrap().epoch, 9);
        }
        #[test]
        fn two_short_owners_never_bank_a_combined_thirty_five_second_span() {
            let mut actor = actor();
            for epoch in 1..=2 {
                prepare(&mut actor, epoch);
                for i in 0..10 { frame(&mut actor, epoch, i + 1, epoch * 30_000 + i * 2_000, true); }
                actor.record(Event::Stop { epoch }, false).unwrap(); cleanup(&mut actor, epoch);
            }
            actor.record(Event::Destroyed, false).unwrap();
            let report = actor.report(0);
            assert_eq!(report.best_live.unwrap().span_ms, 18_000);
            assert!(!actor.report(0).qualified());
        }
        #[test]
        fn delayed_old_frames_cleanup_and_operator_stop_are_inert_for_a_successor() {
            let mut actor = actor(); prepare(&mut actor, 3); frame(&mut actor, 3, 8, 100, true);
            cleanup(&mut actor, 3); prepare(&mut actor, 7); frame(&mut actor, 7, 9, 900, false);
            let before = actor.report(0);
            actor.record(Event::Frame { epoch: 3, frame: 999, received: 0, admitted: 999_999, live_current: false }, true).unwrap();
            actor.record(Event::Stop { epoch: 3 }, false).unwrap();
            cleanup(&mut actor, 3); prepare(&mut actor, 3);
            assert_eq!(actor.report(0), before);
        }
        #[test]
        fn a_successor_before_exact_old_cleanup_is_sticky_invalid() {
            let mut actor = actor(); prepare(&mut actor, 2);
            assert!(actor.record(Event::Prepared { epoch: 3, warm: 9 }, false).is_err());
            assert!(actor.invalid); assert_eq!(actor.current.as_ref().unwrap().sequence, 2);
            assert!(actor.record(Event::Cleanup { epoch: 2, outcome: FixtureRestore::Restored }, false).is_err());
        }
        #[test]
        fn guarded_cleanup_cannot_be_reset_by_a_new_prepared_or_restored_fact() {
            let mut actor = actor(); prepare(&mut actor, 2);
            assert!(actor.record(Event::Cleanup { epoch: 2, outcome: FixtureRestore::Guarded }, false).is_err());
            assert_eq!(actor.current.as_ref().unwrap().cleanup, Some(FixtureRestore::Guarded));
            assert!(actor.record(Event::Prepared { epoch: 3, warm: 9 }, false).is_err());
            assert!(actor.record(Event::Cleanup { epoch: 2, outcome: FixtureRestore::Restored }, false).is_err());
            assert!(actor.invalid);
        }
        #[test]
        fn even_a_late_old_guarded_result_remains_an_invalid_cleanup_observation() {
            let mut actor = actor(); prepare(&mut actor, 2); cleanup(&mut actor, 2); prepare(&mut actor, 3);
            assert!(actor.record(Event::Cleanup { epoch: 2, outcome: FixtureRestore::Guarded }, false).is_err());
            assert!(actor.invalid); assert_eq!(actor.current.as_ref().unwrap().sequence, 3);
            assert!(actor.record(Event::Stop { epoch: 3 }, false).is_err());
        }
        #[test]
        fn each_owner_resets_only_its_own_clock_range_and_preserves_its_actual_maxima() {
            let mut actor = actor(); prepare(&mut actor, 1);
            frame(&mut actor, 1, 10, 1_000, false); frame(&mut actor, 1, 11, 4_000, false);
            assert_eq!(actor.current.as_ref().unwrap().max_gap, 3_000);
            cleanup(&mut actor, 1); prepare(&mut actor, 5); frame(&mut actor, 5, 1, 80_000, false);
            assert_eq!(actor.current.as_ref().unwrap().first, Some(80_000));
            assert_eq!(actor.current.as_ref().unwrap().max_gap, 0);
            actor.record(Event::Frame { epoch: 5, frame: 2, received: 81_751, admitted: 82_000, live_current: true }, false).unwrap();
            assert_eq!(actor.current.as_ref().unwrap().max_age, 249);
            assert_eq!(actor.current.as_ref().unwrap().max_gap, 2_000);
        }
        #[test]
        fn expired_frame_or_gap_cannot_hide_behind_a_previously_qualified_best_span() {
            for bad_age in [false, true] {
                let mut actor = actor(); prepare(&mut actor, 1);
                for i in 0..=12 { frame(&mut actor, 1, i + 1, 100 + i * 3_000, true); }
                let at = if bad_age { 39_100 } else { 42_100 };
                let received = if bad_age { at - 251 } else { at - 20 };
                assert!(actor.record(Event::Frame { epoch: 1, frame: 99, received, admitted: at, live_current: true }, true).is_err());
                assert!(actor.invalid); assert!(!actor.report(0).qualified());
            }
        }
        #[test]
        fn visibility_breaks_cannot_be_added_to_a_longer_hidden_interval() {
            let mut actor = actor(); prepare(&mut actor, 1);
            for i in 0..=4 { frame(&mut actor, 1, i + 1, 100 + i * 2_000, true); }
            actor.clear_hidden();
            for i in 0..=4 { frame(&mut actor, 1, i + 10, 10_100 + i * 2_000, true); }
            assert_eq!(actor.best_hidden.as_ref().unwrap().span_ms, 8_000);
            assert_eq!(actor.best_hidden.as_ref().unwrap().accepted_frames, 5);
        }
        #[test]
        fn identical_prepared_is_idempotent_but_cannot_retarget_its_warm_fact() {
            let mut actor = actor(); prepare(&mut actor, 1); frame(&mut actor, 1, 8, 100, true);
            let before = actor.report(0); prepare(&mut actor, 1); assert_eq!(actor.report(0), before);
            assert!(actor.record(Event::Prepared { epoch: 1, warm: 999 }, false).is_err());
            assert!(actor.invalid); assert_eq!(actor.prepared_epochs, 1);
        }
        #[test]
        fn old_operator_stop_and_shutdown_destroy_cannot_stop_a_new_owner() {
            let mut actor = actor(); prepare(&mut actor, 1);
            for i in 0..=12 { frame(&mut actor, 1, i + 1, 100 + i * 3_000, true); }
            actor.record(Event::Stop { epoch: 1 }, false).unwrap(); cleanup(&mut actor, 1); prepare(&mut actor, 2);
            actor.record(Event::Stop { epoch: 1 }, false).unwrap(); cleanup(&mut actor, 2);
            actor.record(Event::Destroyed, false).unwrap();
            assert!(!actor.current.as_ref().unwrap().stopped); assert!(!actor.report(0).qualified());
        }
        #[test]
        fn unknown_epoch_and_frames_after_current_cleanup_or_stop_fail_closed() {
            for barrier in [0, 1, 2, 3] {
                let mut actor = actor(); prepare(&mut actor, 1);
                if barrier == 0 { cleanup(&mut actor, 1); }
                if barrier == 1 { actor.record(Event::Stop { epoch: 1 }, false).unwrap(); }
                let epoch = if barrier == 2 { 2 } else if barrier == 3 { 0 } else { 1 };
                assert!(actor.record(Event::Frame { epoch, frame: 1, received: 100, admitted: 110, live_current: true }, false).is_err());
                assert!(actor.invalid); assert_eq!(actor.current.as_ref().unwrap().accepted, 0);
            }
        }
    }

}
