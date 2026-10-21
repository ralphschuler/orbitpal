import { afterEach, describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { AvatarStage } from './avatar';
import { captureWorldPose, configureWorldCamera, placeWorldReference, placementForwardYaw, WorldAvatarController,
  type WorldAvatarOptions, type WorldTrial } from './world-avatar';
import { WorldPetPlayer } from './world-pet';
import { WorldRenderGeometry, type RenderPort, type SurfaceObservation } from './world-render-geometry';
import type { LocalWorldFrame, MeasurementChallenge, NativeMotionBinding as MotionBinding, MotionFence, RenderMeasurement,
  NativePhysicalPolicy, WorldAdapterReason } from './world-output';
import type { MotionOffer } from './world-motion';
afterEach(()=>vi.unstubAllGlobals());

// Actual Three geometry/camera/transforms with SYNTHETIC submitted-draw port.
// No WebGL, native delivery, Store completion, pixel/OS visibility or providers.
const expected={actorId:'nova',sourceWorldId:'life-nova',renderSessionId:'render-1',bindingSequence:1,
  worldInstanceId:'world-1',stageId:'stage-1',configRevision:1,topologyRevision:1,layoutRevision:1,
  assetRevision:1,measurementRevision:1,referenceRevision:1};
const challenge=():MeasurementChallenge=>({id:'challenge-1',expiresInMs:5000,expected:{...expected},
  cameraPolicy:'world_iso_v1',enclosureKind:'conservative_submitted_geometry_v1',profileScale:1,effectiveScale:1,expectedReferenceLocal:null});
const surface:SurfaceObservation={connected:true,visible:true,viewport:{width:400,height:400},clip:{left:0,top:0,right:400,bottom:400}};
function fixture(geometry:THREE.BufferGeometry=new THREE.BoxGeometry(.6,1,.4)){
  const scene=new THREE.Scene(),root=new THREE.Group(),heading=new THREE.Group(),offset=new THREE.Group();
  const mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial());mesh.position.y=.5;
  offset.add(mesh);heading.add(offset);root.add(heading);scene.add(root);
  const controller=new WorldAvatarController(root,heading,offset);
  const frames:LocalWorldFrame[]=[],measurements:RenderMeasurement[]=[],reasons:WorldAdapterReason[]=[],activities:(import('./world-avatar').WorldStationFrame|undefined)[]=[];
  let prepared=0,now=0;
  let options:WorldAvatarOptions={worldOwner:true,worldChallenge:challenge(),worldExposed:false,
    worldMirrorPosition:{x:500_000,y:500_000},onWorldPrepared:()=>prepared++,onWorldFrame:f=>frames.push(f),
    onWorldActivity:value=>activities.push(value),onWorldMeasurement:m=>measurements.push(m),onWorldUnavailable:(_identity,reason)=>reasons.push(reason)};
  controller.update(options,now);
  const state={renders:0,lost:false,throwAfter:false,afterDraw:()=>{}};
  const port:RenderPort={getSize:v=>v.set(400,400),getViewport:v=>v.set(0,0,400,400),getScissorTest:()=>false,
    getRenderTarget:()=>null,getContext:()=>({isContextLost:()=>state.lost}),render(s,c){
      state.renders++;s.updateMatrixWorld(true);c.updateMatrixWorld(true);
      root.traverseVisible(node=>{if(node instanceof THREE.Mesh&&c.layers.test(node.layers)){
        const material=node.material;if(!Array.isArray(material))node.onAfterRender(port as THREE.WebGLRenderer,s as THREE.Scene,c,node.geometry,material,null as unknown as THREE.Group);
      }});state.afterDraw();if(state.throwAfter)throw new Error('synthetic render failed');
    }};
  const update=(patch:Partial<WorldAvatarOptions>)=>{options={...options,...patch};controller.update(options,now);};
  const render=(step=17,pose?:(trial:WorldTrial,now:number)=>WorldTrial)=>{
    now+=step;const trial=controller.prepare(now,400,400,true,.5,true,0,false);
    expect(trial).not.toBeNull();const posed=pose?pose(trial!,now):trial!;
    return {trial:posed,result:controller.render(posed,scene,port,()=>surface,now,()=>0)};
  };
  const ready=()=>{
    for(let i=0;i<33;i++)expect(render().result).toBe('committed');
    expect(prepared).toBe(1);expect(measurements).toHaveLength(0);
    update({worldExposed:true});expect(render().result).toBe('committed');
    const m=measurements[0];expect(m).toBeDefined();
    const size={width:2*Math.max(m.envelopeExtents.left,m.envelopeExtents.right),height:2*Math.max(m.envelopeExtents.top,m.envelopeExtents.bottom)};
    const binding:MotionBinding={...expected,enclosureKind:'conservative_submitted_geometry_v1',viewport:m.viewport,safeCenters:{left:size.width/2+1,top:size.height/2+1,right:400-size.width/2-1,bottom:400-size.height/2-1},
      maximumFootprint:size,envelopeExtents:m.envelopeExtents,referenceLocal:m.referenceLocal,position:m.anchor,groundBasis:m.groundBasis,
      modelForwardYaw:m.modelForwardYaw,profileScale:1,effectiveScale:1,scale:m.appliedModelScale,reducedMotion:false,walkSupported:true, stationSupported:m.stationSupported??false};
    update({worldBinding:binding});return binding;
  };
  return {scene,root,heading,offset,mesh,controller,port,state,frames,measurements,reasons,activities,render,update,ready,
    now:()=>now,options:()=>options,prepared:()=>prepared};
}
function move(binding:MotionBinding,ordinal=1,sequence=1):MotionOffer{
  const {sourceWorldId,...fenceStamp}=expected;
  const fence:MotionFence={...fenceStamp,actionId:{actorId:'nova',worldId:sourceWorldId,ordinal},deliveryId:`delivery-${ordinal}`};
  return {version:1,type:'offer',sequence,fence,activity:'move_to', minimumActiveMs: 0,from:binding.position,
    target:{x:binding.position.x+30,y:binding.position.y+20},maximumSpeed:100,acceleration:500,validForMs:10_000};
}

