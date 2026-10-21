//! Stateful scheduling/cancellation tests only. Synthetic attempt keys/atoms
//! are not ConstructorReceipts, UI proofs, Source factories or native grants.
use super::*;
use crate::{life_domain::{ActorId, WorldId}, world_state::{SessionStamp, StageId, coordinates::Point}};
use std::sync::Barrier;

struct Intent { bot: String, key: u64, cancel: AtomicBool, internal: AtomicBool }
impl Intent {
    fn new(bot: &str, key: u64) -> Arc<Self> { Arc::new(Self { bot: bot.into(), key, cancel: AtomicBool::new(false), internal: AtomicBool::new(false) }) }
    fn external(&self) { self.cancel.store(true, Ordering::SeqCst); }
}
impl AttemptState for Intent {
    fn bot(&self) -> &str { &self.bot }
    fn same_key(&self, other: &Self) -> bool { self.key == other.key }
    fn retire(&self) { self.internal.store(true, Ordering::SeqCst); self.cancel.store(true, Ordering::SeqCst); }
    fn external_cancelled(&self) -> bool { self.cancel.load(Ordering::SeqCst) && !self.internal.load(Ordering::SeqCst) }
}
fn state() -> State<Intent> { State::default() }

#[test]
fn unchanged_failed_context_cannot_replay_after_hints_or_clean_polls() {
    let mut state = state(); let first = Intent::new("nova", 1);
    let serial = state.admit(first.clone(), true, 0).unwrap();
    first.retire(); state.finish("nova", serial, Phase::Unavailable);
    for _ in 0..64 {
        state.observe_external();
        assert!(state.admit(Intent::new("nova", 1), true, 0).is_none());
    }
    assert_eq!(state.cells["nova"].serial, serial);
    assert_eq!(state.cells["nova"].resume, 0);
    assert!(state.admit(Intent::new("nova", 2), true, 0).is_some());
}
#[test]
fn one_external_revocation_advances_resume_once_after_owned_cleanup() {
    let mut state = state(); let first = Intent::new("nova", 1);
    let serial = state.admit(first.clone(), true, 0).unwrap();
    first.external();
    for _ in 0..4 { state.observe_external(); }
    assert_eq!(state.cells["nova"].resume, 0); // worker/cleanup still owns it
    state.finish("nova", serial, Phase::Unavailable);
    state.observe_external(); let resume = state.cells["nova"].resume;
    assert!(resume > serial);
    for _ in 0..8 { state.observe_external(); }
    assert_eq!(state.cells["nova"].resume, resume);
    assert!(state.admit(Intent::new("nova", 2), true, resume).is_some());
}
#[test]
fn asset_timeout_cancelling_shared_atom_is_internal_before_finish() {
    let mut state = state(); let first = Intent::new("nova", 1);
    let serial = state.admit(first.clone(), true, 0).unwrap();
    // The document checker may set its latch before returning Expired. While
    // running this cannot be mistaken for a completed external transition.
    first.external(); state.observe_external();
    assert_eq!(state.cells["nova"].resume, 0);
    first.retire(); state.finish("nova", serial, Phase::Unavailable);
    state.observe_external();
    assert_eq!(state.cells["nova"].resume, 0);
    assert!(state.admit(Intent::new("nova", 1), true, 0).is_none());
}
#[test]
fn pending_successor_does_not_erase_old_worker_or_cleanup_charge() {
    let mut state = state(); let old = Intent::new("nova", 1);
    let serial = state.admit(old.clone(), true, 0).unwrap();
    old.retire();
    let waiting = Intent::new("nova", 2);
    assert!(state.admit(waiting.clone(), true, 0).is_none());
    assert!(waiting.internal.load(Ordering::SeqCst));
    assert_eq!(state.cells["nova"].serial, serial);
    assert!(state.cells["nova"].running);
    state.finish("nova", serial, Phase::Unavailable);
    let next = state.admit(Intent::new("nova", 2), true, 0).unwrap();
    assert!(next > serial);
}
#[test]
fn late_old_completion_cannot_mark_successor_ready_or_release_it() {
    let mut state = state();
    let old = state.admit(Intent::new("nova", 1), true, 0).unwrap();
    state.finish("nova", old, Phase::Unavailable);
    let next = state.admit(Intent::new("nova", 2), true, 0).unwrap();
    state.finish("nova", old, Phase::Ready);
    state.finish("nova", old, Phase::Unavailable);
    assert_eq!(state.cells["nova"].serial, next);
    assert!(state.cells["nova"].running);
    assert!(state.cells["nova"].phase == Phase::Releasing);
    assert!(!state.cells["nova"].cleanup_done);
}
#[test]
fn busy_cleanliness_wait_does_not_consume_the_context_attempt() {
    let mut state = state();
    assert!(state.admit(Intent::new("nova", 1), false, 0).is_none());
    assert!(state.cells.is_empty()); assert_eq!(state.serial, 0);
    let serial = state.admit(Intent::new("nova", 1), true, 0).unwrap();
    assert_eq!(serial, 1);
}
#[test]
fn eight_retained_attempts_bound_pending_and_completed_failures() {
    let mut state = state();
    for i in 0..MAX_MEMBERS {
        let bot = format!("actor-{i}");
        let serial = state.admit(Intent::new(&bot, 1), true, 0).unwrap();
        if i % 2 == 0 { state.finish(&bot, serial, Phase::Unavailable); }
    }
    let excess = Intent::new("actor-8", 1);
    assert!(state.admit(excess.clone(), true, 0).is_none());
    assert!(excess.internal.load(Ordering::SeqCst));
    assert_eq!(state.cells.len(), MAX_MEMBERS);
    // A replacement of an existing completed slot does not require slot nine.
    assert!(state.admit(Intent::new("actor-0", 2), true, 0).is_some());
}
#[test]
fn exhausted_counter_cancels_every_retained_attempt_and_stays_unavailable() {
    let mut state = state(); let live = Intent::new("nova", 1);
    state.admit(live.clone(), true, 0).unwrap(); state.serial = MAX_COUNTER;
    assert!(state.admit(Intent::new("other", 1), true, 0).is_none());
    assert!(state.unavailable); assert!(live.cancel.load(Ordering::SeqCst));
    assert!(state.admit(Intent::new("nova", 2), true, 0).is_none());
    assert_eq!(state.cells.len(), 1);
}
#[test]
fn two_actor_completion_is_independent_and_does_not_reset_the_other() {
    let mut state = state();
    let a = state.admit(Intent::new("nova", 1), true, 0).unwrap();
    let b = state.admit(Intent::new("blue", 1), true, 0).unwrap();
    state.finish("nova", a, Phase::Ready);
    assert!(state.cells["blue"].running);
    assert!(!state.cells["blue"].cleanup_done);
    state.finish("blue", b, Phase::Unavailable);
    assert!(state.cells["nova"].phase == Phase::Ready);
    assert!(!state.cells["nova"].running);
}
#[test]
fn competing_current_context_admissions_keep_one_real_worker() {
    let state = Arc::new(Mutex::new(state())); let start = Arc::new(Barrier::new(3));
    let joins = (0..2).map(|_| { let state = state.clone(); let start = start.clone();
        std::thread::spawn(move || { start.wait(); state.lock().unwrap().admit(Intent::new("nova", 1), true, 0) })
    }).collect::<Vec<_>>();
    start.wait();
    let admitted = joins.into_iter().map(|j| j.join().unwrap()).filter(Option::is_some).count();
    assert_eq!(admitted, 1);
    let state = state.lock().unwrap(); assert_eq!(state.cells.len(), 1); assert!(state.cells["nova"].running);
}

