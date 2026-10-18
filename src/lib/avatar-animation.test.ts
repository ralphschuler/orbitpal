import { describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { VRM, VRMExpression, VRMExpressionManager, VRMExpressionMorphTargetBind, VRMHumanoid, VRMNodeConstraintManager, VRMSpringBoneManager, type VRMHumanBones } from '@pixiv/three-vrm';
import { AuthoredAnimationProperties, finishVrmAnimationFrame, selectEmoteAnimation, selectIdleAnimation } from './avatar-animation';

function model() {
  const scene = new THREE.Group();
  const hips = new THREE.Bone(); hips.name = 'Hips';
  scene.add(hips);
  const bone = (parent: THREE.Object3D, name: string, x: number, y: number) => {
    const node = new THREE.Bone(); node.name = name; node.position.set(x, y, 0); parent.add(node); return node;
  };
  const spine = bone(hips, 'Spine', 0, 0.4);
  const head = bone(spine, 'Head', 0, 0.6);
  const leftUpperArm = bone(spine, 'LeftArm', 0.2, 0.3); const leftLowerArm = bone(leftUpperArm, 'LeftForeArm', 0.3, 0); const leftHand = bone(leftLowerArm, 'LeftHand', 0.3, 0);
  const rightUpperArm = bone(spine, 'RightArm', -0.2, 0.3); const rightLowerArm = bone(rightUpperArm, 'RightForeArm', -0.3, 0); const rightHand = bone(rightLowerArm, 'RightHand', -0.3, 0);
  const leftUpperLeg = bone(hips, 'LeftUpperLeg', 0.1, -0.2); const leftLowerLeg = bone(leftUpperLeg, 'LeftLowerLeg', 0, -0.3); const leftFoot = bone(leftLowerLeg, 'LeftFoot', 0, -0.3);
  const rightUpperLeg = bone(hips, 'RightUpperLeg', -0.1, -0.2); const rightLowerLeg = bone(rightUpperLeg, 'RightLowerLeg', 0, -0.3); const rightFoot = bone(rightLowerLeg, 'RightFoot', 0, -0.3);
  const bones: VRMHumanBones = {
    hips: { node: hips }, spine: { node: spine }, head: { node: head },
    leftUpperArm: { node: leftUpperArm }, leftLowerArm: { node: leftLowerArm }, leftHand: { node: leftHand },
    rightUpperArm: { node: rightUpperArm }, rightLowerArm: { node: rightLowerArm }, rightHand: { node: rightHand },
    leftUpperLeg: { node: leftUpperLeg }, leftLowerLeg: { node: leftLowerLeg }, leftFoot: { node: leftFoot },
    rightUpperLeg: { node: rightUpperLeg }, rightLowerLeg: { node: rightLowerLeg }, rightFoot: { node: rightFoot },
  };
  const humanoid = new VRMHumanoid(bones);
  scene.add(humanoid.normalizedHumanBonesRoot);
  const expressions = new VRMExpressionManager();
  const springBones = new VRMSpringBoneManager();
  const constraints = new VRMNodeConstraintManager();
  const material = Object.assign(new THREE.MeshBasicMaterial(), { update: vi.fn() });
  const vrm = new VRM({ scene, humanoid, meta: { metaVersion: '1', name: 'Fixture', authors: ['Test'], licenseUrl: 'https://example.invalid/license' }, expressionManager: expressions, springBoneManager: springBones, nodeConstraintManager: constraints, materials: [material] });
  const mixer = new THREE.AnimationMixer(scene);
  const properties = new AuthoredAnimationProperties(scene);
  return { scene, head, humanoid, vrm, mixer, properties, expressions, springBones, constraints, material };
}

describe('Authored avatar animation ownership', () => {
  it('keeps embedded raw-bone clips while updating VRM expressions, constraints, physics and MToon once', () => {
    const { head, humanoid, vrm, mixer, properties, expressions, springBones, constraints, material } = model();
    const rotation = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 0, 1), 0.7);
    const clip = new THREE.AnimationClip('Wave', 1, [new THREE.QuaternionKeyframeTrack('Head.quaternion', [0, 1], [...rotation.toArray(), ...rotation.toArray()])]);
    const action = mixer.clipAction(clip).play();
    const expressionUpdate = vi.spyOn(expressions, 'update');
    const springUpdate = vi.spyOn(springBones, 'update');
    const constraintUpdate = vi.spyOn(constraints, 'update');
    humanoid.getNormalizedBoneNode('head')!.rotation.z = -0.4;
    humanoid.update();
    mixer.update(0.3);
    properties.captureTransforms([action]);
    finishVrmAnimationFrame(vrm, 0.016, properties, [action]);
    expect(head.quaternion.angleTo(rotation)).toBeLessThan(0.000001);
    expect(expressionUpdate).toHaveBeenCalledOnce();
    expect(springUpdate).toHaveBeenCalledExactlyOnceWith(0.016);
    expect(constraintUpdate).toHaveBeenCalledOnce();
    expect(material.update).toHaveBeenCalledExactlyOnceWith(0.016);
    expect(humanoid.autoUpdateHumanBones).toBe(true);

    // Mixer caches skip writes for unchanged key values, including paused
    // clips. Restoring the last authored pose prevents a procedural reset.
    humanoid.getNormalizedBoneNode('head')!.rotation.z = 0.2;
    humanoid.update();
    action.paused = true;
    properties.restoreTransforms([action]);
    mixer.update(0.016);
    properties.captureTransforms([action]);
    finishVrmAnimationFrame(vrm, 0.016, properties, [action]);
    expect(head.quaternion.angleTo(rotation)).toBeLessThan(0.000001);
  });

  it('protects only morph indices owned by the current animation and preserves other VRM visemes', () => {
    const { scene, vrm, mixer, properties, expressions } = model();
    const face = new THREE.Mesh(new THREE.BufferGeometry(), new THREE.MeshBasicMaterial());
    face.name = 'Face'; face.morphTargetDictionary = { blink: 0, aa: 1 }; face.morphTargetInfluences = [0, 0]; scene.add(face);
    const blink = new VRMExpression('blink');
    blink.addBind(new VRMExpressionMorphTargetBind({ primitives: [face], index: 0, weight: 1 }));
    const mouth = new VRMExpression('aa');
    mouth.addBind(new VRMExpressionMorphTargetBind({ primitives: [face], index: 1, weight: 1 }));
    mouth.weight = 0.7;
    expressions.registerExpression(blink); expressions.registerExpression(mouth);
    const clip = new THREE.AnimationClip('Look', 1, [new THREE.NumberKeyframeTrack('Face.morphTargetInfluences[blink]', [0, 1], [0, 1])]);
    const action = mixer.clipAction(clip).play(); mixer.update(0.5);
    expect(properties.owns([action], face, 'morphTargetInfluences', 0)).toBe(true);
    expect(properties.owns([action], face, 'morphTargetInfluences', 1)).toBe(false);
    finishVrmAnimationFrame(vrm, 0.016, properties, [action]);
    expect(face.morphTargetInfluences).toEqual([0.5, 0.7]);
    action.stop();
    expect(properties.owns([action], face, 'morphTargetInfluences', 0)).toBe(false);
    finishVrmAnimationFrame(vrm, 0.016, properties, [action]);
    expect(face.morphTargetInfluences).toEqual([0, 0.7]);
  });

  it('recognizes whole-array glTF weight tracks and authored expression weights', () => {
    const { scene, mixer, properties } = model();
    const face = new THREE.Mesh(); face.name = 'Face'; face.morphTargetInfluences = [0, 0]; scene.add(face);
    const expression = new VRMExpression('happy'); scene.add(expression);
    const clip = new THREE.AnimationClip('Face', 1, [
      new THREE.NumberKeyframeTrack('Face.morphTargetInfluences', [0, 1], [0, 0, 0.8, 0.3]),
      new THREE.NumberKeyframeTrack(`${expression.name}.weight`, [0, 1], [0, 0.6]),
    ]);
    const action = mixer.clipAction(clip).play(); mixer.update(0.5);
    expect(properties.owns([action], face, 'morphTargetInfluences', 0)).toBe(true);
    expect(properties.owns([action], face, 'morphTargetInfluences', 1)).toBe(true);
    expect(properties.owns([action], expression, 'weight')).toBe(true);
    const restore = properties.captureMorphs([action]);
    face.morphTargetInfluences.fill(0); restore();
    expect(face.morphTargetInfluences[0]).toBeCloseTo(0.4, 6);
    expect(face.morphTargetInfluences[1]).toBeCloseTo(0.15, 6);
  });

  it('restores the SDK humanoid update flag even if a component rejects a frame', () => {
    const { vrm, humanoid } = model();
    vi.spyOn(vrm, 'update').mockImplementation(() => { expect(humanoid.autoUpdateHumanBones).toBe(false); throw new Error('fixture'); });
    expect(() => finishVrmAnimationFrame(vrm, 0.016)).toThrow('fixture');
    expect(humanoid.autoUpdateHumanBones).toBe(true);
  });

  it('lets audible visemes override a mouth-blocking face clip and restores the exact authored face after speech', () => {
    const { scene, vrm, mixer, properties, expressions } = model();
    const face = new THREE.Mesh(); face.name = 'Face'; face.morphTargetDictionary = { blink: 0, aa: 1 }; face.morphTargetInfluences = [0, 0]; scene.add(face);
    const mouth = new VRMExpression('aa');
    mouth.addBind(new VRMExpressionMorphTargetBind({ primitives: [face], index: 1, weight: 1 }));
    const happy = new VRMExpression('happy'); happy.overrideMouth = 'block';
    expressions.registerExpression(mouth); expressions.registerExpression(happy); scene.add(mouth, happy);
    const clip = new THREE.AnimationClip('FaceIdle', 1, [
      new THREE.NumberKeyframeTrack('Face.morphTargetInfluences', [0, 1], [0.5, 0, 0.5, 0]),
      new THREE.NumberKeyframeTrack(`${happy.name}.weight`, [0, 1], [1, 1]),
      new THREE.NumberKeyframeTrack(`${mouth.name}.weight`, [0, 1], [0, 0]),
    ]);
    const action = mixer.clipAction(clip).play(); mixer.update(0.2); properties.captureTransforms([action]);
    mouth.weight = 0.8;
    finishVrmAnimationFrame(vrm, 0.016, properties, [action], [mouth]);
    expect(face.morphTargetInfluences).toEqual([0.5, 0.8]);
    expect(happy.weight).toBe(1);

    properties.restoreTransforms([action]); mixer.update(0); properties.captureTransforms([action]);
    finishVrmAnimationFrame(vrm, 0.016, properties, [action]);
    expect(face.morphTargetInfluences).toEqual([0.5, 0]);
    expect(mouth.weight).toBe(0);
    expect(happy.weight).toBe(1);
  });
});

describe('Imported clip discovery and selection', () => {
  const clips = (...names: string[]) => names.map((name) => new THREE.AnimationClip(name, 1, []));
  it('prefers neutral Idle over activity-specific poses and matches model aliases case-insensitively', () => {
    const animations = clips('2H_Melee_Idle', 'Unarmed_Idle', 'Idle', 'WAVE', 'CHEER');
    expect(selectIdleAnimation(animations)?.name).toBe('Idle');
    expect(selectEmoteAnimation(animations, 'wave')?.name).toBe('WAVE');
    expect(selectEmoteAnimation(animations, 'celebrate')?.name).toBe('CHEER');
    expect(selectEmoteAnimation(animations, 'hide')).toBeUndefined();
  });
  it('keeps exact authored names and rejects ambiguous duplicate clips instead of playing another animation', () => {
    expect(selectEmoteAnimation(clips('Dance', 'dance'), 'dance')?.name).toBe('dance');
    expect(() => selectEmoteAnimation(clips('Dance', 'Dance'), 'Dance')).toThrow('mehrfach');
    expect(() => selectEmoteAnimation(clips('Dance', 'dance'), 'DANCE')).toThrow('eindeutig');
  });
});
