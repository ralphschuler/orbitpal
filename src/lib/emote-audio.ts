/** Optional, local emote effects. A failed effect never rejects TTS playback. */
export type EmoteSoundAudio = { base64: string; mimeType: string };
export type SoundEmote = { sound?: string; soundVolume?: number; [key: string]: unknown };
export type EmoteSoundResolver = (soundId: string) => Promise<EmoteSoundAudio>;

const MAX_CLIP_SECONDS = 15;
const MAX_CACHE_BYTES = 16 * 1024 * 1024;
const MAX_SOUNDS = 8;
const MAX_FILE_BYTES = 5 * 1024 * 1024;
const BUILTINS = new Set(['builtin:wave', 'builtin:cheer', 'builtin:whoosh']);
const CUSTOM_ID = /^custom:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(wav|mp3|ogg|webm|m4a)$/;

export class EmoteAudioPlayer {
  private context?: AudioContext;
  private master?: GainNode;
  private buffers = new Map<string, AudioBuffer>();
  private sources = new Map<AudioScheduledSourceNode, AudioNode[]>();
  private generation = 0;
  private disposed = false;
  private playing = false;

  constructor(private onError?: (message: string) => void, private onPlaying?: (playing: boolean) => void) {}

  private warn(message: string) {
    if (!this.disposed) { try { this.onError?.(message); } catch { /* callbacks cannot interrupt voice playback */ } }
  }
  private setPlaying(playing: boolean) {
    if (this.playing === playing) return;
    this.playing = playing;
    try { this.onPlaying?.(playing); } catch { /* independent notification */ }
  }
  private audioContext(): AudioContext | undefined {
    if (this.disposed) return;
    if (!this.context) {
      const globals = globalThis as typeof globalThis & { webkitAudioContext?: typeof AudioContext };
      const Constructor = globals.AudioContext || globals.webkitAudioContext;
      if (!Constructor) { this.warn('Dieser Browser unterstützt keine Emote-Sounds über WebAudio.'); return; }
      try {
        this.context = new Constructor({ latencyHint: 'interactive' });
        this.master = this.context.createGain(); this.master.gain.value = 1;
        this.master.connect(this.context.destination);
      } catch { this.warn('Die Audioausgabe für Emote-Sounds konnte nicht geöffnet werden.'); return; }
    }
    return this.context;
  }

