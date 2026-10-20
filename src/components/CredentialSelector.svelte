<script lang="ts">
  import { onMount } from 'svelte';
  import { KeyRound } from '@lucide/svelte';
  import { call, native } from '../lib/api';
  import { t } from '../lib/i18n';
  import HelpTip from './HelpTip.svelte';
  import { ApiBindingReads, apiAccessError, type ApiBinding, type ApiPurpose, type ApiRequest, type ApiSnapshot } from '../lib/api-access';
  let { botId, purpose, config, snapshot, disabled=false, onChange, onManage,
    request=call, desktop=native }: { botId:string;purpose:ApiPurpose;config:Record<string,unknown>;
    snapshot?:ApiSnapshot;disabled?:boolean;onChange:(reference:string)=>void;onManage:()=>void;
    request?:ApiRequest;desktop?:boolean }=$props();
  const reads=new ApiBindingReads();
  let mounted=$state(false),pending=$state(false),error=$state(''),binding=$state<ApiBinding>();
  const signature=$derived(JSON.stringify([botId,purpose,config,snapshot?.revision,snapshot?.availability]));
  const selected=$derived(typeof config.apiKeyId==='string'?config.apiKeyId:'');
  const accounts=$derived(snapshot?.availability==='ready'&&binding?snapshot.connections.filter(account=>binding?.compatibleIds.includes(account.id)):[]);
  const resolved=$derived(binding?.status==='default'?snapshot?.connections.find(account=>account.id===binding?.defaultConnectionId):accounts.find(account=>account.id===selected));
  const unavailable=$derived(!!selected&&selected!==binding?.defaultRef&&!accounts.some(account=>account.id===selected));
  async function load(key:string){
    pending=true;binding=undefined;error='';
    try{const next=await reads.load(request,purpose,JSON.parse(JSON.stringify(config)),()=>mounted&&signature===key);if(next&&mounted&&signature===key)binding=next;}
    catch(problem){if(mounted&&signature===key)error=apiAccessError(problem);}
    finally{if(mounted&&signature===key)pending=false;}
  }
  $effect(()=>{const key=signature;if(mounted&&desktop)void load(key);});
  onMount(()=>{mounted=true;return()=>{mounted=false;reads.invalidate();};});
</script>

{#if !binding?.unused}
  <div class="credential-selector">
    <div class="field-label"><label for={'api-choice-'+purpose+'-'+botId}>{$t('API-Zugang')}</label><HelpTip text={$t('Wähle einen zentral gespeicherten Zugang für diesen Bot. Anbieterstandard folgt deiner globalen Auswahl. Ein leerer oder importierter Verweis erhält keinen Zugang automatisch.')}/></div>
    <select id={'api-choice-'+purpose+'-'+botId} value={selected} disabled={disabled||pending||!binding||!!error||snapshot?.availability!=='ready'} onchange={event=>onChange(event.currentTarget.value)} aria-describedby={'api-choice-note-'+purpose+'-'+botId}>
      <option value="">{$t('Kein API-Zugang zugeordnet')}</option>
      {#if binding?.defaultRef}<option value={binding.defaultRef}>{$t('Anbieterstandard')}{binding.defaultConnectionId?' · '+(snapshot?.connections.find(account=>account.id===binding?.defaultConnectionId)?.label||$t('Nicht verfügbar')):' · '+$t('Noch nicht festgelegt')}</option>
      {:else}<option disabled>{$t('Anbieterstandard · zuerst zentral festlegen')}</option>{/if}
      {#each accounts as account}<option value={account.id}>{account.label}{account.keyStatus==='missing'?' · '+$t('Schlüssel fehlt'):''}</option>{/each}
      {#if unavailable}<option value={selected} disabled>{$t('Ausgewählter Zugang nicht verfügbar')}</option>{/if}
    </select>
    <p class="field-note" id={'api-choice-note-'+purpose+'-'+botId} aria-live="polite">
      {#if pending}{$t('Prüft passende Zugänge …')}
      {:else if error}{$t(error)}
      {:else if binding?.status==='invalid'}{$t('Der gewählte Zugang passt nicht oder ist nicht mehr verfügbar. Wähle ihn erneut; es wird kein anderer Zugang eingesetzt.')}
      {:else if resolved?.keyStatus==='registered'}{$t('Schlüssel registriert. Systemzugriff und API-Gültigkeit wurden nicht geprüft.')}
      {:else if resolved?.keyStatus==='missing'}{$t('Für diesen Zugang ist noch kein Schlüssel registriert.')}
      {:else if !desktop}{$t('API-Zugänge werden in der Desktop-App verwaltet.')}
      {:else}{$t('Modell, Stimme und andere Bot-Einstellungen bleiben getrennt.')}{/if}
    </p>
    <button type="button" class="text-link" onclick={onManage} disabled={disabled}><KeyRound size={14}/>{$t('API-Zugänge verwalten')}</button>
  </div>
{/if}

<style>
  .credential-selector{display:grid;gap:.45rem;margin:1rem 0;min-width:0}
  .credential-selector select{width:100%;min-width:0;text-overflow:ellipsis}
  .credential-selector .field-note{margin:0;overflow-wrap:anywhere}
  .credential-selector .text-link{justify-self:start;display:inline-flex;align-items:center;gap:.4rem;text-align:left}
</style>
