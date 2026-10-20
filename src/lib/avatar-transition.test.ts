import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { WorldAvatarController } from './world-avatar';
import { AvatarStage } from './avatar';

/** Real mixer evaluation catches disabled actions and bind-pose fallbacks;
 * no GPU, provider or animation-action mocks are involved. */
function walkingStage() {
  const root = new THREE.Group();
  const arm = new THREE.Bone(); arm.name = 'Arm'; root.add(arm);
  const idlePose = new THREE.Quaternion().setFromEuler(new THREE.Euler(0, 0, -1.1));
  const walkPose = new THREE.Quaternion().setFromEuler(new THREE.Euler(0.3, 0, -0.9));
  const clip = (name: string, pose: THREE.Quaternion) => new THREE.AnimationClip(name, 1, [
    new THREE.QuaternionKeyframeTrack('Arm.quaternion', [0, 1], [...pose.toArray(), ...pose.toArray()]),
  ]);
  const mixer = new THREE.AnimationMixer(root);
  const clips = [clip('Idle', idlePose), clip('Walking', walkPose), clip('Cheer', walkPose)];
  const idleAction = mixer.clipAction(clips[0]).play();
  const walkingAction = mixer.clipAction(clips[1]);
  const stage = Object.create(AvatarStage.prototype) as {
    setWalking(walking: boolean, immediate?: boolean): void;
    triggerEmote(action: string, duration: number, intensity: number): void;
    animateEmote(reduced: boolean): void;
    finishEmote(): void;
    time: number;
  } & Record<string, unknown>;
  const robot={root:new THREE.Group()},imported=root,travel=new THREE.Group(),worldHeading=new THREE.Group(),worldOffset=new THREE.Group();
  worldOffset.add(robot.root,imported);worldHeading.add(worldOffset);travel.add(worldHeading);
  Object.assign(stage, {
    options:{profile:{id:'transition-fixture'},motionSource:'fixed'},previousMotionSource:'fixed',worldRecovery:false,
    world:new WorldAvatarController(travel,worldHeading,worldOffset),travel,worldHeading,worldOffset,
    mixer, clips, idleAction, walkingAction, walking: false, time: 0,emoteReady:true,asset:'fixture.glb',
    robot, imported,humanoidPose: new Map(), humanoidBasis: new THREE.Quaternion(),
  });
  mixer.update(0.016);
  return { stage, mixer, arm, idlePose, walkPose, idleAction, walkingAction };
}

describe('Autopilot returns from walking to the embedded idle', () => {
  it.each([false, true])('restores idle after every completed walk (immediate=%s)', (immediate) => {
    const { stage, mixer, arm, idlePose, walkPose, idleAction } = walkingStage();
    for (let cycle = 0; cycle < 4; cycle++) {
      stage.setWalking(true);
      mixer.update(0.4);
      expect(idleAction.enabled).toBe(false);
      expect(arm.quaternion.angleTo(walkPose)).toBeLessThan(0.001);
      stage.setWalking(false, immediate);
      mixer.update(0.4);
      expect(idleAction.enabled).toBe(true);
      expect(idleAction.getEffectiveWeight()).toBe(1);
      expect(arm.quaternion.angleTo(idlePose)).toBeLessThan(0.001);
      expect(arm.quaternion.angleTo(new THREE.Quaternion())).toBeGreaterThan(1);
    }
  });

  it('returns to idle when walking is interrupted before its fade completes', () => {
    const { stage, mixer, arm, idlePose, idleAction } = walkingStage();
    stage.setWalking(true); mixer.update(0.05);
    stage.setWalking(false); mixer.update(0.4);
    expect(idleAction.enabled).toBe(true);
    expect(arm.quaternion.angleTo(idlePose)).toBeLessThan(0.001);
  });

  it('keeps the idle phase when resuming instead of restarting it on every pause', () => {
    const { stage, mixer, idleAction } = walkingStage();
    stage.setWalking(true); mixer.update(0.4);
    const phase = idleAction.time;
    stage.setWalking(false);
    expect(idleAction.time).toBe(phase);
    mixer.update(0.1);
    expect(idleAction.time).toBeCloseTo((phase + 0.1) % 1, 6);
  });

  it('leaves an active imported emote in control when a walk is stopped', () => {
    const { stage, mixer, idleAction } = walkingStage();
    stage.setWalking(true); mixer.update(0.4);
    stage.emote = { clip: {} };
    stage.setWalking(false, true);
    mixer.update(0.4);
    expect(idleAction.enabled).toBe(false);
    expect(idleAction.getEffectiveWeight()).toBe(0);
  });

  it('restores the held idle pose when idle motion is disabled or reduced', () => {
    const { stage, mixer, arm, idlePose, idleAction } = walkingStage();
    idleAction.paused = true;
    const phase = idleAction.time;
    stage.setWalking(true); mixer.update(0.4);
    stage.setWalking(false); mixer.update(0.4);
    expect(idleAction.enabled).toBe(true);
    expect(idleAction.paused).toBe(true);
    expect(idleAction.time).toBe(phase);
    expect(arm.quaternion.angleTo(idlePose)).toBeLessThan(0.001);
  });
});

describe('Embedded emotes blend back to idle without a bind-pose gap', () => {
  it.each(['Cheer', 'Idle'])('keeps a natural pose through the entire %s preview and its end', (action) => {
    const { stage, mixer, arm, idlePose, idleAction } = walkingStage();
    stage.triggerEmote(action, 3000, 1);
    for (let frame = 1; frame <= 210; frame++) {
      stage.time = frame / 60;
      stage.animateEmote(false);
      mixer.update(1 / 60);
      expect(arm.quaternion.angleTo(new THREE.Quaternion())).toBeGreaterThan(0.5);
    }
    expect(idleAction.enabled).toBe(true);
    expect(idleAction.loop).toBe(THREE.LoopRepeat);
    expect(idleAction.timeScale).toBe(1);
    expect(arm.quaternion.angleTo(idlePose)).toBeLessThan(0.001);
  });

  it('returns immediately to the natural idle pose when an emote is cancelled', () => {
    const { stage, mixer, arm, idlePose } = walkingStage();
    stage.triggerEmote('Cheer', 3000, 1); mixer.update(0.5);
    stage.finishEmote(); mixer.update(1 / 60);
    expect(arm.quaternion.angleTo(idlePose)).toBeLessThan(0.001);
  });

  it('preserves the running idle phase when a different emote finishes', () => {
    const { stage, mixer, idleAction } = walkingStage();
    stage.triggerEmote('Cheer', 3000, 1); mixer.update(0.5);
    const phase = idleAction.time;
    stage.finishEmote();
    expect(idleAction.time).toBe(phase);
  });
});
