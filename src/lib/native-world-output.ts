// SPDX-License-Identifier: MIT
/** Constructor/document transport only. Capabilities stay in this closure;
 * renderer observations never mint a native layout, action or playback grant. */
import { WorldOutputBridge, type WorldRenderAdapter } from './world-output-bridge';
import { parseRegisteredWorldActor, type RegisteredWorldActor, type MeasurementChallenge,
  type NativeMotionBinding, type RenderMeasurement, type LocalWorldFrame,
  type WorldAdapterIdentity, type WorldAdapterReason,type BindingStamp,type CanonicalPoint } from './world-output';
import type { PresentationRelease } from './avatar-presentation';

export type NativeAuth=Readonly<{version:1;capability:string}>;
export type NativeWorldView=Readonly<{phase:'normal'|'warming'|'world';exposed:boolean;covered:boolean;
  binding?:NativeMotionBinding;challenge?:MeasurementChallenge;position?:CanonicalPoint}>;
export interface NativeWorldPort extends WorldRenderAdapter {
  prepareNativeWorldWarm(signal:AbortSignal):Promise<{frameSequence:number;release:PresentationRelease}>;
}
export interface NativeWorldOptions {
  invoke(command:string,args?:Record<string,unknown>):Promise<unknown>;
  adapter():NativeWorldPort|undefined;
  /** Waits for current profile and fixed/no-World props, never a load-ready ACK. */
  normal(signal:AbortSignal):Promise<void>;
  current():boolean;
  actorId():string;
  /** Current parsed native response scope, before any public pose lookup. No
   * event/metadata may select an epoch; signal is the existing mount lifetime. */
  scope(expected:BindingStamp,signal:AbortSignal):void;
  position(expected:BindingStamp):CanonicalPoint|null;
  changed(view:NativeWorldView):void;
  afterDOM():Promise<void>;
  now?:()=>number;
  mountId?:()=>string;
}
export function parseNativeAuth(value:unknown):NativeAuth|null{
  if(!value||typeof value!=='object'||Array.isArray(value))return null;
  const prototype=Object.getPrototypeOf(value),descriptors=Object.getOwnPropertyDescriptors(value);
  if(prototype!==Object.prototype&&prototype!==null||Reflect.ownKeys(descriptors).length!==2)return null;
  if(!descriptors.version||!descriptors.capability||!Object.hasOwn(descriptors.version,'value')||!Object.hasOwn(descriptors.capability,'value')
    ||descriptors.version.value!==1||typeof descriptors.capability.value!=='string'||!/^[a-f0-9]{64}$/.test(descriptors.capability.value))return null;
  return Object.freeze({version:1,capability:descriptors.capability.value});
}
// Preserve the existing three-second idle renewal within the native Live
// owner's five-second accepted-frame lease. Every new measurement/action
// receipt still has its separate <=250ms host-admission bound. Read-only
// mirrors follow the Live lease; they do not require calibration at 10Hz.
// Only an actual rendered measurement, never this timer, renews frame evidence.
export const NATIVE_IDLE_MEASUREMENT_MS=3000;
const cancelled=()=>new DOMException('Die Figurenpräsentation wurde abgebrochen.','AbortError');
/** One document, one active capability/mount. No permanent nonce, URL, event
 * payload or caller-selected actor can supply authority. */
