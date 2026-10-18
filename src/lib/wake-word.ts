import { recordingToWav, type SynthesizedAudio } from './audio';

export type WakeWordState = 'stopped' | 'starting' | 'listening' | 'capturing' | 'transcribing' | 'follow-up' | 'triggered' | 'paused';

export interface WakeWordOptions {
  phrases: string[];
  transcribe: (audio: SynthesizedAudio) => Promise<string>;
  onTrigger: (text: string) => Promise<void> | void;
  onState: (state: WakeWordState) => void;
  onError: (message: string) => void;
}

export function normalizeWakePhrases(phrases: unknown): string[] {
  if (!Array.isArray(phrases)) return [];
  const unique = new Map<string, string>();
  for (const value of phrases) {
    if (typeof value !== 'string') continue;
    const phrase = value.normalize('NFKC').trim().replace(/\s+/g, ' ');
    const key = words(phrase).map((word) => word.value).join(' ');
    if (key && phrase.length <= 100 && !unique.has(key)) unique.set(key, phrase);
  }
  return [...unique.values()].slice(0, 8);
}

function words(text: string): { value: string; end: number }[] {
  // Match whole Unicode words. Punctuation between words is harmless, while
  // e.g. “Nova” must not match the beginning of “Novak”. Preserve source offsets
  // so the actual question retains spelling and punctuation.
  return [...text.matchAll(/[\p{L}\p{M}\p{N}]+/gu)].map((match) => ({
    value: match[0].normalize('NFKC').toLocaleLowerCase('de-DE'),
    end: match.index! + match[0].length,
  }));
}

export function findWakePhrase(text: string, phrases: readonly string[]): { phrase: string; request: string } | undefined {
  const transcript = words(text);
  const candidates = phrases.map((phrase) => ({ phrase, tokens: words(phrase) }))
    .filter((candidate) => candidate.tokens.length).sort((a, b) => b.tokens.length - a.tokens.length);
  for (let start = 0; start < transcript.length; start++) {
    for (const candidate of candidates) {
      if (candidate.tokens.every((token, offset) => transcript[start + offset]?.value === token.value)) {
        const end = transcript[start + candidate.tokens.length - 1].end;
        const request = text.slice(end).replace(/^[\s,;:!?\.\u2026\-\u2013\u2014]+/u, '').trim();
        return { phrase: candidate.phrase, request };
      }
    }
  }
}

/** Explicitly started, STT-based phrase recognition, not an offline KWS model.
 * Audio segments are transcribed before their activation phrase can be known.
 * One epoch owns every microphone, conversion and transcription result. */
export class WakeWordListener {
  private epoch = 0;
  private requested = false;
  private blocked = false;
  private working = false;
  private stream?: MediaStream;
  private context?: AudioContext;
  private source?: MediaStreamAudioSourceNode;
  private analyser?: AnalyserNode;
  private mute?: GainNode;
  private recorder?: MediaRecorder;
  private samples = new Float32Array(1024);
  private pollTimer?: ReturnType<typeof setInterval>;
  private resumeTimer?: ReturnType<typeof setTimeout>;
  private segmentStarted = 0;
  private voiceStarted = 0;
  private voiceCandidate = 0;
  private lastVoice = 0;
  private followUpUntil = 0;
  private utteranceFollowUp = false;
  private noiseFloor = 0.004;
  private state: WakeWordState = 'stopped';
  private mimeType = '';
  private phrases: string[];

  constructor(private options: WakeWordOptions) {
    this.phrases = normalizeWakePhrases(options.phrases);
  }

