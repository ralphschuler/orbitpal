<script lang="ts">
  import { Play, Plus, Upload, Trash2 } from '@lucide/svelte';
  import { t, locale, formatNumber } from '../lib/i18n';
  import { motionAction, type MotionAsset } from '../lib/avatar-motion';
  import HelpTip from './HelpTip.svelte';
  interface Props {
    motions: MotionAsset[];
    compatible: boolean;
    availableActions?: string[];
    disabled?: boolean;
    onImport: () => Promise<void>;
    onRemove: (id: string) => void;
    onPreview: (action: string, duration: number) => void;
    onAddEmote: (action: string, duration: number, name: string) => void;
  }
  let { motions = [], compatible, availableActions = [], disabled = false, onImport, onRemove, onPreview, onAddEmote }: Props = $props();
  let importing = $state(false);
  async function importFile() { importing = true; try { await onImport(); } finally { importing = false; } }
</script>

<section class="settings-panel motion-library" aria-label={$t('Separate Bewegungen')}>
  <div class="heading"><div><h2>{$t('Separate Bewegungen')} <HelpTip text={$t('VRMA 1.0 überträgt Körperbewegungen, Ausdrücke und Blickrichtung auf VRM 0/1. Die Datei gehört diesem Bot und wird im Profilpaket mitgenommen. GLB-Figuren und die App-Figur können diese Bewegungen nicht verwenden.')}/></h2><p>{$t('Importiere eigene VRMA-Dateien und verwende ihre Clips als Emotes.')}</p></div><button class="button secondary small" disabled={disabled||importing||motions.length>=32} onclick={importFile}><Upload size={14}/>{importing?$t('Wird geladen …'):$t('VRMA importieren')}</button></div>
  {#if !compatible}<p class="notice">{$t('Wähle eine VRM-0- oder VRM-1-Figur, um VRMA-Bewegungen abzuspielen. Die Dateien bleiben diesem Bot zugewiesen.')}</p>{/if}
  {#if !motions.length}<p>{$t('Noch keine separaten Bewegungen. Eingebettete Modellanimationen findest du darüber.')}</p>{/if}
  {#each motions as motion (motion.id)}<div class="motion"><div class="motion-heading"><strong>{motion.name}</strong><button class="icon-button danger-text" disabled={disabled||importing} title={$t('Bewegung und zugehörige Emotes entfernen')} onclick={()=>onRemove(motion.id)}><Trash2 size={14}/></button></div>
    {#each motion.clips as clip}<div class="clip"><div><strong>{clip.name} <span>{formatNumber(clip.duration,$locale,{maximumFractionDigits:2})} s</span></strong><small>{clip.body.length?$t('Körper'):''}{clip.expressions.length?(clip.body.length?' · ':'')+$t('Ausdrücke'):''}{clip.lookAt?((clip.body.length||clip.expressions.length)?' · ':'')+$t('Blickrichtung'):''}</small></div><button class="button secondary small" disabled={disabled||importing||!compatible||!availableActions.includes(motionAction(motion.id,clip.index))} onclick={()=>onPreview(motionAction(motion.id,clip.index),clip.duration)}><Play size={13}/>{$t('Stumme Vorschau')}</button><button class="button secondary small" disabled={disabled||importing||!compatible||!availableActions.includes(motionAction(motion.id,clip.index))} onclick={()=>onAddEmote(motionAction(motion.id,clip.index),clip.duration,clip.name)}><Plus size={13}/>{$t('Als Emote übernehmen')}</button></div>{/each}
  </div>{/each}
  <p>{$t('Originaltempo und natürliche Cliplänge bleiben erhalten. Maximal 32 Dateien, 16 MiB pro Datei, 64 MiB insgesamt und 120 Sekunden pro Clip. Emote-Sounds bleiben separat.')}</p>
</section>

<style>
  .motion-library { margin-block:20px; }
  .heading,.motion-heading,.clip { display:flex; align-items:center; justify-content:space-between; gap:12px; }
  .heading { align-items:flex-start; flex-wrap:wrap; }
  h2 { font-size:15px; margin:0; }
  p,.clip small { color:#9daac0; font-size:11px; line-height:1.7; }
  .notice { color:#efc1a4; }
  .motion { border-top:1px solid #344158; padding-top:10px; margin-top:16px; }
  .motion-heading strong { font-size:12px; }
  .clip { flex-wrap:wrap; margin-block:10px; }
  .clip>div { flex:1; min-width:180px; }
  .clip strong { font-size:12px; }
  .clip strong span { font-weight:400; color:#9daac0; margin-left:5px; }
  .clip small { display:block; }
</style>
