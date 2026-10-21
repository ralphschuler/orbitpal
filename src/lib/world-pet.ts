import * as THREE from 'three';
import { clipHasTarget } from './emote-capabilities';
import type { PetClip, PetFrame } from './world-output';

/** An actual authored clip only. There is no procedural or semantic fallback. */
export function admittedPetClip(root: THREE.Object3D, clips: readonly THREE.AnimationClip[], spec: PetClip): THREE.AnimationClip | null {
  if (!/^pet$/i.test(spec.action) || !Number.isSafeInteger(spec.durationMs) || spec.durationMs < 1 || spec.durationMs > 15000) return null;
  const matches = clips.filter(clip => clip.name === spec.action);
  if (matches.length !== 1) return null;
  const clip = matches[0];
  return Number.isFinite(clip.duration) && clip.duration > 0 && Math.ceil(clip.duration * 1000) === spec.durationMs
    && clipHasTarget(root, clip) ? clip : null;
}

/** Clip time advances only from fresh successful submitted poses after native
 * Start. A draw proposal alone cannot finish this activity. No sound or timers. */
export class WorldPetPlayer {
  private committedMs = 0;
  private lastFrameAt: number | null = null;
  private pending: { value: PetFrame; at: number } | null = null;
  private stopped = false;
  readonly action: THREE.AnimationAction;
  constructor(mixer: THREE.AnimationMixer, clip: THREE.AnimationClip, readonly spec: PetClip) {
    this.action = mixer.clipAction(clip);
    this.action.reset().setLoop(THREE.LoopOnce, 1).setEffectiveTimeScale(0).setEffectiveWeight(1).play();
    this.action.clampWhenFinished = true;
  }
  prepare(now: number, running: boolean): PetFrame {
    if (this.stopped || !Number.isFinite(now) || now < 0 || this.lastFrameAt !== null && (now < this.lastFrameAt || now - this.lastFrameAt > 250)) throw new Error('Pet frame unavailable');
    const advance = running && this.lastFrameAt !== null ? Math.min(50, now - this.lastFrameAt) : 0;
    // Integer milliseconds are the closed native wire contract. Keep the last
    // committed frame's fraction in its timestamp rather than inventing credit.
    const elapsedMs = Math.min(this.spec.durationMs, this.committedMs + Math.floor(advance));
    const value = Object.freeze({ action: this.spec.action, elapsedMs, finished: elapsedMs === this.spec.durationMs });
    this.action.time = elapsedMs / 1000;
    this.pending = { value, at: now };
    return value;
  }
  commit(value: PetFrame): boolean {
    if (this.stopped || this.pending?.value !== value) return false;
    this.committedMs = value.elapsedMs; this.lastFrameAt = this.pending.at; this.pending = null;
    return true;
  }
  rollback(): void { this.pending = null; this.action.time = this.committedMs / 1000; }
  stop(): void { this.stopped = true; this.pending = null; this.action.stop(); }
}
