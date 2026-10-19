// SPDX-License-Identifier: MIT
import { afterEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { VRMLoaderPlugin, type VRM } from '@pixiv/three-vrm';
import { createVRMAnimationClip } from '@pixiv/three-vrm-animation';
import { AvatarStage } from './avatar';
import { parseMotion, motionAction, ensureMotionLookAtProxy } from './avatar-motion';
import { availableActions, avatarBinding, clipHasTarget, currentEmotePayload, emoteBinding, presentAdmittedEmote, resolveAvailableAction } from './emote-capabilities';
import type { EmotePayload } from './presentation';
const clip=(name='Wave',track='Face.position')=>new THREE.AnimationClip(name,1,[new THREE.VectorKeyframeTrack(track,[0,1],[0,0,0,0,1,0])]);
function fixture(action='Wave'){
  const imported=new THREE.Group();const face=new THREE.Mesh();face.name='Face';face.morphTargetDictionary={smile:0};face.morphTargetInfluences=[0];imported.add(face);
  const configured={id:'hello',action,sound:'builtin:wave',duration:1,intensity:1,soundVolume:.7,allowChat:true};
  const profile={id:'nova',avatar:{assetPath:'fixture.glb'},motions:[],emotes:[configured]};
  const stage=Object.create(AvatarStage.prototype) as any;
  Object.assign(stage,{options:{profile},asset:'fixture.glb',emoteReady:true,loadedEmoteBinding:JSON.stringify(avatarBinding(profile)),mixer:new THREE.AnimationMixer(imported),clips:[clip()],robot:{root:new THREE.Group()},imported,humanoidPose:new Map(),humanoidBasis:new THREE.Quaternion(),time:0,disposed:false});
  const event:EmotePayload={...configured,botId:profile.id,nonce:'fixture',avatarBinding:avatarBinding(profile),emoteBinding:emoteBinding(configured)};
  const target={canPresent:(event:EmotePayload)=>stage.canPresentEmote(event,profile),present:(event:EmotePayload)=>stage.presentEmote(event,profile)};
  return {stage,face,profile,configured,event,target};
}
afterEach(()=>vi.unstubAllGlobals());
describe('Current character emote admission',()=>{
  it('offers only unique usable clips and resolves exact names before case aliases',()=>{
    const clips=[clip('Wave'),clip('Wave'),clip('Jump'),clip('NoTime')];clips[3].duration=0;
    const actions=availableActions(clips,['wave','hide','celebrate']);
    for(const action of ['Wave','wave','unknown','NoTime'])expect(resolveAvailableAction(actions,action)).toBeUndefined();
    expect(resolveAvailableAction(actions,'celebrate')?.action).toBe('Jump');expect(resolveAvailableAction(actions,'hide')?.kind).toBe('gesture');
    const cases=availableActions([clip('Wave'),clip('wave')],['wave']);expect(resolveAvailableAction(cases,'Wave')?.action).toBe('Wave');expect(resolveAvailableAction(cases,'WAVE')).toBeUndefined();
  });
  it('requires actual target properties, skeleton properties and in-range morph indices',()=>{
    const {stage}=fixture();const root=stage.imported;
    const morph=(name:string)=>new THREE.AnimationClip('Face',1,[new THREE.NumberKeyframeTrack(name,[0,1],[0,1])]);
    for(const track of ['Missing.position','Face.doesNotExist','Face.morphTargetInfluences[999]','Face.morphTargetInfluences[-1]','Face.morphTargetInfluences[unknown]'])expect(clipHasTarget(root,morph(track)),track).toBe(false);
    expect(clipHasTarget(root,morph('Face.morphTargetInfluences[smile]'))).toBe(true);
    const mesh=new THREE.SkinnedMesh();mesh.name='Body';const bone=new THREE.Bone();bone.name='Head';mesh.add(bone);mesh.bind(new THREE.Skeleton([bone]));root.add(mesh);
    expect(clipHasTarget(root,clip('Bone','Body.bones[Head].position'))).toBe(true);expect(clipHasTarget(root,clip('Bone','Body.bones[Head].missing'))).toBe(false);
  });
  it('starts the real mixer action before permitting sound and never sounds for missing/ambiguous clips',()=>{
    const {stage,face,event,target}=fixture();const sound=vi.fn(()=>expect(stage.emote.clip.isScheduled()).toBe(true));
    expect(presentAdmittedEmote(target,event,sound)).toBe(true);stage.mixer.update(.5);expect(face.position.y).toBeCloseTo(.5);expect(sound).toHaveBeenCalledOnce();
    stage.clips.push(clip());expect(presentAdmittedEmote(target,event,sound)).toBe(false);
    stage.clips=[clip('MadeUp','Missing.position')];expect(presentAdmittedEmote(target,{...event,action:'MadeUp'},sound)).toBe(false);expect(sound).toHaveBeenCalledOnce();
  });
  it('drops late effects after loading failures, cancellation, bot/asset/motion changes or configuration edits',async()=>{
    for(const mutate of [
      ({stage}:ReturnType<typeof fixture>)=>stage.emoteReady=false,
      ({stage}:ReturnType<typeof fixture>)=>stage.disposed=true,
      ({profile}:ReturnType<typeof fixture>)=>profile.id='other',
      ({profile}:ReturnType<typeof fixture>)=>profile.avatar.assetPath='other.glb',
      ({profile}:ReturnType<typeof fixture>)=>(profile.motions as any[]).push({id:'motion.vrma'}),
      ({configured}:ReturnType<typeof fixture>)=>configured.action='hide',
      ({configured}:ReturnType<typeof fixture>)=>configured.sound='builtin:cheer',
      ({configured}:ReturnType<typeof fixture>)=>configured.allowChat=false,
    ]){
      const f=fixture();const sound=vi.fn(),prepare=vi.fn(async()=>{});
      if(f.target.canPresent(f.event))await prepare();expect(prepare).toHaveBeenCalledOnce();mutate(f);
      expect(presentAdmittedEmote(f.target,f.event,sound)).toBe(false);expect(sound).not.toHaveBeenCalled();
    }
    const f=fixture();f.stage.emoteReady=false;const prepare=vi.fn();if(f.target.canPresent(f.event))prepare();expect(prepare).not.toHaveBeenCalled();
  });
  it('does not trust a binding copied from a different configured emote or bot',()=>{
    const {profile,event,target}=fixture();expect(currentEmotePayload(profile,event)).toBe(true);
    expect(target.canPresent({...event,action:'hide'})).toBe(false);expect(target.canPresent({...event,botId:'other'})).toBe(false);
    expect(target.canPresent({...event,id:undefined})).toBe(false);profile.emotes.push({...profile.emotes[0]});expect(target.canPresent(event)).toBe(false);
  });
  it.each(['0','1'])('admits actual VRM %s expression and gaze clips with the official loaders',async(version)=>{
    vi.stubGlobal('ProgressEvent',class{constructor(public type:string,public init:unknown){}});
    const file=(name:string)=>{const bytes=readFileSync(new URL(`./fixtures/${name}`,import.meta.url));return bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength) as ArrayBuffer;};
    const loader=new GLTFLoader();loader.register(parser=>new VRMLoaderPlugin(parser));const gltf=await loader.parseAsync(file(`target-vrm${version}.vrm`),'');const vrm=gltf.userData.vrm as VRM;
    const proxy=ensureMotionLookAtProxy(vrm)!;const animation=(await parseMotion(file('greeting.vrma')))[1];const motion=createVRMAnimationClip(animation,vrm);motion.name=motionAction('00000000-0000-4000-8000-000000000001.vrma',1);
    const f=fixture(motion.name);f.stage.vrm=vrm;f.stage.imported=vrm.scene;f.stage.clips=[motion];f.stage.mixer=new THREE.AnimationMixer(vrm.scene);
    expect(presentAdmittedEmote(f.target,f.event,vi.fn())).toBe(true);f.stage.mixer.update(.25);expect(vrm.expressionManager!.getValue('happy')).toBeGreaterThan(0);
    expect(f.stage.emote.duration).toBe(.5);
    const gaze=createVRMAnimationClip((await parseMotion(file('greeting.vrma')))[0],vrm);
    f.stage.motionLookAt=proxy;
    gaze.tracks=gaze.tracks.filter(track=>track.name===`${proxy.name}.quaternion`);gaze.name=f.event.action;f.stage.clips=[gaze];
    expect(f.stage.getModelInfo().actions.some((action:any)=>action.action===gaze.name)).toBe(false);expect(f.target.canPresent(f.event)).toBe(false);
    // A second real GLB/VRM loader target contains a declared eye bone.
    const original=new Uint8Array(file(`target-vrm${version}.vrm`));const header=new DataView(original.buffer);const length=header.getUint32(12,true);
    const document=JSON.parse(new TextDecoder().decode(original.slice(20,20+length)));const eyeIndex=document.nodes.length;
    document.nodes.push({name:'Eye',translation:[0,.1,0]});document.nodes[2].children=[...(document.nodes[2].children??[]),eyeIndex];
    if(version==='0')document.extensions.VRM.humanoid.humanBones.push({bone:'leftEye',node:eyeIndex,useDefaultValues:true});else document.extensions.VRMC_vrm.humanoid.humanBones.leftEye={node:eyeIndex};
    const json=new TextEncoder().encode(JSON.stringify(document));const padded=Math.ceil(json.length/4)*4;const amended=new Uint8Array(20+padded+original.length-20-length);
    amended.set(original.slice(0,20));amended.fill(32,20,20+padded);amended.set(json,20);amended.set(original.slice(20+length),20+padded);const view=new DataView(amended.buffer);view.setUint32(8,amended.length,true);view.setUint32(12,padded,true);
    const eyeGltf=await loader.parseAsync(amended.buffer,'');const eyeVrm=eyeGltf.userData.vrm as VRM;const eyeProxy=ensureMotionLookAtProxy(eyeVrm)!;
    const eyeClip=createVRMAnimationClip((await parseMotion(file('greeting.vrma')))[0],eyeVrm);eyeClip.tracks=eyeClip.tracks.filter(track=>track.name===`${eyeProxy.name}.quaternion`);eyeClip.name=f.event.action;
    const eyeFixture=fixture(eyeClip.name);Object.assign(eyeFixture.stage,{vrm:eyeVrm,imported:eyeVrm.scene,mixer:new THREE.AnimationMixer(eyeVrm.scene),clips:[eyeClip],motionLookAt:eyeProxy});
    expect(presentAdmittedEmote(eyeFixture.target,eyeFixture.event,vi.fn())).toBe(true);eyeFixture.stage.mixer.update(.5);eyeVrm.lookAt!.autoUpdate=false;eyeVrm.update(.016);
    expect(eyeVrm.humanoid.getRawBoneNode('leftEye')!.quaternion.angleTo(new THREE.Quaternion())).toBeGreaterThan(.001);
    f.stage.clips=[motion];
    // Empty expression placeholders must not authorize sound.
    for(const bind of [...vrm.expressionManager!.getExpression('happy')!.binds])vrm.expressionManager!.getExpression('happy')!.deleteBind(bind);expect(f.target.canPresent(f.event)).toBe(false);
  });
});