describe('actual-camera placement, synthetic submitted frames',()=>{
  it('native camera preserves the issued physical size across expanded viewport/DPI without a fit scale',()=>{
    const normalSpan=2*Math.hypot(.55,.7,7.6)*Math.tan(32*Math.PI/360),camera=new THREE.OrthographicCamera();
    const framing:NativePhysicalPolicy={kind:'native_physical_iso_v1',referencePhysicalHeight:500,physicalWidth:1600,
      physicalHeight:1200,scaleFactor:2,normalCamera:'full',normalVerticalSpan:normalSpan,verticalSpan:normalSpan*1200/500};
    expect(configureWorldCamera(camera,800,600,framing)).toBe(true);
    expect(camera.top-camera.bottom).toBeCloseTo(framing.verticalSpan,12);
    const oneUnitPixels=600/framing.verticalSpan*framing.scaleFactor;
    expect(oneUnitPixels).toBeCloseTo(500/normalSpan,12);
    expect(configureWorldCamera(camera,799,600,framing)).toBe(false);
    expect(configureWorldCamera(camera,800,601,framing)).toBe(false);
  });
  it('same-stamp native camera-policy changes cancel instead of retaining old movement or adopting a new fit',()=>{
    const f=fixture(),normalSpan=2*Math.hypot(.55,.7,7.6)*Math.tan(32*Math.PI/360);
    const policy:NativePhysicalPolicy={kind:'native_physical_iso_v1',referencePhysicalHeight:400,physicalWidth:400,physicalHeight:400,
      scaleFactor:1,normalCamera:'full',normalVerticalSpan:normalSpan,verticalSpan:normalSpan};
    // A changed policy under the already established Studio stamp cannot be
    // used to replace that stamp's mapping or reset its cancellation atom.
    f.update({worldChallenge:{...challenge(),cameraPolicy:'native_physical_iso_v1',nativeFraming:policy,expectedReferenceLocal:{x:0,y:1.45,z:0}}});
    expect(f.reasons).toContain('stale_binding');
    expect(f.controller.prepare(17,400,400,true,.5,true,0,false)).toBeNull();
  });
  it('calibrates the loaded asset forward axis independently of an old presentation yaw',()=>{
    const scene=new THREE.Scene(),presentation=new THREE.Group(),imported=new THREE.Group(),normalization=new THREE.Group(),rig=new THREE.Object3D();
    scene.add(presentation);presentation.add(imported);imported.add(normalization);normalization.add(rig);
    normalization.rotation.y=.23*Math.PI;rig.rotation.y=.17;const axis=new THREE.Vector3(0,0,1);
    for(let i=0;i<8;i++){
      presentation.rotation.y=i*Math.PI/4;scene.updateMatrixWorld(true);
      const world=axis.clone().applyQuaternion(rig.getWorldQuaternion(new THREE.Quaternion()));
      expect(placementForwardYaw(imported,world)).toBeCloseTo(.23*Math.PI+.17,8);
    }
    expect(placementForwardYaw(imported,new THREE.Vector3(0,1,0))).toBeNull();
  });
  it.each([.3,1,2.5])('derives yaw-independent ground axes for all eight directions at actual scale %s',scale=>{
    const f=fixture(),camera=f.controller.camera;configureWorldCamera(camera,400,400);f.root.scale.setScalar(scale);
    const reference={x:0,y:.5,z:0};
    for(let facing=0;facing<8;facing++){
      f.heading.position.set(0,.5,0);f.offset.position.set(0,-.5,0);f.heading.rotation.y=facing*Math.PI/4;
      for(let direction=0;direction<8;direction++){
        const x=200+Math.cos(direction*Math.PI/4)*10,y=200+Math.sin(direction*Math.PI/4)*10;
        expect(placeWorldReference(f.root,camera,reference,{x,y},surface.viewport)).toBe(true);
        const probe=new WorldRenderGeometry(),binding={...expected,scale};
        expect(probe.bind(binding,f.root,camera,f.root.matrixWorld,new THREE.Vector3(0,.5,0))).toBe(true);
        const frame={binding,frameSequence:1};const result=probe.renderFrame(frame,f.scene,f.port,()=>surface,()=>frame,()=>0);
        expect(result.available).toBe(true);if(!result.available)throw new Error(result.reason);
        expect(result.measurement.position.x).toBeCloseTo(x,8);expect(result.measurement.position.y).toBeCloseTo(y,8);
        expect(result.measurement.groundBasis.right.x).toBeCloseTo(.02/scale,8);
        expect(result.measurement.groundBasis.right.z).toBeCloseTo(0,8);
        expect(result.measurement.groundBasis.down.x).toBeCloseTo(0,8);
        expect(result.measurement.groundBasis.down.z).toBeCloseTo(.04/scale,8);
      }
    }
  });
  it('calibrates 32 real submitted poses, then requires a NEW exposed idle frame',()=>{
    const f=fixture();for(let i=0;i<33;i++)expect(f.render().result).toBe('committed');
    expect(f.prepared()).toBe(1);expect(f.measurements).toHaveLength(0);
    const coveredCount=f.state.renders;f.update({worldExposed:true});expect(f.measurements).toHaveLength(0);
    expect(f.render().result).toBe('committed');expect(f.state.renders).toBe(coveredCount+1);
    expect(f.measurements).toHaveLength(1);expect(f.measurements[0].frameSequence).toBe(34);
    expect(f.frames).toHaveLength(0); // Calibration and readiness do not start actions.
  });
  it('keeps cooperative rigid preparation covered and uncommitted before 32 actual calibration poses and a new exposed idle draw',()=>{
    const f=fixture(new THREE.SphereGeometry(.5,16,8));expect(f.render().result).toBe('preparing');
    expect(f.prepared()).toBe(0);expect(f.measurements).toHaveLength(0);expect(f.frames).toHaveLength(0);expect(f.state.renders).toBe(1);
    for(let i=0;i<33;i++)expect(f.render().result).toBe('committed');expect(f.prepared()).toBe(1);
    expect(f.measurements).toHaveLength(0);expect(f.frames).toHaveLength(0);const previous=f.state.renders;
    f.update({worldExposed:true});expect(f.render().result).toBe('committed');expect(f.state.renders).toBe(previous+1);
    expect(f.measurements).toHaveLength(1);expect(f.frames).toHaveLength(0);
  });
  it('does not extend a native challenge expiry while local rigid preparation is covered',()=>{
    const f=fixture(new THREE.SphereGeometry(.5,16,8));expect(f.render().result).toBe('preparing');
    expect(f.controller.prepare(f.now()+5000,400,400,true,.5,true,0,false)).toBeNull();
    expect(f.reasons).toContain('expired');expect(f.prepared()).toBe(0);expect(f.measurements).toHaveLength(0);expect(f.frames).toHaveLength(0);
  });
  it('holds movement for the exact Start ACK, while pause cannot be resumed by that ACK',()=>{
    const f=fixture(),binding=f.ready(),offer=move(binding);
    expect(f.controller.command(offer,f.now())).toBe('accepted');f.render();
    expect(f.frames.at(-1)?.purpose).toBe('delivery');expect(f.controller.begin(offer.fence,offer.sequence,f.now())).toBe(true);
    const firstTrial=f.render(50).trial;const first=f.frames.at(-1)!;expect(first.purpose).toBe('first_behavior');
    expect(firstTrial.walking).toBe(true);expect(firstTrial.proposal?.groundDistance).toBe(0);
    expect(first.anchor.x).toBeCloseTo(offer.from.x,8);expect(first.anchor.y).toBeCloseTo(offer.from.y,8);
    expect(first.walking).toBe(false); // Primed clip, no observed translation before Store Start.
    const anchor={...first.anchor};for(let i=0;i<20;i++){f.render();expect(f.frames.at(-1)?.anchor).toEqual(anchor);}
    expect(f.frames.at(-1)?.purpose).toBe('start_hold');
    expect(f.controller.command({version:1,sequence:2,type:'pause',fence:offer.fence},f.now())).toBe('accepted');
    expect(f.controller.confirmStarted(offer.fence,first.frameSequence+1)).toBe(false);
    expect(f.controller.confirmStarted(offer.fence,first.frameSequence)).toBe(true);
    for(let i=0;i<3;i++){f.render();expect(f.frames.at(-1)?.anchor).toEqual(anchor);}
    expect(f.controller.command({version:1,sequence:3,type:'resume',fence:offer.fence},f.now())).toBe('accepted');
    f.render();expect(f.frames.at(-1)?.walking).toBe(true);
  });
  it('renews idle measurement without recalibrating or resetting a current exact action',()=>{
    const f=fixture(),binding=f.ready(),offer=move(binding);f.controller.command(offer,f.now());f.render();f.controller.begin(offer.fence,1,f.now());
    f.render();const first=f.frames.at(-1)!;f.controller.confirmStarted(offer.fence,first.frameSequence);f.render();
    const oldFrame=f.frames.at(-1)!,count=f.prepared();
    f.update({worldChallenge:{...f.options().worldChallenge!,id:'challenge-renew',expectedReferenceLocal:binding.referenceLocal}});
    f.render();expect(f.measurements.at(-1)?.challengeId).toBe('challenge-renew');expect(f.prepared()).toBe(count);
    f.render();expect(f.frames.at(-1)!.frameSequence).toBeGreaterThan(oldFrame.frameSequence);
    expect(f.frames.at(-1)!.anchor.x).toBeGreaterThanOrEqual(oldFrame.anchor.x);
  });
  it('rolls back a failed moving frame, emits no observation and cannot resume through a stale ACK',()=>{
    const f=fixture(),binding=f.ready(),offer=move(binding);f.controller.command(offer,f.now());f.render();f.controller.begin(offer.fence,1,f.now());f.render();
    const first=f.frames.at(-1)!;f.controller.confirmStarted(offer.fence,first.frameSequence);const before=f.root.position.clone(),count=f.frames.length;
    f.state.throwAfter=true;expect(f.render().result).toBe('failed');expect(f.root.position.equals(before)).toBe(true);
    expect(f.frames).toHaveLength(count);expect(f.reasons).toContain('unsupported_geometry');
    expect(f.controller.confirmStarted(offer.fence,first.frameSequence)).toBe(false);
    expect(f.controller.prepare(f.now()+17,400,400,true,.5,true,0,false)).toBeNull();
  });
  it('owner loss, strict stride failure and mirror mode cannot emit action receipts',()=>{
    const f=fixture(),binding=f.ready(),offer=move(binding);f.controller.command(offer,f.now());f.render();
    f.update({worldOwner:false});const count=f.frames.length;
    expect(f.controller.begin(offer.fence,1,f.now())).toBe(false);
    expect(f.controller.command(offer,f.now())).toBe('stale');
    expect(f.controller.prepare(f.now()+17,400,400,true,.5,true,0,false)).toBeNull();expect(f.frames).toHaveLength(count);
    f.root.scale.setScalar(2);f.controller.prepare(f.now()+34,400,400,true,.5,true,0,false);expect(f.root.scale.x).toBe(2);
    const unsupported=fixture();expect(unsupported.controller.prepare(1,400,400,true,.5,false,0,false)).toBeNull();expect(unsupported.reasons).toContain('unsupported_stride');
  });
  it('a stale trial cannot restore/cancel the replacement generation',()=>{
    const f=fixture(),trial=f.controller.prepare(17,400,400,true,.5,true,0,false)!;
    f.update({worldChallenge:{...challenge(),id:'challenge-new',expected:{...expected,bindingSequence:2,assetRevision:2}}});
    f.root.position.x=3;const reasons=f.reasons.length;
    expect(f.controller.render(trial,f.scene,f.port,()=>surface,17,()=>0)).toBe('stale');
    expect(f.root.position.x).toBe(3);expect(f.reasons).toHaveLength(reasons);expect(f.state.renders).toBe(0);
  });
  it('keeps a failed hidden World mount suppressed without fallback draws until explicit reconnect',()=>{
    const f=fixture(),domElement={style:{visibility:''}},stage=Object.create(AvatarStage.prototype) as any;
    Object.assign(stage,{options:{motionSource:'world'},world:f.controller,scene:f.scene,
      host:{isConnected:false},camera:new THREE.PerspectiveCamera(),renderer:{...f.port,domElement},assetState:'ready',modelAudio:{stop(){}}});
    stage.recoverWorldPose();expect(domElement.style.visibility).toBe('hidden');
    for(let i=0;i<3;i++)expect(stage.renderOutput(null)).toBe(false);
    expect(f.state.renders).toBe(0);expect(domElement.style.visibility).toBe('hidden');expect(stage.worldRecovery).toBe(true);
    // An explicit reconnect creates a new Stage/controller; the failed instance
    // never infers a safe visible pose or output grant from its fallback render.
    expect(f.frames).toHaveLength(0);expect(f.measurements).toHaveLength(0);
  });
  it.each(['fixed','ambient'])('preserves ordinary %s recovery only after a successful live-context draw',motionSource=>{
    const f=fixture(),domElement={style:{visibility:''},parentElement:null,getBoundingClientRect:()=>({left:0,top:0,right:400,bottom:400})},stage=Object.create(AvatarStage.prototype) as any;
    vi.stubGlobal('document',{visibilityState:'visible'});vi.stubGlobal('window',{innerWidth:1000,innerHeight:800});
    vi.stubGlobal('getComputedStyle',()=>({display:'block',visibility:domElement.style.visibility||'visible',opacity:'1'}));
    Object.assign(stage,{options:{motionSource},world:f.controller,scene:f.scene,
      host:{isConnected:true,clientWidth:400,clientHeight:400},camera:new THREE.PerspectiveCamera(),renderer:{...f.port,domElement},assetState:'ready',modelAudio:{stop(){}}});
    stage.recoverWorldPose();f.state.lost=true;
    expect(stage.renderOutput(null)).toBe(false);expect(domElement.style.visibility).toBe('hidden');
    f.state.lost=false;expect(stage.renderOutput(null)).toBe(true);expect(domElement.style.visibility).toBe('');
    expect(stage.worldRecovery).toBe(false);expect(f.state.renders).toBe(2);
  });
  it('restores bounded displayed transforms and morph weights without claiming physics rollback',()=>{
    const root=new THREE.Group(),geometry=new THREE.BoxGeometry(),mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial());
    mesh.morphTargetInfluences=[.2];root.add(mesh);const snapshot=captureWorldPose(root)!;
    root.position.x=10;mesh.quaternion.setFromAxisAngle(new THREE.Vector3(0,1,0),1);mesh.morphTargetInfluences[0]=.9;
    snapshot.restore();expect(root.position.x).toBe(0);expect(mesh.quaternion.w).toBe(1);expect(mesh.morphTargetInfluences[0]).toBe(.2);
    mesh.morphTargetInfluences=new Array(65).fill(0);expect(captureWorldPose(root)).toBeNull();
  });
});


