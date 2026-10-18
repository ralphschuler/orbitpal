import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ExternalEmotePlayback, type ExternalEmotePlaybackEvent } from './emote-playback';

function report(id = 'sound', playing = true, botId = 'nova', output = 'obs'): ExternalEmotePlaybackEvent {
  return { botId, output, id, playing };
}

describe('Externe Emote-Sounds pausieren die Sprachaktivierung', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

  it('meldet ausschließlich Wechsel des gesamten Wiedergabezustands', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    expect(changed).not.toHaveBeenCalled();
    playback.accept(report());
    playback.accept(report());
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    playback.accept(report('sound', false));
    playback.accept(report('sound', false));
    expect(changed.mock.calls).toEqual([[true], [false]]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('wartet bei mehreren Bots und Ausgaben auf den letzten passenden Stop', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report('same', true, 'nova', 'obs'));
    playback.accept(report('same', true, 'fox', 'obs'));
    playback.accept(report('same', true, 'nova', 'overlay'));
    playback.accept(report('same', false, 'fox', 'overlay'));
    playback.accept(report('same', false, 'nova', 'obs'));
    playback.accept(report('same', false, 'fox', 'obs'));
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    playback.accept(report('same', false, 'nova', 'overlay'));
    expect(changed.mock.calls).toEqual([[true], [false]]);
  });

  it('lässt einen verspäteten Stop eines alten Sounds keinen neueren Sound beenden', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report('old'));
    playback.accept(report('new'));
    playback.accept(report('old', false));
    playback.accept(report('old', false));
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    playback.accept(report('new', false));
    expect(changed.mock.calls).toEqual([[true], [false]]);
  });

  it('gibt eine verschwundene Ausgabe nach genau 16 Sekunden wieder frei', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report());
    vi.advanceTimersByTime(15_999);
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    vi.advanceTimersByTime(1);
    expect(changed.mock.calls).toEqual([[true], [false]]);
  });

  it('erneuert dieselbe Lease ohne weitere Zustandsmeldung und ohne alten Ablauf', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report());
    vi.advanceTimersByTime(12_000);
    playback.accept(report());
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(15_999);
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    vi.advanceTimersByTime(1);
    expect(changed.mock.calls).toEqual([[true], [false]]);
  });

  it('lässt den Ablauf eines anderen Bots einen erneuerten Sound nicht freigeben', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report('one', true, 'nova'));
    playback.accept(report('two', true, 'fox'));
    vi.advanceTimersByTime(8000);
    playback.accept(report('two', true, 'fox'));
    vi.advanceTimersByTime(8000);
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    vi.advanceTimersByTime(8000);
    expect(changed.mock.calls).toEqual([[true], [false]]);
  });

  it('räumt bei clear alle Timer auf und beeinflusst danach keinen neuen Sound', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report());
    vi.advanceTimersByTime(10_000);
    playback.clear();
    playback.clear();
    expect(vi.getTimerCount()).toBe(0);
    playback.accept(report());
    vi.advanceTimersByTime(6000);
    expect(changed.mock.calls).toEqual([[true], [false], [true]]);
    vi.advanceTimersByTime(10_000);
    expect(changed.mock.calls).toEqual([[true], [false], [true], [false]]);
  });

  it('nimmt nach destroy keine Meldungen an und führt keine späten Callbacks aus', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report());
    playback.destroy();
    playback.destroy();
    playback.accept(report('late'));
    playback.clear();
    vi.runAllTimers();
    expect(changed.mock.calls).toEqual([[true], [false]]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('ignoriert bereits eingereihte alte Timer nach Erneuerung, clear und destroy', () => {
    const changed = vi.fn();
    const timer = vi.spyOn(globalThis, 'setTimeout');
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report());
    const beforeRenewal = timer.mock.calls[0][0] as () => void;
    playback.accept(report());
    const beforeClear = timer.mock.calls[1][0] as () => void;
    beforeRenewal();
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    playback.clear();
    playback.accept(report());
    const beforeDestroy = timer.mock.calls[2][0] as () => void;
    beforeClear();
    expect(changed.mock.calls).toEqual([[true], [false], [true]]);
    playback.destroy();
    beforeDestroy();
    expect(changed.mock.calls).toEqual([[true], [false], [true], [false]]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('begrenzt die Map auf 64 Sounds, erlaubt aber weiterhin deren Heartbeats und Stops', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    for (let index = 0; index < 65; index++) playback.accept(report(`sound-${index}`));
    expect(vi.getTimerCount()).toBe(64);
    vi.advanceTimersByTime(12_000);
    playback.accept(report('sound-0'));
    for (let index = 1; index < 64; index++) playback.accept(report(`sound-${index}`, false));
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    expect(vi.getTimerCount()).toBe(1);
    playback.accept(report('sound-0', false));
    expect(changed.mock.calls).toEqual([[true], [false]]);
    expect(vi.getTimerCount()).toBe(0);
    playback.accept(report('after-capacity'));
    expect(changed.mock.calls).toEqual([[true], [false], [true]]);
    playback.clear();
  });

  it('verwirft ungültige Daten ohne Timer oder falsche Freigabe', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    const invalid: unknown[] = [
      undefined, null, [], {}, 'sound',
      { ...report(), botId: '' }, { ...report(), botId: '  ' },
      { ...report(), botId: 'a'.repeat(257) }, { ...report(), botId: 7 },
      { ...report(), id: '' }, { ...report(), id: '\n\t' },
      { ...report(), id: 'a'.repeat(257) }, { ...report(), id: {} },
      { ...report(), output: 'studio' }, { ...report(), output: 'OBS' },
      { ...report(), output: '' }, { ...report(), playing: 'true' },
      { ...report(), playing: 0 }, { ...report(), playing: undefined },
    ];
    for (const value of invalid) playback.accept(value as ExternalEmotePlaybackEvent);
    expect(changed).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
    playback.accept(report());
    for (const value of invalid) playback.accept(value as ExternalEmotePlaybackEvent);
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    expect(vi.getTimerCount()).toBe(1);
    playback.clear();
  });

  it('unterscheidet IDs mit Trennzeichen und akzeptiert die Identifier-Längengrenze', () => {
    const changed = vi.fn();
    const playback = new ExternalEmotePlayback(changed);
    playback.accept(report('x|obs|y', true, 'a'));
    playback.accept(report('y', true, 'a|obs|x'));
    playback.accept(report('x|obs|y', false, 'a'));
    expect(changed).toHaveBeenCalledExactlyOnceWith(true);
    playback.accept(report('y', false, 'a|obs|x'));
    playback.accept(report('i'.repeat(256), true, 'b'.repeat(256)));
    expect(changed.mock.calls).toEqual([[true], [false], [true]]);
    playback.clear();
  });
});
