// SPDX-License-Identifier: MIT
/** Asset observations only. Loading/ready never authorizes a native surface,
 * successful render, calibration, action, audio or playback. */
import type {AvatarModelInfo} from './avatar';

export type NativeAvatarBinding=readonly[string,string,readonly(string|null)[]];
export type NativeAssetProbe=Readonly<{version:1;probe:string;avatarBinding:NativeAvatarBinding}>;
export type NativeAssetStatus='loading'|'ready'|'failed';
export interface NativeReadinessOptions {
  invoke(command:string,args:Record<string,unknown>):Promise<unknown>;
  /** Current actual profile binding, or null before its initial native read. */
  binding():unknown;
  current():boolean;
  mountId?:()=>string;
  now?:()=>number;
}
const PIN_MS=5000,LOAD_MS=60000,REPORT_MS=2000;
function record(value:unknown,keys:readonly string[]):Record<string,PropertyDescriptor>|null{
  if(!value||typeof value!=='object'||Array.isArray(value))return null;
  const prototype=Object.getPrototypeOf(value),items=Object.getOwnPropertyDescriptors(value);
  if(prototype!==Object.prototype&&prototype!==null||Reflect.ownKeys(items).length!==keys.length
    ||keys.some(key=>!items[key]||!Object.hasOwn(items[key],'value')))return null;
  return items;
}
function array(value:unknown,max:number):unknown[]|null{
  if(!Array.isArray(value)||Object.getPrototypeOf(value)!==Array.prototype||value.length>max)return null;
  const items=Object.getOwnPropertyDescriptors(value);
  if(Reflect.ownKeys(items).length!==value.length+1)return null;
  const out:unknown[]=[];
  for(let i=0;i<value.length;i++){const item=items[String(i)];if(!item||!Object.hasOwn(item,'value'))return null;out.push(item.value);}
  return out;
}
export function parseNativeAvatarBinding(value:unknown):NativeAvatarBinding|null{
  const item=array(value,3);if(!item||item.length!==3)return null;
  const [actor,asset,motionValue]=item,motions=array(motionValue,32);
  if(typeof actor!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(actor)
    ||typeof asset!=='string'||!asset.length||asset.length>4096||/[\u0000-\u001f\u007f-\u009f]/.test(asset)||!motions
    ||motions.some(id=>id!==null&&(typeof id!=='string'||!id.length||id.length>512||/[\u0000-\u001f\u007f-\u009f]/.test(id))))return null;
  return Object.freeze([actor,asset,Object.freeze(motions as (string|null)[])] as const);
}
function same(a:NativeAvatarBinding,b:NativeAvatarBinding):boolean{
  return a[0]===b[0]&&a[1]===b[1]&&a[2].length===b[2].length&&a[2].every((id,index)=>id===b[2][index]);
}
export function parseNativeAssetProbe(value:unknown):NativeAssetProbe|null{
  const fields=record(value,['version','probe','avatarBinding']);if(!fields)return null;
  const binding=parseNativeAvatarBinding(fields.avatarBinding.value),probe=fields.probe.value;
  if(fields.version.value!==1||typeof probe!=='string'||!/^[a-f0-9]{64}$/.test(probe)||!binding)return null;
  return Object.freeze({version:1,probe,avatarBinding:binding});
}
function accepted(value:unknown):boolean{
  const fields=record(value,['version','accepted']);return !!fields&&fields.version.value===1&&fields.accepted.value===true;
}
type Observation={binding:NativeAvatarBinding;status:NativeAssetStatus};
type Probe={value:NativeAssetProbe;mount:string;desired:NativeAssetStatus;loadingSent:boolean;loadingAck:boolean;
  terminalSent:boolean;terminal:boolean;blocked:boolean;issued:number;loadingAt:number|null;deadline?:ReturnType<typeof setTimeout>};
