import * as THREE from 'three';
import type { VRMHumanoid } from '@pixiv/three-vrm';

export const REST_ARM_ANGLE = 1.15;
const WAVE_ARM_ANGLE = THREE.MathUtils.degToRad(70);

/**
 * three-vrm removes bone rotations, but retains the source rest positions.
 * In particular, legacy VRM 0 arms point in the opposite X direction from
 * VRM 1. Derive an anatomical frame from those positions before posing.
 * Coordinates are relative to the normalized rig root, so rotateVRM0 and
 * the dashboard/OBS scene transforms do not change the result.
 */
export function humanoidFrame(humanoid: VRMHumanoid): THREE.Quaternion {
  const root = humanoid.normalizedHumanBonesRoot;
  return frameFromNodes(root, humanoid.getNormalizedBoneNode('leftUpperArm'), humanoid.getNormalizedBoneNode('rightUpperArm'), humanoid.getNormalizedBoneNode('head'), humanoid.getNormalizedBoneNode('hips'));
}

function frameFromNodes(root: THREE.Object3D, leftNode?: THREE.Object3D | null, rightNode?: THREE.Object3D | null, headNode?: THREE.Object3D | null, hipsNode?: THREE.Object3D | null): THREE.Quaternion {
  root.updateWorldMatrix(true, true);
  const point = (node?: THREE.Object3D | null) => node ? root.worldToLocal(node.getWorldPosition(new THREE.Vector3())) : undefined;
  const left = point(leftNode);
  const right = point(rightNode);
  const head = point(headNode);
  const hips = point(hipsNode);
  if (!left || !right || !head || !hips) return new THREE.Quaternion();
  const up = head.sub(hips);
  if (up.lengthSq() < 1e-10) return new THREE.Quaternion();
  up.normalize();
  const across = left.sub(right);
  across.addScaledVector(up, -across.dot(up));
  if (across.lengthSq() < 1e-10) return new THREE.Quaternion();
  across.normalize();
  const forward = new THREE.Vector3().crossVectors(across, up).normalize();
  return new THREE.Quaternion().setFromRotationMatrix(new THREE.Matrix4().makeBasis(across, up, forward));
}

/** Map a canonical (+X = character's left, +Y = up) pose to this rig. */
export function humanoidRotation(frame: THREE.Quaternion, x: number, y = 0, z = 0): THREE.Quaternion {
  return new THREE.Quaternion().setFromEuler(new THREE.Euler(x, y, z))
    .premultiply(frame).multiply(frame.clone().invert());
}

export function humanoidArmRest(frame: THREE.Quaternion, side: 'left' | 'right'): THREE.Quaternion {
  return humanoidRotation(frame, 0, 0, side === 'left' ? -REST_ARM_ANGLE : REST_ARM_ANGLE);
}

/** Keep an upward target at every intensity, without manually tuned rig values. */
export function humanoidWaveRaise(frame: THREE.Quaternion, weight: number): THREE.Quaternion {
  const envelope = Number.isFinite(weight) ? THREE.MathUtils.clamp(weight, 0, 1) : 0;
  const lift = REST_ARM_ANGLE + WAVE_ARM_ANGLE;
  return humanoidRotation(frame, -0.2 * envelope, 0, -lift * envelope);
}

type Arm = { upper: THREE.Object3D; lower: THREE.Object3D; hand: THREE.Object3D };
export interface GenericHumanoidRig {
  root: THREE.Object3D;
  frame: THREE.Quaternion;
  left: Arm;
  right: Arm;
  head: THREE.Object3D;
  rest: Map<THREE.Object3D, THREE.Quaternion>;
}

/** Conservative automatic mapping for coherent upright KayKit/Mixamo rigs.
 * Unknown names and quadrupeds retain their embedded animations unchanged. */
export function discoverGenericHumanoid(root: THREE.Object3D): GenericHumanoidRig | undefined {
  const names = new Map<string, THREE.Object3D[]>();
  root.traverse((node) => {
    if (!(node instanceof THREE.Bone)) return;
    const name = node.name.split(':').pop()!.toLowerCase().replace(/^mixamorig/, '').replace(/[^a-z0-9]/g, '');
    const matches = names.get(name) ?? []; matches.push(node); names.set(name, matches);
  });
  const find = (...aliases: string[]) => {
    const matches = [...new Set(aliases.flatMap((alias) => names.get(alias) ?? []))];
    return matches.length === 1 ? matches[0] : undefined;
  };
  const hips = find('hips', 'pelvis'); const spine = find('spine', 'abdomen'); const head = find('head');
  const arm = (side: 'left' | 'right'): Arm | undefined => {
    const suffix = side[0];
    const upper = find(`${side}upperarm`, `${side}arm`, `upperarm${suffix}`);
    const lower = find(`${side}lowerarm`, `${side}forearm`, `lowerarm${suffix}`);
    const hand = find(`${side}hand`, `hand${suffix}`);
    return upper && lower && hand ? { upper, lower, hand } : undefined;
  };
  const left = arm('left'); const right = arm('right');
  if (!hips || !spine || !head || !left || !right) return;
  const within = (node: THREE.Object3D, ancestor: THREE.Object3D): boolean => {
    for (let current = node.parent; current; current = current.parent) if (current === ancestor) return true;
    return false;
  };
  if (!within(spine, hips) || !within(head, spine)) return;
  for (const { upper, lower, hand } of [left, right]) {
    if (!within(upper, spine) || !within(lower, upper) || !within(hand, lower)) return;
    const length = upper.getWorldPosition(new THREE.Vector3()).distanceTo(hand.getWorldPosition(new THREE.Vector3()));
    if (length < 0.00001) return;
  }
  const up = root.worldToLocal(head.getWorldPosition(new THREE.Vector3())).sub(root.worldToLocal(hips.getWorldPosition(new THREE.Vector3()))).normalize();
  if (up.y < 0.65) return;
  const rest = new Map([left, right].flatMap(({ upper, lower, hand }) => [upper, lower, hand]).map((node) => [node, node.quaternion.clone()]));
  return { root, frame: frameFromNodes(root, left.upper, right.upper, head, hips), left, right, head, rest };
}

