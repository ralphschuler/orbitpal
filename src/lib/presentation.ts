/** Duration is expressed in seconds, matching the host's emote payload. */
export type EmotePayload = {
  action: string;
  nonce?: string | number;
  duration?: number;
  intensity?: number;
  [key: string]: unknown;
};

export function emoteDurationMilliseconds(emote: EmotePayload): number {
  const duration = Number(emote.duration);
  const motion = emote.action.startsWith('vrma:');
  return (Number.isFinite(duration) ? Math.max(motion ? 0.001 : 0.2, Math.min(motion ? 120 : 15, duration)) : 2.5) * 1000;
}

export function emoteSequenceMilliseconds(emotes: readonly EmotePayload[] = []): number {
  return emotes.reduce((total, emote) => total + (emote && typeof emote.action === 'string' && emote.action.trim() ? emoteDurationMilliseconds(emote) : 0), 0);
}

/** Start together with speech; a replacement or stop invalidates every timer. */
export class EmoteSequence {
  private generation = 0;
  private timers: ReturnType<typeof setTimeout>[] = [];

  constructor(private applyEmote: (emote: EmotePayload) => void) {}

  start(emotes: readonly EmotePayload[] = []): void {
    this.stop();
    const generation = this.generation;
    let delay = 0;
    for (const emote of emotes) {
      if (!emote || typeof emote.action !== 'string' || !emote.action.trim()) continue;
      const apply = () => {
        if (generation === this.generation) this.applyEmote(emote);
      };
      if (delay === 0) apply();
      else this.timers.push(setTimeout(apply, delay));
      delay += emoteDurationMilliseconds(emote);
    }
  }

  stop(): void {
    ++this.generation;
    for (const timer of this.timers) clearTimeout(timer);
    this.timers = [];
  }
}
