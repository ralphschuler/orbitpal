// SPDX-License-Identifier: MIT
/** Public pose data only. Native response scope selects an epoch; UUID arrival
 * order, events and these reads never grant a layout, Ready or action ACK. */
import {parseWorldState,type WorldState,type WorldAnchor} from './world-state';
import type {BindingStamp,CanonicalPoint} from './world-output';

export type NativeSnapshotScope=Readonly<Pick<BindingStamp,'actorId'|'worldInstanceId'|'stageId'|'configRevision'>>;
export interface NativeSnapshotOptions {
  read(actorId:string):Promise<unknown>;
  current(scope:NativeSnapshotScope,signal:AbortSignal):boolean;
  /** Primitive exact Overlay profile/route revision, captured before each read. */
  revision():unknown;
  mismatch(scope:NativeSnapshotScope,signal:AbortSignal):void;
  /** Public object config changed or retired; never schedules an IPC read. */
  changed?():void;
  now?:()=>number;
}
const READ_MS=2000,SPACING_MS=250,MAX_READS=8;
/** Static config only: no cached actor pose/life and no native grant. It is
 * first captured from a real scoped read, then bound to that mount/binding and
 * immutable World instance/stage/config revision until context invalidation. */
export type NativeObjectConfig=Readonly<{instanceId:string;binding:Readonly<BindingStamp>;stageId:string;configRevision:number;
  actors:readonly Readonly<{actorId:string}>[];anchors:readonly WorldAnchor[]}>;
const bindingKeys=['actorId','sourceWorldId','renderSessionId','bindingSequence','worldInstanceId','stageId','configRevision',
  'topologyRevision','layoutRevision','assetRevision','measurementRevision','referenceRevision'] as const;
export function sameNativeSnapshotBinding(a:BindingStamp,b:BindingStamp):boolean{
  return bindingKeys.every(key=>a[key]===b[key]);
}
function copyBinding(value:BindingStamp):Readonly<BindingStamp>{
  return Object.freeze(Object.fromEntries(bindingKeys.map(key=>[key,value[key]])) as BindingStamp);
}
type Budget={reads:number;last:number};
type Context={scope:NativeSnapshotScope;signal:AbortSignal;revision:unknown;budget:Budget;
  confirmed:boolean;dirty:boolean;snapshot:WorldState|null;binding:Readonly<BindingStamp>|null;objects:NativeObjectConfig|null;abort:()=>void};
