import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { systemPermission } from './system-permission';
import { recordingToWav } from './audio';
import { findWakePhrase, normalizeWakePhrases, WakeWordListener } from './wake-word';

vi.mock('./audio', () => ({ recordingToWav: vi.fn() }));
const permissionApi = vi.hoisted(() => ({ native: false, call: vi.fn() }));
vi.mock('./api', () => permissionApi);
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
  let contexts: FakeAudioContext[];

  class FakeAudioContext {
    state = 'running';
    destination = {};
    onstatechange: (() => void) | null = null;
    resume = vi.fn(async () => { this.state = 'running'; });
    close = vi.fn(async () => { this.state = 'closed'; });
    constructor() { contexts.push(this); }
    createMediaStreamSource = () => ({ connect: vi.fn(), disconnect: vi.fn() });
    createGain = () => ({ gain: { value: 1 }, connect: vi.fn(), disconnect: vi.fn() });
    createAnalyser = () => ({
      fftSize: 1024, smoothingTimeConstant: 0, connect: vi.fn(), disconnect: vi.fn(),
      getFloatTimeDomainData: (samples: Float32Array) => samples.fill(volume),
    });
  }

  class FakeMediaRecorder {
    static isTypeSupported = () => true;
    state = 'inactive';
    mimeType = 'audio/webm';
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;
    onerror: ((event: { error?: Error }) => void) | null = null;
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
    systemPermission.cancel();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'performance'] });
    volume = 0;
    permissionApi.native = false;
    permissionApi.call.mockReset();
    listeners = [];
    createdRecorders = [];
    contexts = [];
    track = { enabled: true, readyState: 'live', onended: null, stop: vi.fn(() => { track.readyState = 'ended'; }) };
    stream = { getTracks: () => [track], getAudioTracks: () => [track] };
    getUserMedia = vi.fn().mockResolvedValue(stream);
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia } });
    vi.stubGlobal('MediaRecorder', FakeMediaRecorder);
    vi.stubGlobal('AudioContext', FakeAudioContext);
    vi.mocked(recordingToWav).mockReset().mockResolvedValue(wav);
  });

  afterEach(() => {
    listeners.forEach((listener) => listener.stop());
    systemPermission.cancel();
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

  it('öffnet bei verweigerter nativer macOS-Freigabe kein Gerät und wiederholt die Anfrage nicht', async () => {
    permissionApi.native = true;
    permissionApi.call.mockResolvedValue({ platform: 'macos', status: 'denied' });
    const { listener, onState } = setup();
    const started = listener.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(systemPermission.active).toBe(true);
    listener.stop(); await started;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(getUserMedia).not.toHaveBeenCalled();
    expect(permissionApi.call).toHaveBeenCalledExactlyOnceWith('get_microphone_permission');
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(contexts[0].state).toBe('closed');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('verwirft eine native Freigabe nach Stop ohne WebView-Aufnahme oder Neustart', async () => {
    permissionApi.native = true;
    const permission = deferred<{ platform: string; status: string }>();
    permissionApi.call.mockReturnValue(permission.promise);
    const { listener, onState } = setup();
    const started = listener.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(permissionApi.call).toHaveBeenCalledOnce();
    listener.stop();
    permission.resolve({ platform: 'macos', status: 'authorized' });
    await started;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(getUserMedia).not.toHaveBeenCalled();
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('verwirft die Startabsicht bei einer geänderten STT-Zielkonfiguration im Berechtigungsdialog', async () => {
    permissionApi.native = true;
    permissionApi.call.mockResolvedValue({ platform: 'macos', status: 'denied' });
    let destination = 'local-whisper'; const expected = destination;
    const { listener, onState } = setup();
    const started = listener.start(undefined, () => destination === expected);
    await vi.advanceTimersByTimeAsync(0);
    expect(systemPermission.active).toBe(true);
    destination = 'cloud-whisper'; systemPermission.invalidate();
    await started;
    expect(getUserMedia).not.toHaveBeenCalled();
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(systemPermission.active).toBe(false);
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

  it('hört nach einem vorübergehenden STT-Fehler automatisch weiter und erkennt die nächste Frage', async () => {
    const transcribe = vi.fn().mockRejectedValueOnce(new Error('Whisper-Server nicht erreichbar')).mockResolvedValue('hey nova, zweite Frage');
    const { listener, onTrigger, onError, onState } = setup(transcribe);
    await start(listener);
    await utterance();
    expect(onError).toHaveBeenCalledWith(expect.stringContaining('Whisper-Server nicht erreichbar'));
    expect(onState).toHaveBeenLastCalledWith('retrying');
    expect(track.stop).not.toHaveBeenCalled();
    expect(track.enabled).toBe(false);
    expect(onTrigger).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    expect(onState).toHaveBeenLastCalledWith('listening');
    await utterance();
    expect(onTrigger).toHaveBeenCalledExactlyOnceWith('zweite Frage');
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it.each([false, true])('behält die Startabsicht bei einer Pause im Berechtigungsdialog (früh entpausiert: %s)', async (earlyUnblock) => {
    const permission = deferred<typeof stream>();
    getUserMedia.mockReturnValueOnce(permission.promise);
    const { listener, onState, onError } = setup();
    const starting = listener.start();
    await vi.advanceTimersByTimeAsync(0);
    listener.setBlocked(true);
    if (earlyUnblock) listener.setBlocked(false);
    permission.resolve(stream);
    await starting;
    await vi.advanceTimersByTimeAsync(0);
    if (!earlyUnblock) {
      expect(onState).toHaveBeenLastCalledWith('paused');
      expect(track.enabled).toBe(false);
      expect(createdRecorders).toHaveLength(0);
      listener.setBlocked(false);
      await vi.advanceTimersByTimeAsync(650);
    }
    expect(onState).toHaveBeenLastCalledWith('listening');
    expect(track.enabled).toBe(true);
    expect(track.stop).not.toHaveBeenCalled();
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    expect(onError).not.toHaveBeenCalled();
  });

  it('behält die Sitzung bei einer Pause während des anfänglichen Profilspeicherns', async () => {
    const save = deferred<boolean>();
    const { listener, onState } = setup();
    const starting = listener.start(() => save.promise);
    listener.setBlocked(true);
    save.resolve(true);
    await starting;
    expect(onState).toHaveBeenLastCalledWith('paused');
    listener.setBlocked(false);
    await vi.advanceTimersByTimeAsync(650);
    expect(onState).toHaveBeenLastCalledWith('listening');
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it('verwirft eine Transkription aus der Pause, ohne die wiederaufgenommene Sitzung zu beenden', async () => {
    const oldResult = deferred<string>();
    const transcribe = vi.fn().mockReturnValueOnce(oldResult.promise).mockResolvedValue('Hey Nova, aktuelle Frage');
    const { listener, onTrigger, onState } = setup(transcribe);
    await start(listener);
    await utterance();
    listener.setBlocked(true);
    listener.setBlocked(false);
    await vi.advanceTimersByTimeAsync(650);
    oldResult.resolve('Hey Nova, verspätete Frage');
    await vi.advanceTimersByTimeAsync(0);
    expect(onTrigger).not.toHaveBeenCalled();
    expect(onState).toHaveBeenLastCalledWith('listening');
    await utterance();
    expect(onTrigger).toHaveBeenCalledExactlyOnceWith('aktuelle Frage');
  });

  it('nimmt nach einer durch die eigene Antwort ausgelösten Pause wieder auf', async () => {
    const reply = deferred<void>();
    const { listener, onTrigger, onState } = setup();
    onTrigger.mockImplementation(async () => { listener.setBlocked(true); await reply.promise; });
    await start(listener);
    await utterance();
    expect(onState).toHaveBeenLastCalledWith('paused');
    reply.resolve();
    await vi.advanceTimersByTimeAsync(0);
    listener.setBlocked(false);
    await vi.advanceTimersByTimeAsync(650);
    expect(onState).toHaveBeenLastCalledWith('listening');
    expect(track.enabled).toBe(true);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it('wiederholt einen vorübergehend fehlgeschlagenen Mikrofonstart ohne neuen Benutzerstart', async () => {
    getUserMedia.mockRejectedValueOnce(new DOMException('Gerät vorübergehend belegt', 'NotReadableError'));
    const { listener, onState, onError } = setup();
    await listener.start();
    expect(onState).toHaveBeenLastCalledWith('retrying');
    expect(onError).toHaveBeenCalledWith(expect.stringContaining('Gerät vorübergehend belegt'));
    await vi.advanceTimersByTimeAsync(999);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(getUserMedia).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(onState).toHaveBeenLastCalledWith('listening');
  });

  it('startet nach einem Recorder-Fehler eine neue Aufnahme statt die Sitzung zu beenden', async () => {
    const { listener, onState, transcribe } = setup();
    await start(listener);
    createdRecorders[0].onerror?.({ error: new Error('Container unterbrochen') });
    expect(onState).toHaveBeenLastCalledWith('retrying');
    expect(track.enabled).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(createdRecorders).toHaveLength(2);
    expect(onState).toHaveBeenLastCalledWith('listening');
    expect(track.stop).not.toHaveBeenCalled();
    expect(transcribe).not.toHaveBeenCalled();
  });

  it('holt auch einen ohne Fehlerereignis unerwartet beendeten Recorder zurück', async () => {
    const { listener, onState, transcribe } = setup();
    await start(listener);
    createdRecorders[0].stop();
    await vi.advanceTimersByTimeAsync(0);
    expect(onState).toHaveBeenLastCalledWith('retrying');
    expect(transcribe).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    expect(onState).toHaveBeenLastCalledWith('listening');
    expect(createdRecorders).toHaveLength(2);
    expect(track.stop).not.toHaveBeenCalled();
  });

  it('beendet das Zuhören nicht nach einer defekten Aufnahme oder abgelehnten Cohost-Anfrage', async () => {
    vi.mocked(recordingToWav).mockRejectedValueOnce(new Error('Aufnahme unvollständig'));
    const { listener, onState, onTrigger } = setup();
    await start(listener);
    await utterance();
    expect(onState).toHaveBeenLastCalledWith('retrying');
    await vi.advanceTimersByTimeAsync(1000);
    onTrigger.mockRejectedValueOnce(new Error('Cohost kurz beschäftigt'));
    await utterance();
    expect(onState).toHaveBeenLastCalledWith('retrying');
    await vi.advanceTimersByTimeAsync(1000);
    await utterance();
    expect(onTrigger).toHaveBeenCalledTimes(2);
    expect(track.stop).not.toHaveBeenCalled();
  });

  it('erhöht Fehlerabstände und verlangt nach fünf erfolglosen Wiederaufnahmen einen neuen Start', async () => {
    const transcribe = vi.fn().mockRejectedValue(new Error('STT weiterhin offline'));
    const { listener, onState, onError } = setup(transcribe);
    await start(listener);
    for (const delay of [1000, 2000, 4000, 8000, 16_000]) {
      await utterance();
      expect(onState).toHaveBeenLastCalledWith('retrying');
      const count = createdRecorders.length;
      // utterance() spends 50 ms after its final voiced/silence polling edge.
      await vi.advanceTimersByTimeAsync(delay - 51);
      expect(createdRecorders).toHaveLength(count);
      await vi.advanceTimersByTimeAsync(51);
      expect(onState).toHaveBeenLastCalledWith('listening');
    }
    await utterance();
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(onError).toHaveBeenLastCalledWith(expect.stringContaining('Nach fünf erfolglosen Wiederaufnahmen'));
    expect(track.stop).toHaveBeenCalledTimes(1);
    const count = createdRecorders.length;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(createdRecorders).toHaveLength(count);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it.each(['retrying', 'paused'])('übergeht einen ausdrücklichen Stop während %s niemals automatisch', async (phase) => {
    const { listener, onState } = setup(vi.fn().mockRejectedValue(new Error('offline')));
    await start(listener);
    if (phase === 'retrying') await utterance();
    else listener.setBlocked(true);
    listener.stop();
    const count = createdRecorders.length;
    listener.setBlocked(false);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(createdRecorders).toHaveLength(count);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('beachtet nach Entpausieren auch einen noch laufenden Fehlerabstand', async () => {
    const { listener, onState } = setup(vi.fn().mockRejectedValue(new Error('offline')));
    await start(listener);
    await utterance();
    await vi.advanceTimersByTimeAsync(1000);
    await utterance();
    listener.setBlocked(true);
    listener.setBlocked(false);
    await vi.advanceTimersByTimeAsync(650);
    expect(onState).toHaveBeenLastCalledWith('retrying');
    expect(track.enabled).toBe(false);
    await vi.advanceTimersByTimeAsync(1350);
    expect(onState).toHaveBeenLastCalledWith('listening');
  });

  it.each([
    ['NotAllowedError', 'Mikrofonzugriff wurde vom System oder von der WebView abgelehnt'],
    ['NotFoundError', 'Kein passendes Mikrofon verfügbar'],
  ])('fordert bei %s eine konkrete Benutzeraktion statt endlos erneut anzufragen', async (name, message) => {
    getUserMedia.mockRejectedValue(new DOMException('Browserfehler', name));
    const { listener, onState } = setup();
    await expect(listener.start()).rejects.toThrow(message);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('meldet ein entferntes Mikrofon mit neuer Startaktion und öffnet kein anderes Gerät heimlich', async () => {
    const { listener, onError, onState } = setup();
    await start(listener);
    track.readyState = 'ended';
    track.onended?.();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(onError).toHaveBeenCalledWith(expect.stringContaining('Mikrofon wurde getrennt'));
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it('aktiviert einen vorübergehend vom System pausierten Audiokontext wieder', async () => {
    const { listener, onState } = setup();
    await start(listener);
    const context = contexts[0];
    context.state = 'suspended';
    context.onstatechange?.();
    expect(onState).toHaveBeenLastCalledWith('retrying');
    await vi.advanceTimersByTimeAsync(1000);
    expect(context.resume).toHaveBeenCalledTimes(2);
    expect(onState).toHaveBeenLastCalledWith('listening');
    expect(getUserMedia).toHaveBeenCalledTimes(1);
  });

  it('verlangt bei einer vom Browser blockierten Audioaktivierung einen erneuten Klick', async () => {
    vi.stubGlobal('AudioContext', class extends FakeAudioContext {
      constructor() { super(); this.state = 'suspended'; this.resume = vi.fn(() => new Promise<void>(() => {})); }
    });
    const { listener, onState } = setup();
    const result = expect(listener.start()).rejects.toThrow('erneut auf „Zuhören starten“');
    await vi.advanceTimersByTimeAsync(5000);
    await result;
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(getUserMedia).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('bricht auch eine noch nicht aktivierte Audioverarbeitung bei Stop ohne späten Timer ab', async () => {
    vi.stubGlobal('AudioContext', class extends FakeAudioContext {
      constructor() { super(); this.state = 'suspended'; this.resume = vi.fn(() => new Promise<void>(() => {})); }
    });
    const { listener, onError, onState } = setup();
    const starting = listener.start();
    listener.stop();
    await starting;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(onState).toHaveBeenLastCalledWith('stopped');
    expect(onError).not.toHaveBeenCalled();
    expect(getUserMedia).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('lässt einen alten Botstart den Mikrofonstream einer neuen Sitzung nicht schließen', async () => {
    const oldPermission = deferred<typeof stream>();
    const oldTrack = { ...track, stop: vi.fn() };
    const oldStream = { getTracks: () => [oldTrack], getAudioTracks: () => [oldTrack] };
    getUserMedia.mockReturnValueOnce(oldPermission.promise);
    const old = setup();
    const starting = old.listener.start();
    await vi.advanceTimersByTimeAsync(0);
    old.listener.stop();
    const current = setup();
    await start(current.listener);
    oldPermission.resolve(oldStream);
    await starting;
    expect(oldTrack.stop).toHaveBeenCalledTimes(1);
    expect(track.stop).not.toHaveBeenCalled();
    expect(current.onState).toHaveBeenLastCalledWith('listening');
  });
});
