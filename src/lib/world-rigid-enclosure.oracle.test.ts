// SPDX-License-Identifier: MIT
// Independent containment oracle for an explicitly conservative policy. The
// exact-query equality oracles remain separate. These are CPU Three fixtures,
// not evidence of rendered pixels, native admission, arrival or frame timing.
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { RigidSupportPreparation, type RigidRange, type RigidWork } from './world-rigid-support';

type Bounds = { min: readonly number[]; max: readonly number[] };
type Submitted = { world: number[][]; ndc: number[][] };
function ledger(quota = Infinity): RigidWork & { vertices: number; operations: number; checks: number } {
  const work = { vertices: 0, operations: 0, checks: 0,
    vertex() { work.vertices++; }, operation(units: number) { work.operations += units; },
    check() { work.checks++; }, shouldYield() { return work.operations >= quota; } };
  return work;
}
function meshOf(points: readonly number[], indices?: readonly number[]): THREE.Mesh {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(new Float64Array(points), 3));
  if (indices) geometry.setIndex([...indices]);
  const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial()); mesh.updateMatrixWorld(true); return mesh;
}
function fullRange(mesh: THREE.Mesh): RigidRange {
  return { start: 0, end: mesh.geometry.index?.count ?? mesh.geometry.attributes.position.count };
}
function prepare(owner: RigidSupportPreparation, mesh: THREE.Mesh, range = fullRange(mesh)): void {
  expect(['preparing', 'ready']).toContain(owner.request(mesh, range));
  for (let tick = 0; tick < 10_000; tick++) {
    const status = owner.prepare(mesh, range, ledger(256));
    if (status === 'ready') return;
    expect(status).toBe('preparing');
  }
  throw Error('bounded fixture preparation did not finish');
}
function cameraMatrix(): THREE.Matrix4 {
  const camera = new THREE.OrthographicCamera(-9, 9, 7, -7, .1, 1000);
  camera.position.set(7, 11, 17); camera.lookAt(0, 0, 0); camera.updateMatrixWorld(true);
  return new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
}
function submitted(mesh: THREE.Mesh, range: RigidRange, pv: THREE.Matrix4): Submitted {
  const values: Submitted = { world: [], ndc: [] }, point = new THREE.Vector3();
  // Intentionally enumerate every original raw submitted index and execute the
  // real Three point graph in sequence; no prepared root box/helper is reused.
  for (let raw = range.start; raw < range.end; raw++) {
    const index = mesh.geometry.index ? mesh.geometry.index.getX(raw) : raw;
    mesh.getVertexPosition(index, point).applyMatrix4(mesh.matrixWorld);
    values.world.push(point.toArray()); values.ndc.push(point.clone().applyMatrix4(pv).toArray());
  }
  return values;
}
function floatOrder(a: number, b: number): number {
  if (Number.isNaN(a) || Number.isNaN(b)) throw Error('NaN cannot be an ordered oracle bound');
  if (a < b) return -1; if (a > b) return 1;
  return a === 0 && b === 0 && !Object.is(a, b) ? Object.is(a, -0) ? -1 : 1 : 0;
}
function contains(bounds: Bounds, points: readonly (readonly number[])[]): void {
  for (let axis = 0; axis < 3; axis++) {
    expect(Number.isFinite(bounds.min[axis]) && Number.isFinite(bounds.max[axis])).toBe(true);
    expect(floatOrder(bounds.min[axis], bounds.max[axis])).toBeLessThanOrEqual(0);
    for (const point of points) {
      expect(Number.isFinite(point[axis])).toBe(true);
      expect(floatOrder(bounds.min[axis], point[axis])).toBeLessThanOrEqual(0);
      expect(floatOrder(bounds.max[axis], point[axis])).toBeGreaterThanOrEqual(0);
    }
  }
}
function enclose(owner: RigidSupportPreparation, mesh: THREE.Mesh, range = fullRange(mesh), pv = cameraMatrix()) {
  const expected = submitted(mesh, range, pv), work = ledger(), before = owner.stats();
  const result = owner.enclose(mesh, range, pv, work);
  expect(result.status).toBe('ready');
  if (result.status !== 'ready') throw Error('expected an available conservative enclosure');
  contains(result.world, expected.world); contains(result.ndc, expected.ndc);
  // Preparation inspected source coordinates; this draw only transforms their
  // enclosing box. It must not call those current leaf evaluations or advertise
  // them as current-pose vertex work.
  expect(work.vertices).toBe(0); expect(work.operations).toBeGreaterThan(0); expect(work.checks).toBeGreaterThan(0);
  expect(owner.stats()).toEqual(before);
  return { result, expected, work };
}
function union(a: Bounds, b: Bounds): Bounds {
  return { min: a.min.map((v, i) => Math.min(v, b.min[i])), max: a.max.map((v, i) => Math.max(v, b.max[i])) };
}
function extrema(points: readonly (readonly number[])[]): Bounds {
  return { min: [0, 1, 2].map(axis => Math.min(...points.map(point => point[axis]))),
    max: [0, 1, 2].map(axis => Math.max(...points.map(point => point[axis]))) };
}

