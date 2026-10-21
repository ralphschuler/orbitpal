<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount, tick, untrack } from 'svelte';
  import { AvatarStage, type AvatarModelInfo, type DesktopAnchor } from '../lib/avatar';
  import type { EmotePayload } from '../lib/presentation';
  import type { SoundActivity } from '../lib/sound-playback';
  import type { AutopilotVisualConfig } from '../lib/autopilot-motion';
  import { presentationAbortError, type PresentationRelease } from '../lib/avatar-presentation';
  import type { DesktopLayout } from '../lib/output-presentation';
  import type { WorldAvatarOptions, WorldStationFrame } from '../lib/world-avatar';
  import { activityLabels } from '../lib/life-runtime';
  import type { MotionFence, WorldMotionCommand, ReceiveResult } from '../lib/world-motion';
  import type { WorldAdapterReason } from '../lib/world-output';

  interface Props extends WorldAvatarOptions {
    motionSource?: 'fixed'|'ambient'|'world';
    profile: any;
    speaking?: boolean;
    level?: number;
    emotion?: string;
    transparent?: boolean;
    assetUrl?: string;
    outputUrl?: string;
    onModelLoaded?: (info: AvatarModelInfo) => void;
    autopilot?: AutopilotVisualConfig;
    desktop?:DesktopLayout;
    onDesktopAnchor?:(anchor:DesktopAnchor|undefined)=>void;
    audioEnabled?: boolean;
    audioStop?: number;
    onAudioPlaying?: (playing: boolean, activity: SoundActivity) => void;
  }

  let { profile, speaking = false, level = 0, emotion = 'neutral', transparent = false, assetUrl, outputUrl = '', onModelLoaded, autopilot, desktop, onDesktopAnchor, audioEnabled = false, audioStop = 0, onAudioPlaying,
    motionSource,worldOwner=false,worldChallenge,worldBinding,worldExposed=false,worldMirrorPosition,
    onWorldPrepared,onWorldMeasurement,onWorldFrame,onWorldUnavailable,onWorldActivity }: Props = $props();
  let stationActivity = $state<WorldStationFrame>();
  function observeWorldActivity(value: WorldStationFrame | undefined) {
    stationActivity = value; onWorldActivity?.(value);
  }
  let host: HTMLDivElement;
  let stage = $state<AvatarStage>();
  let failure = $state('');
  let unavailable = $state(false);
  let loading = $state(false);
  let emoteFailure = $state('');
  let assetRequest = 0;
  let disposed = false;

  export async function preparePresentation(signal: AbortSignal): Promise<PresentationRelease> {
    if (signal.aborted || disposed) throw presentationAbortError();
    await tick();
    if (signal.aborted || disposed) throw presentationAbortError();
    const renderer = stage;
    if (!renderer || unavailable) throw new Error('Die Figur ist nicht für die Ausgabe verfügbar.');
    return renderer.preparePresentation(signal);
  }
  export function canPresent(event:EmotePayload):boolean { return stage?.canPresentEmote(event,profile)??false; }
  export function present(event:EmotePayload):boolean { return stage?.presentEmote(event,profile)??false; }
  export function acceptWorldCommand(command:WorldMotionCommand):ReceiveResult {return stage?.acceptWorldCommand(command)??'stale';}
  export function beginWorldAfterDelivered(fence:MotionFence,offerSequence:number):boolean {return stage?.beginWorldAfterDelivered(fence,offerSequence)??false;}
  export function confirmWorldStarted(fence:MotionFence,firstBehaviorFrameSequence:number):boolean {return stage?.confirmWorldStarted(fence,firstBehaviorFrameSequence)??false;}
  export function cancelWorld(reason:WorldAdapterReason='cancelled'):void {stage?.cancelWorld(reason);}
  export async function prepareNativeWorldWarm(signal:AbortSignal):Promise<{frameSequence:number;release:PresentationRelease}>{
    if(signal.aborted||disposed)throw presentationAbortError();
    await tick();
    const renderer=stage;
    if(signal.aborted||disposed||!renderer||unavailable)throw presentationAbortError();
    return renderer.prepareNativeWorldWarm(signal);
  }

  onMount(() => {
    try {
      const renderer = new AvatarStage(host, { profile, speaking, level, emotion, transparent, autopilot, desktop, onDesktopAnchor, audioEnabled, audioStop, onAudioPlaying,
        motionSource,worldOwner,worldChallenge,worldBinding,worldExposed,worldMirrorPosition,onWorldPrepared,onWorldMeasurement,onWorldFrame,onWorldUnavailable,onWorldActivity:observeWorldActivity }, (message) => { emoteFailure = message; }, assetUrl);
      stage = renderer;
      return () => { disposed = true; ++assetRequest; renderer.dispose(); };
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
      unavailable = true;
      onModelLoaded?.({ status: 'error', animations: [], proceduralGestures: [], error: failure });
    }
  });

  $effect(() => {
    stage?.update({ profile, speaking, level, emotion, transparent, autopilot, desktop, onDesktopAnchor, audioEnabled, audioStop, onAudioPlaying,
      motionSource,worldOwner,worldChallenge,worldBinding,worldExposed,worldMirrorPosition,onWorldPrepared,onWorldMeasurement,onWorldFrame,onWorldUnavailable,onWorldActivity:observeWorldActivity });
  });

  $effect(() => {
    const renderer = stage;
    const url = assetUrl;
    const motionKey = JSON.stringify(profile?.motions || []);
    const botId = profile?.id || '';
    const endpoint = outputUrl;
    if (!renderer) return;
    const request = ++assetRequest;
    const current = () => !disposed && renderer === stage && request === assetRequest;
    failure = '';
    loading = Boolean(url);
    untrack(() => onModelLoaded?.({ status: 'loading', animations: [], proceduralGestures: [] }));
    renderer.loadAsset(url,JSON.parse(motionKey),endpoint,botId).then(() => {
      if (current()) onModelLoaded?.(renderer.getModelInfo());
    }).catch((error) => {
      if (current()) {
        failure = error instanceof Error ? error.message : String(error);
        onModelLoaded?.({ status: 'error', animations: [], proceduralGestures: [], error: failure });
      }
    }).finally(() => {
      if (current()) loading = false;
    });
  });
