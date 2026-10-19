import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as THREE from 'three';
import { AvatarStage } from './avatar';
import { sampleAmbientMotion } from './autopilot-motion';
import { ModelAudioController, MODEL_AUDIO_EXTENSION as EXT, modelAudioPlan, validateModelWav } from './avatar-audio';

function wav(seconds = 0.5) {
  const frames = Math.ceil(8000 * seconds);
  const bytes = new Uint8Array(44 + frames * 2); const view = new DataView(bytes.buffer);
  const tag = (at: number, text: string) => bytes.set([...text].map(char => char.charCodeAt(0)), at);
  tag(0,'RIFF'); view.setUint32(4,bytes.length-8,true); tag(8,'WAVE'); tag(12,'fmt ');
  view.setUint32(16,16,true);view.setUint16(20,1,true);view.setUint16(22,1,true);
  view.setUint32(24,8000,true);view.setUint32(28,16000,true);view.setUint16(32,2,true);view.setUint16(34,16,true);
  tag(36,'data');view.setUint32(40,frames*2,true);
  return bytes;
}
function model() {
  return {
    extensionsUsed:[EXT], extensions:{[EXT]:{clips:[{bufferView:0,mimeType:'audio/wav'}],emitters:[{clips:[{clip:0}],volume:0.5}]}},
    buffers:[{byteLength:wav().length}],bufferViews:[{buffer:0,byteLength:wav().length}],
    animations:[{extensions:{[EXT]:{events:[{time:0,action:'play',emitter:0},{time:0.2,action:'stop',emitter:0},{time:0.3,action:'play',emitter:0,startOffset:0.1}]}}}],
  } as any;
}
class Sink {
  prepared = new Set<string>();
  prepare = vi.fn(async (emotes: readonly any[], resolver: (id:string)=>Promise<any>) => {
    this.prepared.clear();
    for (const emote of emotes) { try { await resolver(emote.sound); this.prepared.add(emote.sound); } catch { /* EmoteAudioPlayer reports decode failures separately. */ } }
  });
  play = vi.fn(); stopSound = vi.fn(); halt = vi.fn(); unlock = vi.fn(async()=>{});destroy=vi.fn();setVolume=vi.fn();
  stop = vi.fn(()=>{this.prepared.clear();});
  isPrepared = (id:string) => this.prepared.has(id);
}
const frame = (time:number, token='manual:one',rate=1,loop=false) => ({token,time,animation:0,rate,loop});

describe('pinned MSFT audio draft subset',()=>{
  it('loads a real GLB with the production Three parser and resolves its actual embedded WAV view',async()=>{
    const source=readFileSync(new URL('./fixtures/audio-draft-v1.glb',import.meta.url));
    const bytes=source.buffer.slice(source.byteOffset,source.byteOffset+source.byteLength) as ArrayBuffer;
    const gltf=await new GLTFLoader().parseAsync(bytes,'');
    expect(gltf.animations[0].name).toBe('Wave');
    const sink=new Sink();const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    await controller.load(gltf.parser.json,index=>gltf.parser.getDependency('bufferView',index));
    await controller.unlock();expect(sink.play).not.toHaveBeenCalled();
    controller.present(frame(0.01),0.7);expect(sink.play).toHaveBeenCalledTimes(1);
    controller.destroy();
  });
  it('accepts bounded embedded views and canonical WAV data URIs without resolving anything',()=>{
    expect(modelAudioPlan(model())?.animations[0]).toHaveLength(3);
    const value=model();value.extensions[EXT].clips[0]={uri:'data:audio/wav;base64,'+btoa(String.fromCharCode(...wav()))};
    expect(modelAudioPlan(value)?.clips[0].bytes).toBe(wav().length);
  });
  it('rejects unsupported versions, spatial/loop/randomization/pause features and malformed references',()=>{
    const changes=[
      (v:any)=>v.extensions[EXT].specVersion='1.0',
      (v:any)=>v.extensions[EXT].version='next',
      (v:any)=>v.extensions[EXT].emitters[0].loop=true,
      (v:any)=>v.extensions[EXT].emitters[0].refDistance=1,
      (v:any)=>v.extensions[EXT].emitters[0].clips.push({clip:0}),
      (v:any)=>v.animations[0].extensions[EXT].events[0].action='pause',
      (v:any)=>v.animations[0].extensions[EXT].events[0].time=-1,
      (v:any)=>v.animations[0].extensions[EXT].events[0].emitter=9,
      (v:any)=>v.extensions[EXT].clips[0].bufferView=9,
      (v:any)=>v.bufferViews[0].byteOffset=Number.MAX_SAFE_INTEGER,
      (v:any)=>v.nodes=[{extensions:{[EXT]:{emitters:[0]}}}],
    ];
    for(const change of changes){const value=model();change(value);expect(()=>modelAudioPlan(value)).toThrow();}
  });
  it('refuses all external audio/buffer URIs before a loader dependency can be called',async()=>{
    const resolver=vi.fn();const player=new Sink();const warn=vi.fn();const controller=new ModelAudioController(warn,undefined,player);controller.setEnabled(true);
    for(const uri of ['https://example.invalid/a.wav','file:///private.wav','../outside.wav','data:audio/mpeg;base64,AAAA']){
      const value=model();value.extensions[EXT].clips[0]={uri};
      await controller.load(value,resolver);
    }
    const value=model();value.buffers[0].uri='https://example.invalid/buffer.bin';
    await controller.load(value,resolver);
    expect(resolver).not.toHaveBeenCalled();expect(player.prepare).not.toHaveBeenCalled();expect(warn).toHaveBeenCalled();
    value.extensionsRequired=[EXT];await expect(controller.load(value,resolver)).rejects.toThrow();
    controller.destroy();
  });
  it('bounds declared bytes/counts and requires real bounded PCM16 WAV before decoding',()=>{
    const value=model();value.bufferViews[0].byteLength=5*1024*1024+1;value.buffers[0].byteLength=value.bufferViews[0].byteLength;
    expect(()=>modelAudioPlan(value)).toThrow(/Profilbudget/);
    const events=model();events.animations[0].extensions[EXT].events=Array.from({length:129},()=>({time:0,action:'play',emitter:0}));
    expect(()=>modelAudioPlan(events)).toThrow(/Profilbudget/);
    expect(()=>validateModelWav(wav())).not.toThrow();
    expect(()=>validateModelWav(wav(15.001))).toThrow(/PCM16/);
    const malformed=wav();new DataView(malformed.buffer).setUint16(22,64,true);
    expect(()=>validateModelWav(malformed)).toThrow(/PCM16/);
    expect(()=>validateModelWav(new Uint8Array(44))).toThrow(/PCM16/);
  });
});