export class NativeWorldOutput {
  private auth:NativeAuth|null=null;
  private retired='';
  private generation=0;
  private abort?:AbortController;
  private bridge?:WorldOutputBridge;
  private timer?:ReturnType<typeof setInterval>;
  private disposed=false;
  private view:NativeWorldView={phase:'normal',exposed:false,covered:false};
  private now:()=>number;
  constructor(private options:NativeWorldOptions){this.now=options.now??(()=>performance.now());}
  get active():boolean{return this.auth!==null;}
  private publish(view:NativeWorldView):void{this.view=Object.freeze(view);this.options.changed(this.view);}
  private live(generation:number,auth:NativeAuth):boolean{
    return !this.disposed&&generation===this.generation&&this.auth===auth&&!this.abort?.signal.aborted&&this.options.current();
  }
  /** Same-challenge native delivery retries do not restart a warm or binding.
   * A new capability must follow the host's explicit revoke/cleanup barrier. */
  bootstrap(raw:unknown):void{
    const auth=parseNativeAuth(raw);
    if(!auth||this.disposed||auth.capability===this.retired)return;
    if(this.auth)return;
    this.auth=auth;const generation=++this.generation,controller=new AbortController();this.abort=controller;
    this.publish({phase:'warming',exposed:false,covered:false});
    void this.start(auth,generation,controller).catch(()=>{if(this.auth===auth&&generation===this.generation)this.revoke();});
  }
  private async start(auth:NativeAuth,generation:number,controller:AbortController):Promise<void>{
    const deadline=this.now()+5000;
    const timeout=setTimeout(()=>{controller.abort();if(this.auth===auth&&generation===this.generation)this.revoke();},5000);
    let release:PresentationRelease|undefined;
    let last:RegisteredWorldActor|undefined;
    const alive=()=>this.live(generation,auth)&&Number.isFinite(this.now())&&this.now()<=deadline;
    try{
      await this.options.normal(controller.signal);
      if(!alive())throw cancelled();
      const adapter=this.options.adapter();if(!adapter)throw cancelled();
      const warm=await adapter.prepareNativeWorldWarm(controller.signal);release=warm.release;
      if(!alive()||adapter!==this.options.adapter()||!Number.isSafeInteger(warm.frameSequence)||warm.frameSequence<1)throw cancelled();
      // Fixed/audio-disabled mode remains until host layout/challenge arrives.
      // Release the normal hold BEFORE any native layout/context mutation.
      release();release=undefined;
      const mountId=this.options.mountId?.()??crypto.randomUUID();
      if(!/^[A-Za-z0-9_-]{1,96}$/.test(mountId))throw cancelled();
      let first=true;
      const register=async()=>{
        const request={...auth,mountId,...(first?{normalWarmFrameSequence:warm.frameSequence}:{})};first=false;
        const result=this.options.invoke('register_native_world_output',{request});
        // A timed-out/retired response may only unregister its exact identity.
        void result.then(raw=>{if(!this.live(generation,auth)){
          try{this.unregister(auth,parseRegisteredWorldActor(raw));}catch{/* Invalid data grants nothing. */}
        }}).catch(()=>{});
        return result;
      };
      while(alive()){
        const raw=await this.bounded(register(),controller.signal);
        if(!alive())throw cancelled();
        last=parseRegisteredWorldActor(raw);
        if(last.role!=='owner_candidate'||last.readiness.state==='unavailable')throw cancelled();
        if(last.readiness.state==='awaiting_layout'){
          await this.pause(controller.signal);continue;
        }
        const scope=last.readiness.state==='ready'?last.readiness.binding:last.readiness.challenge.expected;
        if(scope.actorId!==this.options.actorId())throw cancelled();
        const challenge=last.readiness.state==='awaiting_measurement'?last.readiness.challenge:null;
        const binding=last.readiness.state==='ready'?last.readiness.binding:null;
        if(challenge?.cameraPolicy!=='native_physical_iso_v1'&&binding?.cameraPolicy!=='native_physical_iso_v1')throw cancelled();
        // A fresh document cannot start from metadata-ready state without its
        // own calibration; require a host-issued measurement challenge first.
        if(!challenge)throw cancelled();
        this.options.scope(scope,controller.signal);
        if(!alive())throw cancelled();
        if(!this.options.position(scope)){await this.pause(controller.signal);continue;}
        let initial:unknown=raw;
        const bridge=new WorldOutputBridge({
          request:{version:1,mountId,actorId:scope.actorId,worldInstanceId:scope.worldInstanceId,stageId:scope.stageId,configRevision:scope.configRevision},
          register:async()=>{if(initial!==undefined){const value=initial;initial=undefined;return value;}return register();},
          invoke:(command,args)=>{
            if(command==='report_world_measurement')return this.options.invoke('report_native_world_measurement',{request:{...auth,report:args?.report}});
            if(command==='report_world_action')return this.options.invoke('report_native_world_action',{request:{...auth,report:args?.report}});
            if(command==='unregister_world_output'){
              const request=args?.request as {renderSessionId:string;bindingSequence:number};
              return this.options.invoke('unregister_native_world_output',{request:{...auth,renderSessionId:request.renderSessionId,bindingSequence:request.bindingSequence}});
            }
            return Promise.reject(cancelled());
          },adapter,current:()=>this.live(generation,auth),now:this.now,
          changed:(_output,nextBinding,nextChallenge)=>{
            if(!this.live(generation,auth))return;
            if(nextBinding&&nextBinding.cameraPolicy!=='native_physical_iso_v1'||nextChallenge&&nextChallenge.cameraPolicy!=='native_physical_iso_v1'){this.revoke();return;}
            const expected=nextChallenge?.expected??nextBinding;
            if(!expected||expected.actorId!==this.options.actorId()){this.revoke();return;}
            this.options.scope(expected,controller.signal);
            if(!this.live(generation,auth))return;
            const position=this.options.position(expected);
            if(!position){this.revoke();return;}
            this.publish({phase:'world',exposed:this.view.phase==='world'&&this.view.exposed,
              covered:this.view.phase==='world'?this.view.covered:true,
              position:Object.freeze({...position}),
              ...(nextBinding?{binding:nextBinding}:{}),...(nextChallenge?{challenge:nextChallenge}:{})});
          },failed:()=>{if(this.live(generation,auth))this.revoke();},
        });
        this.bridge=bridge;
        await bridge.refresh();
        if(!this.live(generation,auth))throw cancelled();
        this.timer=setInterval(()=>{if(this.live(generation,auth))void bridge.refresh();else this.revoke();},NATIVE_IDLE_MEASUREMENT_MS);
        return;
      }
      throw cancelled();
    }finally{clearTimeout(timeout);release?.();if(!this.live(generation,auth)&&last)this.unregister(auth,last);}
  }
  private bounded<T>(promise:Promise<T>,signal:AbortSignal):Promise<T>{
    return new Promise<T>((resolve,reject)=>{
      let settled=false;
      const finish=(success:boolean,value:unknown)=>{if(settled)return;settled=true;clearTimeout(timer);signal.removeEventListener('abort',fail);if(success)resolve(value as T);else reject(value);};
      const fail=()=>finish(false,cancelled()),timer=setTimeout(fail,2000);
      signal.addEventListener('abort',fail,{once:true});
      promise.then(value=>finish(true,value),error=>finish(false,error));if(signal.aborted)fail();
    });
  }
  private pause(signal:AbortSignal):Promise<void>{return new Promise((resolve,reject)=>{
    const abort=()=>{clearTimeout(timer);reject(cancelled());},timer=setTimeout(()=>{signal.removeEventListener('abort',abort);resolve();},100);
    signal.addEventListener('abort',abort,{once:true});if(signal.aborted)abort();
  });}
  private unregister(auth:NativeAuth,output:RegisteredWorldActor):void{
    try{void this.options.invoke('unregister_native_world_output',{request:{...auth,renderSessionId:output.renderSessionId,bindingSequence:output.bindingSequence}}).catch(()=>{});}catch{/* Local revocation is synchronous. */}
  }
  async prepared(identity:WorldAdapterIdentity):Promise<void>{
    const generation=this.generation,auth=this.auth,challenge=this.view.challenge;
    if(!auth||!challenge||identity.challengeId!==challenge.id||Object.entries(challenge.expected).some(([key,value])=>identity[key as keyof WorldAdapterIdentity]!==value))return;
    this.publish({...this.view,covered:false,exposed:false});
    await this.options.afterDOM();
    if(this.live(generation,auth)&&this.view.challenge?.id===challenge.id)this.publish({...this.view,exposed:true});
  }
  measurement(report:RenderMeasurement):void{this.bridge?.measurement(report);}
  frame(frame:LocalWorldFrame):void{this.bridge?.frame(frame);}
  receive(value:unknown):void{this.bridge?.receive(value);}
  unavailable(identity:WorldAdapterIdentity,_reason:WorldAdapterReason):void{
    const expected=this.view.challenge?.expected??this.view.binding;
    if(this.auth&&this.view.phase==='world'&&expected&&Object.entries(expected).filter(([key])=>
      ['actorId','sourceWorldId','renderSessionId','bindingSequence','worldInstanceId','stageId','configRevision','topologyRevision',
        'layoutRevision','assetRevision','measurementRevision','referenceRevision'].includes(key)).every(([key,value])=>identity[key as keyof WorldAdapterIdentity]===value))this.revoke();
  }
  /** Synchronous frontend barrier. Native must independently cancel/restore its
   * exact physical owner; a normal prop alone is NOT cleanup acknowledgement. */
  revoke():void{
    const old=this.auth;if(old)this.retired=old.capability;
    this.auth=null;++this.generation;this.abort?.abort();this.abort=undefined;
    if(this.timer)clearInterval(this.timer);this.timer=undefined;
    const bridge=this.bridge;this.bridge=undefined;bridge?.dispose('cancelled');
    this.publish({phase:'normal',exposed:false,covered:false});
  }
  dispose():void{if(this.disposed)return;this.revoke();this.disposed=true;}
}
