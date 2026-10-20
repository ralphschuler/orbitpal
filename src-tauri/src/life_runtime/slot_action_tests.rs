//! Shared backend transactions with actual selected-Studio registrations and
//! synthetic frames. These tests do not issue a native scene/surface permit.
use super::*;
use crate::world_state::{Anchor, AnchorId, AnchorKind, Slot, SlotId, WorkIdentity};

fn slot_id() -> SlotId { SlotId::new("meeting-place").unwrap() }

fn slot_context(f: &mut Fixture) -> ActionContext {
    f.ticket += 1;
    let at = f.now.load(Ordering::SeqCst);
    f.world.synchronize(&f.store, &world_hub::current_actors(&f.core),
        Some(crate::world_hub::action_slot_fixture_topology(f.ticket)), at).unwrap();
    f.world.action_context(&f.store, &world_hub::current_actors(&f.core), "nova", at).unwrap()
}

struct SlotFixture(VisualFixture);
impl SlotFixture {
    fn new() -> Self {
        let mut v = VisualFixture::new();
        let record = v.f.store.load_world_config().unwrap().unwrap();
        let config = WorldConfig::new(
            StageId::new("stage").unwrap(),
            Rect::full(),
            vec![ActorId::new("nova").unwrap()],
            vec![Anchor::new(
                AnchorId::new("meeting").unwrap(),
                AnchorKind::Meeting,
                v.target(),
                vec![Slot::new(slot_id(), v.target())],
            ).unwrap()],
        ).unwrap().with_revision(record.config.revision()).unwrap();
        v.f.ticket += 1;
        let confirmed = v.f.world.save(
            &v.f.store,
            Some(record.config.revision()),
            &config,
            &record.output_binding,
            &world_hub::current_actors(&v.f.core),
            crate::world_hub::action_slot_fixture_topology(v.f.ticket),
            0,
        ).unwrap();
        let saved = v.f.store.load_world_config().unwrap().unwrap();
        assert_eq!(saved.config.revision(), record.config.revision() + 1);
        // Read the public settings projection instead of reaching into a
        // private WorldSettings field from this independent fixture module.
        assert_eq!(serde_json::to_value(&confirmed).unwrap()["record"],
            serde_json::to_value(&saved).unwrap());
        // A config change revokes the previous registration. Obtain a real new
        // challenge and accept its actual matching synthetic measurement.
        v.source.context = slot_context(&mut v.f);
        let request = RegisterWorldActor {
            version: 1,
            mount_id: "slot-backend-studio".into(),
            actor_id: "nova".into(),
            world_instance_id: v.source.context.instance_id.clone(),
            stage_id: "stage".into(),
            config_revision: v.source.context.config_revision,
            studio_proof: Some(UiProof { connection_id: "studio".into(), sequence: 1 }),
        };
        let (reply, _, _) = v.f.core.world_outputs.register(&request, v.source.clone(), 0).unwrap();
        let challenge = match reply.readiness {
            Readiness::AwaitingMeasurement { challenge } => challenge,
            _ => panic!("new slot configuration needs its own measurement"),
        };
        v.measurement.challenge_id = challenge.id;
        v.measurement.enclosure_kind = challenge.enclosure_kind;
        v.f.core.world_outputs.measurement(&v.measurement, &v.source, 0, 0).unwrap();
        v.output = v.f.core.world_outputs.ready("nova", 0).unwrap();
        Self(v)
    }
    fn admit(&mut self) -> Result<(), String> {
        let v = &mut self.0;
        let profile = v.f.store.bot_profile("nova").unwrap();
        let context = slot_context(&mut v.f);
        let fence = v.f.core.local_fence("nova", v.f.now.load(Ordering::SeqCst)).unwrap();
        // A same-geometry fresh observation must not invalidate the real
        // measured binding or silently fall back to the old empty topology.
        assert_eq!(context.topology_revision, v.source.context.topology_revision);
        assert!(v.f.world.native_output_geometry(&v.f.store, &context).is_ok());
        crate::life_runtime::action_host::admit_studio_slot_fixture(
            &mut v.f.core, &v.f.store, &v.f.world, "nova", &profile,
            fence, context, v.output.clone(), slot_id(), Arc::new(AtomicBool::new(false)),
            || v.f.now.load(Ordering::SeqCst), || 100 + v.f.now.load(Ordering::SeqCst),
        )
    }
    fn occupied(&self) -> serde_json::Value {
        serde_json::to_value(self.0.f.world.snapshot().unwrap()).unwrap()["snapshot"]["occupied"].clone()
    }
    fn started(&mut self) -> MotionFence {
        self.admit().unwrap();
        assert_eq!(self.occupied().as_array().unwrap().len(), 1);
        let v = &mut self.0;
        let fence = v.offer();
        assert_eq!(v.phase(), ActionPhase::Planned);
        let x = v.output.binding.position.x;
        assert!(matches!(v.report(&fence, 1, 2, ReportKind::Delivered, 10, x, false).outcome, ReportOutcome::Accepted));
        assert_eq!(v.phase(), ActionPhase::Planned);
        assert!(matches!(v.report(&fence, 2, 3, ReportKind::Started, 20, x, false).outcome, ReportOutcome::Accepted));
        assert_eq!(v.phase(), ActionPhase::Started);
        fence
    }
}

