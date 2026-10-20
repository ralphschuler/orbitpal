import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {NativeWorldOutput,parseNativeAuth,type NativeWorldPort,type NativeWorldView} from './native-world-output';
import {AvatarStage} from './avatar';
import {AvatarPresentationGate} from './avatar-presentation';
import {parseMeasurementChallenge,parseNativePhysicalPolicy,parseWorldMotionBinding,type MeasurementChallenge,type NativePhysicalPolicy,type NativeMotionBinding,type RegisteredWorldActor,type RenderMeasurement} from './world-output';

// Synthetic IPC acknowledgements exercise transport ordering only. No Tauri,
// window, WebGL, playback, provider or actual native authority is used here.
const cap='a'.repeat(64),other='b'.repeat(64);
const expected={actorId:'nova',sourceWorldId:'lineage',renderSessionId:'render',bindingSequence:1,worldInstanceId:'instance',stageId:'stage',
  configRevision:1,topologyRevision:1,layoutRevision:1,assetRevision:1,measurementRevision:1,referenceRevision:1};
const span=2*Math.hypot(.55,.7,7.6)*Math.tan(32*Math.PI/360);
const framing:NativePhysicalPolicy={kind:'native_physical_iso_v1',referencePhysicalHeight:400,physicalWidth:400,physicalHeight:400,
  scaleFactor:1,normalCamera:'full',normalVerticalSpan:span,verticalSpan:span};
const challenge=():MeasurementChallenge=>({id:'challenge',expiresInMs:5000,expected:{...expected},enclosureKind:'conservative_submitted_geometry_v1',
  cameraPolicy:'native_physical_iso_v1',nativeFraming:{...framing},profileScale:1,effectiveScale:1,expectedReferenceLocal:{x:0,y:1.45,z:0}});
const binding=():NativeMotionBinding=>({...expected,enclosureKind:'conservative_submitted_geometry_v1',cameraPolicy:'native_physical_iso_v1',nativeFraming:{...framing},
  viewport:{width:400,height:400},safeCenters:{left:20,top:30,right:380,bottom:370},maximumFootprint:{width:40,height:60},envelopeExtents:{left:20,top:30,right:20,bottom:30},
  referenceLocal:{x:0,y:1.45,z:0},position:{x:200,y:200},groundBasis:{right:{x:span/400,z:0},down:{x:0,z:span/200}},modelForwardYaw:0,
  profileScale:1,effectiveScale:1,scale:1,reducedMotion:false,walkSupported:true});
const output=(readiness:RegisteredWorldActor['readiness']={state:'awaiting_measurement',challenge:challenge()}):RegisteredWorldActor=>({
  version:1,renderSessionId:'render',bindingSequence:1,surfaceId:'surface',role:'owner_candidate',readiness});
