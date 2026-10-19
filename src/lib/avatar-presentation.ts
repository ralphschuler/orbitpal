export type PresentationRelease = () => void;
interface Lease {
  signal: AbortSignal;
  abort: () => void;
  timer?: ReturnType<typeof setTimeout>;
  resolve?: (release: PresentationRelease) => void;
  reject?: (error: Error) => void;
}
export function presentationAbortError(): Error {
  return Object.assign(new Error('Die Figurenpräsentation wurde abgebrochen.'), { name: 'AbortError' });
}

/** A presentation owns its hold from acquisition through the caller's release.
 * A previous ready frame is deliberately never cached. The renderer must report
 * a new fully ready frame after every acquisition, including concurrent ones. */
export class AvatarPresentationGate {
  private leases = new Set<Lease>();
  private disposed = false;
  private failure?: Error;
  constructor(private timeoutMs = 20_000) {}
  get requested(): boolean { return this.leases.size > 0; }

  acquire(signal: AbortSignal): Promise<PresentationRelease> {
    if (signal.aborted || this.disposed) return Promise.reject(presentationAbortError());
    if (this.failure) return Promise.reject(this.failure);
    return new Promise((resolve, reject) => {
      const lease: Lease = { signal, resolve, reject, abort: () => this.release(lease, presentationAbortError()) };
      this.leases.add(lease);
      signal.addEventListener('abort', lease.abort, { once: true });
      lease.timer = setTimeout(() => this.release(lease, new Error('Die Figur ist nicht rechtzeitig sichtbar und bereit geworden.')), this.timeoutMs);
      // Abort may already have happened while a custom event source was bound.
      if (signal.aborted) lease.abort();
    });
  }

  /** Called only AFTER an actual successful render, never by a motion update. */
  rendered(ready: boolean): void {
    if (!ready || this.disposed || this.failure) return;
    for (const lease of this.leases) {
      if (!lease.resolve) continue;
      if (lease.signal.aborted) { lease.abort(); continue; }
      clearTimeout(lease.timer); lease.timer = undefined;
      const resolve = lease.resolve;
      lease.resolve = undefined; lease.reject = undefined;
      resolve(() => this.release(lease));
    }
  }

  private release(lease: Lease, error = presentationAbortError()): void {
    if (!this.leases.delete(lease)) return;
    clearTimeout(lease.timer);
    lease.signal.removeEventListener('abort', lease.abort);
    const reject = lease.reject;
    lease.resolve = undefined; lease.reject = undefined;
    reject?.(error);
  }
  invalidate(): void { for (const lease of [...this.leases]) this.release(lease); }
  reset(): void { this.invalidate(); this.failure = undefined; }
  fail(error: unknown): void {
    this.failure = error instanceof Error ? error : new Error(String(error));
    for (const lease of [...this.leases]) this.release(lease, this.failure);
  }
  dispose(): void { this.disposed = true; this.invalidate(); }
}
