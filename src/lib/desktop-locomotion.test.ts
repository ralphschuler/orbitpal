// SPDX-License-Identifier: MIT
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { WorldAvatarController } from './world-avatar';
import { AvatarStage, createRobot, type AvatarOptions } from './avatar';
import { DistanceWalkClock } from './avatar-locomotion';
import { desktopTravelVector, measureDesktopModel } from './desktop-framing';

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

/** Real Stage methods, camera projection and walking mixer. The controller's
 * measured frame is supplied explicitly; no GPU, native window or audio. */
function fixture(scale = 1, modelForwardYaw = 0) {
  let now = 10_000;
  vi.stubGlobal('window', { devicePixelRatio: 1 });
  vi.spyOn(Date, 'now').mockImplementation(() => now);
  const robot = createRobot(), imported = new THREE.Group(), travel = new THREE.Group();
  const worldHeading=new THREE.Group(),worldOffset=new THREE.Group();
  robot.root.visible=false;worldOffset.add(robot.root,imported);worldHeading.add(worldOffset);travel.add(worldHeading);
  const foot = new THREE.Bone(); foot.name = 'Foot'; imported.add(foot);
  const clip = new THREE.AnimationClip('Walking', 1, [
    new THREE.VectorKeyframeTrack('Foot.position', [0, .5, 1], [0, 0, -.2, 0, 0, .2, 0, 0, -.2]),
  ]);
  const mixer = new THREE.AnimationMixer(imported), walkingAction = mixer.clipAction(clip);
  const camera = new THREE.PerspectiveCamera(32, 1.5, .05, 100), target = new THREE.Vector3();
  const options: AvatarOptions = {
    profile: { id: 'walk-fixture', avatar: { scale, camera: 'full', accent: '#80b8e8' } },
    transparent: true, speaking: false, level: 0, emotion: 'neutral',
    desktop: { active: true, referencePhysicalHeight: 720 },
    autopilot: { active: true, wander: true, gestures: false, startedAt: 1000, seed: 'walk-session' },
  };
  const point = { x: 600, y: 400, travel: { x: 0, y: 0 }, walking: false, presentationReady: true };
  const stage = Object.create(AvatarStage.prototype) as any;
  Object.assign(stage, {
    options, robot, imported, travel,worldHeading,worldOffset,world:new WorldAvatarController(travel,worldHeading,worldOffset),previousMotionSource:'fixed',worldRecovery:false, camera, mixer, walkingClip: clip, walkingAction,
    controls: { target, update() { camera.lookAt(target); camera.updateMatrixWorld(); } },
    ground: new THREE.Group(), desktopCamera: new THREE.OrthographicCamera(-1, 1, 1, -1, .05, 100),
    host: { clientWidth: 1200, clientHeight: 800 }, desktopModelBounds: measureDesktopModel(robot.root),
    desktopMotion: { reset: vi.fn(), sample: vi.fn(() => point) }, asset: 'synthetic-walker.glb',
    cameraMode: '', ambientSession: '', ambientGesture: '', ambientActions: [], walkStrength: 0,
    walkClock: new DistanceWalkClock(), walkingCycleDistance: 2, walkingPhase: 0, modelForwardYaw,
    contentHeight: 2.9, contentWidth: 2.05, contentDepth: 1.35, travelFraction: 0, walking: false,
  });
  stage.update(options); stage.updateAutopilot(0, false);
  function step(x: number, y: number, delta = 1 / 60) {
    point.travel = { x, y }; point.x += x; point.y += y;
    point.walking = Math.hypot(x, y) > 0; point.presentationReady = !point.walking;
    now += delta * 1000; stage.updateAutopilot(delta, false); mixer.update(delta);
  }
  const units = () => 800 / (stage.desktopCamera.top - stage.desktopCamera.bottom);
  return { stage, options, point, foot, walkingAction, step, units, stall() { now += 60_000; } };
}

