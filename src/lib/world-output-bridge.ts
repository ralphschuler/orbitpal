// SPDX-License-Identifier: MIT
import { parseWorldMotionCommand, type ReceiveResult, type WorldMotionCommand } from './world-motion';
import { parseActionReportAck, parseMeasurementAck, parseRegisteredWorldActor,
  type ActionReport, type BindingStamp, type FrameObservation, type LocalWorldFrame,
  type MeasurementChallenge, type NativeMotionBinding as MotionBinding, type MotionFence, type RegisteredWorldActor,
  type RegisterWorldActor, type RenderMeasurement, type WorldAdapterReason } from './world-output';

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
export interface WorldRenderAdapter {
  acceptWorldCommand(command: WorldMotionCommand): ReceiveResult;
  beginWorldAfterDelivered(fence: MotionFence, offerSequence: number): boolean;
  confirmWorldStarted(fence: MotionFence, firstBehaviorFrameSequence: number): boolean;
  cancelWorld(reason: WorldAdapterReason): void;
}
export interface WorldBridgeOptions {
  request: Omit<RegisterWorldActor, 'studioProof'>;
  /** Uses the existing Studio LifeUI reporter's immediately dispatched proof.
   * This callback must reject while a real modal/input/dirty blocker exists. */
  register(request: Omit<RegisterWorldActor, 'studioProof'>): Promise<unknown>;
  invoke: Invoke;
  adapter: WorldRenderAdapter;
  current(): boolean;
  changed(output: RegisteredWorldActor, binding: MotionBinding | null, challenge: MeasurementChallenge | null): void;
  failed(reason: WorldAdapterReason): void;
  now?: () => number;
  timeoutMs?: number;
}
type Work = { command: Extract<WorldMotionCommand, { type: 'offer' }>; epoch: number;
  delivered: boolean; started: boolean; receipt: number; lastFrame: number;
  paused: boolean; control: number;
  observedFrame: number;
  petElapsed: number;
  stable: { first: number; last: number; count: number } | null };
const stampKeys = ['actorId', 'renderSessionId', 'bindingSequence',
  'worldInstanceId', 'stageId', 'configRevision', 'topologyRevision', 'layoutRevision',
  'assetRevision', 'measurementRevision', 'referenceRevision'] as const;
function sameStamp(a: BindingStamp | MotionFence, b: BindingStamp | MotionFence): boolean {
  return stampKeys.every(key => a[key] === b[key]);
}
function sameFence(a: MotionFence, b: MotionFence): boolean {
  return sameStamp(a, b) && a.deliveryId === b.deliveryId && a.actionId.actorId === b.actionId.actorId
    && a.actionId.worldId === b.actionId.worldId && a.actionId.ordinal === b.actionId.ordinal;
}
const expired = () => new Error('Die sichtbare Figurenausgabe ist nicht mehr aktuell.');
function timed<T>(promise: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  return Promise.race([promise, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(expired()), ms); })])
    .finally(() => clearTimeout(timer));
}
function copyFrame(frame: LocalWorldFrame): FrameObservation {
  // Copies observations now. A queued render can never mutate an older receipt.
  return Object.freeze({ frameSequence: frame.frameSequence, enclosureKind: frame.enclosureKind, viewport: Object.freeze({ ...frame.viewport }),
    clip: Object.freeze({ ...frame.clip }), anchor: Object.freeze({ ...frame.anchor }),
    posedBounds: Object.freeze({ ...frame.posedBounds }), walking: frame.walking,
    targetIdleFrames: frame.targetIdleFrames, ...(frame.pet ? {pet:Object.freeze({...frame.pet})} : {}) });
}
type PendingFrame = { value: LocalWorldFrame; observed: number };

/** One mounted actor, one asynchronous native stream. The synchronous renderer
 * supplies actual successful frames; neither callback nor a mathematical target
 * can write Life/World state. Native ACKs alone release each behavior barrier.
 * Stop/unmount revokes outside this queue, including during an unresolved IPC. */
