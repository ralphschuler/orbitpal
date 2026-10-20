import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { selectWalkingAnimation } from './autopilot-motion';
import { DistanceWalkClock, estimateWalkingCycleDistance, proceduralCycleDistance } from './avatar-locomotion';

function rig() {
  const root = new THREE.Group(); const hips = new THREE.Bone(); hips.name = 'Hips'; root.add(hips);
  const feet = ['LeftFoot', 'RightFoot'].map(name => { const foot = new THREE.Bone(); foot.name = name; hips.add(foot); return foot; });
  return { root, hips, feet };
}

function footClip() {
  return new THREE.AnimationClip('Walking', 1, [
    new THREE.VectorKeyframeTrack('LeftFoot.position', [0, 0.5, 1], [0, 0, -0.25, 0, 0, 0.25, 0, 0, -0.25]),
    new THREE.VectorKeyframeTrack('RightFoot.position', [0, 0.5, 1], [0, 0, 0.25, 0, 0, -0.25, 0, 0, 0.25]),
  ]);
}

/** Preserve real bundled skeletons, buffers, interpolation and animation
 * tracks, while excluding meshes/materials/textures and all resource loads. */
async function bundled(file: string) {
  const source = readFileSync(new URL(`../../public/characters/${file}.glb`, import.meta.url));
  const jsonLength = source.readUInt32LE(12);
  const json = JSON.parse(source.toString('utf8', 20, 20 + jsonLength));
  for (const node of json.nodes) { delete node.mesh; delete node.camera; delete node.extensions; }
  for (const animation of json.animations) animation.channels = animation.channels.filter((channel: { target: { path: string } }) => channel.target.path !== 'weights');
  for (const property of ['meshes', 'materials', 'images', 'textures', 'extensions', 'extensionsUsed', 'extensionsRequired', 'cameras']) delete json[property];
  const bytes = Buffer.from(JSON.stringify(json));
  const padding = (4 - bytes.length % 4) % 4;
  const chunk = Buffer.concat([bytes, Buffer.alloc(padding, 32)]);
  const binary = source.subarray(20 + jsonLength);
  const glb = Buffer.alloc(20 + chunk.length + binary.length);
  glb.writeUInt32LE(0x46546c67, 0); glb.writeUInt32LE(2, 4); glb.writeUInt32LE(glb.length, 8);
  glb.writeUInt32LE(chunk.length, 12); glb.writeUInt32LE(0x4e4f534a, 16); chunk.copy(glb, 20); binary.copy(glb, 20 + chunk.length);
  return new GLTFLoader().parseAsync(glb.buffer.slice(glb.byteOffset, glb.byteOffset + glb.byteLength) as ArrayBuffer, '');
}

describe('Distance-driven walking clock', () => {
  it('advances equally for equal distances at different speeds, frame rates and directions', () => {
    const quick = new DistanceWalkClock(); const slow = new DistanceWalkClock();
    for (let frame = 0; frame < 10; frame++) quick.advance(0.1, 2, 'session');
    for (let frame = 0; frame < 100; frame++) slow.advance(Math.hypot(0.006, 0.008), 2, 'session');
    expect(quick.cycles).toBeCloseTo(0.5, 12); expect(slow.cycles).toBeCloseTo(quick.cycles, 12);
  });

  it('freezes across arbitrary pauses and resumes without jumping to wall-clock age', () => {
    const clock = new DistanceWalkClock();
    expect(clock.advance(0.4, 2, 'first')).toBeCloseTo(0.2);
    for (let frame = 0; frame < 10000; frame++) expect(clock.advance(0, 2, 'first')).toBeCloseTo(0.2);
    expect(clock.advance(0.02, 2, 'first')).toBeCloseTo(0.21);
    expect(clock.advance(0.02, 2, 'second')).toBeCloseTo(0.01);
    clock.reset('second'); expect(clock.advance(0, 2, 'second')).toBe(0);
  });

  it('ignores invalid distances and protects its accumulated phase', () => {
    const clock = new DistanceWalkClock(); clock.advance(1, 2);
    for (const distance of [-1, NaN, Infinity]) expect(clock.advance(distance, 2)).toBe(0.5);
    for (const stride of [-1, 0, NaN, Infinity]) expect(clock.advance(1, stride)).toBe(0.5);
    expect(clock.advance(Number.MAX_VALUE, Number.MIN_VALUE)).toBe(0.5);
  });
});

