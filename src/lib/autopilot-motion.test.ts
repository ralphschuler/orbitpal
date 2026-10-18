import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { VRMHumanoid, type VRMHumanBones } from '@pixiv/three-vrm';
import { humanoidArmRest, humanoidFrame, humanoidRotation } from './avatar-rig';
import { ambientGestureActions, discoverWalkingLegs, followAmbientPosition, motionLaneHalfWidth, sampleAmbientMotion, sampleWalkingGait, selectWalkingAnimation, type AutopilotVisualConfig } from './autopilot-motion';

const config: AutopilotVisualConfig = { active: true, wander: true, gestures: true, startedAt: 1_000_000, seed: 'nova-fixture-session' };
const sample = (milliseconds: number, settings = config) => sampleAmbientMotion(settings, settings.startedAt + milliseconds);

interface GltfFixture {
  nodes: { name?: string; children?: number[]; translation?: number[]; rotation?: number[]; scale?: number[] }[];
  scenes: { nodes: number[] }[]; scene?: number;
  skins?: { joints: number[] }[];
  animations?: { name?: string }[];
  extensions?: { VRM?: { humanoid: { humanBones: { bone: string; node: number }[] } } };
}
function asset(file: string) {
  const data = readFileSync(new URL(`../../public/characters/${file}`, import.meta.url));
  const json = JSON.parse(data.toString('utf8', 20, 20 + data.readUInt32LE(12))) as GltfFixture;
  const joints = new Set(json.skins?.flatMap((skin) => skin.joints));
  const nodes = json.nodes.map((definition, index) => {
    const node = joints.has(index) ? new THREE.Bone() : new THREE.Object3D();
    node.name = definition.name ?? '';
    if (definition.translation) node.position.fromArray(definition.translation);
    if (definition.rotation) node.quaternion.fromArray(definition.rotation);
    if (definition.scale) node.scale.fromArray(definition.scale);
    return node;
  });
  json.nodes.forEach((definition, index) => definition.children?.forEach((child) => nodes[index].add(nodes[child])));
  const scene = new THREE.Group(); json.scenes[json.scene ?? 0].nodes.forEach((index) => scene.add(nodes[index])); scene.updateMatrixWorld(true);
  return { json, scene, nodes };
}

describe('Deterministic optional ambient timeline', () => {
  it('produces the same output in studio/OBS and after reconnect without random frame calls', () => {
    const random = vi.spyOn(Math, 'random').mockImplementation(() => { throw new Error('No per-frame randomness'); });
    try {
      const first = Array.from({ length: 180 }, (_, index) => sample(index * 1000));
      const reconnect = Array.from({ length: 180 }, (_, index) => sampleAmbientMotion({ ...config }, config.startedAt + index * 1000));
      expect(first).toEqual(reconnect);
      expect(first.some((frame) => frame.walking)).toBe(true);
      expect(first.some((frame) => frame.gesture)).toBe(true);
    } finally { random.mockRestore(); }
  });

  it('keeps travel continuous between cycles and always inside the normalized lane', () => {
    for (let cycle = 1; cycle <= 10; cycle++) expect(sample(cycle * 45_000 - 1).x).toBeCloseTo(sample(cycle * 45_000).x, 6);
    for (let time = 0; time < 600_000; time += 103) {
      const frame = sample(time);
      expect(Math.abs(frame.x)).toBeLessThanOrEqual(0.65);
      if (frame.gesture) {
        expect(frame.walking).toBe(false);
        expect(frame.gesture.choice).toBeGreaterThanOrEqual(0);
        expect(frame.gesture.choice).toBeLessThan(1);
      }
    }
  });

  it('does not start visual work from disabled, missing, future or invalid sessions', () => {
    expect(sampleAmbientMotion(undefined, 1)).toMatchObject({ active: false, x: 0 });
    expect(sample(10_000, { ...config, active: false }).active).toBe(false);
    expect(sample(10_000, { ...config, wander: false, gestures: false }).active).toBe(false);
    expect(sampleAmbientMotion(config, config.startedAt - 1).active).toBe(false);
    expect(sampleAmbientMotion({ ...config, startedAt: NaN }, 1).active).toBe(false);
    expect(sampleAmbientMotion(config, Infinity).active).toBe(false);
  });

  it('keeps movement and gestures independently selectable', () => {
    for (let time = 0; time < 90_000; time += 100) {
      expect(sample(time, { ...config, wander: false }).x).toBe(0);
      expect(sample(time, { ...config, wander: false }).walking).toBe(false);
      expect(sample(time, { ...config, gestures: false }).gesture).toBeUndefined();
    }
    expect(sample(12_345, { ...config, seed: 'another-bot' })).not.toEqual(sample(12_345));
  });
});