type Flight={context:Context;expired:boolean;timeout?:ReturnType<typeof setTimeout>};
function same(a:NativeSnapshotScope,b:NativeSnapshotScope):boolean{
  return a.actorId===b.actorId&&a.worldInstanceId===b.worldInstanceId&&a.stageId===b.stageId&&a.configRevision===b.configRevision;
}
function matches(world:WorldState,scope:NativeSnapshotScope):boolean{
  return world.availability==='ready'&&world.instanceId===scope.worldInstanceId
    &&world.snapshot?.stageId===scope.stageId&&world.snapshot.configRevision===scope.configRevision
    &&world.snapshot.actors.some(actor=>actor.actorId===scope.actorId);
}
export class NativeWorldSnapshots {
  private context:Context|null=null;
  private flight:Flight|null=null;
  private scheduled?:ReturnType<typeof setTimeout>;
  private disposed=false;
  // Weak keys do not retain retired native generations. A repeated scope under
  // the SAME generation cannot reset its fixed eight-read budget.
  private budgets=new WeakMap<AbortSignal,Budget>();
  private now:()=>number;
  constructor(private options:NativeSnapshotOptions){this.now=options.now??(()=>performance.now());}
  private valid(context:Context):boolean{
    return !this.disposed&&this.context===context&&!context.signal.aborted
      &&Object.is(context.revision,this.options.revision())&&this.options.current(context.scope,context.signal);
  }
  /** Called ONLY with the current parsed native register/ACK expected scope. */
  expect(expected:NativeSnapshotScope,signal:AbortSignal,binding?:BindingStamp):void{
    if(this.disposed||signal.aborted)return;
    if(binding&&!same(expected,binding))return;
    const objectBinding=binding?copyBinding(binding):null,old=this.context;
    if(old&&old.signal===signal&&same(old.scope,expected)&&this.valid(old)
      &&(old.binding===null&&objectBinding===null||old.binding!==null&&objectBinding!==null&&sameNativeSnapshotBinding(old.binding,objectBinding)))return;
    this.invalidate();
    let budget=this.budgets.get(signal);
    if(!budget){budget={reads:0,last:-Infinity};this.budgets.set(signal,budget);}
    const scope=Object.freeze({actorId:expected.actorId,worldInstanceId:expected.worldInstanceId,stageId:expected.stageId,configRevision:expected.configRevision});
    const context:Context={scope,signal,revision:this.options.revision(),budget,confirmed:false,dirty:true,snapshot:null,binding:objectBinding,objects:null,
      abort:()=>{if(this.context===context)this.invalidate();}};
    this.context=context;signal.addEventListener('abort',context.abort,{once:true});
    if(this.valid(context))this.schedule();else this.invalidate();
  }
  /** Initial App.world is deliberately NOT sent here. Unknown events are hints;
   * they cannot seed an epoch, remove a valid B pose or retire B's capability. */
  hint(value:unknown):void{
    const context=this.context;if(!context||!this.valid(context))return;
    try{
      const next=parseWorldState(value);
      if(context.confirmed&&matches(next,context.scope)){this.admit(context,next);return;}
    }catch{/* No raw event/error content is retained or shown. */}
    context.dirty=true;this.schedule();
  }
  position(expected:NativeSnapshotScope):CanonicalPoint|null{
    const context=this.context;
    if(!context||!this.valid(context)||!context.confirmed||!same(context.scope,expected))return null;
    const point=context.snapshot?.snapshot?.actors.find(actor=>actor.actorId===expected.actorId)?.position;
    return point?Object.freeze({x:point.x,y:point.y}):null;
  }
  /** Defensive read-only display copy. Initial App.world, matching events and
   * the native mirror DTO cannot select or refresh this static object config. */
  objectConfig(expected:BindingStamp):NativeObjectConfig|null{
    const context=this.context;
    if(!context||!this.valid(context)||!context.confirmed||!context.objects||!context.binding
      ||!sameNativeSnapshotBinding(context.binding,expected))return null;
    return Object.freeze({...context.objects,actors:context.objects.actors.map(actor=>Object.freeze({...actor})),
      anchors:structuredClone(context.objects.anchors)});
  }
  private admit(context:Context,next:WorldState,authoritative=false):void{
    if(!this.valid(context)||!matches(next,context.scope))return;
    if((next.snapshot?.sequence??0)<(context.snapshot?.snapshot?.sequence??0))return;
    context.snapshot=next;
    // Anchors and configured membership are immutable within this config
    // revision. Keep their first real read, not later event/read arrival order.
    if(authoritative&&!context.objects&&context.binding&&next.snapshot){
      context.objects={instanceId:next.instanceId,binding:context.binding,stageId:next.snapshot.stageId,
        configRevision:next.snapshot.configRevision,actors:next.snapshot.actors.map(actor=>({actorId:actor.actorId})),
        anchors:structuredClone(next.snapshot.anchors)};
    }
    this.options.changed?.();
  }
  private schedule():void{
    const context=this.context;
    if(!context||!this.valid(context)||!context.dirty||this.flight||this.scheduled!==undefined||context.budget.reads>=MAX_READS)return;
    const now=this.now();
    if(!Number.isFinite(now)||now<0||now>Number.MAX_SAFE_INTEGER||now<context.budget.last){this.invalidate();return;}
    const delay=Math.max(0,context.budget.last+SPACING_MS-now);
    this.scheduled=setTimeout(()=>{this.scheduled=undefined;if(this.context===context&&this.valid(context))this.begin(context);},delay);
  }
  private begin(context:Context):void{
    if(!this.valid(context)||this.flight||!context.dirty||context.budget.reads>=MAX_READS)return;
    const now=this.now();
    if(!Number.isFinite(now)||now<0||now>Number.MAX_SAFE_INTEGER||now<context.budget.last){this.invalidate();return;}
    context.budget.reads++;context.budget.last=now;context.dirty=false;
    const flight:Flight={context,expired:false};this.flight=flight;
    flight.timeout=setTimeout(()=>{flight.expired=true;flight.timeout=undefined;},READ_MS);
    // Do not release the native in-flight slot on local timeout/invalidation:
    // an uncancellable IPC promise may still be running. No overlapping retry.
    let read:Promise<unknown>;
    try{read=Promise.resolve(this.options.read(context.scope.actorId));}catch{read=Promise.reject();}
    void read.then(value=>{
      if(flight.expired||!this.valid(context)||this.flight!==flight)return;
      if(value===null){this.mismatch(context);return;}
      let next:WorldState;try{next=parseWorldState(value);}catch{return;}
      if(!matches(next,context.scope)){this.mismatch(context);return;}
      context.confirmed=true;this.admit(context,next,true);
    },()=>{/* An error cannot fabricate an authoritative epoch or zero pose. */}).finally(()=>{
      if(flight.timeout!==undefined)clearTimeout(flight.timeout);
      if(this.flight!==flight)return;this.flight=null;
      // Only a currently explicit dirty intent may wake. A retired context's
      // data/hints do not transfer; a successor may have independently requested
      // its first read while waiting for this one native slot to settle.
      this.schedule();
    }).catch(()=>{/* No raw transport/callback exception is surfaced as data. */});
  }
  private mismatch(context:Context):void{
    if(!this.valid(context))return;
    this.invalidate();
    // Recheck the captured external lifetime; revoke can only target this mount.
    if(!context.signal.aborted&&Object.is(context.revision,this.options.revision())
      &&this.options.current(context.scope,context.signal))this.options.mismatch(context.scope,context.signal);
  }
  invalidate():void{
    if(this.scheduled!==undefined)clearTimeout(this.scheduled);this.scheduled=undefined;
    const context=this.context;this.context=null;
    context?.signal.removeEventListener('abort',context.abort);
    if(this.flight){this.flight.expired=true;if(this.flight.timeout!==undefined)clearTimeout(this.flight.timeout);this.flight.timeout=undefined;}
    this.options.changed?.();
  }
  dispose():void{if(this.disposed)return;this.disposed=true;this.invalidate();}
}
