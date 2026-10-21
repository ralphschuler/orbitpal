import { afterEach, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { AvatarStage } from './avatar';
import type { StageObservation } from './stage-diagnostics';

// Exercise the actual Stage constructor, RAF callback and render gate. Only
// WebGL/DOM/asset boundaries are local doubles; no browser or GPU is started.
const observed = vi.hoisted(() => ({ draws: [] as number[], ratios: [] as number[], shadows: [] as Record<string, unknown>[] }));
vi.mock('three', async importOriginal => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: class {
    domElement = { style: {}, parentElement: null, getBoundingClientRect: () => ({ left: 0, top: 0, right: 480, bottom: 480 }), setAttribute() {}, addEventListener() {}, removeEventListener() {}, remove() {} };
    shadowMap = {};
    constructor() { observed.shadows.push(this.shadowMap); }
    setPixelRatio(value: number) { observed.ratios.push(value); }
    setClearColor() {}
    setSize() {}
    render() { observed.draws.push(performance.now()); }
    dispose() {}
    forceContextLoss() {}
  } };
});
vi.mock('three/addons/controls/OrbitControls.js', async () => {
  const { Vector3 } = await import('three');
  return { OrbitControls: class { target = new Vector3(); update() {} dispose() {} } };
});
vi.mock('./avatar-loader', () => ({ createAvatarLoader: () => ({ dispose() {} }) }));

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); observed.draws = []; observed.ratios = []; observed.shadows = []; });

function cadence(fps: unknown, instrumented: boolean, callbacks = 120, offsetMs = 1, quality?: string,
  expose: (frame: number) => boolean = () => true, change?: (stage: AvatarStage, frame: number) => void) {
  let now = 0, scheduled = 0;
  let next: FrameRequestCallback | undefined;
  vi.stubGlobal('performance', { now: () => now });
  vi.stubGlobal('window', { devicePixelRatio: 2, innerWidth: 1920, innerHeight: 1080, matchMedia: () => ({ matches: false }) });
  vi.stubGlobal('document', { visibilityState: 'visible' });
  vi.stubGlobal('getComputedStyle', () => ({ display: 'block', visibility: 'visible', opacity: '1' }));
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { next = callback; return ++scheduled; });
  vi.stubGlobal('cancelAnimationFrame', () => { next = undefined; });
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  // Ground painting is irrelevant to scheduling; keep the real robot/scene,
  // animation body, renderOutput and successful submission observation.
  vi.spyOn(AvatarStage.prototype as unknown as { makeGround(): void }, 'makeGround').mockImplementation(() => {});
  const observations: StageObservation[] = [];
  const host = { isConnected: true, clientWidth: 480, clientHeight: 480, appendChild() {} } as unknown as HTMLElement;
  const stage = new AvatarStage(host, { profile: { id: 'cadence-fixture', output: { fps }, avatar: { idle: false } },
    renderQuality: quality==='energy'?'energy':'balanced', motionSource: 'fixed', speaking: false, level: 0, emotion: 'neutral', transparent: false, audioEnabled: false },
  undefined, undefined, instrumented ? observation => observations.push(observation) : undefined);
  expect(observed.draws).toEqual([]); // Constructor call at t=0 is not a draw or RAF callback.
  for (let frame = 1; frame <= callbacks; frame++) {
    (host as unknown as { isConnected: boolean }).isConnected = expose(frame);
    change?.(stage, frame);
    now = frame * 1000 / 120 + offsetMs;
    const callback = next;
    expect(callback).toBeTypeOf('function');
    next = undefined;
    callback!(now);
  }
  const draws = [...observed.draws];
  stage.dispose();
  expect(next).toBeUndefined();
  expect(scheduled).toBe(callbacks + 1); // Frame-rate limiting never changes RAF scheduling.
  return { draws, observations };
}

it('applies energy settings at the real Stage while preserving RAF and cancellation scheduling', () => {
  const { draws } = cadence(60, true, 120, 1, 'energy');
  expect(draws).toHaveLength(20);
  expect(observed.ratios.every(ratio => ratio === 1)).toBe(true);
  expect(observed.shadows[0].enabled).toBe(false);
});

