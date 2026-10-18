import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { AvatarStage } from './avatar';

/** Exercise the speech layer without constructing a GPU renderer or audio
 * source. Idle clips and deliberate emotes are separate layers. */
function speechRig(jawAvailable = true, mouthRig = false) {
  const root = new THREE.Group();
  const head = new THREE.Bone(); const chest = new THREE.Bone(); const leftArm = new THREE.Bone(); const rightArm = new THREE.Bone(); const jaw = new THREE.Bone();
  root.add(chest); chest.add(head, leftArm, rightArm); head.add(jaw);
  head.rotation.y = 0.12; chest.rotation.z = -0.08; leftArm.rotation.x = 0.2; rightArm.rotation.z = 0.3;
  const stage = Object.create(AvatarStage.prototype) as { animateSpeechJaw(): void } & Record<string, unknown>;
  Object.assign(stage, { vrm: undefined, genericMouthRig: mouthRig, mouthLevel: 0.8, jawBone: jawAvailable ? jaw : undefined, genericHead: head, genericChest: chest, previousBoneOffsets: new Map() });
  return { stage, root, head, chest, leftArm, rightArm, jaw };
}

describe('Speech moves a real mouth only', () => {
  it('opens an existing jaw without moving the head, torso, arms or whole character', () => {
    const { stage, root, head, chest, leftArm, rightArm, jaw } = speechRig();
    const body = [root, head, chest, leftArm, rightArm].map((node) => ({ node, rotation: node.quaternion.clone(), position: node.position.clone(), scale: node.scale.clone() }));
    stage.animateSpeechJaw();
    expect(jaw.quaternion.angleTo(new THREE.Quaternion())).toBeGreaterThan(0.1);
    for (const value of body) {
      expect(value.node.quaternion.angleTo(value.rotation)).toBeLessThan(0.000001);
      expect(value.node.position.equals(value.position)).toBe(true);
      expect(value.node.scale.equals(value.scale)).toBe(true);
    }
  });

  it('does not substitute head/body wiggling when a model has no mouth or jaw', () => {
    const { stage, root } = speechRig(false);
    const before = new Map<THREE.Object3D, number[]>();
    root.traverse((node) => before.set(node, [...node.position.toArray(), ...node.quaternion.toArray(), ...node.scale.toArray()]));
    stage.animateSpeechJaw();
    root.traverse((node) => expect([...node.position.toArray(), ...node.quaternion.toArray(), ...node.scale.toArray()]).toEqual(before.get(node)));
  });

  it('leaves a jaw to an existing working mouth morph rig', () => {
    const { stage, jaw } = speechRig(true, true);
    stage.animateSpeechJaw();
    expect(jaw.quaternion.angleTo(new THREE.Quaternion())).toBe(0);
  });
});