#[test]
fn slot_offer_requires_real_plan_start_and_stationary_arrival_then_releases_exact_lease() {
    let mut s = SlotFixture::new();
    let needs = s.0.f.store.load_life("nova").unwrap().unwrap().state().needs();
    let fence = s.started();
    s.0.approach(&fence);
    assert_eq!(s.occupied().as_array().unwrap().len(), 1);
    let x = s.0.output.binding.position.x + 40.0;
    assert!(matches!(s.0.report(&fence, 8, 9, ReportKind::Completed, 670, x, false).outcome, ReportOutcome::Accepted));
    assert_eq!(s.0.phase(), ActionPhase::Completed);
    assert!(s.occupied().as_array().unwrap().is_empty());
    assert_eq!(s.0.f.store.load_life("nova").unwrap().unwrap().state().needs(), needs);
    let context = s.0.f.world.action_context(&s.0.f.store, &world_hub::current_actors(&s.0.f.core), "nova", 670).unwrap();
    assert_eq!(context.position, s.0.target());
    let actual = serde_json::to_value(s.0.f.store.load_life("nova").unwrap().unwrap().state()).unwrap();
    assert!(matches!(s.0.report(&fence, 8, 9, ReportKind::Completed, 680, x, false).outcome, ReportOutcome::Duplicate));
    assert_eq!(serde_json::to_value(s.0.f.store.load_life("nova").unwrap().unwrap().state()).unwrap(), actual);
}

#[test]
fn occupied_slot_cancels_the_real_new_plan_without_offer_or_unrelated_lease_release() {
    let mut s = SlotFixture::new();
    let context = slot_context(&mut s.0.f);
    let independent = s.0.f.world.reserve(&context.session, WorkIdentity::operation("independent-work").unwrap(), &slot_id(), 1000, 0).unwrap();
    let before = s.0.f.store.load_life("nova").unwrap().unwrap();
    assert!(s.admit().is_err());
    let actual = s.0.f.store.load_life("nova").unwrap().unwrap();
    assert_eq!(actual.state().current_action().unwrap().phase(), ActionPhase::Cancelled);
    assert_eq!(actual.state().revision(), before.state().revision() + 2);
    assert_eq!(actual.state().needs(), before.state().needs());
    assert!(s.0.f.core.world_outputs.take_command().is_none());
    assert!(s.0.f.core.actions.as_ref().unwrap().jobs.is_empty());
    assert_eq!(s.occupied().as_array().unwrap().len(), 1);
    assert!(s.0.f.world.release(&context.session, &independent, 1).unwrap());
}

#[test]
fn executor_install_failure_releases_acquired_slot_and_checked_cancels_the_real_plan() {
    let mut s = SlotFixture::new();
    // At the final valid executor tick a new delivery deadline cannot fit.
    // Output/World observations are still current at zero; no action is busy
    // or executor poison present before Plan and real slot acquisition.
    s.0.f.core.actions.as_mut().unwrap().executor.advance(crate::life_domain::MAX_COUNTER).unwrap();
    let actor = ActorId::new("nova").unwrap();
    assert!(s.0.f.core.actions.as_ref().unwrap().executor.can_admit(&actor));
    let before = s.0.f.store.load_life("nova").unwrap().unwrap();
    assert!(s.admit().is_err());
    // Only reaching install_plan's real deadline overflow poisons admission.
    // A preceding reserve failure would leave this executor admissible.
    assert!(!s.0.f.core.actions.as_ref().unwrap().executor.can_admit(&actor));
    let actual = s.0.f.store.load_life("nova").unwrap().unwrap();
    assert_eq!(actual.state().current_action().unwrap().phase(), ActionPhase::Cancelled);
    assert_eq!(actual.state().revision(), before.state().revision() + 2);
    assert_eq!(actual.state().needs(), before.state().needs());
    assert!(s.occupied().as_array().unwrap().is_empty());
    assert!(s.0.f.core.world_outputs.take_command().is_none());
    assert!(s.0.f.core.actions.as_ref().unwrap().jobs.is_empty());
}

