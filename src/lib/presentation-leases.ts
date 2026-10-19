export type PresentationPreparer = (signal: AbortSignal) => Promise<() => void>;
export interface PresentationTicket { readonly ready: Promise<void> }
interface Lease {
  controller: AbortController;
  release?: () => void;
  timer?: ReturnType<typeof setTimeout>;
  visualDone: boolean;
  unlisten: () => void;
}

function cancelled(): Error { return new DOMException('Die Figurenpräsentation wurde abgebrochen.', 'AbortError'); }

/** Renderer leases belong to exact presentations, including visual/SFX tails.
 * Call request immediately before playback, after audio preparation, so ready
 * refers to a fresh rendered frame rather than one from before slow synthesis. */
export class PresentationLeases {
  private leases = new Map<PresentationTicket, Lease>();
  constructor(private soundPlaying: () => boolean = () => false) {}

  request(prepare: PresentationPreparer, signal: AbortSignal): PresentationTicket {
    const controller = new AbortController();
    let fail!: (error: Error) => void;
    const abort = new Promise<never>((_, reject) => { fail = reject; });
    const lease: Lease = { controller, visualDone: false, unlisten: () => {} };
    const acquisition = Promise.resolve().then(() => {
      if (controller.signal.aborted) throw cancelled();
      return prepare(controller.signal);
    }).then(release => {
      if (controller.signal.aborted) { release(); throw cancelled(); }
      lease.release = release;
    });
    const ticket: PresentationTicket = { ready: Promise.race([acquisition, abort]) };
    this.leases.set(ticket, lease);
    const stop = () => this.cancel(ticket);
    const onAbort = () => fail(cancelled());
    signal.addEventListener('abort', stop, { once: true });
    controller.signal.addEventListener('abort', onAbort, { once: true });
    lease.unlisten = () => {
      signal.removeEventListener('abort', stop);
      controller.signal.removeEventListener('abort', onAbort);
    };
    // Install a rejection handler immediately; slow audio or an abandoned
    // caller must not turn an aborted renderer request into an unhandled error.
    void ticket.ready.catch(() => this.cancel(ticket));
    if (signal.aborted) this.cancel(ticket);
    return ticket;
  }

  finish(ticket: PresentationTicket, visualMilliseconds = 0): void {
    const lease = this.leases.get(ticket);
    if (!lease) return;
    if (lease.timer !== undefined) clearTimeout(lease.timer);
    const afterVisual = () => {
      if (this.leases.get(ticket) !== lease) return;
      lease.visualDone = true;
      if (!this.soundPlaying()) { this.cancel(ticket); return; }
      // Imported clips are capped at 15s; missing audio-end callbacks cannot
      // leave the character pinned forever.
      lease.timer = setTimeout(() => this.cancel(ticket), 16_000);
    };
    const delay = Number.isFinite(visualMilliseconds) ? Math.max(0, visualMilliseconds) : 0;
    if (delay) lease.timer = setTimeout(afterVisual, delay);
    else afterVisual();
  }

  soundEnded(): void {
    if (this.soundPlaying()) return;
    for (const [ticket, lease] of this.leases) if (lease.visualDone) this.cancel(ticket);
  }

  cancel(ticket: PresentationTicket): void {
    const lease = this.leases.get(ticket);
    if (!lease) return;
    this.leases.delete(ticket);
    if (lease.timer !== undefined) clearTimeout(lease.timer);
    lease.controller.abort();
    lease.unlisten();
    lease.release?.();
    lease.release = undefined;
  }

  clear(except?: PresentationTicket): void {
    for (const ticket of [...this.leases.keys()]) if (ticket !== except) this.cancel(ticket);
  }
}
