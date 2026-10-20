/** Main-window API connection metadata. Secret values are write-only arguments;
 * they are never part of these snapshot, selector or profile types. */
export const apiServices = ['openai','anthropic','elevenlabs','hindsight','compatible','youtube_data'] as const;
export const apiAuthStyles = ['bearer','anthropic','elevenlabs','youtube_api_key'] as const;
export const apiPurposes = ['dialog','model_catalogue','speech','transcription','hindsight','youtube_data'] as const;
export type ApiService = typeof apiServices[number];
export type ApiAuthStyle = typeof apiAuthStyles[number];
export type ApiPurpose = typeof apiPurposes[number];
export type ApiScope = Readonly<{service:ApiService;destination:string;authStyle:ApiAuthStyle}>;
export type ApiConnection = ApiScope & Readonly<{id:string;label:string;keyStatus:'registered'|'missing';source:'legacy'|'operator';
  writePolicy:'replaceable'|'shared_legacy_read_only';sharedConnectionCount:number;externalReferences:boolean}>;
export type ApiDefault = ApiScope & Readonly<{id:string;connectionId:string|null}>;
export type ApiSnapshot = Readonly<{version:1;availability:'ready'|'unavailable';reason:null|'database'|'unsupported_schema'|'corrupt_registry'|'migration_incomplete';
  revision:number|null;connections:readonly ApiConnection[];defaults:readonly ApiDefault[];legacyUnboundCount:number}>;
export type ApiBinding = Readonly<{binding:ApiScope|null;unused:boolean;selectedRef:string|null;defaultRef:string|null;
  defaultConnectionId:string|null;compatibleIds:readonly string[];status:'unused'|'unbound'|'default'|'named'|'invalid';
  reason:null|'invalid_destination'|'invalid_reference'|'unknown_connection'|'scope_mismatch'|'default_missing'|'registry_unavailable'|'missing_secret'}>;
