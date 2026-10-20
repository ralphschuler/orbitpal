import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { MToonMaterial } from '@pixiv/three-vrm';
import { configureDesktopCamera } from './desktop-framing';
import { mapReachableCanonical, WorldRenderGeometry, GEOMETRY_MAX_NODES, GEOMETRY_MAX_VERTEX_VISITS, GEOMETRY_MAX_DRAW_GROUPS,
  type GeometryBinding, type GeometryFrame, type GeometryResult, type RenderPort, type SurfaceObservation } from './world-render-geometry';

// Real Three CPU geometry/morph/skin/mixer, with a SYNTHETIC draw callback port.
// These tests neither create WebGL nor prove pixels, native visibility or arrival.
const binding = (patch: Partial<GeometryBinding> = {}): GeometryBinding => ({ actorId: 'nova', sourceWorldId: 'life-nova',
  renderSessionId: 'render-1', bindingSequence: 1, worldInstanceId: 'world-1', stageId: 'stage-1', configRevision: 1,
  topologyRevision: 1, layoutRevision: 1, assetRevision: 1, measurementRevision: 1, scale: 1, ...patch });
function triangle() {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute([-.5, 0, 0, .5, 0, 0, 0, 1, 0], 3));
  return geometry;
}
function fixture(mesh: THREE.Object3D = new THREE.Mesh(triangle(), new THREE.MeshBasicMaterial())) {
  const scene = new THREE.Scene(), root = new THREE.Group(); root.add(mesh); scene.add(root);
  const camera = new THREE.OrthographicCamera(-10, 10, 5, -5, .05, 100); configureDesktopCamera(camera);
  const probe = new WorldRenderGeometry(); let frame: GeometryFrame = { binding: binding(), frameSequence: 1 };
  let surface: SurfaceObservation = { connected: true, visible: true, viewport: { width: 200, height: 100 }, clip: { left: 0, top: 0, right: 200, bottom: 100 } };
  const state = { lost: false, draw: true, throwAfterDraw: false, scissor: false, target: false, renders: 0,
    beforeDraw: () => {}, afterDraw: () => {}, viewport: new THREE.Vector4(0, 0, 200, 100), size: new THREE.Vector2(200, 100) };
  const port: RenderPort = {
    getContext: () => ({ isContextLost: () => state.lost }), getScissorTest: () => state.scissor,
    getRenderTarget: () => state.target ? new THREE.WebGLRenderTarget(1, 1) : null,
    getSize: target => target.copy(state.size), getViewport: target => target.copy(state.viewport),
    render(s, c) {
      state.renders++; state.beforeDraw(); s.updateMatrixWorld(true); c.updateMatrixWorld(true);
      if (state.draw) {
        const visit = (node: THREE.Object3D) => {
          if (!node.visible) return;
          if (node instanceof THREE.Mesh && c.layers.test(node.layers)) {
            const materials = node.material;
            const entries = Array.isArray(materials)
              ? node.geometry.groups.map(group => ({ material: materials[group.materialIndex!], group }))
              : [{ material: materials, group: null }];
            for (const { material, group } of entries) if (material?.visible) {
              node.onAfterRender(port as unknown as THREE.WebGLRenderer, s as THREE.Scene, c, node.geometry, material, group as unknown as Parameters<THREE.Mesh['onAfterRender']>[5]);
            }
          }
          for (const child of node.children) visit(child);
        };
        visit(root);
      }
      state.afterDraw(); if (state.throwAfterDraw) throw new Error('synthetic render failure');
    },
  };
  expect(probe.bind(frame.binding, root, camera, new THREE.Matrix4())).toBe(true);
  return { probe, root, scene, camera, state, mesh, port,
    frame: () => frame, setFrame: (next: GeometryFrame) => { frame = next; }, surface: () => surface,
    setSurface: (next: SurfaceObservation) => { surface = next; },
    run: (clock = () => 0) => probe.renderFrame(frame, scene, port, () => surface, () => frame, clock),
  };
}
function measurement(result: GeometryResult) {
  expect(result.available).toBe(true);
  if (!result.available) throw new Error(result.reason);
  return result.measurement;
}