#[test]
fn stop_releases_slot_and_late_arrival_cannot_complete_or_move_actor() {
    let mut s = SlotFixture::new();
    let fence = s.started();
    let before = s.0.f.world.action_context(&s.0.f.store, &world_hub::current_actors(&s.0.f.core), "nova", 20).unwrap().position;
    let needs = s.0.f.store.load_life("nova").unwrap().unwrap().state().needs();
    s.0.f.core.stop("nova", 30);
    s.0.f.world.revoke_actor(&s.0.f.store, "nova");
    poll(&mut s.0.f.core, &s.0.f.store, &s.0.f.world, || 30, || 130).unwrap();
    assert_eq!(s.0.phase(), ActionPhase::Cancelled);
    assert!(s.occupied().as_array().unwrap().is_empty());
    let late = make_report(&fence, 3, 4, ReportKind::Completed, s.0.output.binding.position.x + 40.0, s.0.output.binding.position.y, false);
    assert!(crate::life_runtime::action_host::visual_report(&mut s.0.f.core, &s.0.f.store, &s.0.f.world, &late, 40, || 40, || 140).is_err());
    assert_eq!(s.0.phase(), ActionPhase::Cancelled);
    assert_eq!(s.0.f.store.load_life("nova").unwrap().unwrap().state().needs(), needs);
    assert_eq!(crate::world_output::css(before, s.0.output.binding.viewport), s.0.output.binding.position);
}

#[test]
fn slot_start_and_completion_recheck_output_after_an_actual_sqlite_writer() {
    for completing in [false, true] {
        let mut s = SlotFixture::new();
        let needs = s.0.f.store.load_life("nova").unwrap().unwrap().state().needs();
        let (fence, seq, frame, kind, at, x) = if completing {
            let fence = s.started();
            s.0.approach(&fence);
            (fence, 8, 9, ReportKind::Completed, 670, s.0.output.binding.position.x + 40.0)
        } else {
            s.admit().unwrap();
            let fence = s.0.offer();
            let x = s.0.output.binding.position.x;
            assert!(matches!(s.0.report(&fence, 1, 2, ReportKind::Delivered, 10, x, false).outcome, ReportOutcome::Accepted));
            (fence, 2, 3, ReportKind::Started, 20, x)
        };
        let report = make_report(&fence, seq, frame, kind, x, s.0.output.binding.position.y, false);
        let path = s.0.f.root.join("cohost.sqlite3");
        let output = s.0.output.cancel.clone();
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (commit_tx, commit_rx) = std::sync::mpsc::channel();
        let signal = AtomicBool::new(true);
        std::thread::scope(|scope| {
            let writer = scope.spawn(move || {
                let db = rusqlite::Connection::open(path).unwrap();
                db.execute_batch("BEGIN IMMEDIATE").unwrap();
                held_tx.send(()).unwrap();
                commit_rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
                output.store(true, Ordering::SeqCst);
                db.execute_batch("COMMIT").unwrap();
            });
            held_rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
            // wall() is evaluated at the checked commit boundary, after its
            // preceding output and exact-slot checks. The held writer revokes
            // the output before SQLite can admit Start/Complete.
            let ack = crate::life_runtime::action_host::visual_report(
                &mut s.0.f.core, &s.0.f.store, &s.0.f.world, &report, at, || at,
                || {
                    if signal.swap(false, Ordering::SeqCst) { commit_tx.send(()).unwrap(); }
                    100 + at
                },
            ).unwrap();
            writer.join().unwrap();
            assert!(!signal.load(Ordering::SeqCst), "must reach actual checked commit");
            assert!(matches!(ack.outcome, ReportOutcome::Cancelled));
        });
        poll(&mut s.0.f.core, &s.0.f.store, &s.0.f.world, || at, || 100 + at).unwrap();
        assert_eq!(s.0.phase(), ActionPhase::Cancelled);
        assert_eq!(s.0.f.store.load_life("nova").unwrap().unwrap().state().needs(), needs);
        assert!(s.occupied().as_array().unwrap().is_empty());
    }
}
