import { EmoteAudioPlayer } from './emote-audio';
import type { SoundActivity } from './sound-playback';

export const MODEL_AUDIO_EXTENSION = 'MSFT_audio_emitter';
export const MODEL_AUDIO_PROFILE = 'orbitpal-msft-audio-v1';
const MAX_CLIP_BYTES = 5 * 1024 * 1024;
const UNSUPPORTED = 'Modell-Audio verwendet Funktionen oder eine Version außerhalb des unterstützten MSFT-Draft-Profils.';
const REFERENCE = 'Modell-Audio enthält ungültige Clip-, Puffer-, Emitter- oder Animationsreferenzen.';
const RESOURCE = 'Modell-Audio muss als WAV-Daten-URI oder eingebetteter bufferView vorliegen; externe Ressourcen sind nicht erlaubt.';
const BUDGET = 'Modell-Audio überschreitet das Profilbudget: 8 Clips, 4 Emitter, 128 Ereignisse, 5 MiB pro Clip und 16 MiB insgesamt.';
type ObjectValue = Record<string, any>;
type Clip = { uri?: string; view?: number; bytes: number };
type Emitter = { clip: number; volume: number };
type AudioEvent = { time: number; emitter: number; action: 'play' | 'stop'; offset: number };
export type ModelAudioPlan = { clips: Clip[]; emitters: Emitter[]; animations: AudioEvent[][] };
export type AudioPresentation = { token: string; animation: number; time: number; rate: number; loop: boolean; activity?: SoundActivity };
const object = (value: unknown): value is ObjectValue => Boolean(value && typeof value === 'object' && !Array.isArray(value));
function fields(value: unknown, allowed: string[]): asserts value is ObjectValue {
  if (!object(value)) throw new Error(REFERENCE);
  if (Object.keys(value).some(key => !allowed.includes(key))) throw new Error(UNSUPPORTED);
}
function index(value: unknown, length: number): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0 || (value as number) >= length) throw new Error(REFERENCE);
  return value as number;
}
function finite(value: unknown, fallback: number, maximum: number) {
  const number = value === undefined || value === null ? fallback : value;
  if (typeof number !== 'number' || !Number.isFinite(number) || number < 0 || number > maximum) throw new Error(REFERENCE);
  return number;
}
function audioUri(uri: string): string {
  const prefix = 'data:audio/wav;base64,';
  if (!uri.startsWith(prefix)) throw new Error(RESOURCE);
  const encoded = uri.slice(prefix.length);
  if (encoded.length > Math.ceil(MAX_CLIP_BYTES / 3) * 4) throw new Error(BUDGET);
  try {
    const binary = atob(encoded);
    if (btoa(binary) !== encoded) throw new Error(RESOURCE);
    return binary;
  } catch { throw new Error(RESOURCE); }
}

