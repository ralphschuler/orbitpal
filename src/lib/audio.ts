export type SynthesizedAudio = { audioBase64: string; mimeType: string };

/** Exactly one audible utterance at a time. Stop also invalidates pending synthesis. */
export class SpeechPlayer {
  private generation = 0;
  private startCancellation?: AbortController;
  private context?: AudioContext;
  private source?: AudioBufferSourceNode;
  private gain?: GainNode;
  private analyser?: AnalyserNode;
  private utterance?: SpeechSynthesisUtterance;
  private finish?: () => void;
  private meterFrame = 0;
  private disposed = false;
  private state = false;

  constructor(
    private onLevel: (level: number) => void,
    private onState: (speaking: boolean) => void,
  ) {}

  async speak(
    text: string,
    voice: any,
    fetchAudio?: (text: string) => Promise<SynthesizedAudio>,
    onStart?: () => void,
    beforeStart?: () => Promise<void>,
  ): Promise<void> {
    this.stop();
    if (this.disposed || voice?.enabled === false || !text.trim()) return;
    const generation = this.generation;
    this.startCancellation = new AbortController();
    let started = false;
    const start = () => {
      if (!this.current(generation) || started) return;
      started = true;
      this.setSpeaking(true);
      if (this.current(generation)) onStart?.();
    };
    if (!voice?.provider || ['system', 'browser'].includes(voice.provider)) {
      await this.speakSystem(text, voice, generation, start, beforeStart);
      return;
    }
    if (!fetchAudio) throw new Error('Für diese Stimme ist keine Sprachausgabe verbunden.');
    // Create/resume while still in the initiating gesture where possible.
    const context = this.getContext();
    await this.resumeContext(context);
    if (!this.current(generation)) return;
    let payload: SynthesizedAudio;
    try { payload = await fetchAudio(text); }
    catch (error) {
      if (!this.current(generation)) return;
      throw error;
    }
    if (!this.current(generation)) return;
    if (!payload.audioBase64) throw new Error('Die Sprachausgabe hat keine Audiodaten geliefert.');
    const encoded = payload.audioBase64.replace(/^data:[^,]*,/, '').replace(/\s/g, '');
    let binary: string;
    try { binary = atob(encoded); }
    catch { throw new Error('Die Sprachausgabe hat ungültige Audiodaten geliefert.'); }
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    let buffer: AudioBuffer;
    try { buffer = await context.decodeAudioData(bytes.buffer); }
    catch {
      if (!this.current(generation)) return;
      throw new Error(`Das Audioformat ${payload.mimeType || 'dieser Stimme'} kann hier nicht abgespielt werden.`);
    }
    if (!this.current(generation)) return;
    await this.resumeContext(context);
    if (!this.current(generation)) return;
    // Fetch and decoding may finish while the character returns to its speaking
    // position. Only the actual playback start waits for that presentation gate.
    if (beforeStart && !await this.waitForStart(beforeStart, generation)) return;
    if (!this.current(generation)) return;

    const source = context.createBufferSource();
    const gain = context.createGain();
    const analyser = context.createAnalyser();
    source.buffer = buffer;
    source.playbackRate.value = clamp(voice?.rate, 0.5, 2, 1);
    gain.gain.value = clamp(voice?.volume, 0, 1, 0.85);
    analyser.fftSize = 1024;
    analyser.smoothingTimeConstant = 0.35;
    // Read the actual waveform before the output-volume control so that a quiet
    // listening level does not suppress the character's speech articulation.
    source.connect(analyser);
    analyser.connect(gain);
    gain.connect(context.destination);
    this.source = source;
    this.gain = gain;
    this.analyser = analyser;

    await new Promise<void>((resolve, reject) => {
      const finish = () => {
        if (this.finish === finish) this.finish = undefined;
        if (this.current(generation)) {
          this.releaseAudio();
          this.setSpeaking(false);
        }
        resolve();
      };
      this.finish = finish;
      source.onended = finish;
      try {
        source.start();
        // Audio is fully decoded and has been scheduled successfully. Preparation
        // never reports a start, and stop invalidates any late synthesis result.
        start();
        this.measure(analyser, generation);
      } catch (error) {
        this.releaseAudio();
        this.finish = undefined;
        this.setSpeaking(false);
        reject(error);
      }
    });
  }