describe('model audio presentation ownership and lifetime',()=>{
  it('advances the real ambient animation mixer on consecutive rendered timeline frames without double time scaling',async()=>{
    const source=readFileSync(new URL('./fixtures/audio-draft-v1.glb',import.meta.url));
    const gltf=await new GLTFLoader().parseAsync(source.buffer.slice(source.byteOffset,source.byteOffset+source.byteLength) as ArrayBuffer,'');
    const config={active:true,wander:false,gestures:true,startedAt:1_000_000,seed:'audio-ambient-fixture'};
    let now=config.startedAt;
    for(let offset=0;offset<45_000;offset+=10){const frame=sampleAmbientMotion(config,config.startedAt+offset);if(frame.gesture&&frame.gesture.elapsed>=0.5){now=config.startedAt+offset;break;}}
    expect(now).toBeGreaterThan(config.startedAt);
    const camera=new THREE.PerspectiveCamera(32,1.5,0.05,100);camera.position.set(0,1,7);
    const root=new THREE.Group();root.visible=false;
    const stage=Object.create(AvatarStage.prototype) as any;
    Object.assign(stage,{options:{speaking:false,autopilot:config},asset:'fixture',robot:{root},imported:new THREE.Group(),travel:new THREE.Group(),
      mixer:new THREE.AnimationMixer(gltf.scene),clips:gltf.animations,modelAudio:{stop:vi.fn()},humanoidPose:new Map(),humanoidBasis:new THREE.Quaternion(),
      camera,controls:{target:new THREE.Vector3()},contentWidth:1,contentDepth:1,contentHeight:1,travelFraction:0,walkStrength:0,walking:false,
      time:0,ambientSession:'',ambientGesture:'',ambientActions:['Wave']});
    const clock=vi.spyOn(Date,'now').mockImplementation(()=>now);
    try{
      stage.updateAutopilot(0.1,false);stage.animateEmote(false);stage.mixer.update(0.2);
      const first=stage.emote.clip.time;const y=gltf.scene.getObjectByName('Triangle')!.position.y;
      now+=100;stage.time+=0.1;stage.updateAutopilot(0.1,false);stage.animateEmote(false);stage.mixer.update(0.2);
      const frame=sampleAmbientMotion(config,now).gesture!;
      expect(stage.emote.clip.time).toBeCloseTo(gltf.animations[0].duration*frame.elapsed/frame.duration,6);
      expect(stage.emote.clip.time).toBeGreaterThan(first);
      expect(gltf.scene.getObjectByName('Triangle')!.position.y).toBeGreaterThan(y);
      expect(stage.emote.clip.getEffectiveTimeScale()).toBe(0);
    }finally{clock.mockRestore();}
  });
  it('does not decode in silent previews or autoplay on load, and requires explicit output activation',async()=>{
    const sink=new Sink();const resolve=vi.fn(async()=>wav().buffer);const controller=new ModelAudioController(vi.fn(),undefined,sink);
    await controller.load(model(),resolve);expect(resolve).not.toHaveBeenCalled();
    controller.present(frame(0.01),1);expect(sink.play).not.toHaveBeenCalled();
    controller.setEnabled(true);await controller.unlock();
    expect(sink.play).not.toHaveBeenCalled();
    controller.present(frame(0.02),1);expect(sink.play).toHaveBeenCalledTimes(1);
    controller.setEnabled(false);controller.present(frame(0.31),1);expect(sink.play).toHaveBeenCalledTimes(1);
    controller.destroy();
  });
  it('synchronizes play/stop to actual clip frames, offset/rate and independent emitter voices',async()=>{
    const sink=new Sink();const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    await controller.load(model(),async()=>wav().buffer);
    controller.present(frame(0.01),1);expect(sink.play).not.toHaveBeenCalled();
    await controller.unlock();
    controller.present(frame(0.02),0.4);expect(sink.play).not.toHaveBeenCalled();
    controller.present(frame(0.32,'manual:one',2),0.4);
    expect(sink.stopSound).toHaveBeenCalledWith('model:0');
    expect(sink.play).toHaveBeenLastCalledWith({sound:'model:0',bufferSound:'model:0',soundVolume:0.5,soundOffset:expect.closeTo(0.12),soundRate:2});
    expect(sink.setVolume).toHaveBeenLastCalledWith(0.4);
    controller.present(frame(0.33),0);expect(sink.setVolume).toHaveBeenLastCalledWith(0);
    controller.destroy();
  });
  it('Stop blocks the current presentation and cache reuse is allowed only for a new gesture',async()=>{
    const sink=new Sink();const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    await controller.load(model(),async()=>wav().buffer);await controller.unlock();controller.present(frame(0.01),1);
    expect(sink.play).toHaveBeenCalledTimes(1);
    controller.stop();controller.present(frame(0.32),1);expect(sink.play).toHaveBeenCalledTimes(1);
    controller.present(frame(0.01,'manual:two'),1);expect(sink.play).toHaveBeenCalledTimes(2);
    controller.clear();controller.present(frame(0.32,'manual:two'),1);expect(sink.play).toHaveBeenCalledTimes(2);
    controller.destroy();
  });
  it('prepares a shared clip once while retaining distinct emitter stop identities',async()=>{
    const value=model();value.extensions[EXT].emitters.push({clips:[{clip:0}],volume:1});
    value.animations[0].extensions[EXT].events.splice(1,0,{time:0.1,action:'play',emitter:1});
    const sink=new Sink();const resolve=vi.fn(async()=>wav().buffer);const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    await controller.load(value,resolve);await controller.unlock();expect(resolve).toHaveBeenCalledTimes(1);
    controller.present(frame(0.01),1);controller.present(frame(0.11),1);
    expect(sink.play.mock.calls.map(call=>[call[0].sound,call[0].bufferSound])).toEqual([['model:0','model:0'],['model:1','model:0']]);
    controller.destroy();
  });
  it('skips stale events after a render stall or joining midway and restarts audio on a fresh loop',async()=>{
    const sink=new Sink();const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    await controller.load(model(),async()=>wav().buffer);await controller.unlock();
    controller.present(frame(0.8),1);expect(sink.play).not.toHaveBeenCalled();
    controller.present(frame(0.9),1);controller.present(frame(0.01,'manual:one',0.8,true),1);
    expect(sink.play).toHaveBeenCalledTimes(1);
    controller.present(frame(1.2),1);expect(sink.play).toHaveBeenCalledTimes(1);
    controller.destroy();
  });
  it('discards late resources after model replacement and never turns them into late playback',async()=>{
    const sink=new Sink();const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    let finish!:(value:ArrayBuffer)=>void;
    const pending=controller.load(model(),()=>new Promise(resolve=>{finish=resolve;}));
    controller.clear();finish(wav().buffer);await pending;await controller.unlock();controller.present(frame(0.01),1);
    expect(sink.play).not.toHaveBeenCalled();controller.destroy();
  });
  it('Stop invalidates a pending activation and an unsupported playback rate halts existing sound',async()=>{
    const sink=new Sink();const controller=new ModelAudioController(vi.fn(),undefined,sink);controller.setEnabled(true);
    await controller.load(model(),async()=>wav().buffer);
    let finish!:()=>void;sink.unlock.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
    const unlocking=controller.unlock();controller.stop();finish();await unlocking;
    controller.present(frame(0.01,'manual:new'),1);expect(sink.play).not.toHaveBeenCalled();
    await controller.unlock();controller.present(frame(0.31,'manual:new'),1);expect(sink.play).toHaveBeenCalledTimes(1);
    const before=sink.halt.mock.calls.length;controller.present(frame(0.32,'manual:new',50),1);
    expect(sink.halt.mock.calls.length).toBeGreaterThan(before);controller.destroy();
  });
});
