// SPDX-License-Identifier: MIT
/** Local synchronous 3D adapter. The bridge owns IPC/native acknowledgements;
 * successful submitted geometry is observation, never native arrival. */
import * as THREE from 'three';
import { configureDesktopCamera } from './desktop-framing';
import { DistanceWalkClock } from './avatar-locomotion';
import { containsPosedEnvelope, WorldMotionFollower, type MotionBinding,
  type MotionFence, type MotionOffer, type MotionProposal, type ReceiveResult, type StationActivity, type WorldMotionCommand } from './world-motion';
import { CONSERVATIVE_ENCLOSURE_KIND, WorldRenderGeometry, type GeometryBinding, type GeometryFrame, type GeometryMeasurement,
  type RenderPort, type SurfaceObservation } from './world-render-geometry';
import { parseMeasurementChallenge, parseWorldMotionBinding } from './world-output';
import type { NativeMotionBinding, CanonicalPoint, EnvelopeExtents, LocalReference, MeasurementChallenge,
  NativePhysicalPolicy, RenderMeasurement, WorldAdapterIdentity, WorldAdapterReason, LocalWorldFrame } from './world-output';

export interface WorldAvatarOptions {
  worldOwner?: boolean; worldChallenge?: MeasurementChallenge; worldBinding?: NativeMotionBinding;
  worldExposed?: boolean; worldMirrorPosition?: CanonicalPoint;
  onWorldPrepared?: (identity: WorldAdapterIdentity) => void;
  onWorldMeasurement?: (report: RenderMeasurement) => void;
  onWorldFrame?: (frame: LocalWorldFrame) => void;
  onWorldUnavailable?: (identity: WorldAdapterIdentity, reason: WorldAdapterReason) => void;
  onWorldActivity?: (activity: WorldStationFrame | undefined) => void;
}
/** Display-only successful-frame observation; never an action receipt. */
export type WorldStationFrame = Readonly<{ activity: StationActivity; activeMs: number;
  minimumActiveMs: number; position: Readonly<{ x: number; y: number }> }>;
export type WorldPoseSnapshot = { restore(): void };
export type WorldTrial = Readonly<{ frame: GeometryFrame; proposal: MotionProposal | null;
  walking: boolean; cycles: number; calibration: boolean; generation: number; snapshot: WorldPoseSnapshot }>;
export type WorldRenderResult = 'committed'|'preparing'|'failed'|'stale';
const CALIBRATION_FRAMES = 32;
const NUMERICAL_GUARD = .5;
const stamp = (value: GeometryBinding | MotionBinding | MeasurementChallenge['expected']) =>
  [value.actorId,value.sourceWorldId,value.renderSessionId,value.bindingSequence,value.worldInstanceId,
    value.stageId,value.configRevision,value.topologyRevision,value.layoutRevision,value.assetRevision,
    value.measurementRevision,value.referenceRevision].join(':');
const equalReference = (a: LocalReference, b: LocalReference) => a.x === b.x && a.y === b.y && a.z === b.z;
const equalFence = (a: MotionFence, b: MotionFence) => stamp({ ...a, sourceWorldId:a.actionId.worldId }) === stamp({ ...b, sourceWorldId:b.actionId.worldId })
  && a.deliveryId === b.deliveryId && a.actionId.ordinal === b.actionId.ordinal;
