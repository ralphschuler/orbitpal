// SPDX-License-Identifier: MIT
/** Native desktop output only. Every coordinate and dimension is a LOCAL CSS
 * pixel: the host alone converts monitor origins and physical/DPI coordinates.
 * This module has no timers, randomness, DOM access or Studio/OBS side effects. */
export type Size = Readonly<{width:number;height:number}>;
export type Point = Readonly<{x:number;y:number}>;
export type DesktopMotionPhase = 'entry'|'walk'|'idle'|'exit'|'offscreen'|'return'|'paused'|'centered';
export type DesktopMotionFrame = Point & {facing:-1|1;travel:Point;walking:boolean;phase:DesktopMotionPhase;visible:boolean;presentationReady:boolean};
export type DesktopMotionInput = {
  seed:string; startedAt:number; now:number; delta:number;
  viewport:Size; figure:Size; margin?:number; paused?:boolean; presentationRequested?:boolean;
};
export type DesktopBounds = {left:number;right:number;top:number;bottom:number;centerX:number;centerY:number;valid:boolean};
export const DESKTOP_MAX_DELTA = .05;
export const DESKTOP_MAX_SPEED = 180;
const MAX_DIMENSION = 1_000_000;
const ACCELERATION = 280;
const clamp=(value:number,minimum:number,maximum:number)=>Math.min(maximum,Math.max(minimum,value));
const dimension=(value:number)=>Number.isFinite(value)&&value>0?Math.min(MAX_DIMENSION,value):0;
const validSize=(size:Size)=>Number.isFinite(size.width)&&Number.isFinite(size.height)&&size.width>0&&size.height>0&&size.width<=MAX_DIMENSION&&size.height<=MAX_DIMENSION;
const padding=(value:number|undefined)=>typeof value==='number'&&Number.isFinite(value)&&value>=0?Math.min(256,value):16;

/** Bounds constrain the FIGURE CENTRE, including its measured projected size. */
export function desktopSafeBounds(viewport:Size,figure:Size,margin=16):DesktopBounds {
  const width=dimension(viewport.width),height=dimension(viewport.height),edge=padding(margin);
  const halfWidth=dimension(figure.width)/2,halfHeight=dimension(figure.height)/2;
  const left=edge+halfWidth,right=width-edge-halfWidth,top=edge+halfHeight,bottom=height-edge-halfHeight;
  return {left,right,top,bottom,centerX:width/2,centerY:height/2,valid:validSize(viewport)&&validSize(figure)&&right-left>=24&&bottom-top>=24};
}
function random(seed:string,index:number,label:string):number {
  let hash=2166136261;
  for(const character of `${seed}:${index}:${label}`){hash^=character.codePointAt(0)!;hash=Math.imul(hash,16777619);}
  return (hash>>>0)/4294967296;
}
/** A shuffled 3×3 cycle guarantees visits across both axes rather than allowing
 * a run of random choices to remain clustered in one corner. */
function waypoint(seed:string,index:number):Point {
  const round=Math.floor(index/9),order=[0,1,2,3,4,5,6,7,8];
  for(let i=8;i>0;i--){const j=Math.floor(random(seed,round,`shuffle-${i}`)*(i+1));[order[i],order[j]]=[order[j],order[i]];}
  const cell=order[index%9],jitter=(axis:string)=>.04*(random(seed,index,axis)-.5);
  return {x:clamp((cell%3)/2+jitter('x'),0,1),y:clamp(Math.floor(cell/3)/2+jitter('y'),0,1)};
}
function point(bounds:DesktopBounds,normal:Point):Point {return {x:bounds.left+(bounds.right-bounds.left)*normal.x,y:bounds.top+(bounds.bottom-bounds.top)*normal.y};}

/** Stateful only in its explicit controller instance. Replaying the same input
 * sequence and session seed yields the same path. Pauses do not advance its
 * local waypoint/hold clock; no wall-clock catch-up is performed on resume.
 * A presentation request overrides pauses until the entire figure is safely
 * visible, then holds it there. Consumers must await presentationReady before
 * starting speech/emotes and keep the request active throughout presentation. */
