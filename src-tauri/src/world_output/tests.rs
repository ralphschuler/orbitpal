//! Synthetic native observations only: no GUI, display query or successful GPU
//! render is fabricated by these fixtures.
use super::*;
use crate::{
    life_domain::{ActorId, WorldId},
    life_runtime::action_state::{UiProof, WorldActionFence},
    world_state::{
        coordinates::{Point, Rect},
        SessionStamp, StageId,
    },
};
use serde_json::json;
use std::sync::atomic::Ordering;
fn source() -> NativeSource {
    NativeSource {
        context: WorldActionFence {
            session: SessionStamp::new(ActorId::new("nova").unwrap(), "life-session", 1).unwrap(),
            source_world: WorldId::new("lineage").unwrap(),
            stage: StageId::new("stage").unwrap(),
            config_revision: 1,
            topology_revision: 1,
            position: Point::new(500_000, 500_000).unwrap(),
            instance_id: "world-instance".into(),
            record: json!({"revision":1}),
        },
        profile_token: [1; 32],
        ui_connection: "studio".into(),
        layout_token: json!({"width":800,"height":600}),
        viewport_limit: CssSize {
            width: 800.0,
            height: 600.0,
        },
        safe_region: Rect::full(),
        profile_scale: 1.0,
        effective_scale: 1.0,
        native_walk: true,
        owner: true,
    }
}
fn request() -> RegisterWorldActor {
    RegisterWorldActor {
        version: 1,
        mount_id: "mount".into(),
        actor_id: "nova".into(),
        world_instance_id: "world-instance".into(),
        stage_id: "stage".into(),
        config_revision: 1,
        studio_proof: Some(UiProof {
            connection_id: "studio".into(),
            sequence: 1,
        }),
    }
}
fn challenge(reply: &RegisteredWorldActor) -> MeasurementChallenge {
    match &reply.readiness {
        Readiness::AwaitingMeasurement { challenge } => challenge.clone(),
        _ => panic!("expected a native challenge"),
    }
}
fn measurement(challenge: &MeasurementChallenge, seq: u64) -> RenderMeasurement {
    RenderMeasurement {
        version: 1,
        challenge_id: challenge.id.clone(),
        frame_sequence: seq,
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
        anchor: CssPoint { x: 400.0, y: 300.0 },
        posed_bounds: CssRect {
            left: 410.0,
            top: 260.0,
            right: 425.0,
            bottom: 280.0,
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
    }
}
fn ready() -> (Registry, NativeSource, OutputProof) {
    let mut registry = Registry::default();
    let source = source();
    let (reply, _, created) = registry.register(&request(), source.clone(), 0).unwrap();
    assert!(created);
    let m = measurement(&challenge(&reply), 1);
    registry.measurement(&m, &source, 10, 10).unwrap();
    let proof = registry.ready("nova", 10).unwrap();
    (registry, source, proof)
}
fn frame(seq: u64, x: f64, walking: bool) -> FrameObservation {
    FrameObservation {
        frame_sequence: seq,
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
        anchor: CssPoint { x, y: 300.0 },
        posed_bounds: CssRect {
            left: x - 10.0,
            top: 270.0,
            right: x + 15.0,
            bottom: 300.0,
        },
        walking,
        target_idle_frames: u16::MAX,
    }
}
#[test]
fn renewal_is_idempotent_and_metadata_never_refreshes_the_successful_frame() {
    let (mut registry, source, proof) = ready();
    let (reply, cancel, created) = registry.register(&request(), source.clone(), 3000).unwrap();
    assert!(!created);
    assert!(std::sync::Arc::ptr_eq(&cancel, &proof.cancel));
    let first = challenge(&reply);
    let (second, _, _) = registry.register(&request(), source.clone(), 3100).unwrap();
    assert_eq!(first.id, challenge(&second).id);
    assert!(registry.ready("nova", 5010).is_ok());
    assert!(registry.ready("nova", 5011).is_err());
    assert!(!cancel.load(Ordering::SeqCst));
    let m = measurement(&first, 2);
    registry.measurement(&m, &source, 5100, 5100).unwrap();
    assert!(registry.ready("nova", 5100).is_ok());
    assert!(registry.measurement(&m, &source, 5101, 5101).is_err());
    assert_eq!(
        registry
            .ready("nova", 5100)
            .unwrap()
            .binding
            .reference_revision,
        proof.binding.reference_revision
    );
}
#[test]
fn native_challenge_revisions_scope_and_closed_ground_basis_cannot_be_supplied_by_renderer() {
    let (mut registry, source, proof) = ready();
    let mut foreign = request();
    foreign.config_revision = 2;
    assert!(registry.register(&foreign, source.clone(), 20).is_err());
    let (reply, _, _) = registry.register(&request(), source.clone(), 20).unwrap();
    let mut m = measurement(&challenge(&reply), 2);
    m.ground_basis.down.z = 8.0 / 600.0;
    assert!(registry.measurement(&m, &source, 20, 20).is_err());
    assert!(registry.check_proof(&proof, 20));
    m.ground_basis.down.z = 16.0 / 600.0;
    m.reference_local.x = 1.0;
    assert!(registry.measurement(&m, &source, 20, 20).is_err());
    assert!(proof.cancel.load(Ordering::SeqCst));
    let mut next = request();
    next.mount_id = "new-mount".into();
    let (reply, _, created) = registry.register(&next, source.clone(), 21).unwrap();
    assert!(created);
    assert_ne!(reply.render_session_id, proof.binding.render_session_id);
    assert_ne!(
        challenge(&reply).expected.reference_revision,
        proof.binding.reference_revision
    );
}
#[test]
fn asymmetric_anchor_outside_hull_is_valid_but_target_is_never_clamped() {
    let (registry, _, proof) = ready();
    assert_eq!(
        proof.binding.maximum_footprint,
        CssSize {
            width: 60.0,
            height: 80.0
        }
    );
    assert_eq!(
        proof.binding.safe_centers,
        CssRect {
            left: 30.0,
            top: 40.0,
            right: 770.0,
            bottom: 560.0
        }
    );
    assert!(VisualExecution::new(proof.clone(), Point::new(0, 500_000).unwrap(), 10).is_err());
    let mut mirror = source();
    mirror.owner = false;
    let mut registry2 = Registry::default();
    let (reply, _, _) = registry2.register(&request(), mirror.clone(), 0).unwrap();
    registry2
        .measurement(&measurement(&challenge(&reply), 1), &mirror, 10, 10)
        .unwrap();
    assert!(registry2.ready("nova", 10).is_err());
    assert!(registry
        .check_frame(&proof, &frame(2, 400.0, false), 20)
        .is_ok());
    let mut breach = frame(2, 400.0, false);
    breach.posed_bounds.right = 431.0;
    assert!(registry.check_frame(&proof, &breach, 20).is_err());
}
#[test]
fn native_arrival_requires_distinct_current_frames_and_elapsed_settling_not_renderer_counter() {
    let (_, _, proof) = ready();
    let mut visual =
        VisualExecution::new(proof.clone(), Point::new(550_000, 500_000).unwrap(), 10).unwrap();
    visual
        .observe(&frame(2, 400.0, false), ReportKind::Delivered, 20, 20)
        .unwrap();
    visual
        .observe(&frame(3, 400.0, false), ReportKind::Started, 30, 30)
        .unwrap();
    visual
        .observe(&frame(4, 407.2, true), ReportKind::Progress, 230, 230)
        .unwrap();
    visual
        .observe(&frame(5, 428.0, true), ReportKind::Progress, 430, 430)
        .unwrap();
    visual
        .observe(&frame(6, 440.0, true), ReportKind::Progress, 530, 530)
        .unwrap();
    assert!(visual
        .clone()
        .observe(&frame(7, 440.0, false), ReportKind::Completed, 580, 580)
        .is_err());
    visual
        .observe(&frame(7, 440.0, false), ReportKind::Progress, 580, 580)
        .unwrap();
    visual
        .observe(&frame(8, 440.0, false), ReportKind::Progress, 630, 630)
        .unwrap();
    assert!(visual
        .observe(&frame(9, 440.0, false), ReportKind::Completed, 680, 680)
        .is_ok());
    assert!(!visual.fresh(931));
    assert!(visual
        .observe(&frame(10, 440.0, false), ReportKind::Completed, 950, 950)
        .is_err());
    let mut duplicate = visual.clone();
    assert!(duplicate
        .observe(&frame(9, 440.0, false), ReportKind::Progress, 951, 951)
        .is_err());
    let mut late = VisualExecution::new(proof, Point::new(550_000, 500_000).unwrap(), 0).unwrap();
    assert!(late
        .observe(&frame(2, 400.0, false), ReportKind::Delivered, 1, 252)
        .is_err());
}
#[test]
fn stale_unregistration_cannot_remove_a_new_binding_and_successful_frames_keep_the_original_proof()
{
    let (mut registry, source, proof) = ready();
    let mut next = request();
    next.mount_id = "replacement".into();
    let (reply, _, _) = registry.register(&next, source.clone(), 20).unwrap();
    assert!(!registry.unregister(
        &UnregisterWorldActor {
            version: 1,
            render_session_id: proof.binding.render_session_id.clone(),
            binding_sequence: proof.binding.binding_sequence
        },
        "studio"
    ));
    assert_eq!(
        registry.status("nova", 20).unwrap().render_session_id,
        reply.render_session_id
    );
    registry
        .measurement(&measurement(&challenge(&reply), 2), &source, 21, 21)
        .unwrap();
    let proof = registry.ready("nova", 21).unwrap();
    let f = frame(3, 424.0, true);
    registry
        .accept_frame(&proof, &f, Point::new(530_000, 500_000).unwrap(), 221)
        .unwrap();
    assert!(registry.check_proof(&proof, 221));
    assert_eq!(
        registry.ready("nova", 221).unwrap().binding.position.x,
        424.0
    );
    assert!(registry.check_frame(&proof, &f, 222).is_err());
}

#[test]
fn native_time_and_acceleration_budget_cannot_be_replenished_by_many_same_clock_frames() {
    let (_, _, proof) = ready();
    let mut visual =
        VisualExecution::new(proof, Point::new(550_000, 500_000).unwrap(), 10).unwrap();
    visual
        .observe(&frame(2, 400.0, false), ReportKind::Delivered, 20, 20)
        .unwrap();
    visual
        .observe(&frame(3, 400.0, false), ReportKind::Started, 30, 30)
        .unwrap();
    assert!(visual
        .clone()
        .observe(&frame(4, 412.0, true), ReportKind::Progress, 130, 130)
        .is_err());
    for (seq, x) in [(4, 400.2), (5, 400.4)] {
        visual
            .observe(&frame(seq, x, true), ReportKind::Progress, 30, 30)
            .unwrap();
    }
    assert!(visual
        .observe(&frame(6, 400.6, true), ReportKind::Progress, 30, 30)
        .is_err());
}

#[test]
fn renewal_at_a_new_confirmed_anchor_tolerates_inverse_projection_noise_without_drifting_binding() {
    let (mut registry, mut source, proof) = ready();
    let f = frame(2, 424.0, true);
    let position = Point::new(530_000, 500_000).unwrap();
    registry.accept_frame(&proof, &f, position, 220).unwrap();
    source.context.position = position;
    let (reply, _, created) = registry.register(&request(), source.clone(), 230).unwrap();
    assert!(!created);
    let mut m = measurement(&challenge(&reply), 3);
    m.anchor.x += 24.0;
    m.posed_bounds.left += 24.0;
    m.posed_bounds.right += 24.0;
    m.ground_basis.right.x += 1e-14;
    m.ground_basis.down.z -= 1e-14;
    m.applied_model_scale += 1e-12;
    m.model_forward_yaw += 1e-12;
    registry.measurement(&m, &source, 240, 240).unwrap();
    let renewed = registry.ready("nova", 240).unwrap();
    assert_eq!(renewed.binding.ground_basis, proof.binding.ground_basis);
    assert_eq!(renewed.binding.scale, proof.binding.scale);
    assert_eq!(
        renewed.binding.model_forward_yaw,
        proof.binding.model_forward_yaw
    );
    assert_eq!(
        renewed.binding.reference_revision,
        proof.binding.reference_revision
    );
    assert_eq!(renewed.binding.position.x, 424.0);
    assert!(registry.check_proof(&proof, 240));
}

#[test]
fn faster_stationary_cadence_preserves_first_frame_span_and_does_not_count_delivered() {
    let (_, _, proof) = ready();
    let mut visual = VisualExecution::new(proof, Point::new(500_000, 500_000).unwrap(), 0).unwrap();
    visual
        .observe(&frame(2, 400.0, false), ReportKind::Delivered, 10, 10)
        .unwrap();
    assert!(visual.stable.is_empty());
    visual
        .observe(&frame(3, 400.0, false), ReportKind::Started, 20, 20)
        .unwrap();
    visual
        .observe(&frame(4, 400.0, false), ReportKind::Progress, 60, 60)
        .unwrap();
    visual
        .observe(&frame(5, 400.0, false), ReportKind::Progress, 100, 100)
        .unwrap();
    assert!(visual
        .observe(&frame(6, 400.0, false), ReportKind::Completed, 140, 140)
        .is_ok());
    assert_eq!(visual.stable.len(), 3);
    assert_eq!(visual.stable.front().unwrap().1, 20);
}
