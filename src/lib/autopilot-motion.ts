import * as THREE from 'three';
import { selectEmoteAnimation } from './avatar-animation';

/** Runtime-only, operator-started visual session shared by every output. */
export interface AutopilotVisualConfig {
  active: boolean;
  wander: boolean;
  gestures: boolean;
  startedAt: number;
  seed: string;
}

export interface AmbientMotionFrame {
  active: boolean;
  cycle: number;
  ageSeconds: number;
  x: number;
  walking: boolean;
  gesture?: { key: string; choice: number; elapsed: number; duration: number };
}

const CYCLE_MS = 45_000;
const inactive = (): AmbientMotionFrame => ({ active: false, cycle: 0, ageSeconds: 0, x: 0, walking: false });

function fraction(seed: string, cycle: number, label: string): number {
  let hash = 2166136261;
  const value = `${seed.slice(0, 128)}:${cycle}:${label}`;
  for (let index = 0; index < value.length; index++) hash = Math.imul(hash ^ value.charCodeAt(index), 16777619);
  return (hash >>> 0) / 0x100000000;
}

/** Pure seeded choreography: no model requests, independent renderer timers,
 * persisted positions or random draws during animation frames. */
export function sampleAmbientMotion(config: AutopilotVisualConfig | undefined, now: number): AmbientMotionFrame {
  if (!config?.active || (!config.wander && !config.gestures) || typeof config.seed !== 'string'
    || !Number.isFinite(config.startedAt) || !Number.isFinite(now) || now < config.startedAt) return inactive();
  const age = now - config.startedAt;
  if (!Number.isSafeInteger(Math.floor(age)) || age < 0) return inactive();
  const cycle = Math.floor(age / CYCLE_MS);
  const within = age % CYCLE_MS;
  const endpoint = (index: number) => index < 0 ? 0 : ((index % 2 === 0) === (fraction(config.seed, 0, 'direction') > 0.5) ? 1 : -1) * (0.4 + fraction(config.seed, index, 'endpoint') * 0.25);
  const from = endpoint(cycle - 1);
  const to = endpoint(cycle);
  const walkAt = cycle === 0 ? 2500 + fraction(config.seed, cycle, 'pause') * 2500 : 9000 + fraction(config.seed, cycle, 'pause') * 6000;
  const walkDuration = 4500 + fraction(config.seed, cycle, 'walk') * 2500;
  const rawProgress = THREE.MathUtils.clamp((within - walkAt) / walkDuration, 0, 1);
  const progress = rawProgress * rawProgress * (3 - 2 * rawProgress);
  const gestureAt = walkAt + walkDuration + 3000 + fraction(config.seed, cycle, 'after-walk') * 2500;
  const gestureDuration = 2600 + fraction(config.seed, cycle, 'gesture-duration') * 900;
  return {
    active: true, cycle, ageSeconds: age / 1000,
    x: config.wander ? THREE.MathUtils.lerp(from, to, progress) : 0,
    walking: config.wander && rawProgress > 0 && rawProgress < 1,
    ...(config.gestures && within >= gestureAt && within < gestureAt + gestureDuration
      ? { gesture: { key: `${config.startedAt}:${config.seed.slice(0, 128)}:${cycle}`, choice: fraction(config.seed, cycle, 'gesture-choice'), elapsed: (within - gestureAt) / 1000, duration: gestureDuration / 1000 } }
      : {}),
  };
}

/** Bound every travel lane by the current camera and the turning model's
 * horizontal radius. A narrow portrait may legitimately leave no room. */
export function motionLaneHalfWidth(distance: number, verticalFovDegrees: number, aspect: number, modelRadius: number): number {
  if (![distance, verticalFovDegrees, aspect, modelRadius].every(Number.isFinite)
    || distance <= 0 || verticalFovDegrees <= 0 || verticalFovDegrees >= 170 || aspect <= 0 || modelRadius < 0) return 0;
  const visible = distance * Math.tan(THREE.MathUtils.degToRad(verticalFovDegrees) / 2) * aspect;
  return Math.max(0, Math.min(2.2, visible - modelRadius - 0.14));
}

/** A bounded follower prevents a paused conversation from causing a position
 * jump when the deterministic timeline resumes later in its cycle. */
export function followAmbientPosition(current: number, target: number, delta: number, blocked: boolean): number {
  if (!Number.isFinite(current)) current = 0;
  if (!Number.isFinite(target)) target = 0;
  current = THREE.MathUtils.clamp(current, -0.65, 0.65);
  target = THREE.MathUtils.clamp(target, -0.65, 0.65);
  if (blocked || !Number.isFinite(delta) || delta <= 0) return current;
  const step = Math.min(delta, 0.1) * 0.32;
  return current + THREE.MathUtils.clamp(target - current, -step, step);
}

