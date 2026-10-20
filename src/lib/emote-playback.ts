import { canonicalSpeechId, type SoundActivity } from './sound-playback';

export interface ExternalEmotePlaybackEvent {
  botId: string;
  output: string;
  id: string;
  playing: boolean;
  activity?: SoundActivity;
  speechId?: string;
}

const LEASE_MS = 16_000;
const MAX_ENTRIES = 64;
const MAX_IDENTIFIER_LENGTH = 256;

export interface SoundOwner { botId: string; output: string; speechId: string }
interface Lease { timer: ReturnType<typeof setTimeout>; activity: SoundActivity; botId: string; output: string; speechId?: string }

function identifier(value: unknown): value is string {
  return typeof value === 'string' && value.length <= MAX_IDENTIFIER_LENGTH && value.trim().length > 0;
}

function validEvent(event: unknown): event is ExternalEmotePlaybackEvent {
  if (typeof event !== 'object' || event === null || Array.isArray(event)) return false;
  const value = event as Record<string, unknown>;
  return identifier(value.botId) && identifier(value.id)
    && (value.output === 'obs' || value.output === 'overlay')
    && typeof value.playing === 'boolean'
    && (value.activity === undefined || value.activity === 'foreground' || value.activity === 'ambient');
}

/** Tracks host-validated external SFX reports to pause microphone activation.
 * Heartbeats renew a bounded lease, so a closed output cannot block forever.
 * This does not grant trust to browser reports; the native host validates them.
 */
export class ExternalEmotePlayback {
  private leases = new Map<string, Lease>();
  private playing = false;
  private blocking = false;
  private destroyed = false;

  constructor(private onChange: (playing: boolean) => void, private onBlockingChange?: (blocking: boolean) => void, private onRevision?: () => void) {}

  accept(event: ExternalEmotePlaybackEvent): void {
    if (this.destroyed || !validEvent(event)) return;
    // Tuple encoding avoids collisions between identifiers containing separators.
    const key = JSON.stringify([event.botId, event.output, event.id]);
    const previous = this.leases.get(key);
    const activity = event.activity ?? 'foreground';
    const speechId = canonicalSpeechId(event.speechId) ? event.speechId : undefined;
    // A late/mismatched report cannot reclassify or end another activity's lease.
    if (previous && previous.activity !== activity) return;
    if (previous && (event.playing || event.speechId !== undefined) && previous.speechId !== speechId) return;
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
    const lease: Lease = { activity, botId: event.botId, output: event.output, ...(speechId ? { speechId } : {}), timer: setTimeout(() => {
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

  /** Only host-validated speech ownership may exempt an exact native turn.
   * Legacy/missing association and all foreign/manual leases still block. */
  blocksExcept(owner?: SoundOwner): boolean {
    return [...this.leases.values()].some(lease => lease.activity !== 'ambient' && !(owner && canonicalSpeechId(owner.speechId) && lease.speechId === owner.speechId && lease.botId === owner.botId && lease.output === owner.output));
  }

  private update(): void {
    this.onRevision?.();
    const playing = this.leases.size > 0;
    if (playing !== this.playing) { this.playing = playing; this.onChange(playing); }
    const blocking = [...this.leases.values()].some(lease => lease.activity !== 'ambient');
    if (blocking !== this.blocking) { this.blocking = blocking; this.onBlockingChange?.(blocking); }
  }
}