const emptyEnvelope = (): EnvelopeExtents => ({ left:0,top:0,right:0,bottom:0 });
export function configureWorldCamera(camera: THREE.OrthographicCamera, width: number, height: number, framing?:NativePhysicalPolicy): boolean {
  if (![width,height].every(n=>Number.isFinite(n)&&n>0&&n<=1_000_000)) return false;
  if(framing&&(Math.abs(width-framing.physicalWidth/framing.scaleFactor)>.5||Math.abs(height-framing.physicalHeight/framing.scaleFactor)>.5))return false;
  const half=(framing?.verticalSpan??8)/2;
  camera.left=-half*width/height;camera.right=half*width/height;camera.top=half;camera.bottom=-half;
  camera.near=.05;camera.far=100;configureDesktopCamera(camera);return true;
}
/** Remove presentation/parent facing from an actually discovered asset axis. */
export function placementForwardYaw(root:THREE.Object3D,worldForward:THREE.Vector3):number|null{
  const forward=worldForward.clone().applyQuaternion(root.getWorldQuaternion(new THREE.Quaternion()).invert());
  return forward.toArray().every(Number.isFinite)&&Math.hypot(forward.x,forward.z)>1e-8
    ? Math.atan2(forward.x,forward.z):null;
}
/** Ground frame contains actual uniform scale/translation, never actor yaw. */
export function placeWorldReference(root: THREE.Object3D, camera: THREE.OrthographicCamera,
  reference: LocalReference, anchor: {x:number;y:number}, viewport:{width:number;height:number}): boolean {
  if (![root.scale.x,root.scale.y,root.scale.z,anchor.x,anchor.y,...Object.values(reference)].every(Number.isFinite)
    || root.scale.x<=0||root.scale.x!==root.scale.y||root.scale.x!==root.scale.z) return false;
  const inverse=new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix,camera.matrixWorldInverse).invert();
  const near=new THREE.Vector3(anchor.x/viewport.width*2-1,1-anchor.y/viewport.height*2,-1).applyMatrix4(inverse);
  const far=new THREE.Vector3(anchor.x/viewport.width*2-1,1-anchor.y/viewport.height*2,1).applyMatrix4(inverse);
  const plane=new THREE.Plane(new THREE.Vector3(0,1,0),-reference.y*root.scale.y);
  const hit=new THREE.Ray(near,far.sub(near).normalize()).intersectPlane(plane,new THREE.Vector3());
  if(!hit||!hit.toArray().every(Number.isFinite))return false;
  root.position.set(hit.x-reference.x*root.scale.x,0,hit.z-reference.z*root.scale.z);
  root.rotation.set(0,0,0);root.updateWorldMatrix(true,true);return true;
}
/** Bounded displayed-pose rollback, not a snapshot of mixer/spring internals. */
export function captureWorldPose(root: THREE.Object3D): WorldPoseSnapshot | null {
  const pending=[root],nodes:{node:THREE.Object3D;p:THREE.Vector3;q:THREE.Quaternion;s:THREE.Vector3;m?:number[]}[]=[];
  while(pending.length){
    const node=pending.pop()!;if(!node.visible)continue;
    if(nodes.length+pending.length+node.children.length>=2048)return null;
    const mesh=node as THREE.Mesh;const morphs=mesh.morphTargetInfluences;
    if(morphs&&morphs.length>64)return null;
    nodes.push({node,p:node.position.clone(),q:node.quaternion.clone(),s:node.scale.clone(),m:morphs?.slice()});
    pending.push(...node.children);
  }
  return {restore(){for(const row of nodes){row.node.position.copy(row.p);row.node.quaternion.copy(row.q);row.node.scale.copy(row.s);
    if(row.m){const values=(row.node as THREE.Mesh).morphTargetInfluences;if(values)row.m.forEach((v,i)=>values[i]=v);}}
    root.updateWorldMatrix(true,true);}};
}

