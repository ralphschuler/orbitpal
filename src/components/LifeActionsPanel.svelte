<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { Hourglass, RefreshCw } from '@lucide/svelte';
  import { call, native, onEvent, prettyError } from '../lib/api';
  import { t, locale, formatNumber } from '../lib/i18n';
  import { activityLabels, phaseLabels } from '../lib/life-runtime';
  import { actionReasons, actionStatusLabels, actionFailureLabels, cancellationLabels, parseLifeActions, type LifeActionsSnapshot } from '../lib/life-actions';
  import HelpTip from './HelpTip.svelte';
  interface Props { botId:string;botName:string;disabled?:boolean;onQuietRest:()=>Promise<LifeActionsSnapshot|undefined>; }
  let { botId,botName,disabled=false,onQuietRest }:Props=$props();
  const uid=$props.id();
  let snapshot=$state<LifeActionsSnapshot>(),pending=$state(false),starting=$state(false),unconfirmed=$state(false),error=$state('');
  let alive=false,operation=0,lastBot='',viewEpoch=0;
  const canStart=$derived(native&&!disabled&&!pending&&!starting&&!unconfirmed&&snapshot?.availability==='ready'&&(!snapshot.runtime||['completed','cancelled','failed'].includes(snapshot.runtime.status)));
  const seconds=$derived(snapshot?.runtime?formatNumber(snapshot.runtime.activeMilliseconds/1000,$locale,{maximumFractionDigits:3}):'');
  function accept(next:LifeActionsSnapshot,fromEvent=false){
    if(next.botId!==botId)return;
    // An event may be delayed from an old native instance. It can revoke
    // freshness, but only a read begun after that event establishes an epoch.
    if(fromEvent&&(!snapshot||next.instanceId!==snapshot.instanceId)){++viewEpoch;unconfirmed=true;if(!pending&&!starting)void refresh();return;}
    if(snapshot?.instanceId===next.instanceId&&next.sequence<snapshot.sequence)return;
    snapshot=next;unconfirmed=false;error='';
  }
  async function refresh(){
    if(!native||pending||starting)return;
    const target=botId,ticket=++operation,epoch=viewEpoch;pending=true;
    try{const next=parseLifeActions(await call('get_life_actions',{botId:target}),target);if(alive&&target===botId&&ticket===operation&&epoch===viewEpoch)accept(next);}
    catch(cause){if(alive&&target===botId&&ticket===operation){error=prettyError(cause);unconfirmed=true;}}
    finally{if(alive&&ticket===operation){pending=false;if(epoch!==viewEpoch)void refresh();}}
  }
  async function start(){
    if(!canStart)return;
    const target=botId,ticket=++operation,epoch=viewEpoch;starting=true;error='';
    try{const next=await onQuietRest();if(alive&&target===botId&&ticket===operation&&epoch===viewEpoch&&next)accept(next);}
    catch(cause){if(alive&&target===botId&&ticket===operation){error=prettyError(cause);unconfirmed=true;}}
    finally{if(alive&&ticket===operation){starting=false;if(epoch!==viewEpoch)void refresh();}}
  }
  $effect(()=>{const target=botId;untrack(()=>{if(!alive||target===lastBot)return;lastBot=target;++operation;++viewEpoch;snapshot=undefined;pending=false;starting=false;unconfirmed=false;error='';void refresh();});});
  onMount(()=>{
    alive=true;lastBot=botId;
    const listener=onEvent('cohost://life-actions',value=>{if(!alive||!value||typeof value!=='object'||(value as Record<string,unknown>).botId!==botId)return;try{accept(parseLifeActions(value,botId),true);}catch(cause){error=prettyError(cause);unconfirmed=true;}}).catch(cause=>{if(alive){error=prettyError(cause);unconfirmed=true;}return()=>{};});
    void refresh();const timer=setInterval(()=>void refresh(),5000);
    return()=>{alive=false;++operation;clearInterval(timer);void listener.then(stop=>stop());};
  });
</script>

