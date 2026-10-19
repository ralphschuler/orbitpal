import { LoadingManager, type BufferGeometry, type WebGLRenderer } from 'three';
import { GLTFLoader, type GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { DRACOLoader } from 'three/addons/loaders/DRACOLoader.js';
import { KTX2Loader } from 'three/addons/loaders/KTX2Loader.js';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';
import { VRMLoaderPlugin } from '@pixiv/three-vrm';
import { disposeAvatarObject } from './avatar-resources';

// Assets come from the locked Three release (Basis invokers are precompiled) and checked before
// every build. Resolve against this application's origin, never a model URI.
export const AVATAR_DECODER_PATH = '/decoders/three-0.186.1/';


/** Detect inconsistent decoded Draco counts before the scene is presented.
 * This is a post-decode consistency check, not a WASM allocation sandbox.
 */
export function decodedDracoMatches(gltf: GLTF): boolean {
  let valid = true;
  gltf.scene?.traverse((object) => {
    const association = gltf.parser.associations?.get(object);
    if (association?.meshes === undefined || association.primitives === undefined) return;
    const primitive = gltf.parser.json.meshes?.[association.meshes]?.primitives?.[association.primitives];
    const draco = primitive?.extensions?.KHR_draco_mesh_compression;
    if (!draco) return;
    const geometry = (object as import('three').Mesh).geometry;
    const count = geometry?.getAttribute('position')?.count;
    if (!Number.isSafeInteger(count) || !count || count < 0) { valid = false; return; }
    for (const name of Object.keys(draco.attributes || {})) {
      const accessor = gltf.parser.json.accessors?.[primitive.attributes?.[name]];
      if (accessor?.count !== count) valid = false;
    }
    if (primitive.indices !== undefined && gltf.parser.json.accessors?.[primitive.indices]?.count !== geometry.index?.count) valid = false;
  });
  return valid;
}

export const AVATAR_LOAD_TIMEOUT_MS = 60_000;
const cancelled = () => new Error('Das Laden des Modells wurde abgebrochen.');
type PrivateDraco = DRACOLoader & { decodeGeometry: (buffer: ArrayBuffer, config: Record<string, unknown>) => Promise<BufferGeometry>; _getWorker?: (...args: number[]) => Promise<Worker>; decoderPending?: Promise<unknown>; workerPool?: Worker[]; };
type PrivateKtx = KTX2Loader & { transcoderPending?: Promise<unknown>; workerSourceURL?: string; workerPool?: { postMessage: (...args: any[]) => Promise<any>; workers: Worker[]; } };

/** One codec generation per stage, reused until a fault or final disposal.
 * Caller promises are explicitly rejected before worker termination. Three's
 * own worker promises need not settle after a crash; late scenes still get
 * disposed and a later load receives fresh codecs.
 */
export function createAvatarLoader(renderer: WebGLRenderer) {
  let closed = false;
  function generation() {
    const draco = new DRACOLoader()
      .setDecoderPath({ js: AVATAR_DECODER_PATH + 'draco/gltf/draco_wasm_wrapper.js', wasm: AVATAR_DECODER_PATH + 'draco/gltf/draco_decoder.wasm' })
      .setWorkerLimit(1) as PrivateDraco;
    const ktx2 = new KTX2Loader().setTranscoderPath(AVATAR_DECODER_PATH + 'basis/').setWorkerLimit(1).detectSupport(renderer) as PrivateKtx;
    const tasks = new Set<(error: Error) => void>();
    let retired = false;
    const watched = new WeakSet<Worker>();
    const stop = (error: Error) => {
      if (retired) return;
      retired = true;
      for (const reject of [...tasks]) reject(error);
      draco.dispose();
      // KTX2 increments its global active-loader count only when init starts.
      // Dispose once; a late init may create a Blob URL, but cannot post to the
      // retired pool. Avoid double-dispose and its global counter decrement.
      if (ktx2.transcoderPending) {
        ktx2.dispose();
        void ktx2.transcoderPending.then(() => { if (ktx2.workerSourceURL) URL.revokeObjectURL(ktx2.workerSourceURL); }, () => {});
      }
      if (draco.decoderPending) void draco.decoderPending.then(() => draco.dispose(), () => {});
    };
    const watch = (worker: Worker) => {
      if (watched.has(worker)) return;
      watched.add(worker);
      const fail = () => stop(new Error('Der Modell-Decoder ist fehlgeschlagen. Bitte die Datei erneut laden.'));
      worker.addEventListener('error', fail); worker.addEventListener('messageerror', fail);
    };
    // These pinned Three internals are the task boundaries missing from its
    // worker APIs. They prevent late init continuations creating live workers
    // after dispose, and propagate crashes to all callers of this generation.
    const getWorker = draco._getWorker?.bind(draco);
    if (getWorker) draco._getWorker = (...args) => retired ? Promise.reject(cancelled()) : getWorker(...args).then((worker) => {
      if (retired) { worker.terminate(); throw cancelled(); }
      watch(worker); return worker;
    });
    const pool = ktx2.workerPool;
    if (pool) {
      const post = pool.postMessage.bind(pool);
      pool.postMessage = (...args) => {
        if (retired) return Promise.reject(cancelled());
        const result = post(...args); pool.workers.forEach(watch); return result;
      };
    }
    return { draco, ktx2, tasks, stop, get retired() { return retired; } };
  }
  let current = generation();
  let sequence = 0;
  return {
    loadAsync(url: string): Promise<GLTF> {
      if (closed) return Promise.reject(cancelled());
      if (current.retired) current = generation();
      const codecs = current;
      const token = String(++sequence);
      // GLTFLoader uses one proxy for this model's primitives while the actual
      // codec/worker remains reusable. Only our trusted adapter supplies tokens.
      const modelDraco = Object.create(codecs.draco) as PrivateDraco;
      modelDraco.preload = () => { codecs.draco.preload(); return modelDraco; };
      modelDraco.decodeGeometry = (buffer, config) => codecs.draco.decodeGeometry(buffer, { ...config, orbitpalBudgetToken: token } as typeof config);
      return new Promise<GLTF>((resolve, reject) => {
        let finished = false, scene: GLTF['scene'] | undefined;
        const disposeLate = () => { if (scene) { disposeAvatarObject(scene); scene = undefined; } };
        const complete = () => { finished = true; clearTimeout(timer); codecs.tasks.delete(abort); for (const worker of codecs.draco.workerPool || []) worker.postMessage({ type: 'orbitpal-release-budget', token }); };
        const abort = (error: Error) => { if (finished) return; complete(); disposeLate(); reject(error); };
        const timer = setTimeout(() => codecs.stop(new Error('Das Modell konnte nicht innerhalb von 60 Sekunden geladen werden. Bitte Datei und Decoder prüfen.')), AVATAR_LOAD_TIMEOUT_MS);
        codecs.tasks.add(abort);
        let resourceFailed = false;
        const manager = new LoadingManager(); manager.onError = () => { resourceFailed = true; };
        const loader = new GLTFLoader(manager).setDRACOLoader(modelDraco).setMeshoptDecoder(MeshoptDecoder).setKTX2Loader(codecs.ktx2);
        loader.register((parser) => new VRMLoaderPlugin(parser));
        void loader.loadAsync(url).then(async (gltf) => {
          scene = gltf.scene;
          if (finished) { disposeLate(); return; }
          const textures = await gltf.parser.getDependencies('texture').catch(() => { resourceFailed = true; return []; });
          if (finished) { disposeLate(); return; }
          if (resourceFailed || !decodedDracoMatches(gltf) || textures.some((texture) => !texture)) gltf.userData.orbitpalResourceFailed = true;
          complete(); scene = undefined; resolve(gltf);
        }).catch((error) => { if (!finished) codecs.stop(error instanceof Error ? error : new Error(String(error))); });
      });
    },
    dispose() { if (!closed) { closed = true; current.stop(cancelled()); } },
  };
}
