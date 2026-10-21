import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {NativeWorldReadiness,parseNativeAssetProbe,parseNativeAvatarBinding} from './native-world-readiness';
import type {AvatarModelInfo} from './avatar';

// Controlled AvatarModelInfo callbacks and synthetic fact acknowledgements;
// no WebView/model load/render, OS, native Ready, provider or action proof.
const A='a'.repeat(64),B='b'.repeat(64);
const binding=(asset='nova',actor='nova')=>[actor,asset,[]];
const probe=(token=A,asset='nova')=>({version:1,probe:token,avatarBinding:binding(asset)});
const ready=(asset='nova'):AvatarModelInfo=>({status:'ready',avatarBinding:binding(asset),animations:[],proceduralGestures:[],format:'builtin'});
const loading:AvatarModelInfo={status:'loading',animations:[],proceduralGestures:[]};
const error:AvatarModelInfo={status:'error',animations:[],proceduralGestures:[]};
const ack=()=>({version:1,accepted:true});
function deferred<T>(){let resolve!:(value:T)=>void,reject!:(error?:unknown)=>void;const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};}
async function flush(){for(let i=0;i<16;i++)await Promise.resolve();}
function fixture(){
  let current:unknown=binding(),alive=true;
  const invoke=vi.fn(async(_command:string,_args:Record<string,unknown>):Promise<unknown>=>ack()),mount=vi.fn(()=> 'asset-mount');
  const readiness=new NativeWorldReadiness({invoke,binding:()=>current,current:()=>alive,mountId:mount,now:()=>Date.now()});
  const statuses=()=>invoke.mock.calls.map(([,args])=>(args.request as {status:string}).status);
  return{readiness,invoke,mount,statuses,setBinding:(value:unknown)=>{current=value;},retire:()=>{alive=false;}};
}
describe('native cold-asset readiness, synthetic fact transport',()=>{
  beforeEach(()=>{vi.useFakeTimers();vi.setSystemTime(1000);});afterEach(()=>vi.useRealTimers());
  it('accepts only the closed probe and bounded existing avatar binding without reading accessors',()=>{
    let reads=0;
    for(const raw of [{...probe(),actorId:'extra'},{...probe(),probe:A.toUpperCase()},{...probe(),version:2},
      {version:1,get probe(){reads++;return A;},avatarBinding:binding()},
      Object.assign(Object.create({scope:'inherited'}),probe()),{...probe(),[Symbol('extra')]:true}])expect(parseNativeAssetProbe(raw)).toBeNull();
    const accessor=binding();Object.defineProperty(accessor,'1',{get(){reads++;return'nova';}});
    expect(parseNativeAvatarBinding(accessor)).toBeNull();
    for(const value of [['nova','nova',Array(33).fill('motion')],['nova','x'.repeat(4097),[]],['../nova','nova',[]],['nova','nova'],['nova','nova',[{}]]])expect(parseNativeAvatarBinding(value)).toBeNull();
    expect(reads).toBe(0);expect(parseNativeAssetProbe(probe())).toEqual(probe());
  });
  it('reports loading immediately with only the closed metadata envelope, before any model callback',async()=>{
    const f=fixture();f.setBinding(null);f.readiness.probe(probe());
    expect(f.invoke).toHaveBeenCalledExactlyOnceWith('report_native_world_asset',{request:{version:1,probe:A,mountId:'asset-mount',status:'loading'}});
    await flush();expect(f.statuses()).toEqual(['loading']);f.readiness.dispose();
  });
  it('reports ready only after the current actual model-info binding also matches the captured callback binding',async()=>{
    const f=fixture();f.readiness.probe(probe());await flush();
    f.readiness.model(binding('other'),ready('other'));f.readiness.model(binding(),ready('other'));
    expect(f.statuses()).toEqual(['loading']);f.readiness.model(binding(),ready());await flush();
    expect(f.statuses()).toEqual(['loading','ready']);
    expect(f.invoke.mock.calls.every(([command])=>command==='report_native_world_asset')).toBe(true);f.readiness.dispose();
  });
  it('a genuinely cached ready model still waits for the first loading fact ACK and uses one mount',async()=>{
    const f=fixture(),held=deferred<unknown>();f.invoke.mockReturnValueOnce(held.promise);
    f.readiness.model(binding(),ready());f.readiness.probe(probe());f.readiness.probe(probe());await flush();
    expect(f.statuses()).toEqual(['loading']);expect(f.mount).toHaveBeenCalledTimes(1);
    held.resolve(ack());await flush();expect(f.statuses()).toEqual(['loading','ready']);
    expect(f.invoke.mock.calls.map(([,args])=>(args.request as {mountId:string}).mountId)).toEqual(['asset-mount','asset-mount']);f.readiness.dispose();
  });
  it('coalesces held-report observations to the newest status, without raw model error text',async()=>{
    const f=fixture(),held=deferred<unknown>();f.invoke.mockReturnValueOnce(held.promise);f.readiness.probe(probe());
    f.readiness.model(binding(),ready());f.readiness.model(binding(),loading);
    let reads=0;const failure={...error,get error(){reads++;throw new Error('private synthetic model message');}};
    f.readiness.model(binding(),failure);expect(f.statuses()).toEqual(['loading']);held.resolve(ack());await flush();
    expect(f.statuses()).toEqual(['loading','failed']);expect(reads).toBe(0);
    expect(JSON.stringify(f.invoke.mock.calls)).not.toContain('private');f.readiness.dispose();
  });
  it('a loading callback invalidates the prior ready cache instead of replaying it for a new probe',async()=>{
    const f=fixture();f.readiness.model(binding(),ready());f.readiness.model(binding(),loading);f.readiness.probe(probe());await flush();
    expect(f.statuses()).toEqual(['loading']);f.readiness.model(binding(),ready());await flush();
    expect(f.statuses()).toEqual(['loading','ready']);f.readiness.dispose();
  });
  it('same-probe and foreign-probe retries cannot restart an active mount or extend its loading deadline',async()=>{
    const f=fixture();f.readiness.probe(probe());await flush();
    await vi.advanceTimersByTimeAsync(59_999);f.readiness.probe(probe());f.readiness.probe(probe(B));f.readiness.model(binding(),loading);
    expect(f.statuses()).toEqual(['loading']);expect(f.mount).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);f.readiness.model(binding(),ready());await flush();
    expect(f.statuses()).toEqual(['loading']);f.readiness.dispose();
  });
  it('requires explicit host revoke before another probe and never repeats a terminal fact',async()=>{
    const f=fixture();f.readiness.model(binding(),ready());f.readiness.probe(probe());await flush();
    for(let i=0;i<20;i++){f.readiness.model(binding(),ready());f.readiness.probe(probe(B));}
    await flush();expect(f.statuses()).toEqual(['loading','ready']);
    f.readiness.revoke();f.readiness.probe(probe());expect(f.mount).toHaveBeenCalledTimes(1);
    f.readiness.probe(probe(B));await flush();expect(f.statuses()).toEqual(['loading','ready','loading','ready']);f.readiness.dispose();
  });
  it('an old held report cannot release facts for a revoked probe or overlap a new native report',async()=>{
    const f=fixture(),held=deferred<unknown>();f.invoke.mockReturnValueOnce(held.promise);f.readiness.probe(probe());
    f.readiness.model(binding(),ready());f.readiness.revoke();f.readiness.probe(probe(B));await flush();
    expect(f.statuses()).toEqual(['loading']);held.resolve(ack());await flush();
    expect(f.statuses()).toEqual(['loading','loading','ready']);
    expect(f.invoke.mock.calls.slice(1).every(([,args])=>(args.request as {probe:string}).probe===B)).toBe(true);f.readiness.dispose();
  });
  it('a blocked old native promise cannot pin a queued successor after its original five seconds',async()=>{
    const f=fixture(),held=deferred<unknown>();f.invoke.mockReturnValueOnce(held.promise);f.readiness.probe(probe());
    f.readiness.revoke();f.readiness.probe(probe(B));await vi.advanceTimersByTimeAsync(5000);
    held.resolve(ack());await flush();expect(f.statuses()).toEqual(['loading']);f.readiness.dispose();
  });
  it('a report timeout retains the actual single-flight slot and creates no hot retry or late ready',async()=>{
    const f=fixture(),held=deferred<unknown>();f.invoke.mockReturnValueOnce(held.promise);f.readiness.probe(probe());
    await vi.advanceTimersByTimeAsync(2001);
    for(let i=0;i<100;i++){f.readiness.model(binding(),ready());f.readiness.probe(probe());}
    expect(f.statuses()).toEqual(['loading']);held.resolve(ack());await flush();
    expect(f.statuses()).toEqual(['loading']);f.readiness.dispose();
  });
  it.each([null,{version:1,accepted:false},{version:1,accepted:true,ready:true},true])('a non-closed ACK blocks later facts rather than assuming acceptance: %j',async reply=>{
    const f=fixture();f.invoke.mockResolvedValueOnce(reply);f.readiness.probe(probe());await flush();
    f.readiness.model(binding(),ready());await flush();expect(f.statuses()).toEqual(['loading']);f.readiness.dispose();
  });
  it('rejected reports are terminal until host revoke and their error payload is never serialized',async()=>{
    const f=fixture();f.invoke.mockRejectedValueOnce(new Error('synthetic private backend detail'));f.readiness.probe(probe());await flush();
    f.readiness.model(binding(),ready());await flush();expect(f.statuses()).toEqual(['loading']);
    expect(JSON.stringify(f.invoke.mock.calls)).not.toContain('private');f.readiness.dispose();
  });
  it('profile change drops the old ready cache and rejects callbacks captured for a different asset',async()=>{
    const f=fixture();f.readiness.model(binding(),ready());f.readiness.profileChanged();f.setBinding(binding('new.glb'));
    f.readiness.probe(probe(B,'new.glb'));await flush();f.readiness.model(binding(),ready());f.readiness.model(binding(),error);await flush();
    expect(f.statuses()).toEqual(['loading']);f.readiness.model(binding('new.glb'),ready('new.glb'));await flush();
    expect(f.statuses()).toEqual(['loading','ready']);f.readiness.dispose();
  });
  it.each(['revoke','profile','dispose','document'] as const)('a held loading reply after %s cannot send its pending ready fact',async barrier=>{
    const f=fixture(),held=deferred<unknown>();f.invoke.mockReturnValueOnce(held.promise);f.readiness.probe(probe());f.readiness.model(binding(),ready());
    if(barrier==='revoke')f.readiness.revoke();else if(barrier==='profile')f.readiness.profileChanged();else if(barrier==='dispose')f.readiness.dispose();else f.retire();
    held.resolve(ack());await flush();expect(f.statuses()).toEqual(['loading']);f.readiness.dispose();
  });
  it('a new model loading while a ready report is held cannot queue another readiness fact from that old probe',async()=>{
    const f=fixture(),held=deferred<unknown>();f.readiness.probe(probe());await flush();
    f.invoke.mockReturnValueOnce(held.promise);f.readiness.model(binding(),ready());f.readiness.model(binding(),loading);
    held.resolve(ack());await flush();f.readiness.model(binding(),ready());await flush();
    expect(f.statuses()).toEqual(['loading','ready']);f.readiness.dispose();
  });
  it('does not invoke a malformed payload, a disposed document or an invalid mount factory',async()=>{
    const f=fixture();f.readiness.probe({...probe(),avatarBinding:binding('other') as unknown[],extra:1});
    f.readiness.dispose();f.readiness.probe(probe());await flush();expect(f.invoke).not.toHaveBeenCalled();
    const invoke=vi.fn();const invalid=new NativeWorldReadiness({invoke,binding:()=>binding(),current:()=>true,mountId:()=> '../mount'});
    invalid.probe(probe());expect(invoke).not.toHaveBeenCalled();invalid.dispose();
  });
});
