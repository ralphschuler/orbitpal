import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SpeechPlayer } from './audio';

const audio = { audioBase64: btoa('fixture'), mimeType: 'audio/wav' };
const voice = { provider: 'elevenlabs', enabled: true };

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

class Node { connect = vi.fn(); disconnect = vi.fn(); }
class Source extends Node {
  buffer?: object;
  playbackRate = { value: 1 };
  onended: (() => void) | null = null;
  start = vi.fn();
  stop = vi.fn();
}
class Analyser extends Node {
  fftSize = 0;
  smoothingTimeConstant = 0;
  getFloatTimeDomainData = vi.fn();
}
class Context {
  static instances: Context[] = [];
  state = 'running';
  destination = new Node();
  sources: Source[] = [];
  decodeAudioData = vi.fn(async () => ({}));
  resume = vi.fn(async () => { this.state = 'running'; });
  close = vi.fn(async () => { this.state = 'closed'; });
  constructor() { Context.instances.push(this); }
  createBufferSource() { const source = new Source(); this.sources.push(source); return source; }
  createGain() { return Object.assign(new Node(), { gain: { value: 1 } }); }
  createAnalyser() { return new Analyser(); }
}
class Utterance {
  voice?: object;
  lang = '';
  rate = 1;
  volume = 1;
  pitch = 1;
  onstart: (() => void) | null = null;
  onend: (() => void) | null = null;
  onerror: ((event: { error: string }) => void) | null = null;
  onboundary: (() => void) | null = null;
  constructor(public text: string) {}
}

