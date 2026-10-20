import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { WorldAvatarController } from './world-avatar';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { VRMLoaderPlugin, VRMUtils, type VRM } from '@pixiv/three-vrm';
import { createVRMAnimationClip } from '@pixiv/three-vrm-animation';
import { ensureMotionLookAtProxy, loadMotionClips, motionAction, motionUrl, parseMotion, transferMotionPose } from './avatar-motion';
import { AuthoredAnimationProperties, finishVrmAnimationFrame } from './avatar-animation';
import { AvatarStage } from './avatar';

const file = (name: string) => {
  const bytes = readFileSync(new URL(`./fixtures/${name}`,import.meta.url));
  return bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength) as ArrayBuffer;
};
const id = '11111111-1111-4111-8111-111111111111.vrma';
const endpoint = 'http://127.0.0.1:4040/overlay?session=fixture&bot=nova';
async function target(version: string): Promise<VRM> {
  const loader = new GLTFLoader(); loader.register(parser=>new VRMLoaderPlugin(parser));
  const gltf = await loader.parseAsync(file(`target-vrm${version}.vrm`),'');
  const vrm = gltf.userData.vrm as VRM; VRMUtils.rotateVRM0(vrm);
  return vrm;
}
afterEach(()=>vi.unstubAllGlobals());

describe('Official VRMA 1.0 loader and VRM 0/1 retargeting',()=>{
  it('loads real separate motions with body, expression, gaze and natural clip lengths',async()=>{
    const motions=await parseMotion(file('greeting.vrma'));
    expect(motions.map(motion=>motion.duration)).toEqual([1.25,0.5]);
    expect([...motions[0].humanoidTracks.rotation.keys()]).toContain('leftUpperArm');
    expect([...motions[0].expressionTracks.preset.keys()]).toEqual(['happy']);
    expect(motions[0].lookAtTrack).toBeTruthy();
  });

  it.each(['0','1'])('animates the actual VRM %s loader target without a second transfer erasing the motion',async(version)=>{
    const vrm=await target(version);const proxy=ensureMotionLookAtProxy(vrm)!;
    const motion=(await parseMotion(file('greeting.vrma')))[0];
    const clip=createVRMAnimationClip(motion,vrm);const mixer=new THREE.AnimationMixer(vrm.scene);
    const action=mixer.clipAction(clip).play();const properties=new AuthoredAnimationProperties(vrm.scene);
    const arm=vrm.humanoid.getRawBoneNode('leftUpperArm')!;
    const right=vrm.humanoid.getRawBoneNode('rightUpperArm')!;
    const rest=arm.quaternion.clone();const untouched=right.quaternion.clone();
    for(let frame=0;frame<40;frame++){
      vrm.humanoid.update();properties.restoreTransforms([action]);mixer.update(1/60);properties.captureTransforms([action]);transferMotionPose(vrm,clip);
      vrm.lookAt!.autoUpdate=false;finishVrmAnimationFrame(vrm,1/60,properties,[action]);
    }
    expect(arm.quaternion.angleTo(rest)).toBeGreaterThan(0.1);
    expect(right.quaternion.angleTo(untouched)).toBeLessThan(0.001);
    expect(vrm.expressionManager!.getValue('happy')).toBeGreaterThan(0.2);
    expect(Math.abs(vrm.lookAt!.yaw)).toBeGreaterThan(3);
    expect(properties.owns([action],proxy,'quaternion')).toBe(true);
    expect(clip.tracks.every(track=>Array.from(track.values).every(Number.isFinite))).toBe(true);
    action.stop();mixer.update(0);vrm.humanoid.resetNormalizedPose();vrm.humanoid.update();
    expect(arm.quaternion.angleTo(rest)).toBeLessThan(0.001);
  });

  it('binds fetches to the exact bot and bounded local session route',()=>{
    expect(new URL(motionUrl(id,'fox',endpoint)).searchParams.get('bot')).toBe('fox');
    for(const candidate of ['../private.vrma','https://example.invalid/a.vrma'])expect(()=>motionUrl(candidate,'fox',endpoint)).toThrow();
    expect(()=>motionUrl(id,'fox','https://example.invalid/overlay?session=bad')).toThrow();
  });

  it('rejects cancellation and oversized streams before a clip can be returned',async()=>{
    const vrm=await target('1');ensureMotionLookAtProxy(vrm);
    const controller=new AbortController();
    const motions=[{id,name:'Hello',clips:[]}];
    vi.stubGlobal('fetch',vi.fn(async()=>{controller.abort();return new Response(file('greeting.vrma'));}));
    await expect(loadMotionClips(vrm,motions,endpoint,'nova',controller.signal)).rejects.toThrow();
    let cancelled=false;
    vi.stubGlobal('fetch',vi.fn(async()=>new Response(new ReadableStream({start(stream){stream.enqueue(new Uint8Array(16*1024*1024+1));},cancel(){cancelled=true;}}))));
    await expect(loadMotionClips(vrm,motions,endpoint,'nova',new AbortController().signal)).rejects.toThrow(/Grenzen/);
    expect(cancelled).toBe(true);
  });

  it('uses the natural duration and cancels a VRMA action on Stop or replacement',async()=>{
    const vrm=await target('1');ensureMotionLookAtProxy(vrm);
    const clip=createVRMAnimationClip((await parseMotion(file('greeting.vrma')))[0],vrm);clip.name=motionAction(id,0);
    const stage=Object.create(AvatarStage.prototype) as any;
    const robot={root:new THREE.Group()},imported=vrm.scene,travel=new THREE.Group(),worldHeading=new THREE.Group(),worldOffset=new THREE.Group();
    worldOffset.add(robot.root,imported);worldHeading.add(worldOffset);travel.add(worldHeading);
    Object.assign(stage,{options:{profile:{id:'vrma-fixture'},motionSource:'fixed'},previousMotionSource:'fixed',worldRecovery:false,
      travel,worldHeading,worldOffset,world:new WorldAvatarController(travel,worldHeading,worldOffset),
      emoteReady:true,vrm,mixer:new THREE.AnimationMixer(vrm.scene),clips:[clip],robot,imported,humanoidPose:new Map(),humanoidBasis:new THREE.Quaternion(),time:0});
    stage.triggerEmote(clip.name,15_000,1);
    expect(stage.emote.duration).toBe(1.25);expect(stage.emote.clip.timeScale).toBe(1);
    const action=stage.emote.clip;stage.finishEmote();
    expect(stage.emote).toBeUndefined();expect(action.isScheduled()).toBe(false);
  });
});
