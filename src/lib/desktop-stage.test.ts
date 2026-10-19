import {describe,it,expect,vi}from'vitest';
import * as THREE from'three';
import{AvatarStage}from'./avatar';
import{DesktopMotionController}from'./desktop-motion';
describe('Desktop Stage walk capability',()=>{
 it('keeps an imported GLB without a walk clip or coherent rig centered',()=>{
  const stage=Object.create(AvatarStage.prototype)as any;
  const target=new THREE.Vector3(0,1.35,0),camera=new THREE.PerspectiveCamera(32,16/9,.05,100);camera.position.set(.55,2.05,7.6);camera.lookAt(target);
  const root=new THREE.Group();root.visible=false;const motion=new DesktopMotionController(),sample=vi.spyOn(motion,'sample');
  Object.assign(stage,{options:{profile:{avatar:{}},transparent:true,speaking:false,desktop:{active:true,referencePhysicalHeight:720},autopilot:{active:true,wander:true,gestures:false,startedAt:0,seed:'fixture'}},asset:'no-walk.glb',camera,desktopCamera:new THREE.OrthographicCamera(-4,4,2.25,-2.25,.05,100),controls:{target},host:{clientWidth:1920,clientHeight:1080},robot:{root},travel:new THREE.Group(),desktopMotion:motion,contentWidth:2,contentDepth:1,contentHeight:2.9,desktopModelBounds:{center:new THREE.Vector3(0,1.45,0),size:new THREE.Vector3(2,2.9,1)},ambientActions:[],ambientSession:'',ambientGesture:'',walkStrength:0,modelForwardYaw:0,setWalking:vi.fn()});
  expect(stage.canWalk()).toBe(false);
  stage.updateAutopilot(.05,false);const position=stage.travel.position.clone();stage.updateAutopilot(.05,false);
  expect(sample).not.toHaveBeenCalled();expect(stage.travel.position.distanceTo(position)).toBe(0);expect(stage.setWalking).toHaveBeenLastCalledWith(false,false);
  const center=position.clone().add(new THREE.Vector3(0,2.9/2,0));expect(center.x).toBeCloseTo(0);expect(center.y).toBeCloseTo(0);
 });
});
