import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PlaybackWaiters, matchesPlayback, cancelPlaybackTurn, type PlaybackIdentity } from './playback-wait';
import { AutopilotScheduler } from './autopilot';

describe('native playback completion leases', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('awaits cancellation of the exact bot/job before releasing the UI', async () => {
    let release!: () => void;
    const cancel = vi.fn(() => new Promise<void>(resolve => { release = resolve; }));
    const timeout = vi.fn();
    const waiters = new PlaybackWaiters();
    let finished = false;
    const waiting = waiters.wait('old-job', 'nova', cancel, timeout).then(() => { finished = true; });
    await vi.advanceTimersByTimeAsync(180000);
    expect(cancel).toHaveBeenCalledExactlyOnceWith('old-job', 'nova');
    expect(finished).toBe(false);
    release(); await waiting;
    expect(timeout).toHaveBeenCalledOnce();
    expect(finished).toBe(true);
  });

  it('does not cancel completed, stopped or already acknowledged jobs', async () => {
    const cancel = vi.fn(async () => {});
    const waiters = new PlaybackWaiters();
    waiters.finish('early');
    await waiters.wait('early', 'nova', cancel, vi.fn());
    const one = waiters.wait('one', 'nova', cancel, vi.fn());
    const two = waiters.wait('two', 'other', cancel, vi.fn());
    waiters.finish();
    await Promise.all([one, two]);
    await vi.advanceTimersByTimeAsync(180000);
    expect(cancel).not.toHaveBeenCalled();
  });

  it('ignores a late cancellation result after Stop and a newer bot job', async () => {
    let release!: () => void;
    const cancel = vi.fn(() => new Promise<void>(resolve => { release = resolve; }));
    const timeout = vi.fn();
    const waiters = new PlaybackWaiters();
    const old = waiters.wait('old', 'nova', cancel, timeout);
    await vi.advanceTimersByTimeAsync(180000);
    waiters.finish(); await old;
    let newerFinished = false;
    const newer = waiters.wait('new', 'other', cancel, vi.fn()).then(() => { newerFinished = true; });
    release(); await Promise.resolve(); await Promise.resolve();
    expect(timeout).not.toHaveBeenCalled();
    expect(newerFinished).toBe(false);
    expect(cancel).toHaveBeenCalledTimes(1);
    waiters.finish('new'); await newer;
  });

  it('surfaces a cancellation failure and leaves another waiter intact', async () => {
    const waiters = new PlaybackWaiters();
    const failed = waiters.wait('failed', 'nova', async () => { throw new Error('Host disconnected'); }, vi.fn(), 100);
    const assertion = expect(failed).rejects.toThrow('Host disconnected');
    let otherFinished = false;
    const other = waiters.wait('other', 'james', vi.fn(), vi.fn(), 1000).then(() => { otherFinished = true; });
    await vi.advanceTimersByTimeAsync(100); await assertion;
    expect(otherFinished).toBe(false);
    waiters.finish('other'); await other;
  });

  it('correlates presentation before the RPC resolves and rejects a cancelled late reply', async () => {
    let resolve!: (reply: PlaybackIdentity) => void;
    const rpc = new Promise<PlaybackIdentity>(done => { resolve = done; });
    let current: PlaybackIdentity = { id: 'request-one', botId: 'nova' };
    const early = { id: 'request-one', botId: 'nova' };
    expect(matchesPlayback(early, current)).toBe(true);
    const stop = vi.fn(() => { current = { id: '', botId: '' }; });
    expect(cancelPlaybackTurn(early, current, vi.fn(), stop)).toBe(true);
    resolve(early);
    expect(matchesPlayback(await rpc, current)).toBe(false);
    current = { id: 'request-two', botId: 'nova' };
    expect(cancelPlaybackTurn(early, current, vi.fn(), stop)).toBe(false);
    expect(matchesPlayback(early, current)).toBe(false);
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it('releases the autonomous lease on timeout so another bot can speak', async () => {
    let clock = 0;
    const scheduler = new AutopilotScheduler({ now: () => clock, random: () => 0 });
    const config = { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 12 };
    scheduler.sync([{ botId: 'a', seed: 'a', config }, { botId: 'b', seed: 'b', config }]);
    clock = 15000; expect(scheduler.next()).toBe('a');
    let current: PlaybackIdentity = { id: 'autopilot-job', botId: 'a', autopilot: true };
    const waiters = new PlaybackWaiters();
    const target = { id: current.id, botId: current.botId };
    const waiting = waiters.wait(target.id, target.botId, async () => {
      cancelPlaybackTurn(target, current, botId => scheduler.finish(botId), () => {
        current = { id: '', botId: '', autopilot: false };
        waiters.finish(target.id);
      });
    }, vi.fn());
    clock += 180000; await vi.advanceTimersByTimeAsync(180000); await waiting;
    clock += 15000; expect(scheduler.next()).toBe('b');
  });
});
