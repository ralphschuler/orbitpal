import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AvatarPresentationGate } from './avatar-presentation';

describe('Rendered avatar presentation leases without GPU or output devices', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());
  it('requests a return immediately but requires a new rendered ready frame for every acquisition', async () => {
    const gate = new AvatarPresentationGate(); const signal = new AbortController().signal;
    gate.rendered(true); // A previous visible frame is not a presentation grant.
    let ready = false;
    const pending = gate.acquire(signal).then(release => { ready = true; return release; });
    expect(gate.requested).toBe(true);
    gate.rendered(false); await Promise.resolve(); expect(ready).toBe(false);
    gate.rendered(true); const release = await pending;
    expect(gate.requested).toBe(true); release(); expect(gate.requested).toBe(false);
  });
  it('holds a shared visible position until every concurrent presentation releases', async () => {
    const gate = new AvatarPresentationGate();
    const first = gate.acquire(new AbortController().signal), second = gate.acquire(new AbortController().signal);
    gate.rendered(true); const releaseFirst = await first, releaseSecond = await second;
    releaseFirst(); releaseFirst(); expect(gate.requested).toBe(true);
    releaseSecond(); expect(gate.requested).toBe(false);
  });
  it.each(['Stop', 'bot change', 'autopilot session change', 'asset change'])('rejects pending presentation on %s and cannot let an old release affect a new hold', async () => {
    const gate = new AvatarPresentationGate();
    const pending = gate.acquire(new AbortController().signal);
    const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    gate.invalidate(); await rejected; gate.rendered(true); expect(gate.requested).toBe(false);
    const old = gate.acquire(new AbortController().signal); gate.rendered(true); const releaseOld = await old;
    gate.invalidate();
    const current = gate.acquire(new AbortController().signal); gate.rendered(true); const releaseCurrent = await current;
    releaseOld(); expect(gate.requested).toBe(true); releaseCurrent(); expect(gate.requested).toBe(false);
  });
  it('aborts a waiting return and ignores all later rendered frames', async () => {
    const gate = new AvatarPresentationGate(); const abort = new AbortController();
    const pending = gate.acquire(abort.signal); const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    abort.abort(); await rejected;
    gate.rendered(true); expect(gate.requested).toBe(false); expect(vi.getTimerCount()).toBe(0);
  });
  it('releases an already prepared hold when its owner aborts without invoking release', async () => {
    const gate = new AvatarPresentationGate(); const abort = new AbortController();
    const pending = gate.acquire(abort.signal); gate.rendered(true); const release = await pending;
    abort.abort(); expect(gate.requested).toBe(false); release(); expect(gate.requested).toBe(false);
  });
  it('times out an unavailable/hidden renderer after 20 seconds rather than allowing invisible output', async () => {
    const gate = new AvatarPresentationGate(); let ready = false;
    const pending = gate.acquire(new AbortController().signal).then(release => { ready = true; return release; });
    const rejected = expect(pending).rejects.toThrow('nicht rechtzeitig');
    await vi.advanceTimersByTimeAsync(19_999); expect(gate.requested).toBe(true); expect(ready).toBe(false);
    gate.rendered(false); await vi.advanceTimersByTimeAsync(1); await rejected;
    gate.rendered(true); expect(ready).toBe(false); expect(gate.requested).toBe(false);
  });
  it('keeps a ready presentation held beyond the preparation timeout until explicitly released', async () => {
    const gate = new AvatarPresentationGate(); const pending = gate.acquire(new AbortController().signal);
    gate.rendered(true); const release = await pending; await vi.advanceTimersByTimeAsync(60_000);
    expect(gate.requested).toBe(true); expect(vi.getTimerCount()).toBe(0); release();
  });
  it('rejects a failed asset and permits a fresh asset only after reset and a new rendered frame', async () => {
    const gate = new AvatarPresentationGate(); const pending = gate.acquire(new AbortController().signal);
    const rejected = expect(pending).rejects.toThrow('Missing texture');
    gate.fail(new Error('Missing texture')); await rejected;
    await expect(gate.acquire(new AbortController().signal)).rejects.toThrow('Missing texture');
    gate.reset(); const current = gate.acquire(new AbortController().signal); gate.rendered(true); (await current)();
  });
  it('permanently rejects new work after dispose and removes pending timers/listeners', async () => {
    const gate = new AvatarPresentationGate(); const abort = new AbortController();
    const pending = gate.acquire(abort.signal); const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    gate.dispose(); await rejected;
    await expect(gate.acquire(abort.signal)).rejects.toMatchObject({ name: 'AbortError' });
    gate.rendered(true); expect(gate.requested).toBe(false); expect(vi.getTimerCount()).toBe(0);
  });
});