/** Parse only the pinned MSFT draft subset; never calls a loader or fetch. */
export function modelAudioPlan(document: ObjectValue): ModelAudioPlan | undefined {
  const extension = document.extensions?.[MODEL_AUDIO_EXTENSION];
  if (extension === undefined) return;
  fields(extension, ['clips', 'emitters', 'extras']);
  if (!Array.isArray(extension.clips) || !Array.isArray(extension.emitters)) throw new Error(REFERENCE);
  if (!extension.clips.length || extension.clips.length > 8 || !extension.emitters.length || extension.emitters.length > 4) throw new Error(BUDGET);
  let total = 0;
  const clips: Clip[] = extension.clips.map((clip: unknown) => {
    fields(clip, ['uri', 'bufferView', 'mimeType', 'extras']);
    if (clip.mimeType !== undefined && clip.mimeType !== null && clip.mimeType !== 'audio/wav') throw new Error(RESOURCE);
    let result: Clip;
    if (typeof clip.uri === 'string') {
      if (clip.bufferView !== undefined && clip.bufferView !== null) throw new Error(REFERENCE);
      result = { uri: clip.uri, bytes: audioUri(clip.uri).length };
    } else {
      if (clip.uri !== undefined && clip.uri !== null || clip.mimeType !== 'audio/wav') throw new Error(RESOURCE);
      const views = Array.isArray(document.bufferViews) ? document.bufferViews : [];
      const buffers = Array.isArray(document.buffers) ? document.buffers : [];
      const viewIndex = index(clip.bufferView, views.length); const view = views[viewIndex];
      if (!object(view)) throw new Error(REFERENCE);
      if (view.extensions !== undefined) throw new Error(UNSUPPORTED);
      const bufferIndex = index(view.buffer, buffers.length); const buffer = buffers[bufferIndex];
      if (!object(buffer) || !Number.isSafeInteger(buffer.byteLength) || buffer.byteLength < 0) throw new Error(REFERENCE);
      if (buffer.uri !== undefined && (typeof buffer.uri !== 'string' || !/^data:application\/(?:octet-stream|gltf-buffer);base64,/.test(buffer.uri))) throw new Error(RESOURCE);
      if (buffer.uri === undefined && bufferIndex !== 0) throw new Error(RESOURCE);
      const offset = view.byteOffset ?? 0;
      if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(view.byteLength) || view.byteLength < 0
        || !Number.isSafeInteger(offset + view.byteLength) || offset + view.byteLength > buffer.byteLength) throw new Error(REFERENCE);
      result = { view: viewIndex, bytes: view.byteLength };
    }
    if (!result.bytes || result.bytes > MAX_CLIP_BYTES || (total += result.bytes) > 16 * 1024 * 1024) throw new Error(BUDGET);
    return result;
  });
  const emitters: Emitter[] = extension.emitters.map((emitter: unknown) => {
    fields(emitter, ['name', 'clips', 'volume', 'loop', 'extras']);
    if (emitter.loop !== undefined && emitter.loop !== null && emitter.loop !== false) throw new Error(UNSUPPORTED);
    if (!Array.isArray(emitter.clips) || emitter.clips.length !== 1) throw new Error(UNSUPPORTED);
    fields(emitter.clips[0], ['clip', 'weight']);
    if (finite(emitter.clips[0].weight, 1, 1) !== 1) throw new Error(UNSUPPORTED);
    return { clip: index(emitter.clips[0].clip, clips.length), volume: finite(emitter.volume, 1, 1) };
  });
  for (const kind of ['nodes', 'scenes']) {
    if (Array.isArray(document[kind]) && document[kind].some((item: any) => item?.extensions?.[MODEL_AUDIO_EXTENSION] !== undefined)) throw new Error(UNSUPPORTED);
  }
  let events = 0;
  const animations = (Array.isArray(document.animations) ? document.animations : []).map((animation: any): AudioEvent[] => {
    const extension = animation?.extensions?.[MODEL_AUDIO_EXTENSION];
    if (extension === undefined) return [];
    fields(extension, ['events', 'extras']);
    if (!Array.isArray(extension.events)) throw new Error(REFERENCE);
    if ((events += extension.events.length) > 128) throw new Error(BUDGET);
    let previous = -1;
    return extension.events.map((event: unknown) => {
      fields(event, ['time', 'action', 'emitter', 'startOffset', 'extras']);
      if (event.action !== 'play' && event.action !== 'stop') throw new Error(UNSUPPORTED);
      if (typeof event.time !== 'number' || !Number.isFinite(event.time) || event.time < previous || event.time < 0 || event.time > 120) throw new Error(REFERENCE);
      previous = event.time;
      return { time: event.time, action: event.action, emitter: index(event.emitter, emitters.length), offset: finite(event.startOffset, 0, 15) };
    });
  });
  if (!events) throw new Error(UNSUPPORTED);
  return { clips, emitters, animations };
}

