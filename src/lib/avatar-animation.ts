import * as THREE from 'three';
import { VRMExpressionMorphTargetBind, type VRM, type VRMExpression } from '@pixiv/three-vrm';

type BoundProperty = { node: THREE.Object3D; property: string; index?: string };

/** Track ownership follows the actual active clip, not every clip in a file. */
export class AuthoredAnimationProperties {
  private cache = new WeakMap<THREE.AnimationClip, BoundProperty[]>();
  private previousTransforms = new Map<THREE.Object3D, Map<string, THREE.Vector3 | THREE.Quaternion>>();
  private previousMorphs = new Map<THREE.Mesh, Map<number, number>>();
  private previousWeights = new Map<THREE.Object3D, number>();

  constructor(private root: THREE.Object3D) {}

  private bindings(clip: THREE.AnimationClip): BoundProperty[] {
    let bindings = this.cache.get(clip);
    if (bindings) return bindings;
    bindings = [];
    for (const track of clip.tracks) {
      try {
        const parsed = THREE.PropertyBinding.parseTrackName(track.name);
        let node = THREE.PropertyBinding.findNode(this.root, parsed.nodeName) as THREE.Object3D | null;
        if (parsed.objectName === 'bones' && parsed.objectIndex !== undefined) {
          const skeleton = (node as THREE.SkinnedMesh | undefined)?.skeleton;
          node = skeleton?.getBoneByName(parsed.objectIndex) ?? null;
        } else if (parsed.objectName) continue;
        if (node) bindings.push({ node, property: parsed.propertyName, index: parsed.propertyIndex });
      } catch { /* The mixer reports malformed/unbound imported tracks itself. */ }
    }
    this.cache.set(clip, bindings);
    return bindings;
  }

  private active(actions: readonly (THREE.AnimationAction | undefined)[]) {
    return actions.filter((action): action is THREE.AnimationAction =>
      Boolean(action?.enabled && action.isScheduled() && action.getEffectiveWeight() > 0.00001));
  }

  owns(actions: readonly (THREE.AnimationAction | undefined)[], node: THREE.Object3D, property: string, index?: number): boolean {
    return this.active(actions).some((action) => this.bindings(action.getClip()).some((binding) => {
      if (binding.node !== node || binding.property !== property) return false;
      if (index === undefined || binding.index === undefined) return true;
      const resolved = property === 'morphTargetInfluences'
        ? (node as THREE.Mesh).morphTargetDictionary?.[binding.index] ?? Number(binding.index)
        : Number(binding.index);
      return resolved === index;
    }));
  }

  /** Mixer bindings skip unchanged keys; restore their last authored values
   * before evaluation if the procedural humanoid transfer touched a bone. */
  restoreTransforms(actions: readonly (THREE.AnimationAction | undefined)[]): void {
    for (const action of this.active(actions)) for (const { node, property } of this.bindings(action.getClip())) {
      const value = this.previousTransforms.get(node)?.get(property);
      if (value instanceof THREE.Quaternion && property === 'quaternion') node.quaternion.copy(value);
      else if (value instanceof THREE.Vector3 && property === 'position') node.position.copy(value);
      else if (value instanceof THREE.Vector3 && property === 'scale') node.scale.copy(value);
      else if (property === 'weight' && this.previousWeights.has(node)) (node as VRMExpression).weight = this.previousWeights.get(node)!;
    }
    for (const [mesh, values] of this.previousMorphs) for (const [index, value] of values) {
      if (this.owns(actions, mesh, 'morphTargetInfluences', index)) mesh.morphTargetInfluences![index] = value;
    }
  }

  captureTransforms(actions: readonly (THREE.AnimationAction | undefined)[]): void {
    const active = this.active(actions);
    if (!active.length) { this.previousTransforms.clear(); this.previousWeights.clear(); }
    for (const action of active) for (const { node, property } of this.bindings(action.getClip())) {
      if (property === 'weight' && typeof (node as VRMExpression).weight === 'number') this.previousWeights.set(node, (node as VRMExpression).weight);
      if (!['quaternion', 'position', 'scale'].includes(property)) continue;
      let values = this.previousTransforms.get(node);
      if (!values) { values = new Map(); this.previousTransforms.set(node, values); }
      const previous = values.get(property);
      if (property === 'quaternion') {
        if (previous instanceof THREE.Quaternion) previous.copy(node.quaternion);
        else values.set(property, node.quaternion.clone());
      } else {
        const value = property === 'position' ? node.position : node.scale;
        if (previous instanceof THREE.Vector3) previous.copy(value);
        else values.set(property, value.clone());
      }
    }
    this.previousMorphs = this.currentMorphs(actions);
  }