describe('explicit rigid submitted-range enclosures', () => {
  it('encloses disjoint indexed groups without including unsubmitted source holes', () => {
    const mesh = meshOf([-2, 0, 0, 2, 0, 0, 0, 3, 0, 50_000, 50_000, 50_000,
      4, -1, 1, 5, -1, 1, 4, 1, 1], [0, 1, 2, 3, 3, 3, 4, 5, 6]);
    mesh.material = [new THREE.MeshBasicMaterial(), new THREE.MeshBasicMaterial()];
    mesh.geometry.addGroup(0, 3, 0); mesh.geometry.addGroup(6, 3, 1);
    const first = { start: 0, end: 3 }, second = { start: 6, end: 9 }, owner = new RigidSupportPreparation();
    prepare(owner, mesh, first); prepare(owner, mesh, second);
    const a = enclose(owner, mesh, first, new THREE.Matrix4()), b = enclose(owner, mesh, second, new THREE.Matrix4());
    expect(a.result.world.max[0]).toBeLessThan(3); expect(b.result.world.min[0]).toBeGreaterThan(3);
    expect(a.result.world.max[1]).toBeLessThan(4); expect(b.result.world.max[1]).toBeLessThan(2);
  });
  it.each([0, Math.PI / 4, Math.PI / 2, -Math.PI / 3])('contains actual sequential Three points at current yaw %s', yaw => {
    const mesh = meshOf(Array.from({ length: 360 * 3 }, (_, i) => Math.sin(i * .731) * (i % 3 + 1)));
    const owner = new RigidSupportPreparation(); prepare(owner, mesh);
    mesh.position.set(1.25, -.7, 2.5); mesh.rotation.set(.23, yaw, -.41); mesh.scale.set(.3, 1.7, .65); mesh.updateMatrixWorld(true);
    const first = enclose(owner, mesh);
    mesh.rotation.y += .127; mesh.position.x -= .32; mesh.updateMatrixWorld(true);
    const second = enclose(owner, mesh);
    expect(second.result.world).not.toEqual(first.result.world);
  });
  it('uses each current parent/world transform when meshes share the same geometry', () => {
    const a = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, .3]), b = new THREE.Mesh(a.geometry, a.material), parent = new THREE.Group();
    parent.position.set(2, -.5, 1); parent.rotation.set(.2, -.4, .3); parent.scale.set(.8, 1.6, .5); parent.add(b);
    a.position.set(-3, 1, 0); b.position.set(3, 2, -1); a.updateMatrixWorld(true); parent.updateMatrixWorld(true);
    const owner = new RigidSupportPreparation(); prepare(owner, a); prepare(owner, b);
    const first = enclose(owner, a), second = enclose(owner, b);
    expect(second.result.world).not.toEqual(first.result.world);
  });
  it('reprojects a prepared range with a changed current camera rather than retaining old bounds', () => {
    const mesh = meshOf([-2, -.4, .2, 1, .3, -.8, 0, 2, .7]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const first = enclose(owner, mesh, fullRange(mesh), new THREE.Matrix4());
    const second = enclose(owner, mesh, fullRange(mesh), new THREE.Matrix4().makeRotationY(.47).setPosition(.2, -.1, .3));
    expect(second.result.world).toEqual(first.result.world); expect(second.result.ndc).not.toEqual(first.result.ndc);
  });
  it('preserves signed-zero containment for extrema from separate source blocks', () => {
    const points: number[] = []; for (let i = 0; i < 128; i++) points.push(i < 64 ? +0 : -0, -1, -1);
    const mesh = meshOf(points); mesh.matrixWorld.setPosition(-0, -0, -0);
    const owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const { result } = enclose(owner, mesh, fullRange(mesh), new THREE.Matrix4().setPosition(-0, -0, -0));
    expect(Object.is(result.world.min[0], -0)).toBe(true); expect(Object.is(result.world.max[0], +0)).toBe(true);
    expect(Object.is(result.ndc.min[0], -0)).toBe(true); expect(Object.is(result.ndc.max[0], +0)).toBe(true);
  });
  it('can conservatively fail clipping even though every actual point is inside the clip', () => {
    const mesh = meshOf([-.8, 0, 0, 0, .8, 0, 0, 0, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const pv = new THREE.Matrix4().makeRotationZ(Math.PI / 4), { result, expected } = enclose(owner, mesh, fullRange(mesh), pv);
    expect(expected.ndc.every(point => point.every(n => Math.abs(n) <= 1))).toBe(true);
    expect(result.ndc.min[0]).toBeLessThan(-1);
    const exact = owner.query(mesh, fullRange(mesh), pv, ledger());
    expect(exact.status).toBe('ready');
    if (exact.status !== 'ready') throw Error('the separately retained exact query must remain available');
    expect(exact.ndc.min[0]).toBeGreaterThan(-1); expect(result.ndc.min[0]).toBeLessThan(exact.ndc.min[0]);
  });
  it('unions an exact current morph subset with a rigid enclosure without treating morphs as rigid', () => {
    const rigid = meshOf([-1, 0, 0, 1, 0, 0, 0, 1, 0]), geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.Float32BufferAttribute([-.2, 0, 0, .2, 0, 0, 0, .3, 0], 3));
    geometry.morphAttributes.position = [new THREE.Float32BufferAttribute([-.4, 0, 0, .4, 0, 0, 0, 3, .2], 3)];
    const morph = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial()); morph.morphTargetInfluences![0] = 1; morph.updateMatrixWorld(true);
    const owner = new RigidSupportPreparation(); prepare(owner, rigid);
    expect(owner.enclose(morph, fullRange(morph), new THREE.Matrix4(), ledger()).status).toBe('unsupported');
    const a = enclose(owner, rigid, fullRange(rigid), new THREE.Matrix4()), currentMorph = submitted(morph, fullRange(morph), new THREE.Matrix4());
    const world = union(a.result.world, extrema(currentMorph.world)), ndc = union(a.result.ndc, extrema(currentMorph.ndc));
    contains(world, [...a.expected.world, ...currentMorph.world]); contains(ndc, [...a.expected.ndc, ...currentMorph.ndc]);
    expect(world.max[1]).toBeGreaterThan(2); // Current morph, not its neutral pose.
  });
  it('reports no actual leaf evaluations while retaining nonzero prepared-source accounting', () => {
    const mesh = meshOf(Array.from({ length: 3600 * 3 }, (_, i) => [-1, 0, 0, 1, 0, 0, 0, 2, .5][i % 9]));
    const owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const { work } = enclose(owner, mesh);
    expect(owner.stats().vertices).toBe(3600); expect(work.vertices).toBe(0);
  });
});