export function selectWalkingAnimation(clips: readonly THREE.AnimationClip[]): THREE.AnimationClip | undefined {
  for (const name of ['walk', 'walking', 'walking_a', 'walk_forward', 'walking_forward', 'walkforward', 'walkingforward']) {
    const matches = clips.filter((clip) => Number.isFinite(clip.duration) && clip.duration > 0.1 && clip.name.toLowerCase().replace(/[ .-]+/g, '_') === name);
    if (matches.length === 1) return matches[0];
  }
  return undefined;
}

/** Keep ambient actions friendly. Attack, death, hiding and arbitrary clips
 * are never selected just because an imported file contains them. */
export function ambientGestureActions(clips: readonly THREE.AnimationClip[], procedural: readonly string[]): string[] {
  return ['wave', 'nod', 'celebrate'].filter((action) => {
    try {
      const clip = selectEmoteAnimation(clips, action);
      if (clip) return action !== 'celebrate' || /^cheer$/i.test(clip.name);
      return procedural.includes(action);
    } catch { return false; }
  });
}

/** Small automatic humanoid stride, expressed in the normalized body frame. */
export function sampleWalkingGait(ageSeconds: number) {
  const phase = (Number.isFinite(ageSeconds) ? ageSeconds : 0) * Math.PI * 2 * 0.65;
  const left = Math.sin(phase); const right = -left;
  return {
    left, right, bob: (1 - Math.cos(phase * 2)) * 0.012,
    rotations: [
      ['leftUpperLeg', -left * 0.3], ['rightUpperLeg', -right * 0.3],
      ['leftLowerLeg', Math.max(0, -left) * 0.52], ['rightLowerLeg', Math.max(0, -right) * 0.52],
      ['leftFoot', left * 0.12], ['rightFoot', right * 0.12],
      ['leftUpperArm', left * 0.2], ['rightUpperArm', right * 0.2],
    ] as const,
  };
}

export interface WalkingLegs {
  leftUpperLeg: THREE.Object3D;
  leftLowerLeg: THREE.Object3D;
  leftFoot: THREE.Object3D;
  rightUpperLeg: THREE.Object3D;
  rightLowerLeg: THREE.Object3D;
  rightFoot: THREE.Object3D;
  rest: Map<THREE.Object3D, THREE.Quaternion>;
}

/** Only complete, connected known humanoid leg chains qualify for a gait. */
export function discoverWalkingLegs(root: THREE.Object3D): WalkingLegs | undefined {
  const names = new Map<string, THREE.Object3D[]>();
  root.traverse((node) => {
    if (!(node instanceof THREE.Bone)) return;
    const key = node.name.split(':').pop()!.toLowerCase().replace(/^mixamorig/, '').replace(/[^a-z0-9]/g, '');
    const matches = names.get(key) ?? []; matches.push(node); names.set(key, matches);
  });
  const find = (...aliases: string[]) => {
    const matches = [...new Set(aliases.flatMap((alias) => names.get(alias) ?? []))];
    return matches.length === 1 ? matches[0] : undefined;
  };
  const within = (node: THREE.Object3D, ancestor: THREE.Object3D): boolean => {
    for (let current = node.parent; current; current = current.parent) if (current === ancestor) return true;
    return false;
  };
  const hips = find('hips', 'pelvis'); const head = find('head');
  if (!hips || !head || !within(head, hips)) return;
  const up = root.worldToLocal(head.getWorldPosition(new THREE.Vector3())).sub(root.worldToLocal(hips.getWorldPosition(new THREE.Vector3()))).normalize();
  if (up.y < 0.65) return;
  const leg = (side: 'left' | 'right') => {
    const suffix = side[0];
    const upper = find(`${side}upleg`, `${side}upperleg`, `${side}thigh`, `upperleg${suffix}`);
    const lower = find(`${side}leg`, `${side}lowerleg`, `${side}calf`, `lowerleg${suffix}`);
    const foot = find(`${side}foot`, `foot${suffix}`);
    return upper && lower && foot && within(upper, hips) && within(lower, upper) && within(foot, lower) ? { upper, lower, foot } : undefined;
  };
  const left = leg('left'); const right = leg('right');
  if (!left || !right) return;
  const nodes = [left.upper, left.lower, left.foot, right.upper, right.lower, right.foot];
  if (nodes.some((node) => !Number.isFinite(node.position.lengthSq()))) return;
  return { leftUpperLeg: left.upper, leftLowerLeg: left.lower, leftFoot: left.foot, rightUpperLeg: right.upper, rightLowerLeg: right.lower, rightFoot: right.foot, rest: new Map(nodes.map((node) => [node, node.quaternion.clone()])) };
}
