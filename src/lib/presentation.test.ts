import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EmoteSequence, emoteDurationMilliseconds, emoteSequenceMilliseconds } from './presentation';

describe('speech-aligned emote scheduling', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('starts the first gesture on presentation and waits its duration for the next', () => {
    const apply = vi.fn();
    const sequence = new EmoteSequence(apply);
    expect(apply).not.toHaveBeenCalled();
    sequence.start([{ action: 'wave', duration: 1.2 }, { action: 'celebrate' }]);
    expect(apply.mock.calls.map(([e]) => e.action)).toEqual(['wave']);
    vi.advanceTimersByTime(1199);
    expect(apply).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(apply.mock.calls.map(([e]) => e.action)).toEqual(['wave', 'celebrate']);
  });

  it('never presents a pending gesture after Stop', () => {
    const apply = vi.fn();
    const sequence = new EmoteSequence(apply);
    sequence.start([{ action: 'wave', duration: 1 }, { action: 'hide' }]);
    sequence.stop();
    vi.runAllTimers();
    expect(apply.mock.calls.map(([e]) => e.action)).toEqual(['wave']);
  });

  it('a replacement cancels the previous response gestures', () => {
    const apply = vi.fn();
    const sequence = new EmoteSequence(apply);
    sequence.start([{ action: 'wave' }, { action: 'hide' }]);
    sequence.start([{ action: 'nod' }]);
    vi.runAllTimers();
    expect(apply.mock.calls.map(([e]) => e.action)).toEqual(['wave', 'nod']);
  });

  it('keeps long VRMA natural timing in both the sequence and its presentation tail', () => {
    const motion = { action: 'vrma:00000000-0000-4000-8000-000000000001.vrma:0', duration: 90 };
    const following = { action: 'wave', duration: 1.2 };
    const apply = vi.fn();
    new EmoteSequence(apply).start([motion, following]);
    expect(emoteDurationMilliseconds(motion)).toBe(90_000);
    expect(emoteSequenceMilliseconds([motion, following])).toBe(91_200);
    vi.advanceTimersByTime(89_999); expect(apply).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(1); expect(apply.mock.calls.map(([emote]) => emote.action)).toEqual([motion.action, 'wave']);
  });

  it('bounds VRMA durations independently of ordinary emotes', () => {
    expect(emoteDurationMilliseconds({ action: 'vrma:motion:0', duration: 999 })).toBe(120_000);
    expect(emoteDurationMilliseconds({ action: 'vrma:motion:0', duration: 0.001 })).toBe(1);
    expect(emoteDurationMilliseconds({ action: 'wave', duration: 999 })).toBe(15_000);
    expect(emoteDurationMilliseconds({ action: 'wave', duration: 0.001 })).toBe(200);
  });
});
