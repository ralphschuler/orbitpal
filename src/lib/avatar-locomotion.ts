import * as THREE from 'three';
import { clone as cloneSkeleton } from 'three/addons/utils/SkeletonUtils.js';

export interface WalkingCycleDistance {
  /** Distance per complete left/right stride, in the supplied root's local units. */
  cycleDistance: number;
  source: 'root-motion' | 'foot-motion' | 'height-fallback';
}

/** The automatic humanoid gait has a 0.3-radian leg swing. A full left/right
 * cycle covers about half the height of an ordinarily proportioned character. */
export function proceduralCycleDistance(modelHeight: number): number {
  return (Number.isFinite(modelHeight) && modelHeight > 0.001 ? modelHeight : 2.9) * 0.5;
}

/** Distance, rather than session age or wall time, advances the walk. Callers
 * supply actual movement after collision, pause and presentation constraints. */
export class DistanceWalkClock {
  private session = '';
  cycles = 0;

  reset(session = ''): void { this.session = session; this.cycles = 0; }

  advance(distance: number, cycleDistance: number, session = this.session): number {
    if (session !== this.session) this.reset(session);
    if (Number.isFinite(distance) && distance > 0 && Number.isFinite(cycleDistance) && cycleDistance > 0.000001) {
      const next = this.cycles + distance / cycleDistance;
      if (Number.isFinite(next)) this.cycles = next;
    }
    return this.cycles;
  }
}

const SAMPLE_INTERVALS = 64;
const MAX_NODES = 4096;
const MAX_DEPTH = 128;
const MAX_CLONED_SKIN_REFERENCES = 32768;
const MAX_TRACKS = 512;
const MAX_TRACK_VALUES = 262144;
const key = (name: string) => name.split(':').pop()!.toLowerCase().replace(/^mixamorig/, '').replace(/[^a-z0-9]/g, '');

function safeToSample(root: THREE.Object3D, clip: THREE.AnimationClip): boolean {
  if (!Number.isFinite(clip.duration) || clip.duration <= 0.1 || clip.duration > 600 || !clip.tracks.length || clip.tracks.length > MAX_TRACKS) return false;
  const pending = [{ node: root, depth: 0 }];
  let count = 0; let skinReferences = 0;
  while (pending.length) {
    const { node, depth } = pending.pop()!;
    if (++count > MAX_NODES || depth > MAX_DEPTH || count + pending.length + node.children.length > MAX_NODES) return false;
    // SkeletonUtils duplicates bone arrays/matrices for every mesh, even
    // when many source meshes share one skeleton. Bound that work before
    // cloning; hierarchy size alone cannot bound the extra allocation.
    const skin = node as THREE.SkinnedMesh;
    if (skin.isSkinnedMesh) {
      const skeleton = skin.skeleton;
      if (!skeleton || skeleton.boneInverses.length !== skeleton.bones.length) return false;
      skinReferences += skeleton.bones.length + skeleton.boneInverses.length;
      if (skinReferences > MAX_CLONED_SKIN_REFERENCES) return false;
    }
    for (const child of node.children) pending.push({ node: child, depth: depth + 1 });
  }
  let values = 0;
  return clip.tracks.every(track => {
    values += track.times.length + track.values.length;
    return values <= MAX_TRACK_VALUES && track.times.length > 0
      && Array.from(track.times).every(Number.isFinite) && Array.from(track.values).every(Number.isFinite);
  });
}

/** Principal horizontal excursion. Body-relative points remove pelvic sway;
 * a stance occupies roughly half a complete left/right walking cycle. */
function horizontalSpan(points: readonly THREE.Vector3[]): number {
  const meanX = points.reduce((total, point) => total + point.x, 0) / points.length;
  const meanZ = points.reduce((total, point) => total + point.z, 0) / points.length;
  let xx = 0; let zz = 0; let xz = 0;
  for (const point of points) {
    const x = point.x - meanX; const z = point.z - meanZ;
    xx += x * x; zz += z * z; xz += x * z;
  }
  const angle = Math.atan2(2 * xz, xx - zz) / 2;
  const x = Math.cos(angle); const z = Math.sin(angle);
  let minimum = Infinity; let maximum = -Infinity;
  for (const point of points) { const projection = point.x * x + point.z * z; minimum = Math.min(minimum, projection); maximum = Math.max(maximum, projection); }
  return maximum - minimum;
}

/** Calibrate once on asset load, never in the animation frame. Only a caller's
 * explicitly selected walking clip is measured; no arbitrary animation is
 * promoted to a walk. Sampling a bounded skeleton clone leaves the displayed
 * model, its mixer, geometry, materials and existing pose untouched. */
