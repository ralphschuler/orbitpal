import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SubtitleLifetime } from './subtitles';

describe('Untertitel nach tatsächlichem Sprachende', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('zeigt beim Start an und blendet auch lange Antworten erst 15 Sekunden nach Ende aus', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    expect(show).not.toHaveBeenCalled();
    subtitles.start('long-answer', 'Eine längere Antwort.');
    vi.advanceTimersByTime(60_000);
    expect(show).toHaveBeenLastCalledWith('Eine längere Antwort.');
    subtitles.finish('long-answer');
    vi.advanceTimersByTime(14_999);
    expect(show).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(show).toHaveBeenLastCalledWith('');
  });

  it('entfernt mit dem Timer nur die Anzeige, nicht den gespeicherten Verlauf', () => {
    const history = [{ id: 'answer', text: 'Bleibt im Gesprächsverlauf.' }];
    let visible = '';
    const subtitles = new SubtitleLifetime((text) => { visible = text; });
    subtitles.start(history[0].id, history[0].text);
    subtitles.finish(history[0].id);
    vi.advanceTimersByTime(15_000);
    expect(visible).toBe('');
    expect(history).toEqual([{ id: 'answer', text: 'Bleibt im Gesprächsverlauf.' }]);
  });

  it('lässt eine neuere Antwort nicht vom Timer ihres Vorgängers ausblenden', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    subtitles.start('old', 'Vorherige Antwort');
    subtitles.finish('old');
    vi.advanceTimersByTime(5000);
    subtitles.start('new', 'Neue Antwort');
    vi.advanceTimersByTime(30_000);
    expect(show).toHaveBeenLastCalledWith('Neue Antwort');
    expect(show).toHaveBeenCalledTimes(2);
    subtitles.finish('new');
    vi.advanceTimersByTime(15_000);
    expect(show).toHaveBeenLastCalledWith('');
  });

  it('ignoriert verspätete Endmeldungen anderer Jobs', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    subtitles.start('new', 'Noch gesprochen');
    subtitles.finish('old');
    vi.advanceTimersByTime(20_000);
    expect(show).toHaveBeenCalledExactlyOnceWith('Noch gesprochen');
  });

  it('verlängert den Nachlauf durch doppelte Endmeldungen nicht', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    subtitles.start('answer', 'Einmal beenden');
    subtitles.finish('answer');
    vi.advanceTimersByTime(10_000);
    subtitles.finish('answer');
    vi.advanceTimersByTime(5000);
    expect(show).toHaveBeenLastCalledWith('');
  });

  it('löscht bei Stop sofort und verwirft spätere Endmeldungen', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    subtitles.start('answer', 'Wird gestoppt');
    subtitles.finish('answer');
    subtitles.cancel();
    subtitles.finish('answer');
    const calls = show.mock.calls.length;
    vi.runAllTimers();
    expect(show).toHaveBeenLastCalledWith('');
    expect(show).toHaveBeenCalledTimes(calls);
  });

  it('lässt einen Fehler eines alten Jobs die neue Anzeige nicht löschen', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    subtitles.start('new', 'Aktuelle Antwort');
    subtitles.cancel('old');
    expect(show).toHaveBeenCalledExactlyOnceWith('Aktuelle Antwort');
    subtitles.cancel('new');
    expect(show).toHaveBeenLastCalledWith('');
  });

  it('räumt beim Unmount auf und nimmt anschließend keine Jobs mehr an', () => {
    const show = vi.fn();
    const subtitles = new SubtitleLifetime(show);
    subtitles.start('answer', 'Noch sichtbar');
    subtitles.finish('answer');
    subtitles.destroy();
    const calls = show.mock.calls.length;
    subtitles.start('late', 'Nach Unmount');
    subtitles.finish('late');
    subtitles.cancel();
    vi.runAllTimers();
    expect(show).toHaveBeenLastCalledWith('');
    expect(show).toHaveBeenCalledTimes(calls);
  });
});
