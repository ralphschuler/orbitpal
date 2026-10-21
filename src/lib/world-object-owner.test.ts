// SPDX-License-Identifier: MIT
import {describe,it,expect} from 'vitest';
import {designatedWorldObjectActor,worldObjectAnchorsForActor} from './world-object-owner';
import {WorldSemanticMirrors} from './world-semantic-mirrors';
import type {WorldSnapshot,WorldState} from './world-state';

// Authored public DTO data only; no native owner, Ready, receipt or ACK factory.
function world():WorldState{
  return {version:1,instanceId:'world_a',availability:'ready',reason:null,snapshot:{
    schemaVersion:1,stageId:'stage',configRevision:1,sequence:1,
    safeRegion:{min:{x:0,y:0},max:{x:1000000,y:1000000}},
    anchors:[{id:'rest',kind:'rest',position:{x:500000,y:500000},slots:[{id:'slot',position:{x:500000,y:500000}}]}],
    actors:['a','b'].map(actorId=>({actorId,position:{x:500000,y:500000},life:{
      schemaVersion:1 as const,actorId,worldId:`source_${actorId}`,revision:1,simulationTime:0,activity:null}})),occupied:[]}};
}
function packet(actorId:string,sequence:number,visible=true){
  return {version:1,epoch:'host_epoch',sequence,objectOwnerActorId:actorId,owners:[{
    actorId,controlled:true,owner:{renderSessionId:`owner_${actorId}`,bindingSequence:1,surfaceId:`surface_${actorId}`},
    worldInstanceId:'world_a',sourceWorldId:`source_${actorId}`,stageId:'stage',configRevision:1,visible}]};
}
// Exercise the same production filter used by Overlay, after the real registry
// has applied its freshness and lineage checks. Studio keeps its common layer.
function obsAnchors(registry:WorldSemanticMirrors,actorId:string,state:WorldState){
  const view=registry.view(actorId,state);
  return view&&state.snapshot?worldObjectAnchorsForActor(state.snapshot,actorId,view.anchors):[];
}

describe('configured World object display role',()=>{
  it('uses all configured members, independent of order, activity or readiness',()=>{
    const snapshot=world().snapshot!;
    snapshot.actors.reverse();snapshot.actors.find(actor=>actor.actorId==='a')!.life=null;
    const before=structuredClone(snapshot);
    expect(designatedWorldObjectActor(snapshot)).toBe('a');expect(snapshot).toEqual(before);
    expect(designatedWorldObjectActor({...snapshot,actors:[]})).toBeNull();
  });
  it('prevents same-scope OBS readiness handoff while the previous feed still holds a fresh positive packet',()=>{
    const state=world(),a=new WorldSemanticMirrors(()=>0),b=new WorldSemanticMirrors(()=>0);
    // B is visible first; its real transport would receive the legacy readiness
    // election. Its configured role still forbids the common object layer.
    b.seed(packet('b',1));expect(b.view('b',state)?.anchors).toHaveLength(1);
    expect(obsAnchors(b,'b',state)).toHaveLength(0);
    // A later becomes visible in the SAME World instance/stage/config revision.
    // B receives no newer packet, so both positive packets are still fresh.
    a.seed(packet('a',2));expect(obsAnchors(a,'a',state)).toHaveLength(1);
    expect(obsAnchors(b,'b',state)).toHaveLength(0);
    expect(b.view('b',state)?.anchors).toHaveLength(1);
  });
  it('does not give a sole non-designated OBS feed automatic objects fallback',()=>{
    const state=world(),b=new WorldSemanticMirrors(()=>0);
    state.snapshot!.actors[0].life=null;b.seed(packet('b',1));
    expect(b.view('b',state)).not.toBeNull();expect(obsAnchors(b,'b',state)).toHaveLength(0);
  });
  it('requires the designated feed own fresh visible scope as well as its fixed role',()=>{
    let now=0;const state=world(),a=new WorldSemanticMirrors(()=>now);
    a.seed(packet('a',1,false));expect(obsAnchors(a,'a',state)).toHaveLength(0);
    a.accept(packet('a',2));expect(obsAnchors(a,'a',state)).toHaveLength(1);
    const revised=structuredClone(state);revised.snapshot!.configRevision=2;
    expect(obsAnchors(a,'a',revised)).toHaveLength(0);
    now=1001;expect(obsAnchors(a,'a',state)).toHaveLength(0);
  });
  it('a new configured membership has its own role, without asserting atomic network handover',()=>{
    const previous=world().snapshot!;
    const next:WorldSnapshot={...previous,configRevision:2,actors:previous.actors.filter(actor=>actor.actorId==='b')};
    expect(designatedWorldObjectActor(previous)).toBe('a');expect(designatedWorldObjectActor(next)).toBe('b');
  });
});
