// SPDX-License-Identifier: MIT
import { avatarBinding, type AvailableAction } from './emote-capabilities';
import type { AvatarModelInfo } from './avatar';
export type HostCapabilities={botId:string;avatarBinding:unknown;actions:AvailableAction[]};
/** The editor can operate an external output before mounting a local preview.
 * Studio presentation still requires the loaded local renderer. */
export function uiEmoteActions(profile:any,model:AvatarModelInfo|undefined,host:HostCapabilities|undefined):AvailableAction[]{
  const binding=JSON.stringify(avatarBinding(profile));
  if(model?.status==='ready'&&JSON.stringify(model.avatarBinding)===binding)return model.actions??[];
  if(!['obs','overlay'].includes(profile.output?.mode))return [];
  if(!host||host.botId!==profile.id||JSON.stringify(host.avatarBinding)!==binding)return [];
  return host.actions;
}
/** Every refresh clears stale availability synchronously. Late success and
 * failure from another bot/asset/motion assignment cannot repopulate it. */
export class HostCapabilityRequests {
  private revision=0;
  constructor(private changed:(value:HostCapabilities|undefined)=>void){}
  invalidate(){++this.revision;this.changed(undefined);}
  async refresh(binding:unknown,request:()=>Promise<HostCapabilities>):Promise<void>{
    const revision=++this.revision;this.changed(undefined);
    try{
      const result=await request();
      if(revision!==this.revision)return;
      if(!Array.isArray(result.actions)||result.actions.length>4096||JSON.stringify(result.avatarBinding)!==JSON.stringify(binding)||!Array.isArray(binding)||result.botId!==binding[0])return;
      this.changed(result);
    }catch(error){if(revision===this.revision){this.changed(undefined);throw error;}}
  }
}
