// SPDX-License-Identifier: MIT
/** A host observation for read-only views. No renderer registration, layout,
 * measurement, action command or completion acknowledgement belongs here. */
import type { PublicActor } from './life-runtime';
import type { WorldAnchor, WorldState } from './world-state';

export type SemanticOwner = Readonly<{renderSessionId:string;bindingSequence:number;surfaceId:string}>;
export type SemanticMirrorActor = Readonly<{
  actorId:string;controlled:true;owner:SemanticOwner|null;
  worldInstanceId:string|null;sourceWorldId:string|null;stageId:string|null;configRevision:number|null;visible:boolean;
}>;
export type SemanticMirrorPacket = Readonly<{
  version:1;epoch:string;sequence:number;owners:readonly SemanticMirrorActor[];objectOwnerActorId:string|null;
}>;
export type SemanticMirrorView = Readonly<{
  actorId:string;owner:SemanticOwner;position:Readonly<{x:number;y:number}>;
  action:PublicActor['activity'];key:string;anchors:readonly WorldAnchor[];
}>;
const invalid=()=>new Error('Die native Figurenausgabe ist nicht aktuell bestätigt.');
function fields(value:unknown,keys:readonly string[]):Record<string,unknown>{
  if(!value||typeof value!=='object'||Array.isArray(value))throw invalid();
  const prototype=Object.getPrototypeOf(value);
  if(prototype!==Object.prototype&&prototype!==null)throw invalid();
  const descriptors=Object.getOwnPropertyDescriptors(value);
  if(Reflect.ownKeys(descriptors).length!==keys.length||keys.some(key=>!Object.hasOwn(descriptors,key)||!Object.hasOwn(descriptors[key],'value')))throw invalid();
  return Object.fromEntries(keys.map(key=>[key,descriptors[key].value]));
}
function id(value:unknown):string{if(typeof value!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(value))throw invalid();return value;}
function count(value:unknown,min=1):number{if(typeof value!=='number'||!Number.isSafeInteger(value)||value<min)throw invalid();return value;}
export function parseSemanticMirrorPacket(value:unknown):SemanticMirrorPacket{
  const data=fields(value,['version','epoch','sequence','owners','objectOwnerActorId']);
  if(data.version!==1||!Array.isArray(data.owners)||data.owners.length>80)throw invalid();
  const owners=data.owners.map(raw=>{
    const row=fields(raw,['actorId','controlled','owner','worldInstanceId','sourceWorldId','stageId','configRevision','visible']);
    if(row.controlled!==true||typeof row.visible!=='boolean')throw invalid();
    let owner:SemanticOwner|null=null;
    if(row.owner!==null){const o=fields(row.owner,['renderSessionId','bindingSequence','surfaceId']);owner=Object.freeze({renderSessionId:id(o.renderSessionId),bindingSequence:count(o.bindingSequence),surfaceId:id(o.surfaceId)});}
    const absent=row.worldInstanceId===null&&row.sourceWorldId===null&&row.stageId===null&&row.configRevision===null;
    if(absent){if(owner!==null||row.visible)throw invalid();}
    else if(owner===null)throw invalid();
    return Object.freeze({actorId:id(row.actorId),controlled:true as const,owner,
      worldInstanceId:absent?null:id(row.worldInstanceId),sourceWorldId:absent?null:id(row.sourceWorldId),
      stageId:absent?null:id(row.stageId),configRevision:absent?null:count(row.configRevision),visible:row.visible});
  });
  if(new Set(owners.map(row=>row.actorId)).size!==owners.length)throw invalid();
  const objectOwnerActorId=data.objectOwnerActorId===null?null:id(data.objectOwnerActorId);
  return Object.freeze({version:1,epoch:id(data.epoch),sequence:count(data.sequence),owners:Object.freeze(owners),objectOwnerActorId});
}

/** One instance per actual Studio mount or authenticated OBS connection.
 * Only seed() at that connection's fresh response can select an epoch. Events
 * from another epoch are hints for a fresh read, never replacement authority. */
export class WorldSemanticMirrors {
  private packet:SemanticMirrorPacket|null=null;
  private received=-Infinity;
  private retired=new Map<string,string>();
  private retiredOverflow=false;
  constructor(private now:()=>number=()=>performance.now()){}
  seed(value:unknown):boolean{
    const packet=parseSemanticMirrorPacket(value),now=this.now();
    if(!Number.isFinite(now)||now<0||this.packet?.epoch===packet.epoch&&packet.sequence<=this.packet.sequence)return false;
    if(this.packet?.epoch!==packet.epoch){this.retired.clear();this.retiredOverflow=false;}
    this.packet=packet;this.received=now;return true;
  }
  accept(value:unknown):boolean{
    const packet=parseSemanticMirrorPacket(value),old=this.packet,now=this.now();
    if(!old||packet.epoch!==old.epoch||packet.sequence<=old.sequence||!Number.isFinite(now)||now<this.received)return false;
    this.packet=packet;this.received=now;return true;
  }
  get seeded():boolean{return this.packet!==null;}
  /** Last native negative evidence stays suppress-only if transport is lost.
   * A stale/hidden view cannot reclaim an owner by waiting for a timer. */
  controlled(actorId:string):boolean{return this.packet?.owners.some(row=>row.actorId===actorId)??false;}
  view(actorId:string,world:WorldState|undefined):SemanticMirrorView|null{
    const packet=this.packet,now=this.now();
    if(this.retiredOverflow||!packet||!Number.isFinite(now)||now<this.received||now-this.received>1000||world?.availability!=='ready'||!world.snapshot)return null;
    const row=packet.owners.find(row=>row.actorId===actorId),snapshot=world.snapshot;
    if(!row?.visible||!row.owner||this.retired.get(actorId)===JSON.stringify(row.owner)||row.worldInstanceId!==world.instanceId||row.stageId!==snapshot.stageId||row.configRevision!==snapshot.configRevision)return null;
    const actor=snapshot.actors.find(actor=>actor.actorId===actorId);
    if(!actor?.life||actor.life.worldId!==row.sourceWorldId)return null;
    const action=actor.life.activity;
    if(action&&(action.id.actorId!==actorId||action.id.worldId!==row.sourceWorldId))return null;
    return Object.freeze({actorId,owner:row.owner,position:Object.freeze({...actor.position}),action,
      key:JSON.stringify([packet.epoch,row.owner.renderSessionId,row.owner.bindingSequence,row.worldInstanceId,row.sourceWorldId,row.stageId,row.configRevision,action?.id??null]),
      anchors:packet.objectOwnerActorId===actorId?snapshot.anchors:[]});
  }
  /** Stop/profile change retires this exact observed owner, even if an older
   * in-flight event arrives after the UI event. A successor has a new identity. */
  retire():void{for(const row of this.packet?.owners??[])if(row.owner){
    if(!this.retired.has(row.actorId)&&this.retired.size>=80){this.retiredOverflow=true;continue;}
    this.retired.set(row.actorId,JSON.stringify(row.owner));
  }}
  reset():void{this.packet=null;this.received=-Infinity;this.retired.clear();this.retiredOverflow=false;}
}
