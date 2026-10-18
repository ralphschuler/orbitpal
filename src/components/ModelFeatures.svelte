<script lang="ts">
  import { Play, Plus, CircleHelp } from '@lucide/svelte';
  interface Animation { name: string; duration: number; }
  interface Props {
    info?: any;
    animations: Animation[];
    status: 'loading' | 'ready' | 'error';
    error?: string;
    metadataError?: string;
    disabled?: boolean;
    procedural: string[];
    onPreview: (action: string, duration: number) => void;
    onAddEmote: (action: string, duration: number, name: string) => void;
  }
  let { info, animations, status, error = '', metadataError = '', disabled = false, procedural, onPreview, onAddEmote }: Props = $props();
  let selected = $state('');
  const gestures = [{ name: 'Winken', action: 'wave', duration: 3 }, { name: 'Freuen', action: 'celebrate', duration: 3 }, { name: 'Nicken', action: 'nod', duration: 2 }, { name: 'Tanzen', action: 'dance', duration: 4 }, { name: 'Verstecken', action: 'hide', duration: 3 }];
  const options = $derived(animations.map(clip => ({ name: clip.name, action: clip.name, duration: clip.duration })).concat(gestures.filter(gesture => procedural.includes(gesture.action) && !animations.some(clip => clip.name.toLowerCase() === gesture.action))));
  const current = $derived(options.find(option => option.action === selected) || options[0]);
  const warnings = $derived((info?.warnings || []) as string[]);
  const formatLabels: Record<string,string> = {builtin:'App-Figur',glb2:'GLB 2.0',glb:'GLB 2.0',vrm0:'VRM 0.x',vrm1:'VRM 1.0'};
</script>

<section class="settings-panel model-features" aria-label="Funktionen dieser Figur">
  <div class="model-heading"><div><h2>Was deine Figur mitbringt</h2><p>Haltung und Bewegungen werden automatisch an das Modell angepasst.</p></div><span class="model-status" class:ready={status === 'ready'}>{status === 'ready' ? 'Geladen' : status === 'error' ? 'Nicht verfügbar' : 'Wird geladen …'}</span></div>
  {#if status === 'error'}<p class="model-warning" role="status"><CircleHelp size={16}/>{error || 'Die Figur konnte nicht vollständig geladen werden.'}</p>
  {:else}
    <div class="feature-facts"><span>{formatLabels[info?.format] || '3D-Figur'}</span><span>{animations.length} eigene Animationen</span><span>{info?.audioExtensions?.length ? 'Audio-Erweiterung erkannt · Hinweise beachten' : info ? 'Keine eingebetteten Sounds erkannt' : 'Modelldaten werden geprüft …'}</span></div>
    {#if status === 'ready' && current}<div class="animation-preview"><label for="model-animation-preview">Animation ansehen<select id="model-animation-preview" value={current.action} onchange={event => selected = event.currentTarget.value}>{#each options as option, index (index)}<option value={option.action}>{option.name}{animations.some(clip => clip.name === option.action) ? ' · aus dem Modell' : ' · App-Geste'}</option>{/each}</select></label><button class="button secondary small" disabled={disabled} onclick={() => onPreview(current.action, current.duration)}><Play size={14}/>Stumme Vorschau</button><button class="button secondary small" disabled={disabled} onclick={() => onAddEmote(current.action, current.duration, current.name)}><Plus size={14}/>Als Emote übernehmen</button></div>{/if}
    {#if status === 'ready' && !animations.length}<p class="model-note">Diese Figur enthält keine eigenen Animationsclips. {procedural.length ? 'Die App erzeugt passende Gesten automatisch.' : 'Verfügbare Gesten hängen vom vorhandenen Skelett ab.'}</p>{/if}
    <p class="model-note">Einen Sound kannst du einem Emote unter „Eigene Emotes“ zuordnen. Sounds gehören nicht zum Grundumfang einer VRM- oder GLB-Datei.</p>
    {#each warnings as warning}<p class="model-warning"><CircleHelp size={15}/>{warning}</p>{/each}
    {#if metadataError}<p class="model-warning" role="status"><CircleHelp size={15}/>{metadataError}</p>{/if}
  {/if}
</section>

<style>
  .model-features { margin-block: 20px; }
  .model-heading { display: flex; align-items: flex-start; gap: 20px; justify-content: space-between; }
  h2 { font-size: 15px; margin: 0; }
  .model-heading p { color: #9daac0; font-size: 11px; margin: 7px 0 0; line-height: 1.7; }
  .model-status { padding: 5px 9px; border: 1px solid #53607a; border-radius: 5px; color: #b8c5dc; font-size: 10px; white-space: nowrap; }
  .model-status.ready { border-color: #476b62; color: #a3d5c3; }
  .feature-facts { display: flex; flex-wrap: wrap; gap: 8px; margin: 18px 0; }
  .feature-facts span { background: #182231; border-radius: 5px; padding: 7px 10px; font-size: 10px; color: #bdc9de; }
  .animation-preview { display: flex; align-items: flex-end; gap: 9px; flex-wrap: wrap; }
  .animation-preview label { min-width: 210px; flex: 1; color: #becbdf; font-size: 11px; }
  select { display: block; margin-top: 8px; width: 100%; }
  .model-note { margin: 12px 0 0; color: #98a8c0; font-size: 11px; line-height: 1.8; }
  .model-warning { display: flex; align-items: flex-start; gap: 8px; color: #efc1a4; font-size: 11px; line-height: 1.8; margin: 14px 0 0; }
  .model-warning :global(svg) { flex-shrink: 0; margin-top: 2px; }
</style>