describe('post-draw posed geometry (synthetic render port)', () => {
  it('publishes immutable current posed bounds only after an actual submitted callback', () => {
    const f = fixture(); const m = measurement(f.run());
    expect(m.posedBounds.left).toBeCloseTo(95); expect(m.posedBounds.right).toBeCloseTo(105);
    expect(m.maximumFootprint.width).toBeCloseTo(10); expect(m.meshes).toBe(1); expect(m.vertices).toBe(3);
    expect(Object.isFrozen(m)).toBe(true); expect(Object.isFrozen(m.posedBounds)).toBe(true);
    expect((f.mesh as THREE.Mesh).onAfterRender).toBe(THREE.Object3D.prototype.onAfterRender);
    const absent = fixture(); absent.state.draw = false;
    expect(absent.run()).toEqual({ available: false, reason: 'empty_geometry' });
  });

  it('reads current morph and skeletal animation, ignoring stale cached neutral boxes', () => {
    const geometry = triangle(); geometry.morphTargetsRelative = true;
    geometry.morphAttributes.position = [new THREE.Float32BufferAttribute([0, 0, 0, 0, 0, 0, 0, 1, 0], 3)];
    geometry.setAttribute('skinIndex', new THREE.Uint16BufferAttribute(new Array(12).fill(0), 4));
    geometry.setAttribute('skinWeight', new THREE.Float32BufferAttribute([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 4));
    const mesh = new THREE.SkinnedMesh(geometry, new THREE.MeshBasicMaterial());
    const bone = new THREE.Bone(); bone.name = 'movingBone'; mesh.add(bone); mesh.bind(new THREE.Skeleton([bone]));
    mesh.computeBoundingBox(); const oldBox = mesh.boundingBox!.clone();
    const f = fixture(mesh); const before = measurement(f.run());
    const clip = new THREE.AnimationClip('authored pose', 1, [
      new THREE.NumberKeyframeTrack('.morphTargetInfluences[0]', [0, 1], [0, 1]),
      new THREE.VectorKeyframeTrack('movingBone.position', [0, 1], [0, 0, 0, .25, 0, 0]),
    ]);
    const mixer = new THREE.AnimationMixer(mesh); const action = mixer.clipAction(clip); action.setLoop(THREE.LoopOnce, 1); action.clampWhenFinished = true; action.play();
    mixer.update(1); f.setFrame({ ...f.frame(), frameSequence: 2 }); const after = measurement(f.run());
    expect(mesh.boundingBox!.equals(oldBox)).toBe(true);
    expect(after.posedBounds.left - before.posedBounds.left).toBeCloseTo(2.5);
    expect(after.posedBounds.top).toBeLessThan(before.posedBounds.top - 8);
    expect(after.maximumFootprint.height).toBeGreaterThan(before.maximumFootprint.height);
  });

  it('honours submitted material groups, visibility, camera layers and drawRange', () => {
    const geometry = triangle(); geometry.setAttribute('position', new THREE.Float32BufferAttribute([
      -.5, 0, 0, .5, 0, 0, 0, 1, 0, 1000, 0, 0, 1001, 0, 0, 1000, 1, 0,
    ], 3)); geometry.addGroup(0, 3, 0); geometry.addGroup(3, 3, 1); geometry.setDrawRange(0, 3);
    const mesh = new THREE.Mesh(geometry, [new THREE.MeshBasicMaterial(), new THREE.MeshBasicMaterial({ visible: false })]);
    const hidden = new THREE.Mesh(new THREE.BoxGeometry(1000, 1000, 1000), new THREE.MeshBasicMaterial()); hidden.visible = false; mesh.add(hidden);
    const wrongLayer = new THREE.Mesh(hidden.geometry, hidden.material); wrongLayer.layers.set(2); mesh.add(wrongLayer);
    expect(measurement(fixture(mesh).run()).vertices).toBe(3);
  });

  it('keeps alpha-cutout geometry conservative without claiming visible pixels', () => {
    const mesh = new THREE.Mesh(triangle(), new THREE.MeshBasicMaterial({ alphaTest: .9, transparent: true }));
    const m = measurement(fixture(mesh).run()); expect(m.maximumFootprint.width).toBeCloseTo(10);
    expect(Object.hasOwn(m, 'visiblePixels')).toBe(false); expect(Object.hasOwn(m, 'arrived')).toBe(false);
  });

  it('derives both inverse camera/ground axes from actual orthographic matrices and model scale', () => {
    const f = fixture(); const first = measurement(f.run());
    expect(first.groundBasis.right.x).toBeCloseTo(.1); expect(first.groundBasis.right.z).toBeCloseTo(0);
    expect(first.groundBasis.down.x).toBeCloseTo(0); expect(first.groundBasis.down.z).toBeCloseTo(.2);
    const frame = { binding: binding({ bindingSequence: 2, measurementRevision: 2, scale: 2 }), frameSequence: 2 };
    expect(f.probe.bind(frame.binding, f.root, f.camera, new THREE.Matrix4().makeScale(2, 2, 2))).toBe(true); f.setFrame(frame);
    const next = measurement(f.run()); expect(next.groundBasis.right.x).toBeCloseTo(.05); expect(next.groundBasis.down.z).toBeCloseTo(.1);
  });

  it.each(['throw', 'context loss', 'surface change', 'attachment change', 'invalidate'] as const)('discards submitted observations after %s and restores hooks', mode => {
    const f = fixture();
    if (mode === 'throw') f.state.throwAfterDraw = true;
    if (mode === 'context loss') f.state.afterDraw = () => { f.state.lost = true; };
    if (mode === 'surface change') f.state.afterDraw = () => { (f.surface().viewport as { width: number }).width = 180; };
    if (mode === 'attachment change') f.state.afterDraw = () => { f.setFrame({ binding: binding({ assetRevision: 2 }), frameSequence: 1 }); };
    if (mode === 'invalidate') f.state.afterDraw = () => f.probe.invalidate();
    expect(f.run().available).toBe(false);
    expect((f.mesh as THREE.Mesh).onAfterRender).toBe(THREE.Object3D.prototype.onAfterRender);
  });

  it('consumes failed frame sequences and rejects stale source/layout registration', () => {
    const f = fixture(); const old = f.frame(); f.state.throwAfterDraw = true; expect(f.run().available).toBe(false);
    f.state.throwAfterDraw = false; expect(f.run()).toEqual({ available: false, reason: 'stale_frame' });
    expect(f.probe.bind(binding(), f.root, f.camera, new THREE.Matrix4())).toBe(false);
    const next = { binding: binding({ bindingSequence: 2, sourceWorldId: 'new-lineage', layoutRevision: 2 }), frameSequence: 2 };
    expect(f.probe.bind(next.binding, f.root, f.camera, new THREE.Matrix4())).toBe(true); f.setFrame(next);
    expect(f.probe.renderFrame(old, f.scene, f.port, f.surface, () => next, () => 0)).toEqual({ available: false, reason: 'stale_frame' });
    expect(f.run().available).toBe(true);
  });

  it.each(['perspective', 'shader', 'outline', 'displacement', 'instanced', 'custom hook'] as const)('fails honestly for unsupported %s', mode => {
    let mesh: THREE.Mesh = new THREE.Mesh(triangle(), new THREE.MeshBasicMaterial());
    if (mode === 'shader') mesh.material = new THREE.ShaderMaterial();
    if (mode === 'outline') { const material = new MToonMaterial(); material.isOutline = true; mesh.material = material; }
    if (mode === 'displacement') mesh.material = new THREE.MeshStandardMaterial({ displacementMap: new THREE.Texture() });
    if (mode === 'instanced') mesh = new THREE.InstancedMesh(triangle(), new THREE.MeshBasicMaterial(), 1);
    if (mode === 'custom hook') mesh.onBeforeRender = () => {};
    const f = fixture(mesh);
    if (mode === 'perspective') { const camera = new THREE.PerspectiveCamera(); expect(f.probe.bind(binding({ bindingSequence: 2 }), f.root, camera, new THREE.Matrix4())).toBe(true); f.setFrame({ binding: binding({ bindingSequence: 2 }), frameSequence: 2 }); }
    expect(f.run()).toEqual({ available: false, reason: mode === 'perspective' ? 'unsupported_camera' : 'unsupported_deformation' });
  });

  it('supports current MToon body morph/skin geometry, without promising outline support', () => {
    const material = new MToonMaterial(); material.isOutline = false;
    expect(measurement(fixture(new THREE.Mesh(triangle(), material)).run()).vertices).toBe(3);
  });

  it.each(['empty', 'nonfinite', 'offscreen', 'depth', 'hidden', 'zero viewport', 'scissor', 'offscreen target'] as const)('rejects %s without a placeholder footprint', mode => {
    const mesh = new THREE.Mesh(mode === 'empty' ? new THREE.BufferGeometry() : triangle(), new THREE.MeshBasicMaterial());
    const f = fixture(mesh);
    if (mode === 'nonfinite') mesh.geometry.attributes.position.setX(0, NaN);
    if (mode === 'offscreen') mesh.position.x = 100;
    if (mode === 'depth') mesh.position.z = 200;
    if (mode === 'hidden') f.setSurface({ ...f.surface(), visible: false });
    if (mode === 'zero viewport') f.setSurface({ ...f.surface(), viewport: { width: 0, height: 100 } });
    if (mode === 'scissor') f.state.scissor = true;
    if (mode === 'offscreen target') f.state.target = true;
    expect(f.run().available).toBe(false);
  });

  it('bounds dense trees, vertex work and measurement CPU clock without traversing unbounded data', () => {
    const tree = new THREE.Group(); for (let i = 0; i < GEOMETRY_MAX_NODES; i++) tree.add(new THREE.Group());
    const dense = fixture(tree); expect(dense.run()).toEqual({ available: false, reason: 'measurement_budget' }); expect(dense.state.renders).toBe(0);
    const geometry = new THREE.BufferGeometry(); geometry.setAttribute('position', new THREE.Float32BufferAttribute(new Float32Array((GEOMETRY_MAX_VERTEX_VISITS + 1) * 3), 3));
    expect(fixture(new THREE.Mesh(geometry, new THREE.MeshBasicMaterial())).run()).toEqual({ available: false, reason: 'measurement_budget' });
    let now = 0; expect(fixture().run(() => now += 4)).toEqual({ available: false, reason: 'measurement_budget' });
  });

  it('rejects excessive draw groups before the renderer can iterate them', () => {
    const geometry = triangle();
    for (let i = 0; i <= GEOMETRY_MAX_DRAW_GROUPS; i++) geometry.addGroup(0, 3, 0);
    const f = fixture(new THREE.Mesh(geometry, [new THREE.MeshBasicMaterial()]));
    expect(f.run()).toEqual({ available: false, reason: 'measurement_budget' });
    expect(f.state.renders).toBe(0);
    expect((f.mesh as THREE.Mesh).onAfterRender).toBe(THREE.Object3D.prototype.onAfterRender);
  });

  it('maps canonical targets without clamping or pretending an unreachable slot was reached', () => {
    const m = measurement(fixture().run()); const safe = { min: { x: 0, y: 0 }, max: { x: 1_000_000, y: 1_000_000 } };
    expect(mapReachableCanonical({ x: 500_000, y: 500_000 }, safe, m, 8)?.target).toEqual({ x: 100, y: 50 });
    expect(mapReachableCanonical({ x: 0, y: 500_000 }, safe, m, 8)).toBeNull();
    expect(mapReachableCanonical({ x: NaN, y: 500_000 }, safe, m, 8)).toBeNull();
    expect(mapReachableCanonical({ x: 500_000, y: 500_000 }, { min: { x: 490_000, y: 490_000 }, max: { x: 510_000, y: 510_000 } }, m, 8)).toBeNull();
    expect(Object.hasOwn(m, 'complete')).toBe(false);
  });
});
