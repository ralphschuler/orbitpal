import {describe,it,expect}from'vitest';
import{configureDesktopCamera,DESKTOP_ELEVATION,desktopFraming,desktopTravelVector}from'./desktop-framing';
import * as THREE from 'three';
import {desktopSafeBounds} from './desktop-motion';
describe('Native desktop full-figure framing',()=>{
 const base={width:1920,height:1080,pixelRatio:1,referencePhysicalHeight:720,scale:1,contentWidth:2.05,contentHeight:2.9,contentDepth:1.35,fov:32,distance:Math.hypot(.55,.7,7.6),normalDistance:Math.hypot(.35,.48,5.4)};
 it('preserves approximate physical figure size when changing viewport and DPI',()=>{
  const first=desktopFraming(base),larger=desktopFraming({...base,width:3840,height:2160,pixelRatio:2});
  expect(first.figure.height).toBeCloseTo(larger.figure.height*2);
  expect(desktopFraming({...base,scale:1.2}).figure.height).toBeGreaterThan(first.figure.height);
 });
 it('keeps the full turning figure within both portrait and landscape viewports',()=>{
  for(const[width,height]of[[1920,1080],[1080,1920],[320,240],[3440,1440]])for(const scale of[.3,1,2.5]){
   const frame=desktopFraming({...base,width,height,scale,referencePhysicalHeight:2000});
   expect(frame.figure.width).toBeLessThanOrEqual(width-71.99);expect(frame.figure.height).toBeLessThanOrEqual(height-71.99);
  }
 });
 it('includes projected depth in the full turning height',()=>{
  const frame=desktopFraming(base);
  expect(frame.figure.height).toBeCloseTo((Math.cos(DESKTOP_ELEVATION)*base.contentHeight+Math.sin(DESKTOP_ELEVATION)*Math.hypot(base.contentWidth,base.contentDepth))*frame.scale*frame.units,8);
  expect(desktopFraming({...base,contentDepth:3}).figure.height).toBeGreaterThan(frame.figure.height);
 });
 it('projects ground travel in all eight screen directions while keeping the body upright',()=>{
  const camera=new THREE.OrthographicCamera(-4,4,3,-3,.05,100);
  configureDesktopCamera(camera);
  const projectedOrigin=new THREE.Vector3().project(camera);
  const projected=(point:THREE.Vector3)=>point.clone().project(camera).sub(projectedOrigin);
  const upright=projected(new THREE.Vector3(0,1,0));
  expect(upright.x).toBeCloseTo(0,12);expect(upright.y).toBeGreaterThan(0);
  for(const [x,y] of [[1,0],[1,1],[0,1],[-1,1],[-1,0],[-1,-1],[0,-1],[1,-1]]){
   const ground=desktopTravelVector(x,y);
   expect(ground.y).toBe(0);
   const screen=projected(ground);
   // Restore the orthographic spans; screen Y points downwards.
   expect(screen.x*4).toBeCloseTo(x,10);expect(-screen.y*3).toBeCloseTo(y,10);
   const yaw=Math.atan2(ground.x,ground.z);
   const facing=new THREE.Vector3(0,0,1).applyAxisAngle(new THREE.Vector3(0,1,0),yaw);
   expect(facing.clone().cross(ground).length()).toBeLessThan(1e-10);
   expect(facing.dot(ground)).toBeGreaterThan(0);
  }
  for(const [x,y] of [[NaN,1],[1,Infinity]])expect(desktopTravelVector(x,y).length()).toBe(0);
 });
 it('projects every real box corner inside the viewport at edge waypoints, including flat/deep models',()=>{
  for(const [contentWidth,contentHeight,contentDepth] of [[3.1,.1,3.1],[2.05,2.9,1.35],[.1,3.1,4]])for(const pixelRatio of [1,1.5,2]){
   const frame=desktopFraming({...base,contentWidth,contentHeight,contentDepth,pixelRatio});
   const camera=new THREE.OrthographicCamera(-frame.worldWidth/2,frame.worldWidth/2,frame.worldHeight/2,-frame.worldHeight/2,.05,100);configureDesktopCamera(camera);
   const right=new THREE.Vector3(1,0,0).applyQuaternion(camera.quaternion),up=new THREE.Vector3(0,1,0).applyQuaternion(camera.quaternion);
   const safe=desktopSafeBounds(base,frame.figure,24);
   for(const x of [safe.left,safe.right])for(const y of [safe.top,safe.bottom])for(const yaw of [0,Math.PI/4,Math.PI/2]){
    const translation=right.clone().multiplyScalar((x-base.width/2)/frame.units).addScaledVector(up,(base.height/2-y)/frame.units);
    const matrix=new THREE.Matrix4().compose(translation,new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0,1,0),yaw),new THREE.Vector3().setScalar(frame.scale));
    for(const sx of [-1,1])for(const sy of [-1,1])for(const sz of [-1,1]){
     const corner=new THREE.Vector3(sx*contentWidth/2,sy*contentHeight/2,sz*contentDepth/2).applyMatrix4(matrix).project(camera);
     expect(Math.abs(corner.x)).toBeLessThanOrEqual(1);expect(Math.abs(corner.y)).toBeLessThanOrEqual(1);
    }
   }
  }
 });
});