describe('Stage-only movement bounds and preemption', () => {
  it('leaves the turning model inside the visible camera width and suppresses tight portraits', () => {
    const distance = 7.6; const fov = 32; const aspect = 1.4; const radius = 1.3;
    const lane = motionLaneHalfWidth(distance, fov, aspect, radius);
    const visible = distance * Math.tan(THREE.MathUtils.degToRad(fov) / 2) * aspect;
    expect(lane).toBeGreaterThan(0);
    expect(lane * 0.65 + radius).toBeLessThan(visible);
    expect(motionLaneHalfWidth(5.4, 32, 0.5, 1.8)).toBe(0);
    expect(motionLaneHalfWidth(NaN, 32, 1, 1)).toBe(0);
    expect(motionLaneHalfWidth(7, 180, 1, 1)).toBe(0);
  });

  it('holds position while speaking/manual actions block, then resumes without teleporting', () => {
    expect(followAmbientPosition(0.4, -0.5, 1 / 60, true)).toBe(0.4);
    const resumed = followAmbientPosition(0.4, -0.5, 1 / 60, false);
    expect(resumed).toBeLessThan(0.4);
    expect(0.4 - resumed).toBeLessThan(0.006);
    expect(followAmbientPosition(0.4, -0.5, 20, false)).toBeCloseTo(0.368, 6);
    expect(followAmbientPosition(NaN, Infinity, 1, false)).toBe(0);
  });
});

describe('Real model animation capability selection', () => {
  it.each(['KayKitKnight', 'KayKitMage', 'KayKitRogue', 'KayKitSkeleton', 'RobotExpressive', 'Fox'])('uses an actual safe forward walk clip from %s', (name) => {
    const { json } = asset(`${name}.glb`);
    const clips = json.animations!.map((animation) => new THREE.AnimationClip(animation.name ?? '', 1, []));
    const walking = selectWalkingAnimation(clips);
    expect(walking).toBeDefined();
    expect(['walk', 'walking', 'walking_a']).toContain(walking!.name.toLowerCase());
  });

  it('never substitutes jump/backward/running clips or a zero-duration pose', () => {
    const clips = ['WalkJump', 'Walking_Backwards', 'Running', 'Strafe_Walk'].map((name) => new THREE.AnimationClip(name, 1, []));
    clips.push(new THREE.AnimationClip('Walking', 0, []));
    expect(selectWalkingAnimation(clips)).toBeUndefined();
    expect(selectWalkingAnimation([new THREE.AnimationClip('Walk', 1, []), new THREE.AnimationClip('Walk', 1, [])])).toBeUndefined();
  });

  it('limits automatic gestures to available friendly choices', () => {
    const clips = ['Wave', 'Yes', 'Cheer', 'Death', 'Attack', 'Hide'].map((name) => new THREE.AnimationClip(name, 1, []));
    expect(ambientGestureActions(clips, [])).toEqual(['wave', 'nod', 'celebrate']);
    expect(ambientGestureActions([new THREE.AnimationClip('Jump', 1, [])], [])).toEqual([]);
    expect(ambientGestureActions([], ['wave', 'nod', 'celebrate', 'hide'])).toEqual(['wave', 'nod', 'celebrate']);
    expect(ambientGestureActions([], [])).toEqual([]);
  });

  it.each(['KayKitKnight', 'KayKitMage', 'KayKitRogue', 'KayKitSkeleton'])('recognizes complete coherent leg chains in %s', (name) => {
    const { scene } = asset(`${name}.glb`);
    const legs = discoverWalkingLegs(scene);
    expect(legs).toBeDefined();
    scene.attach(legs!.rightFoot);
    expect(discoverWalkingLegs(scene)).toBeUndefined();
  });

  it('does not invent humanoid legs for the quadruped Fox', () => {
    expect(discoverWalkingLegs(asset('Fox.glb').scene)).toBeUndefined();
  });
});

describe('Automatic normalized VRM leg gait', () => {
  it.each(['Mushy', 'Dracula', 'AlwaysWatching', 'ConeheadBeing', 'CoolAlien', 'CoolTiger', 'GhostPumpking', 'MaxHax', 'MeganTheFox', 'Paco'])('%s moves opposite feet through its real humanoid skeleton', (name) => {
    const { scene, nodes, json } = asset(`osa/${name}.vrm`);
    const bones = Object.fromEntries(json.extensions!.VRM!.humanoid.humanBones.map(({ bone, node }) => [bone, { node: nodes[node] }])) as VRMHumanBones;
    const humanoid = new VRMHumanoid(bones); scene.add(humanoid.normalizedHumanBonesRoot); scene.rotation.y = Math.PI;
    const frame = humanoidFrame(humanoid);
    const footPositions = (seconds: number) => {
      humanoid.resetNormalizedPose();
      for (const side of ['left', 'right'] as const) humanoid.getNormalizedBoneNode(`${side}UpperArm`)!.quaternion.copy(humanoidArmRest(frame, side));
      for (const [bone, x] of sampleWalkingGait(seconds).rotations) humanoid.getNormalizedBoneNode(bone)!.quaternion.multiply(humanoidRotation(frame, x));
      humanoid.update(); scene.updateMatrixWorld(true);
      return ['leftFoot', 'rightFoot'].map((bone) => humanoid.getRawBoneNode(bone as 'leftFoot')!.getWorldPosition(new THREE.Vector3()));
    };
    const first = footPositions(1 / 0.65 / 4);
    const second = footPositions(1 / 0.65 * 3 / 4);
    expect(first[0].distanceTo(second[0])).toBeGreaterThan(0.05);
    expect(first[1].distanceTo(second[1])).toBeGreaterThan(0.05);
    expect((first[0].z - second[0].z) * (first[1].z - second[1].z)).toBeLessThan(0);
    for (const point of [...first, ...second]) expect([point.x, point.y, point.z].every(Number.isFinite)).toBe(true);
  });
});
