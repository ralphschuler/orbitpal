// SPDX-License-Identifier: MIT
// Independent comparison with the already-tested public rounded graph and
// actual Three leaf arithmetic. No timing, rendering or native authority claim.
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { RigidSupportPreparation, roundedAdd, roundedMultiply, roundedAffineBounds,
  type RigidWork } from './world-rigid-support';

type Interval = readonly [number, number];
function reference(input: ArrayLike<number>, matrix: THREE.Matrix4): Float64Array {
  const e = matrix.elements, x: Interval = [input[0], input[3]], y: Interval = [input[1], input[4]], z: Interval = [input[2], input[5]];
  const result = new Float64Array(6);
  for (let row = 0; row < 3; row++) {
    const value = roundedMultiply(roundedAdd(roundedAdd(roundedAdd(
      roundedMultiply(x, [e[row], e[row]]), roundedMultiply(y, [e[row + 4], e[row + 4]])),
      roundedMultiply(z, [e[row + 8], e[row + 8]])), [e[row + 12], e[row + 12]]), [1, 1]);
    result[row] = value[0]; result[row + 3] = value[1];
  }
  return result;
}
function order(a: number, b: number): number {
  if (a === b) return a === 0 ? Number(Object.is(b, -0)) - Number(Object.is(a, -0)) : 0;
  return a < b ? -1 : 1;
}
function includes(bounds: Float64Array, point: THREE.Vector3) {
  for (let axis = 0; axis < 3; axis++) {
    expect(order(bounds[axis], point.getComponent(axis))).toBeLessThanOrEqual(0);
    expect(order(point.getComponent(axis), bounds[axis + 3])).toBeLessThanOrEqual(0);
  }
}
const boxes = [
  { name: 'finite interval', values: [-3, -2, -1, 4, 5, 6] },
  { name: 'signed zero', values: [-0, -1, -1, +0, -1, -1] },
  { name: 'subnormal', values: [-Number.MIN_VALUE, -Number.MIN_VALUE, -0, Number.MIN_VALUE, Number.MIN_VALUE, +0] },
  { name: 'cancelled translation', values: [100_000 - 1e-6, -100_000 - 1e-6, 100_000, 100_000 + 1e-6, -100_000 + 1e-6, 100_000] },
  { name: 'overflow', values: [Number.MAX_VALUE / 2, -1, 0, Number.MAX_VALUE, 1, 0] },
  { name: 'unordered', values: [2, 0, 0, 1, 0, 0] },
  { name: 'unordered signed zero', values: [+0, 0, 0, -0, 0, 0] },
  { name: 'NaN', values: [NaN, 0, 0, 1, 1, 1] },
  { name: 'indeterminate infinity', values: [-Infinity, -1, 0, Infinity, 1, 0] },
];
function matrices(): THREE.Matrix4[] {
  return [new THREE.Matrix4(), new THREE.Matrix4().setPosition(-0, -0, -0), new THREE.Matrix4().makeScale(.5, 2, -3),
    new THREE.Matrix4().compose(new THREE.Vector3(-100_000, 100_000, -100_000),
      new THREE.Quaternion().setFromEuler(new THREE.Euler(.31, -.27, .17)), new THREE.Vector3(.3, 1.7, .65)),
    new THREE.Matrix4().makeScale(0, 0, 0)];
}

