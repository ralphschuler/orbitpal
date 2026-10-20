import { activityLabels, phaseLabels } from './life-runtime';
import type { LifeUI } from './life-ui';

export const actionReasons = {
  life_not_initialized: 'Für diesen Bot ist noch kein lokaler Life-Zustand angelegt.',
  life_not_active: 'Starte zuerst ausdrücklich den Life-Modus für diesen Bot.',
  system_unavailable: 'Der Systemstatus ist nicht bestätigt. Die lokale Aktion bleibt gesperrt.',
  ui_unavailable: 'Verbinde das Studio erneut, bevor du eine lokale Aktion startest.',
  output_unavailable: 'Für die lokale Aktion fehlt eine aktuelle Figurenausgabe.',
  world_unavailable: 'Wähle diesen Bot als Teilnehmer einer gültigen gemeinsamen Bühne aus.',
  storage_unavailable: 'Der gespeicherte Aktionszustand ist nicht verfügbar. Es wird keine Aktion gestartet.',
  busy: 'Beende zuerst die laufende Aktion oder Ausgabe.',
} as const;
export const actionStatusLabels = { awaiting_delivery:'Wartet auf Zustellung', delivered:'Zugestellt', started:'Begonnen', paused:'Pausiert', completed:'Abgeschlossen', cancelled:'Abgebrochen', failed:'Fehlgeschlagen' } as const;
export const actionFailureLabels = { unsupported:'Die benötigte Fähigkeit ist nicht verfügbar.', output_lost:'Die zuständige Figurenausgabe ist nicht mehr verfügbar.', timeout:'Die Aktion wurde nach Ablauf ihrer Frist beendet.', interrupted:'Die aktive Ausführung wurde unterbrochen.', storage:'Der Aktionszustand konnte nicht gespeichert werden.', protocol:'Die Aktionsbestätigung war ungültig.', exhausted:'Die lokale Aktionslaufzeit ist nicht mehr verfügbar.' } as const;
export const cancellationLabels = { operator:'Vom Benutzer abgebrochen', interrupted:'Unterbrochen', unavailable:'Nicht verfügbar', timeout:'Zeitüberschreitung', restart:'Beim Neustart abgebrochen' } as const;
export interface ActionIdentity { actorId:string;worldId:string;ordinal:number; }
export interface LifeActionsSnapshot {
  version:1;instanceId:string;sequence:number;botId:string;
  availability:'ready'|'not_initialized'|'unavailable';reason:keyof typeof actionReasons|null;
  visualAvailable:boolean;
  confirmed:{actionId:ActionIdentity;activity:keyof typeof activityLabels;phase:keyof typeof phaseLabels;cancellation:keyof typeof cancellationLabels|null;revision:number}|null;
  runtime:{actionId:ActionIdentity;presentation:'native_rest_dwell'|'visual';status:keyof typeof actionStatusLabels;failure:keyof typeof actionFailureLabels|null;activeMilliseconds:number}|null;
}
const invalid=()=>new Error('Die native Aktionslaufzeit hat ungültige Daten geliefert. Es wird keine Aktion gestartet.');
export const actionChanged=()=>new Error('Der Auftrag für die stille Pause ist nicht mehr aktuell. Es wurde keine neue Aktion bestätigt.');
function object(value:unknown):Record<string,unknown>{if(!value||typeof value!=='object'||Array.isArray(value))throw invalid();return value as Record<string,unknown>;}
function count(value:unknown,min=0):number{if(typeof value!=='number'||!Number.isSafeInteger(value)||value<min)throw invalid();return value;}
function id(value:unknown):string{if(typeof value!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(value))throw invalid();return value;}
function key<T extends string>(value:unknown,labels:Record<T,string>):T{if(typeof value!=='string'||!Object.hasOwn(labels,value))throw invalid();return value as T;}
function identity(value:unknown,botId:string):ActionIdentity{const item=object(value),actorId=id(item.actorId);if(actorId!==botId)throw invalid();return{actorId,worldId:id(item.worldId),ordinal:count(item.ordinal,1)};}
export function sameAction(a:ActionIdentity,b:ActionIdentity):boolean{return a.actorId===b.actorId&&a.worldId===b.worldId&&a.ordinal===b.ordinal;}
export function parseLifeActions(value:unknown,expectedBotId:string):LifeActionsSnapshot{
  const item=object(value),botId=id(item.botId);
  if(item.version!==1||botId!==id(expectedBotId)||typeof item.instanceId!=='string'||!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(item.instanceId)||typeof item.visualAvailable!=='boolean')throw invalid();
  if(!['ready','not_initialized','unavailable'].includes(String(item.availability)))throw invalid();
  const availability=item.availability as LifeActionsSnapshot['availability'],reason=item.reason===null?null:key(item.reason,actionReasons);
  if(availability==='ready'&&reason!==null||availability==='not_initialized'&&reason!=='life_not_initialized'||availability==='unavailable'&&(reason===null||reason==='life_not_initialized'))throw invalid();
  const confirmed=item.confirmed===null?null:(()=>{const data=object(item.confirmed),phase=key(data.phase,phaseLabels),cancellation=data.cancellation===null?null:key(data.cancellation,cancellationLabels);if((phase==='cancelled')!==(cancellation!==null))throw invalid();return{actionId:identity(data.actionId,botId),activity:key(data.activity,activityLabels),phase,cancellation,revision:count(data.revision)};})();
  const runtime=item.runtime===null?null:(()=>{const data=object(item.runtime),status=key(data.status,actionStatusLabels),failure=data.failure===null?null:key(data.failure,actionFailureLabels);if(!['native_rest_dwell','visual'].includes(String(data.presentation))||(status==='failed')!==(failure!==null))throw invalid();return{actionId:identity(data.actionId,botId),presentation:data.presentation as 'native_rest_dwell'|'visual',status,failure,activeMilliseconds:count(data.activeMilliseconds)};})();
  return{version:1,instanceId:item.instanceId,sequence:count(item.sequence),botId,availability,reason,visualAvailable:item.visualAvailable,confirmed,runtime};
}
export type ActionsInvoke=(command:string,args?:Record<string,unknown>)=>Promise<unknown>;
/** Explicit main-window request only. No mode initialization, automatic retry,
 * capability grant or invented status. The clear is truthful at both checks. */
