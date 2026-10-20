//! Exercises the production Core -> WorldHub -> checked SQLite owner with
//! synthetic renderer observations. This is not a WebGL/platform acceptance run.
use super::*;
use crate::world_output::*;
use crate::world_state::coordinates::Point;
struct VisualFixture {
    f: Fixture,
    source: NativeSource,
    output: OutputProof,
    measurement: RenderMeasurement,
}
impl VisualFixture {
    fn new() -> Self {
        let mut f = Fixture::new(&["nova"]);
        let context = f.context("nova");
        let profile = f.store.bot_profile("nova").unwrap();
        let source = NativeSource {
            context,
            profile_token: f.store.action_profile_token("nova", &profile).unwrap(),
            ui_connection: "studio".into(),
            layout_token: json!({"width":800,"height":600,"scale":1}),
            viewport_limit: CssSize {
                width: 800.0,
                height: 600.0,
            },
            safe_region: Rect::full(),
            profile_scale: 1.0,
            effective_scale: 1.0,
            native_walk: true,
            owner: true,
        };
        let request = RegisterWorldActor {
            version: 1,
            mount_id: "mount".into(),
            actor_id: "nova".into(),
            world_instance_id: source.context.instance_id.clone(),
            stage_id: "stage".into(),
            config_revision: source.context.config_revision,
            studio_proof: Some(UiProof {
                connection_id: "studio".into(),
                sequence: 1,
            }),
        };
        let (reply, _, _) = f
            .core
            .world_outputs
            .register(&request, source.clone(), 0)
            .unwrap();
        let challenge = match reply.readiness {
            Readiness::AwaitingMeasurement { challenge } => challenge,
            _ => panic!("measurement challenge"),
        };
        let binding_position = crate::world_output::css(
            source.context.position,
            CssSize {
                width: 800.0,
                height: 600.0,
            },
        );
        let report = RenderMeasurement {
            version: 1,
            challenge_id: challenge.id,
            frame_sequence: 1,
            viewport: CssSize {
                width: 800.0,
                height: 600.0,
            },
            clip: CssRect {
                left: 0.0,
                top: 0.0,
                right: 800.0,
                bottom: 600.0,
            },
            anchor: binding_position,
            posed_bounds: CssRect {
                left: binding_position.x - 10.0,
                top: binding_position.y - 30.0,
                right: binding_position.x + 10.0,
                bottom: binding_position.y,
            },
            envelope_extents: EnvelopeExtents {
                left: 20.0,
                top: 40.0,
                right: 30.0,
                bottom: 10.0,
            },
            reference_local: LocalReference {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            ground_basis: GroundBasis {
                right: GroundVector {
                    x: 8.0 / 600.0,
                    z: 0.0,
                },
                down: GroundVector {
                    x: 0.0,
                    z: 16.0 / 600.0,
                },
            },
            model_forward_yaw: 0.0,
            applied_model_scale: 1.0,
            reduced_motion: false,
            walk_supported: true,
        };
        f.core
            .world_outputs
            .measurement(&report, &source, 0, 0)
            .unwrap();
        let output = f.core.world_outputs.ready("nova", 0).unwrap();
        Self {
            f,
            source,
            output,
            measurement: report,
        }
    }
    fn target(&self) -> Point {
        let p = self.source.context.position;
        Point::new(p.x() + 50_000, p.y()).unwrap()
    }
    fn admit(&mut self, target: Point) -> Result<(), String> {
        let profile = self.f.store.bot_profile("nova").unwrap();
        let context = self.f.context("nova");
        let fence = self
            .f
            .core
            .local_fence("nova", self.f.now.load(Ordering::SeqCst))
            .unwrap();
        crate::life_runtime::action_host::admit_move(
            &mut self.f.core,
            &self.f.store,
            &self.f.world,
            "nova",
            &profile,
            fence,
            context,
            self.output.clone(),
            target,
            Arc::new(AtomicBool::new(false)),
            || self.f.now.load(Ordering::SeqCst),
            || 100 + self.f.now.load(Ordering::SeqCst),
        )
    }
    fn offer(&mut self) -> MotionFence {
        match self.f.core.world_outputs.take_command().unwrap() {
            MotionCommand::Offer {
                fence, activity, ..
            } => {
                assert_eq!(activity, "move_to");
                fence
            }
            _ => panic!("expected actual native Offer"),
        }
    }
    fn report(
        &mut self,
        fence: &MotionFence,
        seq: u64,
        frame_seq: u64,
        kind: ReportKind,
        at: u64,
        x: f64,
        walking: bool,
    ) -> ActionReportAck {
        self.f.now.store(at, Ordering::SeqCst);
        let report = make_report(
            fence,
            seq,
            frame_seq,
            kind,
            x,
            crate::world_output::css(self.source.context.position, self.output.binding.viewport).y,
            walking,
        );
        let ack = crate::life_runtime::action_host::visual_report(
            &mut self.f.core,
            &self.f.store,
            &self.f.world,
            &report,
            at,
            || self.f.now.load(Ordering::SeqCst),
            || 100 + at,
        )
        .unwrap();
        poll(
            &mut self.f.core,
            &self.f.store,
            &self.f.world,
            || self.f.now.load(Ordering::SeqCst),
            || 100 + at,
        )
        .unwrap();
        ack
    }
    fn phase(&self) -> ActionPhase {
        self.f
            .store
            .load_life("nova")
            .unwrap()
            .unwrap()
            .state()
            .current_action()
            .unwrap()
            .phase()
    }
    fn get_started(&mut self) -> MotionFence {
        self.admit(self.target()).unwrap();
        let fence = self.offer();
        let x = self.output.binding.position.x;
        assert!(matches!(
            self.report(&fence, 1, 2, ReportKind::Delivered, 10, x, false)
                .outcome,
            ReportOutcome::Accepted
        ));
        assert_eq!(self.phase(), ActionPhase::Planned);
        assert!(matches!(
            self.report(&fence, 2, 3, ReportKind::Started, 20, x, false)
                .outcome,
            ReportOutcome::Accepted
        ));
        assert_eq!(self.phase(), ActionPhase::Started);
        fence
    }
    fn approach(&mut self, fence: &MotionFence) {
        let x = self.output.binding.position.x;
        for (seq, frame, at, px, walk) in [
            (3, 4, 220, x + 7.2, true),
            (4, 5, 420, x + 28.0, true),
            (5, 6, 520, x + 40.0, true),
            (6, 7, 570, x + 40.0, false),
            (7, 8, 620, x + 40.0, false),
        ] {
            assert!(matches!(
                self.report(fence, seq, frame, ReportKind::Progress, at, px, walk)
                    .outcome,
                ReportOutcome::Accepted
            ));
        }
    }
}
fn make_report(
    fence: &MotionFence,
    seq: u64,
    frame_seq: u64,
    kind: ReportKind,
    x: f64,
    y: f64,
    walking: bool,
) -> ActionReport {
    let frame = FrameObservation {
        frame_sequence: frame_seq,
        viewport: CssSize {
            width: 800.0,
            height: 600.0,
        },
        clip: CssRect {
            left: 0.0,
            top: 0.0,
            right: 800.0,
            bottom: 600.0,
        },
        anchor: CssPoint { x, y },
        posed_bounds: CssRect {
            left: x - 10.0,
            top: y - 30.0,
            right: x + 10.0,
            bottom: y,
        },
        walking,
        target_idle_frames: u16::MAX,
    };
    match kind {
        ReportKind::Delivered => ActionReport::Delivered {
            version: 1,
            fence: fence.clone(),
            receipt_sequence: seq,
            frame,
        },
        ReportKind::Started => ActionReport::Started {
            version: 1,
            fence: fence.clone(),
            receipt_sequence: seq,
            frame,
        },
        ReportKind::Progress => ActionReport::Progress {
            version: 1,
            fence: fence.clone(),
            receipt_sequence: seq,
            frame,
        },
        ReportKind::Completed => ActionReport::Completed {
            version: 1,
            fence: fence.clone(),
            receipt_sequence: seq,
            frame,
        },
    }
}
#[test]
fn studio_move_real_plan_delivery_start_progress_complete_and_exact_retry() {
    let mut v = VisualFixture::new();
    let needs =
        v.f.store
            .load_life("nova")
            .unwrap()
            .unwrap()
            .state()
            .needs();
    let fence = v.get_started();
    v.approach(&fence);
    let x = v.output.binding.position.x + 40.0;
    let ack = v.report(&fence, 8, 9, ReportKind::Completed, 670, x, false);
    assert!(matches!(ack.outcome, ReportOutcome::Accepted));
    assert_eq!(v.phase(), ActionPhase::Completed);
    assert_eq!(
        v.f.store
            .load_life("nova")
            .unwrap()
            .unwrap()
            .state()
            .needs(),
        needs
    );
    let before =
        serde_json::to_value(v.f.store.load_life("nova").unwrap().unwrap().state()).unwrap();
    assert!(matches!(
        v.report(&fence, 8, 9, ReportKind::Completed, 680, x, false)
            .outcome,
        ReportOutcome::Duplicate
    ));
    assert_eq!(
        before,
        serde_json::to_value(v.f.store.load_life("nova").unwrap().unwrap().state()).unwrap()
    );
    assert!(v.f.core.actions.as_ref().unwrap().jobs.is_empty());
    let actor =
        v.f.world
            .action_context(
                &v.f.store,
                &world_hub::current_actors(&v.f.core),
                "nova",
                680,
            )
            .unwrap();
    assert_eq!(actor.position, v.target());
}
#[test]
fn no_clamp_rejects_target_before_plan_or_ordinal() {
    let mut v = VisualFixture::new();
    let before =
        serde_json::to_value(v.f.store.load_life("nova").unwrap().unwrap().state()).unwrap();
    assert!(v.admit(Point::new(0, v.target().y()).unwrap()).is_err());
    assert_eq!(
        before,
        serde_json::to_value(v.f.store.load_life("nova").unwrap().unwrap().state()).unwrap()
    );
    assert!(v.f.core.world_outputs.take_command().is_none());
}
#[test]
fn incomplete_native_arrival_ignores_renderer_idle_counter_and_cancels() {
    let mut v = VisualFixture::new();
    let fence = v.get_started();
    let x = v.output.binding.position.x + 40.0;
    v.report(&fence, 3, 4, ReportKind::Progress, 520, x, true);
    v.report(&fence, 4, 5, ReportKind::Progress, 570, x, false);
    assert!(matches!(
        v.report(&fence, 5, 6, ReportKind::Completed, 620, x, false)
            .outcome,
        ReportOutcome::Cancelled
    ));
    assert_eq!(v.phase(), ActionPhase::Cancelled);
}
#[test]
fn stop_then_late_frame_cannot_complete_or_move_actor() {
    let mut v = VisualFixture::new();
    let fence = v.get_started();
    let before =
        v.f.world
            .action_context(
                &v.f.store,
                &world_hub::current_actors(&v.f.core),
                "nova",
                20,
            )
            .unwrap()
            .position;
    v.f.core.stop("nova", 30);
    v.f.world.revoke_actor(&v.f.store, "nova");
    poll(&mut v.f.core, &v.f.store, &v.f.world, || 30, || 130).unwrap();
    assert_eq!(v.phase(), ActionPhase::Cancelled);
    let late = make_report(
        &fence,
        3,
        4,
        ReportKind::Completed,
        v.output.binding.position.x + 40.0,
        v.output.binding.position.y,
        false,
    );
    assert!(crate::life_runtime::action_host::visual_report(
        &mut v.f.core,
        &v.f.store,
        &v.f.world,
        &late,
        40,
        || 40,
        || 140
    )
    .is_err());
    assert_eq!(v.phase(), ActionPhase::Cancelled);
    assert_eq!(
        v.output.binding.position,
        crate::world_output::css(before, v.output.binding.viewport)
    );
}
#[test]
fn delivered_cannot_be_replaced_with_changed_payload_same_receipt_sequence() {
    let mut v = VisualFixture::new();
    v.admit(v.target()).unwrap();
    let fence = v.offer();
    let x = v.output.binding.position.x;
    v.report(&fence, 1, 2, ReportKind::Delivered, 10, x, false);
    let changed = make_report(
        &fence,
        1,
        3,
        ReportKind::Delivered,
        x,
        v.output.binding.position.y,
        false,
    );
    assert!(crate::life_runtime::action_host::visual_report(
        &mut v.f.core,
        &v.f.store,
        &v.f.world,
        &changed,
        20,
        || 20,
        || 120
    )
    .is_err());
    assert_eq!(v.phase(), ActionPhase::Planned);
}
#[test]
fn bound_output_revocation_before_core_wait_prevents_completion() {
    let mut v = VisualFixture::new();
    let fence = v.get_started();
    v.approach(&fence);
    v.output.cancel.store(true, Ordering::SeqCst);
    let x = v.output.binding.position.x + 40.0;
    assert!(matches!(
        v.report(&fence, 8, 9, ReportKind::Completed, 670, x, false)
            .outcome,
        ReportOutcome::Cancelled
    ));
    assert_eq!(v.phase(), ActionPhase::Cancelled);
}
#[test]
fn final_completion_frame_is_rechecked_after_actual_sqlite_wait() {
    for cause in ["age", "output", "profile", "world", "selected"] {
        let mut v = VisualFixture::new();
        let fence = v.get_started();
        v.approach(&fence);
        v.f.now.store(670, Ordering::SeqCst);
        let report = make_report(
            &fence,
            8,
            9,
            ReportKind::Completed,
            v.output.binding.position.x + 40.0,
            v.output.binding.position.y,
            false,
        );
        let path = v.f.root.join("cohost.sqlite3");
        let barrier = std::sync::Barrier::new(2);
        let clock = &v.f.now;
        let output = v.output.cancel.clone();
        let core = &mut v.f.core;
        let store = &v.f.store;
        let world = &v.f.world;
        std::thread::scope(|scope| {
            let writer=scope.spawn(||{let db=rusqlite::Connection::open(path).unwrap();db.execute_batch("BEGIN IMMEDIATE").unwrap();barrier.wait();std::thread::sleep(std::time::Duration::from_millis(80));match cause{"age"=>clock.store(950,Ordering::SeqCst),"output"=>output.store(true,Ordering::SeqCst),"profile"=>{db.execute("UPDATE bot_profiles SET profile=json_set(profile,'$.avatar.scale',2) WHERE id='nova'",[]).unwrap();},"selected"=>{db.execute("UPDATE settings SET value=json_set(value,'$.id','other') WHERE key='profile'",[]).unwrap();},_=>{db.execute("UPDATE shared_world_config SET revision=revision+1,config_json=json_set(config_json,'$.revision',2)",[]).unwrap();}}db.execute_batch("COMMIT").unwrap();});
            barrier.wait();
            let ack = crate::life_runtime::action_host::visual_report(
                core,
                store,
                world,
                &report,
                670,
                || clock.load(Ordering::SeqCst),
                || 770,
            )
            .unwrap();
            assert!(matches!(ack.outcome, ReportOutcome::Cancelled), "{cause}");
            writer.join().unwrap();
            poll(core, store, world, || clock.load(Ordering::SeqCst), || 770).unwrap();
        });
        assert_eq!(v.phase(), ActionPhase::Cancelled, "{cause}");
    }
}
#[test]
fn unchanged_surface_renewal_rejects_stale_ticket_without_cancelling_existing_binding() {
    let gate = crate::life_runtime::action_output::OutputGate::default();
    gate.ui_connect("studio");
    gate.ui_report("studio", 1, false);
    let cancel = Arc::new(AtomicBool::new(false));
    let ticket = gate.ticket().unwrap();
    assert!(gate.watch_surface(ticket, "nova", "main", "render", 1, &cancel, ("studio", 1)));
    gate.bot(None);
    assert!(!gate.watch_surface(ticket, "nova", "main", "render", 1, &cancel, ("studio", 1)));
    assert!(!cancel.load(Ordering::SeqCst));
    gate.window("main");
    assert!(cancel.load(Ordering::SeqCst));
}
#[test]
fn move_intent_requires_fresh_truthful_ui_and_is_single_use() {
    let mut v = VisualFixture::new();
    let proof = UiProof {
        connection_id: "studio".into(),
        sequence: 1,
    };
    let profile = v.f.store.bot_profile("nova").unwrap();
    let target = v.target();
    v.f.core
        .prepare_move_intent("nova", profile.clone(), &proof, 0, target, 0);
    let intent = v.f.core.rest_intent("nova", &proof).unwrap();
    assert_eq!(intent.move_target, Some(target));
    assert!(v.f.core.consume_rest_intent(&intent, &profile, 0).is_some());
    assert!(v.f.core.consume_rest_intent(&intent, &profile, 0).is_none());
    assert!(v.f.core.report_ui("studio", 2, true, true, true, 1));
    v.f.core.prepare_move_intent(
        "nova",
        profile.clone(),
        &UiProof {
            connection_id: "studio".into(),
            sequence: 2,
        },
        0,
        target,
        1,
    );
    assert!(v
        .f
        .core
        .rest_intent(
            "nova",
            &UiProof {
                connection_id: "studio".into(),
                sequence: 2
            }
        )
        .is_none());
}

#[test]
fn stale_old_mount_register_cannot_replace_a_new_binding_before_latch_admission() {
    let mut v = VisualFixture::new();
    let gate = crate::life_runtime::action_output::OutputGate::default();
    gate.ui_connect("studio");
    gate.ui_report("studio", 1, false);
    let old_ticket = gate.ticket().unwrap();
    let mut request = RegisterWorldActor {
        version: 1,
        mount_id: "new-mount".into(),
        actor_id: "nova".into(),
        world_instance_id: v.source.context.instance_id.clone(),
        stage_id: "stage".into(),
        config_revision: v.source.context.config_revision,
        studio_proof: Some(UiProof {
            connection_id: "studio".into(),
            sequence: 1,
        }),
    };
    let current = gate
        .admit_surface(old_ticket, "nova", "main", ("studio", 1), || {
            v.f.core
                .world_outputs
                .register(&request, v.source.clone(), 0)
        })
        .unwrap();
    let current_cancel = v.f.core.world_outputs.source("nova").unwrap();
    assert_eq!(current_cancel.ui_connection, "studio");
    let changed = std::cell::Cell::new(false);
    request.mount_id = "old-late-mount".into();
    assert!(gate
        .admit_surface(old_ticket, "nova", "main", ("studio", 1), || {
            changed.set(true);
            v.f.core
                .world_outputs
                .register(&request, v.source.clone(), 0)
        })
        .is_err());
    assert!(!changed.get());
    let actual = v.f.core.world_outputs.status("nova", 0).unwrap();
    assert_eq!(actual.render_session_id, current.render_session_id);
    let challenge = match current.readiness {
        Readiness::AwaitingMeasurement { challenge } => challenge,
        _ => panic!("expected challenge"),
    };
    assert_eq!(
        v.f.core
            .world_outputs
            .measurement_actor(&challenge.id)
            .as_deref(),
        Some("nova")
    );
}

#[test]
fn registration_accepts_only_unchanged_clear_heartbeat_after_proof_not_new_activity() {
    let mut v = VisualFixture::new();
    let proof = UiProof {
        connection_id: "studio".into(),
        sequence: 1,
    };
    let gate = crate::life_runtime::action_output::OutputGate::default();
    gate.ui_connect("studio");
    gate.ui_report("studio", 1, false);
    let ticket = gate.ticket().unwrap();
    assert!(v.f.core.report_ui("studio", 2, false, false, true, 1));
    gate.ui_report("studio", 2, false);
    assert!(v.f.core.world_ui_current(&proof, 1));
    let request = RegisterWorldActor {
        version: 1,
        mount_id: "mount".into(),
        actor_id: "nova".into(),
        world_instance_id: v.source.context.instance_id.clone(),
        stage_id: "stage".into(),
        config_revision: v.source.context.config_revision,
        studio_proof: Some(proof.clone()),
    };
    assert!(gate
        .admit_surface(ticket, "nova", "main", ("studio", 1), || v
            .f
            .core
            .world_outputs
            .register(&request, v.source.clone(), 1))
        .is_ok());
    assert!(v.f.core.report_ui("studio", 3, true, true, true, 2));
    gate.ui_report("studio", 3, true);
    assert!(!v.f.core.world_ui_current(&proof, 2));
    let mutated = std::cell::Cell::new(false);
    assert!(gate
        .admit_surface(
            gate.ticket().unwrap(),
            "nova",
            "main",
            ("studio", 1),
            || {
                mutated.set(true);
                v.f.core
                    .world_outputs
                    .register(&request, v.source.clone(), 2)
            }
        )
        .is_err());
    assert!(!mutated.get());
    gate.ui_connect("new");
    assert!(gate
        .admit_surface(
            gate.ticket().unwrap(),
            "nova",
            "main",
            ("studio", 1),
            || {
                mutated.set(true);
                v.f.core
                    .world_outputs
                    .register(&request, v.source.clone(), 2)
            }
        )
        .is_err());
    assert!(!mutated.get());
}

#[test]
fn source_read_wait_does_not_renew_expired_measurement_or_registration_ui() {
    for kind in ["measurement", "registration"] {
        let mut v = VisualFixture::new();
        let proof = UiProof {
            connection_id: "studio".into(),
            sequence: 1,
        };
        let request = RegisterWorldActor {
            version: 1,
            mount_id: "mount".into(),
            actor_id: "nova".into(),
            world_instance_id: v.source.context.instance_id.clone(),
            stage_id: "stage".into(),
            config_revision: v.source.context.config_revision,
            studio_proof: Some(proof.clone()),
        };
        let (reply, _, _) =
            v.f.core
                .world_outputs
                .register(&request, v.source.clone(), 0)
                .unwrap();
        let challenge = match reply.readiness {
            Readiness::AwaitingMeasurement { challenge } => challenge,
            _ => panic!("challenge"),
        };
        let mut report = v.measurement.clone();
        report.challenge_id = challenge.id;
        report.frame_sequence = 2;
        let gate = std::sync::Barrier::new(2);
        let clock = &v.f.now;
        let store = &v.f.store;
        std::thread::scope(|scope| {
            let holder = scope.spawn(|| {
                // Use the existing checked Store seam; no private field access
                // or visibility widening. The callback holds the actual native
                // connection mutex and IMMEDIATE transaction, then rolls back.
                let result = store.apply_life_action_checked::<()>("nova", 100, |_, _| {
                    gate.wait();
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    clock.store(
                        if kind == "measurement" { 300 } else { 6000 },
                        Ordering::SeqCst,
                    );
                    Err(crate::storage::life_store::LifeStoreError::InvalidInput)
                });
                assert!(matches!(
                    result,
                    Err(crate::storage::life_store::LifeStoreError::InvalidInput)
                ));
            });
            gate.wait();
            let profile = v.source.profile_token;
            let observed = store
                .action_profile_token("nova", &store.bot_profile("nova").unwrap())
                .unwrap();
            assert_eq!(observed, profile);
            holder.join().unwrap();
        });
        let admitted = v.f.now.load(Ordering::SeqCst);
        if kind == "measurement" {
            assert!(v
                .f
                .core
                .accept_world_measurement(&report, &v.source, 0, admitted)
                .is_err());
            assert!(v.f.core.world_outputs.check_proof(&v.output, 300));
        } else {
            assert!(!v.f.core.world_ui_current(&proof, admitted));
        }
        assert_eq!(
            v.f.core
                .world_outputs
                .status("nova", 0)
                .unwrap()
                .render_session_id,
            v.output.binding.render_session_id
        );
    }
}

#[test]
fn capability_and_plan_reject_old_private_session_revision_or_replaced_ui_even_with_fresh_measurement(
) {
    for cause in ["session", "ui"] {
        let mut v = VisualFixture::new();
        assert!(v.f.core.world_output_current("nova", 0));
        let original = (
            v.f.core.sessions["nova"].session_id.clone(),
            v.f.core.sessions["nova"].revision,
        );
        if cause == "session" {
            v.f.core
                .set_output_available("nova", &original.0, original.1, false, 1);
            let revised = (
                v.f.core.sessions["nova"].session_id.clone(),
                v.f.core.sessions["nova"].revision,
            );
            v.f.core
                .set_output_available("nova", &revised.0, revised.1, true, 1);
        } else {
            v.f.core.connect_ui("replacement".into(), 1);
            assert!(v.f.core.report_ui("replacement", 1, false, false, true, 1));
        }
        assert!(v.f.core.world_outputs.ready("nova", 1).is_ok());
        assert!(!v.f.core.world_output_current("nova", 1));
        let before =
            serde_json::to_value(v.f.store.load_life("nova").unwrap().unwrap().state()).unwrap();
        v.f.now.store(1, Ordering::SeqCst);
        assert!(v.admit(v.target()).is_err(), "{cause}");
        assert_eq!(
            before,
            serde_json::to_value(v.f.store.load_life("nova").unwrap().unwrap().state()).unwrap()
        );
        assert!(v.f.core.world_outputs.take_command().is_none());
    }
}