const deferred=<T>()=>{let resolve!:(value:T)=>void;const promise=new Promise<T>(done=>resolve=done);return{promise,resolve};};
async function flush(){for(let i=0;i<18;i++)await Promise.resolve();}
function fixture(){
  const views:NativeWorldView[]=[],release=vi.fn();
  const adapter:NativeWorldPort={prepareNativeWorldWarm:vi.fn(async()=>({frameSequence:7,release})),acceptWorldCommand:vi.fn(()=> 'accepted' as const),
    beginWorldAfterDelivered:vi.fn(()=>true),confirmWorldStarted:vi.fn(()=>true),cancelWorld:vi.fn()};
  const invoke=vi.fn(async(_command:string,_args?:Record<string,unknown>):Promise<unknown>=>output());
  const normal=vi.fn(async()=>{}),afterDOM=vi.fn(async()=>{}),scope=vi.fn();
  const position=vi.fn<(_:typeof expected)=>{x:number;y:number}|null>(()=>({x:500_000,y:500_000}));
  const bridge=new NativeWorldOutput({invoke,adapter:()=>adapter,actorId:()=> 'nova',position,current:()=>true,
    normal,scope,changed:view=>views.push(view),afterDOM,now:()=>0,mountId:()=> 'mount'});
  return{bridge,adapter,views,invoke,normal,afterDOM,release,scope,position};
}
describe('private native document transport, synthetic host',()=>{
  beforeEach(()=>vi.useFakeTimers());afterEach(()=>vi.useRealTimers());
  it('accepts only a closed native challenge without invoking accessors',()=>{
    let reads=0;
    for(const raw of [{version:1,capability:cap,botId:'nova'},{version:1,capability:cap.toUpperCase()},
      {version:1,get capability(){reads++;return cap;}},Object.assign(Object.create({extra:true}),{version:1,capability:cap}),
      {version:1,capability:cap,[Symbol('extra')]:true}])expect(parseNativeAuth(raw)).toBeNull();
    expect(reads).toBe(0);expect(parseNativeAuth({version:1,capability:cap})).toEqual({version:1,capability:cap});
  });
  it('requires a new normal draw, releases its hold before registration and deduplicates retry delivery',async()=>{
    const f=fixture(),warm=deferred<{frameSequence:number;release:()=>void}>();
    vi.mocked(f.adapter.prepareNativeWorldWarm).mockReturnValue(warm.promise);
    f.bridge.bootstrap({version:1,capability:cap});f.bridge.bootstrap({version:1,capability:cap});await flush();
    expect(f.invoke).not.toHaveBeenCalled();expect(f.normal).toHaveBeenCalledTimes(1);
    warm.resolve({frameSequence:7,release:f.release});await flush();
    expect(f.release).toHaveBeenCalledTimes(1);expect(f.invoke).toHaveBeenCalledTimes(1);
    expect(f.release.mock.invocationCallOrder[0]).toBeLessThan(f.invoke.mock.invocationCallOrder[0]);
    expect(f.invoke).toHaveBeenCalledWith('register_native_world_output',{request:{version:1,capability:cap,mountId:'mount',normalWarmFrameSequence:7}});
    f.bridge.bootstrap({version:1,capability:other});await flush();expect(f.normal).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(f.views)).not.toContain(cap);f.bridge.dispose();
  });
  it('releases a late warm result after synchronous revoke and cannot register or revive the old capability',async()=>{
    const f=fixture(),warm=deferred<{frameSequence:number;release:()=>void}>();vi.mocked(f.adapter.prepareNativeWorldWarm).mockReturnValue(warm.promise);
    f.bridge.bootstrap({version:1,capability:cap});await flush();f.bridge.revoke();warm.resolve({frameSequence:8,release:f.release});await flush();
    expect(f.release).toHaveBeenCalledTimes(1);expect(f.invoke).not.toHaveBeenCalled();
    f.bridge.bootstrap({version:1,capability:cap});await flush();expect(f.normal).toHaveBeenCalledTimes(1);expect(f.views.at(-1)?.phase).toBe('normal');f.bridge.dispose();
  });
  it('polls awaiting layout with the same mount without claiming ready or reusing the warm frame as measurement',async()=>{
    const f=fixture();f.invoke.mockResolvedValueOnce(output({state:'awaiting_layout'}));
    f.bridge.bootstrap({version:1,capability:cap});await flush();expect(f.views.at(-1)?.phase).toBe('warming');
    await vi.advanceTimersByTimeAsync(100);await flush();
    expect(f.invoke.mock.calls[1]).toEqual(['register_native_world_output',{request:{version:1,capability:cap,mountId:'mount'}}]);
    expect(f.views.at(-1)).toMatchObject({phase:'world',covered:true,exposed:false});
    expect(f.invoke.mock.calls.every(([name])=>name==='register_native_world_output')).toBe(true);f.bridge.dispose();
  });
  it('removes its own cover before the DOM barrier and exposes only afterward; stale prepared callbacks cannot expose',async()=>{
    const f=fixture(),dom=deferred<void>();f.afterDOM.mockReturnValue(dom.promise);
    f.bridge.bootstrap({version:1,capability:cap});await flush();
    await f.bridge.prepared({...expected,challengeId:'other'});expect(f.afterDOM).not.toHaveBeenCalled();
    const pending=f.bridge.prepared({...expected,challengeId:'challenge'});expect(f.views.at(-1)).toMatchObject({covered:false,exposed:false});
    dom.resolve();await pending;expect(f.views.at(-1)).toMatchObject({covered:false,exposed:true});f.bridge.dispose();
  });
  it('ignores retired Stage failures during warm and a different binding cannot cancel the current native mount',async()=>{
    const f=fixture(),warm=deferred<{frameSequence:number;release:()=>void}>();vi.mocked(f.adapter.prepareNativeWorldWarm).mockReturnValue(warm.promise);
    f.bridge.bootstrap({version:1,capability:cap});await flush();
    f.bridge.unavailable({...expected,renderSessionId:'retired',challengeId:'old'},'cancelled');expect(f.bridge.active).toBe(true);
    warm.resolve({frameSequence:7,release:f.release});await flush();
    f.bridge.unavailable({...expected,renderSessionId:'retired',challengeId:'old'},'cancelled');expect(f.bridge.active).toBe(true);
    f.bridge.unavailable({...expected,challengeId:'challenge'},'context_lost');expect(f.bridge.active).toBe(false);f.bridge.dispose();
  });
  it('does not accept metadata-ready or a Studio policy as a fresh native document grant',async()=>{
    for(const raw of [output({state:'ready',binding:binding()}),output({state:'awaiting_measurement',challenge:{...challenge(),cameraPolicy:'world_iso_v1',nativeFraming:undefined}})]){
      const f=fixture();f.invoke.mockResolvedValue(raw);f.bridge.bootstrap({version:1,capability:cap});await flush();
      expect(f.views.at(-1)?.phase).toBe('normal');expect(f.views.some(view=>view.phase==='world')).toBe(false);f.bridge.dispose();
    }
  });
  it('unregisters only the returned exact late identity after a registration timeout',async()=>{
    const f=fixture(),reply=deferred<unknown>();f.invoke.mockImplementationOnce(()=>reply.promise);
    f.bridge.bootstrap({version:1,capability:cap});await flush();await vi.advanceTimersByTimeAsync(2001);await flush();
    reply.resolve(output());await flush();
    expect(f.views.at(-1)?.phase).toBe('normal');
    expect(f.invoke).toHaveBeenCalledWith('unregister_native_world_output',{request:{version:1,capability:cap,renderSessionId:'render',bindingSequence:1}});f.bridge.dispose();
  });
  it('wraps real bridge observations in native auth and cannot release delivery after a stale held ACK',async()=>{
    const f=fixture(),held=deferred<unknown>(),ready=binding();
    f.invoke.mockImplementation(async(command,_args)=>{
      if(command==='register_native_world_output')return output();
      if(command==='report_native_world_measurement')return{version:1,frameSequence:1,output:output({state:'ready',binding:ready})};
      if(command==='report_native_world_action')return held.promise;
      if(command==='unregister_native_world_output')return{};
      throw new Error('Unexpected synthetic fixture command');
    });
    f.bridge.bootstrap({version:1,capability:cap});await flush();
    await f.bridge.prepared({...expected,challengeId:'challenge'});
    const measurement:RenderMeasurement={version:1,challengeId:'challenge',frameSequence:1,enclosureKind:'conservative_submitted_geometry_v1',
      viewport:ready.viewport,clip:{left:0,top:0,right:400,bottom:400},anchor:ready.position,
      posedBounds:{left:180,top:170,right:220,bottom:230},referenceLocal:ready.referenceLocal,envelopeExtents:ready.envelopeExtents,
      groundBasis:ready.groundBasis,modelForwardYaw:0,appliedModelScale:1,reducedMotion:false,walkSupported:true};
    f.bridge.measurement(measurement);await flush();
    expect(f.views.at(-1)?.binding?.referenceLocal).toEqual({x:0,y:1.45,z:0});
    const {sourceWorldId,...stamp}=expected;
    const fence={...stamp,actionId:{actorId:'nova',worldId:sourceWorldId,ordinal:1},deliveryId:'delivery'};
    f.bridge.receive({version:1,type:'offer',sequence:1,fence,activity:'move_to',from:ready.position,target:{x:220,y:200},maximumSpeed:100,acceleration:500,validForMs:5000});
    f.bridge.frame({...measurement,fence,frameSequence:2,walking:false,targetIdleFrames:0,purpose:'delivery'});
    await flush();expect(f.invoke.mock.calls.some(([command])=>command==='report_native_world_action')).toBe(true);
    f.bridge.revoke();held.resolve({version:1,receiptSequence:1,outcome:'accepted',runtimeStatus:'delivered',confirmedPhase:'planned',reason:null});await flush();
    expect(f.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled();
    const action=f.invoke.mock.calls.find(([command])=>command==='report_native_world_action');
    expect(action?.[1]).toMatchObject({request:{version:1,capability:cap,report:{kind:'delivered',receiptSequence:1}}});
    expect(JSON.stringify(f.views)).not.toContain(cap);f.bridge.dispose();
  });
  it('keeps the original warm/awaiting-layout deadline and never promotes a five-second poll loop',async()=>{
    const f=fixture();f.invoke.mockResolvedValue(output({state:'awaiting_layout'}));f.bridge.bootstrap({version:1,capability:cap});await flush();
    await vi.advanceTimersByTimeAsync(5001);await flush();expect(f.views.at(-1)?.phase).toBe('normal');
    const count=f.invoke.mock.calls.length;await vi.advanceTimersByTimeAsync(10_000);expect(f.invoke).toHaveBeenCalledTimes(count);f.bridge.dispose();
  });
  it('publishes the parsed native epoch before pose lookup so a stale cache cannot prevent reconciliation',async()=>{
    const f=fixture();f.position.mockReturnValue(null);
    f.bridge.bootstrap({version:1,capability:cap});await flush();
    expect(f.scope).toHaveBeenCalledWith(expected,expect.any(AbortSignal));
    expect(f.scope.mock.invocationCallOrder[0]).toBeLessThan(f.position.mock.invocationCallOrder[0]);
    expect(f.views.at(-1)?.phase).toBe('warming');
    expect(f.invoke.mock.calls.every(([name])=>name==='register_native_world_output')).toBe(true);
    const lifetime=f.scope.mock.calls[0][1] as AbortSignal;
    f.position.mockReturnValue({x:500_000,y:500_000});await vi.advanceTimersByTimeAsync(100);await flush();
    expect(f.views.at(-1)).toMatchObject({phase:'world',covered:true,exposed:false});
    expect(f.scope.mock.calls.every(call=>call[1]===lifetime)).toBe(true);
    f.bridge.revoke();expect(lifetime.aborted).toBe(true);f.bridge.dispose();
  });
  it('does not expose a wrong policy or metadata Ready as a snapshot epoch authority',async()=>{
    for(const raw of [output({state:'ready',binding:binding()}),output({state:'awaiting_measurement',challenge:{...challenge(),cameraPolicy:'world_iso_v1',nativeFraming:undefined}})]){
      const f=fixture();f.invoke.mockResolvedValue(raw);f.bridge.bootstrap({version:1,capability:cap});await flush();
      expect(f.scope).not.toHaveBeenCalled();expect(f.position).not.toHaveBeenCalled();f.bridge.dispose();
    }
  });
  it('a synchronous scope-lifetime cancellation cannot continue into position or bridge setup',async()=>{
    const f=fixture();f.scope.mockImplementationOnce(()=>f.bridge.revoke());
    f.bridge.bootstrap({version:1,capability:cap});await flush();
    expect(f.scope).toHaveBeenCalledTimes(1);expect(f.position).not.toHaveBeenCalled();
    expect(f.views.at(-1)?.phase).toBe('normal');f.bridge.dispose();
  });
});

