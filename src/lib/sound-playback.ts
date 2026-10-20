/** Only the renderer's own autonomous tracks are exempt from quiet time. */
export type SoundActivity = 'foreground' | 'ambient';
export type SoundPlaybackReport = { id: string; playing: boolean; activity: SoundActivity; speechId?: string };
export function canonicalSpeechId(value: unknown): value is string { return typeof value === 'string' && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value); }

/** Ownership wins even when an explicit emote reuses the walking ClipAction. */
export function renderedModelSoundActivity(hasEmote: boolean, source?: 'manual' | 'ambient'): SoundActivity {
  return hasEmote && source !== 'ambient' ? 'foreground' : 'ambient';
}

export function foregroundSoundPlaying(effect: boolean, model: boolean, activity: SoundActivity): boolean {
  return effect || model && activity !== 'ambient';
}

/** Separate, immutable IDs keep overlapping foreground and ambient leases exact. */
export class SoundPlaybackReports {
  private ids = new Map<SoundActivity, { id: string; speechId?: string }>();
  constructor(private createId = () => crypto.randomUUID() as string) {}

  update(foreground: boolean, ambient: boolean, parent?: string): SoundPlaybackReport[] {
    const active: SoundPlaybackReport[] = [], ended: SoundPlaybackReport[] = [];
    for (const activity of ['foreground', 'ambient'] as const) {
      const playing = activity === 'foreground' ? foreground : ambient;
      const speechId = activity === 'foreground' && canonicalSpeechId(parent) ? parent : undefined;
      let lease = this.ids.get(activity);
      if (lease && playing && lease.speechId !== speechId) { ended.push({ ...lease, playing: false, activity }); this.ids.delete(activity); lease = undefined; }
      if (playing && !lease) { lease = { id: this.createId(), ...(speechId ? { speechId } : {}) }; this.ids.set(activity, lease); }
      if (!lease) continue;
      (playing ? active : ended).push({ ...lease, playing, activity });
      if (!playing) this.ids.delete(activity);
    }
    // Admit the replacement first so a kind switch cannot briefly unlock STT.
    return [...active, ...ended];
  }
}