</script>

<svelte:window onpointerdown={() => { if (audioEnabled) void stage?.unlockModelAudio(); }} onkeydown={() => { if (audioEnabled) void stage?.unlockModelAudio(); }}/>

<div class="avatar-stage" class:transparent role="img" aria-label={$t('{name}, animierter 3D-Charakter', { name: profile?.name || 'Nova' })}>
  <div class="canvas-host" bind:this={host}></div>
  {#if unavailable}
    <div class="render-fallback">
      <span class="fallback-mark">N</span>
      <strong>{$t("3D-Darstellung nicht verfügbar")}</strong>
      <span>{$t(failure)}</span>
    </div>
  {:else if loading}
    <div class="asset-badge" role="status"><span class="spinner"></span> {$t("Figur wird geladen")}</div>
  {:else if failure}
    <div class="asset-badge error" role="status" title={$t(failure)}>{$t("Import fehlgeschlagen · Nova wird angezeigt")}<span>{$t(failure)}</span></div>
  {:else if emoteFailure}
    <div class="asset-badge error" role="status" title={$t(emoteFailure)}>{$t("Emote nicht verfügbar")}<span>{$t(emoteFailure)}</span></div>
  {/if}
  {#if worldOwner && worldExposed && stationActivity && !unavailable && !loading && !failure}
    <div class="world-activity" aria-hidden="true" style:left={`${stationActivity.position.x}px`} style:top={`${stationActivity.position.y}px`}>
      <span>{$t((activityLabels as Record<string,string>)[stationActivity.activity] ?? 'Aktuelle Aktivität')}</span>
      <span class="world-activity-track"><span style:width={`${Math.min(100, stationActivity.activeMs / stationActivity.minimumActiveMs * 100)}%`}></span></span>
    </div>
  {/if}
</div>

<style>
  .avatar-stage { width: 100%; height: 100%; min-height: 180px; position: relative; isolation: isolate; }
  .canvas-host { width: 100%; height: 100%; position: absolute; inset: 0; outline: none; }
  .asset-badge { position: absolute; left: 50%; bottom: 20px; transform: translateX(-50%); max-width: calc(100% - 36px); display: flex; align-items: center; gap: 8px; border: 1px solid #dce1ec; padding: 8px 12px; border-radius: 10px; color: #626d83; background: #fffffff0; font-size: 11px; line-height: 1.5; box-shadow: 0 3px 14px #1d27450b; }
  .error { flex-direction: column; gap: 2px; text-align: center; color: #8d5336; }
  .error span { max-width: 320px; font-size: 10px; opacity: 0.8; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .spinner { width: 10px; height: 10px; flex: 0 0 auto; border: 1.5px solid #cdd6e6; border-top-color: #748ad4; border-radius: 50%; animation: spin 0.8s linear infinite; }
  .render-fallback { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; padding: 28px; text-align: center; color: #647085; font-size: 12px; }
  .render-fallback > span:last-child { max-width: 300px; line-height: 1.6; }
  .fallback-mark { display: grid; place-items: center; width: 68px; height: 68px; background: #e0e7fc; color: #7b87c1; border-radius: 22px; font-size: 28px; font-weight: 600; }
  .transparent .asset-badge { bottom: 4px; }
  .world-activity { position:absolute; transform:translate(-50%,-100%); display:grid; gap:4px; width:112px; padding:6px 9px; border-radius:9px; pointer-events:none; color:#eef2ff; background:#20283be8; font-size:11px; text-align:center; }
  .world-activity-track { height:3px; overflow:hidden; border-radius:2px; background:#ffffff30; }
  .world-activity-track > span { display:block; height:100%; background:var(--accent,#a6b7ef); }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
</style>
