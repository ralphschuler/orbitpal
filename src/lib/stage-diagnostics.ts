/** Optional observations only. Submission means CPU render-call success,
 * never GPU completion, visible pixels or display presentation. */
export type StageObservation =
  | { kind: 'created' | 'disposed' | 'context_lost' | 'failure' }
  | { kind: 'raf'; at: number }
  | { kind: 'submitted'; at: number; cpuMs: number };
export type StageDiagnostics = (observation: StageObservation) => void;

/** A constructor draw cannot qualify an asset loaded afterwards. This gate
 * observes the existing renderer; it never schedules a frame or resets counts. */
export class FreshSubmissionGate {
  private state: 'new' | 'armed' | 'done' | 'failed' = 'new';
  private pending?: { resolve: () => void; reject: (reason: Error) => void; timer: ReturnType<typeof setTimeout> };
  armAfterLoad(): Promise<void> {
    if (this.state !== 'new') return Promise.reject(new Error('Benchmark fresh submission unavailable'));
    this.state = 'armed';
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => this.cancel(), 5000);
      this.pending = { resolve, reject, timer };
    });
  }
  observe(observation: StageObservation): void {
    if (observation.kind === 'failure' || observation.kind === 'context_lost' || observation.kind === 'disposed') { this.cancel(); return; }
    if (observation.kind !== 'submitted' || this.state !== 'armed' || !this.pending) return;
    const pending = this.pending;
    this.pending = undefined; this.state = 'done'; clearTimeout(pending.timer); pending.resolve();
  }
  cancel(): void {
    if (this.state === 'done' || this.state === 'failed') return;
    this.state = 'failed'; const pending = this.pending; this.pending = undefined;
    if (pending) { clearTimeout(pending.timer); pending.reject(new Error('Benchmark fresh submission unavailable')); }
  }
}

export const DIAGNOSTIC_COUNT_LIMIT = 10_000_000;
export const DIAGNOSTIC_DURATION_LIMIT = 3_600_000;
export const DIAGNOSTIC_BINS = 257;
const channels = ['raf', 'submission', 'cpu'] as const;
type Channel = typeof channels[number];
export type StageCounters = { created: number; disposed: number; raf: number; submitted: number; contextLost: number; failures: number };
export type StageDiagnosticsSnapshot = {
  telemetryInvalid: boolean;
  counters: StageCounters;
  histograms: Record<Channel, number[]>;
  max: Record<Channel, number>;
};
const duration = (value: number) => Number.isFinite(value) && value >= 0 && value <= DIAGNOSTIC_DURATION_LIMIT;
const timestamp = (value: number) => Number.isFinite(value) && value >= 0 && value <= Number.MAX_SAFE_INTEGER;

/** Fixed-size cumulative storage, allocated only by the isolated benchmark. */
export class StageDiagnosticCounters {
  private invalid = false;
  private counts: StageCounters = { created: 0, disposed: 0, raf: 0, submitted: 0, contextLost: 0, failures: 0 };
  private bins: Record<Channel, Uint32Array> = { raf: new Uint32Array(DIAGNOSTIC_BINS), submission: new Uint32Array(DIAGNOSTIC_BINS), cpu: new Uint32Array(DIAGNOSTIC_BINS) };
  private maxima: Record<Channel, number> = { raf: 0, submission: 0, cpu: 0 };
  private previousRaf?: number;
  private previousSubmission?: number;
  constructor(private maximumCount = DIAGNOSTIC_COUNT_LIMIT) {
    if (!Number.isSafeInteger(maximumCount) || maximumCount < 1 || maximumCount > DIAGNOSTIC_COUNT_LIMIT) throw new RangeError('Invalid diagnostic count limit');
  }

  invalidate(): void { this.invalid = true; }
  private count(name: keyof StageCounters): void {
    if (this.counts[name] >= this.maximumCount) this.invalid = true;
    else this.counts[name]++;
  }
  private sample(channel: Channel, value: number): void {
    if (!duration(value)) { this.invalid = true; return; }
    const index = Math.min(256, Math.floor(value));
    if (this.bins[channel][index] >= this.maximumCount) { this.invalid = true; return; }
    this.bins[channel][index]++;
    this.maxima[channel] = Math.max(this.maxima[channel], value);
  }
  readonly observe: StageDiagnostics = observation => {
    if (observation.kind === 'failure') { this.count('failures'); this.invalid = true; return; }
    if (observation.kind === 'context_lost') { this.count('contextLost'); this.invalid = true; return; }
    if (observation.kind === 'created') {
      this.count('created');
      if (this.counts.created !== 1 || this.counts.disposed !== 0) this.invalid = true;
      return;
    }
    if (observation.kind === 'disposed') {
      this.count('disposed');
      if (this.counts.created !== 1 || this.counts.disposed !== 1) this.invalid = true;
      return;
    }
    if (this.counts.created !== 1 || this.counts.disposed !== 0) this.invalid = true;
    if (observation.kind === 'raf') {
      this.count('raf');
      if (!timestamp(observation.at)) { this.invalid = true; return; }
      if (this.previousRaf !== undefined) this.sample('raf', observation.at - this.previousRaf);
      if (this.previousRaf === undefined || observation.at >= this.previousRaf) this.previousRaf = observation.at;
      return;
    }
    if (observation.kind !== 'submitted') { this.invalid = true; return; }
    this.count('submitted');
    this.sample('cpu', observation.cpuMs);
    if (!timestamp(observation.at)) { this.invalid = true; return; }
    if (this.previousSubmission !== undefined) this.sample('submission', observation.at - this.previousSubmission);
    if (this.previousSubmission === undefined || observation.at >= this.previousSubmission) this.previousSubmission = observation.at;
  };
  snapshot(): StageDiagnosticsSnapshot {
    return { telemetryInvalid: this.invalid, counters: { ...this.counts },
      histograms: { raf: Array.from(this.bins.raf), submission: Array.from(this.bins.submission), cpu: Array.from(this.bins.cpu) },
      max: { ...this.maxima } };
  }
}