describe('One-time authored stride calibration', () => {
  it('uses actual root translation before estimating a foot stride', () => {
    const { root } = rig(); const clip = footClip();
    clip.tracks.push(new THREE.VectorKeyframeTrack('Hips.position', [0, 1], [0, 0, 0, 0, 0, 1.6]));
    expect(estimateWalkingCycleDistance(root, clip, 2)).toEqual({ source: 'root-motion', cycleDistance: expect.closeTo(1.6) });
  });

  it('measures a full cycle from foot excursion while ignoring body sway', () => {
    const { root } = rig(); const clip = footClip();
    clip.tracks.push(new THREE.VectorKeyframeTrack('Hips.position', [0, 0.5, 1], [0, 0, 0, 0.02, 0.08, 0.01, 0, 0, 0]));
    expect(estimateWalkingCycleDistance(root, clip, 2)).toEqual({ source: 'foot-motion', cycleDistance: expect.closeTo(1) });
  });

  it('keeps the original live pose, mixer, clip, and parent transforms untouched', () => {
    const { root, hips, feet } = rig(); const clip = footClip();
    const parent = new THREE.Group(); parent.position.set(7, -4, 3); parent.rotation.y = 1.3; parent.scale.setScalar(1.8); parent.add(root);
    const normalization = new THREE.Group(); normalization.scale.setScalar(3); root.add(normalization); normalization.add(hips);
    const mixer = new THREE.AnimationMixer(root); const action = mixer.clipAction(clip).play(); mixer.update(0.125);
    const snapshot = [hips, ...feet].map(node => ({ position: node.position.toArray(), quaternion: node.quaternion.toArray(), scale: node.scale.toArray() }));
    const time = action.time; const values = clip.tracks.map(track => Array.from(track.values));
    expect(estimateWalkingCycleDistance(root, clip, 6)).toEqual({ source: 'foot-motion', cycleDistance: expect.closeTo(3) });
    expect(action.time).toBe(time); expect(clip.tracks.map(track => Array.from(track.values))).toEqual(values);
    expect([hips, ...feet].map(node => ({ position: node.position.toArray(), quaternion: node.quaternion.toArray(), scale: node.scale.toArray() }))).toEqual(snapshot);
    expect(parent.position.toArray()).toEqual([7, -4, 3]); expect(parent.scale.x).toBe(1.8);
  });

  it.each(['Fox', 'RobotExpressive', 'KayKitKnight', 'KayKitMage', 'KayKitRogue', 'KayKitSkeleton'])('calibrates actual %s embedded walking tracks without external resources', async name => {
    const gltf = await bundled(name); const clip = selectWalkingAnimation(gltf.animations)!;
    const measured = estimateWalkingCycleDistance(gltf.scene, clip, name === 'Fox' ? 100 : name === 'RobotExpressive' ? 5 : 3);
    expect(clip).toBeDefined(); expect(measured.source).not.toBe('height-fallback');
    expect(measured.cycleDistance).toBeGreaterThan(0); expect(Number.isFinite(measured.cycleDistance)).toBe(true);
    const wrapper = new THREE.Group(); wrapper.add(gltf.scene); gltf.scene.scale.multiplyScalar(2);
    expect(estimateWalkingCycleDistance(wrapper, clip, (name === 'Fox' ? 100 : name === 'RobotExpressive' ? 5 : 3) * 2).cycleDistance).toBeCloseTo(measured.cycleDistance * 2, 4);
  });

  it('uses a finite conservative height fallback for incomplete or untrusted data', () => {
    const { root } = rig();
    const fallback = { source: 'height-fallback', cycleDistance: 1 };
    expect(estimateWalkingCycleDistance(root, undefined, 2)).toEqual(fallback);
    expect(estimateWalkingCycleDistance(root, new THREE.AnimationClip('Pose', 1, []), 2)).toEqual(fallback);
    const bad = footClip(); bad.tracks[0].values[0] = Infinity;
    expect(estimateWalkingCycleDistance(root, bad, 2)).toEqual(fallback);
    let parent = root;
    for (let index = 0; index < 140; index++) { const node = new THREE.Group(); parent.add(node); parent = node; }
    expect(estimateWalkingCycleDistance(root, footClip(), 2)).toEqual(fallback);
    expect(proceduralCycleDistance(NaN)).toBe(1.45);
  });

  it('bounds duplicated skin data before cloning meshes that share a skeleton', () => {
    const { root, hips, feet } = rig();
    const bones = [hips, ...feet, ...Array.from({ length: 253 }, () => new THREE.Bone())];
    for (const bone of bones.slice(3)) hips.add(bone);
    const skeleton = new THREE.Skeleton(bones);
    const geometry = new THREE.BufferGeometry(), material = new THREE.MeshBasicMaterial();
    const clone = vi.spyOn(root, 'clone');
    try {
      for (let mesh = 0; mesh < 2; mesh++) {
        const skin = new THREE.SkinnedMesh(geometry, material); skin.bind(skeleton, new THREE.Matrix4()); root.add(skin);
      }
      expect(estimateWalkingCycleDistance(root, footClip(), 2).source).toBe('foot-motion');
      expect(clone).toHaveBeenCalledOnce(); clone.mockClear();
      for (let mesh = 2; mesh < 65; mesh++) {
        const skin = new THREE.SkinnedMesh(geometry, material); skin.bind(skeleton, new THREE.Matrix4()); root.add(skin);
      }
      expect(estimateWalkingCycleDistance(root, footClip(), 2).source).toBe('height-fallback');
      expect(clone).not.toHaveBeenCalled();
    } finally { clone.mockRestore(); skeleton.dispose(); geometry.dispose(); material.dispose(); }
  });
});