/** PCM16 mono/stereo WAV only: bound decoded memory before WebAudio sees bytes. */
export function validateModelWav(bytes: Uint8Array): void {
  const fail = () => { throw new Error('Modell-Audio benötigt PCM16-WAV, mono oder stereo, 8–48 kHz und höchstens 15 Sekunden.'); };
  if (bytes.byteLength < 44 || bytes.byteLength > MAX_CLIP_BYTES) fail();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const word = (at: number) => String.fromCharCode(...bytes.subarray(at, at + 4));
  if (word(0) !== 'RIFF' || word(8) !== 'WAVE' || view.getUint32(4, true) + 8 !== bytes.length) fail();
  let at = 12; let frameSize = 0; let sampleRate = 0; let dataBytes = 0;
  while (at + 8 <= bytes.length) {
    const size = view.getUint32(at + 4, true); const start = at + 8;
    if (start + size > bytes.length) fail();
    if (word(at) === 'fmt ') {
      if (frameSize || size !== 16 || view.getUint16(start, true) !== 1 || view.getUint16(start + 14, true) !== 16) fail();
      const channels = view.getUint16(start + 2, true); sampleRate = view.getUint32(start + 4, true); frameSize = channels * 2;
      if (channels < 1 || channels > 2 || sampleRate < 8000 || sampleRate > 48000
        || view.getUint16(start + 12, true) !== frameSize || view.getUint32(start + 8, true) !== sampleRate * frameSize) fail();
    }
    if (word(at) === 'data') { if (dataBytes || !size) fail(); dataBytes = size; }
    at = start + size + size % 2;
  }
  if (at !== bytes.length || !frameSize || !dataBytes || dataBytes % frameSize || dataBytes / frameSize > sampleRate * 15) fail();
}

