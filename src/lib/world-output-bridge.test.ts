import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WorldMotionFollower, type MotionOffer, type WorldMotionCommand } from './world-motion';
import { WorldOutputBridge, type WorldRenderAdapter } from './world-output-bridge';
import type { ActionReport, ActionReportAck, BindingStamp, LocalWorldFrame, MeasurementChallenge,
  NativeMotionBinding as MotionBinding, MotionFence, RegisteredWorldActor, RenderMeasurement, WorldAdapterReason } from './world-output';

// Actual bridge plus actual pure follower, controlled native ACK promises and
// synthetic successful-frame callbacks. No WebGL, native registry, Store or
// actual movement/arrival is established by these orchestration fixtures.
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function drain() { for (let turn = 0; turn < 12; turn++) await Promise.resolve(); }
function binding(): MotionBinding {
  return { enclosureKind: 'conservative_submitted_geometry_v1', actorId: 'nova', sourceWorldId: 'life-nova', renderSessionId: 'render-1', bindingSequence: 1,
    worldInstanceId: 'world-1', stageId: 'shared-stage', configRevision: 1, topologyRevision: 1,
    layoutRevision: 1, assetRevision: 1, measurementRevision: 1, referenceRevision: 1,
    viewport: { width: 1000, height: 800 }, safeCenters: { left: 20, top: 40, right: 980, bottom: 760 },
    maximumFootprint: { width: 40, height: 80 }, envelopeExtents: { left: 20, top: 40, right: 20, bottom: 40 },
    referenceLocal: { x: 0, y: 1, z: 0 }, position: { x: 500, y: 400 },
    groundBasis: { right: { x: .01, z: 0 }, down: { x: 0, z: .02 } }, modelForwardYaw: 0,
    profileScale: 1, effectiveScale: 1, scale: 1, reducedMotion: false, walkSupported: true, stationSupported: false };
}
function stamp(value = binding()): BindingStamp {
  const { actorId, sourceWorldId, renderSessionId, bindingSequence, worldInstanceId, stageId,
    configRevision, topologyRevision, layoutRevision, assetRevision, measurementRevision, referenceRevision } = value;
  return { actorId, sourceWorldId, renderSessionId, bindingSequence, worldInstanceId, stageId,
    configRevision, topologyRevision, layoutRevision, assetRevision, measurementRevision, referenceRevision };
}
function challenge(value = binding()): MeasurementChallenge {
  return { id: 'challenge-1', expiresInMs: 5000, expected: stamp(value), enclosureKind: 'conservative_submitted_geometry_v1', cameraPolicy: 'world_iso_v1',
    profileScale: 1, effectiveScale: 1, expectedReferenceLocal: value.referenceLocal };
}
function output(value = binding(), readiness: RegisteredWorldActor['readiness'] = { state: 'ready', binding: value }): RegisteredWorldActor {
  return { version: 1, renderSessionId: value.renderSessionId, bindingSequence: value.bindingSequence,
    surfaceId: 'surface-1', role: 'owner_candidate', readiness };
}
function offer(value = binding(), ordinal = 1, sequence = ordinal): MotionOffer {
  const { sourceWorldId: _sourceWorldId, ...fields } = stamp(value);
  const fence: MotionFence = { ...fields, actionId: { actorId: value.actorId, worldId: value.sourceWorldId, ordinal },
    deliveryId: `delivery-${ordinal}` };
  return { version: 1, sequence, fence, type: 'offer', activity: 'move_to', minimumActiveMs: 0, from: value.position,
    target: { x: 600, y: 400 }, maximumSpeed: 100, acceleration: 500, validForMs: 10_000 };
}
function frame(move: MotionOffer, frameSequence: number, purpose: LocalWorldFrame['purpose'], atTarget = false): LocalWorldFrame {
  const anchor = atTarget ? { ...move.target } : { ...move.from };
  return { fence: move.fence, frameSequence, purpose, enclosureKind: 'conservative_submitted_geometry_v1', viewport: { width: 1000, height: 800 },
    clip: { left: 0, top: 0, right: 1000, bottom: 800 }, anchor,
    posedBounds: { left: anchor.x - 20, top: anchor.y - 40, right: anchor.x + 20, bottom: anchor.y + 40 },
    walking: false, targetIdleFrames: 0 };
}
function ack(report: ActionReport, overrides: Partial<ActionReportAck> = {}): ActionReportAck {
  const runtimeStatus = report.kind === 'delivered' ? 'delivered' : report.kind === 'completed' ? 'completed' : 'started';
  return { version: 1, receiptSequence: report.receiptSequence, outcome: 'accepted', runtimeStatus,
    confirmedPhase: report.kind === 'delivered' ? 'planned' : report.kind === 'completed' ? 'completed' : 'started',
    reason: null, ...overrides };
}
function harness() {
  const value = binding(), clock = { now: 0 }, follower = new WorldMotionFollower();
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  const state = {
    current: true,
    register: (): Promise<unknown> => Promise.resolve(output(value)),
    action: (report: ActionReport): Promise<unknown> => Promise.resolve(ack(report)),
    measurement: (report: RenderMeasurement): Promise<unknown> => Promise.resolve({ version: 1, frameSequence: report.frameSequence, output: output(value) }),
  };
  const changed: RegisteredWorldActor[] = [], failed: WorldAdapterReason[] = [];
  const adapter: WorldRenderAdapter = {
    acceptWorldCommand: vi.fn((command: WorldMotionCommand) => follower.receive(command, clock.now)),
    beginWorldAfterDelivered: vi.fn((fence: MotionFence, sequence: number) => follower.beginAfterDelivered(fence, sequence, clock.now)),
    confirmWorldStarted: vi.fn(() => true),
    cancelWorld: vi.fn(() => follower.invalidate()),
  };
  const bridge = new WorldOutputBridge({
    request: { version: 1, mountId: 'mount-1', actorId: value.actorId,
      worldInstanceId: value.worldInstanceId, stageId: value.stageId, configRevision: value.configRevision },
    register: () => state.register(), current: () => state.current, adapter, now: () => clock.now,
    invoke: (command, args = {}) => {
      calls.push({ command, args: structuredClone(args) });
      if (command === 'report_world_action') return state.action(args.report as ActionReport);
      if (command === 'report_world_measurement') return state.measurement(args.report as RenderMeasurement);
      if (command === 'unregister_world_output') return Promise.resolve({ version: 1, removed: true });
      return Promise.reject(new Error('Unexpected fixture command'));
    },
    changed: (next, nextBinding) => { changed.push(next); if (nextBinding) {
      const { enclosureKind: _kind, ...motion } = nextBinding;
      follower.bind(motion, clock.now);
    } },
    failed: reason => failed.push(reason),
  });
  const reports = () => calls.filter(call => call.command === 'report_world_action').map(call => call.args.report as ActionReport);
  const unregisters = () => calls.filter(call => call.command === 'unregister_world_output').map(call => call.args.request);
  return { bridge, value, clock, state, adapter, follower, changed, failed, reports, unregisters, calls };
}
async function started(h: ReturnType<typeof harness>) {
  await h.bridge.refresh();
  const move = offer(h.value); expect(h.bridge.receive(move)).toBe('accepted');
  h.bridge.frame(frame(move, 1, 'delivery')); await drain();
  h.clock.now = 16; h.bridge.frame(frame(move, 2, 'first_behavior')); await drain();
  expect(h.adapter.confirmWorldStarted).toHaveBeenCalledWith(move.fence, 2);
  return move;
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe('registered World bridge ownership and ACK barriers', () => {
  it.each([undefined, 'exact_submitted_geometry_v1', 'unknown_policy'])('cancels a frame with unsupported enclosure kind %s before reporting or releasing delivery', async enclosureKind => {
    const h = harness(); await h.bridge.refresh(); const move = offer();
    expect(h.bridge.receive(move)).toBe('accepted');
    h.bridge.frame({ ...frame(move, 1, 'delivery'), enclosureKind } as unknown as LocalWorldFrame); await drain();
    expect(h.failed).toEqual(['protocol_error']); expect(h.reports()).toEqual([]);
    expect(h.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled(); h.bridge.dispose();
  });

  it('does not send a measurement whose enclosure policy differs from its native challenge', async () => {
    const h = harness(), pending = challenge();
    h.state.register = async () => output(h.value, { state: 'awaiting_measurement', challenge: pending });
    await h.bridge.refresh();
    const report: RenderMeasurement = { version: 1, challengeId: pending.id, frameSequence: 1,
      enclosureKind: 'conservative_submitted_geometry_v1', viewport: h.value.viewport,
      clip: { left: 0, top: 0, right: 1000, bottom: 800 }, anchor: h.value.position,
      posedBounds: { left: 480, top: 360, right: 520, bottom: 440 }, envelopeExtents: h.value.envelopeExtents,
      referenceLocal: h.value.referenceLocal, groundBasis: h.value.groundBasis, modelForwardYaw: 0,
      appliedModelScale: 1, reducedMotion: false, walkSupported: true, stationSupported: false };
    h.bridge.measurement({ ...report, enclosureKind: 'exact_submitted_geometry_v1' } as unknown as RenderMeasurement);
    await drain(); expect(h.failed).toEqual(['protocol_error']);
    expect(h.calls.some(call => call.command === 'report_world_measurement')).toBe(false); h.bridge.dispose();
  });

  it('registration establishes capability only; mirrors cannot consume an Offer or report an action', async () => {
    const h = harness(); await h.bridge.refresh();
    expect(h.changed).toHaveLength(1); expect(h.reports()).toEqual([]);
    expect(h.adapter.acceptWorldCommand).not.toHaveBeenCalled();
    const mirror = harness(); mirror.state.register = async () => ({ ...output(), role: 'mirror' });
    await mirror.bridge.refresh();
    expect(mirror.bridge.receive(offer())).toBe('rejected');
    mirror.bridge.frame(frame(offer(), 1, 'delivery'));
    expect(mirror.adapter.acceptWorldCommand).not.toHaveBeenCalled(); expect(mirror.reports()).toEqual([]);
    h.bridge.dispose(); mirror.bridge.dispose();
  });

  it('releases behavior only after exact Delivered and durable Start ACKs', async () => {
    const h = harness(), delivered = deferred<unknown>(), durableStart = deferred<unknown>();
    h.state.action = report => report.kind === 'delivered' ? delivered.promise : durableStart.promise;
    await h.bridge.refresh(); const move = offer(); h.bridge.receive(move);
    h.bridge.frame(frame(move, 1, 'delivery'));
    expect(h.reports()[0].kind).toBe('delivered'); expect(h.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled();
    delivered.resolve(ack(h.reports()[0])); await drain();
    expect(h.adapter.beginWorldAfterDelivered).toHaveBeenCalledWith(move.fence, move.sequence);
    h.clock.now = 16; h.bridge.frame(frame(move, 2, 'first_behavior'));
    expect(h.adapter.confirmWorldStarted).not.toHaveBeenCalled();
    durableStart.resolve(ack(h.reports()[1])); await drain();
    expect(h.adapter.confirmWorldStarted).toHaveBeenCalledWith(move.fence, 2);
    expect(h.failed).toEqual([]); h.bridge.dispose();
  });

  it('does not promote a behavior frame observed before Delivered ACK into a later Started receipt', async () => {
    const h = harness(), held = deferred<unknown>(); h.state.action = () => held.promise;
    await h.bridge.refresh(); const move = offer(); h.bridge.receive(move);
    h.bridge.frame(frame(move, 1, 'delivery'));
    h.clock.now = 10; h.bridge.frame(frame(move, 2, 'first_behavior'));
    held.resolve(ack(h.reports()[0])); await drain();
    expect(h.reports().map(report => report.kind)).toEqual(['delivered']);
    h.state.action = report => Promise.resolve(ack(report));
    h.clock.now = 20; h.bridge.frame(frame(move, 3, 'first_behavior')); await drain();
    expect(h.reports().map(report => report.kind)).toEqual(['delivered', 'started']);
    expect(h.adapter.confirmWorldStarted).toHaveBeenCalledWith(move.fence, 3); h.bridge.dispose();
  });

  it('does not promote pre-Start-ACK progress; a genuine fresh stationary hold may follow that ACK', async () => {
    const h = harness(), held = deferred<unknown>();
    await h.bridge.refresh(); const move = offer(); h.bridge.receive(move);
    h.bridge.frame(frame(move, 1, 'delivery')); await drain();
    h.state.action = () => held.promise;
    h.clock.now = 16; h.bridge.frame(frame(move, 2, 'first_behavior'));
    h.clock.now = 100; h.bridge.frame(frame(move, 3, 'progress'));
    held.resolve(ack(h.reports()[1])); await drain();
    expect(h.reports().map(report => report.kind)).toEqual(['delivered', 'started']);
    h.state.action = report => Promise.resolve(ack(report));
    h.clock.now = 150; h.bridge.frame(frame(move, 4, 'start_hold')); await drain();
    expect(h.reports().at(-1)?.frame.frameSequence).toBe(4); expect(h.reports().at(-1)?.kind).toBe('progress');
    h.bridge.dispose();
  });

  it('rejects transport acceptance without a real committed Start and cleans its exact attachment', async () => {
    const h = harness();
    h.state.action = report => Promise.resolve(ack(report, report.kind === 'started' ? { confirmedPhase: 'planned' } : {}));
    await h.bridge.refresh(); const move = offer(); h.bridge.receive(move);
    h.bridge.frame(frame(move, 1, 'delivery')); await drain();
    h.clock.now = 16; h.bridge.frame(frame(move, 2, 'first_behavior')); await drain();
    expect(h.adapter.confirmWorldStarted).not.toHaveBeenCalled(); expect(h.failed).toEqual(['protocol_error']);
    expect(h.unregisters()).toEqual([{ version: 1, renderSessionId: 'render-1', bindingSequence: 1 }]);
  });

  it('cannot use another pending measurement identity to replace the challenged binding', async () => {
    const h = harness(), expected = challenge();
    h.state.register = async () => output(h.value, { state: 'awaiting_measurement', challenge: expected });
    h.state.measurement = async report => ({ version: 1, frameSequence: report.frameSequence,
      output: output({ ...binding(), renderSessionId: 'foreign-render', bindingSequence: 2 }) });
    await h.bridge.refresh();
    h.bridge.measurement({ version: 1, challengeId: expected.id, frameSequence: 1, enclosureKind: 'conservative_submitted_geometry_v1',
      viewport: h.value.viewport, clip: { left: 0, top: 0, right: 1000, bottom: 800 }, anchor: h.value.position,
      posedBounds: { left: 480, top: 360, right: 520, bottom: 440 }, envelopeExtents: h.value.envelopeExtents,
      referenceLocal: h.value.referenceLocal, groundBasis: h.value.groundBasis, modelForwardYaw: 0,
      appliedModelScale: 1, reducedMotion: false, walkSupported: true, stationSupported: false });
    await drain();
    expect(h.changed.some(next => next.readiness.state === 'ready')).toBe(false);
    expect(h.failed).toEqual(['measurement_unavailable']);
  });
});

describe('bounded fresh frame queue and native terminal authority', () => {
  it('submits only the newest copied pending frame after native waiting, preserving its observed values', async () => {
    const h = harness(), move = await started(h), held = deferred<unknown>(); h.state.action = () => held.promise;
    h.clock.now = 100; h.bridge.frame(frame(move, 3, 'progress'));
    h.clock.now = 120; h.bridge.frame(frame(move, 4, 'progress'));
    const newest = frame(move, 5, 'progress'); h.clock.now = 140; h.bridge.frame(newest);
    newest.anchor.x = 999; newest.posedBounds.left = 999;
    h.state.action = report => Promise.resolve(ack(report));
    h.clock.now = 200; held.resolve(ack(h.reports()[2])); await drain();
    expect(h.reports().map(report => report.frame.frameSequence)).toEqual([1, 2, 3, 5]);
    expect(h.reports().at(-1)!.frame.anchor.x).toBe(500);
    expect(h.reports().at(-1)!.frame.posedBounds.left).toBe(480);
    expect(h.failed).toEqual([]); h.bridge.dispose();
  });

  it('coalesces to the newest immutable observation and never retimes an expired queued frame', async () => {
    const h = harness(), move = await started(h), held = deferred<unknown>();
    h.state.action = () => held.promise;
    h.clock.now = 100; h.bridge.frame(frame(move, 3, 'progress'));
    h.clock.now = 120; h.bridge.frame(frame(move, 4, 'progress'));
    const newest = frame(move, 5, 'progress'); h.clock.now = 140; h.bridge.frame(newest);
    newest.anchor.x = 999; newest.viewport.width = 999;
    h.clock.now = 500; held.resolve(ack(h.reports().at(-1)!)); await drain();
    expect(h.reports().map(report => report.frame.frameSequence)).toEqual([1, 2, 3]);
    h.state.action = report => Promise.resolve(ack(report));
    h.clock.now = 520; h.bridge.frame(frame(move, 6, 'progress')); await drain();
    expect(h.reports().map(report => report.frame.frameSequence)).toEqual([1, 2, 3, 6]);
    expect(Object.keys(h.reports().at(-1)!.frame).sort()).toEqual(['anchor', 'clip', 'enclosureKind', 'frameSequence', 'posedBounds', 'targetIdleFrames', 'viewport', 'walking']);
    h.bridge.dispose();
  });

  it('drops a reordered frame behind an in-flight newer frame instead of replaying it after ACK', async () => {
    const h = harness(), move = await started(h), held = deferred<unknown>(); h.state.action = () => held.promise;
    h.clock.now = 100; h.bridge.frame(frame(move, 4, 'progress'));
    h.clock.now = 120; h.bridge.frame(frame(move, 3, 'progress'));
    h.clock.now = 180; held.resolve(ack(h.reports().at(-1)!)); await drain();
    expect(h.reports().map(report => report.frame.frameSequence)).toEqual([1, 2, 4]);
    h.state.action = report => Promise.resolve(ack(report));
    h.clock.now = 240; h.bridge.frame(frame(move, 5, 'progress')); await drain();
    expect(h.reports().at(-1)!.frame.frameSequence).toBe(5); expect(h.failed).toEqual([]); h.bridge.dispose();
  });

  it('does not retire a semantic action from a local target counter without native completion confirmation', async () => {
    const h = harness(), move = await started(h), complete = deferred<unknown>();
    h.state.action = report => report.kind === 'completed' ? complete.promise : Promise.resolve(ack(report));
    for (const [index, now] of [100, 160, 220, 280].entries()) {
      h.clock.now = now;
      h.bridge.frame({ ...frame(move, index + 3, 'progress', true), targetIdleFrames: 60_000 }); await drain();
    }
    expect(h.reports().map(report => report.kind)).toEqual(['delivered', 'started', 'progress', 'progress', 'progress', 'completed']);
    expect(h.adapter.acceptWorldCommand).toHaveBeenCalledTimes(1); // Only the real Offer, no synthesized Terminal.
    expect(h.failed).toEqual([]);
    complete.resolve(ack(h.reports().at(-1)!, { confirmedPhase: 'started', runtimeStatus: 'started' })); await drain();
    expect(h.failed).toEqual(['protocol_error']); // An uncommitted arrival cannot become local completion.
  });

  it('preserves a newer Offer when an older action terminal is accepted by the actual follower', async () => {
    const h = harness(); await h.bridge.refresh(); const old = offer(), newer = offer(binding(), 2, 2);
    expect(h.bridge.receive(old)).toBe('accepted'); expect(h.bridge.receive(newer)).toBe('accepted');
    const terminal: WorldMotionCommand = { version: 1, type: 'terminal', sequence: 3, fence: old.fence, status: 'cancelled' };
    expect(h.bridge.receive(terminal)).toBe('accepted');
    h.bridge.frame(frame(newer, 1, 'delivery')); await drain();
    expect(h.reports()).toHaveLength(1); expect(h.reports()[0].fence.actionId.ordinal).toBe(2);
    expect(h.adapter.beginWorldAfterDelivered).toHaveBeenCalledWith(newer.fence, 2); h.bridge.dispose();
  });

  it('ignores an old unresolved delivery ACK while allowing the new action to acquire its own barrier', async () => {
    const h = harness(), oldReply = deferred<unknown>(); await h.bridge.refresh();
    h.state.action = report => report.fence.actionId.ordinal === 1 ? oldReply.promise : Promise.resolve(ack(report));
    const old = offer(), newer = offer(binding(), 2, 2); h.bridge.receive(old); h.bridge.frame(frame(old, 1, 'delivery'));
    h.bridge.receive(newer); h.clock.now = 10; h.bridge.frame(frame(newer, 1, 'delivery'));
    oldReply.resolve(ack(h.reports()[0])); await drain();
    expect(h.adapter.beginWorldAfterDelivered).toHaveBeenCalledTimes(1);
    expect(h.adapter.beginWorldAfterDelivered).toHaveBeenCalledWith(newer.fence, 2);
    expect(h.failed).toEqual([]); h.bridge.dispose();
  });
});

describe('registration lifecycle epochs, Stop and renewal', () => {
  it('unregisters exactly a late registration after disposal without publishing or beginning it', async () => {
    const h = harness(), late = deferred<unknown>(); h.state.register = () => late.promise;
    const refresh = h.bridge.refresh(); await drain(); h.bridge.dispose();
    late.resolve(output({ ...binding(), renderSessionId: 'late-render', bindingSequence: 7 })); await refresh; await drain();
    expect(h.unregisters()).toEqual([{ version: 1, renderSessionId: 'late-render', bindingSequence: 7 }]);
    expect(h.changed).toEqual([]); expect(h.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled();
    expect(h.bridge.receive(offer())).toBe('rejected');
  });

  it('does not adopt a registration that arrives after its timeout, and cleans only that returned identity', async () => {
    const h = harness(), late = deferred<unknown>(); h.state.register = () => late.promise;
    const refresh = h.bridge.refresh(); await drain();
    await vi.advanceTimersByTimeAsync(2001); await refresh;
    expect(h.failed).toEqual(['unregistered']); expect(h.changed).toEqual([]);
    late.resolve(output({ ...binding(), renderSessionId: 'expired-render', bindingSequence: 9 })); await drain();
    expect(h.unregisters()).toEqual([{ version: 1, renderSessionId: 'expired-render', bindingSequence: 9 }]);
    expect(h.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled();
  });

  it('cannot release a delivery or Start barrier after explicit Stop/dispose', async () => {
    const h = harness(), held = deferred<unknown>(); h.state.action = () => held.promise;
    await h.bridge.refresh(); const move = offer(); h.bridge.receive(move); h.bridge.frame(frame(move, 1, 'delivery'));
    h.bridge.dispose(); held.resolve(ack(h.reports()[0])); await drain();
    expect(h.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled();
    expect(h.adapter.confirmWorldStarted).not.toHaveBeenCalled(); expect(h.reports()).toHaveLength(1);
    expect(h.unregisters()).toEqual([{ version: 1, renderSessionId: 'render-1', bindingSequence: 1 }]);
  });

  it('retains work across a same-binding renewal that raced a new Offer', async () => {
    const h = harness(); await h.bridge.refresh(); const renewal = deferred<unknown>(); h.state.register = () => renewal.promise;
    const refresh = h.bridge.refresh(); await drain(); const move = offer(); expect(h.bridge.receive(move)).toBe('accepted');
    renewal.resolve(output(h.value, { state: 'awaiting_measurement', challenge: challenge(h.value) })); await refresh; await drain();
    expect(h.adapter.cancelWorld).not.toHaveBeenCalled();
    h.clock.now = 16; h.bridge.frame(frame(move, 1, 'delivery')); await drain();
    expect(h.adapter.beginWorldAfterDelivered).toHaveBeenCalledWith(move.fence, move.sequence);
    expect(h.failed).toEqual([]); h.bridge.dispose();
  });

  it('does not retain action ownership if a same-binding renewal demotes its route to mirror', async () => {
    const h = harness(); await h.bridge.refresh(); const renewal = deferred<unknown>(); h.state.register = () => renewal.promise;
    const refresh = h.bridge.refresh(); await drain(); const move = offer(); h.bridge.receive(move);
    renewal.resolve({ ...output(), role: 'mirror' }); await refresh; await drain();
    h.bridge.frame(frame(move, 1, 'delivery')); await drain();
    expect(h.reports()).toEqual([]); expect(h.adapter.beginWorldAfterDelivered).not.toHaveBeenCalled();
    expect(h.adapter.cancelWorld).toHaveBeenCalled(); h.bridge.dispose();
  });
});


describe('station duration uses accepted target frames, never a completion timer', () => {
  async function stationStarted(h:ReturnType<typeof harness>) {
    h.value.stationSupported=true; await h.bridge.refresh();
    const command:MotionOffer={...offer(h.value),activity:'play',minimumActiveMs:1000,target:h.value.position};
    expect(h.bridge.receive(command)).toBe('accepted');
    h.bridge.frame(frame(command,1,'delivery'));await drain();
    h.clock.now=16;h.bridge.frame(frame(command,2,'first_behavior'));await drain();return command;
  }
  it('waits for the full sustained target span and no elapsed timer can add a completion',async()=>{
    const h=harness(),command=await stationStarted(h);let sequence=2;
    for(const now of [100,300,500,700,900]){h.clock.now=now;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();}
    expect(h.reports().some(report=>report.kind==='completed')).toBe(false);
    const count=h.reports().length;vi.advanceTimersByTime(2000);await drain();expect(h.reports()).toHaveLength(count);
    h.clock.now=1100;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();
    expect(h.reports().at(-1)?.kind).toBe('completed');h.bridge.dispose();
  });
  it('does not count paused frames or carry the pre-pause span across resume',async()=>{
    const h=harness(),command=await stationStarted(h);let sequence=2;
    for(const now of [100,300]){h.clock.now=now;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();}
    h.clock.now=350;expect(h.bridge.receive({version:1,type:'pause',sequence:2,fence:command.fence})).toBe('accepted');
    const count=h.reports().length;h.clock.now=600;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();expect(h.reports()).toHaveLength(count);
    h.clock.now=1000;expect(h.bridge.receive({version:1,type:'resume',sequence:3,fence:command.fence})).toBe('accepted');
    for(const now of [1000,1200,1400,1600,1800]){h.clock.now=now;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();}
    expect(h.reports().some(report=>report.kind==='completed')).toBe(false);
    h.clock.now=2000;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();
    expect(h.reports().at(-1)?.kind).toBe('completed');h.bridge.dispose();
  });
  it('a pre-pause in-flight progress ACK cannot restore the old dwell after resume',async()=>{
    const h=harness(),command=await stationStarted(h),pending=deferred<ActionReportAck>();
    h.state.action=()=>pending.promise;h.clock.now=100;h.bridge.frame(frame(command,3,'progress',true));const old=h.reports().at(-1)!;
    h.clock.now=110;h.bridge.receive({version:1,type:'pause',sequence:2,fence:command.fence});
    h.clock.now=120;h.bridge.receive({version:1,type:'resume',sequence:3,fence:command.fence});
    h.state.action=async report=>ack(report);pending.resolve(ack(old));await drain();
    let sequence=3;for(const now of [300,500,700,900,1100]){h.clock.now=now;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();}
    expect(h.reports().some(report=>report.kind==='completed')).toBe(false);
    h.clock.now=1300;h.bridge.frame(frame(command,++sequence,'progress',true));await drain();
    expect(h.reports().at(-1)?.kind).toBe('completed');h.bridge.dispose();
  });
});
