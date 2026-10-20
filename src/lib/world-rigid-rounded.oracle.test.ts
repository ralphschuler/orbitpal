// SPDX-License-Identifier: MIT
// A second oracle for the *rounded IEEE operation graph*. The independent
// exact-real/dyadic outward tests remain in world-rigid-support.oracle.test.ts.
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { RigidSupportPreparation, roundedAdd, roundedMultiply, roundedContained,
  type RigidWork } from './world-rigid-support';

type Interval = readonly [number, number];
function order(a: number, b: number): number {
  if (a === b) return a === 0 ? Number(Object.is(b, -0)) - Number(Object.is(a, -0)) : 0;
  return a < b ? -1 : 1;
}
function contains(interval: Interval, value: number): void {
  if (Number.isNaN(value)) { expect(interval).toEqual([-Infinity, Infinity]); return; }
  expect(Number.isNaN(interval[0]) || Number.isNaN(interval[1])).toBe(false);
  expect(order(interval[0], value)).toBeLessThanOrEqual(0);
  expect(order(value, interval[1])).toBeLessThanOrEqual(0);
}
const samples: { name: string; values: number[] }[] = [
  { name: 'signed zeros', values: [-0, +0] },
  { name: 'subnormal boundary', values: [-Number.MIN_VALUE, -0, +0, Number.MIN_VALUE] },
  { name: 'adjacent one', values: [1 - Number.EPSILON / 2, 1, 1 + Number.EPSILON] },
  { name: 'adjacent negative one', values: [-1 - Number.EPSILON, -1, -1 + Number.EPSILON / 2] },
  { name: 'large signed cancellation', values: [-1e150, -1, -Number.MIN_VALUE, -0, +0, Number.MIN_VALUE, 1, 1e150] },
  { name: 'tiny positive interior', values: [1e-300, 2e-300, 1e-200, 1e-100] },
  { name: 'overflow boundary', values: [Number.MAX_VALUE / 2, Number.MAX_VALUE / 2 + 2 ** 970, Number.MAX_VALUE] },
];

describe('independent enumeration of rounded Float64 intervals', () => {
  it.each(samples)('encloses scalar endpoints and interior values for $name against every sign family', ({ values }) => {
    const a: Interval = [values[0], values.at(-1)!];
    for (const other of samples) {
      const b: Interval = [other.values[0], other.values.at(-1)!];
      const sum = roundedAdd(a, b), product = roundedMultiply(a, b);
      for (const x of values) for (const y of other.values) {
        // Native JS scalar operations are the target here, not real arithmetic.
        contains(sum, x + y); contains(product, x * y);
      }
    }
  });
  it('keeps singleton constant axes exact without substituting ideal-real outward bounds', () => {
    expect(roundedAdd([1, 1], [2 ** -53, 2 ** -53])).toEqual([1, 1]);
    expect(roundedMultiply([0, 0], [1, 1])).toEqual([0, 0]);
    expect(roundedMultiply([-0, -0], [1, 1])).toEqual([-0, -0]);
  });
  it('makes NaN, unordered and indeterminate infinite arithmetic unbounded rather than prunable', () => {
    expect(roundedAdd([NaN, 0], [1, 2])).toEqual([-Infinity, Infinity]);
    expect(roundedMultiply([2, 1], [1, 2])).toEqual([-Infinity, Infinity]);
    expect(roundedAdd([0, -0], [1, 1])).toEqual([-Infinity, Infinity]);
    expect(roundedAdd([Infinity, Infinity], [-Infinity, -Infinity])).toEqual([-Infinity, Infinity]);
    expect(roundedMultiply([0, 0], [Infinity, Infinity])).toEqual([-Infinity, Infinity]);
  });
  it('cannot prune a possible negative-zero minimum or positive-zero maximum', () => {
    expect(roundedContained([-0, -0], +0, +0)).toBe(false);
    expect(roundedContained([+0, +0], -0, -0)).toBe(false);
    expect(roundedContained([-0, +0], -0, +0)).toBe(true);
    expect(roundedContained([.25, .5], 0, 1)).toBe(true);
    expect(roundedContained([-Infinity, Infinity], -1, 1)).toBe(false);
    expect(roundedContained([Infinity, Infinity], 0, Infinity)).toBe(false);
    expect(roundedContained([NaN, 0], 0, 1)).toBe(false);
    expect(roundedContained([2, 1], 0, 3)).toBe(false);
    expect(roundedContained([+0, -0], -0, +0)).toBe(false);
  });
});