export async function requestQuietRest(invoke:ActionsInvoke,reporter:Pick<LifeUI,'reportProof'>,botId:string,current:()=>boolean,onProof:()=>void=()=>{}):Promise<LifeActionsSnapshot|undefined>{
  const target=id(botId);if(!current())throw actionChanged();
  const raw=await reporter.reportProof(false,true,proof=>{
    if(!current())return Promise.reject(actionChanged());
    onProof();
    return invoke('start_life_rest',{botId:target,ui:{connectionId:proof.connectionId,sequence:proof.sequence}});
  });
  if(!current())return;
  const next=parseLifeActions(raw,target);
  if(!next.runtime||next.runtime.presentation!=='native_rest_dwell'||!next.confirmed||next.confirmed.activity!=='rest'||!sameAction(next.runtime.actionId,next.confirmed.actionId))throw invalid();
  return next;
}
/** A single operator-selected canonical destination. This never starts a mode,
 * retries an old action, chooses a random target or grants missing capabilities. */
export async function requestLifeMove(invoke:ActionsInvoke,reporter:Pick<LifeUI,'reportProof'>,botId:string,target:{x:number;y:number},current:()=>boolean,onProof:()=>void=()=>{}):Promise<LifeActionsSnapshot|undefined>{
  const owner=id(botId),point={x:count(target.x),y:count(target.y)};
  if(point.x>1_000_000||point.y>1_000_000)throw invalid();
  if(!current())throw actionChanged();
  const raw=await reporter.reportProof(false,true,proof=>{
    if(!current())return Promise.reject(actionChanged());
    onProof();return invoke('start_life_move',{botId:owner,target:point,ui:{connectionId:proof.connectionId,sequence:proof.sequence}});
  });
  if(!current())return;
  const next=parseLifeActions(raw,owner);
  if(!next.runtime||next.runtime.presentation!=='visual'||!next.confirmed||next.confirmed.activity!=='move_to'||!sameAction(next.runtime.actionId,next.confirmed.actionId))throw invalid();
  return next;
}
