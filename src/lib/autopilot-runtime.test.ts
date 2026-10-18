import { describe, expect, it } from 'vitest';
import { AutopilotRuntimeVersions, mayUpdateAutopilot, type AutopilotRuntime } from './autopilot-runtime';

const runtime = (changes: Partial<AutopilotRuntime> = {}): AutopilotRuntime => ({
  botId: 'nova', active: true, wander: true, gestures: true, startedAt: 1000, seed: 'session-a', revision: 1, ...changes,
});

describe('autopilot runtime revisions', () => {
  it('retains Stop tombstones and rejects delayed active events and snapshots', () => {
    const versions = new AutopilotRuntimeVersions();
    expect(versions.accept(runtime())).toBe(true);
    expect(versions.accept(runtime({ active: false, revision: 2 }))).toBe(true);
    expect(versions.accept(runtime())).toBe(false);
    expect(versions.accept(runtime({ active: false, revision: 0 }))).toBe(false);
    expect(versions.accept(runtime({ active: false, revision: 2 }))).toBe(true);
  });

  it('rejects older Stop and active events after a newer session starts', () => {
    const versions = new AutopilotRuntimeVersions();
    expect(versions.accept(runtime({ seed: 'session-b', startedAt: 2000, revision: 3 }))).toBe(true);
    expect(versions.accept(runtime({ active: false, revision: 2 }))).toBe(false);
    expect(versions.accept(runtime())).toBe(false);
  });

  it('orders revisions independently for each bot', () => {
    const versions = new AutopilotRuntimeVersions();
    expect(versions.accept(runtime({ active: false, revision: 100 }))).toBe(true);
    expect(versions.accept(runtime({ botId: 'second', revision: 1 }))).toBe(true);
    expect(versions.accept(runtime({ botId: 'second', revision: 0 }))).toBe(false);
  });

  it('accepts identical duplicates but rejects every changed field at the same revision', () => {
    const versions = new AutopilotRuntimeVersions(); const original = runtime();
    expect(versions.accept(original)).toBe(true); expect(versions.accept({ ...original })).toBe(true);
    for (const mutation of [{ active: false }, { wander: false }, { gestures: false }, { startedAt: 2000 }, { seed: 'other' }]) {
      expect(versions.accept({ ...original, ...mutation })).toBe(false);
    }
    expect(versions.accept(original)).toBe(true);
  });

  it('copies accepted DTOs so caller mutation cannot rewrite the stored revision', () => {
    const versions = new AutopilotRuntimeVersions(); const original = runtime();
    versions.accept(original); original.active = false; original.revision = 99;
    expect(versions.accept(runtime())).toBe(true);
    expect(versions.accept(runtime({ active: false, revision: 2 }))).toBe(true);
  });

  it('rejects invalid numbers, fields and active seeds without poisoning later valid events', () => {
    const versions = new AutopilotRuntimeVersions();
    for (const mutation of [
      { revision: NaN }, { revision: Infinity }, { revision: -1 }, { revision: 1.5 }, { revision: Number.MAX_SAFE_INTEGER + 1 },
      { startedAt: -1 }, { startedAt: NaN }, { startedAt: 0.5 }, { seed: '' }, { seed: ' ' }, { seed: 'bad\nseed' },
      { botId: '' }, { botId: '../nova' }, { active: 'true' as unknown as boolean },
    ]) expect(versions.accept(runtime(mutation))).toBe(false);
    expect(versions.accept(runtime())).toBe(true);
    expect(versions.accept(runtime({ active: false, seed: '', revision: 0 }))).toBe(false);
    expect(new AutopilotRuntimeVersions().accept(runtime({ active: false, seed: '', revision: 0, startedAt: 0 }))).toBe(true);
  });
});

describe('autopilot session authorization', () => {
  it('requires explicit authorization to create or reactivate an active session', () => {
    expect(mayUpdateAutopilot(undefined, runtime())).toBe(false);
    expect(mayUpdateAutopilot(undefined, runtime(), true)).toBe(true);
    const stopped = runtime({ active: false });
    expect(mayUpdateAutopilot(stopped, runtime({ revision: 2 }))).toBe(false);
    expect(mayUpdateAutopilot(stopped, runtime({ revision: 2 }), true)).toBe(true);
  });

  it('allows passive updates and Stop only for the currently authorized seed', () => {
    const current = runtime();
    expect(mayUpdateAutopilot(current, runtime({ wander: false, revision: 2 }))).toBe(true);
    expect(mayUpdateAutopilot(current, runtime({ active: false, revision: 2 }))).toBe(true);
    expect(mayUpdateAutopilot(current, runtime({ seed: 'session-b', revision: 2 }))).toBe(false);
    expect(mayUpdateAutopilot(current, runtime({ active: false, seed: 'session-b', revision: 2 }))).toBe(false);
    expect(mayUpdateAutopilot(current, runtime({ seed: 'session-b', revision: 2 }), true)).toBe(true);
    expect(mayUpdateAutopilot(undefined, runtime({ active: false, revision: 2 }))).toBe(true);
  });

  it('rejects another bot, older states and conflicting equal revisions even with start authorization', () => {
    const current = runtime({ revision: 3 });
    expect(mayUpdateAutopilot(current, runtime({ botId: 'other', revision: 4 }), true)).toBe(false);
    expect(mayUpdateAutopilot(current, runtime({ revision: 2 }), true)).toBe(false);
    expect(mayUpdateAutopilot(current, runtime({ active: false, revision: 3 }), true)).toBe(false);
    expect(mayUpdateAutopilot(current, { ...current })).toBe(true);
  });

  it('allows an authorized command duplicate after its passive event was ignored', () => {
    const versions = new AutopilotRuntimeVersions(); const incoming = runtime();
    expect(versions.accept(incoming)).toBe(true); expect(mayUpdateAutopilot(undefined, incoming)).toBe(false);
    expect(versions.accept(incoming)).toBe(true); expect(mayUpdateAutopilot(undefined, incoming, true)).toBe(true);
    const stopped = runtime({ active: false, revision: 2 }); versions.accept(stopped);
    expect(versions.accept(incoming)).toBe(false); // A delayed command cannot cross a later Stop.
  });

  it('rejects malformed incoming or current states', () => {
    expect(mayUpdateAutopilot(undefined, runtime({ revision: 0.1 }), true)).toBe(false);
    expect(mayUpdateAutopilot(runtime({ revision: NaN }), runtime({ revision: 2 }), true)).toBe(false);
  });
});