  stop(): void {
    ++this.generation;
    this.startCancellation?.abort();
    this.startCancellation = undefined;
    cancelAnimationFrame(this.meterFrame);
    this.meterFrame = 0;
    const finish = this.finish;
    this.finish = undefined;
    if (this.utterance) {
      this.utterance.onstart = null;
      this.utterance.onend = null;
      this.utterance.onerror = null;
      this.utterance.onboundary = null;
      this.utterance = undefined;
      window.speechSynthesis?.cancel();
    }
    if (this.source) {
      this.source.onended = null;
      try { this.source.stop(); } catch { /* The source may have already ended. */ }
    }
    this.releaseAudio();
    finish?.();
    this.setSpeaking(false);
  }

  destroy(): void {
    this.stop();
    this.disposed = true;
    if (this.context) void this.context.close();
    this.context = undefined;
  }

  /** Called directly from a user gesture after an autoplay restriction. */
  async unlock(): Promise<void> {
    if (this.disposed) return;
    const context = this.getContext();
    await this.resumeContext(context);
    window.speechSynthesis?.resume();
  }

  private getContext() {
    if (!this.context || this.context.state === 'closed') this.context = new AudioContext();
    return this.context;
  }

  private async resumeContext(context: AudioContext): Promise<void> {
    if (context.state !== 'suspended') return;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      // Chromium may leave resume() pending until a user gesture instead of
      // rejecting it. A bounded wait lets the host release the failed reply.
      await Promise.race([
        context.resume(),
        new Promise<never>((_, reject) => {
          timeout = setTimeout(() => reject(new Error('AudioContext wartet auf eine Nutzerinteraktion. Audio aktivieren und eine neue Antwort anfordern.')), 4000);
        }),
      ]);
    } finally {
      if (timeout !== undefined) clearTimeout(timeout);
    }
  }

  private current(generation: number) { return !this.disposed && generation === this.generation; }

  /** A stopped/replaced utterance must settle even if its presentation hook
   * never does. Its late completion or rejection cannot affect a newer turn. */
  private waitForStart(beforeStart: () => Promise<void>, generation: number): Promise<boolean> {
    const signal = this.startCancellation?.signal;
    if (!this.current(generation) || !signal || signal.aborted) return Promise.resolve(false);
    return new Promise<boolean>((resolve, reject) => {
      const cancel = () => { signal.removeEventListener('abort', cancel); resolve(false); };
      signal.addEventListener('abort', cancel, { once: true });
      Promise.resolve().then(() => {
        if (this.current(generation) && !signal.aborted) return beforeStart();
      }).then(() => {
        signal.removeEventListener('abort', cancel);
        resolve(this.current(generation) && !signal.aborted);
      }, error => {
        signal.removeEventListener('abort', cancel);
        if (this.current(generation) && !signal.aborted) reject(error);
        else resolve(false);
      });
    });
  }

  private setSpeaking(speaking: boolean) {
    if (this.state !== speaking) {
      this.state = speaking;
      this.onState(speaking);
    }
    if (!speaking) this.onLevel(0);
  }

  private releaseAudio() {
    cancelAnimationFrame(this.meterFrame);
    this.meterFrame = 0;
    if (this.source) this.source.onended = null;
    this.source?.disconnect();
    this.gain?.disconnect();
    this.analyser?.disconnect();
    this.source = undefined;
    this.gain = undefined;
    this.analyser = undefined;
  }

  private measure(analyser: AnalyserNode, generation: number) {
    const samples = new Float32Array(analyser.fftSize);
    let smooth = 0;
    let peak = 0.04;
    const tick = () => {
      if (!this.current(generation) || this.analyser !== analyser) return;
      analyser.getFloatTimeDomainData(samples);
      let sum = 0;
      for (const sample of samples) sum += sample * sample;
      const rms = Math.sqrt(sum / samples.length);
      peak = Math.max(0.015, rms, peak * 0.985);
      // A small absolute gate keeps silence closed; adaptive normalization makes
      // quiet synthesized speech visible without pretending to detect phonemes.
      const target = rms <= 0.0015 ? 0 : Math.min(1, Math.sqrt((rms - 0.0015) / peak) * 0.9);
      smooth += (target - smooth) * (target > smooth ? 0.72 : 0.42);
      if (target === 0 && smooth < 0.015) smooth = 0;
      this.onLevel(smooth);
      this.meterFrame = requestAnimationFrame(tick);
    };
    tick();
  }

  private async speakSystem(text: string, voice: any, generation: number, onStart: () => void, beforeStart?: () => Promise<void>): Promise<void> {
    if (!('speechSynthesis' in window)) throw new Error('Dieser WebView bietet keine Systemstimme. Bitte eine lokale oder Cloud-Stimme einrichten.');
    const synth = window.speechSynthesis;
    let voices = synth.getVoices();
    // WebKit/Chromium may populate voices asynchronously. A bounded event listener
    // permits voice selection without changing the global onvoiceschanged handler.
    if (!voices.length) {
      await new Promise<void>((resolve) => {
        const finish = () => {
          clearTimeout(timeout);
          synth.removeEventListener('voiceschanged', finish);
          resolve();
        };
        const timeout = window.setTimeout(finish, 500);
        synth.addEventListener('voiceschanged', finish, { once: true });
      });
      voices = synth.getVoices();
    }
    if (!this.current(generation)) return;
    const utterance = new SpeechSynthesisUtterance(text);
    const language = voice?.language || 'de-DE';
    const selected = voices.find((item) => item.voiceURI === voice?.voiceId || item.name === voice?.voiceId)
      ?? voices.find((item) => item.lang.toLowerCase() === language.toLowerCase())
      ?? voices.find((item) => item.lang.split('-')[0] === language.split('-')[0]);
    if (selected) utterance.voice = selected;
    utterance.lang = language;
    utterance.rate = clamp(voice?.rate, 0.5, 2, 1);
    utterance.volume = clamp(voice?.volume, 0, 1, 0.85);
    utterance.pitch = clamp(voice?.pitch, 0.5, 2, 1);
    if (beforeStart && !await this.waitForStart(beforeStart, generation)) return;
    if (!this.current(generation)) return;
    this.utterance = utterance;

    await new Promise<void>((resolve, reject) => {
      let startedAt = 0;
      let boundaryAt = 0;
      let startTimeout: ReturnType<typeof setTimeout> | undefined;
      const clearStartWait = () => {
        if (startTimeout !== undefined) clearTimeout(startTimeout);
        startTimeout = undefined;
      };
      const finish = () => {
        clearStartWait();
        if (this.finish === finish) this.finish = undefined;
        if (this.current(generation)) {
          cancelAnimationFrame(this.meterFrame);
          this.utterance = undefined;
          this.setSpeaking(false);
        }
        resolve();
      };
      this.finish = finish;
      utterance.onstart = () => {
        if (!this.current(generation) || this.utterance !== utterance || startedAt) return;
        clearStartWait();
        startedAt = performance.now();
        boundaryAt = startedAt;
        onStart();
        // System speech cannot be routed through a WebAudio analyser. This mouth
        // envelope follows real start/end and word boundaries, not measured RMS.
        const tick = () => {
          if (!this.current(generation) || this.utterance !== utterance) return;
          const now = performance.now();
          const elapsed = (now - startedAt) / 1000;
          const wordEnvelope = Math.max(0.12, 1 - (now - boundaryAt) / 650);
          const cadence = 0.16 + Math.abs(Math.sin(elapsed * 11)) * 0.28;
          this.onLevel(Math.min(0.8, cadence * wordEnvelope * 1.6));
          this.meterFrame = requestAnimationFrame(tick);
        };
        tick();
      };
      utterance.onboundary = () => { boundaryAt = performance.now(); };
      utterance.onend = finish;
      utterance.onerror = (event) => {
        clearStartWait();
        if (!this.current(generation) || ['canceled', 'interrupted'].includes(event.error)) { finish(); return; }
        this.finish = undefined;
        this.utterance = undefined;
        cancelAnimationFrame(this.meterFrame);
        this.setSpeaking(false);
        reject(new Error(`Systemstimme konnte nicht sprechen: ${event.error}.`));
      };
      startTimeout = setTimeout(() => {
        if (!this.current(generation) || this.utterance !== utterance) { finish(); return; }
        utterance.onstart = null;
        utterance.onend = null;
        utterance.onerror = null;
        utterance.onboundary = null;
        this.finish = undefined;
        this.utterance = undefined;
        this.setSpeaking(false);
        synth.cancel();
        reject(new Error('Die Systemstimme wurde nicht gestartet. Audio per Nutzerinteraktion aktivieren und eine neue Antwort anfordern.'));
      }, 6000);
      try { synth.speak(utterance); }
      catch (error) {
        clearStartWait();
        this.finish = undefined;
        this.utterance = undefined;
        this.setSpeaking(false);
        reject(error);
      }
    });
  }
}

