import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { VRMHumanoid, type VRMHumanBones } from '@pixiv/three-vrm';
import { configureBundledAccessories, discoverGenericHumanoid, humanoidArmRest, humanoidFrame, humanoidRotation, humanoidWaveRaise, poseGenericArm } from './avatar-rig';

interface GltfNode { name?: string; children?: number[]; translation?: number[]; rotation?: number[]; scale?: number[]; matrix?: number[]; }
interface GltfFixture {
  nodes: GltfNode[];
  scenes: { nodes: number[] }[];
  scene?: number;
  skins?: { joints: number[] }[];
  extensions?: { VRM?: { humanoid: { humanBones: { bone: string; node: number }[] } } };
}

function fixture(file: string) {
  const data = readFileSync(new URL(`../../public/characters/${file}`, import.meta.url));
  expect(data.toString('ascii', 0, 4)).toBe('glTF');
  expect(data.toString('ascii', 16, 20)).toBe('JSON');
  const json = JSON.parse(data.toString('utf8', 20, 20 + data.readUInt32LE(12))) as GltfFixture;
  const joints = new Set(json.skins?.flatMap((skin) => skin.joints));
  const nodes = json.nodes.map((definition, index) => {
    const node = joints.has(index) ? new THREE.Bone() : new THREE.Object3D();
    node.name = definition.name ?? '';
    if (definition.translation) node.position.fromArray(definition.translation);
    if (definition.rotation) node.quaternion.fromArray(definition.rotation);
    if (definition.scale) node.scale.fromArray(definition.scale);
    if (definition.matrix) new THREE.Matrix4().fromArray(definition.matrix).decompose(node.position, node.quaternion, node.scale);
    return node;
  });
  json.nodes.forEach((definition, index) => definition.children?.forEach((child) => nodes[index].add(nodes[child])));
  const scene = new THREE.Group();
  json.scenes[json.scene ?? 0].nodes.forEach((index) => scene.add(nodes[index]));
  scene.updateMatrixWorld(true);
  return { json, nodes, scene };
}

function legacyRig(file: string) {
  const data = fixture(`osa/${file}.vrm`);
  const bones = Object.fromEntries(data.json.extensions!.VRM!.humanoid.humanBones.map(({ bone, node }) => [bone, { node: data.nodes[node] }])) as VRMHumanBones;
  const humanoid = new VRMHumanoid(bones);
  data.scene.add(humanoid.normalizedHumanBonesRoot);
  // Exactly the same post-load orientation as VRMUtils.rotateVRM0.
  data.scene.rotation.y = Math.PI;
  return { ...data, humanoid };
}

function rest(humanoid: VRMHumanoid, frame: THREE.Quaternion) {
  humanoid.resetNormalizedPose();
  for (const side of ['left', 'right'] as const) humanoid.getNormalizedBoneNode(`${side}UpperArm`)!.quaternion.copy(humanoidArmRest(frame, side));
}

function heights(scene: THREE.Object3D, humanoid: VRMHumanoid, side: 'left' | 'right') {
  humanoid.update();
  scene.updateMatrixWorld(true);
  return {
    shoulder: humanoid.getRawBoneNode(`${side}UpperArm`)!.getWorldPosition(new THREE.Vector3()).y,
    hand: humanoid.getRawBoneNode(`${side}Hand`)!.getWorldPosition(new THREE.Vector3()).y,
  };
}