export class WorldOutputBridge {
  private output: RegisteredWorldActor | null = null;
  private binding: MotionBinding | null = null;
  private challenge: MeasurementChallenge | null = null;
  private work: Work | null = null;
  private epoch = 0;
  private disposed = false;
  private refreshing = false;
  private measuring = false;
  private reporting = false;
  private pending: PendingFrame | null = null;
  private lastSubmitted = -Infinity;
  private now: () => number;
  private request: Omit<RegisterWorldActor, 'studioProof'>;
  constructor(private options: WorldBridgeOptions) {
    this.now = options.now ?? (() => performance.now());
    this.request = Object.freeze({ ...options.request });
  }
  private live(epoch = this.epoch): boolean { return !this.disposed && epoch === this.epoch && this.options.current(); }
  private owns(output: RegisteredWorldActor): boolean {
    const value = output.readiness.state === 'ready' ? output.readiness.binding
      : output.readiness.state === 'awaiting_measurement' ? output.readiness.challenge.expected : null;
    return !value || value.actorId === this.request.actorId && value.worldInstanceId === this.request.worldInstanceId
      && value.stageId === this.request.stageId && value.configRevision === this.request.configRevision;
  }
  private unregister(output: RegisteredWorldActor): void {
    try {
      void this.options.invoke('unregister_world_output', { request: {
        version: 1, renderSessionId: output.renderSessionId, bindingSequence: output.bindingSequence,
      } }).catch(() => {});
    } catch { /* Local Stop remains synchronous even if the transport is gone. */ }
  }
  private accept(output: RegisteredWorldActor): void {
    if (!this.owns(output)) throw expired();
    if (this.output?.role === 'owner_candidate' && output.role === 'mirror') {
      this.fail('scope_mismatch'); return;
    }
    if (this.output && (output.renderSessionId !== this.output.renderSessionId
      || output.bindingSequence !== this.output.bindingSequence)) {
      ++this.epoch; this.work = null; this.pending = null;
      this.options.adapter.cancelWorld('stale_binding'); this.binding = null;
    }
    this.output = output;
    this.challenge = output.readiness.state === 'awaiting_measurement' ? output.readiness.challenge : null;
    if (output.readiness.state === 'ready') this.binding = output.readiness.binding;
    else if (!this.challenge || !this.binding || !sameStamp(this.binding, this.challenge.expected)) this.binding = null;
    if (output.readiness.state === 'unavailable') throw expired();
    this.options.changed(output, this.binding, this.challenge);
  }
  /** Idle renewal reuses the mount and retains its unchanged binding. It never
   * refreshes native measurement age; only a subsequent new rendered frame can. */
  async refresh(): Promise<void> {
    if (!this.live() || this.refreshing || this.measuring || this.reporting || this.work) return;
    const epoch = this.epoch; this.refreshing = true;
    const pending = Promise.resolve().then(() => {
      if (!this.live(epoch)) throw expired();
      return this.options.register(this.request);
    });
    // Dispose exact late identities even after timeout/Stop. Never unregister a
    // replacement mount merely because this older registration returned late.
    void pending.then(raw => { if (!this.live(epoch)) {
      try { this.unregister(parseRegisteredWorldActor(raw)); } catch { /* invalid late data grants nothing */ }
    } }).catch(() => {});
    try {
      const output = parseRegisteredWorldActor(await timed(pending, this.options.timeoutMs ?? 2000));
      if (!this.live(epoch)) return;
      this.accept(output);
    } catch { if (this.live(epoch)) this.fail('unregistered'); }
    finally { this.refreshing = false; }
  }
  measurement(report: RenderMeasurement): void {
    const challenge = this.challenge;
    if (!this.live() || this.measuring || this.reporting || this.work || !challenge || report.challengeId !== challenge.id) return;
    if (report.enclosureKind !== challenge.enclosureKind) { this.fail('protocol_error'); return; }
    const epoch = this.epoch; this.measuring = true;
    // The adapter report is freshly created after the challenge's exposed frame.
    const observation: RenderMeasurement = structuredClone(report);
    void timed(this.options.invoke('report_world_measurement', { report: observation }), this.options.timeoutMs ?? 2000)
      .then(raw => {
        if (!this.live(epoch) || this.challenge?.id !== challenge.id) return;
        const ack = parseMeasurementAck(raw, observation.frameSequence);
        const accepted = ack.output.readiness.state === 'ready' ? ack.output.readiness.binding : null;
        if (!accepted || accepted.enclosureKind !== challenge.enclosureKind || !sameStamp(accepted, challenge.expected)
          || accepted.sourceWorldId !== challenge.expected.sourceWorldId) throw expired();
        this.accept(ack.output);
      }).catch(() => { if (this.live(epoch)) this.fail('measurement_unavailable'); })
      .finally(() => { this.measuring = false; });
  }
  receive(raw: unknown): ReceiveResult {
    const command = parseWorldMotionCommand(raw), binding = this.binding;
    if (!this.live() || !command || !binding || this.output?.role !== 'owner_candidate'
      || !sameStamp(command.fence, binding) || command.fence.actionId.worldId !== binding.sourceWorldId) return 'rejected';
    const result = this.options.adapter.acceptWorldCommand(command);
    if (result !== 'accepted') return result;
    if (command.type === 'offer') {
      this.work = { command, epoch: this.epoch, delivered: false, started: false,
        paused: false, control: 0,
        receipt: 0, lastFrame: 0, observedFrame: 0, petElapsed: 0, stable: null };
      this.pending = null; this.lastSubmitted = -Infinity;
    } else if (command.type === 'terminal') {
      if (this.work && command.fence.actionId.ordinal >= this.work.command.fence.actionId.ordinal) {
        this.work = null; this.pending = null;
      }
    } else if (this.work) {
      this.work.stable = null; this.pending = null;
      this.work.paused = command.type === 'pause'; ++this.work.control;
      if (!Number.isSafeInteger(this.work.control)) this.fail('capacity');
    }
    return result;
  }
  frame(value: LocalWorldFrame): void {
    const work = this.work;
    if (!work || work.paused || !this.live(work.epoch) || !sameFence(value.fence, work.command.fence)
      || !Number.isSafeInteger(value.frameSequence) || value.frameSequence <= work.observedFrame) return;
    if (!this.binding || value.enclosureKind !== this.binding.enclosureKind) { this.fail('protocol_error'); return; }
    const pet=work.command.petClip;
    if(pet ? !value.pet || value.pet.action!==pet.action || !Number.isSafeInteger(value.pet.elapsedMs)
      || value.pet.elapsedMs<work.petElapsed || value.pet.elapsedMs>pet.durationMs || value.pet.finished!==(value.pet.elapsedMs===pet.durationMs)
      || !work.started&&(value.pet.elapsedMs!==0||value.pet.finished) : value.pet!==undefined){this.fail('protocol_error');return;}
    const now = this.now();
    if (!Number.isFinite(now) || now < 0) { this.fail('render_failed'); return; }
    work.observedFrame = value.frameSequence;
    // A frame keeps the phase in which it was actually observed. An ACK cannot
    // retrospectively promote a pre-delivery behavior or pre-start progress.
    if (!work.delivered && value.purpose !== 'delivery'
      || work.delivered && !work.started && !['first_behavior', 'start_hold'].includes(value.purpose)
      || work.started && !['progress', 'start_hold'].includes(value.purpose)
      || value.purpose === 'start_hold' && value.walking) return;
    if(value.pet)work.petElapsed=value.pet.elapsedMs;
    // At most one in-flight and one newest frame. Intermediate observations are
    // discarded, never replayed or given a new timestamp after native waiting.
    this.pending = { value: Object.freeze({ ...copyFrame(value), fence: work.command.fence, purpose: value.purpose }), observed: now };
    this.flush();
  }
  private flush(): void {
    const work = this.work, pending = this.pending;
    if (!work || work.paused || !pending || !this.live(work.epoch) || this.reporting || this.measuring) return;
    const now = this.now(), age = now - pending.observed;
    if (!Number.isFinite(now) || age < 0 || age > 250) { this.pending = null; work.stable = null; return; }
    const value = pending.value;
    if (value.frameSequence <= work.lastFrame) { this.pending = null; return; }
    let kind: ActionReport['kind'];
    if (!work.delivered) { if (value.purpose !== 'delivery') return; kind = 'delivered'; }
    else if (!work.started) { if (value.purpose !== 'first_behavior') return; kind = 'started'; }
    else {
      if (value.purpose !== 'progress' && value.purpose !== 'start_hold') return;
      if (now - this.lastSubmitted < 50) return;
      const target = work.command.target;
      const atTarget = !value.walking && Math.hypot(value.anchor.x - target.x, value.anchor.y - target.y) <= .25;
      // A completion request remains only an observation. Host derives arrival
      // from its own distinct accepted frames, elapsed time and current Store.
      const stable = work.stable;
      kind = atTarget && (!work.command.petClip || value.pet?.finished===true) && stable && stable.count >= 3
        && pending.observed - stable.first >= Math.max(100, work.command.minimumActiveMs)
        && pending.observed - stable.last <= 250 ? 'completed' : 'progress';
      if (!atTarget) work.stable = null;
    }
    this.pending = null; this.reporting = true; this.lastSubmitted = now;
    const sequence = ++work.receipt;
    const control = work.control;
    if (!Number.isSafeInteger(sequence)) { this.reporting = false; this.fail('capacity'); return; }
    const report: ActionReport = { version: 1, fence: work.command.fence,
      receiptSequence: sequence, frame: copyFrame(value), kind };
    void timed(this.options.invoke('report_world_action', { report }), this.options.timeoutMs ?? 2000).then(raw => {
      if (!this.live(work.epoch) || this.work !== work || work.control !== control) return;
      const ack = parseActionReportAck(raw, sequence);
      if (!['accepted', 'duplicate'].includes(ack.outcome) || ['cancelled', 'failed'].includes(ack.runtimeStatus ?? '')) {
        this.fail(ack.reason ?? 'cancelled'); return;
      }
      work.lastFrame = value.frameSequence;
      if (kind === 'delivered') {
        if (ack.runtimeStatus !== 'delivered' || !this.options.adapter.beginWorldAfterDelivered(work.command.fence, work.command.sequence)) { this.fail('protocol_error'); return; }
        work.delivered = true;
      } else if (kind === 'started') {
        if (!['started', 'paused'].includes(ack.runtimeStatus ?? '') || ack.confirmedPhase !== 'started'
          || !this.options.adapter.confirmWorldStarted(work.command.fence, value.frameSequence)) { this.fail('protocol_error'); return; }
        work.started = true;
      } else if (kind === 'completed') {
        if (ack.runtimeStatus !== 'completed' || ack.confirmedPhase !== 'completed') { this.fail('protocol_error'); return; }
        // Terminal native event owns adapter retirement; ACK owns bridge cleanup.
        this.work = null; this.pending = null;
      } else {
        const target = work.command.target, atTarget = !value.walking
          && Math.hypot(value.anchor.x - target.x, value.anchor.y - target.y) <= .25;
        if (atTarget) {
          const previous = work.stable;
          work.stable = previous && pending.observed - previous.last <= 250
            ? { first: previous.first, last: pending.observed, count: previous.count + 1 }
            : { first: pending.observed, last: pending.observed, count: 1 };
        } else work.stable = null;
      }
    }).catch(() => { if (this.live(work.epoch) && this.work === work) this.fail('protocol_error'); })
      .finally(() => { this.reporting = false; this.flush(); });
  }
  fail(reason: WorldAdapterReason): void {
    if (this.disposed) return;
    this.dispose(reason); this.options.failed(reason);
  }
  dispose(reason: WorldAdapterReason = 'cancelled'): void {
    if (this.disposed) return;
    this.disposed = true; ++this.epoch; this.work = null; this.pending = null;
    this.options.adapter.cancelWorld(reason);
    if (this.output) this.unregister(this.output);
    this.output = null; this.binding = null; this.challenge = null;
  }
}
