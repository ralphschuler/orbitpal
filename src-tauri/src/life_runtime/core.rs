//! Single native scheduling owner. All time and randomness are explicit; no
//! renderer event can create a session or submit a domain effect.
use crate::{
    life_availability::{Availability, Snapshot as SystemSnapshot},
    life_domain::{LifeState, PublicActor, MAX_COUNTER, MAX_ELAPSED_MS},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
pub const MAX_SESSIONS: usize = 64;
pub const UI_TTL_MS: u64 = 5_000;
const CHECKPOINT_MS: u64 = 30_000;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Ambient,
    Life,
}
#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Ready,
    ModelUnavailable,
    UiUnavailable,
    UiBlocked,
    SystemUnavailable,
    OutputUnavailable,
    StorageUnavailable,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeechConfig {
    pub speak: bool,
    pub min_ms: u64,
    pub max_ms: u64,
}
impl SpeechConfig {
    fn valid(&self) -> bool {
        (15_000..=3_600_000).contains(&self.min_ms)
            && self.max_ms >= self.min_ms
            && self.max_ms <= 3_600_000
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSnapshot {
    pub eligible: bool,
    pub in_flight: bool,
    pub turn_id: Option<String>,
    pub remaining_seconds: u64,
    pub attempts: usize,
    pub limit: usize,
    pub budget_limited: bool,
    pub waiting_for_bot: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub bot_id: String,
    pub mode: Mode,
    pub session_id: String,
    pub revision: u64,
    pub active: bool,
    pub paused: bool,
    pub reason: Reason,
    pub speech: SpeechSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<PublicActor>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub version: u16,
    pub sequence: u64,
    pub system: SystemSnapshot,
    pub sessions: Vec<SessionSnapshot>,
}
#[derive(Clone)]
pub struct Claim {
    pub bot_id: String,
    pub session_id: String,
    pub revision: u64,
    pub turn_id: String,
    pub cancel: Arc<AtomicBool>,
    pub system_generation: u64,
    pub ui_generation: u64,
}
pub struct Session {
    pub mode: Mode,
    pub session_id: String,
    pub revision: u64,
    pub state: Option<LifeState>,
    pub checkpoint: u64,
    pub config: SpeechConfig,
    pub has_model: bool,
    pub output: bool,
    pub storage_failed: bool,
    due: u64,
    last_served: u64,
    random: u64,
}
struct Pending {
    claim: Claim,
    bound: Option<Arc<AtomicBool>>,
    generating: bool,
}
#[derive(Clone)]
pub struct HideBarrier {
    id: String,
    sequence: u64,
    generation: u64,
}
struct Operator {
    connection: Option<String>,
    sequence: u64,
    generation: u64,
    reported_at: Option<u64>,
    blocked: bool,
    hidden: bool,
    hidden_clean: bool,
}
impl Default for Operator {
    fn default() -> Self {
        Self {
            connection: None,
            sequence: 0,
            generation: 0,
            reported_at: None,
            blocked: true,
            hidden: false,
            hidden_clean: false,
        }
    }
}
pub struct Core {
    pub sessions: BTreeMap<String, Session>,
    starts: BTreeMap<String, String>,
    sequence: u64,
    last_time: u64,
    claim_order: u64,
    pending: Option<Pending>,
    ui: Operator,
    system: SystemSnapshot,
    shutdown: bool,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn next(random: &mut u64) -> u64 {
    *random = random.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *random;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
fn due(session: &mut Session, now: u64) -> u64 {
    let span = session.config.max_ms.saturating_sub(session.config.min_ms);
    let wait = session.config.min_ms + next(&mut session.random) % (span + 1);
    now.saturating_add(wait).min(MAX_COUNTER)
}
impl Core {
    pub fn new(system: SystemSnapshot) -> Self {
        Self {
            sessions: BTreeMap::new(),
            starts: BTreeMap::new(),
            sequence: 1,
            last_time: 0,
            claim_order: 0,
            pending: None,
            ui: Operator::default(),
            system,
            shutdown: false,
        }
    }
    fn changed(&mut self) {
        self.sequence = self.sequence.saturating_add(1).min(MAX_COUNTER);
        if self.sequence == MAX_COUNTER {
            self.shutdown = true;
            self.cancel_pending();
        }
    }
    fn clock(&mut self, raw: u64) -> u64 {
        let now = raw.min(MAX_COUNTER).max(self.last_time);
        if self.last_time > 0 && now - self.last_time > MAX_ELAPSED_MS {
            self.cancel_pending();
            self.postpone(now);
        }
        self.last_time = now;
        now
    }
    fn postpone(&mut self, now: u64) {
        for session in self.sessions.values_mut() {
            session.due = due(session, now);
        }
    }
    fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.claim.cancel.store(true, Ordering::SeqCst);
            if let Some(cancel) = pending.bound {
                cancel.store(true, Ordering::SeqCst);
            }
        }
    }
    pub fn activity(&mut self, raw: u64) {
        let now = self.clock(raw);
        self.cancel_pending();
        self.postpone(now);
        self.changed();
    }
    pub fn any_active(&self) -> bool {
        !self.sessions.is_empty() || !self.starts.is_empty()
    }
    pub fn begin_start(&mut self, bot: &str, intent: String) -> Result<(), &'static str> {
        if self.shutdown
            || !valid_id(bot)
            || (!self.starts.contains_key(bot) && self.starts.len() >= MAX_SESSIONS)
        {
            return Err("Der Start der Life-Laufzeit wurde abgelehnt.");
        }
        self.starts.insert(bot.into(), intent);
        self.changed();
        Ok(())
    }
    pub fn start_current(&self, bot: &str, intent: &str) -> bool {
        !self.shutdown && self.starts.get(bot).is_some_and(|value| value == intent)
    }
    pub fn cancel_start(&mut self, bot: &str, intent: &str) {
        if self.start_current(bot, intent) {
            self.starts.remove(bot);
            self.changed();
        }
    }
    pub fn invalidate_bot(&mut self, bot: &str, raw: u64) {
        let now = self.clock(raw);
        self.starts.remove(bot);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.claim.bot_id == bot)
        {
            self.cancel_pending();
        }
        if let Some(session) = self.sessions.get_mut(bot) {
            session.revision = session.revision.saturating_add(1).min(MAX_COUNTER);
            session.due = due(session, now);
        }
        self.changed();
    }
    pub fn session_current(&self, claim: &Claim) -> bool {
        !self.shutdown
            && self.sessions.get(&claim.bot_id).is_some_and(|session| {
                session.session_id == claim.session_id
                    && session.revision == claim.revision
                    && !session.storage_failed
            })
    }
    pub fn session_cloud_current(&self, claim: &Claim, now: u64) -> bool {
        self.session_current(claim)
            && !claim.cancel.load(Ordering::SeqCst)
            && self.system.generation == claim.system_generation
            && self.ui.generation == claim.ui_generation
            && self.ui_clear(now)
    }
    pub fn start(
        &mut self,
        bot: &str,
        mode: Mode,
        id: String,
        config: SpeechConfig,
        state: Option<LifeState>,
        seed: u64,
        has_model: bool,
        output: bool,
        raw: u64,
    ) -> Result<(), &'static str> {
        if self.shutdown || !valid_id(bot) || uuid::Uuid::parse_str(&id).is_err() || !config.valid()
        {
            return Err("Der Start der Life-Laufzeit wurde abgelehnt.");
        }
        if mode == Mode::Life
            && state
                .as_ref()
                .is_none_or(|state| state.actor_id().as_str() != bot || state.validate().is_err())
        {
            return Err("Der Zustand der Life-Laufzeit ist nicht verfügbar.");
        }
        if !self.sessions.contains_key(bot) && self.sessions.len() >= MAX_SESSIONS {
            return Err("Die maximale Anzahl an Life-Sitzungen wurde erreicht.");
        }
        let now = self.clock(raw);
        self.stop(bot, now);
        let mut session = Session {
            mode,
            session_id: id,
            revision: 1,
            state,
            checkpoint: now,
            config,
            has_model,
            output,
            storage_failed: false,
            due: now,
            last_served: 0,
            random: seed,
        };
        session.due = due(&mut session, now);
        self.sessions.insert(bot.into(), session);
        self.changed();
        Ok(())
    }
    pub fn take_session(&mut self, bot: &str, raw: u64) -> Option<Session> {
        let now = self.clock(raw);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.claim.bot_id == bot)
        {
            self.cancel_pending();
            self.postpone(now);
        }
        let removed = self.sessions.remove(bot);
        if removed.is_some() {
            self.changed();
        }
        removed
    }
    pub fn stop(&mut self, bot: &str, raw: u64) -> Option<Session> {
        let now = self.clock(raw);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.claim.bot_id == bot)
        {
            self.cancel_pending();
            self.postpone(now);
        }
        self.starts.remove(bot);
        let removed = self.sessions.remove(bot);
        if removed.is_some() {
            self.changed();
        }
        removed
    }
    pub fn stop_all(&mut self, raw: u64) -> Vec<(String, Session)> {
        self.clock(raw);
        self.cancel_pending();
        self.starts.clear();
        let removed = std::mem::take(&mut self.sessions).into_iter().collect();
        self.changed();
        removed
    }
    pub fn shutdown(&mut self, raw: u64) -> Vec<(String, Session)> {
        self.shutdown = true;
        self.stop_all(raw)
    }
    pub fn reconfigure(
        &mut self,
        bot: &str,
        config: SpeechConfig,
        has_model: bool,
        output: bool,
        raw: u64,
    ) -> Result<(), &'static str> {
        if !config.valid() {
            return Err("Die Konfiguration der Life-Laufzeit wurde abgelehnt.");
        }
        self.starts.remove(bot);
        let now = self.clock(raw);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.claim.bot_id == bot)
        {
            self.cancel_pending();
        }
        if let Some(session) = self.sessions.get_mut(bot) {
            session.revision = session.revision.saturating_add(1).min(MAX_COUNTER);
            session.config = config;
            session.has_model = has_model;
            session.output = output;
            session.due = due(session, now);
        }
        self.changed();
        Ok(())
    }
    pub fn connect_ui(&mut self, id: String, raw: u64) {
        let now = self.clock(raw);
        self.cancel_pending();
        self.ui.connection = Some(id);
        self.ui.sequence = 0;
        self.ui.generation = self.ui.generation.saturating_add(1).min(MAX_COUNTER);
        self.ui.reported_at = None;
        self.ui.blocked = true;
        self.ui.hidden_clean = false;
        self.postpone(now);
        self.changed();
    }
    pub fn disconnect_ui(&mut self, id: &str, raw: u64) {
        if self.ui.connection.as_deref() != Some(id) {
            return;
        }
        let now = self.clock(raw);
        self.cancel_pending();
        self.ui.connection = None;
        self.ui.reported_at = None;
        self.ui.hidden_clean = false;
        self.ui.generation = self.ui.generation.saturating_add(1).min(MAX_COUNTER);
        self.postpone(now);
        self.changed();
    }
    pub fn report_ui(
        &mut self,
        id: &str,
        sequence: u64,
        blocked: bool,
        activity: bool,
        visible: bool,
        raw: u64,
    ) -> bool {
        if self.shutdown
            || self.ui.connection.as_deref() != Some(id)
            || sequence <= self.ui.sequence
            || sequence > MAX_COUNTER
        {
            return false;
        }
        let now = self.clock(raw);
        self.ui.sequence = sequence;
        if self.ui.hidden_clean && !blocked && !activity && !visible {
            return true;
        }
        if visible && self.ui.hidden {
            self.shown(now);
        }
        if !visible {
            if blocked || activity {
                self.operator_activity(now);
            }
            return false;
        }
        let was_clear = self.ui_clear(now);
        self.ui.reported_at = Some(now);
        self.ui.blocked = blocked;
        self.ui.hidden = false;
        self.ui.hidden_clean = false;
        if blocked || activity || !was_clear {
            self.cancel_pending();
            self.postpone(now);
            self.ui.generation = self.ui.generation.saturating_add(1).min(MAX_COUNTER);
        }
        self.changed();
        true
    }
    fn ui_clear(&self, now: u64) -> bool {
        self.ui.hidden_clean
            || (self.ui.connection.is_some()
                && !self.ui.hidden
                && !self.ui.blocked
                && self
                    .ui
                    .reported_at
                    .is_some_and(|at| now.saturating_sub(at) <= UI_TTL_MS))
    }
    pub fn prepare_hide(&self, raw: u64) -> Option<HideBarrier> {
        let now = raw.max(self.last_time);
        if !self.ui_clear(now) || self.ui.hidden {
            return None;
        }
        Some(HideBarrier {
            id: self.ui.connection.clone()?,
            sequence: self.ui.sequence,
            generation: self.ui.generation,
        })
    }
    pub fn hidden(&mut self, barrier: Option<HideBarrier>, actually_hidden: bool, raw: u64) {
        let now = self.clock(raw);
        let clean = actually_hidden
            && barrier.is_some_and(|barrier| {
                self.ui.connection.as_ref() == Some(&barrier.id)
                    && self.ui.sequence == barrier.sequence
                    && self.ui.generation == barrier.generation
                    && self.ui_clear(now)
            });
        self.ui.hidden = actually_hidden;
        self.ui.hidden_clean = clean;
        self.ui.reported_at = None;
        if !clean {
            self.cancel_pending();
        }
        self.postpone(now);
        self.changed();
    }
    pub fn shown(&mut self, raw: u64) {
        let now = self.clock(raw);
        self.ui.hidden = false;
        self.ui.hidden_clean = false;
        self.ui.reported_at = None;
        self.ui.blocked = true;
        self.ui.generation = self.ui.generation.saturating_add(1).min(MAX_COUNTER);
        self.cancel_pending();
        self.postpone(now);
        self.changed();
    }
    pub fn operator_activity(&mut self, raw: u64) {
        let now = self.clock(raw);
        self.ui.hidden_clean = false;
        self.ui.reported_at = None;
        self.ui.blocked = true;
        self.ui.generation = self.ui.generation.saturating_add(1).min(MAX_COUNTER);
        self.cancel_pending();
        self.postpone(now);
        self.changed();
    }
    pub fn observe_system(&mut self, system: SystemSnapshot, raw: u64) {
        let now = self.clock(raw);
        if self.system.generation != system.generation || self.system.state != system.state {
            self.cancel_pending();
            self.postpone(now);
            for session in self.sessions.values_mut() {
                session.checkpoint = now;
            }
            self.changed();
        }
        self.system = system;
    }
    pub fn reconcile_visibility(&mut self, visible: bool, raw: u64) {
        if visible && self.ui.hidden {
            self.shown(raw);
        } else if !visible && !self.ui.hidden {
            self.hidden(None, true, raw);
        }
    }
    pub fn set_output_available(
        &mut self,
        bot: &str,
        session_id: &str,
        revision: u64,
        output: bool,
        raw: u64,
    ) {
        if self.sessions.get(bot).is_none_or(|session| {
            session.session_id != session_id
                || session.revision != revision
                || session.output == output
        }) {
            return;
        }
        let now = self.clock(raw);
        if !output
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.claim.bot_id == bot)
        {
            self.cancel_pending();
        }
        if let Some(session) = self.sessions.get_mut(bot) {
            session.output = output;
            session.due = due(session, now);
        }
        self.changed();
    }
    pub fn mark_storage_failure(&mut self, bot: &str, raw: u64) {
        let now = self.clock(raw);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.claim.bot_id == bot)
        {
            self.cancel_pending();
        }
        if let Some(session) = self.sessions.get_mut(bot) {
            session.storage_failed = true;
            session.revision = session.revision.saturating_add(1).min(MAX_COUNTER);
            session.checkpoint = now;
        }
        self.changed();
    }
    pub fn replace_confirmed(
        &mut self,
        bot: &str,
        session_id: &str,
        revision: u64,
        state: LifeState,
        checkpoint: Option<u64>,
    ) -> bool {
        let Some(session) = self.sessions.get_mut(bot) else {
            return false;
        };
        if session.session_id != session_id
            || session.revision != revision
            || state.actor_id().as_str() != bot
            || state.validate().is_err()
        {
            return false;
        }
        session.state = Some(state);
        if let Some(at) = checkpoint {
            session.checkpoint = at;
        }
        self.changed();
        true
    }
    pub fn checkpoint_jobs(&mut self, raw: u64) -> Vec<(String, String, u64, u64, u64)> {
        let now = self.clock(raw);
        if self.system.state == Availability::SuspendedLocked {
            for session in self.sessions.values_mut() {
                session.checkpoint = now;
            }
            return Vec::new();
        }
        self.sessions
            .iter()
            .filter_map(|(bot, session)| {
                let state = session.state.as_ref()?;
                if session.mode != Mode::Life
                    || session.storage_failed
                    || now.saturating_sub(session.checkpoint) < CHECKPOINT_MS
                {
                    return None;
                }
                Some((
                    bot.clone(),
                    session.session_id.clone(),
                    session.revision,
                    state.revision(),
                    now.saturating_sub(session.checkpoint).min(MAX_ELAPSED_MS),
                ))
            })
            .collect()
    }
    fn reason(&self, session: &Session, now: u64) -> Reason {
        if session.storage_failed {
            return Reason::StorageUnavailable;
        }
        if self.system.state != Availability::AwakeUnlocked {
            return Reason::SystemUnavailable;
        }
        if !self.ui_clear(now) {
            return if self.ui.blocked && self.ui.reported_at.is_some() {
                Reason::UiBlocked
            } else {
                Reason::UiUnavailable
            };
        }
        if !session.output {
            return Reason::OutputUnavailable;
        }
        if !session.has_model || !session.config.speak {
            return Reason::ModelUnavailable;
        }
        Reason::Ready
    }
    pub fn claim(&mut self, raw: u64, blocked: bool, turn_id: String) -> Option<Claim> {
        let now = self.clock(raw);
        if self.shutdown
            || blocked
            || self.pending.is_some()
            || !self.ui_clear(now)
            || self.system.state != Availability::AwakeUnlocked
        {
            if blocked || !self.ui_clear(now) || self.system.state != Availability::AwakeUnlocked {
                self.cancel_pending();
                self.postpone(now);
            }
            return None;
        }
        let bot = self
            .sessions
            .iter()
            .filter(|(_, session)| session.due <= now && self.reason(session, now) == Reason::Ready)
            .min_by_key(|(bot, session)| (session.last_served, session.due, *bot))
            .map(|(bot, _)| bot.clone())?;
        if self.claim_order == MAX_COUNTER {
            return None;
        }
        self.claim_order += 1;
        let session = self.sessions.get_mut(&bot)?;
        session.last_served = self.claim_order;
        let claim = Claim {
            bot_id: bot,
            session_id: session.session_id.clone(),
            revision: session.revision,
            turn_id,
            cancel: Arc::new(AtomicBool::new(false)),
            system_generation: self.system.generation,
            ui_generation: self.ui.generation,
        };
        self.pending = Some(Pending {
            claim: claim.clone(),
            bound: None,
            generating: true,
        });
        self.postpone(now);
        self.changed();
        Some(claim)
    }
    pub fn current(&self, claim: &Claim, raw: u64) -> bool {
        let now = raw.max(self.last_time);
        !self.shutdown
            && !claim.cancel.load(Ordering::SeqCst)
            && self.system.state == Availability::AwakeUnlocked
            && self.system.generation == claim.system_generation
            && self.ui.generation == claim.ui_generation
            && self.ui_clear(now)
            && self.sessions.get(&claim.bot_id).is_some_and(|session| {
                session.session_id == claim.session_id
                    && session.revision == claim.revision
                    && !session.storage_failed
            })
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(&pending.claim.cancel, &claim.cancel))
    }
    pub fn bind(&mut self, claim: &Claim, cancel: Arc<AtomicBool>, raw: u64) -> bool {
        if !self.current(claim, raw) {
            cancel.store(true, Ordering::SeqCst);
            return false;
        }
        if let Some(pending) = self.pending.as_mut() {
            if let Some(existing) = &pending.bound {
                if Arc::ptr_eq(existing, &cancel) {
                    return true;
                }
                cancel.store(true, Ordering::SeqCst);
                return false;
            }
            pending.bound = Some(cancel);
            true
        } else {
            false
        }
    }
    pub fn defer_speech(&mut self, bot: &str, remaining_seconds: u64, raw: u64) {
        let now = self.clock(raw);
        if let Some(session) = self.sessions.get_mut(bot) {
            session.due = session.due.max(
                now.saturating_add(remaining_seconds.saturating_mul(1000))
                    .min(MAX_COUNTER),
            );
        }
    }
    pub fn generated(&mut self, claim: &Claim, raw: u64) {
        if !self.current(claim, raw) {
            return;
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.generating = false;
        }
        self.changed();
    }
    pub fn pending(&self) -> Option<(Claim, bool)> {
        self.pending
            .as_ref()
            .map(|pending| (pending.claim.clone(), pending.generating))
    }
    pub fn finish(&mut self, claim: &Claim, raw: u64) {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| !Arc::ptr_eq(&pending.claim.cancel, &claim.cancel))
        {
            return;
        }
        let now = self.clock(raw);
        self.cancel_pending();
        self.postpone(now);
        self.changed();
    }
    pub fn snapshot(&self, raw: u64) -> RuntimeSnapshot {
        let now = raw.max(self.last_time);
        RuntimeSnapshot {
            version: 1,
            sequence: self.sequence,
            system: self.system,
            sessions: self
                .sessions
                .iter()
                .map(|(bot, session)| {
                    let reason = self.reason(session, now);
                    SessionSnapshot {
                        bot_id: bot.clone(),
                        mode: session.mode,
                        session_id: session.session_id.clone(),
                        revision: session.revision,
                        active: true,
                        paused: session.storage_failed
                            || self.system.state == Availability::SuspendedLocked,
                        reason,
                        speech: SpeechSnapshot {
                            eligible: reason == Reason::Ready,
                            in_flight: self
                                .pending
                                .as_ref()
                                .is_some_and(|pending| pending.claim.bot_id == *bot),
                            turn_id: self
                                .pending
                                .as_ref()
                                .filter(|pending| {
                                    pending.claim.bot_id == *bot
                                        && pending.claim.session_id == session.session_id
                                        && pending.claim.revision == session.revision
                                })
                                .map(|pending| pending.claim.turn_id.clone()),
                            remaining_seconds: session.due.saturating_sub(now).div_ceil(1000),
                            attempts: 0,
                            limit: 0,
                            budget_limited: false,
                            waiting_for_bot: self
                                .pending
                                .as_ref()
                                .filter(|pending| pending.claim.bot_id != *bot)
                                .map(|pending| pending.claim.bot_id.clone()),
                        },
                        actor: session.state.as_ref().map(LifeState::public_projection),
                    }
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life_availability::{Reason as SystemReason, Source};
    use crate::life_domain::{ActorId, Needs, WorldId};
    fn system(state: Availability, generation: u64) -> SystemSnapshot {
        SystemSnapshot {
            state,
            reason: if state == Availability::AwakeUnlocked {
                SystemReason::Ready
            } else {
                SystemReason::ObservationUnavailable
            },
            generation,
            source: Source::MacosCgSessionCompatibility,
        }
    }
    fn runtime() -> Core {
        Core::new(system(Availability::AwakeUnlocked, 1))
    }
    fn config() -> SpeechConfig {
        SpeechConfig {
            speak: true,
            min_ms: 15_000,
            max_ms: 15_000,
        }
    }
    fn start(core: &mut Core, bot: &str, mode: Mode, model: bool) {
        let state = (mode == Mode::Life).then(|| {
            LifeState::new(
                ActorId::new(bot).unwrap(),
                WorldId::new(format!("world_{bot}")).unwrap(),
                Needs::new(5000, 5000, 5000, 5000).unwrap(),
            )
        });
        core.start(
            bot,
            mode,
            uuid::Uuid::new_v4().to_string(),
            config(),
            state,
            7,
            model,
            true,
            0,
        )
        .unwrap();
    }
    fn hidden_clear(core: &mut Core) {
        core.connect_ui("connection".into(), 0);
        assert!(core.report_ui("connection", 1, false, false, true, 0));
        let barrier = core.prepare_hide(0);
        assert!(barrier.is_some());
        core.hidden(barrier, true, 0);
    }
    fn claim(core: &mut Core, now: u64) -> Claim {
        core.claim(now, false, uuid::Uuid::new_v4().to_string())
            .unwrap()
    }
    #[test]
    fn explicit_exclusive_mode_empty_restart_and_life_without_a_model() {
        let mut core = runtime();
        assert!(!core.any_active());
        start(&mut core, "nova", Mode::Life, false);
        assert_eq!(core.sessions["nova"].mode, Mode::Life);
        assert_eq!(core.snapshot(0).sessions[0].reason, Reason::UiUnavailable);
        hidden_clear(&mut core);
        assert!(core
            .claim(15_000, false, uuid::Uuid::new_v4().to_string())
            .is_none());
        assert_eq!(core.checkpoint_jobs(30_000).len(), 1);
        assert!(!runtime().any_active());
        start(&mut core, "nova", Mode::Ambient, true);
        assert_eq!(core.sessions.len(), 1);
        assert_eq!(core.sessions["nova"].mode, Mode::Ambient);
    }
    #[test]
    fn stop_revokes_pending_starts_and_late_same_bot_callbacks() {
        let mut core = runtime();
        core.begin_start("nova", "old".into()).unwrap();
        assert!(core.any_active());
        core.stop("nova", 0);
        assert!(!core.start_current("nova", "old"));
        start(&mut core, "nova", Mode::Ambient, true);
        hidden_clear(&mut core);
        let old = claim(&mut core, 15_000);
        core.stop("nova", 15_000);
        start(&mut core, "nova", Mode::Ambient, true);
        let next = claim(&mut core, 30_000);
        core.finish(&old, 30_000);
        assert!(core.current(&next, 30_000));
        assert!(old.cancel.load(Ordering::SeqCst));
    }
    #[test]
    fn clean_native_hide_survives_missing_heartbeats_but_show_requires_fresh_status() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        hidden_clear(&mut core);
        assert!(core
            .claim(15_000, false, uuid::Uuid::new_v4().to_string())
            .is_some());
        core.shown(15_001);
        assert!(core.pending().is_none());
        assert!(core
            .claim(30_001, false, uuid::Uuid::new_v4().to_string())
            .is_none());
        assert_eq!(
            core.snapshot(30_001).sessions[0].reason,
            Reason::UiUnavailable
        );
    }
    #[test]
    fn blocked_hide_and_expired_visible_status_never_grant() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        core.connect_ui("one".into(), 0);
        core.report_ui("one", 1, true, false, true, 0);
        assert!(core.prepare_hide(0).is_none());
        core.hidden(None, true, 0);
        assert!(core
            .claim(15_000, false, uuid::Uuid::new_v4().to_string())
            .is_none());
        core.shown(15_000);
        core.report_ui("one", 2, false, false, true, 15_000);
        assert!(core
            .claim(30_000, false, uuid::Uuid::new_v4().to_string())
            .is_none());
    }
    #[test]
    fn reconnect_rejects_old_clear_duplicate_and_old_disconnect() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        core.connect_ui("old".into(), 0);
        core.report_ui("old", 1, false, false, true, 0);
        core.connect_ui("new".into(), 1);
        assert!(!core.report_ui("old", 2, false, false, true, 2));
        core.disconnect_ui("old", 3);
        assert!(core.report_ui("new", 1, true, false, true, 4));
        assert!(!core.report_ui("new", 1, false, false, true, 5));
        assert!(core
            .claim(20_000, false, uuid::Uuid::new_v4().to_string())
            .is_none());
    }
    #[test]
    fn foreground_action_racing_clean_hide_invalidates_the_barrier() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        core.connect_ui("one".into(), 0);
        core.report_ui("one", 1, false, false, true, 0);
        let barrier = core.prepare_hide(0);
        core.operator_activity(1);
        core.hidden(barrier, true, 2);
        assert!(core
            .claim(20_000, false, uuid::Uuid::new_v4().to_string())
            .is_none());
        assert_eq!(
            core.snapshot(20_000).sessions[0].reason,
            Reason::UiUnavailable
        );
    }
    #[test]
    fn quiet_turns_are_fair_and_one_global_claim_owns_the_slot() {
        let mut core = runtime();
        for bot in ["c", "b", "a"] {
            start(&mut core, bot, Mode::Ambient, true);
        }
        hidden_clear(&mut core);
        let mut chosen = Vec::new();
        for now in [15_000, 30_000, 45_000, 60_000] {
            let current = claim(&mut core, now);
            chosen.push(current.bot_id.clone());
            assert!(core
                .claim(now, false, uuid::Uuid::new_v4().to_string())
                .is_none());
            core.finish(&current, now);
        }
        assert_eq!(chosen, vec!["a", "b", "c", "a"]);
    }
    #[test]
    fn deferred_checkpoint_accumulates_from_last_success_and_jump_is_bounded() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Life, true);
        let job = core.checkpoint_jobs(30_000).pop().unwrap();
        let state = core.sessions["nova"].state.clone().unwrap();
        core.replace_confirmed("nova", &job.1, job.2, state, None);
        assert_eq!(core.checkpoint_jobs(40_000)[0].4, 40_000);
        let jobs = core.checkpoint_jobs(500_000);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].4, MAX_ELAPSED_MS);
        assert!(core
            .claim(500_000, false, uuid::Uuid::new_v4().to_string())
            .is_none());
    }
    #[test]
    fn unknown_allows_local_ticks_but_not_any_cloud_claim_and_lock_has_no_catchup() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Life, true);
        hidden_clear(&mut core);
        core.observe_system(system(Availability::Unknown, 2), 1);
        assert_eq!(core.checkpoint_jobs(30_001).len(), 1);
        assert!(core
            .claim(30_001, false, uuid::Uuid::new_v4().to_string())
            .is_none());
        core.observe_system(system(Availability::SuspendedLocked, 3), 30_002);
        assert!(core.checkpoint_jobs(90_002).is_empty());
        core.observe_system(system(Availability::AwakeUnlocked, 4), 90_003);
        assert!(core.checkpoint_jobs(90_004).is_empty());
        assert!(core
            .claim(90_004, false, uuid::Uuid::new_v4().to_string())
            .is_none());
    }
    #[test]
    fn native_system_generation_cancels_bound_work_between_provider_rounds() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        hidden_clear(&mut core);
        let claim = claim(&mut core, 15_000);
        let bound = Arc::new(AtomicBool::new(false));
        assert!(core.bind(&claim, bound.clone(), 15_000));
        core.observe_system(system(Availability::SuspendedLocked, 2), 15_001);
        assert!(bound.load(Ordering::SeqCst));
        assert!(!core.current(&claim, 15_001));
        core.observe_system(system(Availability::AwakeUnlocked, 3), 15_002);
        assert!(!core.current(&claim, 15_003));
    }
    #[test]
    fn corrupt_one_session_pauses_only_that_actor_and_public_projection_is_whitelisted() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Life, false);
        start(&mut core, "james", Mode::Life, false);
        core.mark_storage_failure("nova", 0);
        let jobs = core.checkpoint_jobs(30_000);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0, "james");
        let text = serde_json::to_string(&core.snapshot(30_000)).unwrap();
        for forbidden in [
            "needs",
            "energy",
            "random",
            "credentials",
            "PERSONA",
            "recentEvents",
        ] {
            assert!(!text.contains(forbidden));
        }
    }
    #[test]
    fn invalid_reconfigure_is_atomic_and_pending_start_changes_are_revoked() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        hidden_clear(&mut core);
        let current = claim(&mut core, 15_000);
        let before = core.sessions["nova"].revision;
        let invalid = SpeechConfig {
            speak: true,
            min_ms: 0,
            max_ms: u64::MAX,
        };
        assert!(core
            .reconfigure("nova", invalid, true, true, 15_000)
            .is_err());
        assert_eq!(core.sessions["nova"].revision, before);
        assert!(core.current(&current, 15_000));
        core.begin_start("other", "pending-before-profile-await".into())
            .unwrap();
        core.invalidate_bot("other", 15_001);
        assert!(!core.start_current("other", "pending-before-profile-await"));
        core.begin_start("other", "new".into()).unwrap();
        core.cancel_start("other", "old");
        assert!(core.start_current("other", "new"));
    }
    #[test]
    fn binding_is_single_assignment_and_snapshot_exposes_only_current_turn() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        start(&mut core, "other", Mode::Ambient, true);
        hidden_clear(&mut core);
        let current = claim(&mut core, 15_000);
        let first = Arc::new(AtomicBool::new(false));
        let replacement = Arc::new(AtomicBool::new(false));
        assert!(core.bind(&current, first.clone(), 15_000));
        assert!(core.bind(&current, first.clone(), 15_000));
        assert!(!core.bind(&current, replacement.clone(), 15_000));
        assert!(replacement.load(Ordering::SeqCst));
        for session in core.snapshot(15_000).sessions {
            assert_eq!(
                session.speech.turn_id.as_deref(),
                if session.bot_id == current.bot_id {
                    Some(current.turn_id.as_str())
                } else {
                    None
                }
            );
        }
        assert!(core.session_cloud_current(&current, 15_000));
        core.operator_activity(15_001);
        assert!(first.load(Ordering::SeqCst));
        assert!(!core.session_cloud_current(&current, 15_001));
    }
    #[test]
    fn taking_session_for_mode_switch_keeps_only_new_start_intent() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Life, true);
        hidden_clear(&mut core);
        let previous = claim(&mut core, 15_000);
        core.begin_start("nova", "new-mode".into()).unwrap();
        let removed = core.take_session("nova", 15_001).unwrap();
        assert_eq!(removed.mode, Mode::Life);
        assert!(previous.cancel.load(Ordering::SeqCst));
        assert!(core.start_current("nova", "new-mode"));
        assert!(core.sessions.is_empty());
    }
    #[test]
    fn final_session_network_check_rejects_naturally_expired_ui_lease() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        core.connect_ui("studio".into(), 0);
        for (sequence, at) in [(1, 0), (2, 5_000), (3, 10_000), (4, 15_000)] {
            core.report_ui("studio", sequence, false, false, true, at);
        }
        let current = claim(&mut core, 15_000);
        assert!(core.session_cloud_current(&current, 20_000));
        assert!(!core.session_cloud_current(&current, 20_001));
    }
    #[tokio::test]
    async fn held_initial_probe_cannot_restore_profile_changed_or_stopped_start() {
        use std::sync::Mutex;
        for change in ["profile", "stop"] {
            let core = Arc::new(Mutex::new(runtime()));
            core.lock()
                .unwrap()
                .begin_start("nova", "old-intent".into())
                .unwrap();
            let (entered, wait) = tokio::sync::oneshot::channel();
            let (release, resume) = tokio::sync::oneshot::channel();
            let actor = core.clone();
            let task = tokio::spawn(async move {
                entered.send(()).unwrap();
                resume.await.unwrap();
                actor.lock().unwrap().start_current("nova", "old-intent")
            });
            wait.await.unwrap();
            {
                let mut core = core.lock().unwrap();
                if change == "profile" {
                    core.reconfigure(
                        "nova",
                        SpeechConfig {
                            speak: true,
                            min_ms: 15_000,
                            max_ms: 15_000,
                        },
                        true,
                        true,
                        1,
                    )
                    .unwrap();
                } else {
                    core.stop("nova", 1);
                }
                core.begin_start("nova", "new-intent".into()).unwrap();
            }
            release.send(()).unwrap();
            assert!(!task.await.unwrap(), "{change}");
            assert!(core.lock().unwrap().start_current("nova", "new-intent"));
        }
    }
    #[test]
    fn lost_output_revokes_only_that_claim_and_recovery_starts_fresh_quiet() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        start(&mut core, "other", Mode::Ambient, true);
        hidden_clear(&mut core);
        let current = claim(&mut core, 15_000);
        assert_eq!(current.bot_id, "nova");
        core.set_output_available("nova", &current.session_id, current.revision, false, 15_001);
        assert!(current.cancel.load(Ordering::SeqCst));
        assert!(core.pending().is_none());
        assert!(core.sessions["other"].output);
        core.set_output_available("nova", &current.session_id, current.revision, true, 15_002);
        assert_eq!(core.sessions["nova"].due, 30_002);
        // Claim admission itself began a global quiet period at15000.
        let other = claim(&mut core, 30_000);
        assert_eq!(other.bot_id, "other");
        assert_eq!(core.snapshot(30_000).sessions.len(), 2);
    }
    #[test]
    fn stale_output_observation_cannot_overwrite_replacement_session_route() {
        let mut core = runtime();
        start(&mut core, "nova", Mode::Ambient, true);
        let old_id = core.sessions["nova"].session_id.clone();
        let old_revision = core.sessions["nova"].revision;
        core.stop("nova", 1);
        start(&mut core, "nova", Mode::Ambient, true);
        let new_id = core.sessions["nova"].session_id.clone();
        let new_revision = core.sessions["nova"].revision;
        core.set_output_available("nova", &new_id, new_revision, false, 2);
        assert!(!core.sessions["nova"].output);
        core.set_output_available("nova", &old_id, old_revision, true, 3);
        assert!(!core.sessions["nova"].output);
        core.set_output_available("nova", &new_id, new_revision, true, 4);
        assert!(core.sessions["nova"].output);
    }
}
