import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const state = vi.hoisted(() => ({ requests: [] as { manager: any; resolve: (value: any) => void }[], releases: [] as string[], capabilities: [] as unknown[], dracos: [] as any[], ktxs: [] as any[], adapters: [] as any[], decodes: [] as any[] }));
class FakeWorker extends EventTarget { terminate = vi.fn(); }
vi.mock('three/addons/loaders/DRACOLoader.js', () => ({ DRACOLoader: class {
  decoderPending?: Promise<unknown>; worker = new FakeWorker();
  constructor() { state.dracos.push(this); }
  _getWorker() { return Promise.resolve(this.worker); }
  preload() { return this; }
  decodeGeometry(_buffer: ArrayBuffer, config: unknown) { state.decodes.push(config); return Promise.resolve({}); }
  setDecoderPath() { return this; } setWorkerLimit() { return this; } dispose() { state.releases.push('draco'); this.worker.terminate(); }
} }));
vi.mock('three/addons/loaders/KTX2Loader.js', () => ({ KTX2Loader: class {
  transcoderPending?: Promise<unknown>; workerSourceURL?: string;
  workerPool = { workers: [new FakeWorker()], postMessage: vi.fn(async () => 1) };
  constructor() { state.ktxs.push(this); }
  setTranscoderPath() { return this; } setWorkerLimit() { return this; } detectSupport(renderer: unknown) { state.capabilities.push(renderer); return this; } dispose() { state.releases.push('ktx2'); this.workerPool.workers.forEach((worker) => worker.terminate()); }
} }));
vi.mock('three/addons/loaders/GLTFLoader.js', () => ({ GLTFLoader: class {
  constructor(private manager: unknown) {} setDRACOLoader(loader: unknown) { state.adapters.push(loader); return this; } setMeshoptDecoder() { return this; } setKTX2Loader() { return this; } register() { return this; }
  loadAsync() { return new Promise((resolve) => state.requests.push({ manager: this.manager, resolve })); }
} }));
vi.mock('three/addons/libs/meshopt_decoder.module.js', () => ({ MeshoptDecoder: {} }));
vi.mock('@pixiv/three-vrm', () => ({ VRMLoaderPlugin: class {} }));
import { AVATAR_LOAD_TIMEOUT_MS, createAvatarLoader } from './avatar-loader';
const gltf = (textures: unknown[] = []) => ({ userData: {} as Record<string, unknown>, scene: { traverse(_visitor?: any) {} }, parser: { getDependencies: async () => textures } });
const renderer = {} as Parameters<typeof createAvatarLoader>[0];
beforeEach(() => { state.requests.length = state.releases.length = state.capabilities.length = state.dracos.length = state.ktxs.length = state.adapters.length = state.decodes.length = 0; });
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });
describe('shared avatar decoder lifetime', () => {
  it('detects actual renderer capabilities and avoids double-disposing an uninitialized KTX2 counter', () => {
    const loader = createAvatarLoader(renderer); expect(state.capabilities).toEqual([renderer]); loader.dispose(); loader.dispose(); expect(state.releases).toEqual(['draco']);
  });
  it('rejects every pending caller before termination and disposes a scene that arrives late', async () => {
    const loader = createAvatarLoader(renderer); const first = loader.loadAsync('/old.glb'); const second = loader.loadAsync('/new.glb');
    const rejects = [expect(first).rejects.toThrow('abgebrochen'), expect(second).rejects.toThrow('abgebrochen')];
    loader.dispose(); await Promise.all(rejects); await expect(loader.loadAsync('/late.glb')).rejects.toThrow('abgebrochen');
    const dispose = vi.fn(); const late = gltf(); late.scene.traverse = (visit) => visit({ geometry: { dispose } });
    state.requests[0].resolve(late); state.requests[1].resolve(gltf()); await Promise.resolve(); await Promise.resolve(); expect(dispose).toHaveBeenCalledOnce();
  });
  it('keeps failed resource state per load even when another load starts', async () => {
    const loader = createAvatarLoader(renderer); const first = loader.loadAsync('/broken.glb'); state.requests[0].manager.onError('missing image'); const second = loader.loadAsync('/good.glb');
    state.requests[1].resolve(gltf([{}])); expect((await second).userData.orbitpalResourceFailed).toBeUndefined();
    state.requests[0].resolve(gltf()); expect((await first).userData.orbitpalResourceFailed).toBe(true); loader.dispose();
  });
  it('surfaces texture decode errors swallowed by GLTFLoader', async () => {
    const loader = createAvatarLoader(renderer); const result = loader.loadAsync('/bad-ktx.glb'); state.requests[0].resolve(gltf([null])); expect((await result).userData.orbitpalResourceFailed).toBe(true); loader.dispose();
  });
  it('bounds a decoder that never settles and gives retries fresh codecs', async () => {
    vi.useFakeTimers(); const loader = createAvatarLoader(renderer); const pending = loader.loadAsync('/stalled.glb'); const rejection = expect(pending).rejects.toThrow('60 Sekunden');
    await vi.advanceTimersByTimeAsync(AVATAR_LOAD_TIMEOUT_MS); await rejection;
    const retry = loader.loadAsync('/retry.glb'); expect(state.dracos).toHaveLength(2); state.requests[1].resolve(gltf()); await retry; loader.dispose();
  });
  it('cleans late initialization without reviving workers or double-decrementing KTX2 active count', async () => {
    const loader = createAvatarLoader(renderer); const pending = loader.loadAsync('/init.glb'); const rejection = expect(pending).rejects.toThrow('abgebrochen');
    let finish!: () => void; const init = new Promise<void>((resolve) => { finish = resolve; });
    const draco = state.dracos[0], ktx = state.ktxs[0]; draco.decoderPending = init; ktx.transcoderPending = init.then(() => { ktx.workerSourceURL = 'blob:late-fixture'; });
    const revoke = vi.spyOn(URL, 'revokeObjectURL'); loader.dispose(); await rejection; finish(); await init; await Promise.resolve(); await Promise.resolve();
    await expect(draco._getWorker(1, 1)).rejects.toThrow('abgebrochen'); await expect(ktx.workerPool.postMessage({})).rejects.toThrow('abgebrochen');
    expect(revoke).toHaveBeenCalledWith('blob:late-fixture'); expect(state.releases.filter((item) => item === 'ktx2')).toHaveLength(1);
  });
  for (const codec of ['draco','ktx'] as const) it(`propagates ${codec} worker crashes and recovers on retry`, async () => {
    const loader = createAvatarLoader(renderer); const pending = loader.loadAsync('/crash.glb'); const rejection = expect(pending).rejects.toThrow('Decoder');
    let worker: FakeWorker;
    if (codec === 'draco') worker = await state.dracos[0]._getWorker(1, 1);
    else { state.ktxs[0].transcoderPending = Promise.resolve(); await state.ktxs[0].workerPool.postMessage({}); worker = state.ktxs[0].workerPool.workers[0]; }
    worker.dispatchEvent(new Event('error')); await rejection; expect(worker.terminate).toHaveBeenCalled();
    const retry = loader.loadAsync('/retry.glb'); state.requests[1].resolve(gltf()); await retry; loader.dispose();
  });
  it('tags all primitives of a model with one trusted budget token and keeps concurrent models separate', async () => {
    const loader=createAvatarLoader(renderer);const first=loader.loadAsync('/one.glb'),second=loader.loadAsync('/two.glb');
    await state.adapters[0].decodeGeometry(new ArrayBuffer(1),{orbitpalBudgetToken:'untrusted'});
    await state.adapters[0].decodeGeometry(new ArrayBuffer(1),{});await state.adapters[1].decodeGeometry(new ArrayBuffer(1),{});
    expect(state.decodes[0].orbitpalBudgetToken).toBe(state.decodes[1].orbitpalBudgetToken);expect(state.decodes[0].orbitpalBudgetToken).not.toBe(state.decodes[2].orbitpalBudgetToken);expect(state.decodes[0].orbitpalBudgetToken).not.toBe('untrusted');
    state.requests[0].resolve(gltf());state.requests[1].resolve(gltf());await Promise.all([first,second]);loader.dispose();
  });

});
