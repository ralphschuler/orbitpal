//! Closed camera/schema and synthetic Studio geometry math only. No native
//! constructor, document capability, Live surface, GUI or renderer is faked.
use super::*;
use crate::{life_domain::{ActorId,WorldId},life_runtime::action_state::{UiProof,WorldActionFence},
    world_state::{coordinates::{Point,Rect},SessionStamp,StageId}};
use serde_json::json;

fn source()->NativeSource {
    NativeSource {route:SourceRoute::Studio,context:WorldActionFence {
        session:SessionStamp::new(ActorId::new("nova").unwrap(),"synthetic-session",1).unwrap(),
        source_world:WorldId::new("lineage").unwrap(),stage:StageId::new("stage").unwrap(),
        config_revision:1,topology_revision:1,position:Point::new(500_000,800_000).unwrap(),
        instance_id:"world-instance".into(),record:json!({"revision":1})},profile_token:[7;32],
        ui_connection:"studio".into(),layout_token:json!({"width":1000,"height":1080}),
        viewport_limit:CssSize{width:1000.0,height:1080.0},safe_region:Rect::full(),
        profile_scale:1.0,effective_scale:1.0,native_walk:true,owner:true,native_framing:None}
}
fn request()->RegisterWorldActor {
    RegisterWorldActor {version:1,mount_id:"mount".into(),actor_id:"nova".into(),world_instance_id:"world-instance".into(),
        stage_id:"stage".into(),config_revision:1,studio_proof:Some(UiProof{connection_id:"studio".into(),sequence:1})}
}
fn registered()->(Registry,NativeSource,MeasurementChallenge) {
    let mut registry=Registry::default();let source=source();
    let (reply,_,_)=registry.register(&request(),source.clone(),0).unwrap();
    let Readiness::AwaitingMeasurement{challenge}=reply.readiness else {panic!("synthetic challenge absent")};
    (registry,source,challenge)
}
fn measurement(c:&MeasurementChallenge)->RenderMeasurement {
    RenderMeasurement {version:1,challenge_id:c.id.clone(),frame_sequence:1,enclosure_kind:c.enclosure_kind,
        viewport:CssSize{width:1000.0,height:1080.0},clip:CssRect{left:0.0,top:0.0,right:1000.0,bottom:1080.0},
        anchor:CssPoint{x:500.0,y:864.0},posed_bounds:CssRect{left:480.0,top:830.0,right:530.0,bottom:870.0},
        envelope_extents:EnvelopeExtents{left:30.0,top:40.0,right:40.0,bottom:20.0},
        reference_local:LocalReference{x:0.0,y:1.45,z:0.0},ground_basis:GroundBasis{right:GroundVector{x:8.0/1080.0,z:0.0},down:GroundVector{x:0.0,z:16.0/1080.0}},
        model_forward_yaw:0.0,applied_model_scale:1.0,reduced_motion:false,walk_supported:true}
}

#[test]
fn native_physical_camera_matches_normal_fov_and_preserves_reference_physical_size() {
    for camera in [NormalCamera::Full,NormalCamera::Portrait] {
        for dpi in [0.25,1.0,2.0,8.0] {
            let p=NativePhysicalPolicy::new(780,1920,1080,dpi,camera).unwrap();
            let distance=match camera {NormalCamera::Full=>(0.55_f64.powi(2)+0.7_f64.powi(2)+7.6_f64.powi(2)).sqrt(),
                NormalCamera::Portrait=>(0.35_f64.powi(2)+0.48_f64.powi(2)+5.4_f64.powi(2)).sqrt()};
            let normal=2.0*distance*(16.0_f64*std::f64::consts::PI/180.0).tan();
            assert!((p.normal_vertical_span-normal).abs()<1e-12);
            assert!((p.vertical_span-normal*1080.0/780.0).abs()<1e-12);
            let css=p.viewport();assert_eq!(css.width,1920.0/dpi);assert_eq!(css.height,1080.0/dpi);
            assert!((css.height/p.vertical_span*dpi-780.0/normal).abs()<1e-10);
        }
    }
}

#[test]
fn native_physical_camera_rejects_zero_huge_nonfinite_and_outside_dpi_bounds() {
    for (reference,width,height,dpi) in [(0,1920,1080,1.0),(780,0,1080,1.0),(780,1920,0,1.0),
        (1_000_001,1920,1080,1.0),(780,1_000_001,1080,1.0),(780,1920,1_000_001,1.0),
        (780,1920,1080,0.249),(780,1920,1080,8.001),(780,1920,1080,f64::NAN),
        (780,1920,1080,f64::INFINITY)] {
        assert_eq!(NativePhysicalPolicy::new(reference,width,height,dpi,NormalCamera::Full),Err(RenderReason::LayoutUnconfirmed));
    }
}

#[test]
fn native_policy_cannot_change_camera_tag_spans_or_viewport_under_a_valid_shape() {
    let p=NativePhysicalPolicy::new(780,1920,1080,1.0,NormalCamera::Portrait).unwrap();
    for bad in [NativePhysicalPolicy{kind:CameraPolicy::WorldIsoV1,..p},
        NativePhysicalPolicy{normal_vertical_span:p.normal_vertical_span*0.9,..p},
        NativePhysicalPolicy{vertical_span:p.vertical_span*0.9,..p},
        NativePhysicalPolicy{normal_camera:NormalCamera::Full,..p},
        NativePhysicalPolicy{reference_physical_height:781,..p},
        NativePhysicalPolicy{physical_height:1081,..p}] {assert!(!bad.valid());}
}