export function estimateWalkingCycleDistance(root: THREE.Object3D, clip: THREE.AnimationClip | undefined, modelHeight: number): WalkingCycleDistance {
  const fallback: WalkingCycleDistance = { cycleDistance: proceduralCycleDistance(modelHeight), source: 'height-fallback' };
  if (!clip || !safeToSample(root, clip)) return fallback;
  const height = fallback.cycleDistance * 2;
  let copy: THREE.Object3D | undefined;
  let mixer: THREE.AnimationMixer | undefined;
  try {
    copy = cloneSkeleton(root);
    // Ignore transforms belonging to a parent outside the measured model.
    copy.position.set(0, 0, 0); copy.quaternion.identity(); copy.scale.set(1, 1, 1);
    copy.updateMatrixWorld(true);
    const feet: THREE.Object3D[] = [];
    const anchors: THREE.Object3D[] = [];
    copy.traverse(node => {
      const name = key(node.name);
      if (!(node instanceof THREE.Mesh) && /^(?:b)?(?:left|right)?foot(?:[lr]|\d+)?(?:\d+)?$/.test(name)) feet.push(node);
      if (!(node instanceof THREE.Mesh) && /^(?:b)?(?:hips?|pelvis|body|root)(?:\d+)?$/.test(name)) anchors.push(node);
    });
    // Bone endpoints, toe/control helpers and arbitrary mesh names do not
    // become extra feet. Known imported rigs have at most four real feet.
    const measuredFeet = feet.slice(0, 8);
    const measuredAnchors = anchors.slice(0, 8);
    const body = measuredAnchors.find(node => /(?:hip|pelvis|body)/.test(key(node.name))) ?? measuredAnchors[0];
    if (!body && measuredFeet.length < 2) return fallback;
    const transforms = clip.tracks.filter(track => /\.(?:position|quaternion|scale)$/.test(track.name));
    if (!transforms.length) return fallback;
    mixer = new THREE.AnimationMixer(copy);
    const sampledClip = new THREE.AnimationClip(clip.name, clip.duration, transforms);
    const action = mixer.clipAction(sampledClip);
    action.setLoop(THREE.LoopOnce, 1); action.clampWhenFinished = true; action.play();
    const anchorPoints = measuredAnchors.map(() => [] as THREE.Vector3[]);
    const footPoints = measuredFeet.map(() => [] as THREE.Vector3[]);
    const inverse = new THREE.Matrix4(); const bodyPosition = new THREE.Vector3();
    for (let sample = 0; sample <= SAMPLE_INTERVALS; sample++) {
      mixer.setTime(clip.duration * sample / SAMPLE_INTERVALS);
      copy.updateMatrixWorld(true);
      inverse.copy(copy.matrixWorld).invert();
      body ? bodyPosition.setFromMatrixPosition(body.matrixWorld).applyMatrix4(inverse) : bodyPosition.set(0, 0, 0);
      measuredAnchors.forEach((node, index) => anchorPoints[index].push(new THREE.Vector3().setFromMatrixPosition(node.matrixWorld).applyMatrix4(inverse)));
      measuredFeet.forEach((node, index) => footPoints[index].push(new THREE.Vector3().setFromMatrixPosition(node.matrixWorld).applyMatrix4(inverse).sub(bodyPosition)));
    }
    if ([...anchorPoints, ...footPoints].some(points => points.some(point => ![point.x, point.y, point.z].every(Number.isFinite)))) return fallback;
    for (const points of anchorPoints) {
      const start = points[0]; const end = points[points.length - 1];
      const distance = Math.hypot(end.x - start.x, end.z - start.z);
      let path = 0;
      for (let index = 1; index < points.length; index++) path += Math.hypot(points[index].x - points[index - 1].x, points[index].z - points[index - 1].z);
      // A translating root is authoritative. Reject tiny sway, implausible
      // distances and looping root wobble that ends near its starting point.
      if (distance >= height * 0.04 && distance <= height * 4 && distance >= path * 0.8) return { cycleDistance: distance, source: 'root-motion' };
    }
    const strides = footPoints.map(points => horizontalSpan(points) * 2).filter(distance => distance >= height * 0.08 && distance <= height * 3).sort((a, b) => a - b);
    if (strides.length >= 2) {
      const middle = Math.floor(strides.length / 2);
      const cycleDistance = strides.length % 2 ? strides[middle] : (strides[middle - 1] + strides[middle]) / 2;
      return { cycleDistance, source: 'foot-motion' };
    }
    return fallback;
  } catch { return fallback; }
  finally { mixer?.stopAllAction(); if (copy) mixer?.uncacheRoot(copy); }
}