it('changes rendering cost without invoking output cancellation', () => {
  const { draws } = cadence(60, false, 120, 1, undefined, () => true, (stage, frame) => {
    if (frame !== 60) return;
    const cancel = vi.spyOn(stage, 'cancelWorld');
    stage.update({ profile: { id: 'cadence-fixture', output: { fps: 60 }, avatar: { idle: false } },
      renderQuality: 'energy', motionSource: 'fixed', speaking: false, level: 0, emotion: 'neutral', transparent: false, audioEnabled: false });
    expect(cancel).not.toHaveBeenCalled();
  });
  expect(draws.length).toBeGreaterThan(20);
  expect(draws.length).toBeLessThan(60);
  expect(observed.ratios).toContain(1.75);
  expect(observed.ratios.at(-1)).toBe(1);
  expect(observed.shadows[0].enabled).toBe(false);
});

it('invalidates each shared material once per shadow transition and retains programs for unchanged quality', () => {
  cadence(60, false, 1, 1, undefined, () => true, (stage, frame) => {
    if (frame !== 1) return;
    const scene = (stage as unknown as { scene: THREE.Scene }).scene;
    const shared = new THREE.MeshStandardMaterial(), plain = new THREE.MeshBasicMaterial();
    scene.add(new THREE.Mesh(new THREE.BoxGeometry(), [shared, plain]), new THREE.Mesh(new THREE.BoxGeometry(), shared));
    const materials = new Set<THREE.Material>();
    scene.traverse(object => {
      const material = (object as THREE.Mesh).material;
      if (material) for (const value of Array.isArray(material) ? material : [material]) materials.add(value);
    });
    const versions = new Map([...materials].map(material => [material, material.version]));
    const update = (quality: 'balanced'|'energy') => stage.update({ profile: { id: 'cadence-fixture', output: { fps: 60 }, avatar: { idle: false } },
      renderQuality: quality, motionSource: 'fixed', speaking: false, level: 0, emotion: 'neutral', transparent: false, audioEnabled: false });
    update('balanced');
    materials.forEach(material => expect(material.version).toBe(versions.get(material)));
    update('energy');
    materials.forEach(material => expect(material.version).toBe(versions.get(material)! + 1));
    update('energy');
    materials.forEach(material => expect(material.version).toBe(versions.get(material)! + 1));
    update('balanced');
    materials.forEach(material => expect(material.version).toBe(versions.get(material)! + 2));
  });
});

it('skips hidden GPU submissions and resumes fresh draws without stopping the loop', () => {
  const { draws, observations } = cadence(30, true, 120, 1, undefined, frame => frame > 60);
  expect(draws).toHaveLength(15);
  expect(draws[0]).toBeGreaterThan(500);
  expect(observations.filter(observation => observation.kind === 'raf')).toHaveLength(120);
  expect(observations.filter(observation => observation.kind === 'submitted')).toHaveLength(15);
});

it.each([20, 30, 60])('submits at the actual %ifps render gate while observing every RAF', fps => {
  const { draws, observations } = cadence(fps, true);
  expect(draws).toHaveLength(fps);
  expect(draws[0]).toBeCloseTo(1000 / fps + 1);
  expect(draws.at(-1)).toBeCloseTo(1001);
  expect(observations.filter(observation => observation.kind === 'raf')).toHaveLength(120);
  expect(observations.filter(observation => observation.kind === 'submitted')).toHaveLength(fps);
  expect(observations.filter(observation => observation.kind === 'failure')).toEqual([]);
});

it.each([30, 60, undefined, 17])('keeps the existing production cadence without a diagnostic observer (%s)', fps => {
  const { draws, observations } = cadence(fps, false);
  expect(draws).toHaveLength(fps === 30 ? 30 : 60);
  expect(observations).toEqual([]);
});

it.each([20, 30, 60])('does not duplicate fractional deadline ticks at exact 120Hz (%ifps)', fps => {
  const seconds = 10, callbacks = 120 * seconds;
  const { draws, observations } = cadence(fps, true, callbacks, 0);
  // Test the real RAF/render path, including 1000/120 and 1000/30 rounding.
  // One boundary frame is permitted; repeated early draws are not.
  expect(draws.length).toBeGreaterThanOrEqual(fps * seconds - 1);
  expect(draws.length).toBeLessThanOrEqual(fps * seconds + 1);
  expect(draws[0]).toBeCloseTo(1000 / fps);
  expect(draws.at(-1)).toBeCloseTo(seconds * 1000);
  for (let index = 1; index < draws.length; index++) {
    expect(draws[index] - draws[index - 1]).toBeGreaterThanOrEqual(1000 / fps - 0.5);
  }
  expect(observations.filter(observation => observation.kind === 'raf')).toHaveLength(callbacks);
  expect(observations.filter(observation => observation.kind === 'submitted')).toHaveLength(draws.length);
  expect(observations.filter(observation => observation.kind === 'failure')).toEqual([]);
});
