import {describe,expect,it} from 'vitest';
import {nativeWorldObjectAnchors} from './native-world-objects';
import {parseWorldState} from './world-state';
import type {NativeWorldView} from './native-world-output';
import type {NativeObjectConfig} from './native-world-snapshots';
import type {NativeMotionBinding} from './world-output';

// Authored public values test display filtering only. No Tauri/native window,
// renderer frame, Ready factory, movement, provider or Store is exercised.
const instance='11111111-1111-1111-1111-111111111111';
function binding(actorId='blue'):NativeMotionBinding{
  const span=2*Math.hypot(.55,.7,7.6)*Math.tan(32*Math.PI/360),verticalSpan=span*1800/720;
  return {actorId,sourceWorldId:'lineage',renderSessionId:'render',bindingSequence:1,worldInstanceId:instance,stageId:'stage',configRevision:1,
    topologyRevision:1,layoutRevision:1,assetRevision:1,measurementRevision:1,referenceRevision:1,
    enclosureKind:'conservative_submitted_geometry_v1',cameraPolicy:'native_physical_iso_v1',
    nativeFraming:{kind:'native_physical_iso_v1',referencePhysicalHeight:720,physicalWidth:3200,physicalHeight:1800,scaleFactor:2,
      normalCamera:'full',normalVerticalSpan:span,verticalSpan},viewport:{width:1600,height:900},
    safeCenters:{left:180,top:160,right:1400,bottom:760},maximumFootprint:{width:100,height:160},
    envelopeExtents:{left:50,top:80,right:50,bottom:80},referenceLocal:{x:0,y:1.45,z:0},position:{x:800,y:450},
    groundBasis:{right:{x:verticalSpan/900,z:0},down:{x:0,z:verticalSpan/450}},modelForwardYaw:0,
    profileScale:1,effectiveScale:1,scale:1,reducedMotion:false,walkSupported:true, stationSupported: false};
}
function data(actorId='blue',members=['nova','blue']):NativeObjectConfig{
  const world=parseWorldState({version:1,instanceId:instance,availability:'ready',reason:null,snapshot:{schemaVersion:1,stageId:'stage',configRevision:1,sequence:1,
    safeRegion:{min:{x:100_000,y:100_000},max:{x:900_000,y:900_000}},
    anchors:[{id:'play',kind:'play',position:{x:250_000,y:750_000},slots:[{id:'slot',position:{x:250_000,y:750_000}}]}],
    actors:members.map(actorId=>({actorId,position:{x:500_000,y:500_000},life:null})),occupied:[]}});
  return {instanceId:world.instanceId,binding:binding(actorId),stageId:world.snapshot!.stageId,configRevision:world.snapshot!.configRevision,
    actors:world.snapshot!.actors.map(actor=>({actorId:actor.actorId})),anchors:world.snapshot!.anchors};
}
const view=(actorId='blue'):NativeWorldView=>({phase:'world',exposed:true,covered:false,binding:binding(actorId)});
const viewport={width:1600,height:900};
describe('native common World object display join',()=>{
  it('uses complete configured membership, including inactive members, and never fails over by readiness',()=>{
    const anchors=data().anchors;
    expect(nativeWorldObjectAnchors(view(),data(),viewport,true)).toEqual(anchors);
    expect(nativeWorldObjectAnchors(view('nova'),data('nova'),viewport,true)).toEqual([]);
    // The only supplied view may be Nova; absent Blue still owns the fixed role.
    expect(nativeWorldObjectAnchors(view('nova'),data('nova',['nova','blue']),viewport,true)).toEqual([]);
    expect(nativeWorldObjectAnchors(view('nova'),data('nova',['nova']),viewport,true)).toEqual(anchors);
  });
  it('requires a real-current ready-view join rather than a warm or exposed challenge',()=>{
    for(const next of [{...view(),phase:'normal' as const},{...view(),phase:'warming' as const},{...view(),exposed:false},
      {...view(),covered:true},{...view(),binding:undefined},{...view(),challenge:{} as NonNullable<NativeWorldView['challenge']>}])
      expect(nativeWorldObjectAnchors(next,data(),viewport,true)).toEqual([]);
    expect(nativeWorldObjectAnchors(view(),null,viewport,true)).toEqual([]);
    expect(nativeWorldObjectAnchors(view(),data(),viewport,false)).toEqual([]);
  });
  it('rejects a snapshot from another binding or World instance/stage/revision without retargeting it',()=>{
    for(const [key,value] of Object.entries({actorId:'nova',sourceWorldId:'other',renderSessionId:'new-render',bindingSequence:2,
      worldInstanceId:'22222222-2222-2222-2222-222222222222',stageId:'new-stage',configRevision:2,topologyRevision:2,
      layoutRevision:2,assetRevision:2,measurementRevision:2,referenceRevision:2})){
      const next={...data(),binding:{...data().binding,[key]:value}};
      expect(nativeWorldObjectAnchors(view(),next,viewport,true)).toEqual([]);
    }
    expect(nativeWorldObjectAnchors(view(),{...data(),instanceId:'22222222-2222-2222-2222-222222222222'},viewport,true)).toEqual([]);
    expect(nativeWorldObjectAnchors(view(),{...data(),configRevision:2},viewport,true)).toEqual([]);
  });
  it('requires the measured native CSS content viewport and preserves canonical anchors across safe-area/model changes',()=>{
    expect(nativeWorldObjectAnchors(view(),data(),{width:1599,height:900},true)).toEqual([]);
    expect(nativeWorldObjectAnchors(view(),data(),{width:3200,height:1800},true)).toEqual([]);
    const current=view(),before=structuredClone(current.binding);
    current.binding!.safeCenters={left:300,top:250,right:1100,bottom:650};
    current.binding!.envelopeExtents={left:120,top:100,right:160,bottom:140};
    const anchors=nativeWorldObjectAnchors(current,data(),viewport,true);
    expect(anchors[0].position).toEqual({x:250_000,y:750_000});
    // The CSS component uses 25%/75%: 400/675 CSS px at this same viewport.
    expect({x:anchors[0].position.x/1_000_000*viewport.width,y:anchors[0].position.y/1_000_000*viewport.height}).toEqual({x:400,y:675});
    expect(current.binding!.nativeFraming).toEqual(before!.nativeFraming);
  });
});
