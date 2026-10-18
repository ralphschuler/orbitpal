/** Duration is expressed in seconds, matching the host's emote payload. */
export type EmotePayload = {
  action: string;
  nonce?: string | number;
  duration?: number;
  intensity?: number;
  [key: string]: unknown;
};

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
      const duration = Number(emote.duration);
      const seconds = Number.isFinite(duration) ? Math.max(0.2, Math.min(15, duration)) : 2.5;
      const apply = () => {
        if (generation === this.generation) this.applyEmote(emote);
      };
      if (delay === 0) apply();
      else this.timers.push(setTimeout(apply, delay));
      delay += seconds * 1000;
    }
  }

  stop(): void {
    ++this.generation;
    for (const timer of this.timers) clearTimeout(timer);
    this.timers = [];
  }
}
