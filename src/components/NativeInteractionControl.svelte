<script lang="ts">
  import {onMount} from 'svelte';
  import {call,onEvent,native} from '../lib/api';
  import {t,locale} from '../lib/i18n';
  import {parseInteraction} from '../lib/experience';
  let {botId,active,targetActorId=null,disabled=false}:{botId:string;active:boolean;targetActorId?:string|null;disabled?:boolean}=$props();
  let pending=$state(false),feedback=$state('');let alive=false;
  async function openMenu(){if(!active||pending||disabled)return;const id=botId;pending=true;feedback='';try{await call('open_native_character_menu',{botId:id,language:$locale,targetActorId});}catch{if(alive&&id===botId)feedback='Das native Figurenmenü ist derzeit nicht verfügbar.';}finally{if(alive)pending=false;}}
  async function closeMenu(){try{await call('close_native_character_menu',{botId});}catch{/* The native lease also expires and invalidates on lifecycle changes. */}}
  onMount(()=>{alive=true;let stop=()=>{};void onEvent('experience-interaction-result',raw=>{if(raw?.botId!==botId)return;try{const result=parseInteraction(raw,botId,raw.kind);feedback=result.status==='accepted'?'Aktion angenommen. Die Ausführung wird separat bestätigt.':result.status==='waiting'?'Die Figur wartet.':result.status==='declined'?'Die Figur hat die Anfrage abgelehnt.':'Die Aktion wurde abgebrochen.';}catch{feedback='Das native Figurenmenü ist derzeit nicht verfügbar.';}}).then(fn=>{if(alive)stop=fn;else fn();});const escape=(event:KeyboardEvent)=>{if(event.key==='Escape')void closeMenu();};window.addEventListener('keydown',escape);return()=>{alive=false;stop();window.removeEventListener('keydown',escape);void closeMenu();};});
</script>
<div class="native-interaction"><button class="button secondary" disabled={!native||!active||pending||disabled} onclick={openMenu}>{$t('Menü an der nativen Figur öffnen')}</button><p>{$t('Ein echtes Systemmenü mit Tastatursteuerung. Das Figurenfenster bleibt durchklickbar; Escape schließt das Menü.')}</p>{#if feedback}<p role="status">{$t(feedback)}</p>{/if}</div>
<style>.native-interaction{margin-top:14px}.native-interaction p{font-size:11px;line-height:1.7;color:var(--muted);max-width:65ch;margin:8px 0}</style>
