import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { createAvatarLoader } from '../../src/lib/avatar-loader';
import { AvatarStage } from '../../src/lib/avatar';
import { DECODER_PROGRESS_STAGE as S, encodeDecoderProgress } from '../../scripts/avatar-decoder-progress.mjs';

const NativeWorker = window.Worker;
window.Worker = class extends NativeWorker { constructor(url: string | URL, options?: WorkerOptions) { super(url, options); this.addEventListener('error', (error) => console.error('Decoder worker failed:', error.message)); } };

const violations: string[] = [];
document.addEventListener('securitypolicyviolation', (event) => violations.push(`${event.violatedDirective}: ${event.blockedURI}`));

function redPixels(renderer: THREE.WebGLRenderer, scene: THREE.Scene, camera: THREE.Camera) {
  renderer.render(scene, camera);
  const gl = renderer.getContext();
  const bytes = new Uint8Array(64 * 64 * 4);
  gl.readPixels(0, 0, 64, 64, gl.RGBA, gl.UNSIGNED_BYTE, bytes);
  let count = 0;
  for (let i = 0; i < bytes.length; i += 4) if (bytes[i] > 150 && bytes[i] > bytes[i + 1] * 1.5 && bytes[i] > bytes[i + 2] * 1.5 && bytes[i + 3] > 128) ++count;
  return count;
}

