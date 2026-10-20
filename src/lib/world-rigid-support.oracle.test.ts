// SPDX-License-Identifier: MIT
// Independent exact arithmetic / exhaustive submitted-point oracle. These tests
// create CPU Three objects only; they do not establish WebGL or native arrival.
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { RigidSupportPreparation, nextDown, nextUp, outwardAdd, outwardMultiply,
  type RigidRange, type RigidWork } from './world-rigid-support';

type Dyadic = { n: bigint; e: number };
type Interval = readonly [number, number];

// Independent oracle: recover the exact mathematical value represented by a
// finite IEEE double, then perform integer arithmetic without floating rounding.
function exact(value: number): Dyadic {
  if (!Number.isFinite(value)) throw new Error('finite oracle input required');
  const bytes = new DataView(new ArrayBuffer(8));
  bytes.setFloat64(0, value, false);
  const bits = bytes.getBigUint64(0, false), exponent = Number((bits >> 52n) & 2047n);
  const fraction = bits & ((1n << 52n) - 1n), sign = bits >> 63n ? -1n : 1n;
  return { n: sign * (exponent ? (1n << 52n) + fraction : fraction),
    e: exponent ? exponent - 1023 - 52 : -1074 };
}
function addExact(a: Dyadic, b: Dyadic): Dyadic {
  const e = Math.min(a.e, b.e);
  return { n: (a.n << BigInt(a.e - e)) + (b.n << BigInt(b.e - e)), e };
}
function multiplyExact(a: Dyadic, b: Dyadic): Dyadic { return { n: a.n * b.n, e: a.e + b.e }; }
function compareExact(a: Dyadic, b: Dyadic): number {
  const e = Math.min(a.e, b.e), difference = (a.n << BigInt(a.e - e)) - (b.n << BigInt(b.e - e));
  return difference < 0n ? -1 : difference > 0n ? 1 : 0;
}
function encloses(interval: Interval, value: Dyadic): void {
  expect(Number.isNaN(interval[0]) || Number.isNaN(interval[1])).toBe(false);
  expect(interval[0]).toBeLessThanOrEqual(interval[1]);
  if (interval[0] !== -Infinity) expect(compareExact(exact(interval[0]), value)).toBeLessThanOrEqual(0);
  if (interval[1] !== Infinity) expect(compareExact(exact(interval[1]), value)).toBeGreaterThanOrEqual(0);
}

function work(quota = Infinity): RigidWork & { operations: number; vertices: number } {
  const ledger = { operations: 0, vertices: 0,
    operation(units: number) { ledger.operations += units; },
    vertex() { ledger.vertices++; }, check() {}, shouldYield() { return ledger.operations >= quota; } };
  return ledger;
}
function meshOf(points: readonly number[], indices?: readonly number[]): THREE.Mesh {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(new Float64Array(points), 3));
  if (indices) geometry.setIndex([...indices]);
  const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial());
  mesh.updateMatrixWorld(true);
  return mesh;
}
function fullRange(mesh: THREE.Mesh): RigidRange {
  return { start: 0, end: mesh.geometry.index?.count ?? mesh.geometry.attributes.position.count };
}
function cameraMatrix(): THREE.Matrix4 {
  const camera = new THREE.OrthographicCamera(-8, 8, 5, -5, .1, 1000);
  camera.position.set(7, 11, 17); camera.lookAt(0, 0, 0); camera.updateMatrixWorld(true);
  return new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
}
function prepared(owner: RigidSupportPreparation, mesh: THREE.Mesh, range = fullRange(mesh)): void {
  expect(['preparing', 'ready']).toContain(owner.request(mesh, range));
  for (let tick = 0; tick < 10_000; tick++) {
    const status = owner.prepare(mesh, range, work(512));
    if (status === 'ready') return;
    expect(status).toBe('preparing');
  }
  throw new Error('bounded source fixture never finished preparation');
}
type Extrema = { min: number[]; max: number[] };
function empty(): Extrema { return { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] }; }
function include(bounds: Extrema, point: THREE.Vector3): void {
  const values = [point.x, point.y, point.z];
  for (let axis = 0; axis < 3; axis++) {
    bounds.min[axis] = Math.min(bounds.min[axis], values[axis]);
    bounds.max[axis] = Math.max(bounds.max[axis], values[axis]);
  }
}
function exhaustive(mesh: THREE.Mesh, range: RigidRange, pv: THREE.Matrix4): { world: Extrema; ndc: Extrema } {
  const result = { world: empty(), ndc: empty() }, point = new THREE.Vector3(), projected = new THREE.Vector3();
  // Deliberately use the original actual Three evaluation path for every raw
  // submitted index, without hierarchy, de-duplication or composed transforms.
  for (let entry = range.start; entry < range.end; entry++) {
    const index = mesh.geometry.index ? mesh.geometry.index.getX(entry) : entry;
    mesh.getVertexPosition(index, point).applyMatrix4(mesh.matrixWorld);
    include(result.world, point); include(result.ndc, projected.copy(point).applyMatrix4(pv));
  }
  return result;
}
function matchesExhaustive(owner: RigidSupportPreparation, mesh: THREE.Mesh,
  range = fullRange(mesh), pv = cameraMatrix()) {
  const expected = exhaustive(mesh, range, pv), result = owner.query(mesh, range, pv, work());
  expect(result.status).toBe('ready');
  if (result.status !== 'ready') throw new Error('expected current exact point query');
  // Each answer must be an actual leaf extremum, not a looser transformed box.
  expect(result.world).toEqual(expected.world); expect(result.ndc).toEqual(expected.ndc);
  return result;
}