describe('explicit conservative geometry policy fencing',()=>{
  it('tags only calibrated exposed measurements and current action frames with the pinned kind',()=>{
    const f=fixture(),binding=f.ready(),offer=move(binding);
    expect(f.measurements[0].enclosureKind).toBe('conservative_submitted_geometry_v1');
    expect(f.controller.command(offer,f.now())).toBe('accepted');expect(f.render().result).toBe('committed');
    expect(f.frames.at(-1)?.enclosureKind).toBe('conservative_submitted_geometry_v1');
  });
  it.each([undefined,'exact_submitted_geometry_v1'])('rejects a missing/changed challenge policy before an unchanged-stamp update (%s)',kind=>{
    const f=fixture();expect(f.render().result).toBe('committed');
    const malformed={...f.options().worldChallenge} as Record<string,unknown>;
    if(kind===undefined)delete malformed.enclosureKind;else malformed.enclosureKind=kind;
    f.update({worldChallenge:malformed as unknown as MeasurementChallenge});
    expect(f.reasons).toContain('stale_binding');expect(f.controller.prepare(f.now()+17,400,400,true,.5,true,0,false)).toBeNull();
    expect(f.prepared()).toBe(0);expect(f.measurements).toHaveLength(0);expect(f.frames).toHaveLength(0);
  });
  it.each([undefined,'exact_submitted_geometry_v1'])('cancels a current action when the binding policy changes without a stamp change (%s)',kind=>{
    const f=fixture(),binding=f.ready(),offer=move(binding);
    expect(f.controller.command(offer,f.now())).toBe('accepted');f.render();const before=f.frames.length;
    const malformed={...binding} as Record<string,unknown>;
    if(kind===undefined)delete malformed.enclosureKind;else malformed.enclosureKind=kind;
    f.update({worldBinding:malformed as unknown as MotionBinding});
    expect(f.reasons).toContain('stale_binding');expect(f.controller.begin(offer.fence,offer.sequence,f.now())).toBe(false);
    expect(f.controller.prepare(f.now()+17,400,400,true,.5,true,0,false)).toBeNull();expect(f.frames).toHaveLength(before);
    // Reintroducing the previous valid tag cannot revive this failed mount.
    f.update({worldBinding:binding});expect(f.controller.prepare(f.now()+34,400,400,true,.5,true,0,false)).toBeNull();
  });
});


