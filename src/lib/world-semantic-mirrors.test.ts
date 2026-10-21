// SPDX-License-Identifier: MIT
import {describe,it,expect} from 'vitest';
import {WorldSemanticMirrors,parseSemanticMirrorPacket} from './world-semantic-mirrors';
import type {WorldState} from './world-state';

const world=(instance='world_a',ordinal=1):WorldState=>({version:1,instanceId:instance,availability:'ready',reason:null,snapshot:{schemaVersion:1,stageId:'stage',configRevision:1,sequence:ordinal,safeRegion:{min:{x:0,y:0},max:{x:1000000,y:1000000}},anchors:[{id:'rest',kind:'rest',position:{x:500000,y:500000},slots:[{id:'rest_slot',position:{x:500000,y:500000}}]}],occupied:[],actors:[{actorId:'nova',position:{x:200000+ordinal,y:300000},life:{schemaVersion:1,actorId:'nova',worldId:'source_world',revision:ordinal,simulationTime:0,activity:{id:{actorId:'nova',worldId:'source_world',ordinal},activity:'move_to',phase:'started'}}}]}});
const packet=(sequence=1,epoch='host_epoch',owner='owner_a')=>({version:1,epoch,sequence,objectOwnerActorId:'nova',owners:[{actorId:'nova',controlled:true,owner:{renderSessionId:owner,bindingSequence:1,surfaceId:'native_surface'},worldInstanceId:'world_a',sourceWorldId:'source_world',stageId:'stage',configRevision:1,visible:true}]});

describe('host-only read-only World mirrors (source authored; execution is separate)',()=>{
  it('shows exact actor/source/action identities without any owner or ACK API',()=>{
    const registry=new WorldSemanticMirrors(()=>0);registry.seed(packet());const view=registry.view('nova',world());
    expect(view?.position.x).toBe(200001);expect(view?.action?.id).toEqual({actorId:'nova',worldId:'source_world',ordinal:1});
    for(const forbidden of ['register','startAction','complete','measurement','frame','beginAfterDelivered'])expect(forbidden in registry).toBe(false);
    expect(registry.view('other',world())).toBeNull();
  });
  it('duplicate/out-of-order packets cannot replace a successor or restart its key',()=>{
    const registry=new WorldSemanticMirrors(()=>0);registry.seed(packet());registry.accept(packet(3,'host_epoch','owner_b'));
    const key=registry.view('nova',world())?.key;
    expect(registry.accept(packet(2))).toBe(false);expect(registry.accept(packet(3))).toBe(false);
    expect(registry.view('nova',world())?.key).toBe(key);
  });
  it('foreign epoch event cannot retire a pinned owner; only fresh connection seed can select an epoch',()=>{
    const registry=new WorldSemanticMirrors(()=>0);registry.seed(packet());
    expect(registry.accept({...packet(999,'foreign'),owners:[]})).toBe(false);expect(registry.controlled('nova')).toBe(true);
    registry.reset();expect(registry.accept(packet())).toBe(false);registry.seed(packet(1,'new_epoch'));
    expect(registry.view('nova',world())?.key).toContain('new_epoch');
  });
  it('pending retained constructors suppress Studio registration but do not become a visible owner',()=>{
    const registry=new WorldSemanticMirrors(()=>0);registry.seed({...packet(),objectOwnerActorId:null,owners:[{actorId:'nova',controlled:true,owner:null,worldInstanceId:null,sourceWorldId:null,stageId:null,configRevision:null,visible:false}]});
    expect(registry.controlled('nova')).toBe(true);expect(registry.view('nova',world())).toBeNull();
  });
  it('Stop/expired transport cannot revive pose or return authority to Studio by elapsed time',()=>{
    let now=0;const registry=new WorldSemanticMirrors(()=>now);registry.seed(packet());now=1001;
    expect(registry.view('nova',world())).toBeNull();expect(registry.controlled('nova')).toBe(true);
    registry.accept({...packet(2),owners:[]});expect(registry.controlled('nova')).toBe(false);
    expect(registry.accept(packet())).toBe(false);expect(registry.view('nova',world())).toBeNull();
  });
  it('retired owner cannot replay after Stop even with a newer observation; a successor can appear',()=>{
    const registry=new WorldSemanticMirrors(()=>0);registry.seed(packet());registry.retire();
    registry.accept(packet(2,'host_epoch','owner_a'));expect(registry.view('nova',world())).toBeNull();
    registry.accept(packet(3,'host_epoch','owner_b'));expect(registry.view('nova',world())).not.toBeNull();
  });
  it('joins only exact World/config/source lineage and never fabricates origin or action',()=>{
    const registry=new WorldSemanticMirrors(()=>0);registry.seed(packet());
    expect(registry.view('nova',world('world_b'))).toBeNull();const changed=world();changed.snapshot!.configRevision=2;
    expect(registry.view('nova',changed)).toBeNull();const wrongSource=world();wrongSource.snapshot!.actors[0].life!.worldId='other_source';
    expect(registry.view('nova',wrongSource)).toBeNull();
  });
  it('elected object actor alone renders the shared layer across two bot-bound OBS feeds',()=>{
    const a=new WorldSemanticMirrors(()=>0),b=new WorldSemanticMirrors(()=>0);a.seed(packet());
    b.seed({...packet(),owners:packet().owners.map(row=>({...row,actorId:'other'}))});
    const both=world(),other=structuredClone(both.snapshot!.actors[0]);other.actorId='other';other.life!.actorId='other';other.life!.activity!.id.actorId='other';both.snapshot!.actors.push(other);
    expect(a.view('nova',both)?.anchors).toHaveLength(1);expect(b.view('other',both)).not.toBeNull();expect(b.view('other',both)?.anchors).toHaveLength(0);
  });
  it('rejects authority-shaped extras, getters, duplicate actors and fake pending visibility',()=>{
    expect(()=>parseSemanticMirrorPacket({...packet(),capability:'forged'})).toThrow();
    const getter={...packet()};Object.defineProperty(getter,'epoch',{get(){throw new Error('getter executed');}});
    expect(()=>parseSemanticMirrorPacket(getter)).toThrow();
    expect(()=>parseSemanticMirrorPacket({...packet(),owners:[...packet().owners,...packet().owners]})).toThrow();
    expect(()=>parseSemanticMirrorPacket({...packet(),owners:[{...packet().owners[0],owner:null}]})).toThrow();
  });
});