describe('native physical policy parser',()=>{
  it('matches native DPI/dimension bounds and uses the independently derived span tolerance',()=>{
    for(const scaleFactor of [.25,1,2,8])expect(parseNativePhysicalPolicy({...framing,scaleFactor}).scaleFactor).toBe(scaleFactor);
    for(const bad of [{...framing,scaleFactor:.249},{...framing,scaleFactor:8.001},{...framing,scaleFactor:Infinity},
      {...framing,referencePhysicalHeight:0},{...framing,physicalWidth:1_000_001},{...framing,physicalHeight:1.5},
      {...framing,verticalSpan:span+2e-8}])expect(()=>parseNativePhysicalPolicy(bad)).toThrow();
    // Rust validates both spans against its fixed camera constant, rather than
    // multiplying tolerated normal-span noise into an expanded viewport.
    const enlarged={...framing,referencePhysicalHeight:1,physicalHeight:100_000,normalVerticalSpan:span+5e-9,verticalSpan:span*100_000};
    expect(parseNativePhysicalPolicy(enlarged).verticalSpan).toBe(span*100_000);
  });
  it('preserves exact old Studio shapes and accepts only the closed native pair',()=>{
    const native=binding();expect(parseWorldMotionBinding(native)?.nativeFraming).toEqual(framing);
    const {cameraPolicy:_camera,nativeFraming:_framing,...studio}=native;
    expect(parseWorldMotionBinding(studio)).not.toBeNull();
    for(const value of [{...studio,cameraPolicy:'world_iso_v1'},{...studio,nativeFraming:framing},{...studio,cameraPolicy:'native_physical_iso_v1'},
      {...native,nativeFraming:{...framing,verticalSpan:framing.verticalSpan*.5}},{...native,nativeFraming:{...framing,extra:true}},
      {...native,referenceLocal:{x:0,y:0,z:0}},{...native,scale:.9},{...native,groundBasis:{right:{x:1,z:0},down:{x:0,z:2}}}])expect(parseWorldMotionBinding(value)).toBeNull();
    const parsed=parseMeasurementChallenge(challenge());expect(Object.isFrozen(parsed.nativeFraming)).toBe(true);
    expect(()=>parseMeasurementChallenge({...challenge(),nativeFraming:undefined})).toThrow();
    expect(()=>parseMeasurementChallenge({...challenge(),expectedReferenceLocal:null})).toThrow();
  });
});