fn key() -> AttemptKey {
    AttemptKey { route: "lease-a".into(), lease: 1, signals: 2,
        local: LocalFence { session: "session-a".into(), revision: 1, system_generation: 1, ui_generation: 1 },
        world: ActionContext { session: SessionStamp::new(ActorId::new("nova").unwrap(), "session-a", 1).unwrap(),
            source_world: WorldId::new("life-a").unwrap(), stage: StageId::new("stage").unwrap(), config_revision: 1,
            topology_revision: 1, position: Point::new(100, 200).unwrap(), instance_id: "instance-a".into(), record: serde_json::json!({"revision":1}) },
        profile: [3; 32], load: 4, dialog: 0, resume: 0 }
}
#[test]
fn accepted_world_position_updates_do_not_trigger_surface_or_action_replay() {
    let first = key(); let mut moved = first.clone();
    moved.world.position = Point::new(800_000, 900_000).unwrap();
    assert!(first == moved);
    moved.world.config_revision += 1; assert!(first != moved);
}
#[test]
fn admission_context_distinguishes_real_lease_load_profile_and_dialog_changes() {
    let original = key();
    let changes: Vec<Box<dyn Fn(&mut AttemptKey)>> = vec![
        Box::new(|k| k.lease += 1), Box::new(|k| k.signals += 1), Box::new(|k| k.route.push('b')),
        Box::new(|k| k.load += 1), Box::new(|k| k.profile[0] += 1), Box::new(|k| k.dialog += 1),
        Box::new(|k| k.resume += 1), Box::new(|k| k.local.ui_generation += 1),
        Box::new(|k| k.local.system_generation += 1), Box::new(|k| k.local.revision += 1),
        Box::new(|k| k.world.topology_revision += 1), Box::new(|k| k.world.instance_id.push('b')),
        Box::new(|k| k.world.record["revision"] = 2.into()),
    ];
    for change in changes { let mut changed = original.clone(); change(&mut changed); assert!(original != changed); }
}