<section class="settings-panel life-actions" aria-labelledby={`${uid}-title`}>
  <div class="actions-heading"><div class="section-heading"><Hourglass size={20}/><div><h2 id={`${uid}-title`}>{$t('Aktivitäten für {name}',{name:botName})}</h2><p>{$t('Letztes gespeichertes Ergebnis und aktuelle Aktivität.')}</p></div></div><button class="button secondary small" onclick={refresh} disabled={!native||pending||starting}><RefreshCw size={14}/>{$t('Aktivitäten aktualisieren')}</button></div>
  {#if !native}<p class="field-note">{$t('Aktivitäten werden in der Desktop-App verwaltet.')}</p>{:else}
    {#if !snapshot}<p role="status">{$t(pending?'Aktivitäten werden geladen …':'Aktivitäten noch nicht bestätigt.')}</p>{:else}
      <div class="action-states">
        <div><h3>{$t('Zuletzt gespeicherte Aktivität')}<HelpTip text={$t('Ein Abschluss wird erst angezeigt, wenn er gespeichert ist. Das Versenden einer Animation oder ein abgelaufener Timer reicht dafür nicht.')}/></h3>
          {#if snapshot.confirmed}<p><strong>{$t(activityLabels[snapshot.confirmed.activity])}</strong> · {$t(phaseLabels[snapshot.confirmed.phase])}</p><p class="field-note">{$t('Aktivität {number} · gespeicherter Stand {revision}',{number:formatNumber(snapshot.confirmed.actionId.ordinal,$locale),revision:formatNumber(snapshot.confirmed.revision,$locale)})}</p>{#if snapshot.confirmed.cancellation}<p class="field-note">{$t(cancellationLabels[snapshot.confirmed.cancellation])}</p>{/if}{:else}<p class="field-note">{$t('Noch keine Aktivität gespeichert.')}</p>{/if}
        </div>
        <div><h3>{$t('Aktuelle Aktivität')}</h3>{#if snapshot.runtime}<p><strong>{$t(snapshot.runtime.presentation==='native_rest_dwell'?'Stille Pause (lokal)':'Visuelle Aktion')}</strong> · {$t(actionStatusLabels[snapshot.runtime.status])}</p><p class="field-note">{$t('Aktive Dauer: {seconds} s.',{seconds})}</p>{#if snapshot.runtime.failure}<p class="action-warning">{$t(actionFailureLabels[snapshot.runtime.failure])}</p>{/if}{:else}<p class="field-note">{$t('Gerade keine Aktivität.')}</p>{/if}</div>
      </div>
      {#if snapshot.reason}<p class="action-warning" role="status">{$t(actionReasons[snapshot.reason])}</p>{/if}
    {/if}
    <div class="rest-control"><button class="button secondary" onclick={start} disabled={!canStart} aria-describedby={`${uid}-rest`}><Hourglass size={15}/>{$t(starting?'Pause wird angefordert …':'Stille Pause · 10 Sekunden')}</button><p id={`${uid}-rest`} class="field-note">{$t('Startet eine einmalige Pause von 10 aktiven Sekunden. Life muss bereits laufen. Die Pause hat keine eigene Animation und füllt keine Bedürfnisse auf. Sie benötigt weder KI-Modell noch Stimme. Mit Stop brichst du sie ab. Die Pause startet keine Folgeaktivität.')}</p></div>
    <p class="field-note">{$t('Gehen, Aufladen, Spielen, Umschauen und Winken lassen sich hier noch nicht als eigene Aktivitäten starten. Nicht verfügbare Bewegungen werden nicht als Erfolg angezeigt.')}</p>
    {#if unconfirmed}<p class="action-warning" role="status">{$t('Der Aktivitätsstatus ist noch nicht bestätigt. Angezeigt wird der letzte bekannte Stand. Neue Aktivitäten bleiben gesperrt.')}</p>{/if}
    {#if error}<p class="action-error" role="alert">{error}</p>{/if}
  {/if}
</section>

<style>
  .life-actions{margin-bottom:20px}.actions-heading{display:flex;justify-content:space-between;align-items:flex-start;gap:20px}.actions-heading button{flex-shrink:0}.section-heading p,.field-note{color:var(--muted);font-size:12px;line-height:1.7}.action-states{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px;margin:18px 0}.action-states>div{padding:14px;background:var(--input-bg);min-width:0}.action-states h3{display:flex;align-items:center;gap:7px;margin-bottom:10px}.action-states p{font-size:12px;line-height:1.7;overflow-wrap:anywhere}.rest-control{border-top:1px solid var(--line);padding-top:18px;margin-top:18px}.rest-control p{margin-top:12px;max-width:90ch}.action-warning,.action-error{padding:12px 14px;border-left:3px solid var(--warning);background:var(--input-bg);font-size:12px;line-height:1.7;margin:12px 0}.action-error{color:var(--danger);border-color:var(--danger)}@media(max-width:680px){.actions-heading{flex-direction:column;gap:0}.actions-heading button{margin-bottom:14px}.action-states{grid-template-columns:minmax(0,1fr)}}
</style>