type Flight={owner:Probe;status:NativeAssetStatus;expired:boolean;timeout?:ReturnType<typeof setTimeout>};
export class NativeWorldReadiness {
  private active:Probe|null=null;
  private retired='';
  private observation:Observation|null=null;
  private flight:Flight|null=null;
  private disposed=false;
  private now:()=>number;
  constructor(private options:NativeReadinessOptions){this.now=options.now??(()=>performance.now());}
  private live(probe:Probe):boolean{return !this.disposed&&this.active===probe&&!probe.blocked&&this.options.current();}
  /** Native eval only. Identical delivery cannot mint a new mount or deadline;
   * another probe must follow explicit host revoke. */
  probe(raw:unknown):void{
    const value=parseNativeAssetProbe(raw);
    if(!value||this.disposed||value.probe===this.retired||this.active||!this.options.current())return;
    const issued=this.now();let mount:string;
    try{mount=this.options.mountId?.()??crypto.randomUUID();}catch{return;}
    if(!Number.isFinite(issued)||issued<0||!/^[A-Za-z0-9_-]{1,96}$/.test(mount))return;
    const probe:Probe={value,mount,desired:'loading',loadingSent:false,loadingAck:false,terminalSent:false,terminal:false,blocked:false,issued,loadingAt:null};
    this.active=probe;
    probe.deadline=setTimeout(()=>this.stop(probe),PIN_MS);
    const current=parseNativeAvatarBinding(this.options.binding()),cached=this.observation;
    if(current&&cached&&same(current,value.avatarBinding)&&same(cached.binding,current))probe.desired=cached.status;
    this.pump();
  }
  /** Only Avatar.onModelLoaded calls this. Its callback closes over the profile
   * binding; ready additionally carries the loader's actual loaded binding.
   * Avatar's existing assetRequest/dispose guards remain mandatory. */
  model(callbackBinding:unknown,info:Pick<AvatarModelInfo,'status'|'avatarBinding'>):void{
    if(this.disposed||!this.options.current())return;
    const observed=parseNativeAvatarBinding(callbackBinding),current=parseNativeAvatarBinding(this.options.binding());
    if(!observed||!current||!same(observed,current))return;
    let status:NativeAssetStatus;
    if(info.status==='ready'){
      const loaded=parseNativeAvatarBinding(info.avatarBinding);if(!loaded||!same(loaded,current))return;
      status='ready';
    }else if(info.status==='loading')status='loading';
    else if(info.status==='error')status='failed';else return;
    this.observation={binding:current,status};
    const probe=this.active;
    if(!probe||!this.live(probe)||!same(probe.value.avatarBinding,current))return;
    if(probe.terminalSent){if(status==='loading'||status==='failed')this.stop(probe);return;}
    probe.desired=status;this.pump();
  }
  private stop(probe:Probe):void{
    probe.blocked=true;if(probe.deadline!==undefined)clearTimeout(probe.deadline);probe.deadline=undefined;
    if(this.flight?.owner===probe)this.flight.expired=true;
  }
  private pump():void{
    const probe=this.active;if(!probe||!this.live(probe)||probe.terminal||this.flight)return;
    const now=this.now();
    if(!Number.isFinite(now)||now<probe.issued||now-probe.issued>=PIN_MS&&!probe.loadingAck
      ||probe.loadingAt!==null&&now-probe.loadingAt>=LOAD_MS){this.stop(probe);return;}
    const status:NativeAssetStatus=!probe.loadingSent?'loading':probe.desired;
    if(probe.loadingSent&&(!probe.loadingAck||status==='loading'||probe.terminalSent))return;
    if(status==='loading'){probe.loadingSent=true;probe.loadingAt=now;}else probe.terminalSent=true;
    const flight:Flight={owner:probe,status,expired:false};this.flight=flight;
    flight.timeout=setTimeout(()=>{flight.timeout=undefined;flight.expired=true;this.stop(probe);},REPORT_MS);
    let report:Promise<unknown>;
    try{report=Promise.resolve(this.options.invoke('report_native_world_asset',{request:{version:1,probe:probe.value.probe,mountId:probe.mount,status}}));}
    catch{report=Promise.reject();}
    void report.then(result=>{
      if(flight.expired||!this.live(probe)||this.flight!==flight)return;
      if(!accepted(result)){this.stop(probe);return;}
      if(status==='loading'){
        probe.loadingAck=true;if(probe.deadline!==undefined)clearTimeout(probe.deadline);
        const remaining=LOAD_MS-(this.now()-(probe.loadingAt??probe.issued));
        if(!Number.isFinite(remaining)||remaining<=0||remaining>LOAD_MS){this.stop(probe);return;}
        probe.deadline=setTimeout(()=>this.stop(probe),remaining);
      }else{
        probe.terminal=true;if(probe.deadline!==undefined)clearTimeout(probe.deadline);probe.deadline=undefined;
      }
    },()=>{if(this.active===probe)this.stop(probe);}).finally(()=>{
      if(flight.timeout!==undefined)clearTimeout(flight.timeout);
      if(this.flight!==flight)return;this.flight=null;
      // Only a separately current probe's pending fact may wake. A timed-out
      // promise keeps this one native slot until it actually settles.
      this.pump();
    }).catch(()=>{/* Never surface raw IPC/model error content. */});
  }
  /** Real profile change invalidates its cached observation as well. Route or
   * World revoke can retain an actually loaded identical asset for a new probe. */
  profileChanged():void{this.observation=null;this.revoke();}
  revoke():void{
    const old=this.active;if(old){this.retired=old.value.probe;this.stop(old);}this.active=null;
  }
  dispose():void{if(this.disposed)return;this.disposed=true;this.observation=null;this.revoke();}
}