  private currentMorphs(actions: readonly (THREE.AnimationAction | undefined)[]) {
    const values = new Map<THREE.Mesh, Map<number, number>>();
    for (const action of this.active(actions)) for (const binding of this.bindings(action.getClip())) {
      if (binding.property !== 'morphTargetInfluences') continue;
      const mesh = binding.node as THREE.Mesh;
      if (!mesh.morphTargetInfluences) continue;
      const indices = binding.index === undefined
        ? mesh.morphTargetInfluences.map((_, index) => index)
        : [mesh.morphTargetDictionary?.[binding.index] ?? Number(binding.index)];
      let captured = values.get(mesh);
      if (!captured) { captured = new Map(); values.set(mesh, captured); }
      for (const index of indices) if (Number.isInteger(index) && index >= 0 && index < mesh.morphTargetInfluences.length) captured.set(index, mesh.morphTargetInfluences[index]);
    }
    return values;
  }

  /** Preserve authored face weights, except available live-speech visemes. */
  captureMorphs(actions: readonly (THREE.AnimationAction | undefined)[], speechMouths?: ReadonlyMap<THREE.Mesh, ReadonlySet<number>>): () => void {
    const values = this.currentMorphs(actions);
    return () => {
      for (const [mesh, captured] of values) for (const [index, value] of captured) {
        if (!speechMouths?.get(mesh)?.has(index)) mesh.morphTargetInfluences![index] = value;
      }
    };
  }
}

/**
 * The normalized pose has already been transferred before the authored mixer.
 * Keep the library's look-at, expression, constraint, spring-bone and MToon
 * updates, but do not transfer the humanoid twice and erase imported clips.
 */
export function finishVrmAnimationFrame(vrm: VRM, delta: number, properties?: AuthoredAnimationProperties, actions: readonly (THREE.AnimationAction | undefined)[] = [], speechMouths: readonly VRMExpression[] = []): void {
  const mouthIndices = new Map<THREE.Mesh, Set<number>>();
  for (const expression of speechMouths) for (const bind of expression.binds) if (bind instanceof VRMExpressionMorphTargetBind) {
    for (const mesh of bind.primitives) {
      const indices = mouthIndices.get(mesh) ?? new Set<number>(); indices.add(bind.index); mouthIndices.set(mesh, indices);
    }
  }
  const restoreMorphs = properties?.captureMorphs(actions, mouthIndices);
  const mutedExpressions = speechMouths.length ? (vrm.expressionManager?.expressions ?? []).filter((expression) => expression.overrideMouth !== 'none' && !speechMouths.includes(expression)).map((expression) => ({ expression, weight: expression.weight })) : [];
  // Preserve the authored weight for the next frame. Suppression lasts only
  // for this audible frame, so cached/paused expression tracks recover too.
  for (const { expression } of mutedExpressions) expression.weight = 0;
  const eyes = (['leftEye', 'rightEye'] as const).flatMap((name) => {
    const node = vrm.humanoid.getRawBoneNode(name);
    return node && properties?.owns(actions, node, 'quaternion') ? [{ node, quaternion: node.quaternion.clone() }] : [];
  });
  const automatic = vrm.humanoid.autoUpdateHumanBones;
  vrm.humanoid.autoUpdateHumanBones = false;
  try { vrm.update(delta); }
  finally {
    vrm.humanoid.autoUpdateHumanBones = automatic;
    restoreMorphs?.();
    for (const { expression, weight } of mutedExpressions) expression.weight = weight;
    for (const { node, quaternion } of eyes) node.quaternion.copy(quaternion);
  }
}

export function selectIdleAnimation(clips: readonly THREE.AnimationClip[]): THREE.AnimationClip | undefined {
  return clips.find((clip) => /^idle$/i.test(clip.name))
    ?? clips.find((clip) => /^unarmed_idle$/i.test(clip.name))
    ?? clips.find((clip) => /^survey$/i.test(clip.name))
    ?? clips.find((clip) => /^(rest|stand|standing|idle_b)$/i.test(clip.name))
    ?? clips.find((clip) => /(?:^|[_ .-])idle(?:$|[_ .-])/i.test(clip.name));
}

/** Exact imported names take priority; semantic aliases are case insensitive. */
export function selectEmoteAnimation(clips: readonly THREE.AnimationClip[], action: string): THREE.AnimationClip | undefined {
  const exact = clips.filter((clip) => clip.name === action);
  if (exact.length > 1) throw new Error(`Animation „${action}“ ist mehrfach vorhanden. Verwende im Modell eindeutige Namen.`);
  if (exact.length === 1) return exact[0];
  const insensitive = clips.filter((clip) => clip.name.toLowerCase() === action.toLowerCase());
  if (insensitive.length > 1) throw new Error(`Animation „${action}“ ist nicht eindeutig. Verwende den vollständigen Namen aus der Animationsliste.`);
  if (insensitive.length === 1) return insensitive[0];
  const aliases: Record<string, readonly string[]> = { wave: ['wave'], celebrate: ['cheer', 'jump', 'jump_full_short'], nod: ['yes'], dance: ['dance'] };
  for (const alias of aliases[action] ?? []) {
    const matches = clips.filter((clip) => clip.name.toLowerCase() === alias);
    if (matches.length > 1) throw new Error(`Animation „${alias}“ ist mehrfach vorhanden. Verwende im Modell eindeutige Namen.`);
    if (matches.length === 1) return matches[0];
  }
  return undefined;
}
