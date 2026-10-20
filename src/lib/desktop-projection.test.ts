// SPDX-License-Identifier: MIT
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { AvatarStage, createRobot, type AvatarOptions } from './avatar';
import { configureDesktopCamera, desktopFraming, measureDesktopModel } from './desktop-framing';
import { desktopSafeBounds } from './desktop-motion';
import { DistanceWalkClock, proceduralCycleDistance } from './avatar-locomotion';

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function corners(center:THREE.Vector3,size:THREE.Vector3) {
  const points:THREE.Vector3[]=[];
  for(const x of [-1,1]) for(const y of [-1,1]) for(const z of [-1,1]) {
    points.push(center.clone().add(new THREE.Vector3(x*size.x/2,y*size.y/2,z*size.z/2)));
  }
  return points;
}
function css(point:THREE.Vector3,camera:THREE.Camera,width:number,height:number) {
  const projected=point.clone().project(camera);
  return {x:(projected.x+1)*width/2,y:(1-projected.y)*height/2,z:projected.z};
}
function plane(width:number,height:number,depth:number) {
  const root=new THREE.Group();
  const mesh=new THREE.Mesh(depth===0?new THREE.PlaneGeometry(width,height):new THREE.BoxGeometry(width,height,depth));
  mesh.position.set(.37,height/2+.08,-.24);
  root.add(mesh);
  return root;
}

/** Use the real Stage methods, scene graph and camera matrices. Only GPU,
 * controls input and animation scheduling are absent from this harness. */
function stageFixture(root?:THREE.Group,width=1920,height=1080,pixelRatio=1) {
  vi.stubGlobal('window',{devicePixelRatio:pixelRatio});
  vi.spyOn(Date,'now').mockReturnValue(10_000);
  const robot=createRobot(), imported=root??new THREE.Group();
  const model=root??robot.root, bounds=measureDesktopModel(model);
  robot.root.visible=!root;
  const travel=new THREE.Group(); travel.add(robot.root,imported);
  const camera=new THREE.PerspectiveCamera(32,width/height,.05,100);
  const target=new THREE.Vector3();
  const controls={target,enabled:false,update:()=>{camera.lookAt(target);camera.updateMatrixWorld();}};
  const options:AvatarOptions={profile:{avatar:{scale:1,camera:'portrait',accent:'#80b8e8'}},transparent:true,speaking:false,level:0,emotion:'neutral',autopilot:{active:true,wander:true,gestures:false,startedAt:1000,seed:'projection-fixture'}};
  const point={x:width/2,y:height/2,walking:false,facing:1,travel:{x:0,y:0},presentationReady:true};
  const stage=Object.create(AvatarStage.prototype) as any;
  Object.assign(stage,{options,robot,imported,travel,camera,controls,ground:new THREE.Group(),desktopCamera:new THREE.OrthographicCamera(-1,1,1,-1,.05,100),host:{clientWidth:width,clientHeight:height},desktopModelBounds:bounds,desktopMotion:{reset:vi.fn(),sample:vi.fn(()=>point)},asset:root?'fixture.glb':'',walkingClip:root?new THREE.AnimationClip('Walk',1,[]):undefined,contentWidth:bounds.size.x,contentHeight:bounds.size.y,contentDepth:bounds.size.z,cameraMode:'',ambientSession:'',ambientGesture:'',ambientActions:[],walkStrength:0,walkClock:new DistanceWalkClock(),walkingCycleDistance:proceduralCycleDistance(bounds.size.y),modelForwardYaw:0,setWalking:vi.fn()});
  stage.update(options);
  const desktopOptions=(scale=1):AvatarOptions=>({...options,profile:{avatar:{...options.profile.avatar,scale}},desktop:{active:true,referencePhysicalHeight:720}});
  return {stage,robot,model,bounds,options,desktopOptions,point,camera,controls,width,height};
}

