import * as THREE from 'three';

export type FrameInput={width:number;height:number;pixelRatio:number;referencePhysicalHeight:number;scale:number;contentWidth:number;contentHeight:number;contentDepth:number;fov:number;normalDistance:number};
/** A fixed elevated desktop view makes both ground axes visible. Characters
 * still turn around their upright Y axis; Studio/OBS keep their own camera. */
export const DESKTOP_ELEVATION=Math.PI/6;

export function configureDesktopCamera(camera:THREE.OrthographicCamera):void {
  camera.up.set(0,1,0);
  camera.position.set(0,20*Math.sin(DESKTOP_ELEVATION),20*Math.cos(DESKTOP_ELEVATION));
  camera.lookAt(0,0,0);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld();
}

/** Screen Y increases downwards. Undo the ground's vertical foreshortening
 * before deriving yaw or stride distance; CSS translation itself stays in the
 * camera's right/up plane so a large monitor never changes clipping depth. */
export function desktopTravelVector(x:number,y:number):THREE.Vector3 {
  if(!Number.isFinite(x)||!Number.isFinite(y))return new THREE.Vector3();
  return new THREE.Vector3(x,0,y/Math.sin(DESKTOP_ELEVATION));
}

/** Match the fixed window's approximate physical pixel size, then cap to fit
 * the full turning figure. The canvas/window itself always stays monitor-sized. */
export function desktopFraming(input:FrameInput){
  const height=Math.max(1,input.height),width=Math.max(1,input.width),ratio=Math.max(.1,input.pixelRatio||1);
  const normalWorldHeight=2*input.normalDistance*Math.tan(input.fov*Math.PI/360);
  const worldHeight=normalWorldHeight*height*ratio/Math.max(1,input.referencePhysicalHeight);
  const turningWidth=Math.hypot(input.contentWidth,input.contentDepth);
  const turningHeight=Math.cos(DESKTOP_ELEVATION)*input.contentHeight+Math.sin(DESKTOP_ELEVATION)*turningWidth;
  const units=height/worldHeight;
  // The elevated orthographic view projects depth into screen height. This
  // conservative yaw hull includes it at every turn and screen position.
  // A subpixel inset keeps the controller's minimum 24px travel lane valid
  // when floating-point projection lands infinitesimally beyond the fit.
  const scale=Math.max(Number.MIN_VALUE,Math.min(input.scale,Math.max(1,height-72-1e-7)/(turningHeight*units),Math.max(1,width-72-1e-7)/(turningWidth*units)));
  return {scale,units,worldHeight,worldWidth:worldHeight*width/height,figure:{width:turningWidth*scale*units,height:turningHeight*scale*units}};
}
/** Cache neutral measured geometry in root-local space. Taking the inverse of
 * the world AABB is conservative even when a previous travel pose is rotated. */
export function measureDesktopModel(root:THREE.Object3D){
  root.updateWorldMatrix(true,true);
  const box=new THREE.Box3().setFromObject(root).applyMatrix4(root.matrixWorld.clone().invert());
  const size=box.getSize(new THREE.Vector3()),center=box.getCenter(new THREE.Vector3());
  if(box.isEmpty()||![...size,...center].every(Number.isFinite))throw new Error('Die Modellgrenzen konnten nicht bestimmt werden.');
  size.max(new THREE.Vector3(1e-6,1e-6,1e-6));
  return {size,center};
}
