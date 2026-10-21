import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { AvatarStage } from './avatar';
import { avatarBinding } from './emote-capabilities';
import { AvatarPresentationGate } from './avatar-presentation';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, failed) => { resolve = done; reject = failed; });
  return { promise, resolve, reject };
}

function model() {
  const scene = new THREE.Group();
  const geometry = new THREE.BoxGeometry(1, 2, 1);
  scene.add(new THREE.Mesh(geometry, new THREE.MeshBasicMaterial()));
  return {
    scene, animations: [] as THREE.AnimationClip[], userData: {} as Record<string, unknown>,
    parser: { json: {}, getDependency: vi.fn(async () => new ArrayBuffer(0)) },
    disposed: vi.spyOn(geometry, 'dispose'),
  };
}

/** Exercise the production Stage load/normalization/ready/disposal path with
 * real Three geometry. Only WebGL, the network loader and decoded audio are
 * replaced; completing the deferred loader is not itself a Ready assertion. */
function stageFixture() {
  const profile = { id: 'cold-load', avatar: { assetPath: 'fixture.glb' }, motions: [] };
  const requests: { url: string; result: ReturnType<typeof deferred<ReturnType<typeof model>>> }[] = [];
  const root = new THREE.Group();
  root.add(new THREE.Mesh(new THREE.BoxGeometry(1, 2, 1), new THREE.MeshBasicMaterial()));
  const imported = new THREE.Group(), scene = new THREE.Scene(); scene.add(root, imported);
  const audio = { load: vi.fn(async (_json: unknown, _dependency: unknown) => {}), clear: vi.fn(), destroy: vi.fn() };
  const stage = Object.create(AvatarStage.prototype) as AvatarStage;
  Object.assign(stage, {
    options: { profile, motionSource: 'fixed' }, request: 0, disposed: false, asset: '', assetKey: '',
    emoteReady: false, loadedEmoteBinding: '', assetState: 'ready',
    assetLoader: {
      loadAsync: vi.fn((url: string) => { const result = deferred<ReturnType<typeof model>>(); requests.push({ url, result }); return result.promise; }),
      dispose: vi.fn(),
    },
    presentation: new AvatarPresentationGate(), modelAudio: audio,
    robot: { root }, imported, scene, travel: new THREE.Group(),
    desktopMotion: { reset: vi.fn() }, walkClock: { reset: vi.fn() },
    walkAnchorRest: new THREE.Vector3(), humanoidBasis: new THREE.Quaternion(),
    humanoidPose: new Map(), faceVowels: new Map(), vrmExpressions: new Map(), mouthExpressions: new Map(),
    genericBoneRest: new Map(), previousBoneOffsets: new Map(),
    finishEmote: vi.fn(), update: vi.fn(),
    world: { dispose: vi.fn() }, observer: { disconnect: vi.fn() }, controls: { dispose: vi.fn() },
    renderer: { domElement: { removeEventListener: vi.fn(), remove: vi.fn() }, dispose: vi.fn(), forceContextLoss: vi.fn() },
  });
  return { stage, profile, requests, audio, imported };
}