export function rawHumanoidRig(root: THREE.Object3D, humanoid: VRMHumanoid): GenericHumanoidRig | undefined {
  const left = { upper: humanoid.getRawBoneNode('leftUpperArm'), lower: humanoid.getRawBoneNode('leftLowerArm'), hand: humanoid.getRawBoneNode('leftHand') };
  const right = { upper: humanoid.getRawBoneNode('rightUpperArm'), lower: humanoid.getRawBoneNode('rightLowerArm'), hand: humanoid.getRawBoneNode('rightHand') };
  const head = humanoid.getRawBoneNode('head'); const hips = humanoid.getRawBoneNode('hips');
  if (!left.upper || !left.lower || !left.hand || !right.upper || !right.lower || !right.hand || !head || !hips) return;
  return { root, frame: frameFromNodes(root, left.upper, right.upper, head, hips), left: left as Arm, right: right as Arm, head, rest: new Map() };
}

export function rotateGenericBone(rig: GenericHumanoidRig, node: THREE.Object3D, x: number, y: number, z: number): THREE.Quaternion {
  const before = node.quaternion.clone();
  if (!node.parent) return new THREE.Quaternion();
  const worldFrame = rig.root.getWorldQuaternion(new THREE.Quaternion()).multiply(rig.frame);
  const delta = humanoidRotation(worldFrame, x, y, z);
  const desiredWorld = delta.multiply(node.getWorldQuaternion(new THREE.Quaternion()));
  node.quaternion.copy(node.parent.getWorldQuaternion(new THREE.Quaternion()).invert().multiply(desiredWorld));
  return before.invert().multiply(node.quaternion);
}

/** Aim in anatomical world space instead of assuming imported bone axes.
 * Returns exact local offsets so cancellation/next-frame reset cannot drift. */
export function poseGenericArm(rig: GenericHumanoidRig, side: 'left' | 'right', upperElevation: number, lowerElevation: number, weight: number, wristWave = 0): Map<THREE.Object3D, THREE.Quaternion> {
  const arm = rig[side];
  const envelope = Number.isFinite(weight) ? THREE.MathUtils.clamp(weight, 0, 1) : 0;
  const before = new Map([arm.upper, arm.lower, arm.hand].map((node) => [node, node.quaternion.clone()]));
  const worldFrame = rig.root.getWorldQuaternion(new THREE.Quaternion()).multiply(rig.frame);
  const direction = (elevation: number) => new THREE.Vector3((side === 'left' ? 1 : -1) * Math.cos(elevation), Math.sin(elevation), 0).applyQuaternion(worldFrame).normalize();
  const aim = (node: THREE.Object3D, child: THREE.Object3D, target: THREE.Vector3) => {
    const current = child.getWorldPosition(new THREE.Vector3()).sub(node.getWorldPosition(new THREE.Vector3()));
    if (current.lengthSq() < 1e-10 || !node.parent) return;
    const desiredWorld = new THREE.Quaternion().setFromUnitVectors(current.normalize(), target).multiply(node.getWorldQuaternion(new THREE.Quaternion()));
    const desiredLocal = node.parent.getWorldQuaternion(new THREE.Quaternion()).invert().multiply(desiredWorld);
    node.quaternion.slerp(desiredLocal, envelope);
    node.updateWorldMatrix(false, true);
  };
  aim(arm.upper, arm.lower, direction(upperElevation));
  aim(arm.lower, arm.hand, direction(lowerElevation));
  if (wristWave && arm.hand.parent) {
    const axis = new THREE.Vector3(0, 0, 1).applyQuaternion(worldFrame);
    const desiredWorld = new THREE.Quaternion().setFromAxisAngle(axis, wristWave * envelope).multiply(arm.hand.getWorldQuaternion(new THREE.Quaternion()));
    arm.hand.quaternion.copy(arm.hand.parent.getWorldQuaternion(new THREE.Quaternion()).invert().multiply(desiredWorld));
  }
  return new Map([...before].map(([node, quaternion]) => [node, quaternion.invert().multiply(node.quaternion)]));
}

/** These bundled KayKit meshes are alternative equipment, never two books. */
export function configureBundledAccessories(scene: THREE.Object3D, pathname: string): void {
  if (!pathname.endsWith('/characters/KayKitMage.glb')) return;
  const closedBook = scene.getObjectByName('Spellbook');
  const openBook = scene.getObjectByName('Spellbook_open');
  if (closedBook?.parent && closedBook.parent === openBook?.parent) closedBook.visible = false;
  const staff = scene.getObjectByName('2H_Staff');
  const wand = scene.getObjectByName('1H_Wand');
  if (staff?.parent && staff.parent === wand?.parent) staff.visible = false;
}