  async start(beforeListen?: () => Promise<boolean | void> | boolean | void): Promise<void> {
    this.stop();
    if (!this.phrases.length) throw new Error('Bitte mindestens eine Aktivierungsphrase mit höchstens 100 Zeichen eingeben.');
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
      throw new Error('Diese WebView unterstützt keine Mikrofonaufnahme. Bitte die Desktop-App und ihre WebView aktualisieren.');
    }
    if (this.blocked) throw new Error('Der Cohost ist gerade beschäftigt. Starte das Zuhören nach seiner Antwort.');
    const epoch = ++this.epoch;
    this.requested = true;
    this.setState('starting');
    try {
      const context = new AudioContext();
      this.context = context;
      // Resume is initiated directly by the Start click, before profile saving
      // or the microphone permission dialog can consume user activation.
      let resumeTimeout: ReturnType<typeof setTimeout> | undefined;
      try {
        const [, saved] = await Promise.all([
          Promise.race([
            context.resume(),
            new Promise<never>((_, reject) => {
              resumeTimeout = setTimeout(() => reject(new Error('Audio konnte nicht aktiviert werden. Bitte erneut auf „Zuhören starten“ klicken.')), 5000);
            }),
          ]),
          Promise.resolve(beforeListen?.()),
        ]);
        if (!this.current(epoch)) return;
        if (saved === false) { this.stop(); return; }
      } finally { if (resumeTimeout !== undefined) clearTimeout(resumeTimeout); }
      const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true }, video: false });
      if (!this.current(epoch)) { stream.getTracks().forEach((track) => track.stop()); return; }
      this.stream = stream;
      for (const track of stream.getAudioTracks()) {
        track.onended = () => { if (this.requested) this.fail('Das Mikrofon wurde getrennt oder seine Freigabe beendet.'); };
      }
      this.source = context.createMediaStreamSource(stream);
      this.analyser = context.createAnalyser();
      this.analyser.fftSize = 1024;
      this.analyser.smoothingTimeConstant = 0;
      this.mute = context.createGain();
      this.mute.gain.value = 0;
      this.source.connect(this.analyser);
      this.analyser.connect(this.mute);
      this.mute.connect(context.destination);
      this.mimeType = ['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4', 'audio/webm']
        .find((mime) => MediaRecorder.isTypeSupported(mime)) || '';
      this.noiseFloor = 0.004;
      this.resumeCapture(0);
    } catch (error) {
      if (!this.current(epoch)) return;
      this.stop();
      if (error instanceof DOMException && ['NotAllowedError', 'SecurityError'].includes(error.name)) {
        throw new Error('Mikrofonzugriff wurde nicht freigegeben. Erlaube ihn in den Systemeinstellungen und starte das Zuhören erneut.');
      }
      throw error;
    }
  }

  setBlocked(blocked: boolean): void {
    if (this.blocked === blocked) return;
    this.blocked = blocked;
    if (!this.requested) return;
    if (blocked) {
      ++this.epoch;
      this.followUpUntil = 0;
      this.working = false;
      this.suspendCapture();
      this.setState('paused');
    } else if (this.stream && this.context?.state !== 'closed') {
      // Leave a short gap after playback, including any room/speaker echo.
      this.resumeCapture(650);
    } else {
      // A profile change/permission race must never open a new microphone on
      // its own. The operator can start a fresh listening session explicitly.
      this.stop();
    }
  }

  stop(): void {
    ++this.epoch;
    this.requested = false;
    this.working = false;
    this.followUpUntil = 0;
    this.suspendCapture();
    for (const track of this.stream?.getTracks() ?? []) { track.onended = null; track.stop(); }
    this.stream = undefined;
    this.source?.disconnect();
    this.analyser?.disconnect();
    this.mute?.disconnect();
    this.source = undefined;
    this.analyser = undefined;
    this.mute = undefined;
    if (this.context) void this.context.close().catch(() => {});
    this.context = undefined;
    this.setState('stopped');
  }

  private current(epoch: number) { return this.requested && this.epoch === epoch; }

  private setState(state: WakeWordState) {
    if (this.state === state) return;
    this.state = state;
    this.options.onState(state);
  }

  private suspendCapture() {
    if (this.pollTimer !== undefined) clearInterval(this.pollTimer);
    if (this.resumeTimer !== undefined) clearTimeout(this.resumeTimer);
    this.pollTimer = undefined;
    this.resumeTimer = undefined;
    this.discardRecorder();
    this.stream?.getAudioTracks().forEach((track) => { track.enabled = false; });
  }

  private discardRecorder() {
    const recorder = this.recorder;
    this.recorder = undefined;
    if (recorder) {
      recorder.ondataavailable = null;
      recorder.onstop = null;
      recorder.onerror = null;
      if (recorder.state !== 'inactive') { try { recorder.stop(); } catch { /* Already ending. */ } }
    }
    this.voiceStarted = this.voiceCandidate = this.lastVoice = 0;
    this.utteranceFollowUp = false;
  }

  private resumeCapture(delay: number) {
    if (!this.requested || this.blocked || this.working || !this.stream) return;
    if (this.resumeTimer !== undefined) clearTimeout(this.resumeTimer);
    const epoch = this.epoch;
    this.resumeTimer = setTimeout(() => {
      this.resumeTimer = undefined;
      if (!this.current(epoch) || this.blocked || this.working || !this.stream) return;
      if (!this.stream.getAudioTracks().some((track) => track.readyState === 'live')) { this.fail('Das Mikrofon ist nicht mehr verfügbar.'); return; }
      this.stream.getAudioTracks().forEach((track) => { track.enabled = true; });
      try {
        this.beginSegment(epoch);
        this.pollTimer = setInterval(() => this.poll(epoch), 50);
      } catch (error) { this.fail(error instanceof Error ? error.message : String(error)); }
    }, delay);
  }

  private beginSegment(epoch: number) {
    if (!this.stream || !this.current(epoch) || this.blocked) return;
    this.discardRecorder();
    const recorder = new MediaRecorder(this.stream, this.mimeType ? { mimeType: this.mimeType } : undefined);
    const parts: Blob[] = [];
    this.recorder = recorder;
    this.segmentStarted = performance.now();
    recorder.ondataavailable = (event) => { if (this.current(epoch) && event.data.size) parts.push(event.data); };
    recorder.onerror = () => { if (this.current(epoch)) this.fail('Die Mikrofonaufnahme wurde unterbrochen. Bitte Zuhören erneut starten.'); };
    recorder.onstop = () => {
      if (!this.current(epoch) || this.blocked || this.recorder !== recorder) return;
      this.recorder = undefined;
      const followUp = this.utteranceFollowUp;
      const blob = new Blob(parts, { type: recorder.mimeType || this.mimeType });
      void this.processSegment(blob, epoch, followUp);
    };
    // Keep each recording independently decodable, including its container
    // header and up to 1.8 s of pre-roll. Discard silence without an STT call.
    recorder.start();
    this.setState(this.followUpUntil > performance.now() ? 'follow-up' : 'listening');
  }

  private poll(epoch: number) {
    if (!this.current(epoch) || this.blocked || this.working || !this.analyser || !this.recorder) return;
    const now = performance.now();
    this.analyser.getFloatTimeDomainData(this.samples);
    let energy = 0;
    for (const sample of this.samples) energy += sample * sample;
    const rms = Math.sqrt(energy / this.samples.length);
    const threshold = Math.max(0.012, Math.min(0.06, this.noiseFloor * 2.8));
    if (!this.voiceStarted && rms < threshold * 0.8) this.noiseFloor = this.noiseFloor * 0.97 + rms * 0.03;
    if (rms >= threshold) {
      this.lastVoice = now;
      if (!this.voiceCandidate) this.voiceCandidate = now;
      if (!this.voiceStarted && now - this.voiceCandidate >= 100) {
        this.voiceStarted = this.voiceCandidate;
        this.utteranceFollowUp = this.followUpUntil > this.voiceStarted;
        this.setState('capturing');
      }
    } else if (!this.voiceStarted) this.voiceCandidate = 0;

    if (this.voiceStarted && (now - this.lastVoice >= 650 || now - this.voiceStarted >= 12_000)) {
      if (this.pollTimer !== undefined) clearInterval(this.pollTimer);
      this.pollTimer = undefined;
      this.stream?.getAudioTracks().forEach((track) => { track.enabled = false; });
      this.working = true;
      this.setState('transcribing');
      try { this.recorder.stop(); } catch { this.fail('Der Audioabschnitt konnte nicht abgeschlossen werden.'); }
    } else if (!this.voiceStarted && !this.voiceCandidate && now - this.segmentStarted >= 1800) {
      try { this.beginSegment(epoch); } catch (error) { this.fail(error instanceof Error ? error.message : String(error)); }
    } else if (!this.voiceStarted && this.followUpUntil && now >= this.followUpUntil) {
      this.followUpUntil = 0;
      this.setState('listening');
    }
  }

  private async processSegment(blob: Blob, epoch: number, followUp: boolean) {
    try {
      if (!this.current(epoch) || this.blocked) return;
      const audio = await recordingToWav(blob);
      if (!this.current(epoch) || this.blocked) return;
      const transcript = (await this.options.transcribe(audio)).trim();
      if (!this.current(epoch) || this.blocked) return;
      const wake = findWakePhrase(transcript, this.phrases);
      if (wake && !words(wake.request).length) {
        this.followUpUntil = performance.now() + 10_000;
      } else {
        const request = wake?.request || (followUp ? transcript : '');
        if (words(request).length) {
          this.followUpUntil = 0;
          // Capture is already suspended before the callback can start TTS.
          this.setState('triggered');
          await this.options.onTrigger(request);
          if (!this.current(epoch)) return;
        }
      }
    } catch (error) {
      if (this.current(epoch)) this.fail(error instanceof Error ? error.message : String(error));
      return;
    } finally {
      if (this.current(epoch)) {
        this.working = false;
        if (!this.blocked) this.resumeCapture(200);
      }
    }
  }

  private fail(message: string) {
    this.stop();
    this.options.onError(message);
  }
}
