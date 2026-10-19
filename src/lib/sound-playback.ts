/** Only the renderer's own autonomous tracks are exempt from quiet time. */
export type SoundActivity = 'foreground' | 'ambient';
export type SoundPlaybackReport = { id: string; playing: boolean; activity: SoundActivity };

/** Ownership wins even when an explicit emote reuses the walking ClipAction. */
export function renderedModelSoundActivity(hasEmote: boolean, source?: 'manual' | 'ambient'): SoundActivity {
  return hasEmote && source !== 'ambient' ? 'foreground' : 'ambient';
}

export function foregroundSoundPlaying(effect: boolean, model: boolean, activity: SoundActivity): boolean {
  return effect || model && activity !== 'ambient';
}

/** Separate, immutable IDs keep overlapping foreground and ambient leases exact. */
export class SoundPlaybackReports {
  private ids = new Map<SoundActivity, string>();
  constructor(private createId = () => crypto.randomUUID() as string) {}

  update(foreground: boolean, ambient: boolean): SoundPlaybackReport[] {
    const active: SoundPlaybackReport[] = [], ended: SoundPlaybackReport[] = [];
    for (const activity of ['foreground', 'ambient'] as const) {
      const playing = activity === 'foreground' ? foreground : ambient;
      let id = this.ids.get(activity);
      if (playing && !id) { id = this.createId(); this.ids.set(activity, id); }
      if (!id) continue;
      (playing ? active : ended).push({ id, playing, activity });
      if (!playing) this.ids.delete(activity);
    }
    // Admit the replacement first so a kind switch cannot briefly unlock STT.
    return [...active, ...ended];
  }
}