describe('VRM rest-pose retargeting', () => {
  it.each(['Mushy', 'Dracula', 'AlwaysWatching', 'ConeheadBeing', 'CoolAlien', 'CoolTiger', 'GhostPumpking', 'MaxHax', 'MeganTheFox', 'Paco'])('%s lowers both arms and raises the waving hand using the original bundled skeleton', (file) => {
    const { scene, humanoid } = legacyRig(file);
    const frame = humanoidFrame(humanoid);
    rest(humanoid, frame);
    for (const side of ['left', 'right'] as const) {
      const idle = heights(scene, humanoid, side);
      expect(idle.hand).toBeLessThan(idle.shoulder - 0.1);
    }
    humanoid.getNormalizedBoneNode('rightUpperArm')!.quaternion.multiply(humanoidWaveRaise(frame, 1));
    humanoid.getNormalizedBoneNode('rightLowerArm')!.quaternion.multiply(humanoidRotation(frame, 0, 0, -0.35));
    const waving = heights(scene, humanoid, 'right');
    expect(waving.hand).toBeGreaterThan(waving.shoulder + 0.1);
  });

  it('also supports the opposite, modern VRM rest basis', () => {
    const scene = new THREE.Group();
    const definitions = {
      hips: [0, 0.7, 0], head: [0, 1.6, 0],
      leftUpperArm: [0.25, 1.3, 0], leftLowerArm: [0.55, 1.3, 0], leftHand: [0.85, 1.3, 0],
      rightUpperArm: [-0.25, 1.3, 0], rightLowerArm: [-0.55, 1.3, 0], rightHand: [-0.85, 1.3, 0],
    };
    const bones = Object.fromEntries(Object.entries(definitions).map(([name, position]) => {
      const node = new THREE.Object3D(); node.position.fromArray(position); scene.add(node); return [name, { node }];
    })) as VRMHumanBones;
    // Match the anatomical hierarchy while retaining these absolute positions.
    for (const side of ['left', 'right'] as const) {
      bones[`${side}UpperArm`]!.node.attach(bones[`${side}LowerArm`]!.node);
      bones[`${side}LowerArm`]!.node.attach(bones[`${side}Hand`]!.node);
    }
    const humanoid = new VRMHumanoid(bones);
    scene.add(humanoid.normalizedHumanBonesRoot);
    const frame = humanoidFrame(humanoid);
    rest(humanoid, frame);
    expect(heights(scene, humanoid, 'left').hand).toBeLessThan(1);
    expect(heights(scene, humanoid, 'right').hand).toBeLessThan(1);
    humanoid.getNormalizedBoneNode('rightUpperArm')!.quaternion.multiply(humanoidWaveRaise(frame, 1));
    const wave = heights(scene, humanoid, 'right');
    expect(wave.hand).toBeGreaterThan(wave.shoulder + 0.3);
  });

  it('keeps the anatomical frame independent of display rotation, translation and scale', () => {
    const { scene, humanoid } = legacyRig('Mushy');
    const before = humanoidFrame(humanoid);
    scene.position.set(3, 7, -4);
    scene.rotation.set(0.4, 1.7, -0.3);
    scene.scale.setScalar(3.2);
    expect(humanoidFrame(humanoid).angleTo(before)).toBeLessThan(0.000001);
  });

  it('never over-rotates an automatic wave at doubled or invalid intensity', () => {
    const { scene, humanoid } = legacyRig('Mushy');
    const frame = humanoidFrame(humanoid);
    rest(humanoid, frame);
    humanoid.getNormalizedBoneNode('rightUpperArm')!.quaternion.multiply(humanoidWaveRaise(frame, 2));
    const wave = heights(scene, humanoid, 'right');
    expect(wave.hand).toBeGreaterThan(wave.shoulder + 0.1);
    expect(humanoidWaveRaise(frame, 2).angleTo(humanoidWaveRaise(frame, 1))).toBeLessThan(0.000001);
    expect(humanoidWaveRaise(frame, NaN).angleTo(new THREE.Quaternion())).toBeLessThan(0.000001);
  });

});

describe('Conservative automatic GLB gestures', () => {
  it.each(['KayKitKnight', 'KayKitMage', 'KayKitRogue', 'KayKitSkeleton'])('%s discovers coherent arms and raises the actual hand using its imported bone axes', (name) => {
    const { scene } = fixture(`${name}.glb`);
    const rig = discoverGenericHumanoid(scene);
    expect(rig).toBeDefined();
    scene.rotation.y = 1.2; scene.scale.setScalar(2);
    for (const side of ['left', 'right'] as const) poseGenericArm(rig!, side, -1.15, -1.15, 1);
    const restingHand = rig!.right.hand.getWorldPosition(new THREE.Vector3()).y;
    const offsets = poseGenericArm(rig!, 'right', THREE.MathUtils.degToRad(70), Math.PI / 2, 1, 0.2);
    const raisedHand = rig!.right.hand.getWorldPosition(new THREE.Vector3()).y;
    expect(raisedHand).toBeGreaterThan(rig!.right.upper.getWorldPosition(new THREE.Vector3()).y + 0.1);
    for (const [node, delta] of offsets) node.quaternion.multiply(delta.invert());
    expect(rig!.right.hand.getWorldPosition(new THREE.Vector3()).y).toBeCloseTo(restingHand, 6);
  });

  it.each(['Fox', 'RobotExpressive'])('leaves the %s skeleton to its authored animation clips', (name) => {
    expect(discoverGenericHumanoid(fixture(`${name}.glb`).scene)).toBeUndefined();
  });

  it('rejects ambiguous names and disconnected arm chains instead of guessing', () => {
    const { scene } = fixture('KayKitMage.glb');
    const rig = discoverGenericHumanoid(scene)!;
    scene.attach(rig.right.hand);
    expect(discoverGenericHumanoid(scene)).toBeUndefined();
    const original = fixture('KayKitMage.glb').scene;
    const duplicate = new THREE.Bone(); duplicate.name = 'upperarm.l'; original.add(duplicate);
    expect(discoverGenericHumanoid(original)).toBeUndefined();
  });
});

describe('Bundled Mage equipment variants', () => {
  it('shows one open book and one wand while retaining the animated hand hierarchy', () => {
    const { scene } = fixture('KayKitMage.glb');
    const book = scene.getObjectByName('Spellbook_open')!;
    const hand = book.parent;
    configureBundledAccessories(scene, '/characters/KayKitMage.glb');
    expect(scene.getObjectByName('Spellbook')!.visible).toBe(false);
    expect(book.visible).toBe(true);
    expect(scene.getObjectByName('2H_Staff')!.visible).toBe(false);
    expect(scene.getObjectByName('1H_Wand')!.visible).toBe(true);
    expect(book.parent).toBe(hand);
  });

  it('does not hide similarly named accessories in a user import or another model', () => {
    const { scene } = fixture('KayKitMage.glb');
    configureBundledAccessories(scene, '/assets/KayKitMage.glb');
    expect(scene.getObjectByName('Spellbook')!.visible).toBe(true);
    expect(scene.getObjectByName('2H_Staff')!.visible).toBe(true);
  });
});
