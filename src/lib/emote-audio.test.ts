import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EmoteAudioPlayer } from './emote-audio';

const SOUND = 'custom:00000000-0000-4000-8000-000000000001.wav';
const audio = { base64: btoa('fixture'), mimeType: 'audio/wav' };
class Param { value = 0; setValueAtTime = vi.fn(); exponentialRampToValueAtTime = vi.fn(); }
class Node { connect = vi.fn(); disconnect = vi.fn(); }
class Gain extends Node { gain = new Param(); }
class Source extends Node { onended: (() => void) | null = null; buffer?: Buffer; start = vi.fn(); stop = vi.fn(); frequency = new Param(); type = ''; }
class Buffer {
  constructor(public numberOfChannels = 1, public length = 24000, public sampleRate = 48000) {}
  get duration() { return this.length / this.sampleRate; }
  getChannelData() { return new Float32Array(this.length); }
  copyToChannel = vi.fn();
}
class Context {
  static instances: Context[] = [];
  state = 'running'; currentTime = 10; sampleRate = 48000; destination = new Node();
  sources: Source[] = []; oscillators: Source[] = []; gains: Gain[] = [];
  decodeAudioData = vi.fn(async () => new Buffer()); resume = vi.fn(async () => { this.state = 'running'; }); close = vi.fn(async () => { this.state = 'closed'; });
  constructor() { Context.instances.push(this); }
  createGain() { const node = new Gain(); this.gains.push(node); return node; }
  createBufferSource() { const node = new Source(); this.sources.push(node); return node; }
  createOscillator() { const node = new Source(); this.oscillators.push(node); return node; }
  createBuffer(channels: number, length: number, sampleRate: number) { return new Buffer(channels, length, sampleRate); }
  createBiquadFilter() { return Object.assign(new Node(), { type: '', frequency: new Param(), Q: new Param() }); }
}

describe('optional emote audio', () => {
  beforeEach(() => { Context.instances = []; vi.stubGlobal('AudioContext', Context); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('creates no audio context and performs no fetch when no sound is selected', async () => {
    const resolver = vi.fn(); const player = new EmoteAudioPlayer();
    await player.prepare([{}], resolver); player.play({});
    expect(Context.instances).toHaveLength(0); expect(resolver).not.toHaveBeenCalled();
  });

  it('decodes before presentation, starts immediately with its own volume and stops all sources', async () => {
    const playing = vi.fn(); const player = new EmoteAudioPlayer(undefined, playing); const resolver = vi.fn(async () => audio);
    await player.prepare([{ sound: SOUND }], resolver);
    const context = Context.instances[0]; expect(context.sources).toHaveLength(0); expect(playing).not.toHaveBeenCalled();
    player.play({ sound: SOUND, soundVolume: 0.25 });
    expect(context.sources[0].start).toHaveBeenCalledWith(10, 0, 0.5);
    expect(context.gains[1].gain.setValueAtTime).toHaveBeenCalledWith(0.25, 10);
    expect(playing).toHaveBeenLastCalledWith(true);
    player.stop(); expect(context.sources[0].stop).toHaveBeenCalled(); expect(playing).toHaveBeenLastCalledWith(false);
    player.destroy();
  });

  it('discards a late fetch after stop and never decodes or plays it', async () => {
    let finish!: (value: typeof audio) => void;
    const player = new EmoteAudioPlayer(); const preparing = player.prepare([{ sound: SOUND }], () => new Promise(resolve => { finish = resolve; }));
    player.stop(); finish(audio); await preparing;
    expect(Context.instances[0].decodeAudioData).not.toHaveBeenCalled(); expect(Context.instances[0].sources).toHaveLength(0);
    player.destroy();
  });

  it('discards a late decode after destroy and keeps cache scoped to each preparation', async () => {
    const player = new EmoteAudioPlayer(); await player.unlock(); const context = Context.instances[0];
    let finish!: (value: Buffer) => void;
    context.decodeAudioData.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const preparing = player.prepare([{ sound: SOUND }], async () => audio);
    await Promise.resolve(); player.destroy(); finish(new Buffer()); await preparing;
    expect(context.sources).toHaveLength(0); expect(context.close).toHaveBeenCalled();
    const scoped = new EmoteAudioPlayer(); const resolver = vi.fn(async () => audio);
    await scoped.prepare([{ sound: SOUND }], resolver); await scoped.prepare([{ sound: SOUND }], resolver);
    expect(resolver).toHaveBeenCalledTimes(2); scoped.destroy();
  });

  it('clips long decoded effects to fifteen seconds and clamps volume', async () => {
    const player = new EmoteAudioPlayer(); await player.unlock(); const context = Context.instances[0];
    context.decodeAudioData.mockResolvedValueOnce(new Buffer(1, 48000 * 30));
    await player.prepare([{ sound: SOUND }], async () => audio); player.play({ sound: SOUND, soundVolume: 8 });
    expect(context.sources[0].buffer?.duration).toBe(15); expect(context.sources[0].start).toHaveBeenCalledWith(10, 0, 15);
    expect(context.gains[1].gain.setValueAtTime).toHaveBeenCalledWith(1, 10); player.destroy();
  });

  it('synthesizes only explicitly selected builtins and counts simultaneous tones until the last ends', async () => {
    const playing = vi.fn(); const player = new EmoteAudioPlayer(undefined, playing); const resolver = vi.fn();
    await player.prepare([{ sound: 'builtin:wave' }], resolver); player.play({ sound: 'builtin:wave' });
    const context = Context.instances[0]; expect(resolver).not.toHaveBeenCalled(); expect(context.oscillators).toHaveLength(2);
    context.oscillators[0].onended?.(); expect(playing).toHaveBeenLastCalledWith(true);
    context.oscillators[1].onended?.(); expect(playing).toHaveBeenLastCalledWith(false); player.destroy();
  });

  it('reports unavailable effects without rejecting presentation preparation or queuing locked playback', async () => {
    const warning = vi.fn(); const player = new EmoteAudioPlayer(warning);
    await expect(player.prepare([{ sound: SOUND }], async () => { throw new Error('unavailable'); })).resolves.toBeUndefined();
    expect(warning).toHaveBeenCalled(); const context = Context.instances[0]; context.state = 'suspended'; player.play({ sound: 'builtin:cheer' });
    expect(context.oscillators).toHaveLength(0); expect(context.resume).not.toHaveBeenCalled(); player.destroy();
  });
});
