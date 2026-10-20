// SPDX-License-Identifier: MIT
// Independently authored traversal-hint regressions. Every expected bound is
// exhaustively evaluated from submitted CPU points with actual Three arithmetic.
// No elapsed-time assertion, WebGL/native grant or performance claim.
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import { RigidSupportPreparation, type RigidRange, type RigidWork } from './world-rigid-support';

function ledger(maxVertices = 200_000, quota = Infinity): RigidWork & { vertices: number; operations: number; checks: number } {
  const work = { vertices: 0, operations: 0, checks: 0,
    vertex() { if (++work.vertices > maxVertices) throw Error('measurement_budget'); },
    operation(units: number) { work.operations += units; if (work.operations > 2_000_000) throw Error('measurement_budget'); },
    check() { work.checks++; }, shouldYield() { return work.operations >= quota; } };
  return work;
}
function meshOf(points: number[]): THREE.Mesh {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(new Float64Array(points), 3));
  const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial()); mesh.updateMatrixWorld(true); return mesh;
}
function rangeOf(mesh: THREE.Mesh): RigidRange { return { start: 0, end: mesh.geometry.index?.count ?? mesh.geometry.attributes.position.count }; }
function compile(owner: RigidSupportPreparation, mesh: THREE.Mesh, range = rangeOf(mesh)) {
  expect(['ready', 'preparing']).toContain(owner.request(mesh, range));
  for (let tick = 0; tick < 10_000; tick++) {
    const status = owner.prepare(mesh, range, ledger(200_000, 128));
    if (status === 'ready') return;
    expect(status).toBe('preparing'); expect(owner.request(mesh, range)).toBe('preparing');
  }
  throw Error('bounded fixture preparation never completes');
}
function exhaustive(mesh: THREE.Mesh, range: RigidRange, pv: THREE.Matrix4) {
  const result = { world: { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] },
    ndc: { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] } };
  const point = new THREE.Vector3(), projected = new THREE.Vector3();
  for (let raw = range.start; raw < range.end; raw++) {
    const index = mesh.geometry.index ? mesh.geometry.index.getX(raw) : raw;
    mesh.getVertexPosition(index, point).applyMatrix4(mesh.matrixWorld); projected.copy(point).applyMatrix4(pv);
    for (let axis = 0; axis < 3; axis++) {
      const w = point.getComponent(axis), p = projected.getComponent(axis);
      result.world.min[axis] = Math.min(result.world.min[axis], w); result.world.max[axis] = Math.max(result.world.max[axis], w);
      result.ndc.min[axis] = Math.min(result.ndc.min[axis], p); result.ndc.max[axis] = Math.max(result.ndc.max[axis], p);
    }
  }
  return result;
}
function query(owner: RigidSupportPreparation, mesh: THREE.Mesh, range: RigidRange, pv: THREE.Matrix4, maxVertices = 200_000) {
  const expected = exhaustive(mesh, range, pv), work = ledger(maxVertices), actual = owner.query(mesh, range, pv, work);
  expect(actual.status).toBe('ready');
  if (actual.status !== 'ready') throw Error('expected actual current point extrema');
  expect(actual.world).toEqual(expected.world); expect(actual.ndc).toEqual(expected.ndc);
  return { actual, work };
}

