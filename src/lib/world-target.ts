import type { CanonicalPoint } from './world-output';

export interface TargetBounds { left:number; top:number; right:number; bottom:number; width:number; height:number; }
export function targetPoint(x:number,y:number,bounds:TargetBounds):CanonicalPoint {
  const values=[x,y,bounds.left,bounds.top,bounds.right,bounds.bottom,bounds.width,bounds.height];
  if(values.some(value=>!Number.isFinite(value))||bounds.width<=0||bounds.height<=0||bounds.left>bounds.right||bounds.top>bounds.bottom||bounds.left<0||bounds.top<0||bounds.right>bounds.width||bounds.bottom>bounds.height)throw new Error('Der Bewegungsbereich ist noch nicht verfügbar.');
  const quantize=(value:number,min:number,max:number,size:number)=>{
    const low=Math.ceil(min/size*1_000_000),high=Math.floor(max/size*1_000_000);
    if(low>high)throw new Error('Der Bewegungsbereich ist noch nicht verfügbar.');
    return Math.max(low,Math.min(high,Math.round(value/size*1_000_000)));
  };
  return {x:quantize(x,bounds.left,bounds.right,bounds.width),y:quantize(y,bounds.top,bounds.bottom,bounds.height)};
}
export function pointerTarget(clientX:number,clientY:number,rect:{left:number;top:number;width:number;height:number},bounds:TargetBounds):CanonicalPoint {
  if(![clientX,clientY,rect.left,rect.top,rect.width,rect.height].every(Number.isFinite)||rect.width<=0||rect.height<=0)throw new Error('Die Bühnenvorschau ist noch nicht bereit.');
  return targetPoint((clientX-rect.left)/rect.width*bounds.width,(clientY-rect.top)/rect.height*bounds.height,bounds);
}
export function stepTarget(point:CanonicalPoint,key:string,coarse:boolean,bounds:TargetBounds):CanonicalPoint|undefined {
  const direction=({ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,-1],ArrowDown:[0,1]} as Record<string,number[]>)[key];
  if(!direction)return;
  const step=coarse ? .05 : .01;
  return targetPoint(point.x/1_000_000*bounds.width+direction[0]*step*bounds.width,point.y/1_000_000*bounds.height+direction[1]*step*bounds.height,bounds);
}