export type BenchmarkPage = { kind: 'control' } | { kind: 'figure'; epoch: number; fps: 20 | 30 };
export function parseBenchmarkPage(search: string): BenchmarkPage | null {
  const entries = [...new URLSearchParams(search)];
  if (entries.length === 1 && entries[0][0] === 'benchmark' && entries[0][1] === 'control') return { kind: 'control' };
  if ((entries.length !== 2 && entries.length !== 3) || entries.some(([key]) => key !== 'benchmark' && key !== 'epoch' && key !== 'fps')) return null;
  const params = new URLSearchParams(search);
  const raw = params.get('epoch');
  if (params.getAll('benchmark').length !== 1 || params.getAll('epoch').length !== 1 || params.get('benchmark') !== 'figure'
    || params.getAll('fps').length > 1 || (params.has('fps') && params.get('fps') !== '20' && params.get('fps') !== '30')
    || raw === null || !/^[1-9][0-9]{0,15}$/.test(raw)) return null;
  const epoch = Number(raw);
  return Number.isSafeInteger(epoch) && epoch > 0 ? { kind: 'figure', epoch, fps: params.get('fps') === '20' ? 20 : 30 } : null;
}
export type BenchmarkCommand = 'continue' | 'dom_out' | 'dom_in' | 'dispose';
export type BenchmarkPacket = StageDiagnosticsSnapshot & { version: 1; epoch: number; sequence: number; disposed: boolean; domOut: boolean };
export function parseBenchmarkReply(value: unknown, epoch: number): BenchmarkCommand | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const reply = value as Record<string, unknown>;
  if (Object.keys(reply).length !== 3 || !Object.hasOwn(reply, 'accepted') || !Object.hasOwn(reply, 'epoch') || !Object.hasOwn(reply, 'command')
    || reply.accepted !== true || reply.epoch !== epoch || !Number.isSafeInteger(reply.epoch)) return null;
  return reply.command === 'continue' || reply.command === 'dom_out' || reply.command === 'dom_in' || reply.command === 'dispose' ? reply.command : null;
}
type Timer = ReturnType<typeof setTimeout>;
export type BenchmarkTimers = { set: (callback: () => void, delay: number) => Timer; clear: (timer: Timer) => void };
const browserTimers: BenchmarkTimers = { set: (callback, delay) => setTimeout(callback, delay), clear: timer => clearTimeout(timer) };

/** At most one IPC promise. Timeout stops collection; its late result is inert. */
export class BenchmarkBatcher {
  private timer?: Timer;
  private timeout?: Timer;
  private sequence = 0;
  private inflight = false;
  private stopped = false;
  private started = false;
  private final = false;
  constructor(private epoch: number, private read: () => Omit<BenchmarkPacket, 'version' | 'epoch' | 'sequence'>,
    private send: (packet: BenchmarkPacket) => Promise<unknown>, private command: (command: Exclude<BenchmarkCommand, 'continue'>) => void,
    private invalid: () => void, private timers: BenchmarkTimers = browserTimers) {}
  start(): void {
    if (this.started || this.stopped) return;
    this.started = true;
    if (!Number.isSafeInteger(this.epoch) || this.epoch <= 0) { this.fail(); return; }
    this.schedule();
  }
  stop(): void {
    this.stopped = true;
    if (this.timer !== undefined) this.timers.clear(this.timer);
    if (this.timeout !== undefined) this.timers.clear(this.timeout);
    this.timer = this.timeout = undefined;
  }
  private fail(): void { this.stop(); this.invalid(); }
  private schedule(): void {
    if (!this.stopped) this.timer = this.timers.set(() => { this.timer = undefined; this.batch(); }, 1000);
  }
  private batch(): void {
    if (this.stopped || this.inflight || this.sequence >= DIAGNOSTIC_COUNT_LIMIT) { if (!this.stopped && !this.inflight) this.fail(); return; }
    let packet: BenchmarkPacket;
    try { packet = { ...this.read(), version: 1, epoch: this.epoch, sequence: ++this.sequence }; }
    catch { this.fail(); return; }
    if (packet.disposed !== this.final) { this.fail(); return; }
    this.inflight = true;
    this.timeout = this.timers.set(() => this.fail(), 10_000);
    let pending: Promise<unknown>;
    try { pending = this.send(packet); }
    catch { this.fail(); return; }
    void pending.then(value => {
      if (this.stopped) return;
      this.inflight = false;
      if (this.timeout !== undefined) this.timers.clear(this.timeout);
      this.timeout = undefined;
      const command = parseBenchmarkReply(value, this.epoch);
      if (command === null || (this.final && command !== 'continue')) { this.fail(); return; }
      if (this.final) { this.stop(); return; }
      try {
        if (command !== 'continue') this.command(command);
      } catch { this.fail(); return; }
      if (command === 'dispose') {
        this.final = true;
        // No periodic timer survives successful cleanup. One final cumulative
        // packet acknowledges it; native closes only its exact figure window.
        this.batch();
      } else this.schedule();
    }, () => { if (!this.stopped) this.fail(); });
  }
}
