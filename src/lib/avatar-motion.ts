import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { createVRMAnimationClip, VRMAnimationLoaderPlugin, VRMLookAtQuaternionProxy, type VRMAnimation } from '@pixiv/three-vrm-animation';
import type { VRM, VRMHumanBoneName } from '@pixiv/three-vrm';

export interface MotionClip { name: string; index: number; duration: number; body: string[]; expressions: string[]; lookAt: boolean }
export interface MotionAsset { id: string; name: string; clips: MotionClip[] }
const idPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.vrma$/;
export const motionAction = (id: string, index: number) => `vrma:${id}:${index}`;
export const isMotionAction = (action: string) => action.startsWith('vrma:');

export function motionUrl(id: string, botId: string, outputUrl: string): string {
  if (!idPattern.test(id)) throw new Error('Ungültige VRMA-ID.');
  const output = new URL(outputUrl);
  if (output.protocol !== 'http:' || output.hostname !== '127.0.0.1' || !output.searchParams.get('session')) throw new Error('Bewegungen sind in der Desktop-App verfügbar.');
  const url = new URL(`/motions/${id}`, output);
  url.searchParams.set('session', output.searchParams.get('session')!);
  url.searchParams.set('bot', botId);
  return url.href;
}

/** Same official loader/retargeter for Studio, native overlay and OBS. */
export async function parseMotion(bytes: ArrayBuffer): Promise<VRMAnimation[]> {
  if (bytes.byteLength > 16 * 1024 * 1024 || bytes.byteLength < 20) throw new Error('VRMA ist ungültig oder überschreitet die unterstützten Grenzen.');
  const header = new DataView(bytes);
  if (header.getUint32(0,true) !== 0x46546c67 || header.getUint32(4,true) !== 2 || header.getUint32(8,true) !== bytes.byteLength || header.getUint32(16,true) !== 0x4e4f534a) throw new Error('VRMA ist ungültig oder überschreitet die unterstützten Grenzen.');
  const size = header.getUint32(12,true);
  if (!size || size > 8*1024*1024 || size > bytes.byteLength-20) throw new Error('VRMA ist ungültig oder überschreitet die unterstützten Grenzen.');
  const document = JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(new Uint8Array(bytes,20,size)));
  if (document.extensions?.VRMC_vrm_animation?.specVersion !== '1.0') throw new Error('Der Bewegungsimport unterstützt ausschließlich VRMA 1.0.');
  if (['meshes','images','textures','materials'].some(key => document[key]?.length)) throw new Error('VRMA-Bewegungen müssen ohne Geometrie, Bilder und externe Ressourcen eingebettet sein.');
  const manager = new THREE.LoadingManager();
  manager.setURLModifier(url => {
    if (!/^data:application\/(?:octet-stream|gltf-buffer);base64,/.test(url)) throw new Error('VRMA-Bewegungen müssen ohne Geometrie, Bilder und externe Ressourcen eingebettet sein.');
    return url;
  });
  const loader = new GLTFLoader(manager);
  loader.register(parser => new VRMAnimationLoaderPlugin(parser));
  const gltf = await loader.parseAsync(bytes,'');
  const animations = gltf.userData.vrmAnimations as VRMAnimation[] | undefined;
  if (!animations?.length || animations.length > 32 || animations.some(animation => !Number.isFinite(animation.duration) || animation.duration <= 0 || animation.duration > 120)) throw new Error('VRMA-Clips müssen länger als 0 und höchstens 120 Sekunden sein.');
  return animations;
}

export async function loadMotionClips(vrm: VRM, motions: readonly MotionAsset[], outputUrl: string, botId: string, signal: AbortSignal): Promise<THREE.AnimationClip[]> {
  if (motions.length > 32) throw new Error('Maximal 32 Bewegungsdateien pro Bot.');
  const clips: THREE.AnimationClip[] = [];
  let total = 0;
  for (const motion of motions) {
    const response = await fetch(motionUrl(motion.id, botId, outputUrl),{signal,cache:'no-store'});
    if (!response.ok) throw new Error('Die VRMA-Bewegung konnte nicht geladen werden.');
    const reader = response.body?.getReader();
    if (!reader) throw new Error('Die VRMA-Bewegung konnte nicht geladen werden.');
    const parts: Uint8Array[] = []; let length = 0;
    try {
      for (;;) { const {done,value} = await reader.read(); if (done) break; length += value.length; total += value.length; if (length > 16*1024*1024 || total > 64*1024*1024) throw new Error('VRMA ist ungültig oder überschreitet die unterstützten Grenzen.'); parts.push(value); }
    } catch (error) { await reader.cancel().catch(()=>{}); throw error; }
    finally { reader.releaseLock(); }
    const bytes = new Uint8Array(length); let offset = 0; for (const part of parts) { bytes.set(part,offset); offset += part.length; }
    const animations = await parseMotion(bytes.buffer);
    signal.throwIfAborted();
    for (const [index, animation] of animations.entries()) {
      const clip = createVRMAnimationClip(animation, vrm);
      if (!clip.tracks.length || clip.tracks.some(track=>Array.from(track.values).some(value=>!Number.isFinite(value)))) throw new Error('Diese VRMA-Bewegung hat keine passenden Spuren für diese VRM-Figur.');
      clip.name = motionAction(motion.id,index);
      clips.push(clip);
    }
  }
  return clips;
}

export function ensureMotionLookAtProxy(vrm: VRM): VRMLookAtQuaternionProxy | undefined {
  if (!vrm.lookAt) return undefined;
  const proxy = new VRMLookAtQuaternionProxy(vrm.lookAt);
  proxy.name = `OrbitPalLookAt_${proxy.uuid.replaceAll('-','')}`;
  vrm.scene.add(proxy);
  return proxy;
}

/** Retargeted VRMA tracks animate normalized bones. Transfer only their owned
 * properties, preserving any raw embedded idle/other unowned authored tracks. */
export function transferMotionPose(vrm: VRM, clip: THREE.AnimationClip): void {
  const owned = new Set(clip.tracks.map(track => track.name));
  const preserve: {raw: THREE.Object3D; quaternion?: THREE.Quaternion; position?: THREE.Vector3; scale: THREE.Vector3}[] = [];
  for (const name of Object.keys(vrm.humanoid.humanBones) as VRMHumanBoneName[]) {
    const raw = vrm.humanoid.getRawBoneNode(name), normalized = vrm.humanoid.getNormalizedBoneNode(name);
    if (raw && normalized) preserve.push({raw,quaternion:owned.has(`${normalized.name}.quaternion`)?undefined:raw.quaternion.clone(),position:owned.has(`${normalized.name}.position`)?undefined:raw.position.clone(),scale:raw.scale.clone()});
  }
  vrm.humanoid.update();
  for (const {raw,quaternion,position,scale} of preserve) { if (quaternion) raw.quaternion.copy(quaternion); if (position) raw.position.copy(position); raw.scale.copy(scale); }
}