describe('independent IEEE interval oracle', () => {
  it('uses the adjacent representable number at zero, powers of two and finite limits', () => {
    expect(nextUp(0)).toBe(Number.MIN_VALUE); expect(nextDown(0)).toBe(-Number.MIN_VALUE);
    expect(nextUp(-0)).toBe(Number.MIN_VALUE); expect(nextDown(-0)).toBe(-Number.MIN_VALUE);
    expect(nextUp(1)).toBe(1 + Number.EPSILON); expect(nextDown(1)).toBe(1 - Number.EPSILON / 2);
    expect(nextDown(-1)).toBe(-1 - Number.EPSILON); expect(nextUp(-1)).toBe(-1 + Number.EPSILON / 2);
    expect(nextUp(Number.MAX_VALUE)).toBe(Infinity); expect(nextDown(-Number.MAX_VALUE)).toBe(-Infinity);
  });
  it.each([
    [1, 2 ** -53], [1, -(1 - Number.EPSILON / 2)],
    [2 ** 100, -(2 ** 100) + 2 ** 48], [Number.MIN_VALUE, Number.MIN_VALUE],
    [Number.MAX_VALUE, Number.MAX_VALUE], [-Number.MAX_VALUE, -Number.MAX_VALUE],
    [1e-300, -1e-300], [-0, 0], [-1e100, 1e100], [3.141592653589793, -3.1415926535897927],
  ])('encloses exact dyadic addition for %s and %s', (a, b) => {
    encloses(outwardAdd([a, a], [b, b]), addExact(exact(a), exact(b)));
  });
  it.each([
    [1 + Number.EPSILON, 1 + Number.EPSILON], [Number.MIN_VALUE, .5],
    [-Number.MIN_VALUE, .5], [Number.MAX_VALUE, 2], [-Number.MAX_VALUE, 2],
    [1e-200, 1e-200], [1e150, 1e150], [-1e150, 1e150], [0, Number.MAX_VALUE],
    [Math.PI, Math.E],
  ])('encloses exact dyadic multiplication for %s and %s', (a, b) => {
    encloses(outwardMultiply([a, a], [b, b]), multiplyExact(exact(a), exact(b)));
  });
  it('encloses every exact endpoint product for signed and cancellation-heavy intervals', () => {
    const cases: [Interval, Interval][] = [
      [[-3, 5], [-7, 11]], [[-1e-300, 1e-300], [1e-100, 1e100]],
      [[Number.MIN_VALUE, 2 * Number.MIN_VALUE], [-.5, .5]],
      [[1 - Number.EPSILON / 2, 1 + Number.EPSILON], [-1 - Number.EPSILON, -1 + Number.EPSILON / 2]],
    ];
    for (const [a, b] of cases) {
      const product = outwardMultiply(a, b), sum = outwardAdd(a, b);
      for (const x of a) for (const y of b) {
        encloses(product, multiplyExact(exact(x), exact(y)));
        encloses(sum, addExact(exact(x), exact(y)));
      }
    }
  });
});