describe('conservative enclosure failure and generation boundaries', () => {
  it('emits no enclosure while any source preparation for that range is incomplete', () => {
    const mesh = meshOf(Array.from({ length: 300 }, (_, i) => Math.sin(i))), owner = new RigidSupportPreparation(), range = fullRange(mesh);
    expect(owner.request(mesh, range)).toBe('preparing'); expect(owner.prepare(mesh, range, ledger(1))).toBe('preparing');
    const work = ledger(); expect(owner.enclose(mesh, range, new THREE.Matrix4(), work).status).toBe('preparing'); expect(work.vertices).toBe(0);
  });
  const mutations: [string, (mesh: THREE.Mesh) => void][] = [
    ['position version', mesh => { mesh.geometry.attributes.position.needsUpdate = true; }],
    ['position storage', mesh => { const p = mesh.geometry.attributes.position as THREE.BufferAttribute; p.array = p.array.slice(); }],
    ['index version', mesh => { mesh.geometry.index!.needsUpdate = true; }],
    ['index replacement', mesh => { mesh.geometry.setIndex([2, 1, 0]); }],
    ['draw range', mesh => { mesh.geometry.setDrawRange(0, 2); }],
    ['material group', mesh => { mesh.geometry.addGroup(0, 3, 0); }],
    ['material replacement', mesh => { mesh.material = new THREE.MeshBasicMaterial(); }],
    ['new morph', mesh => { mesh.geometry.morphAttributes.position = [mesh.geometry.attributes.position.clone()]; }],
  ];
  it.each(mutations)('never publishes a stale enclosure after %s changes', (_name, mutate) => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0], [0, 1, 2]), range = fullRange(mesh), owner = new RigidSupportPreparation();
    prepare(owner, mesh, range); mutate(mesh);
    expect(owner.enclose(mesh, range, new THREE.Matrix4(), ledger()).status).toBe('invalid');
    expect(owner.request(mesh, range)).toBe('invalid');
  });
  it('rejects a source changed during enclosure evaluation instead of publishing a mixed-generation box', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const work = ledger(); let changed = false;
    work.check = () => { if (!changed) { changed = true; mesh.geometry.attributes.position.needsUpdate = true; } };
    expect(owner.enclose(mesh, fullRange(mesh), new THREE.Matrix4(), work).status).toBe('invalid');
    expect(owner.request(mesh, fullRange(mesh))).toBe('invalid');
  });
  it.each(['world', 'projection'] as const)('rejects a %s pose changed after the current query snapshot', target => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const pv = new THREE.Matrix4(), work = ledger(); let changed = false;
    work.check = () => { if (!changed) { changed = true; (target === 'world' ? mesh.matrixWorld : pv).elements[12] += .3; } };
    expect(owner.enclose(mesh, fullRange(mesh), pv, work).status).toBe('invalid');
    expect(owner.request(mesh, fullRange(mesh))).toBe('invalid');
  });
  it('rejects an invalid intermediate world before a projection could cancel the excessive translation', () => {
    const mesh = meshOf([-.2, 0, 0, .2, 0, 0, 0, .2, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    mesh.position.x = 1_100_000; mesh.updateMatrixWorld(true);
    const pv = new THREE.Matrix4().makeTranslation(-1_100_000, 0, 0);
    expect(submitted(mesh, fullRange(mesh), pv).ndc.every(point => point.every(n => Math.abs(n) < 1))).toBe(true);
    expect(owner.enclose(mesh, fullRange(mesh), pv, ledger()).status).toBe('invalid');
  });
  it('never returns ready for nonfinite sequential world arithmetic', () => {
    const mesh = meshOf([2, 0, 0, 3, 1, 0, 2, 0, 1]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    mesh.matrixWorld.elements[0] = Number.MAX_VALUE;
    const result = owner.enclose(mesh, fullRange(mesh), new THREE.Matrix4(), ledger());
    expect(['invalid', 'unsupported']).toContain(result.status);
  });
  it('never promotes a nonfinite source by retrying an invalidated owner', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, NaN, 2, 0]), owner = new RigidSupportPreparation(), range = fullRange(mesh);
    expect(() => prepare(owner, mesh, range)).toThrow('invalid_geometry');
    expect(owner.enclose(mesh, range, new THREE.Matrix4(), ledger()).status).toBe('invalid');
  });
  it('publishes no bounds for a nonfinite current projection', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const pv = new THREE.Matrix4(); pv.elements[12] = NaN;
    expect(['invalid', 'unsupported']).toContain(owner.enclose(mesh, fullRange(mesh), pv, ledger()).status);
  });
  it.each(['world', 'projection'] as const)('does not apply affine enclosures to a genuinely projective %s matrix', target => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const pv = new THREE.Matrix4(); (target === 'world' ? mesh.matrixWorld : pv).elements[3] = 1e-12;
    expect(owner.enclose(mesh, fullRange(mesh), pv, ledger()).status).toBe('unsupported');
  });
  it('keeps unsupported skin deformation on its separate current-pose path', () => {
    const source = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), mesh = new THREE.SkinnedMesh(source.geometry, source.material);
    expect(new RigidSupportPreparation().enclose(mesh, fullRange(mesh), new THREE.Matrix4(), ledger()).status).toBe('unsupported');
  });
  it('permanently retires an owner when the current frame ledger refuses its bounded work', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), owner = new RigidSupportPreparation(); prepare(owner, mesh);
    const work = ledger(); work.check = () => { throw Error('measurement_budget'); };
    expect(() => owner.enclose(mesh, fullRange(mesh), new THREE.Matrix4(), work)).toThrow('measurement_budget');
    expect(owner.enclose(mesh, fullRange(mesh), new THREE.Matrix4(), ledger()).status).toBe('invalid');
  });
});