#[test]
fn actual_started_finished_order_waits_without_consuming_a_start_attempt() {
    let docs = super::super::native_document::Documents::default();
    let signals = Arc::new(crate::window_signals::WindowSignals::default());
    assert!(docs.track("nova", &signals)); assert!(docs.started("nova", &signals));
    assert!(docs.loaded("nova", &signals).is_err());
    let mut state = state();
    assert!(state.admit(Intent::new("nova", 1), false, 0).is_none());
    let _grant = docs.finished("nova", &signals, 10).unwrap();
    let loaded = docs.loaded("nova", &signals).unwrap();
    assert!(loaded.current());
    let serial = state.admit(Intent::new("nova", loaded.load_id()), true, 0).unwrap();
    let _same = docs.finished("nova", &signals, 500).unwrap();
    let duplicate = docs.loaded("nova", &signals).unwrap();
    assert!(duplicate.same_loaded(&loaded));
    assert!(state.admit(Intent::new("nova", duplicate.load_id()), true, 0).is_none());
    assert_eq!(state.cells["nova"].serial, serial);
}
#[test]
fn semantic_capability_rotation_is_not_a_new_loaded_context_but_real_navigation_is() {
    let docs = super::super::native_document::Documents::default();
    let signals = Arc::new(crate::window_signals::WindowSignals::default());
    assert!(docs.track("nova", &signals)); assert!(docs.started("nova", &signals));
    let first = docs.finished("nova", &signals, 1).unwrap();
    let physical = docs.loaded("nova", &signals).unwrap();
    let rotated = docs.rotate("nova", &signals, 2).unwrap();
    assert_ne!(first.capability(), rotated.capability());
    let same = docs.loaded("nova", &signals).unwrap();
    assert!(same.same_loaded(&physical)); assert_eq!(same.load_id(), physical.load_id());
    assert!(physical.current());
    assert!(docs.started("nova", &signals)); assert!(!physical.current());
    assert!(docs.loaded("nova", &signals).is_err());
    let _next = docs.finished("nova", &signals, 3).unwrap();
    let next = docs.loaded("nova", &signals).unwrap();
    assert!(!next.same_loaded(&physical)); assert_ne!(next.load_id(), physical.load_id());
}