describe('normal render warm observation without GPU',()=>{
  function warmStage(){
    const stage=Object.create(AvatarStage.prototype) as any;
    Object.assign(stage,{options:{motionSource:'fixed',audioEnabled:false,speaking:false},presentationContext:'normal',normalWarmFrameSequence:0,
      disposed:false,presentation:new AvatarPresentationGate()});
    return stage as {prepareNativeWorldWarm:AvatarStage['prepareNativeWorldWarm'];presentation:AvatarPresentationGate;normalWarmFrameSequence:number;options:any};
  }
  it('a previous/load-ready frame cannot satisfy a new warm and the held fresh result is released explicitly',async()=>{
    const stage=warmStage();stage.presentation.rendered(true);
    let done=false;const warm=stage.prepareNativeWorldWarm(new AbortController().signal).then(value=>{done=true;return value;});
    await Promise.resolve();expect(done).toBe(false);
    // Controlled actual-render seam only; this unit does not claim WebGL.
    stage.normalWarmFrameSequence=1;stage.presentation.rendered(true);const value=await warm;
    expect(value.frameSequence).toBe(1);expect(stage.presentation.requested).toBe(true);value.release();expect(stage.presentation.requested).toBe(false);
  });
  it('a World or audible context never acquires a normal warm lease',async()=>{
    for(const options of [{motionSource:'world'},{motionSource:'fixed',audioEnabled:true},{motionSource:'fixed',worldChallenge:challenge()}]){
      const stage=warmStage();stage.options=options;await expect(stage.prepareNativeWorldWarm(new AbortController().signal)).rejects.toMatchObject({name:'AbortError'});
      expect(stage.presentation.requested).toBe(false);
    }
  });
});
