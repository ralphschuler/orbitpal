import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { createAvatarLoader } from '../../src/lib/avatar-loader';
import { AvatarStage } from '../../src/lib/avatar';

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

async function run() {
  const fixtures: string[] = await fetch('/fixture-list.json').then((response) => response.json());
  const results: { name: string; red: number }[] = [];
  for (const rgbaFallback of [false, true]) {
    const renderer = new THREE.WebGLRenderer({ alpha: true });
    renderer.setSize(64, 64); renderer.setClearColor(0, 0);
    if (rgbaFallback) {
      const has = renderer.extensions.has.bind(renderer.extensions);
      renderer.extensions.has = (name: any) => /compressed_texture/.test(name) ? false : has(name);
    }
    const loader = createAvatarLoader(renderer);
    try {
      for (const name of fixtures) {
        const gltf = await loader.loadAsync(name);
        if (gltf.userData.orbitpalResourceFailed) throw new Error('Missing fixture resources: ' + name);
        const meshes: THREE.Mesh[] = [];
        gltf.scene.traverse((node) => { if ((node as THREE.Mesh).isMesh) meshes.push(node as THREE.Mesh); });
        if (meshes.length !== 1 || meshes[0].geometry.getAttribute('position').count !== 3) throw new Error('Incorrect decoded geometry: ' + name);
        const texture = (meshes[0].material as THREE.MeshStandardMaterial).map;
        if (name.includes('/basisu') && (!texture || texture.image.width !== 4 || texture.image.height !== 4)) throw new Error('Missing transcoded texture');
        const scene = new THREE.Scene(); scene.add(gltf.scene);
        const camera = new THREE.OrthographicCamera(-1.3, 1.3, 2.3, -.3, .1, 10); camera.position.set(0, 0, 4);
        const red = redPixels(renderer, scene, camera);
        if (red < 500) throw new Error(`Unrendered geometry/texture: ${name} (${red} red pixels)`);
        results.push({ name: `${name}/${rgbaFallback ? 'RGBA fallback' : 'GPU format'}`, red });
        for (const mesh of meshes) { mesh.geometry.dispose(); texture?.dispose(); (mesh.material as THREE.Material).dispose(); }
      }
    } finally { loader.dispose(); renderer.dispose(); renderer.forceContextLoss(); }
  }
  // Native declaration checks cannot know Draco's encoded vertex count. The
  // original tiny stream must be rejected by the actual decoded-scene check.
  const negativeRenderer = new THREE.WebGLRenderer();
  const negativeLoader = createAvatarLoader(negativeRenderer);
  try {
    const invalid = await negativeLoader.loadAsync('/fixtures/invalid-draco-count.glb');
    if (!invalid.userData.orbitpalResourceFailed) throw new Error('Underdeclared Draco vertices were presented as valid');
    invalid.scene.traverse((object) => { const mesh=object as THREE.Mesh; if (mesh.isMesh) { mesh.geometry.dispose(); (mesh.material as THREE.Material).dispose(); } });
  } finally { negativeLoader.dispose(); negativeRenderer.dispose(); negativeRenderer.forceContextLoss(); }
  // A client without these decoders must still render valid optional core data.
  for (const name of ['meshopt-ext-optional.glb', 'meshopt-khr-optional.glb', 'basisu-optional.glb']) {
    const renderer = new THREE.WebGLRenderer({ alpha: true }); renderer.setSize(64,64);
    try {
      const gltf = await new GLTFLoader().loadAsync('/fixtures/' + name);
      const scene = new THREE.Scene(); scene.add(gltf.scene);
      const camera = new THREE.OrthographicCamera(-1.3,1.3,2.3,-.3,.1,10); camera.position.z=4;
      const red = redPixels(renderer,scene,camera); if (red<500) throw new Error('Invalid optional core fallback: '+name);
      results.push({name:name+'/no decoder core fallback',red});
      gltf.scene.traverse((node) => { const mesh=node as THREE.Mesh; if (mesh.isMesh) { mesh.geometry.dispose(); const material=mesh.material as THREE.MeshStandardMaterial; material.map?.dispose(); material.dispose(); }});
    } finally { renderer.dispose();renderer.forceContextLoss(); }
  }
  // The actual shared stage is used by Studio, transparent native overlay and
  // OBS. Exercise its real normalization/render/disposal as well as the factory.
  for (const transparent of [false, true]) {
    const host = document.createElement('div'); host.style.cssText = 'width:64px;height:64px'; document.body.append(host);
    const stage = new AvatarStage(host, { profile: { avatar: {} }, speaking: false, level: 0, emotion: 'neutral', transparent });
    try {
      for (const name of fixtures) {
        await stage.loadAsset(name);
        const state = stage as unknown as { renderer: THREE.WebGLRenderer; scene: THREE.Scene; camera: THREE.Camera };
        const red = redPixels(state.renderer, state.scene, state.camera);
        if (red < 50) throw new Error(`Stage did not render ${name}/${transparent}: ${red}`);
        results.push({ name: `${name}/${transparent ? 'overlay' : 'studio'}`, red });
      }
    } finally { stage.dispose(); host.remove(); }
  }
  if (violations.length) throw new Error('CSP violations: ' + violations.join(', '));
  return { results, violations };
}
(window as any).runAvatarDecoderFixtures = run;
