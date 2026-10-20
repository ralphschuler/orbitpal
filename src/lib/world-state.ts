import { publicActor, type PublicActor } from './life-runtime';
export const worldReasons = {
  database:'Die gemeinsame Bühne ist vorübergehend nicht verfügbar.',
  unsupported_schema:'Diese Version der Bühnenkonfiguration wird nicht unterstützt.',
  corrupt_config:'Die gespeicherte Bühnenkonfiguration ist ungültig. Andere Bot-Daten bleiben verfügbar.',
  unknown_actor:'Ein ausgewählter Bot ist nicht mehr verfügbar.',
  invalid_scope:'Die gemeinsame Bühne kann nur für diese Installation verwaltet werden.',
  stale_revision:'Die Bühne wurde inzwischen geändert. Bitte neu laden.',
  wrong_stage:'Eine vorhandene Bühne kann nicht durch eine andere Bühnen-ID ersetzt werden.',
  invalid_binding:'Die Monitorauswahl der Bühne ist ungültig.',
  capacity:'Die Bühnenkonfiguration überschreitet die zulässige Größe.',
} as const;
export const anchorKinds={rest:'Ruhe- oder Ladeplatz',meeting:'Treffpunkt',play:'Spielobjekt'} as const;
export type AnchorKind=keyof typeof anchorKinds;
export interface WorldPoint {x:number;y:number}
export interface WorldRegion {min:WorldPoint;max:WorldPoint}
export interface WorldSlot {id:string;position:WorldPoint}
export interface WorldAnchor {id:string;kind:AnchorKind;position:WorldPoint;slots:WorldSlot[]}
export interface WorldConfig {schemaVersion:1;stageId:string;revision:number;safeRegion:WorldRegion;members:string[];anchors:WorldAnchor[]}
export interface WorldRecord {config:WorldConfig;outputBinding:{monitorId:string|null}}
export interface WorldSettings {version:1;instanceId:string;availability:'ready'|'not_initialized'|'unavailable';reason:keyof typeof worldReasons|null;record:WorldRecord|null}
export interface WorldSnapshot {schemaVersion:1;stageId:string;configRevision:number;sequence:number;safeRegion:WorldRegion;anchors:WorldAnchor[];actors:{actorId:string;position:WorldPoint;life:PublicActor|null}[];occupied:{slotId:string;actorId:string}[]}
export interface WorldState {version:1;instanceId:string;availability:WorldSettings['availability'];reason:WorldSettings['reason'];snapshot:WorldSnapshot|null}
export interface PointDraft {x:string;y:string}
export interface WorldDraft {stageId:string;revision:number;monitorId:string;members:string[];safeRegion:{min:PointDraft;max:PointDraft};anchors:{id:string;kind:AnchorKind;position:PointDraft;slots:{id:string;position:PointDraft}[]}[]}
export const invalidWorld=()=>new Error('Die Bühnenangaben sind ungültig. Prüfe Bewegungsbereich, Plätze und Teilnehmer.');
function object(value:unknown):Record<string,unknown>{if(!value||typeof value!=='object'||Array.isArray(value))throw invalidWorld();return value as Record<string,unknown>;}
function number(value:unknown,min=0,max=Number.MAX_SAFE_INTEGER):number{if(typeof value!=='number'||!Number.isSafeInteger(value)||value<min||value>max)throw invalidWorld();return value;}
function id(value:unknown):string{if(typeof value!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(value))throw invalidWorld();return value;}
function list(value:unknown,max:number):unknown[]{if(!Array.isArray(value)||value.length>max)throw invalidWorld();return value;}
function unique(values:string[]):void{if(new Set(values).size!==values.length)throw invalidWorld();}
function point(value:unknown):WorldPoint{const item=object(value);return{x:number(item.x,0,1_000_000),y:number(item.y,0,1_000_000)};}
function region(value:unknown):WorldRegion{const item=object(value),min=point(item.min),max=point(item.max);if(min.x>=max.x||min.y>=max.y)throw invalidWorld();return{min,max};}
function inside(p:WorldPoint,r:WorldRegion):void{if(p.x<r.min.x||p.y<r.min.y||p.x>r.max.x||p.y>r.max.y)throw invalidWorld();}
function anchors(value:unknown,safe:WorldRegion):WorldAnchor[]{
  const result=list(value,64).map(raw=>{const item=object(raw),kind=item.kind;if(typeof kind!=='string'||!Object.hasOwn(anchorKinds,kind))throw invalidWorld();const position=point(item.position);inside(position,safe);const slots=list(item.slots,128).map(raw=>{const slot=object(raw),position=point(slot.position);inside(position,safe);return{id:id(slot.id),position};});if(!slots.length)throw invalidWorld();return{id:id(item.id),kind:kind as AnchorKind,position,slots};});
  unique(result.map(anchor=>anchor.id));const slots=result.flatMap(anchor=>anchor.slots);if(slots.length>128)throw invalidWorld();unique(slots.map(slot=>slot.id));return result;
}
export function parseWorldConfig(value:unknown):WorldConfig{const item=object(value);if(item.schemaVersion!==1)throw invalidWorld();const safeRegion=region(item.safeRegion),members=list(item.members,64).map(id);unique(members);return{schemaVersion:1,stageId:id(item.stageId),revision:number(item.revision),safeRegion,members,anchors:anchors(item.anchors,safeRegion)};}
function envelope(value:unknown){const item=object(value);if(item.version!==1||typeof item.instanceId!=='string'||!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(item.instanceId)||!['ready','not_initialized','unavailable'].includes(String(item.availability)))throw invalidWorld();const reason=item.reason;if(reason!==null&&(typeof reason!=='string'||!Object.hasOwn(worldReasons,reason)))throw invalidWorld();if((item.availability==='unavailable')!==(reason!==null))throw invalidWorld();return{item,header:{version:1 as const,instanceId:item.instanceId,availability:item.availability as WorldSettings['availability'],reason:reason as WorldSettings['reason']}};}
function binding(value:unknown):WorldRecord['outputBinding']{const item=object(value),monitorId=item.monitorId;if(monitorId!==null&&(typeof monitorId!=='string'||new TextEncoder().encode(monitorId).byteLength>512||!monitorId.length||/\p{Cc}/u.test(monitorId)))throw invalidWorld();return{monitorId};}
export function parseWorldSettings(value:unknown):WorldSettings{const{item,header}=envelope(value);const record=item.record===null?null:(()=>{const raw=object(item.record);return{config:parseWorldConfig(raw.config),outputBinding:binding(raw.outputBinding)};})();if(header.availability==='ready'&&!record||header.availability==='not_initialized'&&record||header.availability==='unavailable'&&record&&header.reason!=='unknown_actor')throw invalidWorld();return{...header,record};}
export function parseWorldState(value:unknown):WorldState{
  const{item,header}=envelope(value);if(item.snapshot===null){if(header.availability==='ready')throw invalidWorld();return{...header,snapshot:null};}if(header.availability!=='ready')throw invalidWorld();
  const raw=object(item.snapshot);if(raw.schemaVersion!==1)throw invalidWorld();const safeRegion=region(raw.safeRegion),resources=anchors(raw.anchors,safeRegion);
  const actors=list(raw.actors,64).map(value=>{const actor=object(value),actorId=id(actor.actorId),position=point(actor.position),life=actor.life===null?null:publicActor(actor.life);inside(position,safeRegion);if(life&&life.actorId!==actorId)throw invalidWorld();return{actorId,position,life};});unique(actors.map(actor=>actor.actorId));
  const slotIds=new Set(resources.flatMap(anchor=>anchor.slots.map(slot=>slot.id))),actorIds=new Set(actors.map(actor=>actor.actorId));
  const occupied=list(raw.occupied,128).map(value=>{const item=object(value),slotId=id(item.slotId),actorId=id(item.actorId);if(!slotIds.has(slotId)||!actorIds.has(actorId))throw invalidWorld();return{slotId,actorId};});unique(occupied.map(slot=>slot.slotId));
  return{...header,snapshot:{schemaVersion:1,stageId:id(raw.stageId),configRevision:number(raw.configRevision),sequence:number(raw.sequence),safeRegion,anchors:resources,actors,occupied}};
}
/** Exact fixed-point conversion: 100% == 1,000,000; no floating point rounding. */
export function percentToCanonical(value:string):number{const trimmed=value.trim().replace(',','.');if(!/^\d{1,3}(?:\.\d{1,4})?$/.test(trimmed))throw invalidWorld();const[whole,fraction='']=trimmed.split('.');return number(Number(whole)*10000+Number(fraction.padEnd(4,'0')),0,1_000_000);}
export function canonicalToPercent(value:number):string{number(value,0,1_000_000);const fraction=String(value%10000).padStart(4,'0').replace(/0+$/,'');return`${Math.floor(value/10000)}${fraction?'.'+fraction:''}`;}
const draftPoint=(point:WorldPoint):PointDraft=>({x:canonicalToPercent(point.x),y:canonicalToPercent(point.y)});
export function worldDraft(record:WorldRecord):WorldDraft{const c=record.config;return{stageId:c.stageId,revision:c.revision,monitorId:record.outputBinding.monitorId??'',members:[...c.members],safeRegion:{min:draftPoint(c.safeRegion.min),max:draftPoint(c.safeRegion.max)},anchors:c.anchors.map(anchor=>({...anchor,position:draftPoint(anchor.position),slots:anchor.slots.map(slot=>({...slot,position:draftPoint(slot.position)}))}))};}
export function draftWorld(draft:WorldDraft):WorldRecord{const position=(p:PointDraft)=>({x:percentToCanonical(p.x),y:percentToCanonical(p.y)});return{config:parseWorldConfig({schemaVersion:1,stageId:draft.stageId,revision:draft.revision,members:draft.members,safeRegion:{min:position(draft.safeRegion.min),max:position(draft.safeRegion.max)},anchors:draft.anchors.map(anchor=>({...anchor,position:position(anchor.position),slots:anchor.slots.map(slot=>({...slot,position:position(slot.position)}))}))}),outputBinding:binding({monitorId:draft.monitorId||null})};}
export function worldDraftChanged(draft:WorldDraft,record:WorldRecord|null):boolean{if(!record)return true;return JSON.stringify(draft)!==JSON.stringify(worldDraft(record));}
export function newWorldDraft(stageId:string):WorldDraft{return worldDraft({config:{schemaVersion:1,stageId:id(stageId),revision:0,members:[],safeRegion:{min:{x:50000,y:50000},max:{x:950000,y:950000}},anchors:[]},outputBinding:{monitorId:null}});}
export type WorldInvoke=(command:string,args?:Record<string,unknown>)=>Promise<unknown>;
export async function saveWorld(invoke:WorldInvoke,draft:WorldDraft,expectedRevision:number|null):Promise<WorldSettings>{
  const record=draftWorld(draft);if(expectedRevision!==null)number(expectedRevision,0,Number.MAX_SAFE_INTEGER-1);if(record.config.revision!==(expectedRevision??0))throw invalidWorld();
  const next=parseWorldSettings(await invoke('save_world_config',{expectedRevision,...record}));
  if(next.availability!=='ready'||!next.record||next.record.config.stageId!==record.config.stageId||next.record.config.revision!==(expectedRevision??0)+1)throw invalidWorld();
  return next;
}
export async function placeWorld(invoke:WorldInvoke,revision:number,actorId:string,position:PointDraft):Promise<WorldState>{
  const point={x:percentToCanonical(position.x),y:percentToCanonical(position.y)},owner=id(actorId),expectedRevision=number(revision);
  const next=parseWorldState(await invoke('place_world_actor',{expectedRevision,actorId:owner,position:point}));
  const confirmed=next.snapshot?.actors.find(actor=>actor.actorId===owner);
  if(next.availability!=='ready'||next.snapshot?.configRevision!==expectedRevision||!confirmed||confirmed.position.x!==point.x||confirmed.position.y!==point.y)throw invalidWorld();
  return next;
}
