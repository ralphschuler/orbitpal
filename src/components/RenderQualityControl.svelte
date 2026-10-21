<script lang="ts">
  import {onMount} from 'svelte';
  import {call,native} from '../lib/api';
  import {t} from '../lib/i18n';
  import type {RenderQuality} from '../lib/avatar-render-policy';
  import {acceptRenderSettings,renderSettingsByBot} from '../lib/render-settings';
  let {botId,disabled=false}:{botId:string;disabled?:boolean}=$props();
  let draft=$state<RenderQuality>('balanced'),pending=$state(false),error=$state(''),loaded=$state(false);
  let alive=false,epoch=0;
  const uid=$props.id();
  const current=$derived($renderSettingsByBot[botId]);
  async function load(){const id=botId,token=++epoch;loaded=false;error='';try{const value=await call('get_render_settings',{botId:id});if(alive&&token===epoch&&id===botId){const snapshot=acceptRenderSettings(value,id)??$renderSettingsByBot[id];draft=snapshot.quality;loaded=true;}}catch{if(alive&&token===epoch)error='Das Renderprofil konnte nicht übernommen werden. Bitte neu laden.';}}
  $effect(()=>{botId;if(alive&&native)void load();});
  async function apply(){if(!current||!loaded||pending||disabled)return;const id=botId,token=++epoch;pending=true;error='';try{const value=await call('set_render_quality',{botId:id,expectedRevision:current.revision,quality:draft});if(alive&&id===botId&&token===epoch){const snapshot=acceptRenderSettings(value,id)??$renderSettingsByBot[id];draft=snapshot.quality;}}catch{if(alive&&id===botId)error='Das Renderprofil konnte nicht übernommen werden. Bitte neu laden.';}finally{if(alive)pending=false;}}
  onMount(()=>{alive=true;if(native)void load();return()=>{alive=false;++epoch;};});
</script>
<div class="render-quality" data-render-settings-control><label for={`${uid}-quality`}>{$t('Renderprofil')}<select id={`${uid}-quality`} bind:value={draft} disabled={!native||!loaded||pending||disabled}><option value="balanced">{$t('Ausgewogen')}</option><option value="energy">{$t('Energiesparend · höchstens 20 FPS')}</option></select></label><button class="button secondary small" disabled={!native||!loaded||pending||disabled||draft===current?.quality} onclick={apply}>{$t('Renderprofil übernehmen')}</button><p class="field-note">{$t('Gilt sofort für die Ausgaben dieser Figur und bleibt lokal gespeichert. Laufende Aktionen und Sprache werden nicht unterbrochen. Importierte oder kopierte Figuren beginnen ausgewogen.')}</p>{#if error}<p role="alert" class="field-note">{$t(error)} <button class="text-link" disabled={pending||disabled} onclick={load}>{$t('Neu laden')}</button></p>{/if}</div>
<style>.render-quality{display:grid;gap:8px;margin-bottom:18px}.render-quality label{display:grid;gap:8px}.render-quality>button{justify-self:start}.field-note{font-size:11px;line-height:1.7;color:var(--muted)}</style>