#[test]
fn actual_claim_early_generation_failure_reconciles_once_without_renderer_hot_retry() {
    use super::super::core::{Core, Mode, SpeechConfig};
    use crate::{life_availability::{Availability, Reason, Snapshot, Source}, life_domain::{LifeState, Needs}};
    let mut core = Core::new(Snapshot { state: Availability::AwakeUnlocked, reason: Reason::Ready,
        generation: 1, source: Source::MacosCgSessionCompatibility });
    core.start("nova", Mode::Life, uuid::Uuid::new_v4().to_string(),
        SpeechConfig { speak: true, min_ms: 15_000, max_ms: 15_000 },
        Some(LifeState::new(ActorId::new("nova").unwrap(), WorldId::new("world_nova").unwrap(),
            Needs::new(5000, 5000, 5000, 5000).unwrap())), 7, true, true, 0).unwrap();
    core.connect_ui("studio".into(), 0);
    assert!(core.report_ui("studio", 1, false, false, true, 0));
    let mut state = state();
    let epoch = state.dialog;
    let before = Intent::new("nova", epoch);
    let serial = state.admit(before.clone(), true, 0).unwrap();
    state.finish("nova", serial, Phase::Ready); // Scheduler state only, no native proof.
    // An unsuccessful scheduling check is not a transition and cannot renew it.
    if core.claim(0, false, "not-due".into()).is_some() { state.transition_started(); }
    assert_eq!(state.dialog, epoch);
    for (sequence, at) in [(2, 5_000), (3, 10_000), (4, 15_000)] {
        assert!(core.report_ui("studio", sequence, false, false, true, at));
    }
    let fence = core.local_fence("nova", 15_000).unwrap();
    let claim = core.claim(15_000, false, "accepted".into()).unwrap();
    state.transition_started(); // Same production State method called by claim_started.
    assert!(core.local_fence("nova", 15_000).is_none());
    assert!(before.internal.load(Ordering::SeqCst));
    assert!(state.dialog > epoch);
    // The actual agent can lose this try_lock before reaching dialog quiesce.
    let generation = tokio::sync::Mutex::new(()); let held = generation.try_lock().unwrap();
    assert!(generation.try_lock().is_err()); drop(held);
    core.generated(&claim, 15_001); // The existing agent wrapper does this on Err too.
    assert!(core.pending().is_some_and(|(_, generating)| !generating));
    assert!(core.local_fence("nova", 15_002).is_none());
    // The lifecycle pulse/controlled exact cleanup runs before Core finishes it.
    state.finish("nova", serial, Phase::Unavailable);
    state.observe_external();
    assert_eq!(state.cells["nova"].resume, 0);
    core.finish(&claim, 15_002);
    assert!(core.pending().is_none());
    assert!(core.local_fence("nova", 15_002).as_ref() == Some(&fence));
    let resumed_epoch = state.dialog;
    let next = Intent::new("nova", resumed_epoch);
    let resumed = state.admit(next.clone(), true, 0).unwrap();
    assert!(resumed > serial);
    state.finish("nova", resumed, Phase::Ready);
    assert!(state.admit(Intent::new("nova", resumed_epoch), true, 0).is_none());
    // A genuine renderer failure still consumes this unchanged context once;
    // ordinary clean polls cannot turn the new transition into retry authority.
    next.retire(); state.finish("nova", resumed, Phase::Unavailable);
    for _ in 0..8 {
        state.observe_external();
        assert!(state.admit(Intent::new("nova", resumed_epoch), true, 0).is_none());
    }
    assert_eq!(state.dialog, resumed_epoch);
}