export type ApiRequest = (command:string,args?:Record<string,unknown>)=>Promise<unknown>;
export type NewApiConnection = ApiScope & Readonly<{label:string;existingSecretId?:string}>;
const limit=4096;
const invalid=()=>new Error('Die API-Zugänge konnten nicht sicher gelesen werden. Lade den Status erneut.');
const id=(value:unknown):value is string=>typeof value==='string'&&/^[A-Za-z0-9_-]{1,96}$/.test(value);
const integer=(value:unknown,min=0):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=min;
const bytes=(value:string)=>new TextEncoder().encode(value).length;
function record(value:unknown,keys:readonly string[]):Record<string,unknown>{
  if(!value||typeof value!=='object'||Array.isArray(value))throw invalid();
  const proto=Object.getPrototypeOf(value),data=Object.getOwnPropertyDescriptors(value);
  if(proto!==Object.prototype&&proto!==null||Reflect.ownKeys(data).length!==keys.length||keys.some(key=>!Object.hasOwn(data,key)||!Object.hasOwn(data[key],'value')))throw invalid();
  return Object.fromEntries(keys.map(key=>[key,data[key].value]));
}
function choice<T extends string>(value:unknown,values:readonly T[]):T{if(typeof value!=='string'||!values.includes(value as T))throw invalid();return value as T;}
function reference(value:unknown):string|null{if(value===null)return null;if(!id(value))throw invalid();return value;}
function scope(value:Record<string,unknown>):ApiScope{
  const destination=value.destination;
  if(typeof destination!=='string'||bytes(destination)>4096||/[\u0000-\u0020\u007f]/.test(destination))throw invalid();
  try{const url=new URL(destination);if(!['http:','https:'].includes(url.protocol)||url.username||url.password||url.search||url.hash)throw invalid();}catch{throw invalid();}
  return Object.freeze({service:choice(value.service,apiServices),destination,authStyle:choice(value.authStyle,apiAuthStyles)});
}
export const sameApiScope=(a:ApiScope,b:ApiScope)=>a.service===b.service&&a.destination===b.destination&&a.authStyle===b.authStyle;
function bounded<T>(value:unknown,parse:(value:unknown)=>T):readonly T[]{if(!Array.isArray(value)||value.length>limit)throw invalid();return Object.freeze(value.map(parse));}
function connection(value:unknown):ApiConnection{
  const row=record(value,['id','label','service','destination','authStyle','keyStatus','source','writePolicy','sharedConnectionCount','externalReferences']);
  if(!id(row.id)||typeof row.label!=='string'||!row.label.trim()||bytes(row.label)>120||/[\u0000-\u001f\u007f]/.test(row.label)
    ||!integer(row.sharedConnectionCount,1)||row.sharedConnectionCount>limit||typeof row.externalReferences!=='boolean')throw invalid();
  return Object.freeze({...scope(row),id:row.id,label:row.label,keyStatus:choice(row.keyStatus,['registered','missing']),source:choice(row.source,['legacy','operator']),
    writePolicy:choice(row.writePolicy,['replaceable','shared_legacy_read_only']),sharedConnectionCount:row.sharedConnectionCount,externalReferences:row.externalReferences});
}
function defaultEntry(value:unknown):ApiDefault{const row=record(value,['id','service','destination','authStyle','connectionId']);if(!id(row.id))throw invalid();return Object.freeze({...scope(row),id:row.id,connectionId:reference(row.connectionId)});}
export function parseApiSnapshot(value:unknown):ApiSnapshot{
  const row=record(value,['version','availability','reason','revision','connections','defaults','legacyUnboundCount']);
  if(row.version!==1||!integer(row.legacyUnboundCount))throw invalid();
  const availability=choice(row.availability,['ready','unavailable']),reason=row.reason===null?null:choice(row.reason,['database','unsupported_schema','corrupt_registry','migration_incomplete']);
  if(availability==='ready'&&(!integer(row.revision)||reason!==null)||availability==='unavailable'&&(row.revision!==null||reason===null))throw invalid();
  const connections=bounded(row.connections,connection),defaults=bounded(row.defaults,defaultEntry);
  if(availability==='unavailable'&&(connections.length||defaults.length))throw invalid();
  const ids=[...connections,...defaults].map(item=>item.id);if(new Set(ids).size!==ids.length)throw invalid();
  const scopes=defaults.map(item=>JSON.stringify([item.service,item.destination,item.authStyle]));if(new Set(scopes).size!==scopes.length)throw invalid();
  const byId=new Map(connections.map(item=>[item.id,item]));
  for(const item of defaults)if(item.connectionId){const target=byId.get(item.connectionId);if(!target||!sameApiScope(item,target))throw invalid();}
  return Object.freeze({version:1,availability,reason,revision:row.revision as number|null,connections,defaults,legacyUnboundCount:row.legacyUnboundCount});
}
export function parseApiBinding(value:unknown):ApiBinding{
  const row=record(value,['binding','unused','selectedRef','defaultRef','defaultConnectionId','compatibleIds','status','reason']);
  if(typeof row.unused!=='boolean')throw invalid();
  const binding=row.binding===null?null:scope(record(row.binding,['service','destination','authStyle']));
  const compatibleIds=bounded(row.compatibleIds,value=>{if(!id(value))throw invalid();return value;});
  if(new Set(compatibleIds).size!==compatibleIds.length)throw invalid();
  return Object.freeze({binding,unused:row.unused,selectedRef:reference(row.selectedRef),defaultRef:reference(row.defaultRef),defaultConnectionId:reference(row.defaultConnectionId),compatibleIds,
    status:choice(row.status,['unused','unbound','default','named','invalid']),reason:row.reason===null?null:choice(row.reason,['invalid_destination','invalid_reference','unknown_connection','scope_mismatch','default_missing','registry_unavailable','missing_secret'])});
}
/** Fixed fallback: arbitrary native exception text must never echo a write value. */
export const safeApiDiagnostics:readonly string[]=Object.freeze(["Globale API-Zugänge werden ausschließlich im Studio verwaltet.", "Globale API-Zugänge werden ausschließlich für diese Installation verwaltet.", "Der API-Zugang passt nicht zu Anbieter und Zieladresse. Bitte einen passenden globalen Zugang auswählen.", "Die globalen API-Zugänge sind nicht verfügbar. Bitte die Einstellungen prüfen.", "Die API-Zugänge wurden inzwischen geändert. Bitte neu laden.", "Dieser alte Schlüssel wird von mehreren Zugängen oder anderen Integrationen verwendet. Bitte einen unabhängigen API-Zugang mit eigenem Schlüssel anlegen.", "API-Zugänge benötigen einen Namen mit maximal 120 Bytes ohne Steuerzeichen.", "Der vorhandene Schlüsselbund-Verweis ist nicht registriert.", "Ein API-Schlüssel muss zwischen 1 und 65536 Bytes enthalten.", "Der API-Schlüssel konnte nicht im sicheren Systemspeicher gespeichert werden.", "Der API-Schlüssel konnte nicht aus dem sicheren Systemspeicher gelesen werden.", "Der globale API-Zugang enthält keinen registrierten Schlüssel.", "Der globale API-Zugang enthält keinen verfügbaren Schlüssel."]);
export function apiAccessError(cause:unknown):string{
  const message=typeof cause==='string'?cause:cause instanceof Error?cause.message:'';
  return safeApiDiagnostics.includes(message)?message:'Die API-Zugänge konnten nicht aktualisiert werden. Prüfe den Status und versuche es erneut.';
}
export async function getApiConnections(request:ApiRequest):Promise<ApiSnapshot>{return parseApiSnapshot(await request('get_api_connections'));}
export async function createApiConnection(request:ApiRequest,expectedRevision:number,definition:NewApiConnection){
  const row=record(await request('create_api_connection',{expectedRevision,connection:definition}),['snapshot','connectionId']);
  if(!id(row.connectionId))throw invalid();const snapshot=parseApiSnapshot(row.snapshot);
  if(snapshot.availability!=='ready'||!snapshot.connections.some(c=>c.id===row.connectionId))throw invalid();
  return {snapshot,connectionId:row.connectionId};
}
export async function mutateApiConnection(request:ApiRequest,command:'rename_api_connection'|'set_default_api_connection'|'remove_api_connection'|'set_api_connection_secret',args:Record<string,unknown>):Promise<ApiSnapshot>{
  return parseApiSnapshot(await request(command,args));
}
/** Only the latest exact draft may receive native scope/default suggestions. */
export class ApiBindingReads{
  private sequence=0;
  invalidate(){++this.sequence;}
  async load(request:ApiRequest,purpose:ApiPurpose,config:Record<string,unknown>,current:()=>boolean):Promise<ApiBinding|null>{
    const sequence=++this.sequence;
    const result=parseApiBinding(await request('get_api_binding',{purpose,config}));
    return sequence===this.sequence&&current()?result:null;
  }
}
