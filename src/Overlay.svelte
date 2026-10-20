<script lang="ts">
  import {LifeOutputVersions,type LifeOutput} from './lib/life-output';
  import { t } from './lib/i18n';
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Avatar from './components/Avatar.svelte';
  import { PresentationVersions, type PresentationSnapshot } from './lib/output-presentation';
  import { captionPlacement } from './lib/desktop-motion';
  import type { DesktopAnchor } from './lib/avatar';
  import { AutopilotRuntimeVersions, type AutopilotRuntime } from './lib/autopilot-runtime';
  import { SpeechPlayer } from './lib/audio';
  import { EmoteSequence, emoteDurationMilliseconds, emoteSequenceMilliseconds, type EmotePayload } from './lib/presentation';
  import { PresentationLeases, type PresentationTicket } from './lib/presentation-leases';
  import { presentAdmittedEmote } from './lib/emote-capabilities';
  import { EmoteAudioPlayer } from './lib/emote-audio';
  import { foregroundSoundPlaying, SoundPlaybackReports, type SoundActivity } from './lib/sound-playback';
  import { SubtitleLifetime } from './lib/subtitles';
  import { call, native, cloneDefaults, onEvent, assetUrl } from './lib/api';
  import { NativeWorldOutput,type NativeWorldPort,type NativeWorldView } from './lib/native-world-output';
  import {NativeWorldSnapshots} from './lib/native-world-snapshots';
  import type {BindingStamp} from './lib/world-output';

  type SpeechEvent = { presentationOutput?:'studio'|'overlay'|'obs';presentationRouteId?:string|null;kind: 'speech'; botId?: string; id: string; text: string; speechText?: string; voice: any; emotion?: string; emotes?: EmotePayload[] };
  type PlaybackJob = { event: SpeechEvent; started: boolean; finished: boolean; reporting: Promise<void>; controller: AbortController; presentation?: PresentationTicket; visualEndAt: number; emoteRevision: number };
  let avatar = $state<NativeWorldPort&{preparePresentation:(signal:AbortSignal)=>Promise<()=>void>;canPresent:(event:EmotePayload)=>boolean;present:(event:EmotePayload)=>boolean}>();
  let nativeWorld:NativeWorldOutput|undefined;
  let nativeWorldView=$state<NativeWorldView>({phase:'normal',exposed:false,covered:false});
  let nativeWorldSeen=$state(false);
  let nativeSnapshots:NativeWorldSnapshots|undefined;
  function acceptNativeWorld(value:unknown){
    if(native)nativeSnapshots?.hint(value);
  }
  function nativePosition(expected:BindingStamp){
    return nativeSnapshots?.position(expected)??null;
  }
  let profileLoaded=false;
  const worldActive=$derived(nativeWorldView.phase==='world');
  const nativeFixed=$derived(nativeWorldSeen||nativeWorldView.phase!=='normal');
  const admittedEmotes=(events:EmotePayload[]=[])=>events.filter(event=>avatar?.canPresent(event));
  let profile = $state<any>(cloneDefaults());
  let lifeOutput=$state<LifeOutput>();
  const lifeOutputVersions=new LifeOutputVersions();
  function acceptLifeOutput(value:unknown){try{const next=lifeOutputVersions.accept(value,botId);if(next){if(lifeOutput?.routeId&&(lifeOutput.routeId!==next.routeId||next.state!=='visible')){nativeWorld?.revoke();cancelCurrent();}if(next.state==='inactive')nativeWorldSeen=false;lifeOutput=next;}}catch{nativeWorld?.revoke();connectionFailure='Die Desktop-Ausgabe konnte nicht bestätigt werden.';}}
  const outputEnabled=$derived(lifeOutput&&lifeOutput.state!=='inactive'?native&&lifeOutput.state==='visible':profile.output.mode===(native?'overlay':'obs'));
  let presentation=$state<PresentationSnapshot>();
  const presentationVersions=new PresentationVersions();
  function acceptPresentation(value:unknown){if(presentationVersions.accept(value,botId))presentation=value;}
  const renderProfile=$derived(presentation?{...profile,avatar:{...profile.avatar,scale:presentation.scale}}:profile);
  const desktop=$derived(native&&presentation?.desktop.active?presentation.desktop:undefined);
  let anchor=$state<DesktopAnchor>();
  let captionWidth=$state(320),captionHeight=$state(80);
  const captionPosition=$derived(anchor?captionPlacement(anchor,{width:captionWidth,height:captionHeight},anchor.viewport,16):undefined);
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
  let modelAudioStop = $state(0);
  let modelSoundPlaying = false;
  let modelSoundActivity: SoundActivity = 'foreground';
  let effectSoundPlaying = false;
  function modelAudioPlaying(value:boolean,activity:SoundActivity){modelSoundPlaying=value;modelSoundActivity=activity;reportCombinedSound();}
  let emoteRevision = 0;
  const soundPlaybackReports = new SoundPlaybackReports();
  let soundPlaying = false;
  const presentationLeases = new PresentationLeases(() => soundPlaying);
  let manualController: AbortController | undefined;
  let soundReporting = Promise.resolve();
  let current: PlaybackJob | undefined;
  let disposed = false;
  let profileRevision = 0;
  let runtimeRevision = 0;
  let ready: Promise<void> = Promise.resolve();
  const receivedIds = new Set<string>();
  const emoteSequence = new EmoteSequence(applyEmote);
  const subtitles = new SubtitleLifetime((value) => { text = value; });

  async function prepareFigure(signal: AbortSignal): Promise<() => void> {
    // Native has already authorized/restored this speech route. The local
    // barrier must not depend on delayed evalRevoke arriving before speech.
    nativeWorld?.revoke();
    await tick();
    if (disposed || signal.aborted) throw new DOMException('Die Figurenpräsentation wurde abgebrochen.', 'AbortError');
    if (!avatar) throw new Error('Die Figur ist nicht für die Ausgabe verfügbar.');
    return avatar.preparePresentation(signal);
  }

  function applyEmote(event: EmotePayload) {
    return presentAdmittedEmote(avatar,event,()=>{profile.avatar.emote = {...event,
      action: event.action, nonce: event.nonce ?? `${Date.now()}-${Math.random()}`,
      duration: (event.duration ?? 2.5) * 1000, intensity: event.intensity ?? 1,
    };
    if(outputEnabled)emoteAudio?.play(event);});
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
    manualController?.abort();
    const controller = new AbortController(); manualController = controller;
    presentationLeases.clear(current?.presentation);
    modelAudioStop++;
    ++emoteRevision; const revision = emoteRevision;
    emoteSequence.stop(); emoteAudio?.stop();
    let presentation: PresentationTicket | undefined;
    try {
      await ready;
      if (disposed || revision !== emoteRevision || controller.signal.aborted) return;
      if(!avatar?.canPresent(event))return;
      if(outputEnabled)await emoteAudio?.prepare([event], resolveSound);
      if (disposed || revision !== emoteRevision || controller.signal.aborted) return;
      presentation = presentationLeases.request(prepareFigure, controller.signal);
      await presentation.ready;
      if (disposed || revision !== emoteRevision || controller.signal.aborted) { presentationLeases.cancel(presentation); return; }
      if(!applyEmote(event)){presentationLeases.cancel(presentation);return;}
      presentationLeases.finish(presentation, emoteDurationMilliseconds(event));
    } catch (error) {
      if (presentation) presentationLeases.cancel(presentation);
      if (disposed || revision !== emoteRevision || controller.signal.aborted || error instanceof Error && error.name === 'AbortError') return;
      failureDetail = error instanceof Error ? error.message : String(error);
      activationNeeded = false; failure = 'Ausgabehinweis schließen';
    }
  }

  function reportSoundPlaying(playing: boolean) {effectSoundPlaying=playing;reportCombinedSound();}
  function reportCombinedSound() {
    const playing=effectSoundPlaying||modelSoundPlaying;
    soundPlaying = playing;
    if (!playing) presentationLeases.soundEnded();
    const targetBot = botId;
    if (!targetBot) return;
    const reports = soundPlaybackReports.update(
      foregroundSoundPlaying(effectSoundPlaying, modelSoundPlaying, modelSoundActivity),
      modelSoundPlaying && modelSoundActivity === 'ambient',
      current?.started && !current.finished && current.emoteRevision === emoteRevision ? current.event.id : undefined,
    );
    for (const report of reports) {
      const payload = {botId: targetBot, output: native ? 'overlay' : 'obs', ...report};
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
    if (job.presentation) {
      if (error || !job.started) presentationLeases.cancel(job.presentation);
      else presentationLeases.finish(job.presentation, Math.max(0, job.visualEndAt - Date.now()));
    }
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
    manualController?.abort(); manualController = undefined;
    current?.controller.abort();
    presentationLeases.clear();
    modelAudioStop++;
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

  function speechRouteCurrent(event:SpeechEvent):boolean {
    if(event.presentationRouteId!==undefined&&event.presentationRouteId!==null)return native&&event.presentationOutput==='overlay'&&typeof event.presentationRouteId==='string'&&!!event.presentationRouteId&&lifeOutput?.state==='visible'&&lifeOutput.routeId===event.presentationRouteId;
    if(lifeOutput&&lifeOutput.state!=='inactive')return false;
    if(event.presentationOutput!==undefined&&event.presentationOutput!==(native?'overlay':'obs'))return false;
    return profile.output.mode===(native?'overlay':'obs');
  }
  async function speak(event: SpeechEvent) {
    if (disposed || !speechRouteCurrent(event)) return;
    cancelCurrent();
    const job: PlaybackJob = { event, started: false, finished: false, reporting: Promise.resolve(), controller: new AbortController(), visualEndAt: 0, emoteRevision };
    current = job;
    const plannedEmoteRevision = emoteRevision;
    failure = '';
    failureDetail = '';
    activationNeeded = false;
    let playbackError: string | undefined;
    const beforeStart = async () => {
      if (disposed || current !== job || job.finished || job.controller.signal.aborted || !speechRouteCurrent(event)) throw new DOMException('Die Figurenpräsentation wurde abgebrochen.', 'AbortError');
      job.presentation ??= presentationLeases.request(prepareFigure, job.controller.signal);
      await job.presentation.ready;
      if (disposed || current !== job || job.finished || job.controller.signal.aborted || !speechRouteCurrent(event)) throw new DOMException('Die Figurenpräsentation wurde abgebrochen.', 'AbortError');
    };
    const onStart = () => {
      if (disposed || current !== job || job.finished || job.started || !speechRouteCurrent(event)) return;
      job.started = true;
      emotion = event.emotion || 'neutral';
      subtitles.start(event.id, event.text);
      if(plannedEmoteRevision===emoteRevision){
        const emotes = admittedEmotes(Array.isArray(event.emotes) ? event.emotes : []);
        job.visualEndAt = Date.now() + emoteSequenceMilliseconds(emotes);
        emoteSequence.start(emotes);
      }
      playback(job, true);
    };
    try {
      await ready;
      if (disposed || current !== job || job.finished) return;
      if (!event.voice) throw new Error('Stimme des Wiedergabeauftrags fehlt.');
      const voice = { ...event.voice };
      if(plannedEmoteRevision===emoteRevision&&outputEnabled)await emoteAudio.prepare(admittedEmotes(Array.isArray(event.emotes) ? event.emotes : []), resolveSound);
      if (disposed || current !== job || job.finished) return;
      if (voice.enabled === false) { await beforeStart(); onStart(); return; }
      const speechText = voice.provider === 'elevenlabs' ? event.speechText || event.text : event.text;
      await player.speak(speechText, voice, async () => {
        if (native) return call('get_speech_audio', { id: event.id, botId: event.botId || botId });
        const url = new URL(`/speech/${encodeURIComponent(event.id)}`, overlayUrl);
        url.search = new URL(overlayUrl).search;
        if (botId) url.searchParams.set('bot', botId);
        const result = await fetch(url);
        if (!result.ok) throw new Error('Sprachausgabe nicht verfügbar. Einstellungen in der App prüfen.');
        return result.json();
      }, onStart, beforeStart);
      if (disposed || current !== job || job.finished) return;
      failure = '';
    } catch (error) {
      if (disposed || current !== job || job.finished) return;
      if (error instanceof Error && error.name === 'AbortError') return;
      failureDetail = error instanceof Error ? error.message : String(error);
      playbackError = failureDetail;
      activationNeeded = /not.allowed|autoplay|gesture|interact|suspend|permission|audio.?context/i.test(failureDetail);
      failure = activationNeeded ? 'Audio für die nächste Antwort aktivieren' : 'Ausgabehinweis schließen';
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
    if(event.kind==='life-output'){acceptLifeOutput(event.lifeOutput??event);return;}
    if(event.kind==='world-state'){acceptNativeWorld(event.world);return;}
    if(event.kind==='presentation'){acceptPresentation(event);return;}
    if (event.kind === 'profile') {
      nativeWorld?.revoke();
      cancelCurrent();
      botId ||= event.profile.id;
      acceptPresentation(event.presentation);
      if(event.lifeOutput)acceptLifeOutput(event.lifeOutput);
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
    if (event.kind === 'stop') { nativeWorld?.revoke();++runtimeRevision; autopilot = undefined; cancelCurrent(); failure = ''; failureDetail = ''; return; }
    if (event.kind === 'error') { subtitles.cancel(); connectionFailure = event.text || 'Die Verbindung zur App ist nicht verfügbar.'; return; }
    if (event.kind !== 'speech' || !event.id || receivedIds.has(event.id) || !speechRouteCurrent(event)) return;
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
    if (!native || desktop?.active || profile.output?.clickThrough || event.button !== 0) return;
    event.preventDefault(); event.stopPropagation();
    try { await getCurrentWindow().startDragging(); }
    catch (error) { failureDetail = error instanceof Error ? error.message : String(error); }
  }

  onMount(() => {
    document.documentElement.classList.add('overlay-document');
    player = new SpeechPlayer((value) => { level = value; }, (state) => { speaking = state; });
    emoteAudio = new EmoteAudioPlayer((message) => { soundNotice = message; soundActivationNeeded = /gesperrt|aktivieren|klick/i.test(message); },reportSoundPlaying);
    const soundHeartbeat=window.setInterval(()=>{if(soundPlaying&&!disposed)reportCombinedSound();},4000);
    const cleanup: (() => void)[] = [];
    if (native) {
      nativeSnapshots=new NativeWorldSnapshots({
        read:async actorId=>(await call('get_app_state',{botId:actorId})).world,
        current:(scope,signal)=>!disposed&&!signal.aborted&&profileLoaded&&botId===scope.actorId
          &&profile.id===scope.actorId&&outputEnabled&&document.visibilityState!=='hidden',
        revision:()=>`${profileRevision}:${lifeOutput?.routeId??''}:${lifeOutput?.state??''}`,
        mismatch:()=>nativeWorld?.revoke(),
      });
      nativeWorld=new NativeWorldOutput({invoke:call,adapter:()=>avatar,actorId:()=>botId,
        scope:(expected,signal)=>nativeSnapshots?.expect(expected,signal),
        position:nativePosition,
        current:()=>!disposed&&profileLoaded&&!!botId&&profile.id===botId&&outputEnabled&&document.visibilityState!=='hidden',
        normal:async(signal)=>{
          await ready;await tick();
          if(disposed||signal.aborted||current||!avatar)throw new DOMException('Die Figurenpräsentation wurde abgebrochen.','AbortError');
          cancelCurrent();await tick();
          if(disposed||signal.aborted||soundPlaying)throw new DOMException('Die Figurenpräsentation wurde abgebrochen.','AbortError');
        },changed:view=>{
          nativeWorldView=view;
          if(view.phase==='normal')nativeSnapshots?.invalidate();
          if(view.phase!=='normal'){nativeWorldSeen=true;modelAudioStop++;}
        },afterDOM:()=>tick()});
      const initialRevision = profileRevision;
      const initialRuntimeRevision = runtimeRevision;
      const subscribe=(name:string,handler:(event:any)=>void)=>onEvent(name,handler).then(stop=>{if(disposed)stop();else cleanup.push(stop);});
      const listeners=Promise.all([subscribe('cohost://life-output',acceptLifeOutput),subscribe('cohost://overlay',receive)]);
      ready = listeners.then(()=>call('get_app_state', { botId: botId || undefined })).then((state) => {
        if (disposed) return;
        botId ||= state.profile.id;
        if (profileRevision === initialRevision) profile = state.profile;
        profileLoaded=true;
        // This read began before a native scope was known. Its .world may be an
        // old epoch; the current native response schedules its own fresh read.
        acceptPresentation(state.presentation);
        if(state.lifeOutput)acceptLifeOutput(state.lifeOutput);
        if (runtimeRevision === initialRuntimeRevision) { const runtime=(state.autopilot || []).find((entry:any) => entry.botId === botId); if(runtime)acceptAutopilot(runtime); }
        const url = new URL(state.overlayUrl); url.searchParams.set('bot', botId); overlayUrl = url.toString();
      }).catch(() => { if (!disposed) connectionFailure = 'Charakterprofil konnte nicht geladen werden.'; });
      const target=window as Window&{__orbitpalNativeWorldBootstrap?:(auth:unknown)=>void;__orbitpalNativeWorldRevoke?:()=>void};
      const bootstrap=(auth:unknown)=>{if(window===window.top&&!disposed)nativeWorld?.bootstrap(auth);};
      const revoke=()=>nativeWorld?.revoke();
      if(window===window.top){
        Object.defineProperty(target,'__orbitpalNativeWorldBootstrap',{value:bootstrap,configurable:true});
        Object.defineProperty(target,'__orbitpalNativeWorldRevoke',{value:revoke,configurable:true});
        cleanup.push(()=>{if(target.__orbitpalNativeWorldBootstrap===bootstrap)delete target.__orbitpalNativeWorldBootstrap;
          if(target.__orbitpalNativeWorldRevoke===revoke)delete target.__orbitpalNativeWorldRevoke;});
      }
      void subscribe('cohost://world-action',event=>nativeWorld?.receive(event));
      const hidden=()=>{if(document.visibilityState==='hidden')nativeWorld?.revoke();};
      document.addEventListener('visibilitychange',hidden);window.addEventListener('pagehide',revoke);
      cleanup.push(()=>{document.removeEventListener('visibilitychange',hidden);window.removeEventListener('pagehide',revoke);});
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
      disposed = true;nativeSnapshots?.dispose();nativeWorld?.dispose();window.clearInterval(soundHeartbeat);cancelCurrent(); subtitles.destroy(); cleanup.forEach((close) => close()); player.destroy(); emoteAudio.destroy();
      document.documentElement.classList.remove('overlay-document');
    };
  });
</script>

<div class="overlay-scene">
  <Avatar bind:this={avatar} audioEnabled={outputEnabled&&!worldActive&&nativeWorldView.phase!=='warming'} audioStop={modelAudioStop} onAudioPlaying={modelAudioPlaying} profile={renderProfile} {speaking} {level} {emotion}
    autopilot={nativeFixed?undefined:autopilot} desktop={worldActive||nativeWorldView.phase==='warming'?undefined:desktop}
    motionSource={worldActive?'world':nativeFixed?'fixed':undefined}
    worldOwner={worldActive} worldChallenge={nativeWorldView.challenge} worldBinding={nativeWorldView.binding} worldExposed={nativeWorldView.exposed}
    worldMirrorPosition={nativeWorldView.position}
    onWorldPrepared={identity=>void nativeWorld?.prepared(identity)} onWorldMeasurement={report=>nativeWorld?.measurement(report)}
    onWorldFrame={frame=>nativeWorld?.frame(frame)} onWorldUnavailable={(identity,reason)=>nativeWorld?.unavailable(identity,reason)}
    onDesktopAnchor={value=>anchor=value} transparent={true} outputUrl={overlayUrl} assetUrl={assetUrl(profile.avatar.assetPath, overlayUrl)} />
  {#if nativeWorldView.covered}<div class="native-world-preparing" aria-hidden="true"></div>{/if}
  {#if native && !worldActive && !desktop?.active && !profile.output?.clickThrough}
    <button class="overlay-drag" aria-label={$t("Figurenfenster verschieben")} title={$t("Figurenfenster verschieben")} onpointerdown={drag}>
      <svg width="20" height="14" viewBox="0 0 20 14" fill="currentColor" aria-hidden="true"><circle cx="5" cy="4" r="1.4"/><circle cx="10" cy="4" r="1.4"/><circle cx="15" cy="4" r="1.4"/><circle cx="5" cy="10" r="1.4"/><circle cx="10" cy="10" r="1.4"/><circle cx="15" cy="10" r="1.4"/></svg>
    </button>
  {/if}
  {#if text && profile.output.showSubtitles && (!desktop?.active||anchor?.visible)}
    <div class="overlay-subtitle" class:desktop-caption={desktop?.active&&captionPosition!==undefined} bind:clientWidth={captionWidth} bind:clientHeight={captionHeight} style={desktop?.active&&captionPosition?`left:${captionPosition.left}px;top:${captionPosition.top}px;max-width:${captionPosition.maxWidth}px;max-height:${captionPosition.maxHeight}px`:undefined}><strong>{profile.name}</strong><span>{text}</span></div>
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
  .native-world-preparing{position:absolute;inset:0;background:#141923;pointer-events:none;z-index:20;}
  .overlay-subtitle.desktop-caption {bottom:auto;right:auto;align-items:center;width:max-content;overflow:hidden;transform:none;overflow-wrap:anywhere;}
  .desktop-caption>span{max-width:min(360px,calc(100vw - 32px));}
  .overlay-drag { position: absolute; top: 12px; right: 12px; display: grid; place-items: center; width: 36px; height: 29px; border: 1px solid #b2c6e54d; border-radius: 8px; background: #1d2c45c9; color: #d1dff6; cursor: grab; opacity: 0; transition: opacity 0.15s; backdrop-filter: blur(8px); }
  .overlay-scene:hover .overlay-drag, .overlay-drag:focus-visible { opacity: 1; }
  .overlay-drag:active { cursor: grabbing; }
  .overlay-connection { position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%); background: #243b5ced; border: 1px solid #8aa3c7; color: white; border-radius: 7px; padding: 9px 13px; max-width: 90%; text-align: center; font-size: 11px; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .overlay-drag { transition: none; } }
</style>