type AudioSink = Pick<EmoteAudioPlayer, 'prepare' | 'play' | 'stopSound' | 'halt' | 'stop' | 'unlock' | 'destroy' | 'setVolume' | 'isPrepared'>;
export class ModelAudioController {
  private plan?: ModelAudioPlan;
  private resolveView?: (index: number) => Promise<ArrayBuffer>;
  private generation = 0;
  private prepared = false;
  private preparing?: Promise<void>;
  private enabled = false;
  private activated = false;
  private activationNotice = false;
  private token = '';
  private blockedToken = '';
  private time = 0;
  private activity: SoundActivity = 'foreground';
  private player: AudioSink;
  constructor(private warn: (message: string) => void, onPlaying?: (playing: boolean, activity: SoundActivity) => void, player?: AudioSink) {
    this.player = player ?? new EmoteAudioPlayer(warn, playing => onPlaying?.(playing, this.activity), true);
  }
  async load(document: ObjectValue, resolveView: (index: number) => Promise<ArrayBuffer>) {
    this.clear();
    try { this.plan = modelAudioPlan(document); }
    catch (error) {
      if (document.extensionsRequired?.includes(MODEL_AUDIO_EXTENSION)) throw error;
      this.warn(error instanceof Error ? error.message : UNSUPPORTED); return;
    }
    this.resolveView = resolveView;
    if (this.plan && this.enabled) {
      try { await this.prepare(); }
      catch (error) {
        if (document.extensionsRequired?.includes(MODEL_AUDIO_EXTENSION)) throw error;
        this.plan = undefined; this.warn(error instanceof Error ? error.message : RESOURCE);
      }
    }
  }
  private prepare(): Promise<void> {
    if (this.preparing) return this.preparing;
    const pending = this.runPrepare(); this.preparing = pending;
    void pending.finally(() => { if (this.preparing === pending) this.preparing = undefined; }).catch(() => {});
    return pending;
  }
  private async runPrepare() {
    const plan = this.plan; const resolveView = this.resolveView; if (!plan || !resolveView) return;
    const generation = ++this.generation;
    const references = [...new Set(plan.emitters.map(emitter => emitter.clip))];
    await this.player.prepare(references.map(clip => ({ sound: `model:${clip}` })), async id => {
      const clip = plan.clips[Number(id.slice(6))];
      const bytes = clip.uri ? Uint8Array.from(audioUri(clip.uri), char => char.charCodeAt(0)) : new Uint8Array(await resolveView(clip.view!));
      if (generation !== this.generation) throw new Error('cancelled');
      if (bytes.byteLength !== clip.bytes) throw new Error(RESOURCE);
      validateModelWav(bytes);
      let binary = '';
      for (let at = 0; at < bytes.length; at += 8192) binary += String.fromCharCode(...bytes.subarray(at, at + 8192));
      return { base64: btoa(binary), mimeType: 'audio/wav' };
    });
    if (generation === this.generation) {
      if (!references.every(index => this.player.isPrepared(`model:${index}`))) throw new Error('Modell-Audio konnte nicht rechtzeitig als begrenztes PCM16-WAV vorbereitet werden.');
      this.prepared = true;
    }
  }
  setEnabled(value: boolean) {
    if (this.enabled === value) return;
    this.stop(); this.enabled = value;
    if (value && this.plan && !this.prepared) void this.prepare().catch(() => this.warn(RESOURCE));
  }
  async unlock() {
    if (!this.enabled || !this.plan) return;
    const generation = this.generation;
    await this.player.unlock();
    if (!this.enabled || generation !== this.generation) return;
    this.activated = true;
    this.activationNotice = false; this.warn('');
    if (!this.prepared) await this.prepare();
  }
  /** Called only after a successful visible render, never merely on model load. */
  present(presentation: AudioPresentation | undefined, volume: number) {
    if (!presentation || !this.enabled) {
      if (this.token) this.player.halt();
      this.token = ''; this.blockedToken = ''; return;
    }
    if (!this.plan || !Number.isFinite(presentation.time) || presentation.time < 0) return;
    const { token, time, rate, loop } = presentation;
    const activity = presentation.activity === 'ambient' ? 'ambient' : 'foreground';
    const events = this.plan.animations[presentation.animation] ?? [];
    let previous = this.time;
    if (token !== this.token || activity !== this.activity) {
      this.player.halt(); this.token = token;
      this.activity = activity;
      // Joining a clip midway must not replay already elapsed sound events.
      previous = time <= 0.25 ? -Number.EPSILON : time;
    } else if (time < previous) {
      this.player.halt();
      previous = loop ? -Number.EPSILON : time;
    }
    this.time = time;
    if (!this.activated && !this.activationNotice && events.some(event => event.action === 'play' && event.time > previous && event.time <= time)) {
      this.activationNotice = true;
      this.warn('Modell-Sounds: Klicke in diese Ausgabe, um Audio für künftige Animationen zu aktivieren.');
    }
    if (token === this.blockedToken || !this.prepared || !this.activated) return;
    if (rate < 0.25 || rate > 4 || !Number.isFinite(rate)) { this.player.halt(); return; }
    const master = Number.isFinite(volume) ? Math.min(1, Math.max(0, volume)) : 0;
    this.player.setVolume(master);
    if (!master) this.player.halt();
    for (const event of events) {
      if (event.time <= previous || event.time > time) continue;
      const id = `model:${event.emitter}`;
      this.player.stopSound(id);
      // A stalled renderer skips stale play events instead of producing a late burst.
      if (event.action === 'play' && (time - event.time) / rate <= 0.25 && master > 0) {
        this.player.play({ sound: id, bufferSound: `model:${this.plan.emitters[event.emitter].clip}`, soundVolume: this.plan.emitters[event.emitter].volume,
          soundOffset: event.offset + time - event.time, soundRate: rate });
      }
    }
  }
  stop() { this.generation++; this.preparing = undefined; this.blockedToken = this.token; this.player.halt(); }
  clear() { this.stop(); this.player.stop(); this.plan = undefined; this.resolveView = undefined; this.prepared = false; this.token = ''; this.blockedToken = ''; this.activationNotice = false; }
  destroy() { this.clear(); this.player.destroy(); }
}
