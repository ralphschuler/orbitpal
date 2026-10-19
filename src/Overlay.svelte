<script lang="ts">
  import { t } from './lib/i18n';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Avatar from './components/Avatar.svelte';
  import { AutopilotRuntimeVersions, type AutopilotRuntime } from './lib/autopilot-runtime';
  import { SpeechPlayer } from './lib/audio';
  import { EmoteSequence, type EmotePayload } from './lib/presentation';
  import { presentAdmittedEmote } from './lib/emote-capabilities';
  import { EmoteAudioPlayer } from './lib/emote-audio';
  import { SubtitleLifetime } from './lib/subtitles';
  import { call, native, cloneDefaults, onEvent, assetUrl } from './lib/api';

  type SpeechEvent = { kind: 'speech'; botId?: string; id: string; text: string; speechText?: string; voice: any; emotion?: string; emotes?: EmotePayload[] };
  type PlaybackJob = { event: SpeechEvent; started: boolean; finished: boolean; reporting: Promise<void> };
  let liveAvatar = $state<{canPresent:(event:EmotePayload)=>boolean;present:(event:EmotePayload)=>boolean}>();
  const admittedEmotes=(events:EmotePayload[]=[])=>events.filter(event=>liveAvatar?.canPresent(event));
  let profile = $state<any>(cloneDefaults());
  let speaking = $state(false);
  let level = $state(0);
  let emotion = $state('neutral');
  let autopilot = $state<AutopilotRuntime>();
  const autopilotVersions = new AutopilotRuntimeVersions();
  function acceptAutopilot(runtime:AutopilotRuntime){if(autopilotVersions.accept(runtime))autopilot=runtime.active?runtime:undefined;}
  let text = $state('');
  let overlayUrl = $state(location.href);
  // Each renderer stays bound to its own bot even when the studio selects another one.
  let botId = new URL(location.href).searchParams.get('bot') || '';
  let failure = $state('');
  let failureDetail = $state('');
  let activationNeeded = $state(false);
  let connectionFailure = $state('');
  let soundNotice = $state('');
  let soundActivationNeeded = $state(false);
  let player: SpeechPlayer;
  let emoteAudio: EmoteAudioPlayer;
  let emoteRevision = 0;
  let soundPlaybackId = '';
  let soundPlaying = false;
  let soundReporting = Promise.resolve();
  let current: PlaybackJob | undefined;
  let disposed = false;
  let profileRevision = 0;
  let runtimeRevision = 0;
  let ready: Promise<void> = Promise.resolve();
  const receivedIds = new Set<string>();
  const emoteSequence = new EmoteSequence(applyEmote);
  const subtitles = new SubtitleLifetime((value) => { text = value; });

  function applyEmote(event: EmotePayload) {
    presentAdmittedEmote(liveAvatar,event,()=>{profile.avatar.emote = {...event,
      action: event.action, nonce: event.nonce ?? `${Date.now()}-${Math.random()}`,
      duration: (event.duration ?? 2.5) * 1000, intensity: event.intensity ?? 1,
    };
    if(profile.output.mode===(native?'overlay':'obs'))emoteAudio?.play(event);});
  }

  async function resolveSound(soundId: string) {
    if (native) return call('get_emote_sound', { soundId, botId });
    const url = new URL(`/emote-sound/${encodeURIComponent(soundId)}`, overlayUrl);
    url.search = new URL(overlayUrl).search;
    if (botId) url.searchParams.set('bot', botId);
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 5000);
    try {
      const result = await fetch(url, { signal: controller.signal });
      if (!result.ok) throw new Error('Emote-Sound nicht verfügbar.');
      return await result.json();
    } finally { window.clearTimeout(timeout); }
  }

  async function manualEmote(event: EmotePayload) {
    ++emoteRevision; const revision = emoteRevision;
    emoteSequence.stop(); emoteAudio?.stop();
    await ready;
    if (disposed || revision !== emoteRevision) return;
    if(!liveAvatar?.canPresent(event))return;
    if(profile.output.mode===(native?'overlay':'obs'))await emoteAudio?.prepare([event], resolveSound);
    if (disposed || revision !== emoteRevision) return;
    applyEmote(event);
  }

  function reportSoundPlaying(playing: boolean) {
    soundPlaying = playing;
    if (playing) soundPlaybackId ||= crypto.randomUUID();
    const id = soundPlaybackId; const targetBot = botId;
    if (!id || !targetBot) return;
    if (!playing) soundPlaybackId = '';
    const payload = {botId: targetBot, output: native ? 'overlay' : 'obs', id, playing};
    soundReporting = soundReporting.then(async () => {
      try {
        if(native) { await call('report_emote_playback',payload); return; }
        const url = new URL('/emote-playback',overlayUrl); url.search=new URL(overlayUrl).search;url.searchParams.set('bot',targetBot);
        const controller = new AbortController(); const timer=window.setTimeout(()=>controller.abort(),3000);
        try { await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload),signal:controller.signal,keepalive:true}); }
        finally { window.clearTimeout(timer); }
      } catch { /* A bounded host lease expires when an output disconnects. */ }
    });
  }

  async function sendPlayback(id: string, state: boolean, targetBot: string, error?: string) {
    const payload = { botId: targetBot, id, speaking: state, ...(error ? { error } : {}) };
    try {
      if (native) {
        await call('report_playback', payload);
      } else {
        const url = new URL('/playback', overlayUrl);
        url.search = new URL(overlayUrl).search;
        if (targetBot) url.searchParams.set('bot', targetBot);
        const controller = new AbortController();
        const timeout = window.setTimeout(() => controller.abort(), 3000);
        try {
          const result = await fetch(url, {
            method: 'POST', headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
            signal: controller.signal, keepalive: true,
          });
          if (!result.ok) throw new Error('Wiedergabestatus konnte nicht bestätigt werden.');
        } finally {
          window.clearTimeout(timeout);
        }
      }
    } catch { /* Disconnected overlays cannot acknowledge playback. */ }
  }

  function playback(job: PlaybackJob, state: boolean, error?: string) {
    // Keep actual start and finish ordered, and capture the utterance ID rather than
    // reading a mutable current ID from an old player callback.
    job.reporting = job.reporting.then(() => sendPlayback(job.event.id, state, job.event.botId || botId, error));
  }

  function finish(job: PlaybackJob, error?: string) {
    if (job.finished) return;
    job.finished = true;
    playback(job, false, error);
    if (current === job) {
      if (error) subtitles.cancel(job.event.id);
      else if (job.started) subtitles.finish(job.event.id);
      current = undefined;
      if (error) {
        emoteSequence.stop();
        emoteAudio?.stop();
        profile.avatar.emote = undefined;
      }
    }
  }

  function cancelCurrent() {
    ++emoteRevision;
    const job = current;
    current = undefined;
    subtitles.cancel();
    emoteSequence.stop();
    emoteAudio?.stop();
    profile.avatar.emote = undefined;
    player?.stop();
    if (job) finish(job);
    speaking = false;
    level = 0;
  }

  async function speak(event: SpeechEvent) {
    if (disposed) return;
    cancelCurrent();
    const job: PlaybackJob = { event, started: false, finished: false, reporting: Promise.resolve() };
    current = job;
    const plannedEmoteRevision = emoteRevision;
    failure = '';
    failureDetail = '';
    activationNeeded = false;
    let playbackError: string | undefined;
    const onStart = () => {
      if (disposed || current !== job || job.finished || job.started) return;
      job.started = true;
      emotion = event.emotion || 'neutral';
      subtitles.start(event.id, event.text);
      if(plannedEmoteRevision===emoteRevision)emoteSequence.start(Array.isArray(event.emotes) ? event.emotes : []);
      playback(job, true);
    };
    try {
      await ready;
      if (disposed || current !== job || job.finished) return;
      if (!event.voice) throw new Error('Stimme des Wiedergabeauftrags fehlt.');
      const voice = { ...event.voice };
      if(plannedEmoteRevision===emoteRevision&&profile.output.mode===(native?'overlay':'obs'))await emoteAudio.prepare(admittedEmotes(Array.isArray(event.emotes) ? event.emotes : []), resolveSound);
      if (disposed || current !== job || job.finished) return;
      if (voice.enabled === false) { onStart(); return; }
      const speechText = voice.provider === 'elevenlabs' ? event.speechText || event.text : event.text;
      await player.speak(speechText, voice, async () => {
        if (native) return call('get_speech_audio', { id: event.id, botId: event.botId || botId });
        const url = new URL(`/speech/${encodeURIComponent(event.id)}`, overlayUrl);
        url.search = new URL(overlayUrl).search;
        if (botId) url.searchParams.set('bot', botId);
        const result = await fetch(url);
        if (!result.ok) throw new Error('Sprachausgabe nicht verfügbar. Einstellungen in der App prüfen.');
        return result.json();
      }, onStart);
      if (disposed || current !== job || job.finished) return;
      failure = '';
    } catch (error) {
      if (disposed || current !== job || job.finished) return;
      failureDetail = error instanceof Error ? error.message : String(error);
      playbackError = failureDetail;
      activationNeeded = /not.allowed|autoplay|gesture|interact|suspend|permission|audio.?context/i.test(failureDetail);
      failure = activationNeeded ? 'Audio für die nächste Antwort aktivieren' : 'Audiohinweis schließen';
    } finally {
      // Failure before onStart still releases this exact pending host job. A
      // completed ID is never replayed while the host may already speak another bot.
      finish(job, playbackError);
    }
  }

  function receive(event: any) {
    if (disposed) return;
    const targetBot = event.botId || event.profile?.id;
    const globalStop = event.kind === 'stop' && !targetBot;
    if (!globalStop && (!targetBot || (botId && targetBot !== botId))) return;
    if (event.kind === 'profile') {
      botId ||= event.profile.id;
      ++profileRevision; ++emoteRevision; emoteSequence.stop();emoteAudio?.stop(); profile = event.profile;
      ++runtimeRevision; if(event.autopilotRuntime||event.profile?.autopilotRuntime)acceptAutopilot(event.autopilotRuntime||event.profile.autopilotRuntime);
      // Pin legacy OBS links without a bot parameter after their initial profile arrives.
      const url = new URL(overlayUrl); url.searchParams.set('bot', botId); overlayUrl = url.toString();
      return;
    }
    if (event.kind === 'autopilot') { ++runtimeRevision; acceptAutopilot(event); return; }
    if (event.kind === 'emote') {
      void manualEmote(event);
      return;
    }
    if (event.kind === 'cancel-speech') { if (current?.event.id === event.id) cancelCurrent(); return; }
    if (event.kind === 'stop') { ++runtimeRevision; autopilot = undefined; cancelCurrent(); failure = ''; failureDetail = ''; return; }
    if (event.kind === 'error') { subtitles.cancel(); connectionFailure = event.text || 'Die Verbindung zur App ist nicht verfügbar.'; return; }
    if (event.kind !== 'speech' || !event.id || receivedIds.has(event.id)) return;
    receivedIds.add(event.id);
    if (receivedIds.size > 128) receivedIds.delete(receivedIds.values().next().value!);
    void speak(event);
  }

  async function acknowledgeAudioFailure() {
    if (activationNeeded) {
      try { await Promise.all([player.unlock(), emoteAudio.unlock()]); }
      catch (error) { failureDetail = error instanceof Error ? error.message : String(error); return; }
    }
    if (!disposed) { failure = ''; failureDetail = ''; activationNeeded = false; }
  }

  async function activateEmoteAudio() {
    if (soundActivationNeeded) await emoteAudio?.unlock();
    if (!disposed) soundNotice = '';
  }

  async function drag(event: PointerEvent) {
    if (!native || profile.output?.clickThrough || event.button !== 0) return;
    event.preventDefault(); event.stopPropagation();
    try { await getCurrentWindow().startDragging(); }
    catch (error) { failureDetail = error instanceof Error ? error.message : String(error); }
  }

  onMount(() => {
    document.documentElement.classList.add('overlay-document');
    player = new SpeechPlayer((value) => { level = value; }, (state) => { speaking = state; });
    emoteAudio = new EmoteAudioPlayer((message) => { soundNotice = message; soundActivationNeeded = /gesperrt|aktivieren|klick/i.test(message); },reportSoundPlaying);
    const soundHeartbeat=window.setInterval(()=>{if(soundPlaying&&!disposed)reportSoundPlaying(true);},4000);
    const cleanup: (() => void)[] = [];
    if (native) {
      const initialRevision = profileRevision;
      const initialRuntimeRevision = runtimeRevision;
      ready = call('get_app_state', { botId: botId || undefined }).then((state) => {
        if (disposed) return;
        botId ||= state.profile.id;
        if (profileRevision === initialRevision) profile = state.profile;
        if (runtimeRevision === initialRuntimeRevision) { const runtime=(state.autopilot || []).find((entry:any) => entry.botId === botId); if(runtime)acceptAutopilot(runtime); }
        const url = new URL(state.overlayUrl); url.searchParams.set('bot', botId); overlayUrl = url.toString();
      }).catch(() => { if (!disposed) connectionFailure = 'Charakterprofil konnte nicht geladen werden.'; });
      onEvent('cohost://overlay', receive).then((unlisten) => {
        if (disposed) unlisten(); else cleanup.push(unlisten);
      }).catch(() => { if (!disposed) connectionFailure = 'Die Verbindung zur App konnte nicht hergestellt werden.'; });
    } else {
      const url = new URL('/events', overlayUrl);
      url.search = new URL(overlayUrl).search;
      const events = new EventSource(url);
      events.onmessage = (event) => {
        try { receive(JSON.parse(event.data)); }
        catch { connectionFailure = 'Eine Nachricht der App konnte nicht gelesen werden.'; }
      };
      events.onerror = () => { if (!disposed) connectionFailure = 'Verbindung zur App wird wiederhergestellt …'; };
      events.onopen = () => { if (!disposed) connectionFailure = ''; };
      cleanup.push(() => events.close());
    }
    return () => {
      disposed = true; window.clearInterval(soundHeartbeat);cancelCurrent(); subtitles.destroy(); cleanup.forEach((close) => close()); player.destroy(); emoteAudio.destroy();
      document.documentElement.classList.remove('overlay-document');
    };
  });
