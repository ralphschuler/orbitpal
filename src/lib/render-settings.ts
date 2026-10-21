import {writable} from 'svelte/store';
import type {RenderQuality} from './avatar-render-policy';
export interface RenderSettings {version:1;botId:string;revision:number;quality:RenderQuality}
export function parseRenderSettings(value:unknown,botId?:string):RenderSettings {
  if(!value||typeof value!=='object'||Array.isArray(value))throw Error('render_settings_unavailable');
  const entry=value as Record<string,unknown>;
  if(entry.version!==1||typeof entry.botId!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(entry.botId)
    ||botId!==undefined&&entry.botId!==botId||!Number.isSafeInteger(entry.revision)||Number(entry.revision)<0
    ||entry.quality!=='balanced'&&entry.quality!=='energy')throw Error('render_settings_unavailable');
  return {version:1,botId:entry.botId,revision:Number(entry.revision),quality:entry.quality};
}
export class RenderSettingsVersions {
  private values=new Map<string,RenderSettings>();
  accept(value:unknown,botId?:string):RenderSettings|undefined {
    const next=parseRenderSettings(value,botId),previous=this.values.get(next.botId);
    if(previous&&(previous.revision>next.revision||previous.revision===next.revision&&previous.quality!==next.quality))return;
    this.values.set(next.botId,next);return next;
  }
}
export const renderSettingsByBot=writable<Record<string,RenderSettings>>({});
const versions=new RenderSettingsVersions();
export function acceptRenderSettings(value:unknown,botId?:string):RenderSettings|undefined {
  const snapshot=versions.accept(value,botId);if(snapshot)renderSettingsByBot.update(values=>({...values,[snapshot.botId]:snapshot}));return snapshot;
}