it('station progress follows only committed target frames after Start and clears on pause/exposure loss',()=>{
  const f=fixture(),binding=f.ready();
  expect(f.measurements[0].stationSupported).toBe(true);
  const offer:MotionOffer={...move(binding),activity:'play',minimumActiveMs:1000,target:binding.position};
  expect(f.controller.command(offer,f.now())).toBe('accepted');f.render();
  expect(f.controller.begin(offer.fence,offer.sequence,f.now())).toBe(true);f.render();
  const first=f.frames.at(-1)!;expect(first.purpose).toBe('first_behavior');expect(first.walking).toBe(false);
  expect(f.controller.confirmStarted(offer.fence,first.frameSequence)).toBe(true);f.render();
  expect(f.activities.at(-1)?.activeMs).toBe(0);
  for(let index=0;index<3;index++)f.render(200);
  expect(f.activities.at(-1)?.activeMs).toBe(600);
  expect(f.controller.command({version:1,type:'pause',sequence:2,fence:offer.fence},f.now())).toBe('accepted');
  expect(f.activities.at(-1)).toBeUndefined();f.render();expect(f.activities.at(-1)).toBeUndefined();
  expect(f.controller.command({version:1,type:'resume',sequence:3,fence:offer.fence},f.now())).toBe('accepted');
  f.render();expect(f.activities.at(-1)?.activeMs).toBe(0);
  f.update({worldExposed:false});expect(f.activities.at(-1)).toBeUndefined();
  f.controller.dispose();expect(f.activities.at(-1)).toBeUndefined();
});

