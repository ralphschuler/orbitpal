import { afterEach, expect, it, vi } from 'vitest';
import { AvatarStage } from './avatar';
import type { StageObservation } from './stage-diagnostics';

// Exercise the actual Stage constructor, RAF callback and render gate. Only
// WebGL/DOM/asset boundaries are local doubles; no browser or GPU is started.
const observed = vi.hoisted(() => ({ draws: [] as number[] }));
vi.mock('three', async importOriginal => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: class {
    domElement = { style: {}, setAttribute() {}, addEventListener() {}, removeEventListener() {}, remove() {} };
    shadowMap = {};
    setPixelRatio() {}
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

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); observed.draws = []; });

function cadence(fps: unknown, instrumented: boolean) {
  let now = 0, scheduled = 0;
  let next: FrameRequestCallback | undefined;
  vi.stubGlobal('performance', { now: () => now });
  vi.stubGlobal('window', { devicePixelRatio: 1, matchMedia: () => ({ matches: false }) });
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { next = callback; return ++scheduled; });
  vi.stubGlobal('cancelAnimationFrame', () => { next = undefined; });
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  // Ground painting is irrelevant to scheduling; keep the real robot/scene,
  // animation body, renderOutput and successful submission observation.
  vi.spyOn(AvatarStage.prototype as unknown as { makeGround(): void }, 'makeGround').mockImplementation(() => {});
  const observations: StageObservation[] = [];
  const host = { clientWidth: 480, clientHeight: 480, appendChild() {} } as unknown as HTMLElement;
  const stage = new AvatarStage(host, { profile: { id: 'cadence-fixture', output: { fps }, avatar: { idle: false } },
    motionSource: 'fixed', speaking: false, level: 0, emotion: 'neutral', transparent: false, audioEnabled: false },
  undefined, undefined, instrumented ? observation => observations.push(observation) : undefined);
  expect(observed.draws).toEqual([]); // Constructor call at t=0 is not a draw or RAF callback.
  for (let frame = 1; frame <= 120; frame++) {
    // A fixed 1ms callback offset avoids making exact binary rounding at a
    // fractional interval boundary part of this target-rate regression.
    now = frame * 1000 / 120 + 1;
    const callback = next;
    expect(callback).toBeTypeOf('function');
    next = undefined;
    callback!(now);
  }
  const draws = [...observed.draws];
  stage.dispose();
  expect(next).toBeUndefined();
  expect(scheduled).toBe(121); // Frame-rate limiting never changes RAF scheduling.
  return { draws, observations };
}

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