describe('Actual desktop walk direction and cadence', () => {
  it('faces the projected path in all eight directions, keeping the body upright and respecting model forward axes', () => {
    for (const modelForwardYaw of [0, .73, Math.PI]) {
      const { stage, step } = fixture(.6, modelForwardYaw);
      for (const [x, y] of [[0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0], [-1, -1]]) {
        for (let frame = 0; frame < 120; frame++) step(x, y);
        const forward = new THREE.Vector3(Math.sin(modelForwardYaw), 0, Math.cos(modelForwardYaw)).applyQuaternion(stage.travel.quaternion);
        const origin = new THREE.Vector3().project(stage.desktopCamera), tip = forward.project(stage.desktopCamera);
        const screen = new THREE.Vector2((tip.x - origin.x) * 1200, -(tip.y - origin.y) * 800).normalize();
        expect(screen.dot(new THREE.Vector2(x, y).normalize())).toBeGreaterThan(.999999);
        expect(new THREE.Vector3(0, 1, 0).applyQuaternion(stage.travel.quaternion).distanceTo(new THREE.Vector3(0, 1, 0))).toBeLessThan(1e-8);
      }
    }
  });

  it('advances real embedded clip poses by actual distance, with matching animation sound rate', () => {
    const { stage, foot, walkingAction, step, units } = fixture(.6);
    const distance = 6 / (units() * stage.imported.scale.x);
    step(6, 0, .05);
    expect(walkingAction.time).toBeCloseTo(distance / 2);
    expect(stage.walkingAudioRate).toBeCloseTo(distance / 2 / .05);
    // The mixer actually evaluates the new distance-based time, independently
    // of the much larger session age and its zero effective time scale.
    expect(foot.position.z).toBeCloseTo((-.2 + walkingAction.time * .8) * walkingAction.getEffectiveWeight());
    const before = stage.walkClock.cycles;
    step(12, 0, .05);
    expect(stage.walkClock.cycles - before).toBeCloseTo(distance);
    expect(stage.walkingAudioRate).toBeCloseTo(distance / .05);
  });

  it('compensates vertical foreshortening and live scale when advancing steps', () => {
    const { stage, options, step, units } = fixture(.6);
    step(4, 3);
    const first = desktopTravelVector(4, 3).length() / (units() * .6 * 2);
    expect(stage.walkClock.cycles).toBeCloseTo(first);
    stage.update({ ...options, profile: { ...options.profile, avatar: { ...options.profile.avatar, scale: 1.2 } } });
    step(4, 3);
    expect(stage.walkClock.cycles).toBeCloseTo(first * 1.5);
    const before = stage.walkClock.cycles;
    step(0, 3);
    expect(stage.walkClock.cycles - before).toBeCloseTo(desktopTravelVector(0, 3).length() / (units() * 1.2 * 2));
  });

  it('holds phase during speech and render stalls, resumes from that phase, and resets for a new session', () => {
    const { stage, options, step, stall, units } = fixture();
    step(3, 0); const before = stage.walkClock.cycles;
    stage.options = { ...options, speaking: true };
    for (let frame = 0; frame < 5; frame++) { stall(); step(0, 0); }
    expect(stage.walkClock.cycles).toBe(before); expect(stage.walking).toBe(false);
    expect(stage.walkingAudioRate).toBe(0);
    stage.options = options; step(3, 0);
    expect(stage.walkClock.cycles).toBeCloseTo(before * 2);
    expect(stage.walkingAction.time).toBeCloseTo(before * 2);
    stage.options = { ...options, autopilot: { ...options.autopilot!, seed: 'new-walk-session' } };
    step(1, 0);
    expect(stage.walkClock.cycles).toBeCloseTo(1 / (units() * 2));
    const phase = stage.walkClock.cycles;
    stage.updateAutopilot(.05, true);
    expect(stage.walkClock.cycles).toBe(phase); expect(stage.walking).toBe(false);
  });

  it('uses the same distance clock for the procedural gait and freezes it at rest', () => {
    const { stage, step, units } = fixture();
    stage.asset = ''; stage.robot.root.visible = true;
    stage.walkingClip = undefined; stage.walkingAction = undefined;
    step(6, 0);
    expect(stage.walkingPhase).toBeCloseTo(6 / (units() * 2 * .65));
    const phase = stage.walkingPhase;
    step(0, 0, 60);
    expect(stage.walkingPhase).toBe(phase);
  });
});