async function run(policyIndex: number) {
  const results: { name: string; red: number }[] = [];
  function progress(stage: number, after = false, fixtureOrdinal = 0, rgbaFallback = false, transparent = false) {
    const line = encodeDecoderProgress({ version: 1, stage, after, policyIndex, fixtureOrdinal, completedCount: results.length, rgbaFallback, transparent });
    if (line) console.info(line);
  }
  progress(S.list);
  const fixtures: string[] = await fetch('/fixture-list.json').then((response) => response.json());
  progress(S.list, true);
  for (const rgbaFallback of [false, true]) {
    progress(S.factoryConstruct, false, 0, rgbaFallback);
    const renderer = new THREE.WebGLRenderer({ alpha: true });
    renderer.setSize(64, 64); renderer.setClearColor(0, 0);
    if (rgbaFallback) {
      const has = renderer.extensions.has.bind(renderer.extensions);
      renderer.extensions.has = (name: any) => /compressed_texture/.test(name) ? false : has(name);
    }
    const loader = createAvatarLoader(renderer);
    progress(S.factoryConstruct, true, 0, rgbaFallback);
    try {
      for (const [index, name] of fixtures.entries()) {
        progress(S.factoryLoad, false, index + 1, rgbaFallback);
        const gltf = await loader.loadAsync(name);
        progress(S.factoryLoad, true, index + 1, rgbaFallback);
        if (gltf.userData.orbitpalResourceFailed) throw new Error('Missing fixture resources: ' + name);
        const meshes: THREE.Mesh[] = [];
        gltf.scene.traverse((node) => { if ((node as THREE.Mesh).isMesh) meshes.push(node as THREE.Mesh); });
        if (meshes.length !== 1 || meshes[0].geometry.getAttribute('position').count !== 3) throw new Error('Incorrect decoded geometry: ' + name);
        const texture = (meshes[0].material as THREE.MeshStandardMaterial).map;
        if (name.includes('/basisu') && (!texture || texture.image.width !== 4 || texture.image.height !== 4)) throw new Error('Missing transcoded texture');
        const scene = new THREE.Scene(); scene.add(gltf.scene);
        const camera = new THREE.OrthographicCamera(-1.3, 1.3, 2.3, -.3, .1, 10); camera.position.set(0, 0, 4);
        progress(S.factoryRender, false, index + 1, rgbaFallback);
        const red = redPixels(renderer, scene, camera);
        progress(S.factoryRender, true, index + 1, rgbaFallback);
        if (red < 500) throw new Error(`Unrendered geometry/texture: ${name} (${red} red pixels)`);
        results.push({ name: `${name}/${rgbaFallback ? 'RGBA fallback' : 'GPU format'}`, red });
        progress(S.factoryModelDispose, false, index + 1, rgbaFallback);
        for (const mesh of meshes) { mesh.geometry.dispose(); texture?.dispose(); (mesh.material as THREE.Material).dispose(); }
        progress(S.factoryModelDispose, true, index + 1, rgbaFallback);
      }
    } finally {
      progress(S.factoryDispose, false, 0, rgbaFallback);
      loader.dispose(); renderer.dispose(); renderer.forceContextLoss();
      progress(S.factoryDispose, true, 0, rgbaFallback);
    }
  }
  // Native declaration checks cannot know Draco's encoded vertex count. The
  // original tiny stream must be rejected by the actual decoded-scene check.
  progress(S.negativeConstruct);
  const negativeRenderer = new THREE.WebGLRenderer();
  const negativeLoader = createAvatarLoader(negativeRenderer);
  progress(S.negativeConstruct, true);
  try {
    progress(S.negativeLoad, false, 1);
    const invalid = await negativeLoader.loadAsync('/fixtures/invalid-draco-count.glb');
    progress(S.negativeLoad, true, 1);
    if (!invalid.userData.orbitpalResourceFailed) throw new Error('Underdeclared Draco vertices were presented as valid');
    progress(S.negativeModelDispose, false, 1);
    invalid.scene.traverse((object) => { const mesh=object as THREE.Mesh; if (mesh.isMesh) { mesh.geometry.dispose(); (mesh.material as THREE.Material).dispose(); } });
    progress(S.negativeModelDispose, true, 1);
  } finally {
    progress(S.negativeDispose);
    negativeLoader.dispose(); negativeRenderer.dispose(); negativeRenderer.forceContextLoss();
    progress(S.negativeDispose, true);
  }
  // A client without these decoders must still render valid optional core data.
  for (const [index, name] of ['meshopt-ext-optional.glb', 'meshopt-khr-optional.glb', 'basisu-optional.glb'].entries()) {
    progress(S.optionalConstruct, false, index + 1);
    const renderer = new THREE.WebGLRenderer({ alpha: true }); renderer.setSize(64,64);
    progress(S.optionalConstruct, true, index + 1);
    try {
      progress(S.optionalLoad, false, index + 1);
      const gltf = await new GLTFLoader().loadAsync('/fixtures/' + name);
      progress(S.optionalLoad, true, index + 1);
      const scene = new THREE.Scene(); scene.add(gltf.scene);
      const camera = new THREE.OrthographicCamera(-1.3,1.3,2.3,-.3,.1,10); camera.position.z=4;
      progress(S.optionalRender, false, index + 1);
      const red = redPixels(renderer,scene,camera);
      progress(S.optionalRender, true, index + 1);
      if (red<500) throw new Error('Invalid optional core fallback: '+name);
      results.push({name:name+'/no decoder core fallback',red});
      progress(S.optionalModelDispose, false, index + 1);
      gltf.scene.traverse((node) => { const mesh=node as THREE.Mesh; if (mesh.isMesh) { mesh.geometry.dispose(); const material=mesh.material as THREE.MeshStandardMaterial; material.map?.dispose(); material.dispose(); }});
      progress(S.optionalModelDispose, true, index + 1);
    } finally {
      progress(S.optionalDispose, false, index + 1);
      renderer.dispose();renderer.forceContextLoss();
      progress(S.optionalDispose, true, index + 1);
    }
  }
  // The actual shared stage is used by Studio, transparent native overlay and
  // OBS. Exercise its real normalization/render/disposal as well as the factory.
  for (const transparent of [false, true]) {
    progress(S.stageConstruct, false, 0, false, transparent);
    const host = document.createElement('div'); host.style.cssText = 'width:64px;height:64px'; document.body.append(host);
    const stage = new AvatarStage(host, { profile: { avatar: {} }, speaking: false, level: 0, emotion: 'neutral', transparent });
    progress(S.stageConstruct, true, 0, false, transparent);
    try {
      for (const [index, name] of fixtures.entries()) {
        progress(S.stageLoad, false, index + 1, false, transparent);
        await stage.loadAsset(name);
        progress(S.stageLoad, true, index + 1, false, transparent);
        const state = stage as unknown as { renderer: THREE.WebGLRenderer; scene: THREE.Scene; camera: THREE.Camera };
        progress(S.stageRender, false, index + 1, false, transparent);
        const red = redPixels(state.renderer, state.scene, state.camera);
        progress(S.stageRender, true, index + 1, false, transparent);
        if (red < 50) throw new Error(`Stage did not render ${name}/${transparent}: ${red}`);
        results.push({ name: `${name}/${transparent ? 'overlay' : 'studio'}`, red });
      }
    } finally {
      progress(S.stageDispose, false, 0, false, transparent);
      stage.dispose(); host.remove();
      progress(S.stageDispose, true, 0, false, transparent);
    }
  }
  if (violations.length) throw new Error('CSP violations: ' + violations.join(', '));
  progress(S.complete, true);
  return { results, violations };
}
(window as any).runAvatarDecoderFixtures = run;
