/** Only the renderer's own walking track is exempt from autonomous quiet time. */
export type SoundActivity = 'foreground' | 'ambient-walk';
export type SoundPlaybackReport = { id: string; playing: boolean; activity: SoundActivity };

export function foregroundSoundPlaying(effect: boolean, model: boolean, activity: SoundActivity): boolean {
  return effect || model && activity !== 'ambient-walk';
}

/** Separate, immutable IDs keep overlapping foreground and walking leases exact. */
export class SoundPlaybackReports {
  private ids = new Map<SoundActivity, string>();
  constructor(private createId = () => crypto.randomUUID() as string) {}

  update(foreground: boolean, ambientWalk: boolean): SoundPlaybackReport[] {
    const active: SoundPlaybackReport[] = [], ended: SoundPlaybackReport[] = [];
    for (const activity of ['foreground', 'ambient-walk'] as const) {
      const playing = activity === 'foreground' ? foreground : ambientWalk;
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