export class DesktopMotionController {
  private session=''; private initialized=false; private lastNow=0;
  private x=0; private y=0; private facing:-1|1=1; private speed=0;
  private travel:Point={x:0,y:0};
  private mode:'entry'|'walk'|'idle'|'exit'|'offscreen'|'return'='idle'; private target:Point={x:.5,y:.5};
  private nextWaypoint=0; private hold=0; private geometry:number[]=[];
  private visits=0; private nextExcursion=4; private excursions=0; private exitSide:-1|1=-1;
  reset():void {this.session='';this.initialized=false;this.speed=0;this.geometry=[];}

  sample(input:DesktopMotionInput):DesktopMotionFrame {
    // Only this frame's actual displacement drives heading and gait. Never
    // count initial placement, reframing or a paused wall-clock interval.
    this.travel={x:0,y:0};
    const bounds=desktopSafeBounds(input.viewport,input.figure,input.margin);
    const sessionValid=typeof input.seed==='string'&&input.seed.length>0&&input.seed.length<=256&&Number.isFinite(input.startedAt)&&input.startedAt>=0&&Number.isFinite(input.now)&&input.now>=input.startedAt;
    const session=sessionValid?`${input.startedAt}:${input.seed}`:'';
    if(!sessionValid||!bounds.valid){
      if(session!==this.session){this.session=session;this.initialized=false;this.nextWaypoint=0;}
      this.x=bounds.centerX;this.y=bounds.centerY;this.speed=0;this.lastNow=Number.isFinite(input.now)?input.now:0;
      if(this.initialized){this.mode='idle';this.hold=.5;}
      return this.frame('centered',false,input.viewport,input.figure,bounds);
    }
    if(this.session!==session){this.session=session;this.initialized=false;this.nextWaypoint=0;this.speed=0;this.geometry=[];}
    const geometry=[input.viewport.width,input.viewport.height,input.figure.width,input.figure.height,padding(input.margin)];
    const changed=this.geometry.length>0&&geometry.some((value,index)=>Math.abs(value-this.geometry[index])>1);
    this.geometry=geometry;
    if(!this.initialized){
      this.initialized=true;this.lastNow=input.now;this.speed=0;this.visits=0;this.excursions=0;
      this.nextExcursion=4+Math.floor(3*random(session,0,'excursion-spacing'));
      this.facing=random(session,0,'entry-side')<.5?1:-1;
      this.target={x:this.facing===1?0:1,y:.15+.7*random(session,0,'entry-y')};
      const landing=point(bounds,this.target);
      this.x=this.facing===1?-input.figure.width/2-padding(input.margin):input.viewport.width+input.figure.width/2+padding(input.margin);
      this.y=landing.y;this.mode='entry';
    }
    const elapsed=Math.max(0,(input.now-this.lastNow)/1000);this.lastNow=input.now;
    const delta=Number.isFinite(input.delta)&&input.delta>0?Math.min(input.delta,elapsed,DESKTOP_MAX_DELTA):0;
    // Reframing and a presentation request take the shortest path to a fully
    // visible resting place. In particular, pausing must never freeze a return
    // or teleport an entering/off-screen character into a conversation.
    if(changed||input.presentationRequested){
      if(this.inside(bounds)){
        this.mode='idle';this.hold=.5;this.speed=0;
        if(input.presentationRequested)return this.frame('paused',false,input.viewport,input.figure,bounds);
      }else this.mode='return';
    }
    if(input.paused&&!(input.presentationRequested&&this.mode==='return')){
      this.speed=0;return this.frame('paused',false,input.viewport,input.figure,bounds);
    }
    if(this.mode==='offscreen'){
      this.hold=Math.max(0,this.hold-delta);this.speed=0;
      if(this.hold>0)return this.frame('offscreen',false,input.viewport,input.figure,bounds);
      this.mode='return';
    }
    if(this.mode==='idle'){
      this.hold=Math.max(0,this.hold-delta);this.speed=0;
      if(this.hold>0)return this.frame('idle',false,input.viewport,input.figure,bounds);
      if(this.visits>=this.nextExcursion){
        this.exitSide=random(session,this.excursions,'exit-side')<.5?-1:1;
        this.nextExcursion+=4+Math.floor(3*random(session,++this.excursions,'excursion-spacing'));
        this.mode='exit';
      }else{
        for(let attempt=0;attempt<9;attempt++){
          this.target=waypoint(session,this.nextWaypoint++);const destination=point(bounds,this.target);
          if(Math.hypot(destination.x-this.x,destination.y-this.y)>6)break;
        }
        this.mode='walk';
      }
    }
    const destination=this.mode==='return'
      ?{x:clamp(this.x,bounds.left,bounds.right),y:clamp(this.y,bounds.top,bounds.bottom)}
      :this.mode==='exit'
        ?{x:this.exitSide===-1?-input.figure.width/2-padding(input.margin):input.viewport.width+input.figure.width/2+padding(input.margin),y:this.y}
        :point(bounds,this.target);
    const dx=destination.x-this.x,dy=destination.y-this.y,distance=Math.hypot(dx,dy);
    const phase=this.mode;
    const cruise=this.mode==='walk'?clamp(Math.min(input.figure.width,input.figure.height)*.45,40,110):DESKTOP_MAX_SPEED;
    const desired=Math.min(cruise,Math.sqrt(2*ACCELERATION*distance));
    this.speed+=clamp(desired-this.speed,-ACCELERATION*delta,ACCELERATION*delta);
    const travel=Math.min(distance,Math.max(0,this.speed)*delta);
    if(travel>0&&distance>0){this.travel={x:dx/distance*travel,y:dy/distance*travel};this.x+=this.travel.x;this.y+=this.travel.y;if(Math.abs(dx/distance)>.05)this.facing=dx>0?1:-1;}
    if(delta>0&&distance-travel<=1e-9){
      this.x=destination.x;this.y=destination.y;this.speed=0;
      if(this.mode==='exit'){
        this.mode='offscreen';this.hold=3+5*random(session,this.excursions,'offscreen-hold');
      }else{
        if(this.mode==='walk')this.visits++;
        this.mode='idle';this.hold=.8+1.8*random(session,this.nextWaypoint,'hold');
      }
    }
    return this.frame(phase,travel>.001,input.viewport,input.figure,bounds);
  }
  private inside(bounds:DesktopBounds):boolean {return bounds.valid&&this.x>=bounds.left&&this.x<=bounds.right&&this.y>=bounds.top&&this.y<=bounds.bottom;}
  private frame(phase:DesktopMotionPhase,walking:boolean,viewport:Size,figure:Size,bounds:DesktopBounds):DesktopMotionFrame {
    const width=dimension(viewport.width),height=dimension(viewport.height),halfWidth=dimension(figure.width)/2,halfHeight=dimension(figure.height)/2;
    return {x:this.x,y:this.y,facing:this.facing,travel:{...this.travel},walking,phase,visible:validSize(viewport)&&validSize(figure)&&this.x+halfWidth>0&&this.x-halfWidth<width&&this.y+halfHeight>0&&this.y-halfHeight<height,presentationReady:!walking&&this.inside(bounds)};
  }
}