describe('scalar scratch kernel preserves the public rounded operation graph', () => {
  it.each(boxes)('matches all six graph outputs for $name without stale output lanes', ({ values }) => {
    const out = new Float64Array(6).fill(12345);
    for (const matrix of matrices()) {
      roundedAffineBounds(values, matrix, out);
      expect(Array.from(out)).toEqual(Array.from(reference(values, matrix)));
    }
  });
  it('captures all six inputs before writing when input and output are the same buffer', () => {
    for (const { values } of boxes) for (const matrix of matrices()) {
      const alias = new Float64Array(values), expected = reference(alias, matrix);
      roundedAffineBounds(alias, matrix, alias); expect(Array.from(alias)).toEqual(Array.from(expected));
    }
  });
  it('accepts a noniterable ArrayLike input and overwrites all lanes across valid/invalid/valid reuse', () => {
    const input: ArrayLike<number> = { 0: -1, 1: -2, 2: -3, 3: 4, 4: 5, 5: 6, length: 6 };
    const matrix = matrices()[3], out = new Float64Array(6).fill(NaN);
    roundedAffineBounds(input, matrix, out); expect(Array.from(out)).toEqual(Array.from(reference(input, matrix)));
    roundedAffineBounds([NaN, 0, 0, 1, 1, 1], matrix, out); expect(Array.from(out)).toEqual([-Infinity, -Infinity, -Infinity, Infinity, Infinity, Infinity]);
    roundedAffineBounds(input, matrix, out); expect(Array.from(out)).toEqual(Array.from(reference(input, matrix)));
    expect(() => roundedAffineBounds(input, matrix, new Float64Array(5))).toThrow('invalid_geometry');
  });
  it.each([0, Math.PI / 4, Math.PI / 2, Math.PI, -Math.PI / 3])('encloses actual sequential Three points for fresh yaw %s', yaw => {
    const points = Array.from({ length: 75 }, (_, i) => new THREE.Vector3(Math.sin(i * .37) * 3, Math.cos(i * .71) * 2, Math.sin(i * .19)));
    points.push(new THREE.Vector3(-0, -Number.MIN_VALUE, +0), new THREE.Vector3(+0, Number.MIN_VALUE, -0));
    const box = new THREE.Box3().setFromPoints(points), input = new Float64Array([...box.min.toArray(), ...box.max.toArray()]);
    const world = new THREE.Matrix4().compose(new THREE.Vector3(1.7, -.4, 2.1),
      new THREE.Quaternion().setFromEuler(new THREE.Euler(.31, yaw, -.27)), new THREE.Vector3(.3, 1.7, .65));
    const camera = new THREE.OrthographicCamera(-10, 10, 8, -8, .1, 1000); camera.position.set(7, 11, 17); camera.lookAt(0, 0, 0); camera.updateMatrixWorld(true);
    const pv = new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
    const worldBounds = new Float64Array(6), ndcBounds = new Float64Array(6);
    roundedAffineBounds(input, world, worldBounds); roundedAffineBounds(worldBounds, pv, ndcBounds);
    expect(Array.from(worldBounds)).toEqual(Array.from(reference(input, world)));
    expect(Array.from(ndcBounds)).toEqual(Array.from(reference(reference(input, world), pv)));
    for (const local of points) { const actual = local.clone().applyMatrix4(world); includes(worldBounds, actual); actual.applyMatrix4(pv); includes(ndcBounds, actual); }
  });
});

function work(): RigidWork { return { operation() {}, vertex() {}, check() {}, shouldYield: () => false }; }
function mesh(phase: number): THREE.Mesh {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(new Float64Array(Array.from({ length: 300 }, (_, i) => Math.sin(i * .71 + phase))), 3));
  const actor = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial()); actor.position.set(phase, -phase / 2, phase / 3); actor.rotation.set(.21, phase, -.13); actor.updateMatrixWorld(true); return actor;
}
function exhaustive(actor: THREE.Mesh, pv: THREE.Matrix4) {
  const result = { world: { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] }, ndc: { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] } };
  const point = new THREE.Vector3(), projected = new THREE.Vector3();
  for (let index = 0; index < actor.geometry.attributes.position.count; index++) {
    actor.getVertexPosition(index, point).applyMatrix4(actor.matrixWorld); projected.copy(point).applyMatrix4(pv);
    for (let axis = 0; axis < 3; axis++) {
      result.world.min[axis] = Math.min(result.world.min[axis], point.getComponent(axis)); result.world.max[axis] = Math.max(result.world.max[axis], point.getComponent(axis));
      result.ndc.min[axis] = Math.min(result.ndc.min[axis], projected.getComponent(axis)); result.ndc.max[axis] = Math.max(result.ndc.max[axis], projected.getComponent(axis));
    }
  }
  return result;
}
describe('query-local scratch isolation', () => {
  it('keeps both exact results when a work callback makes a nested query on another prepared actor', () => {
    const first = mesh(.37), second = mesh(1.13), owner = new RigidSupportPreparation(), range = { start: 0, end: 100 };
    for (const actor of [first, second]) { owner.request(actor, range); expect(owner.prepare(actor, range, work())).toBe('ready'); }
    const firstPV = new THREE.Matrix4().makeRotationX(.37), secondPV = new THREE.Matrix4().makeRotationY(-.41);
    let nested = false;
    const nestedResult: { value?: ReturnType<RigidSupportPreparation['query']> } = {};
    const outerWork = work(); outerWork.check = () => { if (!nested) { nested = true; nestedResult.value = owner.query(second, range, secondPV, work()); } };
    const firstResult = owner.query(first, range, firstPV, outerWork);
    const secondResult = nestedResult.value;
    expect(nested).toBe(true); expect(firstResult.status).toBe('ready'); expect(secondResult?.status).toBe('ready');
    if (firstResult.status !== 'ready' || secondResult?.status !== 'ready') throw Error('both current queries must succeed');
    expect(firstResult.world).toEqual(exhaustive(first, firstPV).world); expect(firstResult.ndc).toEqual(exhaustive(first, firstPV).ndc);
    expect(secondResult.world).toEqual(exhaustive(second, secondPV).world); expect(secondResult.ndc).toEqual(exhaustive(second, secondPV).ndc);
  });
});
