export type DesktopLayout={active:boolean;referencePhysicalHeight?:number};
export type PresentationSnapshot={botId:string;revision:number;scale:number;desktop:DesktopLayout};
export type OutputDisplay={id:string;name:string;bounds:{x:number;y:number;width:number;height:number};scaleFactor:number};
export const validScale=(value:unknown):value is number=>typeof value==='number'&&Number.isFinite(value)&&value>=.3&&value<=2.5;
/** Profiles and initial responses may race a newer preview event. Render from
 * this separate snapshot rather than overwriting the persisted profile. */
export class PresentationVersions{
  private revisions=new Map<string,number>();
  accept(value:unknown,botId:string):value is PresentationSnapshot{
    const snapshot=value as PresentationSnapshot;
    if(!snapshot||snapshot.botId!==botId||!Number.isSafeInteger(snapshot.revision)||snapshot.revision<0||!validScale(snapshot.scale)||typeof snapshot.desktop?.active!=='boolean')return false;
    if(snapshot.desktop.active&&(!Number.isFinite(snapshot.desktop.referencePhysicalHeight)||snapshot.desktop.referencePhysicalHeight!<=0))return false;
    if(snapshot.revision<(this.revisions.get(botId)??-1))return false;
    this.revisions.set(botId,snapshot.revision);return true;
  }
}
export type OutputPreview={botId:string;field:'scale'|'monitor';value:number|string};
/** One bounded/coalesced stream; no profile, grants or other form drafts can
 * enter this channel. Explicit saving awaits the latest queued preview. */
export class OutputPreviewQueue{
  private pending=new Map<string,OutputPreview>();
  private running?:Promise<void>;
  constructor(private send:(preview:OutputPreview)=>Promise<unknown>,private failed:(error:unknown)=>void){}
  put(preview:OutputPreview){
    if(preview.field==='scale'&&!validScale(preview.value))return;
    this.pending.set(`${preview.botId}:${preview.field}`,preview);
    this.running??=Promise.resolve().then(()=>this.drain());
  }
  private async drain(){try{while(this.pending.size){const[key,value]=this.pending.entries().next().value!;this.pending.delete(key);try{await this.send(value);}catch(error){this.failed(error);}}}finally{this.running=undefined;}}
  async flush(){while(this.running)await this.running;}
}
