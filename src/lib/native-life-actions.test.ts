// SPDX-License-Identifier: MIT
import {describe,it,expect,vi} from 'vitest';
import {requestNativeLifeMove,requestNativeLifeSlot,requestQuietRestForOutput,type ActionsInvoke} from './life-actions';
import type {LifeUI} from './life-ui';

// Local protocol fixtures only. No constructor, native permit, renderer frame,
// actual arrival, provider or success of a native application is represented.
const intent='11111111-1111-4111-8111-111111111111';
const prepared=()=>({version:1,intentId:intent,expiresInMs:2000});
const result=(rest=false)=>({version:1,instanceId:'22222222-2222-4222-8222-222222222222',sequence:1,botId:'nova',availability:'ready',reason:null,visualAvailable:true,
  confirmed:{actionId:{actorId:'nova',worldId:'source_world',ordinal:1},activity:rest?'rest':'move_to',phase:rest?'started':'planned',cancellation:null,revision:1},
  runtime:{actionId:{actorId:'nova',worldId:'source_world',ordinal:1},presentation:rest?'native_rest_dwell':'visual',status:rest?'started':'awaiting_delivery',failure:null,activeMilliseconds:0}});
const reporter=()=>({reportProof:vi.fn(async(_blocked:boolean,_activity:boolean,dispatch:(proof:{connectionId:string;sequence:number})=>Promise<unknown>)=>dispatch({connectionId:'studio',sequence:1}))}) as unknown as Pick<LifeUI,'reportProof'>;

describe('explicit main native operator intent protocol, source authored UNRUN',()=>{
  it('native point movement consumes only the host-minted intent without a Studio renderer grant',async()=>{
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(prepared()).mockResolvedValueOnce(result());
    await requestNativeLifeMove(invoke,'nova',{x:250000,y:300000},()=>true);
    expect(invoke.mock.calls).toEqual([
      ['prepare_native_life_action',{botId:'nova'}],
      ['start_life_move',{botId:'nova',nativeIntentId:intent,target:{x:250000,y:300000}}],
    ]);
    const start=invoke.mock.calls[1][1]!;
    for(const field of ['ui','nativeSource','permit','owner','capability','actionId'])expect(start).not.toHaveProperty(field);
  });
  it('slot movement names a slot for final host resolution and supplies no caller-selected target',async()=>{
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(prepared()).mockResolvedValueOnce(result());
    await requestNativeLifeSlot(invoke,'nova','rest_slot',()=>true);
    expect(invoke.mock.calls[1]).toEqual(['start_life_move',{botId:'nova',nativeIntentId:intent,slotId:'rest_slot'}]);
    expect(invoke.mock.calls[1][1]).not.toHaveProperty('target');
  });
  it('a pending/missing native owner cannot silently fall back to a Studio movement request',async()=>{
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(null);
    await expect(requestNativeLifeMove(invoke,'nova',{x:250000,y:300000},()=>true)).rejects.toThrow();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it('a changed profile/operator lifetime after prepare prevents consumption and never retries',async()=>{
    let current=true;
    const invoke=vi.fn<ActionsInvoke>().mockImplementation(async()=>{current=false;return prepared();});
    await expect(requestNativeLifeMove(invoke,'nova',{x:250000,y:300000},()=>current)).rejects.toThrow();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it('a held start result cannot be committed to a successor view',async()=>{
    let current=true,release!:(value:unknown)=>void;
    const held=new Promise<unknown>(resolve=>release=resolve);
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(prepared()).mockImplementationOnce(()=>held);
    const request=requestNativeLifeMove(invoke,'nova',{x:250000,y:300000},()=>current);
    for(let i=0;i<8;i++)await Promise.resolve();current=false;release(result());
    expect(await request).toBeUndefined();expect(invoke).toHaveBeenCalledTimes(2);
  });
  it('rejects malformed/authority-shaped native intent responses without invoking accessors',async()=>{
    let reads=0;
    for(const value of [{...prepared(),permit:'forged'}, {...prepared(),expiresInMs:5000},
      {version:1,expiresInMs:2000,get intentId(){reads++;return intent;}},
      {...prepared(),intentId:'------------------------------------'}]){
      const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(value);
      await expect(requestNativeLifeSlot(invoke,'nova','rest_slot',()=>true)).rejects.toThrow();
      expect(invoke).toHaveBeenCalledTimes(1);
    }
    expect(reads).toBe(0);
  });
  it('native Here rest keeps the nonvisual dwell contract and does not request a renderer ACK',async()=>{
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(prepared()).mockResolvedValueOnce(result(true));
    const proof=reporter();await requestQuietRestForOutput(invoke,proof,'nova',()=>true);
    expect(proof.reportProof).not.toHaveBeenCalled();
    expect(invoke.mock.calls[1]).toEqual(['start_life_rest',{botId:'nova',nativeIntentId:intent}]);
  });
  it('ordinary Studio quiet rest retains its existing truthful UI reporter only after host reports no native owner',async()=>{
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(null).mockResolvedValueOnce(result(true));
    const proof=reporter();await requestQuietRestForOutput(invoke,proof,'nova',()=>true);
    expect(proof.reportProof).toHaveBeenCalledOnce();
    expect(invoke.mock.calls[1]).toEqual(['start_life_rest',{botId:'nova',ui:{connectionId:'studio',sequence:1}}]);
  });
  it('rejects a wrong actor/action result rather than treating an IPC return as native success',async()=>{
    const wrong=result();wrong.runtime.actionId.ordinal=2;
    const invoke=vi.fn<ActionsInvoke>().mockResolvedValueOnce(prepared()).mockResolvedValueOnce(wrong);
    await expect(requestNativeLifeMove(invoke,'nova',{x:250000,y:300000},()=>true)).rejects.toThrow();
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});
