export interface LifeOutput {
  version:1;botId:string;sequence:number;routeId:string|null;output:'overlay'|null;
  state:'opening'|'visible'|'unavailable'|'inactive';reason:null|'window_unavailable';
}
export function parseLifeOutput(value:unknown):LifeOutput {
  if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Die Desktop-Ausgabe konnte nicht bestätigt werden.');
  const item=value as LifeOutput;
  if(item.version!==1||typeof item.botId!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(item.botId)||!Number.isSafeInteger(item.sequence)||item.sequence<0||!['opening','visible','unavailable','inactive'].includes(item.state))throw new Error('Die Desktop-Ausgabe konnte nicht bestätigt werden.');
  if(item.state==='inactive'?(item.routeId!==null||item.output!==null||item.reason!==null):(typeof item.routeId!=='string'||!item.routeId.length||item.routeId.length>128||item.output!=='overlay'||(item.state==='unavailable'?item.reason!=='window_unavailable':item.reason!==null)))throw new Error('Die Desktop-Ausgabe konnte nicht bestätigt werden.');
  return {...item};
}
export class LifeOutputVersions {
  private latest=new Map<string,LifeOutput>();
  accept(value:unknown,botId?:string):LifeOutput|undefined {
    const next=parseLifeOutput(value);if(botId&&next.botId!==botId)return;
    const previous=this.latest.get(next.botId);
    if(previous&&next.sequence<=previous.sequence)return;
    this.latest.set(next.botId,next);return next;
  }
}
