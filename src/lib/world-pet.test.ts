import { describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { admittedPetClip, WorldPetPlayer } from './world-pet';
import { AvatarStage } from './avatar';
import { avatarBinding } from './emote-capabilities';
import type { MotionOffer } from './world-motion';

function fixture() {
  const root = new THREE.Group(), bone = new THREE.Object3D(); bone.name = 'pet-target'; root.add(bone);
  const clip = new THREE.AnimationClip('Pet', .2, [new THREE.VectorKeyframeTrack('pet-target.position', [0,.2], [0,0,0,.1,0,0])]);
  const spec = {action:'Pet',durationMs:200};
  return {root,bone,clip,spec,mixer:new THREE.AnimationMixer(root)};
}
describe('Actual authored Pet playback without WebGL or timers', () => {
  it('keeps stale/duplicate offers inert and stops actual Stage clip playback on cancellation', () => {
    const f=fixture(),profile={id:'nova',avatar:{assetPath:'fixture.glb'},motions:[]};
    const world={command:vi.fn(()=> 'accepted'),cancel:vi.fn(),petPlayback:{clip:f.spec,running:false}};
    const stage=Object.create(AvatarStage.prototype) as AvatarStage;
    Object.assign(stage,{options:{motionSource:'world',profile},world,mixer:f.mixer,clips:[f.clip],
      emoteReady:true,assetState:'ready',loadedEmoteBinding:JSON.stringify(avatarBinding(profile)),modelAudio:{stop:vi.fn()}});
    const offer:MotionOffer={version:1,type:'offer',sequence:1,activity:'pet',from:{x:10,y:20},target:{x:10,y:20},
      minimumActiveMs:200,petClip:f.spec,maximumSpeed:100,acceleration:500,validForMs:10000,
      fence:{actorId:'nova',renderSessionId:'render',bindingSequence:1,worldInstanceId:'world',stageId:'stage',
        configRevision:1,topologyRevision:1,layoutRevision:1,assetRevision:1,measurementRevision:1,referenceRevision:1,
        actionId:{actorId:'nova',worldId:'source',ordinal:1},deliveryId:'delivery'}};
    expect(stage.acceptWorldCommand(offer)).toBe('accepted');
    const player=(stage as unknown as {worldPet:WorldPetPlayer}).worldPet;expect(player.action.isScheduled()).toBe(true);
    world.command.mockReturnValue('stale');
    expect(stage.acceptWorldCommand({...offer,petClip:{action:'Pet',durationMs:999}})).toBe('stale');
    expect(world.cancel).not.toHaveBeenCalled();expect((stage as unknown as {worldPet:WorldPetPlayer}).worldPet).toBe(player);
    world.command.mockReturnValue('duplicate');expect(stage.acceptWorldCommand(offer)).toBe('duplicate');
    expect((stage as unknown as {worldPet:WorldPetPlayer}).worldPet).toBe(player);
    stage.cancelWorld('cancelled');expect(world.cancel).toHaveBeenCalledWith('cancelled');
    expect(player.action.isScheduled()).toBe(false);expect(()=>player.prepare(0,true)).toThrow();
    expect((stage as unknown as {worldPet?:WorldPetPlayer}).worldPet).toBeUndefined();
    world.command.mockReturnValue('accepted');
    expect(stage.acceptWorldCommand({...offer,petClip:{action:'Pet',durationMs:999}})).toBe('rejected');
    expect(world.cancel).toHaveBeenLastCalledWith('unsupported_capability');
  });
  it('rejects missing, duplicate, unbound, procedural and substituted clips/durations', () => {
    const f=fixture();
    expect(admittedPetClip(f.root,[f.clip],f.spec)).toBe(f.clip);
    expect(admittedPetClip(f.root,[],f.spec)).toBeNull();
    expect(admittedPetClip(f.root,[f.clip,f.clip.clone()],f.spec)).toBeNull();
    expect(admittedPetClip(new THREE.Group(),[f.clip],f.spec)).toBeNull();
    expect(admittedPetClip(f.root,[f.clip],{...f.spec,action:'wave'})).toBeNull();
    expect(admittedPetClip(f.root,[f.clip],{...f.spec,durationMs:199})).toBeNull();
    expect(admittedPetClip(f.root,[new THREE.AnimationClip('Pet',.2,[])],f.spec)).toBeNull();
  });
  it('holds the first authored pose until native Start and credits only committed rendered proposals', () => {
    const f=fixture(), player=new WorldPetPlayer(f.mixer,f.clip,f.spec);
    const first=player.prepare(0,false);f.mixer.update(0);expect(f.bone.position.x).toBe(0);player.commit(first);
    const held=player.prepare(100,false);f.mixer.update(.1);expect(held.elapsedMs).toBe(0);player.commit(held);
    const failed=player.prepare(150,true);f.mixer.update(.05);expect(f.bone.position.x).toBeCloseTo(.025);expect(failed.finished).toBe(false);
    player.rollback();f.mixer.update(0);expect(f.bone.position.x).toBe(0);
    expect(player.commit(failed)).toBe(false);
    let last=held;
    for(const now of [200,250,300,350]){last=player.prepare(now,true);f.mixer.update(0);expect(player.commit(last)).toBe(true);}
    expect(last).toEqual({action:'Pet',elapsedMs:200,finished:true});
    expect(f.bone.position.x).toBeCloseTo(.1);
    player.stop();expect(player.action.isScheduled()).toBe(false);
    expect(()=>player.prepare(400,true)).toThrow();expect(player.commit(last)).toBe(false);
  });
  it('rejects time regression and unobserved gaps rather than catching up to completion', () => {
    for(const now of [-1,251,NaN]){
      const f=fixture(),player=new WorldPetPlayer(f.mixer,f.clip,f.spec);player.commit(player.prepare(0,true));
      expect(()=>player.prepare(now,true)).toThrow();player.stop();
    }
  });
});
