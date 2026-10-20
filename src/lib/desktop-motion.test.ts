// SPDX-License-Identifier: MIT
import { describe,expect,it } from 'vitest';
import { captionPlacement, DESKTOP_MAX_DELTA, DESKTOP_MAX_SPEED, desktopSafeBounds, DesktopMotionController, type DesktopMotionInput } from './desktop-motion';
const input=(extra:Partial<DesktopMotionInput>={}):DesktopMotionInput=>({seed:'nova-session',startedAt:1000,now:1000,delta:.016,viewport:{width:1440,height:900},figure:{width:140,height:260},margin:24,paused:false,...extra});
const distance=(a:{x:number;y:number},b:{x:number;y:number})=>Math.hypot(a.x-b.x,a.y-b.y);
function run(controller:DesktopMotionController,options:DesktopMotionInput,frames:number){let frame=controller.sample(options);for(let index=0;index<frames;index++){options.now+=16;frame=controller.sample(options);}return frame;}
function until(controller:DesktopMotionController,options:DesktopMotionInput,predicate:(frame:ReturnType<DesktopMotionController['sample']>)=>boolean,limit=30_000){
  let frame=controller.sample(options);
  for(let index=0;index<limit&&!predicate(frame);index++){options.now+=16;frame=controller.sample(options);}
  expect(predicate(frame)).toBe(true);return frame;
}
describe('Local logical-pixel desktop wandering',()=>{
  it('reserves the measured figure footprint and margin in both axes',()=>{
    expect(desktopSafeBounds({width:1920,height:1080},{width:200,height:300},20)).toEqual({left:120,right:1800,top:170,bottom:910,centerX:960,centerY:540,valid:true});
    expect(desktopSafeBounds({width:400,height:300},{width:380,height:280},20).valid).toBe(false);
  });
  it('replays identical sessions deterministically while different seeds choose different routes',()=>{
    const first=new DesktopMotionController(),second=new DesktopMotionController(),other=new DesktopMotionController();const options=input();
    let different=false;
    for(let index=0;index<1200;index++){options.now+=16;const frame=first.sample(options);expect(second.sample(options)).toEqual(frame);if(distance(frame,other.sample({...options,seed:'other-session'}))>1)different=true;}
    expect(different).toBe(true);
  });
  it('enters from both horizontal sides and first rests fully within safe 2D bounds',()=>{
    const sides=new Set<number>();
    for(let seed=0;seed<30;seed++){
      const controller=new DesktopMotionController(),options=input({seed:`entry-${seed}`});let frame=controller.sample(options);sides.add(frame.facing);
      expect(frame.phase).toBe('entry');expect(frame.visible).toBe(false);
      let firstVisible=false;
      for(let index=0;index<200;index++){options.now+=16;frame=controller.sample(options);firstVisible ||= frame.visible;}
      const bounds=desktopSafeBounds(options.viewport,options.figure,options.margin);expect(firstVisible).toBe(true);expect(frame.x).toBeGreaterThanOrEqual(bounds.left);expect(frame.x).toBeLessThanOrEqual(bounds.right);expect(frame.y).toBeGreaterThanOrEqual(bounds.top);expect(frame.y).toBeLessThanOrEqual(bounds.bottom);
    }
    expect([...sides].sort()).toEqual([-1,1]);
  });
  it('visits the full available monitor area rather than a narrow horizontal lane',()=>{
    const controller=new DesktopMotionController(),options=input(),bounds=desktopSafeBounds(options.viewport,options.figure,options.margin);
    let minX=Infinity,maxX=-Infinity,minY=Infinity,maxY=-Infinity,walking=0,idle=0,excursions=0,returns=0;
    for(let index=0;index<60_000;index++){
      options.now+=16;const frame=controller.sample(options);if(frame.phase==='entry')continue;
      expect(frame.y>=bounds.top-1e-7&&frame.y<=bounds.bottom+1e-7).toBe(true);
      if(frame.phase==='exit'||frame.phase==='offscreen'){excursions++;continue;}
      if(frame.phase==='return'){returns++;continue;}
      minX=Math.min(minX,frame.x);maxX=Math.max(maxX,frame.x);minY=Math.min(minY,frame.y);maxY=Math.max(maxY,frame.y);
      expect(frame.x>=bounds.left-1e-7&&frame.x<=bounds.right+1e-7&&frame.y>=bounds.top-1e-7&&frame.y<=bounds.bottom+1e-7).toBe(true);
      if(frame.walking)walking++;else idle++;
    }
    expect(minX).toBeLessThan(bounds.left+(bounds.right-bounds.left)*.08);expect(maxX).toBeGreaterThan(bounds.left+(bounds.right-bounds.left)*.92);
    expect(minY).toBeLessThan(bounds.top+(bounds.bottom-bounds.top)*.08);expect(maxY).toBeGreaterThan(bounds.top+(bounds.bottom-bounds.top)*.92);expect(walking).toBeGreaterThan(0);expect(idle).toBeGreaterThan(0);expect(excursions).toBeGreaterThan(0);expect(returns).toBeGreaterThan(0);
  });
  it('caps frame delta, speed and long-gap movement, including entry',()=>{
    const controller=new DesktopMotionController(),options=input();let previous=controller.sample(options);
    for(let index=0;index<600;index++){
      options.now+=index%40===0?60_000:16;options.delta=index%40===0?60:.016;
      const frame=controller.sample(options);expect(distance(frame,previous)).toBeLessThanOrEqual(DESKTOP_MAX_SPEED*Math.min(options.delta,DESKTOP_MAX_DELTA)+1e-7);previous=frame;
    }
    for(const delta of [0,-1,NaN,Infinity]){options.now+=1000;const frame=controller.sample({...options,delta});expect(distance(frame,previous)).toBe(0);previous=frame;}
  });
  it('reports the actual two-axis step rather than only horizontal facing, with no motion during pauses or stalls',()=>{
    const controller=new DesktopMotionController(),options=input({viewport:{width:1200,height:1200},figure:{width:180,height:180}});let previous=controller.sample(options);
    expect(previous.travel).toEqual({x:0,y:0});
    const directions=new Set<number>();
    for(let index=0;index<30_000;index++){
      options.now+=index%117===0?60_000:16;
      const paused=index%91===0;
      const frame=controller.sample({...options,paused,delta:index%117===0?60:.016});
      expect(frame.travel.x).toBeCloseTo(frame.x-previous.x,8);
      expect(frame.travel.y).toBeCloseTo(frame.y-previous.y,8);
      expect(Math.hypot(frame.travel.x,frame.travel.y)).toBeLessThanOrEqual(DESKTOP_MAX_SPEED*DESKTOP_MAX_DELTA+1e-8);
      if(paused)expect(frame.travel).toEqual({x:0,y:0});
      if(frame.walking)directions.add((Math.round(Math.atan2(frame.travel.y,frame.travel.x)/(Math.PI/4))+8)%8);
      previous=frame;
    }
    expect([...directions].sort()).toEqual([0,1,2,3,4,5,6,7]);
    expect(controller.sample({...options,viewport:{width:0,height:0}}).travel).toEqual({x:0,y:0});
  });
  it('freezes during speech/emotes and resumes the same waypoint smoothly without wall-clock catch-up',()=>{
    const controller=new DesktopMotionController(),options=input();let frame=run(controller,options,1200);const before={x:frame.x,y:frame.y};
    for(let index=0;index<300;index++){options.now+=1000;frame=controller.sample({...options,paused:true,delta:1});expect(frame.phase).toBe('paused');expect(frame.walking).toBe(false);expect({x:frame.x,y:frame.y}).toEqual(before);}
    options.now+=16;frame=controller.sample(options);expect(distance(frame,before)).toBeLessThan(.1);
    const control=new DesktopMotionController(),controlOptions=input();run(control,controlOptions,1200);const controlBefore=control.sample({...controlOptions,paused:true});controlOptions.now+=16;
    const resumed=control.sample(controlOptions);expect(distance(resumed,controlBefore)).toBeCloseTo(distance(frame,before),6);
  });
  it('walks an off-screen entry back before presentation, even while paused, without teleporting',()=>{
    for(const seed of ['entry-0','entry-1','entry-2','entry-3']){
      const controller=new DesktopMotionController(),options=input({seed});const outside=controller.sample(options);expect(outside.visible).toBe(false);
      options.now+=16;const paused=controller.sample({...options,paused:true});
      expect(paused).toMatchObject({phase:'paused',visible:false,walking:false,presentationReady:false});expect(distance(paused,outside)).toBe(0);
      options.paused=true;options.presentationRequested=true;options.now+=16;
      const returning=controller.sample(options);expect(returning.phase).toBe('return');expect(returning.presentationReady).toBe(false);expect(distance(returning,paused)).toBeLessThan(1);
      const ready=until(controller,options,frame=>frame.presentationReady,1000);const bounds=desktopSafeBounds(options.viewport,options.figure,options.margin);
      expect(ready).toMatchObject({phase:'paused',visible:true,walking:false,presentationReady:true});expect(ready.x).toBeGreaterThanOrEqual(bounds.left);expect(ready.x).toBeLessThanOrEqual(bounds.right);
      options.now+=60_000;expect(controller.sample(options)).toEqual(ready);
      options.paused=false;options.presentationRequested=false;options.now+=16;expect(distance(controller.sample(options),ready)).toBe(0);
    }
  });
  it('returns smoothly into new bounds on viewport/figure changes during a presentation hold',()=>{
    const controller=new DesktopMotionController(),options=input();run(controller,options,1500);
    options.presentationRequested=true;const previous=until(controller,options,frame=>frame.presentationReady);
    options.now+=16;options.viewport={width:500,height:450};options.figure={width:220,height:300};const bounds=desktopSafeBounds(options.viewport,options.figure,options.margin);
    options.paused=true;const first=controller.sample(options);expect(distance(first,previous)).toBeLessThanOrEqual(DESKTOP_MAX_SPEED*.016);
    const frame=until(controller,options,frame=>frame.presentationReady,2000);expect(frame.x).toBeGreaterThanOrEqual(bounds.left);expect(frame.x).toBeLessThanOrEqual(bounds.right);expect(frame.y).toBeGreaterThanOrEqual(bounds.top);expect(frame.y).toBeLessThanOrEqual(bounds.bottom);
    const centered=controller.sample({...options,viewport:{width:240,height:200}});expect(centered).toMatchObject({x:120,y:100,walking:false,phase:'centered'});
    expect(centered.presentationReady).toBe(false);
  });
  it('requires a visible return for a newly opened paused character and keeps invalid inputs quiet',()=>{
    const controller=new DesktopMotionController(),options=input({paused:true});expect(controller.sample(options)).toMatchObject({phase:'paused',visible:false,walking:false,presentationReady:false});
    for(const viewport of [{width:NaN,height:500},{width:0,height:-1},{width:Infinity,height:Infinity},{width:1e308,height:1e308}]){
      const frame=controller.sample(input({viewport}));expect(Number.isFinite(frame.x)&&Number.isFinite(frame.y)).toBe(true);expect(frame.phase).toBe('centered');expect(frame.walking).toBe(false);expect(frame.presentationReady).toBe(false);
    }
    for(const extra of [{seed:''},{now:NaN},{now:100,startedAt:1000},{figure:{width:NaN,height:200}}])expect(controller.sample(input(extra))).toMatchObject({phase:'centered',walking:false});
  });
  it('has no screen-origin or DPI inputs, and scales the footprint only once in logical pixels',()=>{
    const options=input({viewport:{width:1920,height:1080},figure:{width:200,height:300}});
    const first=new DesktopMotionController(),second=new DesktopMotionController();
    expect(first.sample(options)).toEqual(second.sample({...options,monitorOrigin:{x:-3840,y:-1000},scaleFactor:2} as DesktopMotionInput));
    expect(desktopSafeBounds(options.viewport,options.figure).left).toBe(116);
  });
  it('resets explicitly and per new bot session rather than inheriting a previous route',()=>{
    const controller=new DesktopMotionController(),options=input();run(controller,options,500);controller.reset();
    expect(controller.sample(input())).toEqual(new DesktopMotionController().sample(input()));
    expect(controller.sample(input({seed:'new-bot',startedAt:5000,now:5000}))).toEqual(new DesktopMotionController().sample(input({seed:'new-bot',startedAt:5000,now:5000})));
  });
  it('leaves the viewport, waits outside, and returns automatically after a bounded hold',()=>{
    const controller=new DesktopMotionController(),options=input();const outside=until(controller,options,frame=>frame.phase==='offscreen');
    expect(outside.visible).toBe(false);expect(outside.presentationReady).toBe(false);
    const started=options.now;
    const returning=until(controller,options,frame=>frame.phase==='return',600);
    expect(options.now-started).toBeGreaterThanOrEqual(2900);expect(options.now-started).toBeLessThanOrEqual(8100);
    expect(distance(outside,returning)).toBeLessThan(1);
    const ready=until(controller,options,frame=>frame.presentationReady,1000);expect(ready.visible).toBe(true);
  });
  it('can leave through either side while bounding every entry, roaming, and exit step',()=>{
    const sides=new Set<number>();
    for(let seed=0;seed<8;seed++){
      const controller=new DesktopMotionController(),options=input({seed:`excursion-${seed}`});let previous=controller.sample(options),largestStep=0,found=false;
      const bounds=desktopSafeBounds(options.viewport,options.figure,options.margin);let minimumY=Infinity,maximumY=-Infinity;
      for(let index=0;index<30_000;index++){
        options.now+=16;const frame=controller.sample(options);largestStep=Math.max(largestStep,distance(previous,frame));minimumY=Math.min(minimumY,frame.y);maximumY=Math.max(maximumY,frame.y);previous=frame;
        if(frame.phase==='offscreen'){sides.add(frame.x<0?-1:1);found=true;break;}
      }
      expect(found).toBe(true);expect(largestStep).toBeLessThanOrEqual(DESKTOP_MAX_SPEED*.016+1e-7);expect(minimumY).toBeGreaterThanOrEqual(bounds.top);expect(maximumY).toBeLessThanOrEqual(bounds.bottom);
    }
    expect([...sides].sort()).toEqual([-1,1]);
  });
  it('preempts an off-screen hold for a presentation and remains visible until release',()=>{
    const controller=new DesktopMotionController(),options=input();let previous=until(controller,options,frame=>frame.phase==='offscreen');
    options.presentationRequested=true;options.paused=true;let ready=false;
    for(let index=0;index<1000;index++){
      options.now+=16;const frame=controller.sample(options);
      expect(distance(frame,previous)).toBeLessThanOrEqual(DESKTOP_MAX_SPEED*.016+1e-7);
      if(frame.presentationReady){ready=true;expect(frame.visible).toBe(true);expect(frame.walking).toBe(false);}
      else expect(ready).toBe(false);
      previous=frame;
    }
    expect(ready).toBe(true);
    options.presentationRequested=false;options.paused=false;
    expect(until(controller,options,frame=>frame.walking,1000).walking).toBe(true);
  });
  it('preempts a partially clipped exit and cannot report readiness for a single visible pixel',()=>{
    const controller=new DesktopMotionController(),options=input();
    const partial=until(controller,options,frame=>frame.phase==='exit'&&frame.visible&&(frame.x<options.figure.width/2||frame.x>options.viewport.width-options.figure.width/2));
    expect(partial.presentationReady).toBe(false);options.presentationRequested=true;options.now+=16;
    const returning=controller.sample(options);expect(returning.phase).toBe('return');expect(returning.presentationReady).toBe(false);expect(distance(partial,returning)).toBeLessThanOrEqual(DESKTOP_MAX_SPEED*.016+1e-7);
    expect(until(controller,options,frame=>frame.presentationReady,1000).walking).toBe(false);
  });
  it('stops a fully visible walk for a presentation without moving to the centre',()=>{
    const controller=new DesktopMotionController(),options=input();
    const walking=until(controller,options,frame=>frame.phase==='walk'&&frame.walking);options.presentationRequested=true;options.now+=16;
    const ready=controller.sample(options);expect(ready).toMatchObject({x:walking.x,y:walking.y,walking:false,presentationReady:true});
  });
});
describe('Caption placement near the actual character',()=>{
  it('prefers above, falls below near the upper edge, and clamps horizontally',()=>{
    const viewport={width:1000,height:700},caption={width:300,height:80};
    expect(captionPlacement({x:500,y:400,width:160,height:240},caption,viewport)).toMatchObject({left:350,top:192});
    expect(captionPlacement({x:40,y:110,width:160,height:160},caption,viewport)).toMatchObject({left:16,top:198});
    expect(captionPlacement({x:990,y:650,width:160,height:80},caption,viewport)).toMatchObject({left:684,top:522});
  });
  it('reports visible size limits for oversized captions and survives empty/invalid geometry',()=>{
    const result=captionPlacement({x:900,y:600,width:100,height:300},{width:2000,height:2000},{width:1000,height:700},20);
    expect(result).toEqual({left:20,top:20,maxWidth:960,maxHeight:660});
    for(const viewport of [{width:0,height:0},{width:NaN,height:Infinity},{width:1,height:1}]){
      const result=captionPlacement({x:NaN,y:Infinity,width:NaN,height:NaN},{width:100,height:100},viewport);
      for(const value of Object.values(result))expect(Number.isFinite(value)&&value>=0).toBe(true);
    }
  });
});