describe('rigid support against exhaustive submitted-point geometry', () => {
  it('retains exact support for a dense nonindexed point set with many duplicate coordinates', () => {
    const points = Array.from({ length: 120_000 * 3 }, (_, i) => [-1, 0, 0, 1, 0, 0, 0, 2, .5][i % 9]);
    const mesh = meshOf(points), owner = new RigidSupportPreparation(); prepared(owner, mesh);
    matchesExhaustive(owner, mesh); expect(owner.stats().vertices).toBeLessThanOrEqual(200_000);
  });
  it('does not include unsubmitted holes or loose box corners in disjoint indexed groups', () => {
    const mesh = meshOf([-2, 0, 0, 2, 0, 0, 0, 3, 0, 5000, 5000, 5000,
      4, -1, 1, 5, -1, 1, 4, 1, 1], [0, 1, 2, 3, 3, 3, 4, 5, 6]);
    mesh.geometry.addGroup(0, 3, 0); mesh.geometry.addGroup(6, 3, 1);
    const owner = new RigidSupportPreparation(), first = { start: 0, end: 3 }, second = { start: 6, end: 9 };
    prepared(owner, mesh, first); prepared(owner, mesh, second);
    const a = matchesExhaustive(owner, mesh, first), b = matchesExhaustive(owner, mesh, second);
    expect(a.world.max[0]).toBe(2); expect(b.world.min[0]).toBe(4);
    expect(a.world.max[1]).toBe(3); expect(b.world.max[1]).toBe(1);
  });
  it.each([0, Math.PI / 4, Math.PI / 2, -Math.PI / 3])('uses current tilted/yawed/nonuniform transforms at yaw %s', yaw => {
    const points = Array.from({ length: 300 * 3 }, (_, i) => Math.sin(i * .731) * (i % 3 + 1));
    const mesh = meshOf(points), owner = new RigidSupportPreparation(); prepared(owner, mesh);
    mesh.position.set(1.25, -.7, 2.5); mesh.rotation.set(.23, yaw, -.41); mesh.scale.set(.3, 1.7, .65); mesh.updateMatrixWorld(true);
    matchesExhaustive(owner, mesh);
    mesh.rotation.y += .127; mesh.position.x -= .32; mesh.updateMatrixWorld(true); matchesExhaustive(owner, mesh);
  });
  it('cannot prune an actual clipped or depth-outside point merely because other points are visible', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 1, 0, 100, 0, 2, 0, 100, 3, 0, 0, 100]);
    const owner = new RigidSupportPreparation(); prepared(owner, mesh);
    const result = matchesExhaustive(owner, mesh, fullRange(mesh), new THREE.Matrix4());
    expect(result.ndc.max).toEqual([100, 100, 100]);
  });
  it('keeps current transforms separate for meshes that share the same source geometry', () => {
    const first = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), second = new THREE.Mesh(first.geometry, first.material);
    first.position.set(-3, 1, 0); second.position.set(4, -2, 1); second.rotation.y = .4;
    first.updateMatrixWorld(true); second.updateMatrixWorld(true);
    const owner = new RigidSupportPreparation(); prepared(owner, first); prepared(owner, second);
    const a = matchesExhaustive(owner, first), b = matchesExhaustive(owner, second);
    expect(a.world.min[0]).toBe(-4); expect(b.world.min[0]).toBeGreaterThan(3);
  });
  it('matches exhaustive sequential arithmetic under huge cancelled translations', () => {
    const points = Array.from({ length: 240 * 3 }, (_, i) => (i % 3 === 0 ? 100_000 : -100_000) + Math.sin(i * 1.71) * .00001);
    const mesh = meshOf(points); mesh.position.set(-100_000, 100_000, 100_000); mesh.rotation.z = 1e-12; mesh.updateMatrixWorld(true);
    const pv = new THREE.Matrix4().makeTranslation(1e-10, -1e-10, 2e-10), owner = new RigidSupportPreparation();
    prepared(owner, mesh); matchesExhaustive(owner, mesh, fullRange(mesh), pv);
  });
  it('reads normalized interleaved coordinates through the actual Three attribute contract', () => {
    const geometry = new THREE.BufferGeometry();
    const interleaved = new THREE.InterleavedBuffer(new Int16Array([
      -32767, 0, 0, 111, 32767, 0, 0, 222, 0, 32767, 16384, 333,
    ]), 4);
    geometry.setAttribute('position', new THREE.InterleavedBufferAttribute(interleaved, 3, 0, true));
    const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial()); mesh.updateMatrixWorld(true);
    const owner = new RigidSupportPreparation(); prepared(owner, mesh); matchesExhaustive(owner, mesh);
    interleaved.needsUpdate = true;
    expect(owner.query(mesh, fullRange(mesh), cameraMatrix(), work()).status).toBe('invalid');
  });
  it('rejects support-tree projection for genuinely projective matrices so the outer exact fallback can run', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]), owner = new RigidSupportPreparation(); prepared(owner, mesh);
    const pv = new THREE.Matrix4(); pv.elements[3] = 1e-12;
    expect(owner.query(mesh, fullRange(mesh), pv, work()).status).toBe('unsupported');
    mesh.matrixWorld.elements[7] = 1e-12;
    expect(owner.query(mesh, fullRange(mesh), new THREE.Matrix4(), work()).status).toBe('unsupported');
  });
});