beforeEach(() => {
  vi.stubGlobal('window', { location: { href: 'http://127.0.0.1/fixture/' } });
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe('AvatarStage cold-load completion', () => {
  it('keeps a same-key replacement subscriber pending until the actual model and audio load complete', async () => {
    const { stage, profile, requests, audio } = stageFixture();
    const audioReady = deferred<void>(); audio.load.mockImplementation(() => audioReady.promise);
    const first = stage.loadAsset('/fixture.glb', [], '', profile.id);
    const replacement = stage.loadAsset('/fixture.glb', [], '', profile.id);
    expect(replacement).toBe(first);
    const completed = vi.fn(); void replacement.then(completed);
    await Promise.resolve();
    expect(completed).not.toHaveBeenCalled();
    expect(stage.getModelInfo().status).toBe('loading');
    expect(requests).toHaveLength(1);
    requests[0].result.resolve(model()); await Promise.resolve();
    expect(audio.load).toHaveBeenCalledOnce();
    expect(completed).not.toHaveBeenCalled();
    expect(stage.loadAsset('/fixture.glb', [], '', profile.id)).toBe(first);
    audioReady.resolve(); await Promise.all([first, replacement]);
    expect(completed).toHaveBeenCalledOnce();
    expect(stage.getModelInfo()).toMatchObject({ status: 'ready', format: 'glb', avatarBinding: avatarBinding(profile) });
    expect(stage.loadAsset('/fixture.glb', [], '', profile.id)).toBe(first);
    expect(requests).toHaveLength(1);
    stage.dispose();
  });

  it('delivers the real failed-resource error to every same-key subscriber, including later subscribers', async () => {
    const { stage, profile, requests } = stageFixture();
    const first = stage.loadAsset('/broken.glb', [], '', profile.id);
    const replacement = stage.loadAsset('/broken.glb', [], '', profile.id);
    const results = Promise.allSettled([first, replacement]);
    const broken = model(); broken.userData.orbitpalResourceFailed = true;
    requests[0].result.resolve(broken);
    for (const result of await results) {
      expect(result.status).toBe('rejected');
      if (result.status === 'rejected') expect(String(result.reason)).toContain('nicht vollständig geladen');
    }
    expect(broken.disposed).toHaveBeenCalledOnce();
    expect(stage.getModelInfo().status).not.toBe('ready');
    await expect(stage.loadAsset('/broken.glb', [], '', profile.id)).rejects.toThrow('nicht vollständig geladen');
    expect(requests).toHaveLength(1);
    stage.dispose();
  });

  it.each(['url', 'endpoint', 'bot', 'avatarBinding'] as const)('starts a distinct request for a changed %s and ignores the older completion', async (changed) => {
    const { stage, profile, requests, imported } = stageFixture();
    const first = stage.loadAsset('/first.glb', [], '', profile.id);
    const oldAbort = (stage as unknown as { motionAbort: AbortController }).motionAbort;
    if (changed === 'avatarBinding') profile.avatar.assetPath = 'other.glb';
    const current = stage.loadAsset(changed === 'url' ? '/second.glb' : '/first.glb', [], changed === 'endpoint' ? 'http://127.0.0.1/other' : '', changed === 'bot' ? 'other-bot' : profile.id);
    expect(current).not.toBe(first);
    expect(oldAbort.signal.aborted).toBe(true);
    expect(requests).toHaveLength(2);
    const stale = model(); requests[0].result.resolve(stale); await first;
    expect(stale.disposed).toHaveBeenCalledOnce();
    expect(imported.children).toHaveLength(0);
    expect(stage.getModelInfo().status).toBe('loading');
    requests[1].result.resolve(model()); await current;
    expect(stage.getModelInfo()).toMatchObject({ status: 'ready', avatarBinding: avatarBinding(profile) });
    expect(imported.children).toHaveLength(1);
    stage.dispose();
  });

  it('does not let a late old failure overwrite a ready replacement', async () => {
    const { stage, profile, requests } = stageFixture();
    const first = stage.loadAsset('/first.glb', [], '', profile.id);
    const current = stage.loadAsset('/second.glb', [], '', profile.id);
    requests[1].result.resolve(model()); await current;
    requests[0].result.reject(new Error('old request failed')); await first;
    expect(stage.getModelInfo().status).toBe('ready');
    expect(stage.loadAsset('/second.glb', [], '', profile.id)).toBe(current);
    stage.dispose();
  });

  it('disposes a late model after unmount without becoming ready or starting another request', async () => {
    const { stage, profile, requests, imported, audio } = stageFixture();
    const first = stage.loadAsset('/fixture.glb', [], '', profile.id);
    const replacement = stage.loadAsset('/fixture.glb', [], '', profile.id);
    const abort = (stage as unknown as { motionAbort: AbortController }).motionAbort;
    stage.dispose();
    expect(abort.signal.aborted).toBe(true);
    const late = model(); requests[0].result.resolve(late); await Promise.all([first, replacement]);
    expect(late.disposed).toHaveBeenCalledOnce();
    expect(audio.load).not.toHaveBeenCalled();
    expect(imported.children).toHaveLength(0);
    expect(stage.getModelInfo().status).not.toBe('ready');
    await expect(stage.loadAsset('/fixture.glb', [], '', profile.id)).rejects.toMatchObject({ name: 'AbortError' });
    expect(requests).toHaveLength(1);
  });
});