  /** Call from a user gesture. Suspended audio is never queued for a late start. */
  async unlock(): Promise<void> {
    const context = this.audioContext();
    if (!context || context.state === 'running') return;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([context.resume(), new Promise<never>((_, reject) => { timeout = setTimeout(() => reject(new Error('Audiofreigabe')), 3000); })]);
      // resume() may change state after the initial synchronous guard.
      if (!this.disposed && (context.state as AudioContextState) !== 'running') this.warn('Bitte Emote-Sounds durch einen Klick in dieser Ausgabe aktivieren.');
    } catch { this.warn('Emote-Sounds sind gesperrt. Bitte die Audioausgabe durch einen Klick aktivieren.'); }
    finally { if (timeout !== undefined) clearTimeout(timeout); }
  }

  /** Prepare one presentation's sounds. Cache is deliberately scoped to this
   * call/resolver, so matching imported UUIDs cannot leak audio between bots. */
  async prepare(emotes: readonly SoundEmote[], resolver: EmoteSoundResolver): Promise<void> {
    this.stop();
    if (this.disposed) return;
    const generation = this.generation;
    const ids = [...new Set(emotes.map(emote => typeof emote.sound === 'string' ? emote.sound : '').filter(Boolean))];
    if (!ids.length) return;
    const context = this.audioContext(); if (!context) return;
    let cacheBytes = 0;
    for (const id of ids.slice(0, MAX_SOUNDS)) {
      if (this.disposed || generation !== this.generation) return;
      if (BUILTINS.has(id)) continue;
      if (!CUSTOM_ID.test(id)) { this.warn('Ein Emote verweist auf eine unbekannte Sound-ID.'); continue; }
      try {
        const audio = await resolver(id);
        if (this.disposed || generation !== this.generation) return;
        if (typeof audio?.base64 !== 'string' || audio.base64.length > Math.ceil(MAX_FILE_BYTES / 3) * 4 + 4 || !audio.mimeType?.startsWith('audio/')) throw new Error('Audioformat');
        const binary = atob(audio.base64); if (!binary.length || binary.length > MAX_FILE_BYTES) throw new Error('Audiogröße');
        const bytes = new Uint8Array(binary.length); for (let at = 0; at < binary.length; at++) bytes[at] = binary.charCodeAt(at);
        const decoded = await context.decodeAudioData(bytes.buffer);
        if (this.disposed || generation !== this.generation) return;
        const frames = Math.min(decoded.length, Math.floor(decoded.sampleRate * MAX_CLIP_SECONDS));
        const size = frames * decoded.numberOfChannels * 4;
        if (!frames || decoded.numberOfChannels < 1 || decoded.numberOfChannels > 8 || cacheBytes + size > MAX_CACHE_BYTES) {
          this.warn('Ein Emote-Sound überschreitet den begrenzten Audiocache und wird übersprungen.'); continue;
        }
        let buffer = decoded;
        if (frames < decoded.length) {
          buffer = context.createBuffer(decoded.numberOfChannels, frames, decoded.sampleRate);
          for (let channel = 0; channel < decoded.numberOfChannels; channel++) buffer.copyToChannel(decoded.getChannelData(channel).subarray(0, frames), channel);
        }
        cacheBytes += size; this.buffers.set(id, buffer);
      } catch {
        if (!this.disposed && generation === this.generation) this.warn('Ein Emote-Sound konnte nicht geladen oder dekodiert werden und wird übersprungen.');
      }
    }
    if (ids.length > MAX_SOUNDS && !this.disposed && generation === this.generation) this.warn('Pro Antwort werden höchstens acht unterschiedliche Emote-Sounds vorbereitet.');
  }

  private started(source: AudioScheduledSourceNode, cleanup: AudioNode[], start: () => void) {
    this.sources.set(source, cleanup);
    source.onended = () => {
      if (!this.sources.delete(source)) return;
      source.disconnect(); for (const node of cleanup) node.disconnect();
      if (!this.sources.size) this.setPlaying(false);
    };
    try { start(); this.setPlaying(true); }
    catch {
      this.sources.delete(source); source.onended = null;
      try { source.stop(); } catch { /* not started */ }
      source.disconnect(); for (const node of cleanup) node.disconnect();
      if (!this.sources.size) this.setPlaying(false);
      this.warn('Ein Emote-Sound konnte nicht gestartet werden und wird übersprungen.');
    }
  }
  private tone(context: AudioContext, at: number, seconds: number, frequency: number, volume: number) {
    const oscillator = context.createOscillator(); oscillator.type = 'triangle';
    oscillator.frequency.setValueAtTime(frequency, at);
    const gain = context.createGain(); gain.gain.setValueAtTime(0.0001, at);
    gain.gain.exponentialRampToValueAtTime(Math.max(0.0001, volume), at + 0.015);
    gain.gain.exponentialRampToValueAtTime(0.0001, at + seconds);
    oscillator.connect(gain); gain.connect(this.master!);
    this.started(oscillator, [gain], () => { oscillator.start(at); oscillator.stop(at + seconds); });
  }

  /** Synchronous actual start beside the visual gesture. No fetching here. */
  play(emote: SoundEmote): void {
    const id = emote.sound;
    if (this.disposed || typeof id !== 'string' || !id) return;
    if (!BUILTINS.has(id) && !CUSTOM_ID.test(id)) { this.warn('Unbekannter Emote-Sound wird übersprungen.'); return; }
    const volume = typeof emote.soundVolume === 'number' && Number.isFinite(emote.soundVolume) ? Math.min(1, Math.max(0, emote.soundVolume)) : 0.7;
    if (!volume) return;
    const context = this.audioContext(); if (!context) return;
    if (context.state !== 'running') { this.warn('Emote-Sound übersprungen: Bitte Audio vor der nächsten Geste durch einen Klick aktivieren.'); return; }
    try {
      const now = context.currentTime;
      if (id === 'builtin:wave') { this.tone(context, now, 0.16, 660, volume * 0.12); this.tone(context, now + 0.17, 0.22, 880, volume * 0.12); }
      else if (id === 'builtin:cheer') { [523.25, 659.25, 783.99].forEach((frequency, index) => this.tone(context, now + index * 0.045, 0.62, frequency, volume * 0.075)); }
      else if (id === 'builtin:whoosh') {
        const seconds = 0.42; const buffer = context.createBuffer(1, Math.ceil(context.sampleRate * seconds), context.sampleRate);
        const samples = buffer.getChannelData(0); for (let at = 0; at < samples.length; at++) samples[at] = Math.random() * 2 - 1;
        const source = context.createBufferSource(); source.buffer = buffer;
        const filter = context.createBiquadFilter(); filter.type = 'bandpass'; filter.Q.value = 0.7;
        filter.frequency.setValueAtTime(2400, now); filter.frequency.exponentialRampToValueAtTime(350, now + seconds);
        const gain = context.createGain(); gain.gain.setValueAtTime(0.0001, now); gain.gain.exponentialRampToValueAtTime(Math.max(0.0001, volume * 0.32), now + 0.045); gain.gain.exponentialRampToValueAtTime(0.0001, now + seconds);
        source.connect(filter); filter.connect(gain); gain.connect(this.master!);
        this.started(source, [filter, gain], () => source.start(now, 0, seconds));
      } else {
        const buffer = this.buffers.get(id);
        if (!buffer) { this.warn('Emote-Sound wurde nicht rechtzeitig vorbereitet und wird übersprungen.'); return; }
        const source = context.createBufferSource(); source.buffer = buffer;
        const gain = context.createGain(); gain.gain.setValueAtTime(volume, now); source.connect(gain); gain.connect(this.master!);
        this.started(source, [gain], () => source.start(now, 0, Math.min(MAX_CLIP_SECONDS, buffer.duration)));
      }
    } catch { this.warn('Der Emote-Sound ist in dieser Audioausgabe nicht verfügbar und wird übersprungen.'); }
  }

  stop(): void {
    this.generation++; this.buffers.clear();
    for (const [source, cleanup] of this.sources) {
      source.onended = null; try { source.stop(); } catch { /* already finished */ }
      try { source.disconnect(); for (const node of cleanup) node.disconnect(); } catch { /* context closed */ }
    }
    this.sources.clear(); this.setPlaying(false);
  }
  destroy(): void {
    this.stop(); this.disposed = true;
    this.master?.disconnect(); this.master = undefined;
    if (this.context) void this.context.close().catch(() => {}); this.context = undefined;
  }
}