describe('rigid preparation source identity and bounded lifetime', () => {
  const mutations: [string, (mesh: THREE.Mesh) => void][] = [
    ['position version', mesh => { mesh.geometry.attributes.position.needsUpdate = true; }],
    ['position replacement', mesh => { mesh.geometry.setAttribute('position', mesh.geometry.attributes.position.clone()); }],
    ['position storage replacement', mesh => { const p = mesh.geometry.attributes.position as THREE.BufferAttribute; p.array = p.array.slice(); }],
    ['index replacement', mesh => { mesh.geometry.setIndex([2, 1, 0]); }],
    ['index version', mesh => { mesh.geometry.index!.needsUpdate = true; }],
    ['position layout', mesh => { mesh.geometry.attributes.position.normalized = true; }],
    ['draw range', mesh => { mesh.geometry.setDrawRange(0, 2); }],
    ['material group', mesh => { mesh.geometry.addGroup(0, 3, 0); }],
    ['positional morph classification', mesh => { mesh.geometry.morphAttributes.position = [mesh.geometry.attributes.position.clone()]; }],
  ];
  it.each(mutations)('never publishes a stale ready cache after %s changes', (_name, mutate) => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0], [0, 1, 2]), range = fullRange(mesh), owner = new RigidSupportPreparation();
    prepared(owner, mesh, range); mutate(mesh);
    expect(['invalid', 'unsupported']).toContain(owner.query(mesh, range, cameraMatrix(), work()).status);
    expect(owner.request(mesh, range)).toBe('invalid');
  });
  it('invalidates partial preparation instead of resetting work after a source version changes', () => {
    const mesh = meshOf(Array.from({ length: 900 }, (_, i) => Math.sin(i))), range = fullRange(mesh), owner = new RigidSupportPreparation();
    expect(owner.request(mesh, range)).toBe('preparing');
    expect(owner.prepare(mesh, range, work(1))).toBe('preparing');
    const before = owner.stats(); mesh.geometry.attributes.position.needsUpdate = true;
    expect(owner.prepare(mesh, range, work())).toBe('invalid');
    expect(owner.request(mesh, range)).toBe('invalid'); expect(owner.stats().vertices).toBeGreaterThanOrEqual(before.vertices);
  });
  it('cancels a partial owner permanently while a separately constructed generation can prepare', () => {
    const mesh = meshOf(Array.from({ length: 900 }, (_, i) => Math.sin(i))), range = fullRange(mesh), old = new RigidSupportPreparation();
    old.request(mesh, range); old.prepare(mesh, range, work(1)); old.invalidate();
    expect(old.prepare(mesh, range, work())).toBe('invalid'); expect(old.query(mesh, range, cameraMatrix(), work()).status).toBe('invalid');
    const fresh = new RigidSupportPreparation(); prepared(fresh, mesh, range); matchesExhaustive(fresh, mesh, range);
  });
  it('rejects a source mutation made inside the query rather than publishing mixed cached and current data', () => {
    const mesh = meshOf(Array.from({ length: 300 }, (_, i) => Math.sin(i))), owner = new RigidSupportPreparation(); prepared(owner, mesh);
    let changed = false;
    const queryWork = work(); queryWork.check = () => {
      if (!changed) { changed = true; mesh.geometry.attributes.position.needsUpdate = true; }
    };
    expect(owner.query(mesh, fullRange(mesh), cameraMatrix(), queryWork).status).toBe('invalid');
    expect(owner.request(mesh, fullRange(mesh))).toBe('invalid');
  });
  it('cannot resume an owner after the supplied frame CPU ledger reports exhaustion', () => {
    const mesh = meshOf(Array.from({ length: 300 }, (_, i) => Math.sin(i))), owner = new RigidSupportPreparation(); prepared(owner, mesh);
    const exhausted = work(); exhausted.check = () => { throw new Error('measurement_budget'); };
    expect(() => owner.query(mesh, fullRange(mesh), cameraMatrix(), exhausted)).toThrow('measurement_budget');
    expect(owner.query(mesh, fullRange(mesh), cameraMatrix(), work()).status).toBe('invalid');
  });
  it('enforces one actor-wide source budget across multiple meshes', () => {
    const owner = new RigidSupportPreparation({ maxVertices: 5, maxOperations: 2_000_000, maxBytes: 32 * 1024 * 1024 }), first = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]);
    const second = meshOf([-2, 0, 0, 2, 0, 0, 0, 3, 0]); prepared(owner, first);
    expect(() => prepared(owner, second)).toThrow('measurement_budget'); expect(owner.stats().vertices).toBeLessThanOrEqual(5);
    expect(owner.request(second, fullRange(second))).toBe('invalid');
  });
  it('cannot skip a failed final coordinate by catching an exception and retrying the same owner', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, NaN, 2, 0]), range = fullRange(mesh), owner = new RigidSupportPreparation();
    expect(owner.request(mesh, range)).toBe('preparing');
    expect(() => owner.prepare(mesh, range, work())).toThrow('invalid_geometry');
    expect(owner.prepare(mesh, range, work())).toBe('invalid');
    expect(owner.query(mesh, range, cameraMatrix(), work()).status).toBe('invalid');
  });
  it('counts overlapping range scans against a single finite preparation budget', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0], Array.from({ length: 90 }, (_, i) => i % 3));
    const owner = new RigidSupportPreparation({ maxVertices: 10, maxOperations: 120, maxBytes: 32 * 1024 * 1024 });
    prepared(owner, mesh, { start: 0, end: 60 });
    expect(() => prepared(owner, mesh, { start: 30, end: 90 })).toThrow('measurement_budget');
    expect(owner.stats().operations).toBeLessThanOrEqual(120);
    expect(owner.request(mesh, { start: 0, end: 60 })).toBe('invalid');
  });
  it('rejects cache allocations before exceeding the reserved typed-buffer allowance', () => {
    const mesh = meshOf([-1, 0, 0, 1, 0, 0, 0, 2, 0]);
    const owner = new RigidSupportPreparation({ maxVertices: 100, maxOperations: 1000, maxBytes: 16 });
    expect(() => prepared(owner, mesh)).toThrow('measurement_budget'); expect(owner.stats().bytes).toBeLessThanOrEqual(16);
    expect(owner.request(mesh, fullRange(mesh))).toBe('invalid');
  });
});
