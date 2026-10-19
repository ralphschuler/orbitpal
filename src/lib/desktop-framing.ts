export type FrameInput={width:number;height:number;pixelRatio:number;referencePhysicalHeight:number;scale:number;contentWidth:number;contentHeight:number;contentDepth:number;fov:number;normalDistance:number};
/** Match the fixed window's approximate physical pixel size, then cap to fit
 * the full turning figure. The canvas/window itself always stays monitor-sized. */
export function desktopFraming(input:FrameInput){
  const height=Math.max(1,input.height),width=Math.max(1,input.width),ratio=Math.max(.1,input.pixelRatio||1);
  const normalWorldHeight=2*input.normalDistance*Math.tan(input.fov*Math.PI/360);
  const worldHeight=normalWorldHeight*height*ratio/Math.max(1,input.referencePhysicalHeight);
  const turningWidth=Math.hypot(input.contentWidth,input.contentDepth);
  const units=height/worldHeight;
  // The separate frontal orthographic camera makes this yaw hull independent
  // of screen position and depth. Leave a real motion lane on both axes.
  const scale=Math.max(Number.MIN_VALUE,Math.min(input.scale,Math.max(1,height-72)/(input.contentHeight*units),Math.max(1,width-72)/(turningWidth*units)));
  return {scale,units,worldHeight,worldWidth:worldHeight*width/height,figure:{width:turningWidth*scale*units,height:input.contentHeight*scale*units}};
}
import * as THREE from 'three';
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