describe('speech playback presentation gate', () => {
  let players: SpeechPlayer[];
  let synth: {
    getVoices: ReturnType<typeof vi.fn>;
    speak: ReturnType<typeof vi.fn>;
    cancel: ReturnType<typeof vi.fn>;
    resume: ReturnType<typeof vi.fn>;
  };
  function player(onStartState = vi.fn()) {
    const value = new SpeechPlayer(vi.fn(), onStartState);
    players.push(value);
    return value;
  }
  async function flush() { await vi.advanceTimersByTimeAsync(0); }

  beforeEach(() => {
    vi.useFakeTimers();
    players = [];
    Context.instances = [];
    synth = {
      getVoices: vi.fn(() => [{ name: 'Fixture', voiceURI: 'fixture', lang: 'de-DE' }]),
      speak: vi.fn(), cancel: vi.fn(), resume: vi.fn(),
    };
    vi.stubGlobal('window', { speechSynthesis: synth, setTimeout, clearTimeout });
    vi.stubGlobal('AudioContext', Context);
    vi.stubGlobal('SpeechSynthesisUtterance', Utterance);
    vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1));
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
  });
  afterEach(() => {
    players.forEach(value => value.destroy());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('fetches and decodes before waiting, then starts audio and presentation together', async () => {
    const gate = deferred();
    const onStart = vi.fn();
    const state = vi.fn();
    const fetch = vi.fn(async () => audio);
    const beforeStart = vi.fn(() => gate.promise);
    const speaking = player(state).speak('Hallo', voice, fetch, onStart, beforeStart);
    await flush();
    const context = Context.instances[0];
    expect(fetch).toHaveBeenCalledExactlyOnceWith('Hallo');
    expect(context.decodeAudioData).toHaveBeenCalledOnce();
    expect(beforeStart).toHaveBeenCalledOnce();
    expect(context.sources).toHaveLength(0);
    expect(onStart).not.toHaveBeenCalled();
    expect(state).not.toHaveBeenCalledWith(true);
    gate.resolve(); await flush();
    const source = context.sources[0];
    expect(source.start).toHaveBeenCalledOnce();
    expect(onStart).toHaveBeenCalledOnce();
    expect(source.start.mock.invocationCallOrder[0]).toBeLessThan(onStart.mock.invocationCallOrder[0]);
    expect(state).toHaveBeenLastCalledWith(true);
    source.onended?.(); await speaking;
    expect(state).toHaveBeenLastCalledWith(false);
  });

  it.each(['stop', 'destroy'] as const)('%s settles a never-resolving gate and rejects its late start', async action => {
    const gate = deferred();
    const onStart = vi.fn();
    const state = vi.fn();
    const value = player(state);
    const speaking = value.speak('Alt', voice, async () => audio, onStart, () => gate.promise);
    await flush();
    value[action]();
    await expect(speaking).resolves.toBeUndefined();
    expect(Context.instances[0].sources).toHaveLength(0);
    gate.resolve(); await flush();
    expect(Context.instances[0].sources).toHaveLength(0);
    expect(onStart).not.toHaveBeenCalled();
    expect(state).not.toHaveBeenCalledWith(true);
  });

  it('does not let an old gate release the replacement utterance', async () => {
    const oldGate = deferred(); const newGate = deferred();
    const oldStart = vi.fn(); const newStart = vi.fn();
    const value = player();
    const old = value.speak('Alt', voice, async () => audio, oldStart, () => oldGate.promise);
    await flush();
    const newer = value.speak('Neu', voice, async () => audio, newStart, () => newGate.promise);
    await expect(old).resolves.toBeUndefined(); await flush();
    oldGate.resolve(); await flush();
    expect(Context.instances[0].sources).toHaveLength(0);
    expect(oldStart).not.toHaveBeenCalled(); expect(newStart).not.toHaveBeenCalled();
    newGate.resolve(); await flush();
    const sources = Context.instances[0].sources;
    expect(sources).toHaveLength(1); expect(newStart).toHaveBeenCalledOnce();
    sources[0].onended?.(); await newer;
  });

  it('surfaces a current gate failure before starting audio or visible presentation', async () => {
    const failure = new Error('Figure unavailable'); const gate = deferred();
    const onStart = vi.fn(); const state = vi.fn();
    const speaking = player(state).speak('Hallo', voice, async () => audio, onStart, () => gate.promise);
    const rejected = expect(speaking).rejects.toBe(failure);
    await flush(); gate.reject(failure); await rejected;
    expect(Context.instances[0].sources).toHaveLength(0);
    expect(onStart).not.toHaveBeenCalled(); expect(state).not.toHaveBeenCalledWith(true);
  });

  it('absorbs a superseded gate rejection without affecting the newer audio', async () => {
    const gate = deferred(); const value = player();
    const old = value.speak('Alt', voice, async () => audio, vi.fn(), () => gate.promise);
    await flush();
    const newer = value.speak('Neu', voice, async () => audio);
    await old; await flush();
    const source = Context.instances[0].sources[0];
    gate.reject(new Error('Late old gate failure')); await flush();
    expect(source.stop).not.toHaveBeenCalled();
    source.onended?.(); await newer;
  });

  it('waits before browser speech and starts its timeout only after the gate', async () => {
    const gate = deferred(); const onStart = vi.fn(); const state = vi.fn();
    const speaking = player(state).speak('System', { provider: 'system' }, undefined, onStart, () => gate.promise);
    await flush(); await vi.advanceTimersByTimeAsync(30_000);
    expect(synth.getVoices).toHaveBeenCalledOnce();
    expect(synth.speak).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
    expect(onStart).not.toHaveBeenCalled(); expect(state).not.toHaveBeenCalledWith(true);
    gate.resolve(); await flush();
    const utterance = synth.speak.mock.calls[0][0] as Utterance;
    expect(utterance.voice).toMatchObject({ voiceURI: 'fixture' });
    expect(vi.getTimerCount()).toBe(1);
    expect(onStart).not.toHaveBeenCalled();
    utterance.onstart?.(); expect(onStart).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
    utterance.onend?.(); await speaking;
  });

  it('settles stopped browser speech while its gate remains pending', async () => {
    const gate = deferred(); const onStart = vi.fn(); const value = player();
    const speaking = value.speak('System', { provider: 'browser' }, undefined, onStart, () => gate.promise);
    await flush(); value.stop(); await speaking;
    gate.resolve(); await flush();
    expect(synth.speak).not.toHaveBeenCalled(); expect(onStart).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });
});