describe('Actual desktop camera projection',()=>{
  it('keeps all eight box corners visible across edge waypoints, yaw, scale and DPI',()=>{
    for(const root of [plane(3.1,.1,3.1),plane(.1,2.9,3.1),createRobot().root]) {
      const {size,center}=measureDesktopModel(root);
      for(const [width,height] of [[1920,1080],[1080,1920],[320,240]]) for(const pixelRatio of [1,1.5,2]) for(const requestedScale of [.3,1,2.5]) {
        const frame=desktopFraming({width,height,pixelRatio,referencePhysicalHeight:2000,scale:requestedScale,contentWidth:size.x,contentHeight:size.y,contentDepth:size.z,fov:32,normalDistance:Math.hypot(.35,.48,5.4)});
        const camera=new THREE.OrthographicCamera(-frame.worldWidth/2,frame.worldWidth/2,frame.worldHeight/2,-frame.worldHeight/2,.05,100);
        configureDesktopCamera(camera);
        const right=new THREE.Vector3(1,0,0).applyQuaternion(camera.quaternion),up=new THREE.Vector3(0,1,0).applyQuaternion(camera.quaternion);
        const safe=desktopSafeBounds({width,height},frame.figure,24);
        expect(safe.valid).toBe(true);
        for(const x of [safe.left,safe.right]) for(const y of [safe.top,safe.bottom]) for(const yaw of [0,.63,Math.PI/2,Math.PI]) {
          const turn=new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0,1,0),yaw);
          const translation=right.clone().multiplyScalar((x-width/2)/frame.units).addScaledVector(up,(height/2-y)/frame.units).sub(center.clone().multiplyScalar(frame.scale).applyQuaternion(turn));
          const matrix=new THREE.Matrix4().compose(translation,turn,new THREE.Vector3().setScalar(frame.scale));
          const projectedCenter=css(center.clone().applyMatrix4(matrix),camera,width,height);
          expect(projectedCenter.x).toBeCloseTo(x,8); expect(projectedCenter.y).toBeCloseTo(y,8);
          for(const corner of corners(center,size)) {
            const p=css(corner.applyMatrix4(matrix),camera,width,height);
            expect(p.x).toBeGreaterThanOrEqual(24-1e-8); expect(p.x).toBeLessThanOrEqual(width-24+1e-8);
            expect(p.y).toBeGreaterThanOrEqual(24-1e-8); expect(p.y).toBeLessThanOrEqual(height-24+1e-8);
            expect(Math.abs(p.z)).toBeLessThan(1);
          }
        }
      }
    }
  });

  it('measures the actual Nova antenna and accepts a valid zero-depth mesh',()=>{
    const robot=createRobot(), measured=measureDesktopModel(robot.root);
    const actual=new THREE.Box3().setFromObject(robot.root);
    expect(actual.max.y).toBeGreaterThan(2.99);
    expect(measured.center.y+measured.size.y/2).toBeCloseTo(actual.max.y,8);
    const flat=measureDesktopModel(plane(2,2.9,0));
    expect(flat.size.x).toBeCloseTo(2); expect(flat.size.y).toBeCloseTo(2.9,5);
    expect(flat.size.z).toBeGreaterThanOrEqual(0);
  });

  it('maps the real Stage midpoint to the sampled CSS point including offset centres and yaw',()=>{
    for(const root of [undefined,plane(3.1,.1,3.1)]) {
      const {stage,model,bounds,desktopOptions,point,width,height}=stageFixture(root,1920,1080,1.5);
      stage.update(desktopOptions(1.4));
      const frame=stage.desktopFrame(1.4),safe=desktopSafeBounds({width,height},frame.figure,24);
      for(const yaw of [0,.63,Math.PI/2,Math.PI]) for(const x of [safe.left,safe.right]) for(const y of [safe.top,safe.bottom]) {
        stage.travel.rotation.y=yaw; point.x=x; point.y=y;
        stage.updateAutopilot(0,false);
        const actual=css(model.localToWorld(bounds.center.clone()),stage.activeCamera,width,height);
        expect(stage.activeCamera).toBe(stage.desktopCamera);
        expect(actual.x).toBeCloseTo(x,8); expect(actual.y).toBeCloseTo(y,8);
        for(const corner of corners(bounds.center,bounds.size)) {
          const p=css(model.localToWorld(corner),stage.activeCamera,width,height);
          expect(p.x).toBeGreaterThanOrEqual(24-1e-8); expect(p.x).toBeLessThanOrEqual(width-24+1e-8);
          expect(p.y).toBeGreaterThanOrEqual(24-1e-8); expect(p.y).toBeLessThanOrEqual(height-24+1e-8);
        }
      }
    }
  });

  it('preserves portrait camera and controls across desktop and applies live scale in both modes',()=>{
    const {stage,model,bounds,options,desktopOptions,camera,controls,width,height}=stageFixture();
    const original={position:camera.position.clone(),quaternion:camera.quaternion.clone(),target:controls.target.clone(),projection:camera.projectionMatrix.clone()};
    const reference=corners(bounds.center,bounds.size)[0];
    const before=css(model.localToWorld(reference.clone()),camera,width,height);
    stage.update({...options,profile:{avatar:{...options.profile.avatar,scale:1.2}}});
    expect(model.scale.x).toBeCloseTo(1.2);
    expect(css(model.localToWorld(reference.clone()),camera,width,height).y).not.toBeCloseTo(before.y,3);
    stage.update(desktopOptions(.6)); stage.updateAutopilot(0,false);
    const small=stage.desktopFigure.height;
    stage.update(desktopOptions(1.2)); stage.updateAutopilot(0,false);
    expect(stage.desktopFigure.height).toBeCloseTo(small*2);
    expect(camera.position.equals(original.position)).toBe(true); expect(camera.quaternion.angleTo(original.quaternion)).toBeLessThan(1e-7);
    expect(controls.target.equals(original.target)).toBe(true); expect(camera.projectionMatrix.equals(original.projection)).toBe(true);
    stage.update({...options,autopilot:{...options.autopilot!,active:false}}); stage.updateAutopilot(0,false);
    expect(stage.activeCamera).toBe(camera); expect(model.scale.x).toBe(1);
    expect(stage.travel.position.length()).toBe(0); expect(stage.travel.rotation.y).toBe(0);
    const restored=css(model.localToWorld(reference.clone()),camera,width,height);
    expect(restored.x).toBeCloseTo(before.x,8); expect(restored.y).toBeCloseTo(before.y,8);
  });

  it('keeps full-body framing still while native restoration is pending after Stop',()=>{
    const {stage,model,bounds,desktopOptions,width,height}=stageFixture(undefined,1920,1080,2);
    const running=desktopOptions(2.5); running.desktop!.referencePhysicalHeight=1440;
    stage.update(running); stage.updateAutopilot(.05,false);
    const perspective=stage.camera.position.clone(),target=stage.controls.target.clone();
    const samples=stage.desktopMotion.sample.mock.calls.length;
    const pending={...running,autopilot:{...running.autopilot!,active:false}};
    stage.update(pending); stage.updateAutopilot(.05,false);
    expect(stage.activeCamera).toBe(stage.desktopCamera);
    expect(stage.desktopMotion.sample).toHaveBeenCalledTimes(samples);
    expect(stage.setWalking).toHaveBeenLastCalledWith(false,false);
    const position=stage.travel.position.clone(); stage.updateAutopilot(.05,false);
    expect(stage.travel.position.distanceTo(position)).toBe(0);
    const center=css(model.localToWorld(bounds.center.clone()),stage.activeCamera,width,height);
    expect(center.x).toBeCloseTo(width/2,8); expect(center.y).toBeCloseTo(height/2,8);
    for(const corner of corners(bounds.center,bounds.size)) {
      const p=css(model.localToWorld(corner),stage.activeCamera,width,height);
      expect(p.x).toBeGreaterThanOrEqual(0); expect(p.x).toBeLessThanOrEqual(width);
      expect(p.y).toBeGreaterThanOrEqual(0); expect(p.y).toBeLessThanOrEqual(height);
    }
    stage.update({...pending,desktop:{active:false}}); stage.updateAutopilot(0,false);
    expect(stage.activeCamera).toBe(stage.camera);
    expect(stage.camera.position.equals(perspective)).toBe(true);
    expect(stage.controls.target.equals(target)).toBe(true);
  });

  it('keeps physical projected figure size stable when DPR changes once',()=>{
    const physicalHeights:number[]=[];
    for(const dpr of [1,1.5,2]) {
      const {stage,model,bounds,desktopOptions,width,height}=stageFixture(undefined,1920,1080,dpr);
      stage.update(desktopOptions(.6)); stage.updateAutopilot(0,false);
      const projected=corners(bounds.center,bounds.size).map(p=>css(model.localToWorld(p),stage.activeCamera,width,height).y);
      physicalHeights.push((Math.max(...projected)-Math.min(...projected))*dpr);
    }
    expect(physicalHeights[1]).toBeCloseTo(physicalHeights[0],8); expect(physicalHeights[2]).toBeCloseTo(physicalHeights[0],8);
  });

  it('contains an actual celebrate jump at a top edge without changing the paused waypoint',()=>{
    const {stage,model,desktopOptions,point,width,height}=stageFixture(undefined,1920,1080,2);
    const options=desktopOptions(2.5); options.desktop!.referencePhysicalHeight=1440;
    stage.update(options);
    const safe=desktopSafeBounds({width,height},stage.desktopFrame(2.5).figure,24);
    point.x=safe.left; point.y=safe.top;
    stage.updateAutopilot(0,false);
    stage.humanoidPose=new Map(); stage.time=Math.PI/2;
    stage.emote={action:'celebrate',start:0,duration:3,intensity:2};
    stage.animateEmote(false);
    const top=()=>{
      const box=new THREE.Box3().setFromObject(model);
      return Math.min(...corners(box.getCenter(new THREE.Vector3()),box.getSize(new THREE.Vector3())).map(p=>css(p,stage.activeCamera,width,height).y));
    };
    expect(top()).toBeLessThan(0);
    const waypoint={...point},samples=stage.desktopMotion.sample.mock.calls.length;
    stage.clampDesktopEmote();
    expect(top()).toBeGreaterThanOrEqual(0);
    expect(point).toEqual(waypoint); expect(stage.desktopMotion.sample).toHaveBeenCalledTimes(samples);
    const clamped=stage.travel.position.clone(); stage.clampDesktopEmote();
    expect(stage.travel.position.distanceTo(clamped)).toBeLessThan(1e-9);
  });

  it('preserves intentional hiding and never applies desktop pose clamping to Studio or OBS',()=>{
    const {stage,model,options,desktopOptions}=stageFixture();
    stage.update(desktopOptions()); stage.updateAutopilot(0,false);
    model.position.y=-3.3; stage.emote={action:'hide'};
    const visibleTravel=stage.travel.position.clone(); stage.clampDesktopEmote();
    expect(stage.travel.position.equals(visibleTravel)).toBe(true);
    stage.emote={action:'celebrate'}; model.position.y=10;
    for(const transparent of [false,true]) {
      stage.update({...options,transparent});
      const previous=stage.travel.position.clone(); stage.clampDesktopEmote();
      expect(stage.activeCamera).toBe(stage.camera);
      expect(stage.travel.position.equals(previous)).toBe(true);
    }
  });
});
