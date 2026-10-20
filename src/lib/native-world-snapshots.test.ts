import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {NativeWorldSnapshots,type NativeSnapshotScope} from './native-world-snapshots';

// Synthetic host reads/events prove ordering only: no Tauri, real native epoch,
// GUI, provider, Store, frame/Ready/action or playback acknowledgement here.
const A='11111111-1111-1111-1111-111111111111';
const B='22222222-2222-2222-2222-222222222222';
const C='33333333-3333-3333-3333-333333333333';
const scope=(instance=B,configRevision=1):NativeSnapshotScope=>({actorId:'nova',worldInstanceId:instance,stageId:'stage',configRevision});
const world=(instance=B,sequence=1,x=500_000,configRevision=1)=>({version:1,instanceId:instance,availability:'ready',reason:null,snapshot:{
  schemaVersion:1,stageId:'stage',configRevision,sequence,safeRegion:{min:{x:0,y:0},max:{x:1_000_000,y:1_000_000}},anchors:[],
  actors:[{actorId:'nova',position:{x,y:500_000},life:null}],occupied:[]}});
function deferred<T>(){let resolve!:(value:T)=>void,reject!:(value?:unknown)=>void;const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};}
async function flush(){for(let i=0;i<16;i++)await Promise.resolve();}
async function tick(ms=0){await vi.advanceTimersByTimeAsync(ms);await flush();}
function fixture(){
  let revision=1,allowed=true;
  const read=vi.fn(async(_actor:string):Promise<unknown>=>world()),mismatch=vi.fn();
  const snapshots=new NativeWorldSnapshots({read,mismatch,revision:()=>revision,current:()=>allowed,now:()=>Date.now()});
  const controller=new AbortController();
  return{snapshots,controller,read,mismatch,changeRevision:()=>revision++,unavailable:()=>{allowed=false;}};
}
describe('native World snapshot epoch and asynchronous fences, synthetic host',()=>{
  beforeEach(()=>{vi.useFakeTimers();vi.setSystemTime(1000);});afterEach(()=>vi.useRealTimers());
  it('does not guess an epoch from pre-scope events and reads only once for repeated genuine scope',async()=>{
    const f=fixture();f.snapshots.hint(world(A));f.snapshots.hint(world(C));await tick();
    expect(f.read).not.toHaveBeenCalled();expect(f.snapshots.position(scope())).toBeNull();
    f.snapshots.expect(scope(),f.controller.signal);f.snapshots.expect(scope(),f.controller.signal);await tick();
    expect(f.read).toHaveBeenCalledTimes(1);expect(f.read).toHaveBeenCalledWith('nova');
    expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});f.snapshots.dispose();
  });
  it('late A after confirmed B cannot replace its position or revoke B, and authoritative B recovers the hint',async()=>{
    const f=fixture();f.snapshots.expect(scope(),f.controller.signal);await tick();
    f.snapshots.hint(world(A,99,100_000));expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});
    expect(f.mismatch).not.toHaveBeenCalled();await tick(250);
    expect(f.read).toHaveBeenCalledTimes(2);expect(f.mismatch).not.toHaveBeenCalled();
    expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});f.snapshots.dispose();
  });
  it('matching events require a first authoritative confirmation and cannot be replaced by lower read sequence',async()=>{
    const f=fixture(),held=deferred<unknown>();f.read.mockReturnValueOnce(held.promise);
    f.snapshots.expect(scope(),f.controller.signal);await tick();f.snapshots.hint(world(B,20,600_000));
    expect(f.snapshots.position(scope())).toBeNull();held.resolve(world());await tick();
    f.snapshots.hint(world(B,20,600_000));f.snapshots.hint(world(B,19,100_000));
    expect(f.snapshots.position(scope())).toEqual({x:600_000,y:500_000});await tick(250);
    expect(f.snapshots.position(scope())).toEqual({x:600_000,y:500_000});f.snapshots.dispose();
  });
  it('coalesces unknown C during a held B read into one later authoritative B read without revocation',async()=>{
    const f=fixture(),held=deferred<unknown>();f.read.mockReturnValueOnce(held.promise);
    f.snapshots.expect(scope(),f.controller.signal);await tick();
    for(let i=0;i<100;i++)f.snapshots.hint(world(C,i+1,100_000));
    expect(f.read).toHaveBeenCalledTimes(1);held.resolve(world());await tick(250);
    expect(f.read).toHaveBeenCalledTimes(2);expect(f.mismatch).not.toHaveBeenCalled();
    expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});await tick(5000);
    expect(f.read).toHaveBeenCalledTimes(2);f.snapshots.dispose();
  });
  it('a genuinely authoritative C revokes only the current B lifetime and never installs a C grant/position',async()=>{
    const f=fixture();f.snapshots.expect(scope(),f.controller.signal);await tick();
    f.read.mockResolvedValueOnce(world(C));f.snapshots.hint(world(A));await tick(250);
    expect(f.mismatch).toHaveBeenCalledExactlyOnceWith(scope(),f.controller.signal);
    expect(f.snapshots.position(scope())).toBeNull();expect(f.snapshots.position(scope(C))).toBeNull();
    f.snapshots.hint(world(B));await tick(1000);expect(f.read).toHaveBeenCalledTimes(2);f.snapshots.dispose();
  });
  it('same instance with another stage/config and malformed/null events are hints, not destructive replacements',async()=>{
    const f=fixture();f.snapshots.expect(scope(),f.controller.signal);await tick();
    for(const value of [world(B,100,100_000,2),{...world(),snapshot:{...world().snapshot,stageId:'other'}},null,{unexpected:'opaque'}])f.snapshots.hint(value);
    expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});expect(f.mismatch).not.toHaveBeenCalled();
    await tick(250);expect(f.read).toHaveBeenCalledTimes(2);expect(f.mismatch).not.toHaveBeenCalled();f.snapshots.dispose();
  });
  it.each([null,{version:1,instanceId:B,availability:'unavailable',reason:'database',snapshot:null}])('a fresh current unavailable host read fails closed without a fabricated pose: %j',async value=>{
    const f=fixture();f.read.mockResolvedValueOnce(value);f.snapshots.expect(scope(),f.controller.signal);await tick();
    expect(f.mismatch).toHaveBeenCalledExactlyOnceWith(scope(),f.controller.signal);expect(f.snapshots.position(scope())).toBeNull();f.snapshots.dispose();
  });
  it('malformed and rejected reads cannot replace a confirmed B or manufacture a new epoch',async()=>{
    const f=fixture();f.snapshots.expect(scope(),f.controller.signal);await tick();
    f.read.mockResolvedValueOnce({unexpected:true});f.snapshots.hint(world(C));await tick(250);
    f.read.mockRejectedValueOnce(new Error('Synthetic unlogged error'));f.snapshots.hint(world(A));await tick(250);
    expect(f.mismatch).not.toHaveBeenCalled();expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});f.snapshots.dispose();
  });
  it.each(['abort','revision','route-unavailable','invalidate','dispose'] as const)('a held response after %s cannot mutate data, revoke, or schedule its old dirty follow-up',async barrier=>{
    const f=fixture(),held=deferred<unknown>();f.read.mockReturnValueOnce(held.promise);
    f.snapshots.expect(scope(),f.controller.signal);await tick();f.snapshots.hint(world(C));
    if(barrier==='abort')f.controller.abort();else if(barrier==='revision')f.changeRevision();else if(barrier==='route-unavailable')f.unavailable();
    else if(barrier==='invalidate')f.snapshots.invalidate();else f.snapshots.dispose();
    held.resolve(world(C));await tick(5000);expect(f.mismatch).not.toHaveBeenCalled();expect(f.snapshots.position(scope())).toBeNull();
    expect(f.read).toHaveBeenCalledTimes(1);f.snapshots.dispose();
  });
  it('late A read from an invalidated mount cannot replace or revoke a separately requested B successor',async()=>{
    const f=fixture(),held=deferred<unknown>();f.read.mockReturnValueOnce(held.promise);
    f.snapshots.expect(scope(A),f.controller.signal);await tick();f.controller.abort();
    const successor=new AbortController();f.snapshots.expect(scope(),successor.signal);await tick(500);
    expect(f.read).toHaveBeenCalledTimes(1);held.resolve(world(A));await tick();
    expect(f.read).toHaveBeenCalledTimes(2);expect(f.mismatch).not.toHaveBeenCalled();
    expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});f.snapshots.dispose();
  });
  it('local timeout retains the one actual IPC slot and late success cannot satisfy the expired read',async()=>{
    const f=fixture(),held=deferred<unknown>();f.read.mockReturnValueOnce(held.promise);
    f.snapshots.expect(scope(),f.controller.signal);await tick();await tick(2001);
    for(let i=0;i<100;i++){f.snapshots.hint(world(C));f.snapshots.expect(scope(),f.controller.signal);}
    await tick(5000);expect(f.read).toHaveBeenCalledTimes(1);expect(f.snapshots.position(scope())).toBeNull();
    held.resolve(world());await flush();expect(f.snapshots.position(scope())).toBeNull();await tick();
    expect(f.read).toHaveBeenCalledTimes(2);expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});f.snapshots.dispose();
  });
  it('bounds hint-driven native work to eight reads per actual generation, without resetting on same scope or events',async()=>{
    const f=fixture();f.snapshots.expect(scope(),f.controller.signal);await tick();
    for(let i=0;i<30;i++){f.snapshots.hint(world(A,i+1));f.snapshots.expect(scope(),f.controller.signal);await tick(250);}
    expect(f.read).toHaveBeenCalledTimes(8);expect(f.mismatch).not.toHaveBeenCalled();
    expect(f.snapshots.position(scope())).toEqual({x:500_000,y:500_000});f.snapshots.hint(world(B,99,600_000));
    expect(f.snapshots.position(scope())).toEqual({x:600_000,y:500_000});f.snapshots.dispose();
  });
  it('does not revive a disposed helper or issue reads for an aborted signal',async()=>{
    const f=fixture();f.controller.abort();f.snapshots.expect(scope(),f.controller.signal);await tick();expect(f.read).not.toHaveBeenCalled();
    f.snapshots.dispose();const next=new AbortController();f.snapshots.expect(scope(),next.signal);f.snapshots.hint(world());await tick(5000);
    expect(f.read).not.toHaveBeenCalled();expect(f.mismatch).not.toHaveBeenCalled();
  });
});