export class WorldAvatarController {
  readonly camera=new THREE.OrthographicCamera(-4,4,4,-4,.05,100);
  private options:WorldAvatarOptions={};private key='';private generation=0;private blocked=false;
  private probe=new WorldRenderGeometry(CONSERVATIVE_ENCLOSURE_KIND);private geometryBinding?:GeometryBinding;private reference?:LocalReference;
  private envelope=emptyEnvelope();private calibration=-1;private prepared=false;private sentChallenge='';
  private challenge?:MeasurementChallenge;private challengeAt=0;private accepted?:MotionBinding;
  private follower=new WorldMotionFollower();private offer?:MotionOffer;private firstFrame=0;private firstFrameAt=0;private started=false;
  private delivered=false;private frameSequence=0;private clock=new DistanceWalkClock();private stride=1;
  private forward=0;private viewport={width:0,height:0};private reduced=false;
  private framing?:NativePhysicalPolicy;private policyKey='';
  private stationSince:number|undefined;private stationLast:number|undefined;
  constructor(readonly root:THREE.Group,readonly heading:THREE.Group,readonly offset:THREE.Group){}
  get identity():WorldAdapterIdentity|undefined{
    const value=this.accepted??this.challenge?.expected;
    if(!value)return undefined;
    const {actorId,sourceWorldId,renderSessionId,bindingSequence,worldInstanceId,stageId,configRevision,
      topologyRevision,layoutRevision,assetRevision,measurementRevision,referenceRevision}=value;
    return {actorId,sourceWorldId,renderSessionId,bindingSequence,worldInstanceId,stageId,configRevision,
      topologyRevision,layoutRevision,assetRevision,measurementRevision,referenceRevision,challengeId:this.challenge?.id??null};
  }
  update(options:WorldAvatarOptions,now:number):void{
    if(this.options.worldOwner&&!options.worldOwner)this.cancel('cancelled');
    this.options=options;
    if(!options.worldOwner||!options.worldExposed)this.clearStation();
    let challenge:MeasurementChallenge|undefined;
    try{challenge=options.worldChallenge?parseMeasurementChallenge(options.worldChallenge):undefined;}catch{this.cancel('stale_binding');return;}
    const nativeBinding=options.worldBinding?parseWorldMotionBinding(options.worldBinding):undefined;
    // Policy is checked before stamp/owner/unchanged-binding early returns.
    // A missing or changed kind cannot retain a previously accepted action.
    if(challenge&&challenge.enclosureKind!==CONSERVATIVE_ENCLOSURE_KIND||options.worldBinding&&!nativeBinding){this.cancel('stale_binding');return;}
    let binding:MotionBinding|undefined;
    if(nativeBinding){
      const {enclosureKind,cameraPolicy:_camera,nativeFraming:_framing,...pureBinding}=nativeBinding;
      if(enclosureKind!==CONSERVATIVE_ENCLOSURE_KIND){this.cancel('stale_binding');return;}
      binding=pureBinding; // The pure follower parser never accepts wire tags.
    }
    const key=challenge?stamp(challenge.expected):binding?stamp(binding):'';
    const framing=challenge?.nativeFraming??nativeBinding?.nativeFraming;
    const policyKey=JSON.stringify([framing??null]);
    if(key&&key===this.key&&policyKey!==this.policyKey){this.cancel('stale_binding');return;}
    if(challenge&&nativeBinding&&JSON.stringify(challenge.nativeFraming??null)!==JSON.stringify(nativeBinding.nativeFraming??null)){this.cancel('stale_binding');return;}
    if(key!==this.key){this.reset();this.key=key;this.policyKey=policyKey;this.framing=framing;this.blocked=false;this.generation++;}
    if(challenge&&challenge.id!==this.challenge?.id){
      this.challenge=challenge;this.challengeAt=now;this.sentChallenge='';
      if(this.reference&&challenge.expectedReferenceLocal&&!equalReference(this.reference,challenge.expectedReferenceLocal))this.cancel('stale_binding');
    }
    if(binding&&!options.worldOwner){
      const parsed=binding;this.accepted=parsed;this.reference={...parsed.referenceLocal};this.root.scale.setScalar(parsed.scale);
      return;
    }
    if(binding&&(!this.accepted||stamp(binding)!==stamp(this.accepted))){
      const parsed=binding;
      if(!parsed||!this.reference||!equalReference(parsed.referenceLocal,this.reference)
        ||this.geometryBinding?.scale!==parsed.scale){this.cancel('stale_binding');return;}
      this.accepted=parsed;this.envelope={...parsed.envelopeExtents};
      if(options.worldOwner&&!this.follower.bind(parsed,now)){this.cancel('stale_binding');return;}
    }
  }
  private reset():void{
    this.clearStation();
    this.probe=new WorldRenderGeometry(CONSERVATIVE_ENCLOSURE_KIND);this.geometryBinding=undefined;this.reference=undefined;this.envelope=emptyEnvelope();
    this.calibration=-1;this.prepared=false;this.sentChallenge='';this.challenge=undefined;this.accepted=undefined;
    this.follower.dispose();this.follower=new WorldMotionFollower();this.offer=undefined;this.firstFrame=0;this.started=false;this.delivered=false;this.clock.reset();
    this.heading.position.set(0,0,0);this.heading.rotation.set(0,0,0);this.offset.position.set(0,0,0);
  }
  cancel(reason:WorldAdapterReason='cancelled'):void{
    this.clearStation();
    const notify=!this.blocked;
    this.blocked=true;this.generation++;this.follower.invalidate();this.offer=undefined;this.delivered=false;this.started=false;
    const identity=this.identity;if(identity&&notify)this.options.onWorldUnavailable?.(identity,reason);
  }
  dispose():void{this.clearStation();this.blocked=true;this.generation++;this.follower.dispose();this.probe.invalidate();}
  private clearStation():void{
    this.stationSince=undefined;this.stationLast=undefined;this.options.onWorldActivity?.(undefined);
  }
  command(command:WorldMotionCommand,now:number):ReceiveResult{
    if(!this.options.worldOwner||this.blocked||!this.accepted)return 'stale';
    const result=this.follower.receive(command,now);
    if(result==='accepted')this.clearStation();
    if(result==='accepted'&&command.type==='offer'){this.offer=command;this.delivered=false;this.firstFrame=0;this.started=false;this.clock.reset(command.fence.deliveryId);}
    if(result==='accepted'&&command.type==='terminal'&&this.offer
      &&command.fence.actionId.ordinal>=this.offer.fence.actionId.ordinal){this.offer=undefined;this.delivered=false;this.started=false;}
    if(result==='accepted'&&command.type==='pause'&&this.follower.snapshot().phase==='idle'){this.offer=undefined;this.delivered=false;}
    return result;
  }
  begin(fence:MotionFence,sequence:number,now:number):boolean{
    if(!this.options.worldOwner||this.blocked||!this.offer||!equalFence(this.offer.fence,fence))return false;
    return this.delivered=this.follower.beginAfterDelivered(fence,sequence,now);
  }
  confirmStarted(fence:MotionFence,frame:number):boolean{
    if(!this.options.worldOwner||this.blocked||!this.offer||!equalFence(this.offer.fence,fence)||frame!==this.firstFrame||!frame)return false;
    this.started=true;return true;
  }
  prepare(now:number,width:number,height:number,ready:boolean,stride:number,walkSupported:boolean,forward:number,reduced:boolean):WorldTrial|null{
    if(!configureWorldCamera(this.camera,width,height,this.framing)){this.cancel('surface_unavailable');return null;}
    const resized=this.viewport.width>0&&(this.viewport.width!==width||this.viewport.height!==height);
    if(resized||this.reduced!==reduced&&!!this.geometryBinding){this.viewport={width,height};this.cancel('surface_unavailable');return null;}
    this.viewport={width,height};this.reduced=reduced;this.forward=forward;this.stride=stride;
    if(!this.options.worldOwner){this.positionMirror();return null;}
    if(this.blocked||!ready)return null;
    if(!this.challenge&&!this.accepted){this.positionMirror();return null;}
    if(!walkSupported||!Number.isFinite(stride)||stride<=0){this.cancel('unsupported_stride');return null;}
    if(reduced){this.cancel('unsupported_capability');return null;}
    if(this.firstFrame&&!this.started&&now-this.firstFrameAt>5000){this.cancel('expired');return null;}
    if(this.challenge&&this.sentChallenge!==this.challenge.id&&now-this.challengeAt>=this.challenge.expiresInMs){this.cancel('expired');return null;}
    if(!this.geometryBinding){
      const c=this.challenge;if(!c)return null;
      this.root.scale.setScalar(c.effectiveScale);this.root.rotation.set(0,0,0);
      this.reference=c.expectedReferenceLocal?{...c.expectedReferenceLocal}:undefined;
      this.geometryBinding={...c.expected,scale:c.effectiveScale};
      this.root.updateWorldMatrix(true,true);
      if(!this.probe.bind(this.geometryBinding,this.root,this.camera,this.root.matrixWorld,this.reference?new THREE.Vector3(this.reference.x,this.reference.y,this.reference.z):undefined)){
        this.cancel('unsupported_geometry');return null;
      }
    }
    const snapshot=captureWorldPose(this.root);if(!snapshot){this.cancel('unsupported_geometry');return null;}
    let proposal:MotionProposal|null=null,walking=false,cycles=this.clock.cycles,yaw=0;
    if(this.prepared&&this.accepted&&this.offer&&this.delivered){
      const renewal=!!this.challenge&&this.sentChallenge!==this.challenge.id;
      // Prime the first behavior pose at the frozen origin. Native Store Start
      // must be acknowledged before any displacement or distance-gait credit.
      proposal=this.follower.sample(now,renewal||!this.started?0:.05);
      if(!proposal){snapshot.restore();this.cancel('cancelled');return null;}
      walking=proposal.walking||!this.firstFrame&&this.offer.activity==='move_to';cycles+=proposal.groundDistance/stride;yaw=proposal.yaw;
    }else if(!this.prepared&&this.calibration>=0){walking=true;cycles=(this.calibration%4)/4;yaw=Math.floor(this.calibration/4)*Math.PI/4;}
    const anchor=proposal?.position??(this.accepted?(this.follower.snapshot().position??this.accepted.position):this.mirrorAnchor());
    const reference=this.reference??{x:0,y:0,z:0};
    const actualAnchor=this.reference?anchor:{x:width/2,y:height*.9};
    this.heading.position.set(reference.x,reference.y,reference.z);this.heading.rotation.set(0,yaw,0);
    this.offset.position.set(-reference.x,-reference.y,-reference.z);
    if(!placeWorldReference(this.root,this.camera,reference,actualAnchor,this.viewport)){snapshot.restore();this.cancel('unsupported_geometry');return null;}
    if(this.frameSequence>=Number.MAX_SAFE_INTEGER){snapshot.restore();this.cancel('capacity');return null;}
    return {frame:{binding:this.geometryBinding,frameSequence:++this.frameSequence},proposal,walking,cycles,
      calibration:!this.prepared,generation:this.generation,snapshot};
  }
  private mirrorAnchor(){const point=this.options.worldMirrorPosition??{x:500_000,y:500_000};return {x:point.x/1_000_000*this.viewport.width,y:point.y/1_000_000*this.viewport.height};}
  private positionMirror(){const r=this.reference??{x:0,y:1.45,z:0};this.heading.position.set(r.x,r.y,r.z);this.offset.position.set(-r.x,-r.y,-r.z);
    placeWorldReference(this.root,this.camera,r,this.mirrorAnchor(),this.viewport);}
  render(trial:WorldTrial,scene:THREE.Scene,renderer:RenderPort,observe:()=>SurfaceObservation,now:number,clock:()=>number):WorldRenderResult{
    if(trial.generation!==this.generation)return 'stale';
    this.root.updateWorldMatrix(true,true);
    // The frozen orthographic ground basis is independent of both heading and
    // translation. Its scale/camera/layout cannot change under this binding.
    const result=this.probe.renderFrame(trial.frame,scene,renderer,observe,()=>({binding:this.geometryBinding!,frameSequence:this.frameSequence}),clock,
      {allowCoveredPreparation:!this.prepared&&!this.accepted&&!this.options.worldExposed});
    if(trial.generation!==this.generation)return 'stale';
    if(!result.available&&result.preparing){trial.snapshot.restore();return 'preparing';}
    if(!result.available){trial.snapshot.restore();this.cancel('unsupported_geometry');return 'failed';}
    const m=result.measurement;
    if(m.enclosureKind!==CONSERVATIVE_ENCLOSURE_KIND){trial.snapshot.restore();this.cancel('unsupported_geometry');return 'failed';}
    if(!this.reference){this.reference={...m.referenceLocal};this.calibration=0;
      for(const k of ['left','top','right','bottom'] as const)this.envelope[k]=m.envelopeExtents[k]+NUMERICAL_GUARD;
      return 'committed';}
    if(!this.prepared){
      for(const k of ['left','top','right','bottom'] as const)this.envelope[k]=Math.max(this.envelope[k],m.envelopeExtents[k]+NUMERICAL_GUARD);
      if(++this.calibration>=CALIBRATION_FRAMES){this.prepared=true;const identity=this.identity;if(identity)this.options.onWorldPrepared?.(identity);}
      return 'committed';
    }
    if(!containsPosedEnvelope(m.position,m.posedBounds,this.envelope,.01)){trial.snapshot.restore();this.cancel('unsupported_geometry');return 'failed';}
    let walking=false,idleFrames=0,purpose:LocalWorldFrame['purpose']='delivery';
    if(trial.proposal){
      const committed=this.follower.afterSuccessfulRender(trial.proposal.token,{fence:trial.proposal.fence,frameSequence:trial.frame.frameSequence,now,
        position:m.position,viewport:m.viewport,posedBounds:m.posedBounds});
      if(!committed){trial.snapshot.restore();this.cancel('render_failed');return 'failed';}
      this.clock.advance(committed.groundDistance,this.stride,this.offer!.fence.deliveryId);walking=committed.walking;idleFrames=committed.targetIdleFrames;
    }
    const challenge=this.challenge;
    if(challenge&&this.sentChallenge!==challenge.id&&this.options.worldExposed){
      if(this.options.worldOwner)this.publishMeasurement(challenge,m);
      return 'committed';
    }
    if(!this.options.worldOwner||!this.options.worldExposed||!this.offer||!this.accepted){this.clearStation();return 'committed';}
    if(this.offer.activity!=='move_to'&&this.started&&idleFrames>0&&!walking){
      if(this.stationLast===undefined||now<this.stationLast||now-this.stationLast>250)this.stationSince=now;
      this.stationLast=now;
      this.options.onWorldActivity?.({activity:this.offer.activity,minimumActiveMs:this.offer.minimumActiveMs,
        activeMs:Math.min(this.offer.minimumActiveMs,Math.max(0,now-this.stationSince!)),position:{...m.position}});
    }else this.clearStation();
    if(trial.proposal){
      if(!this.firstFrame){this.firstFrame=m.frame.frameSequence;this.firstFrameAt=now;purpose='first_behavior';}
      else purpose=this.started?'progress':'start_hold';
    }
    this.options.onWorldFrame?.({enclosureKind:CONSERVATIVE_ENCLOSURE_KIND,fence:this.offer.fence,frameSequence:m.frame.frameSequence,viewport:m.viewport,clip:m.clip,
      anchor:m.position,posedBounds:m.posedBounds,walking,targetIdleFrames:idleFrames,purpose});
    return 'committed';
  }
  private publishMeasurement(challenge:MeasurementChallenge,m:GeometryMeasurement):void{
    if(!this.reference||!this.options.worldOwner)return;
    if(challenge.enclosureKind!==CONSERVATIVE_ENCLOSURE_KIND||m.enclosureKind!==CONSERVATIVE_ENCLOSURE_KIND){this.cancel('stale_binding');return;}
    this.sentChallenge=challenge.id;
    this.options.onWorldMeasurement?.({version:1,enclosureKind:CONSERVATIVE_ENCLOSURE_KIND,challengeId:challenge.id,frameSequence:m.frame.frameSequence,
      viewport:m.viewport,clip:m.clip,anchor:m.position,posedBounds:m.posedBounds,envelopeExtents:{...this.envelope},referenceLocal:{...this.reference},
      groundBasis:m.groundBasis,modelForwardYaw:this.forward,appliedModelScale:this.root.scale.x,reducedMotion:this.reduced,walkSupported:true,
      stationSupported:typeof this.options.onWorldActivity==='function'});
  }
}
