// SPDX-License-Identifier: MIT
import { describe,expect,it,vi } from 'vitest';
import { HostCapabilityRequests, uiEmoteActions, type HostCapabilities } from './emote-catalogue';
import { avatarBinding } from './emote-capabilities';
const profile=(mode='obs')=>({id:'nova',avatar:{assetPath:'nova'},motions:[],output:{mode}});
const snapshot=(bot=profile()):HostCapabilities=>({botId:bot.id,avatarBinding:avatarBinding(bot),actions:[{action:'wave',name:'Winken',kind:'gesture'}]});
describe('Emote catalogue for external outputs',()=>{
  it.each(['overlay','obs'])('offers the native catalogue on initial %s startup without a mounted local preview',async(mode)=>{
    const bot=profile(mode);let host:HostCapabilities|undefined;const requests=new HostCapabilityRequests(value=>host=value);
    await requests.refresh(avatarBinding(bot),async()=>snapshot(bot));expect(uiEmoteActions(bot,undefined,host).map(action=>action.action)).toEqual(['wave']);
    expect(uiEmoteActions({...bot,output:{mode:'studio'}},undefined,host)).toEqual([]);
    // A stale local ready preview cannot override the host for a different bot.
    expect(uiEmoteActions(bot,{status:'ready',animations:[],proceduralGestures:[],actions:[{action:'phantom',name:'phantom',kind:'clip'}],avatarBinding:['other']},host)[0].action).toBe('wave');
  });
  it.each(['bot','asset','motion'])('ignores a late success and failure after a %s switch',async(change)=>{
    const bot=profile();let host:HostCapabilities|undefined;const requests=new HostCapabilityRequests(value=>host=value);
    let success!:(value:HostCapabilities)=>void;const first=requests.refresh(avatarBinding(bot),()=>new Promise(resolve=>success=resolve));
    const changed=structuredClone(bot);if(change==='bot')changed.id='other';else if(change==='asset')changed.avatar.assetPath='other.vrm';else (changed.motions as any[]).push({id:'motion.vrma'});
    requests.invalidate();await requests.refresh(avatarBinding(changed),async()=>snapshot(changed));success(snapshot(bot));await first;
    expect(host?.botId).toBe(changed.id);expect(JSON.stringify(host?.avatarBinding)).toBe(JSON.stringify(avatarBinding(changed)));
    expect(uiEmoteActions(bot,undefined,host)).toEqual([]);
    let reject!:(error:Error)=>void;const stale=requests.refresh(avatarBinding(bot),()=>new Promise((_,failed)=>reject=failed));requests.invalidate();await requests.refresh(avatarBinding(changed),async()=>snapshot(changed));reject(new Error('stale'));await expect(stale).resolves.toBeUndefined();expect(host?.botId).toBe(changed.id);
  });
  it('clears old availability before refresh and retains none after an error or mismatched response',async()=>{
    const bot=profile();let host:HostCapabilities|undefined;const requests=new HostCapabilityRequests(value=>host=value);
    await requests.refresh(avatarBinding(bot),async()=>snapshot(bot));expect(host).toBeDefined();
    const failure=requests.refresh(avatarBinding(bot),async()=>{throw new Error('unavailable');});expect(host).toBeUndefined();await expect(failure).rejects.toThrow('unavailable');expect(uiEmoteActions(bot,undefined,host)).toEqual([]);
    await requests.refresh(avatarBinding(bot),async()=>({...snapshot(bot),avatarBinding:['wrong']}));expect(host).toBeUndefined();
    const changed=vi.fn();const cancelled=new HostCapabilityRequests(changed);cancelled.invalidate();expect(changed).toHaveBeenLastCalledWith(undefined);
  });
});
