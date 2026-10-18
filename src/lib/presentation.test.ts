import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EmoteSequence } from './presentation';

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
});
