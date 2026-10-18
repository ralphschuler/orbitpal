export interface ExternalEmotePlaybackEvent {
  botId: string;
  output: string;
  id: string;
  playing: boolean;
}

const LEASE_MS = 16_000;
const MAX_ENTRIES = 64;
const MAX_IDENTIFIER_LENGTH = 256;

interface Lease { timer: ReturnType<typeof setTimeout> }

function identifier(value: unknown): value is string {
  return typeof value === 'string' && value.length <= MAX_IDENTIFIER_LENGTH && value.trim().length > 0;
}

function validEvent(event: unknown): event is ExternalEmotePlaybackEvent {
  if (typeof event !== 'object' || event === null || Array.isArray(event)) return false;
  const value = event as Record<string, unknown>;
  return identifier(value.botId) && identifier(value.id)
    && (value.output === 'obs' || value.output === 'overlay')
    && typeof value.playing === 'boolean';
}

/** Tracks host-validated external SFX reports to pause microphone activation.
 * Heartbeats renew a bounded lease, so a closed output cannot block forever.
 * This does not grant trust to browser reports; the native host validates them.
 */
export class ExternalEmotePlayback {
  private leases = new Map<string, Lease>();
  private playing = false;
  private destroyed = false;

  constructor(private onChange: (playing: boolean) => void) {}

  accept(event: ExternalEmotePlaybackEvent): void {
    if (this.destroyed || !validEvent(event)) return;
    // Tuple encoding avoids collisions between identifiers containing separators.
    const key = JSON.stringify([event.botId, event.output, event.id]);
    const previous = this.leases.get(key);
    if (!event.playing) {
      if (!previous) return;
      clearTimeout(previous.timer);
      this.leases.delete(key);
      this.update();
      return;
    }
    // Keep existing playback protected when an output exceeds the entry bound.
    if (!previous && this.leases.size >= MAX_ENTRIES) return;
    if (previous) clearTimeout(previous.timer);
    const lease: Lease = { timer: setTimeout(() => {
      // Identity also rejects a timer already queued before renewal or clear().
      if (this.destroyed || this.leases.get(key) !== lease) return;
      this.leases.delete(key);
      this.update();
    }, LEASE_MS) };
    this.leases.set(key, lease);
    this.update();
  }

  clear(): void {
    for (const lease of this.leases.values()) clearTimeout(lease.timer);
    this.leases.clear();
    this.update();
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.clear();
  }

  private update(): void {
    const playing = this.leases.size > 0;
    if (playing === this.playing) return;
    this.playing = playing;
    this.onChange(playing);
  }
}
