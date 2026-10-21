import { afterEach, describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { AvatarStage, createRobot } from './avatar';
import { BenchmarkBatcher, FreshSubmissionGate, DIAGNOSTIC_BINS, DIAGNOSTIC_COUNT_LIMIT, DIAGNOSTIC_DURATION_LIMIT,
  parseBenchmarkPage, parseBenchmarkReply, StageDiagnosticCounters, type BenchmarkPacket, type StageObservation } from './stage-diagnostics';

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
const sum = (values: number[]) => values.reduce((total, value) => total + value, 0);

describe('A fresh submission after explicit asset load', () => {
  it('ignores constructor submissions and RAF-only activity', async () => {
    vi.useFakeTimers(); const gate = new FreshSubmissionGate(); const ready = vi.fn();
    gate.observe({ kind: 'submitted', at: 1, cpuMs: 1 });
    expect(vi.getTimerCount()).toBe(0);
    const pending = gate.armAfterLoad().then(ready);
    gate.observe({ kind: 'raf', at: 20 }); await vi.advanceTimersByTimeAsync(1000);
    expect(ready).not.toHaveBeenCalled();
    gate.observe({ kind: 'submitted', at: 40, cpuMs: 2 }); await pending;
    expect(ready).toHaveBeenCalledOnce(); expect(vi.getTimerCount()).toBe(0);
    await expect(gate.armAfterLoad()).rejects.toThrow('unavailable');
  });
  it('never qualifies a stalled renderer and clears the deadline', async () => {
    vi.useFakeTimers(); const gate = new FreshSubmissionGate();
    const pending = expect(gate.armAfterLoad()).rejects.toThrow('unavailable');
    await vi.advanceTimersByTimeAsync(5000); await pending;
    gate.observe({ kind: 'submitted', at: 6000, cpuMs: 2 });
    expect(vi.getTimerCount()).toBe(0);
  });
  it('cancels startup on page teardown without leaving a timer or late grant', async () => {
    vi.useFakeTimers(); const gate = new FreshSubmissionGate();
    const pending = expect(gate.armAfterLoad()).rejects.toThrow('unavailable');
    gate.cancel(); await pending; gate.observe({ kind: 'submitted', at: 40, cpuMs: 2 });
    expect(vi.getTimerCount()).toBe(0);
  });
  it('rejects failure or disposal before load completes', async () => {
    vi.useFakeTimers();
    for (const kind of ['failure', 'context_lost', 'disposed'] as const) {
      const gate = new FreshSubmissionGate(); gate.observe({ kind });
      await expect(gate.armAfterLoad()).rejects.toThrow('unavailable');
    }
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('Cumulative Stage observations', () => {
  it('distinguishes callback intervals, actual submissions and CPU elapsed', () => {
    const counters = new StageDiagnosticCounters();
    counters.observe({ kind: 'created' });
    counters.observe({ kind: 'submitted', at: 5, cpuMs: 3.25 }); // Initial direct call, no RAF.
    for (const at of [10, 26.7, 60.2]) counters.observe({ kind: 'raf', at });
    counters.observe({ kind: 'submitted', at: 55, cpuMs: 8.5 });
    counters.observe({ kind: 'disposed' });
    const result = counters.snapshot();
    expect(result.telemetryInvalid).toBe(false);
    expect(result.counters).toEqual({ created: 1, disposed: 1, raf: 3, submitted: 2, contextLost: 0, failures: 0 });
    expect(sum(result.histograms.raf)).toBe(2);
    expect(sum(result.histograms.submission)).toBe(1);
    expect(sum(result.histograms.cpu)).toBe(2);
    expect(result.histograms.raf[16]).toBe(1);
    expect(result.histograms.submission[50]).toBe(1);
    expect(result.histograms.cpu[3]).toBe(1);
    expect(result.max.raf).toBeCloseTo(33.5);
    expect(result.max.cpu).toBe(8.5);
  });
  it('records 256ms and larger gaps in the explicit overflow bin', () => {
    const counters = new StageDiagnosticCounters(); counters.observe({ kind: 'created' });
    counters.observe({ kind: 'raf', at: 0 });
    counters.observe({ kind: 'raf', at: 256 });
    counters.observe({ kind: 'raf', at: 256 + DIAGNOSTIC_DURATION_LIMIT });
    const result = counters.snapshot();
    expect(result.histograms.raf).toHaveLength(DIAGNOSTIC_BINS);
    expect(result.histograms.raf[256]).toBe(2);
    expect(result.max.raf).toBe(DIAGNOSTIC_DURATION_LIMIT);
    expect(result.telemetryInvalid).toBe(false);
  });
  it('marks invalid clocks without manufacturing replacement samples or resetting history', () => {
    const counters = new StageDiagnosticCounters(); counters.observe({ kind: 'created' });
    for (const at of [10, 9, NaN, 11]) counters.observe({ kind: 'raf', at });
    counters.observe({ kind: 'submitted', at: 12, cpuMs: Infinity });
    const result = counters.snapshot();
    expect(result.telemetryInvalid).toBe(true);
    expect(result.counters.raf).toBe(4);
    expect(sum(result.histograms.raf)).toBe(1);
    expect(result.histograms.raf[1]).toBe(1);
    expect(sum(result.histograms.cpu)).toBe(0);
  });
  it('saturates a lowered fixture limit instead of wrapping a counter', () => {
    const counters = new StageDiagnosticCounters(3); counters.observe({ kind: 'created' });
    for (const at of [0, 1, 2, 3, 4]) counters.observe({ kind: 'raf', at });
    expect(counters.snapshot().counters.raf).toBe(3);
    expect(counters.snapshot().telemetryInvalid).toBe(true);
    for (const limit of [0, 1.5, DIAGNOSTIC_COUNT_LIMIT + 1]) expect(() => new StageDiagnosticCounters(limit)).toThrow();
  });
  it('does not allow invalidation, context loss or late lifecycle work to become valid again', () => {
    const counters = new StageDiagnosticCounters(); counters.observe({ kind: 'created' });
    counters.observe({ kind: 'context_lost' }); counters.observe({ kind: 'failure' });
    counters.observe({ kind: 'disposed' }); counters.observe({ kind: 'raf', at: 20 });
    expect(counters.snapshot().telemetryInvalid).toBe(true);
    expect(counters.snapshot().counters.contextLost).toBe(1);
    expect(counters.snapshot().counters.failures).toBe(1);
  });
  it('copies outgoing snapshots so an in-flight packet cannot be changed by later frames', () => {
    const counters = new StageDiagnosticCounters(); counters.observe({ kind: 'created' });
    counters.observe({ kind: 'raf', at: 0 }); const first = counters.snapshot();
    counters.observe({ kind: 'raf', at: 17 });
    expect(first.counters.raf).toBe(1); expect(sum(first.histograms.raf)).toBe(0);
    first.counters.created = 99; first.histograms.raf[17] = 99;
    expect(counters.snapshot().counters.created).toBe(1);
    expect(counters.snapshot().histograms.raf[17]).toBe(1);
  });
});

describe('Closed benchmark page and host replies', () => {
  it('accepts only native-issued safe decimal epochs or the static control page', () => {
    expect(parseBenchmarkPage('?benchmark=control')).toEqual({ kind: 'control' });
    expect(parseBenchmarkPage('?benchmark=figure&epoch=21')).toEqual({ kind: 'figure', epoch: 21 });
    for (const query of ['', '?benchmark=control&epoch=1', '?benchmark=figure&epoch=0', '?benchmark=figure&epoch=01',
      '?benchmark=figure&epoch=1&epoch=2', '?benchmark=figure&epoch=1&path=private',
      '?benchmark=figure&epoch=9007199254740992', '?benchmark=figure&epoch=1e2']) expect(parseBenchmarkPage(query)).toBeNull();
  });
  it('rejects unknown fields, foreign epochs, unknown commands and non-success replies', () => {
    expect(parseBenchmarkReply({ accepted: true, epoch: 2, command: 'continue' }, 2)).toBe('continue');
    for (const reply of [{ accepted: true, epoch: 3, command: 'continue' }, { accepted: false, epoch: 2, command: 'continue' },
      { accepted: true, epoch: 2, command: 'eval' }, { accepted: true, epoch: 2, command: 'dispose', path: 'private' }, null])
      expect(parseBenchmarkReply(reply, 2)).toBeNull();
  });
});

function batchFixture() {
  const diagnostics = new StageDiagnosticCounters(); diagnostics.observe({ kind: 'created' });
  let disposed = false, domOut = false;
  const send = vi.fn((_packet: BenchmarkPacket): Promise<unknown> => Promise.resolve({ accepted: true, epoch: 1, command: 'continue' }));
  const command = vi.fn((value: 'dom_out' | 'dom_in' | 'dispose') => {
    if (value === 'dispose') { diagnostics.observe({ kind: 'disposed' }); disposed = true; }
    else domOut = value === 'dom_out';
  });
  const batcher = new BenchmarkBatcher(1, () => ({ ...diagnostics.snapshot(), disposed, domOut }), send, command, () => diagnostics.invalidate());
  return { batcher, diagnostics, send, command };
}
describe('Single-flight native batching and final cleanup ACK', () => {
  it('never overlaps a held promise and preserves strict cumulative sequence', async () => {
    vi.useFakeTimers(); const f = batchFixture();
    let release!: (value: unknown) => void;
    f.send.mockImplementationOnce(() => new Promise(resolve => { release = resolve; }));
    f.batcher.start(); await vi.advanceTimersByTimeAsync(1000);
    await vi.advanceTimersByTimeAsync(5000); expect(f.send).toHaveBeenCalledTimes(1);
    release({ accepted: true, epoch: 1, command: 'continue' }); await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1000); expect(f.send).toHaveBeenCalledTimes(2);
    expect(f.send.mock.calls.map(([packet]) => packet.sequence)).toEqual([1, 2]);
    f.batcher.stop();
  });
  it('stops after timeout and ignores a late dispose reply without another in-flight call', async () => {
    vi.useFakeTimers(); const f = batchFixture(); let release!: (value: unknown) => void;
    f.send.mockImplementationOnce(() => new Promise(resolve => { release = resolve; }));
    f.batcher.start(); await vi.advanceTimersByTimeAsync(11_000);
    release({ accepted: true, epoch: 1, command: 'dispose' }); await vi.advanceTimersByTimeAsync(20_000);
    expect(f.send).toHaveBeenCalledTimes(1); expect(f.command).not.toHaveBeenCalled();
    expect(f.diagnostics.snapshot().telemetryInvalid).toBe(true);
  });
  it('sends one final disposed packet only after successful actual cleanup callback', async () => {
    vi.useFakeTimers(); const f = batchFixture();
    f.send.mockResolvedValueOnce({ accepted: true, epoch: 1, command: 'dispose' });
    f.batcher.start(); await vi.advanceTimersByTimeAsync(1000);
    expect(f.send).toHaveBeenCalledTimes(2);
    const packets = f.send.mock.calls.map(([packet]) => packet);
    expect(packets.map(packet => [packet.sequence, packet.disposed, packet.counters.disposed])).toEqual([[1, false, 0], [2, true, 1]]);
    await vi.advanceTimersByTimeAsync(20_000); expect(f.send).toHaveBeenCalledTimes(2);
    expect(f.command).toHaveBeenCalledOnce(); expect(vi.getTimerCount()).toBe(0);
  });
  it('never acknowledges a partial/throwing dispose', async () => {
    vi.useFakeTimers(); const f = batchFixture();
    f.send.mockResolvedValueOnce({ accepted: true, epoch: 1, command: 'dispose' });
    f.command.mockImplementationOnce(() => { throw new Error('fixture cleanup failed'); });
    f.batcher.start(); await vi.advanceTimersByTimeAsync(20_000);
    expect(f.send).toHaveBeenCalledOnce(); expect(f.diagnostics.snapshot().counters.disposed).toBe(0);
    expect(f.diagnostics.snapshot().telemetryInvalid).toBe(true); expect(vi.getTimerCount()).toBe(0);
  });
  it('makes stop during an outstanding promise a terminal barrier', async () => {
    vi.useFakeTimers(); const f = batchFixture(); let release!: (value: unknown) => void;
    f.send.mockImplementationOnce(() => new Promise(resolve => { release = resolve; }));
    f.batcher.start(); await vi.advanceTimersByTimeAsync(1000); f.batcher.stop();
    release({ accepted: true, epoch: 1, command: 'dispose' }); await vi.advanceTimersByTimeAsync(20_000);
    expect(f.send).toHaveBeenCalledOnce(); expect(f.command).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
  });
  it('treats a foreign host reply as fatal without applying DOM commands', async () => {
    vi.useFakeTimers(); const f = batchFixture(); f.send.mockResolvedValueOnce({ accepted: true, epoch: 2, command: 'dom_out' });
    f.batcher.start(); await vi.advanceTimersByTimeAsync(20_000);
    expect(f.send).toHaveBeenCalledOnce(); expect(f.command).not.toHaveBeenCalled();
    expect(f.diagnostics.snapshot().telemetryInvalid).toBe(true);
  });
});

function cleanupStage(sink: (observation: StageObservation) => void, fail = false) {
  const stage = Object.create(AvatarStage.prototype) as any;
  const calls: string[] = [];
  const mark = (name: string) => () => { calls.push(name); };
  Object.assign(stage, { diagnostics: sink, diagnosticsFailed: false, diagnosticsDisposed: false, request: 0, frame: 1,
    world: { dispose: mark('world') }, presentation: { dispose: mark('presentation') }, modelAudio: { destroy: mark('audio'), clear: mark('audio-clear'), stop: mark('audio-stop') },
    observer: { disconnect: mark('observer') }, controls: { dispose: mark('controls') }, desktopMotion: { reset: mark('motion') },
    previousBoneOffsets: new Map(), imported: new THREE.Group(), scene: new THREE.Scene(), assetLoader: { dispose: mark('loader') },
    robot: createRobot(), travel: new THREE.Group(), walkClock: { reset: mark('walk-clock') }, walkAnchorRest: new THREE.Vector3(),
    humanoidPose: new Map(), humanoidBasis: new THREE.Quaternion(), faceVowels: new Map(), vrmExpressions: new Map(), mouthExpressions: new Map(), genericBoneRest: new Map(),
    renderer: { domElement: { removeEventListener: mark('listener'), remove: mark('canvas') },
      dispose: () => { calls.push('renderer'); if (fail) throw new Error('fixture renderer cleanup failed'); }, forceContextLoss: mark('context') },
  });
  stage.scene.add(stage.robot.root);
  vi.stubGlobal('cancelAnimationFrame', mark('cancel-raf'));
  return { stage, calls };
}
describe('Actual Stage observation seams without a WebGL claim', () => {
  it('contains a throwing observer and marks only telemetry invalid', () => {
    const stage = Object.create(AvatarStage.prototype) as any;
    stage.diagnostics = () => { throw new Error('fixture sink failed'); };
    expect(() => stage.observeDiagnostic({ kind: 'created' })).not.toThrow();
    expect(stage.telemetryInvalid).toBe(true);
    stage.diagnostics = undefined; expect(() => stage.observeDiagnostic({ kind: 'raf', at: 2 })).not.toThrow();
    expect(stage.telemetryInvalid).toBe(true);
  });
  it('emits disposed after the actual cleanup body removes its canvas, once', () => {
    const observations: StageObservation[] = [];
    const f: ReturnType<typeof cleanupStage> = cleanupStage(observation => { expect(f.calls.at(-1)).toBe('canvas'); observations.push(observation); });
    f.stage.dispose(); f.stage.dispose();
    expect(observations).toEqual([{ kind: 'disposed' }]);
    expect(f.calls).toContain('cancel-raf'); expect(f.calls).toContain('loader');
    expect(f.calls.indexOf('renderer')).toBeLessThan(f.calls.indexOf('context'));
    expect(f.calls.indexOf('context')).toBeLessThan(f.calls.indexOf('canvas'));
  });
  it('reports failure and never disposed when actual cleanup throws', () => {
    const observations: StageObservation[] = [];
    const f = cleanupStage(observation => observations.push(observation), true);
    expect(() => f.stage.dispose()).toThrow('fixture renderer cleanup failed');
    expect(observations).toEqual([{ kind: 'failure' }]);
    expect(f.calls).not.toContain('context'); expect(f.calls).not.toContain('canvas');
  });
});