it('composes a real authored Pet clip with successful geometry frames and both native ACK barriers',()=>{
  const f=fixture(),binding=f.ready();f.mesh.name='pet-target';
  const clip=new THREE.AnimationClip('Pet',.2,[new THREE.VectorKeyframeTrack('pet-target.scale',[0,.2],[1,1,1,.99,1,1])]);
  const mixer=new THREE.AnimationMixer(f.root),spec={action:'Pet',durationMs:200},player=new WorldPetPlayer(mixer,clip,spec);
  const offer:MotionOffer={...move(binding),activity:'pet',minimumActiveMs:200,target:binding.position,petClip:spec};
  expect(f.controller.command(offer,f.now())).toBe('accepted');
  const render=()=>{const result=f.render(50,(trial,now)=>{const pet=player.prepare(now,f.controller.petPlayback!.running);mixer.update(0);return{...trial,pet};});
    if(result.result==='committed')expect(player.commit(result.trial.pet!)).toBe(true);else player.rollback();return result;};
  expect(render().result).toBe('committed');expect(f.frames.at(-1)?.pet?.elapsedMs).toBe(0);
  expect(f.controller.begin(offer.fence,offer.sequence,f.now())).toBe(true);
  expect(render().result).toBe('committed');const first=f.frames.at(-1)!;
  expect(first.purpose).toBe('first_behavior');expect(first.pet?.elapsedMs).toBe(0);expect(f.mesh.scale.x).toBe(1);
  expect(f.controller.confirmStarted(offer.fence,first.frameSequence)).toBe(true);
  for(let index=0;index<4;index++)expect(render().result).toBe('committed');
  expect(f.frames.at(-1)?.pet).toEqual({action:'Pet',elapsedMs:200,finished:true});expect(f.mesh.scale.x).toBeCloseTo(.99);
  const count=f.frames.length;f.state.throwAfter=true;expect(render().result).toBe('failed');
  expect(f.frames).toHaveLength(count);expect(f.controller.petPlayback).toBeNull();player.stop();
});

it('never substitutes stationary idle frames when an admitted Pet clip observation is absent',()=>{
  const f=fixture(),binding=f.ready();
  const offer:MotionOffer={...move(binding),activity:'pet',minimumActiveMs:200,target:binding.position,petClip:{action:'Pet',durationMs:200}};
  expect(f.controller.command(offer,f.now())).toBe('accepted');
  expect(f.render().result).toBe('failed');expect(f.frames).toEqual([]);expect(f.reasons).toContain('unsupported_capability');
});
