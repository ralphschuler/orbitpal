interface StoppableStream { getTracks(): { stop(): void }[]; }
export interface MicrophoneLease<T> { stream: T; revision: number; }

/** Own a pending microphone request, including permission results arriving after Stop. */
export class MicrophoneSession<T extends StoppableStream> {
  private revision = 0;
  private pending = false;
  private stream: T | null = null;
  constructor(private pendingChanged: (pending: boolean) => void) {}
  async request(open: () => Promise<T | null>, stillCurrent: () => boolean): Promise<MicrophoneLease<T> | null> {
    if (this.pending || this.stream) return null;
    const revision = ++this.revision;
    this.pending = true; this.pendingChanged(true);
    try {
      const stream = await open();
      if (!stream) return null;
      if (revision !== this.revision || !stillCurrent()) { stream.getTracks().forEach(track => track.stop()); return null; }
      this.stream = stream;
      return { stream, revision };
    } finally {
      if (revision === this.revision) { this.pending = false; this.pendingChanged(false); }
    }
  }
  isCurrent(lease: MicrophoneLease<T>) { return lease.revision === this.revision; }
  release(lease: MicrophoneLease<T>) {
    lease.stream.getTracks().forEach(track => track.stop());
    if (this.stream === lease.stream) this.stream = null;
  }
  cancel() {
    ++this.revision;
    this.stream?.getTracks().forEach(track => track.stop()); this.stream = null;
    this.pending = false; this.pendingChanged(false);
  }
}
