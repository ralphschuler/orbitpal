/** Read-only Studio projection. None of these fields is an action/owner capability. */
export const intentActivityLabels = {
  move_to: 'Unterwegs', rest: 'Ruhen', recharge: 'Energie sammeln', snack: 'Snackpause',
  play: 'Spielen', socialize: 'Gesellig sein', look_at: 'Umschauen', wave: 'Winken', greeting: 'Gemeinsame Begrüßung',
} as const;
export const intentPhaseLabels = {
  preparing: 'Ausgabe wird vorbereitet', delivered: 'An Ausgabe übergeben', invitation: 'Wartet auf eine Einladungsantwort',
  accepted: 'Einladung angenommen', resources_reserved: 'Gemeinsame Plätze reserviert', approach: 'Auf dem Weg zum Ziel',
  execution: 'Aktivität läuft', paused: 'Aktivität unterbrochen', cancelling: 'Abbruch angefordert',
  completed: 'Ausführung bestätigt', cancelled: 'Absicht abgebrochen', declined: 'Einladung abgelehnt', failed: 'Ausführung fehlgeschlagen',
} as const;
export const intentReasonLabels = {
  declined: 'Die andere Figur hat abgelehnt.', timeout: 'Das Zeitfenster ist abgelaufen.',
  unavailable: 'Die benötigte Sitzung oder ein Platz ist nicht mehr verfügbar.', operator: 'Durch deine Bedienung beendet.',
  restart: 'Die Sitzung wurde neu gestartet.', unsupported: 'Die Ausgabe unterstützt diese Aktivität nicht.',
  output_lost: 'Die geprüfte Ausgabe ist nicht mehr verfügbar.', interrupted: 'Die Ausführung wurde unterbrochen.',
  storage: 'Der lokale Zustand konnte nicht bestätigt werden.', protocol: 'Die Ausgabebestätigung war ungültig.',
  exhausted: 'Die native Ausführungsgrenze wurde erreicht.',
} as const;
const slotLabels = {rest:'Ziel: Ruheplatz',snack:'Ziel: Snackplatz',play:'Ziel: Spielplatz',meeting:'Ziel: Treffpunkt'} as const;
type Activity = keyof typeof intentActivityLabels;
type Phase = keyof typeof intentPhaseLabels;
type Reason = keyof typeof intentReasonLabels;
export type IntentTarget = {type:'here'|'point'|'actor'} | {type:'slot';anchorKind:keyof typeof slotLabels};
export interface RuntimeIntent {
  version:1; botId:string; sessionId:string; source:'action'|'social'; activity:Activity;
  phase:Phase; target:IntentTarget|null; reason:Reason|null; actorIds:string[];
}
const invalid = ():never => {throw Error('experience_invalid');};
function record(value:unknown):Record<string,unknown> {
  if(!value||typeof value!=='object'||Array.isArray(value))return invalid();
  const prototype=Object.getPrototypeOf(value);
  if(prototype!==Object.prototype&&prototype!==null)return invalid();
  if(Object.values(Object.getOwnPropertyDescriptors(value)).some(field=>!('value' in field)))return invalid();
  return value as Record<string,unknown>;
}
function id(value:unknown):string {return typeof value==='string'&&/^[A-Za-z0-9_-]{1,96}$/.test(value)?value:invalid();}
function member<T extends string>(value:unknown, values:readonly T[]):T {
  return typeof value==='string'&&values.includes(value as T)?value as T:invalid();
}
function target(value:unknown):IntentTarget|null {
  if(value===null)return null;
  const p=record(value),type=member(p.type,['here','point','slot','actor'] as const);
  return type==='slot'?{type,anchorKind:member(p.anchorKind,Object.keys(slotLabels) as (keyof typeof slotLabels)[])}:{type};
}
export function parseRuntimeIntent(value:unknown, botId:string):RuntimeIntent|null {
  // Older native fixtures have no projection; an album fact is never a substitute.
  if(value===undefined||value===null)return null;
  const p=record(value);
  if(p.version!==1||p.botId!==botId)return invalid();
  const source=member(p.source,['action','social'] as const);
  const activity=member(p.activity,Object.keys(intentActivityLabels) as Activity[]);
  const phase=member(p.phase,Object.keys(intentPhaseLabels) as Phase[]);
  const reason=p.reason===null?null:member(p.reason,Object.keys(intentReasonLabels) as Reason[]);
  if(!Array.isArray(p.actorIds)||p.actorIds.length<1||p.actorIds.length>2)return invalid();
  const descriptors=Object.getOwnPropertyDescriptors(p.actorIds);
  if(Object.values(descriptors).some(field=>!('value' in field)))return invalid();
  const actorIds=Array.from({length:p.actorIds.length},(_,index)=>id(descriptors[String(index)]?.value));
  if(new Set(actorIds).size!==actorIds.length||!actorIds.includes(botId))return invalid();
  const parsedTarget=target(p.target);
  if(source==='action') {
    if(actorIds.length!==1||activity==='greeting'||!parsedTarget||['invitation','accepted','resources_reserved','declined'].includes(phase))return invalid();
  } else {
    if(actorIds.length!==2||!['greeting','play','rest'].includes(activity)||['preparing','delivered','paused','cancelling','failed'].includes(phase))return invalid();
    if(reason!==null&&!['declined','timeout','unavailable','operator','restart'].includes(reason))return invalid();
  }
  if(reason!==null&&!['cancelled','declined','failed','cancelling'].includes(phase))return invalid();
  return {version:1,botId:id(p.botId),sessionId:id(p.sessionId),source,activity,phase,target:parsedTarget,reason,actorIds};
}
export function intentScopeKey(botId:string, sessionId:string|null):string {
  return JSON.stringify([botId,sessionId]);
}
interface IntentRead {readonly epoch:number;readonly scope:string}
/** One panel lifetime; every accepted event or newer read retires old responses. */
export class IntentSnapshotReads {
  private epoch=0;
  begin(botId:string, sessionId:string|null):IntentRead {
    return {epoch:++this.epoch,scope:intentScopeKey(botId,sessionId)};
  }
  invalidate():void {this.epoch++;}
  current(read:IntentRead, botId:string, sessionId:string|null):boolean {
    return read.epoch===this.epoch&&read.scope===intentScopeKey(botId,sessionId);
  }
}
export function acceptsIntentEvent(intent:RuntimeIntent|null|undefined, botId:string, sessionId:string|null):boolean {
  return !!sessionId&&intent?.botId===botId&&intent.sessionId===sessionId;
}
export function currentRuntimeIntent(intent:RuntimeIntent|null|undefined, botId:string, sessionId:string|null, active:boolean, paused:boolean):RuntimeIntent|null {
  return active&&!paused&&acceptsIntentEvent(intent,botId,sessionId)?intent!:null;
}
export function intentTargetLabel(target:IntentTarget):string {
  switch(target.type) {
    case 'slot': return slotLabels[target.anchorKind];
    case 'here': return 'Hier, ohne Objektplatz';
    case 'point': return 'Freier Zielpunkt';
    case 'actor': return 'Andere Figur als Ziel';
  }
}