function graph(box: readonly Interval[], matrix: THREE.Matrix4): Interval[] {
  const e = matrix.elements;
  return [0, 1, 2].map(row => roundedMultiply(roundedAdd(roundedAdd(roundedAdd(
    roundedMultiply(box[0], [e[row], e[row]]), roundedMultiply(box[1], [e[row + 4], e[row + 4]])),
    roundedMultiply(box[2], [e[row + 8], e[row + 8]])), [e[row + 12], e[row + 12]]), [1, 1]));
}
function ledger(): RigidWork { return { operation() {}, vertex() {}, check() {}, shouldYield: () => false }; }
function source(points: number[], matrix = new THREE.Matrix4()): THREE.Mesh {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(new Float64Array(points), 3));
  const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial());
  mesh.matrixAutoUpdate = false; mesh.matrix.copy(matrix); mesh.updateMatrixWorld(true); return mesh;
}
function compareQuery(mesh: THREE.Mesh, pv: THREE.Matrix4) {
  const range = { start: 0, end: mesh.geometry.attributes.position.count }, owner = new RigidSupportPreparation();
  expect(owner.request(mesh, range)).toBe('preparing'); expect(owner.prepare(mesh, range, ledger())).toBe('ready');
  const world = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
  const ndc = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
  const point = new THREE.Vector3(), projected = new THREE.Vector3();
  for (let index = 0; index < range.end; index++) {
    mesh.getVertexPosition(index, point).applyMatrix4(mesh.matrixWorld); projected.copy(point).applyMatrix4(pv);
    for (let axis = 0; axis < 3; axis++) {
      world.min[axis] = Math.min(world.min[axis], point.getComponent(axis)); world.max[axis] = Math.max(world.max[axis], point.getComponent(axis));
      ndc.min[axis] = Math.min(ndc.min[axis], projected.getComponent(axis)); ndc.max[axis] = Math.max(ndc.max[axis], projected.getComponent(axis));
    }
  }
  const actual = owner.query(mesh, range, pv, ledger()); expect(actual.status).toBe('ready');
  if (actual.status !== 'ready') throw Error('expected a finite current graph query');
  expect(actual.world).toEqual(world); expect(actual.ndc).toEqual(ndc); return actual;
}

describe('rounded graph against the separate actual Three sequential oracle', () => {
  it.each([0, Math.PI / 4, Math.PI / 2, Math.PI, -Math.PI / 3])('encloses every current world then projection point at yaw %s', yaw => {
    const local = [[-3, -1, -0, 0, .75, 3], [-2, -.1, 0, .1, 2], [-1, -Number.MIN_VALUE, +0, Number.MIN_VALUE, 1]];
    const box = local.map(values => [values[0], values.at(-1)!] as Interval);
    const world = new THREE.Matrix4().compose(new THREE.Vector3(1.7, -.4, 2.1),
      new THREE.Quaternion().setFromEuler(new THREE.Euler(.31, yaw, -.27)), new THREE.Vector3(.3, 1.7, .65));
    const camera = new THREE.OrthographicCamera(-10, 10, 8, -8, .1, 1000);
    camera.position.set(7, 11, 17); camera.lookAt(0, 0, 0); camera.updateMatrixWorld(true);
    const pv = new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
    const worldIntervals = graph(box, world), ndcIntervals = graph(worldIntervals, pv), points: number[] = [];
    for (const x of local[0]) for (const y of local[1]) for (const z of local[2]) {
      points.push(x, y, z); const actual = new THREE.Vector3(x, y, z).applyMatrix4(world);
      for (let axis = 0; axis < 3; axis++) contains(worldIntervals[axis], actual.getComponent(axis));
      actual.applyMatrix4(pv); for (let axis = 0; axis < 3; axis++) contains(ndcIntervals[axis], actual.getComponent(axis));
    }
    compareQuery(source(points, world), pv);
  });
  it('retains negative-zero and positive-zero extrema discovered in different leaf blocks', () => {
    const points: number[] = [];
    for (let index = 0; index < 128; index++) points.push(index < 64 ? +0 : -0, -1, -1);
    const world = new THREE.Matrix4().setPosition(-0, -0, -0), pv = new THREE.Matrix4().setPosition(-0, -0, -0);
    const result = compareQuery(source(points, world), pv);
    expect(Object.is(result.world.min[0], -0)).toBe(true); expect(Object.is(result.world.max[0], +0)).toBe(true);
    expect(Object.is(result.ndc.min[0], -0)).toBe(true); expect(Object.is(result.ndc.max[0], +0)).toBe(true);
  });
  it('matches rounded underflow and large cancelled translations without a heuristic epsilon', () => {
    const tiny = Array.from({ length: 300 }, (_, index) => index % 2 ? Number.MIN_VALUE : -Number.MIN_VALUE);
    compareQuery(source(tiny, new THREE.Matrix4().makeScale(.5, .5, .5)), new THREE.Matrix4());
    const points = Array.from({ length: 600 }, (_, index) => 100_000 + Math.sin(index * .371) * .000001);
    const world = new THREE.Matrix4().makeTranslation(-100_000, -100_000, -100_000);
    const pv = new THREE.Matrix4().makeRotationY(.317).setPosition(1e-12, -1e-12, 0);
    compareQuery(source(points, world), pv);
  });
  it('does not hide a nonfinite actual leaf behind an overflowed node bound', () => {
    const points = Array.from({ length: 128 * 3 }, () => 0); points[points.length - 3] = Number.MAX_VALUE;
    const mesh = source(points, new THREE.Matrix4().makeScale(2, 1, 1)), range = { start: 0, end: 128 }, owner = new RigidSupportPreparation();
    owner.request(mesh, range); expect(owner.prepare(mesh, range, ledger())).toBe('ready');
    expect(owner.query(mesh, range, new THREE.Matrix4(), ledger()).status).toBe('invalid');
    expect(owner.request(mesh, range)).toBe('invalid');
  });
});