// Real Three.js scene geometry and desktop motion, without constructing WebGL.
// Returning to the screen alone must never resolve an output presentation.
import * as THREE from 'three';
import { AvatarStage, createRobot } from './avatar';
import { DesktopMotionController } from './desktop-motion';
import { measureDesktopModel } from './desktop-framing';
function desktopStage() {
  let now = 1_000;
  vi.stubGlobal('window', { devicePixelRatio: 1 });
  vi.spyOn(Date, 'now').mockImplementation(() => now);
  const robot = createRobot(), imported = new THREE.Group(), travel = new THREE.Group();
  travel.add(robot.root, imported);
  const camera = new THREE.PerspectiveCamera(32, 1.5, .05, 100), target = new THREE.Vector3();
  const profile = { id: 'nova', avatar: { scale: .6, camera: 'full', accent: '#80b8e8' } };
  const options = { profile, transparent: true, speaking: false, level: 0, emotion: 'neutral', desktop: { active: true, referencePhysicalHeight: 720 }, autopilot: { active: true, wander: true, gestures: false, startedAt: 1_000, seed: 'presentation-fixture' } };
  const gate = new AvatarPresentationGate();
  const stage = Object.create(AvatarStage.prototype) as any;
  Object.assign(stage, { options, robot, imported, travel, camera, controls: { target, update() { camera.lookAt(target); camera.updateMatrixWorld(); } }, ground: new THREE.Group(), desktopCamera: new THREE.OrthographicCamera(-1, 1, 1, -1, .05, 100), host: { clientWidth: 1200, clientHeight: 800 }, desktopModelBounds: measureDesktopModel(robot.root), desktopMotion: new DesktopMotionController(), asset: '', cameraMode: '', ambientSession: '', ambientGesture: '', ambientActions: [], walkStrength: 0, modelForwardYaw: 0, walking: false, presentation: gate, presentationContext: '', assetState: 'ready' });
  stage.update(options); stage.updateAutopilot(0, false);
  return { stage, gate, options, advance() { now += 50; stage.updateAutopilot(.05, false); } };
}

describe('AvatarStage presentation integration without WebGL', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  it('walks back from offscreen before a hold can be rendered ready, then stays until release', async () => {
    const { stage, gate, advance } = desktopStage();
    const initial = stage.travel.position.clone();
    let ready = false; const pending = stage.preparePresentation(new AbortController().signal).then((release: () => void) => { ready = true; return release; });
    expect(gate.requested).toBe(true);
    advance(); expect(stage.travel.position.distanceTo(initial)).toBeGreaterThan(0); expect(stage.desktopPresentationReady).toBe(false);
    for (let frame = 0; frame < 400 && !stage.desktopPresentationReady; frame++) advance();
    expect(stage.desktopPresentationReady).toBe(true); expect(stage.walking).toBe(false);
    await Promise.resolve(); expect(ready).toBe(false); // Motion readiness is not a rendered frame.
    gate.rendered(true); const release = await pending;
    const center = () => stage.travel.position.clone().add(stage.desktopModelBounds.center.clone().multiplyScalar(stage.robot.root.scale.x).applyQuaternion(stage.travel.quaternion));
    const held = center(); for (let frame = 0; frame < 60; frame++) advance();
    expect(center().distanceTo(held)).toBeLessThan(1e-9);
    release(); expect(gate.requested).toBe(false);
  });
  it.each(['profile', 'session', 'asset', 'motion assignment'])('invalidates a pending lease before a new %s can become visible', async change => {
    const { stage, options, gate } = desktopStage();
    const pending = stage.preparePresentation(new AbortController().signal);
    const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    const updated = structuredClone(options);
    if (change === 'profile') updated.profile.id = 'other';
    if (change === 'session') updated.autopilot.seed = 'new-session';
    if (change === 'asset') (updated.profile.avatar as Record<string, unknown>).assetPath = 'new.glb';
    if (change === 'motion assignment') (updated.profile as Record<string, unknown>).motions = [{ id: 'new.vrma', clips: [] }];
    stage.update(updated); await rejected; gate.rendered(true); expect(gate.requested).toBe(false);
  });
  it('does not disturb a current ambient gesture for an already aborted request', async () => {
    const { stage, gate } = desktopStage(); stage.emoteSource = 'ambient'; stage.finishEmote = vi.fn();
    const abort = new AbortController(); abort.abort();
    await expect(stage.preparePresentation(abort.signal)).rejects.toMatchObject({ name: 'AbortError' });
    expect(stage.finishEmote).not.toHaveBeenCalled(); expect(gate.requested).toBe(false);
  });
  it('never clamps an offscreen ambient emote into view', () => {
    const { stage } = desktopStage(); stage.emote = { action: 'celebrate' }; stage.emoteSource = 'ambient';
    const offscreen = stage.travel.position.clone(); stage.clampDesktopEmote();
    expect(stage.travel.position.equals(offscreen)).toBe(true);
  });
});