#[test]
fn native_policy_serde_is_closed_and_studio_omits_both_native_binding_fields() {
    let p=NativePhysicalPolicy::new(780,1920,1080,1.0,NormalCamera::Full).unwrap();
    let value=serde_json::to_value(p).unwrap();assert_eq!(value["kind"],"native_physical_iso_v1");
    assert_eq!(serde_json::from_value::<NativePhysicalPolicy>(value.clone()).unwrap(),p);
    let mut extra=value.clone();extra["fitScale"]=json!(0.5);assert!(serde_json::from_value::<NativePhysicalPolicy>(extra).is_err());
    let mut missing=value.clone();missing.as_object_mut().unwrap().remove("normalCamera");assert!(serde_json::from_value::<NativePhysicalPolicy>(missing).is_err());
    let mut unknown=value;unknown["kind"]=json!("future_native_camera");assert!(serde_json::from_value::<NativePhysicalPolicy>(unknown).is_err());
    let (mut registry,source,c)=registered();let report=measurement(&c);
    let ack=registry.measurement(&report,&source,10,10).unwrap();
    let Readiness::Ready{binding}=ack.output.readiness else {panic!("synthetic Studio binding absent")};
    let encoded=serde_json::to_value(binding).unwrap();assert!(encoded.get("cameraPolicy").is_none());assert!(encoded.get("nativeFraming").is_none());
    let encoded_challenge=serde_json::to_value(c).unwrap();assert_eq!(encoded_challenge["cameraPolicy"],"world_iso_v1");assert!(encoded_challenge.get("nativeFraming").is_none());
}

#[test]
fn studio_route_cannot_become_native_by_attaching_camera_metadata_or_dropping_ui_proof() {
    let mut registry=Registry::default();let mut contradictory=source();
    contradictory.native_framing=Some(NativePhysicalPolicy::new(780,1000,1080,1.0,NormalCamera::Full).unwrap());
    assert!(matches!(registry.register(&request(),contradictory,0),Err(RenderReason::ScopeMismatch)));
    let mut without_ui=request();without_ui.studio_proof=None;
    assert!(matches!(registry.register(&without_ui,source(),0),Err(RenderReason::ScopeMismatch)));
    assert!(registry.status("nova",0).is_none());
}

#[test]
fn all_yaw_asymmetric_envelope_has_real_safe_edges_even_when_symmetric_metadata_exceeds_viewport() {
    // This pure helper supplies math only, not a forged Native SourceRoute.
    let viewport=CssSize{width:1000.0,height:1080.0};
    let clip=CssRect{left:0.0,top:0.0,right:1000.0,bottom:1080.0};
    let envelope=EnvelopeExtents{left:40.0,top:700.0,right:60.0,bottom:100.0};
    let safe=state::measured_safe_centers(Rect::full(),clip,viewport,envelope);
    assert_eq!(safe,CssRect{left:40.0,top:700.0,right:940.0,bottom:980.0});
    assert_eq!(2.0*envelope.top.max(envelope.bottom),1400.0);
    assert!(2.0*envelope.top.max(envelope.bottom)>viewport.height);
    for edge in [CssPoint{x:40.0,y:864.0},CssPoint{x:940.0,y:864.0},CssPoint{x:500.0,y:700.0},CssPoint{x:500.0,y:980.0}] {
        assert!(safe.contains(edge));assert!(clip.contains_rect(envelope.bounds(edge)));
    }
    for outside in [CssPoint{x:39.99,y:864.0},CssPoint{x:940.01,y:864.0},CssPoint{x:500.0,y:699.99},CssPoint{x:500.0,y:980.01}] {
        assert!(!safe.contains(outside));assert!(!clip.contains_rect(envelope.bounds(outside)));
    }
}

#[test]
fn studio_registry_keeps_its_existing_symmetric_safe_region() {
    let (mut registry,source,c)=registered();let report=measurement(&c);
    registry.measurement(&report,&source,10,10).unwrap();let proof=registry.ready("nova",10).unwrap();
    assert_eq!(proof.binding.envelope_extents,report.envelope_extents);
    assert_eq!(proof.binding.maximum_footprint,CssSize{width:80.0,height:80.0});
    assert_eq!(proof.binding.safe_centers,CssRect{left:40.0,top:40.0,right:960.0,bottom:1040.0});
}

#[test]
fn clip_intersection_and_exact_saved_anchor_cannot_be_clamped_into_asymmetric_safe_centres() {
    let (mut registry,source,c)=registered();let mut report=measurement(&c);
    report.clip.left=450.0; // The all-yaw left margin must now start at490.
    let ack=registry.measurement(&report,&source,10,10).unwrap();
    let Readiness::Ready{binding}=ack.output.readiness else {panic!("synthetic binding absent")};
    assert_eq!(binding.safe_centers.left,490.0);assert_eq!(binding.position,CssPoint{x:500.0,y:864.0});
    let (mut registry,mut source,c)=registered();source.context.position=Point::new(480_000,800_000).unwrap();
    let mut report=measurement(&c);report.anchor.x=480.0;report.clip.left=450.0;
    report.posed_bounds=CssRect{left:460.0,top:830.0,right:510.0,bottom:870.0};
    assert!(matches!(registry.measurement(&report,&source,10,10),Err(RenderReason::TargetDoesNotFit)));
}