function clamp(value: unknown, min: number, max: number, fallback: number) {
  const number = Number(value);
  return Number.isFinite(number) ? Math.max(min, Math.min(max, number)) : fallback;
}

/** Convert a completed MediaRecorder recording into whisper.cpp-compatible WAV.
 * Decoding/resampling stays in the WebView and needs no external converter. */
export async function recordingToWav(blob: Blob): Promise<{ audioBase64: string; mimeType: string }> {
  const maxBytes = 25 * 1024 * 1024;
  const sampleRate = 16000;
  if (!blob.size) throw new Error('Die Mikrofonaufnahme ist leer.');
  if (blob.size > maxBytes) throw new Error('Die Mikrofonaufnahme überschreitet 25 MiB. Bitte kürzer aufnehmen.');
  const context = new AudioContext();
  try {
    let decoded: AudioBuffer;
    try { decoded = await context.decodeAudioData(await blob.arrayBuffer()); }
    catch { throw new Error('Das Aufnahmeformat konnte nicht gelesen werden. Bitte ein anderes Mikrofon oder Aufnahmeformat verwenden.'); }
    const frames = Math.ceil(decoded.duration * sampleRate);
    const dataBytes = frames * 2;
    if (!Number.isFinite(frames) || frames < 1) throw new Error('Die Mikrofonaufnahme enthält kein Audio.');
    if (dataBytes + 44 > maxBytes) throw new Error('Die WAV-Aufnahme überschreitet 25 MiB. Bitte kürzer aufnehmen.');
    const offline = new OfflineAudioContext(1, frames, sampleRate);
    const source = offline.createBufferSource();
    source.buffer = decoded;
    source.connect(offline.destination);
    source.start();
    const mono = await offline.startRendering();
    source.disconnect();
    const samples = mono.getChannelData(0);
    const buffer = new ArrayBuffer(44 + dataBytes);
    const view = new DataView(buffer);
    const ascii = (offset: number, value: string) => {
      for (let index = 0; index < value.length; index++) view.setUint8(offset + index, value.charCodeAt(index));
    };
    ascii(0, 'RIFF');
    view.setUint32(4, 36 + dataBytes, true);
    ascii(8, 'WAVE');
    ascii(12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true); // PCM
    view.setUint16(22, 1, true); // mono
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    ascii(36, 'data');
    view.setUint32(40, dataBytes, true);
    for (let index = 0; index < frames; index++) {
      const sample = Number.isFinite(samples[index]) ? Math.min(1, Math.max(-1, samples[index])) : 0;
      view.setInt16(44 + index * 2, Math.round(sample * (sample < 0 ? 32768 : 32767)), true);
    }
    const audioBase64 = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const value = typeof reader.result === 'string' ? reader.result : '';
        const comma = value.indexOf(',');
        if (comma < 0) reject(new Error('Die Aufnahme konnte nicht kodiert werden.'));
        else resolve(value.slice(comma + 1));
      };
      reader.onerror = () => reject(new Error('Die Aufnahme konnte nicht kodiert werden.'));
      reader.onabort = () => reject(new Error('Das Kodieren der Aufnahme wurde abgebrochen.'));
      reader.readAsDataURL(new Blob([buffer], { type: 'audio/wav' }));
    });
    return { audioBase64, mimeType: 'audio/wav' };
  } finally {
    if (context.state !== 'closed') await context.close().catch(() => {});
  }
}