describe('actual source-ID hints preserve bounded exact queries', () => {
  it('does not charge seed points twice when rotated loose leaf boxes force remaining point work', () => {
    const count = 300, points = Array.from({ length: count * 3 }, (_, i) => [-1, 0, 0, 1, 0, 0, 0, 2, 0][i % 9]);
    const mesh = meshOf(points), range = rangeOf(mesh), owner = new RigidSupportPreparation(); compile(owner, mesh, range);
    // Every mixed leaf box contains the impossible corner (-1,2), whose
    // projected x is below the actual triangle minimum. Pruning cannot replace
    // proof of these leaves with a loose box answer. The *total* query vertex
    // allowance is exactly the source size, including all real seed evaluations.
    const { work } = query(owner, mesh, range, new THREE.Matrix4().makeRotationZ(Math.PI / 4), count);
    expect(work.vertices).toBeGreaterThan(0); expect(work.vertices).toBeLessThanOrEqual(count);
  });
  it('never seeds from excluded source points or another prepared submitted range', () => {
    const points = Array.from({ length: 180 * 3 }, (_, i) => {
      const point = Math.floor(i / 3), axis = i % 3;
      return point < 3 || point >= 150 ? 10_000 : Math.sin(point * .317 + axis) + (point >= 90 ? 4 : -4);
    });
    const mesh = meshOf(points), owner = new RigidSupportPreparation(), first = { start: 3, end: 63 }, second = { start: 90, end: 150 };
    mesh.material = [new THREE.MeshBasicMaterial(), new THREE.MeshBasicMaterial()];
    mesh.geometry.addGroup(first.start, first.end - first.start, 0); mesh.geometry.addGroup(second.start, second.end - second.start, 1);
    compile(owner, mesh, first); compile(owner, mesh, second);
    const a = query(owner, mesh, first, new THREE.Matrix4()), b = query(owner, mesh, second, new THREE.Matrix4());
    expect(a.actual.world.max[0]).toBeLessThan(-2); expect(b.actual.world.min[0]).toBeGreaterThan(2);
  });
  it.each([0, Math.PI / 4, Math.PI / 2, -Math.PI / 3])('evaluates static IDs through the fresh current matrix at yaw %s', yaw => {
    const geometry = new RoundedBoxGeometry(.9, 1.4, .6, 5, .1), mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial());
    mesh.updateMatrixWorld(true); const owner = new RigidSupportPreparation(); compile(owner, mesh);
    query(owner, mesh, rangeOf(mesh), new THREE.Matrix4());
    mesh.position.set(1.7, -.3, 2.1); mesh.rotation.set(.21, yaw, -.17); mesh.scale.set(.4, 1.7, .8); mesh.updateMatrixWorld(true);
    const pv = new THREE.Matrix4().makeRotationX(.37).setPosition(.1, -.2, .3);
    query(owner, mesh, rangeOf(mesh), pv);
    mesh.position.x -= .7; mesh.rotation.y += .127; mesh.updateMatrixWorld(true); query(owner, mesh, rangeOf(mesh), pv);
  });
  it('retains signed-zero extrema located in separate source blocks', () => {
    const points: number[] = [];
    for (let i = 0; i < 128; i++) points.push(i < 64 ? +0 : -0, -1, -1);
    const mesh = meshOf(points); mesh.matrixWorld.setPosition(-0, -0, -0);
    const pv = new THREE.Matrix4().setPosition(-0, -0, -0), owner = new RigidSupportPreparation(); compile(owner, mesh);
    const { actual } = query(owner, mesh, rangeOf(mesh), pv, 128);
    expect(Object.is(actual.world.min[0], -0)).toBe(true); expect(Object.is(actual.world.max[0], +0)).toBe(true);
    expect(Object.is(actual.ndc.min[0], -0)).toBe(true); expect(Object.is(actual.ndc.max[0], +0)).toBe(true);
  });
  it('keeps every yielded preparation unready until the source tree and its hint work finish', () => {
    const mesh = meshOf(Array.from({ length: 96 * 3 }, (_, i) => Math.sin(i * .731))), range = rangeOf(mesh), owner = new RigidSupportPreparation();
    expect(owner.request(mesh, range)).toBe('preparing');
    let pending = 0, ready = false;
    for (let tick = 0; tick < 10_000; tick++) {
      const status = owner.prepare(mesh, range, ledger(200_000, 1));
      if (status === 'ready') { ready = true; break; }
      expect(status).toBe('preparing'); pending++;
      expect(owner.request(mesh, range)).toBe('preparing'); expect(owner.query(mesh, range, new THREE.Matrix4(), ledger()).status).toBe('preparing');
    }
    expect(ready).toBe(true); expect(pending).toBeGreaterThan(1); query(owner, mesh, range, new THREE.Matrix4());
  });
  it('keeps a cold real RoundedBox query within explicit source/query ledgers without a time claim', () => {
    const geometry = new RoundedBoxGeometry(1, 1.4, .6, 5, .1), mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial());
    mesh.updateMatrixWorld(true); const owner = new RigidSupportPreparation(); compile(owner, mesh);
    const { work } = query(owner, mesh, rangeOf(mesh), new THREE.Matrix4().makeRotationZ(.31));
    expect(work.vertices).toBeGreaterThan(0); expect(work.vertices).toBeLessThanOrEqual(geometry.attributes.position.count);
    expect(work.operations).toBeGreaterThan(0); expect(work.operations).toBeLessThanOrEqual(2_000_000); expect(work.checks).toBeGreaterThan(0);
    expect(owner.stats().vertices).toBeLessThanOrEqual(200_000); expect(owner.stats().operations).toBeLessThanOrEqual(2_000_000);
  });
});