/** Anchor x/y is the figure centre, like DesktopMotionFrame. Constrain the DOM
 * caption to maxWidth/maxHeight; an oversized element cannot otherwise fit.
 * Prefer above the figure, then below, then clamp to the visible viewport. */
export function captionPlacement(anchor:Point&Size,caption:Size,viewport:Size,margin=16):{left:number;top:number;maxWidth:number;maxHeight:number}{
  const width=dimension(viewport.width),height=dimension(viewport.height);
  const edgeX=Math.min(padding(margin),width/2),edgeY=Math.min(padding(margin),height/2);
  const maxWidth=Math.max(0,width-2*edgeX),maxHeight=Math.max(0,height-2*edgeY);
  const captionWidth=Math.min(dimension(caption.width),maxWidth),captionHeight=Math.min(dimension(caption.height),maxHeight);
  const x=Number.isFinite(anchor.x)?anchor.x:width/2,y=Number.isFinite(anchor.y)?anchor.y:height/2;
  const above=y-dimension(anchor.height)/2-8-captionHeight,below=y+dimension(anchor.height)/2+8;
  const candidate=above>=edgeY?above:below;
  return {left:clamp(x-captionWidth/2,edgeX,width-edgeX-captionWidth),top:clamp(candidate,edgeY,height-edgeY-captionHeight),maxWidth,maxHeight};
}
