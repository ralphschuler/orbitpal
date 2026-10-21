import { describe, expect, it } from 'vitest';
import { parseActionReportAck, parseMeasurementAck, parseMeasurementChallenge, parseWorldMotionBinding,
  parseRegisteredWorldActor, parseWorldOutputStatus, type BindingStamp,
  type MeasurementChallenge, type NativeMotionBinding as MotionBinding, type RegisteredWorldActor } from './world-output';

// Public protocol fixtures only. A parsed observation grants no native route,
// Life activation, movement, credential, capture or Store authority.
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
function challenge(): MeasurementChallenge {
  return { id: 'challenge-1', expiresInMs: 5000, expected: stamp(), enclosureKind: 'conservative_submitted_geometry_v1', cameraPolicy: 'world_iso_v1',
    profileScale: 1, effectiveScale: 1, expectedReferenceLocal: { x: 0, y: 1, z: 0 } };
}
function output(readiness: RegisteredWorldActor['readiness'] = { state: 'ready', binding: binding() }): RegisteredWorldActor {
  return { version: 1, renderSessionId: 'render-1', bindingSequence: 1, surfaceId: 'surface-1',
    role: 'owner_candidate', readiness };
}

describe('closed public World output protocol', () => {
  it('requires the host-pinned enclosure kind without defaulting legacy or unknown geometry policies', () => {
    const { enclosureKind: _challengeKind, ...missingChallenge } = challenge();
    const { enclosureKind: _bindingKind, ...missingBinding } = binding();
    for (const value of [missingChallenge, { ...challenge(), enclosureKind: 'exact_submitted_geometry_v1' },
      { ...challenge(), enclosureKind: 'future_policy' }]) expect(() => parseMeasurementChallenge(value)).toThrow();
    for (const value of [missingBinding, { ...binding(), enclosureKind: 'exact_submitted_geometry_v1' },
      { ...binding(), enclosureKind: 'future_policy' }]) {
      expect(parseWorldMotionBinding(value)).toBeNull();
      expect(() => parseRegisteredWorldActor(output({ state: 'ready', binding: value as MotionBinding }))).toThrow();
    }
    const parsed = parseWorldMotionBinding(binding());
    expect(parsed?.enclosureKind).toBe('conservative_submitted_geometry_v1');
    expect(Object.isFrozen(parsed)).toBe(true);
  });

  it('copies and freezes nested ready and challenge data without retaining mutable input', () => {
    const source = output(), parsed = parseRegisteredWorldActor(source);
    if (source.readiness.state !== 'ready' || parsed.readiness.state !== 'ready') throw new Error('fixture');
    source.readiness.binding.referenceLocal.y = 100;
    source.readiness.binding.envelopeExtents.left = 100;
    expect(parsed.readiness.binding.referenceLocal.y).toBe(1);
    expect(parsed.readiness.binding.envelopeExtents.left).toBe(20);
    expect(Object.isFrozen(parsed.readiness.binding.referenceLocal)).toBe(true);
    expect(Object.isFrozen(parsed.readiness.binding.envelopeExtents)).toBe(true);
    const raw = challenge(), frozen = parseMeasurementChallenge(raw);
    raw.expected.referenceRevision = 99; raw.expectedReferenceLocal!.x = 99;
    expect(frozen.expected.referenceRevision).toBe(1);
    expect(frozen.expectedReferenceLocal!.x).toBe(0);
    expect(Object.isFrozen(frozen.expected)).toBe(true);
  });

  it('rejects extra fields, inherited tags, accessors and symbol keys without executing getters', () => {
    let reads = 0;
    const readiness = { get state() { reads++; return 'ready'; }, binding: binding() };
    const nested = { ...challenge(), expectedReferenceLocal: { x: 0, get y() { reads++; return 1; }, z: 0 } };
    for (const value of [
      { ...output(), extra: true }, { ...output(), readiness: { state: 'awaiting_layout', binding: binding() } },
      { ...output(), readiness }, { ...output(), readiness: Object.create({ state: 'awaiting_layout' }) },
      { ...output(), [Symbol('extra')]: true }, Object.assign(Object.create({ inherited: true }), output()),
    ]) expect(() => parseRegisteredWorldActor(value)).toThrow();
    expect(() => parseMeasurementChallenge(nested)).toThrow();
    expect(reads).toBe(0);
  });

  it('binds pending and ready revisions to the exact outer render attachment', () => {
    expect(parseRegisteredWorldActor(output({ state: 'awaiting_layout' })).readiness.state).toBe('awaiting_layout');
    expect(parseRegisteredWorldActor(output({ state: 'awaiting_measurement', challenge: challenge() })).readiness.state).toBe('awaiting_measurement');
    for (const readiness of [
      { state: 'ready', binding: { ...binding(), renderSessionId: 'old-render' } },
      { state: 'ready', binding: { ...binding(), bindingSequence: 2 } },
      { state: 'awaiting_measurement', challenge: { ...challenge(), expected: { ...stamp(), bindingSequence: 2 } } },
    ]) expect(() => parseRegisteredWorldActor({ ...output(), readiness })).toThrow();
  });

  it('does not confuse render capability with permission to admit an action', () => {
    const ready = output();
    const blocked = parseWorldOutputStatus({ version: 1, botId: 'nova', output: ready, visualAvailable: false }, 'nova');
    expect(blocked.visualAvailable).toBe(false);
    expect(blocked.output?.readiness.state).toBe('ready');
    expect(parseWorldOutputStatus({ version: 1, botId: 'nova', output: ready, visualAvailable: true }, 'nova').visualAvailable).toBe(true);
    for (const value of [null, { ...ready, role: 'mirror' }, output({ state: 'awaiting_layout' })]) {
      expect(() => parseWorldOutputStatus({ version: 1, botId: 'nova', output: value, visualAvailable: true }, 'nova')).toThrow();
    }
  });

  it.each(['ready', 'awaiting_measurement'] as const)('rejects another actor inside a same-bot %s status envelope', state => {
    const readiness: RegisteredWorldActor['readiness'] = state === 'ready'
      ? { state, binding: { ...binding(), actorId: 'james' } }
      : { state, challenge: { ...challenge(), expected: { ...stamp(), actorId: 'james' } } };
    expect(() => parseWorldOutputStatus({ version: 1, botId: 'nova', output: output(readiness), visualAvailable: false }, 'nova')).toThrow();
    expect(() => parseWorldOutputStatus({ version: 1, botId: 'james', output: output(), visualAvailable: false }, 'nova')).toThrow();
  });

  it('rejects unsupported challenge policies, unsafe revisions and nonfinite reference or scale observations', () => {
    for (const value of [
      { ...challenge(), expiresInMs: 5001 }, { ...challenge(), cameraPolicy: 'free_camera' },
      { ...challenge(), profileScale: .299 }, { ...challenge(), effectiveScale: Infinity },
      { ...challenge(), expected: { ...stamp(), referenceRevision: 0 } },
      { ...challenge(), expected: { ...stamp(), layoutRevision: Number.MAX_SAFE_INTEGER + 1 } },
      { ...challenge(), expectedReferenceLocal: { x: 0, y: NaN, z: 0 } },
    ]) expect(() => parseMeasurementChallenge(value)).toThrow();
  });

  it('requires the exact requested frame and receipt sequence without accepting unknown completion data', () => {
    const measurement = { version: 1, frameSequence: 7, output: output() };
    expect(parseMeasurementAck(measurement, 7).frameSequence).toBe(7);
    expect(() => parseMeasurementAck(measurement, 8)).toThrow();
    const action = { version: 1, receiptSequence: 3, outcome: 'accepted', runtimeStatus: 'started', confirmedPhase: 'started', reason: null };
    expect(parseActionReportAck(action, 3).confirmedPhase).toBe('started');
    for (const value of [
      { ...action, receiptSequence: 2 }, { ...action, receiptSequence: 3.5 },
      { ...action, outcome: 'success' }, { ...action, confirmedPhase: 'arrived' },
      { ...action, completion: { effect: 100 } }, { ...action, reason: 'raw driver output' },
    ]) expect(() => parseActionReportAck(value, 3)).toThrow();
  });
});