</script>

<div class="overlay-scene">
  <Avatar bind:this={liveAvatar} {profile} {speaking} {level} {emotion} {autopilot} transparent={true} outputUrl={overlayUrl} assetUrl={assetUrl(profile.avatar.assetPath, overlayUrl)} />
  {#if native && !profile.output?.clickThrough}
    <button class="overlay-drag" aria-label={$t("Figurenfenster verschieben")} title={$t("Figurenfenster verschieben")} onpointerdown={drag}>
      <svg width="20" height="14" viewBox="0 0 20 14" fill="currentColor" aria-hidden="true"><circle cx="5" cy="4" r="1.4"/><circle cx="10" cy="4" r="1.4"/><circle cx="15" cy="4" r="1.4"/><circle cx="5" cy="10" r="1.4"/><circle cx="10" cy="10" r="1.4"/><circle cx="15" cy="10" r="1.4"/></svg>
    </button>
  {/if}
  {#if text && profile.output.showSubtitles}
    <div class="overlay-subtitle"><strong>{profile.name}</strong><span>{text}</span></div>
  {/if}
  {#if failure}
    <button class="overlay-audio" onclick={acknowledgeAudioFailure} title={$t(failureDetail)}>{$t(failure)}</button>
  {:else if soundNotice}
    <button class="overlay-audio" onclick={activateEmoteAudio} title={$t(soundNotice)}>{soundActivationNeeded ? $t("Emote-Audio aktivieren") : $t("Soundhinweis schließen")}</button>
  {:else if connectionFailure}
    <div class="overlay-connection" role="status">{$t(connectionFailure)}</div>
  {/if}
</div>

<style>
  .overlay-drag { position: absolute; top: 12px; right: 12px; display: grid; place-items: center; width: 36px; height: 29px; border: 1px solid #b2c6e54d; border-radius: 8px; background: #1d2c45c9; color: #d1dff6; cursor: grab; opacity: 0; transition: opacity 0.15s; backdrop-filter: blur(8px); }
  .overlay-scene:hover .overlay-drag, .overlay-drag:focus-visible { opacity: 1; }
  .overlay-drag:active { cursor: grabbing; }
  .overlay-connection { position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%); background: #243b5ced; border: 1px solid #8aa3c7; color: white; border-radius: 7px; padding: 9px 13px; max-width: 90%; text-align: center; font-size: 11px; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .overlay-drag { transition: none; } }
</style>
