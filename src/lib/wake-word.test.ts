import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { recordingToWav } from './audio';
import { findWakePhrase, normalizeWakePhrases, WakeWordListener } from './wake-word';

vi.mock('./audio', () => ({ recordingToWav: vi.fn() }));
const wav = { audioBase64: 'local-fixture', mimeType: 'audio/wav' };

describe('Aktivierungsphrasen', () => {
  it('ignoriert Großschreibung und Satzzeichen und erhält die Frage', () => {
    expect(findWakePhrase('HEY, Nova! Wie spät ist es?', ['hey nova']))
      .toEqual({ phrase: 'hey nova', request: 'Wie spät ist es?' });
  });

  it('erkennt Unicode-Buchstaben und normalisierte Umlaute', () => {
    expect(findWakePhrase('HEJ, MA\u0308RTA: Grüße die Gäste.', ['hej märta']))
      .toEqual({ phrase: 'hej märta', request: 'Grüße die Gäste.' });
  });

  it('prüft ganze Wörter statt Namenspräfixe', () => {
    expect(findWakePhrase('Hey Novak, bist du da?', ['hey nova'])).toBeUndefined();
    expect(findWakePhrase('Hey supernova, bist du da?', ['nova'])).toBeUndefined();
  });

  it('findet eine Phrase im Text und verwendet nur die nachfolgende Frage', () => {
    expect(findWakePhrase('Einen Moment. Hey Nova – erkläre mir das bitte!', ['hey nova']))
      .toEqual({ phrase: 'hey nova', request: 'erkläre mir das bitte!' });
  });

  it('bevorzugt am gleichen Wort die längste eingerichtete Phrase', () => {
    expect(findWakePhrase('hey nova bot, hallo', ['hey nova', 'hey nova bot']))
      .toEqual({ phrase: 'hey nova bot', request: 'hallo' });
  });

  it('gibt eine allein gesprochene Phrase ohne erfundene Frage zurück', () => {
    expect(findWakePhrase('Hey, Nova!', ['hey nova'])).toEqual({ phrase: 'hey nova', request: '' });
    expect(findWakePhrase('Hallo zusammen', ['hey nova'])).toBeUndefined();
  });

  it('entfernt doppelte, leere und ungültige Konfigurationseinträge', () => {
    expect(normalizeWakePhrases(['  hey   nova ', 'HEY, NOVA', '', '!!!', null, 'x'.repeat(101), 'okay bot']))
      .toEqual(['hey nova', 'okay bot']);
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe('Mikrofon- und Antwortlebenszyklus mit lokalen Browser-Doubles', () => {
  let volume = 0;
  let listeners: WakeWordListener[];
  let track: { enabled: boolean; readyState: string; onended: (() => void) | null; stop: ReturnType<typeof vi.fn> };
  let stream: { getTracks: () => typeof track[]; getAudioTracks: () => typeof track[] };
  let getUserMedia: ReturnType<typeof vi.fn>;
  let createdRecorders: FakeMediaRecorder[];

  class FakeMediaRecorder {
    static isTypeSupported = () => true;
    state = 'inactive';
    mimeType = 'audio/webm';
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;
    onerror: (() => void) | null = null;
    constructor() { createdRecorders.push(this); }
    start() { this.state = 'recording'; }
    stop() {
      this.state = 'inactive';
      queueMicrotask(() => {
        this.ondataavailable?.({ data: new Blob(['mock recording'], { type: this.mimeType }) });
        this.onstop?.();
      });
    }
  }

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'performance'] });
    volume = 0;
    listeners = [];
    createdRecorders = [];
    track = { enabled: true, readyState: 'live', onended: null, stop: vi.fn(() => { track.readyState = 'ended'; }) };
    stream = { getTracks: () => [track], getAudioTracks: () => [track] };
    getUserMedia = vi.fn().mockResolvedValue(stream);
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia } });
    vi.stubGlobal('MediaRecorder', FakeMediaRecorder);
    vi.stubGlobal('AudioContext', class {
      state = 'running';
      destination = {};
      resume = vi.fn().mockResolvedValue(undefined);
      close = vi.fn(async () => { this.state = 'closed'; });
      createMediaStreamSource = () => ({ connect: vi.fn(), disconnect: vi.fn() });
      createGain = () => ({ gain: { value: 1 }, connect: vi.fn(), disconnect: vi.fn() });
      createAnalyser = () => ({
        fftSize: 1024, smoothingTimeConstant: 0, connect: vi.fn(), disconnect: vi.fn(),
        getFloatTimeDomainData: (samples: Float32Array) => samples.fill(volume),
      });
    });
    vi.mocked(recordingToWav).mockReset().mockResolvedValue(wav);
  });

  afterEach(() => {
    listeners.forEach((listener) => listener.stop());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  function setup(transcribe = vi.fn().mockResolvedValue('hey nova, eine Frage')) {
    const onTrigger = vi.fn().mockResolvedValue(undefined);
    const onState = vi.fn();
    const onError = vi.fn();
    const listener = new WakeWordListener({ phrases: ['hey nova'], transcribe, onTrigger, onState, onError });
    listeners.push(listener);
    return { listener, transcribe, onTrigger, onState, onError };
  }

  async function start(listener: WakeWordListener) {
    await listener.start();
    await vi.advanceTimersByTimeAsync(0);
  }

  async function utterance() {
    volume = 0.08;
    await vi.advanceTimersByTimeAsync(300);
    volume = 0;
    await vi.advanceTimersByTimeAsync(700);
  }

  it('öffnet beim Erstellen und bei Stille weder Transkription noch Modellaufruf', async () => {
    const { listener, transcribe, onTrigger } = setup();
    expect(getUserMedia).not.toHaveBeenCalled();
    await start(listener);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    expect(createdRecorders.length).toBeGreaterThan(1);
    expect(recordingToWav).not.toHaveBeenCalled();
    expect(transcribe).not.toHaveBeenCalled();
    expect(onTrigger).not.toHaveBeenCalled();
  });

  it('beginnt keine Aufnahme, wenn das Speichern abgelehnt wurde', async () => {
    const { listener } = setup();
    await listener.start(() => false);
    expect(getUserMedia).not.toHaveBeenCalled();
  });

  it('übergibt die Frage und deaktiviert Aufnahme vor dem Antwortcallback', async () => {
    const { listener, onTrigger, transcribe } = setup();
    onTrigger.mockImplementation(async () => { expect(track.enabled).toBe(false); });
    await start(listener);
    await utterance();
    expect(transcribe).toHaveBeenCalledWith(wav);
    expect(onTrigger).toHaveBeenCalledExactlyOnceWith('eine Frage');
  });

  it('akzeptiert nach alleiniger Aktivierung die nächste Frage ohne erneute Phrase', async () => {
    const transcribe = vi.fn().mockResolvedValueOnce('Hey Nova!').mockResolvedValueOnce('Was passiert heute?');
    const { listener, onTrigger, onState } = setup(transcribe);
    await start(listener);
    await utterance();
    expect(onTrigger).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(250);
    expect(onState).toHaveBeenCalledWith('follow-up');
    await utterance();
    expect(onTrigger).toHaveBeenCalledExactlyOnceWith('Was passiert heute?');
  });

  it('leitet nach Ablauf des Nachfragefensters keine gewöhnliche Äußerung weiter', async () => {
    const transcribe = vi.fn().mockResolvedValueOnce('Hey Nova').mockResolvedValueOnce('Ein Gespräch ohne Anrede');
    const { listener, onTrigger } = setup(transcribe);
    await start(listener);
    await utterance();
    await vi.advanceTimersByTimeAsync(11_000);
    await utterance();
    expect(transcribe).toHaveBeenCalledTimes(2);
    expect(onTrigger).not.toHaveBeenCalled();
  });

  it('verwirft eine nach Stop verspätet abgeschlossene WAV-Konvertierung', async () => {
    const conversion = deferred<typeof wav>();
    vi.mocked(recordingToWav).mockReturnValueOnce(conversion.promise);
    const { listener, transcribe, onTrigger } = setup();
    await start(listener);
    await utterance();
    expect(recordingToWav).toHaveBeenCalledTimes(1);
    listener.stop();
    conversion.resolve(wav);
    await vi.advanceTimersByTimeAsync(1000);
    expect(transcribe).not.toHaveBeenCalled();
    expect(onTrigger).not.toHaveBeenCalled();
    expect(track.stop).toHaveBeenCalled();
  });

  it('verwirft eine nach Stop verspätet eingehende Transkription', async () => {
    const result = deferred<string>();
    const transcribe = vi.fn().mockReturnValue(result.promise);
    const { listener, onTrigger, onState } = setup(transcribe);
    await start(listener);
    await utterance();
    expect(transcribe).toHaveBeenCalledTimes(1);
    listener.stop();
    result.resolve('Hey Nova, starte jetzt');
    await vi.advanceTimersByTimeAsync(2000);
    expect(onTrigger).not.toHaveBeenCalled();
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it('stoppt auch einen erst nach Stop freigegebenen Mikrofonstream', async () => {
    const permission = deferred<typeof stream>();
    getUserMedia.mockReturnValueOnce(permission.promise);
    const { listener } = setup();
    const starting = listener.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    listener.stop();
    permission.resolve(stream);
    await starting;
    expect(track.stop).toHaveBeenCalled();
    expect(createdRecorders).toHaveLength(0);
  });

  it('pausiert laufende Abschnitte und startet nach TTS erst mit Echoabstand neu', async () => {
    const { listener, transcribe } = setup();
    await start(listener);
    volume = 0.08;
    await vi.advanceTimersByTimeAsync(300);
    listener.setBlocked(true);
    expect(track.enabled).toBe(false);
    await vi.advanceTimersByTimeAsync(3000);
    expect(transcribe).not.toHaveBeenCalled();
    volume = 0;
    listener.setBlocked(false);
    await vi.advanceTimersByTimeAsync(649);
    expect(track.enabled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(track.enabled).toBe(true);
  });

  it('beendet das Zuhören sichtbar bei fehlender STT-Verbindung', async () => {
    const transcribe = vi.fn().mockRejectedValue(new Error('Whisper-Server nicht erreichbar'));
    const { listener, onTrigger, onError, onState } = setup(transcribe);
    await start(listener);
    await utterance();
    expect(onError).toHaveBeenCalledWith('Whisper-Server nicht erreichbar');
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(track.stop).toHaveBeenCalled();
    expect(onTrigger).not.toHaveBeenCalled();
  });
});
