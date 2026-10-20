<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { KeyRound, Plus, RotateCcw, Pencil, Trash2, Check } from '@lucide/svelte';
  import { call, native } from '../lib/api';
  import { t } from '../lib/i18n';
  import HelpTip from './HelpTip.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';
  import CredentialSessionSettings from './CredentialSessionSettings.svelte';
  import { apiServices, apiAccessError, createApiConnection, getApiConnections, mutateApiConnection, sameApiScope,
    type ApiConnection, type ApiScope, type ApiService, type ApiAuthStyle, type ApiSnapshot, type ApiRequest } from '../lib/api-access';
  let { visible=false, disabled=false, onSnapshot=()=>{}, onDirtyChange=()=>{}, onBusyChange=()=>{}, onLocked=async()=>{},
    request=call, desktop=native }: { visible?:boolean;disabled?:boolean;onSnapshot?:(value:ApiSnapshot|undefined)=>void;
    onDirtyChange?:(value:boolean)=>void;onBusyChange?:(value:boolean)=>void;onLocked?:()=>Promise<void>;
    request?:ApiRequest;desktop?:boolean }=$props();
  const names:Record<ApiService,string>={openai:'OpenAI',anthropic:'Anthropic',elevenlabs:'ElevenLabs',hindsight:'Hindsight',compatible:'Eigene API',youtube_data:'YouTube Data API'};
  const destinations:Record<ApiService,string>={openai:'https://api.openai.com/v1',anthropic:'https://api.anthropic.com/v1',elevenlabs:'https://api.elevenlabs.io/v1',hindsight:'http://127.0.0.1:8888',compatible:'http://127.0.0.1:8000/v1',youtube_data:'https://www.googleapis.com/youtube/v3'};
  let mounted=$state(false),pending=$state(false),snapshot=$state<ApiSnapshot>(),error=$state(''),notice=$state('');
  let mode=$state<'create'|'key'|'rename'|null>(null),entryId=$state<string|null>(null),draftRevision=$state<number|null>(null);
  let service=$state<ApiService>('openai'),destination=$state(destinations.openai),authStyle=$state<ApiAuthStyle>('bearer'),label=$state(''),makeDefault=$state(true);
  let password=$state.raw<HTMLInputElement>();let labelInput=$state.raw<HTMLInputElement>();
  let removing=$state<ApiConnection>();
  const editing=$derived(mode!==null||!!removing);
  const ready=$derived(snapshot?.availability==='ready');
  const selected=$derived(snapshot?.connections.find(item=>item.id===entryId));
  const mutable=$derived(!disabled&&!pending&&ready);
  let lastVisible=false;
  function accept(value:ApiSnapshot){snapshot=value;onSnapshot(value);}
  function clearPassword(){if(password)password.value='';}
  export function discardDraft(){if(pending)return;clearPassword();mode=null;entryId=null;draftRevision=null;removing=undefined;error='';notice='';}
  async function load(rebase=false){
    if(pending||!desktop)return;pending=true;error='';
    try{const next=await getApiConnections(request);if(!mounted)return;accept(next);if(rebase&&mode&&next.availability==='ready'){draftRevision=next.revision;notice='Status aktualisiert. Dein Entwurf bleibt erhalten.';}}
    catch(problem){if(mounted){snapshot=undefined;onSnapshot(undefined);error=apiAccessError(problem);}}
    finally{if(mounted)pending=false;}
  }
  function scopeDefaults(next:ApiService){service=next;destination=destinations[next];authStyle=next==='anthropic'?'anthropic':next==='elevenlabs'?'elevenlabs':next==='youtube_data'?'youtube_api_key':'bearer';}
  async function begin(kind:'create'|'key'|'rename',entry?:ApiConnection){
    if(!mutable||mode)return;mode=kind;entryId=entry?.id??null;draftRevision=snapshot!.revision;label=entry?.label??'';
    scopeDefaults(entry?.service??'openai');if(entry){destination=entry.destination;authStyle=entry.authStyle;}
    makeDefault=kind==='create';error='';notice='';await tick();clearPassword();if(kind==='key')password?.focus();else labelInput?.focus();
  }
  async function setDefault(account:ApiConnection){
    if(!mutable||mode||snapshot?.revision===null||snapshot?.revision===undefined)return;pending=true;error='';notice='';
    try{const next=await mutateApiConnection(request,'set_default_api_connection',{expectedRevision:snapshot.revision,service:account.service,destination:account.destination,authStyle:account.authStyle,connectionId:account.id});if(mounted){accept(next);notice='Anbieterstandard aktualisiert.';}}
    catch(problem){if(mounted)error=apiAccessError(problem);}finally{if(mounted)pending=false;}
  }
  async function clearDefault(scope:ApiScope){
    if(!mutable||mode||snapshot?.revision===null||snapshot?.revision===undefined)return;pending=true;error='';notice='';
    try{const next=await mutateApiConnection(request,'set_default_api_connection',{expectedRevision:snapshot.revision,...scope,connectionId:null});if(mounted){accept(next);notice='Anbieterstandard entfernt. Bestehende Standardverweise erhalten keinen Ersatz.';}}
    catch(problem){if(mounted)error=apiAccessError(problem);}finally{if(mounted)pending=false;}
  }
  async function remove(){
    if(!removing||!mutable||snapshot?.revision===null||snapshot?.revision===undefined)return;
    const id=removing.id;pending=true;error='';
    try{const next=await mutateApiConnection(request,'remove_api_connection',{expectedRevision:snapshot.revision,id});if(mounted){accept(next);removing=undefined;notice='Verbindung entfernt. Der Schlüsselbund-Eintrag bleibt bestehen.';}}
    catch(problem){if(mounted)error=apiAccessError(problem);}finally{if(mounted)pending=false;}
  }
  export async function saveDraft(){
    if(!mutable||!mode||draftRevision===null)return;
    if(mode!=='key'&&!label.trim()){error='Gib einen Namen für den Zugang ein.';labelInput?.focus();return;}
    if(mode!=='key'&&new TextEncoder().encode(label.trim()).length>120){error='Der Name ist zu lang. Verwende einen kürzeren Namen.';labelInput?.focus();return;}
    // Deliberately never copied into reactive/App state, logs, events or profiles.
    const value=password?.value??'';
    if(mode==='key'&&!value.trim()){error='Gib einen neuen API-Schlüssel ein.';password?.focus();return;}
    pending=true;error='';notice='';
    try{
      let id=entryId,revision=draftRevision;
      if(mode==='rename'){
        const next=await mutateApiConnection(request,'rename_api_connection',{expectedRevision:revision,id,label:label.trim()});if(!mounted)return;accept(next);
      }else{
        if(!id){
          const created=await createApiConnection(request,revision,{label:label.trim(),service,destination:destination.trim(),authStyle});
          if(!mounted)return;accept(created.snapshot);id=created.connectionId;entryId=id;revision=created.snapshot.revision!;draftRevision=revision;
          notice='Verbindung angelegt. Der Schlüssel wurde noch nicht gespeichert.';
        }
        if(value.trim()){
          const next=await mutateApiConnection(request,'set_api_connection_secret',{expectedRevision:revision,id,value});
          if(!mounted)return;accept(next);revision=next.revision!;draftRevision=revision;clearPassword();notice='Schlüssel im Systemschlüsselbund gespeichert.';
        }
        if(mode==='create'&&makeDefault){
          const account=snapshot?.connections.find(item=>item.id===id);if(!account)throw new Error('missing metadata');
          const existing=snapshot?.defaults.find(item=>sameApiScope(item,account));
          if(existing?.connectionId!==id){
            const next=await mutateApiConnection(request,'set_default_api_connection',{expectedRevision:revision,service:account.service,destination:account.destination,authStyle:account.authStyle,connectionId:id});
            if(!mounted)return;accept(next);draftRevision=next.revision;
          }
        }
      }
      if(mounted){clearPassword();mode=null;entryId=null;draftRevision=null;notice='API-Zugang gespeichert.';}
    }catch(problem){if(mounted)error=apiAccessError(problem);}
    finally{if(mounted)pending=false;}
  }
  function cancelRemoval(event:KeyboardEvent){if(!removing||event.key!=='Escape'||event.isComposing)return;event.preventDefault();event.stopImmediatePropagation();if(!pending)removing=undefined;}
  $effect(()=>{onDirtyChange(editing);});
  $effect(()=>{onBusyChange(pending);});
  $effect(()=>{if(mounted&&visible&&!lastVisible){lastVisible=true;void load();}else if(!visible)lastVisible=false;});
  onMount(()=>{mounted=true;window.addEventListener('keydown',cancelRemoval,true);return()=>{mounted=false;window.removeEventListener('keydown',cancelRemoval,true);clearPassword();onDirtyChange(false);onBusyChange(false);};});
</script>

<section class="settings-panel api-access" aria-labelledby="api-access-heading">
  <div class="manager-heading"><div class="section-heading"><KeyRound size={21}/><div><h2 id="api-access-heading">{$t('Einmal speichern. Für deine Bots auswählen.')}</h2><p>{$t('API-Schlüssel werden zentral im Systemschlüsselbund verwaltet. Modell, Stimme und Gedächtnis bleiben pro Bot einstellbar.')}</p></div></div><div class="button-row"><button class="button secondary small" disabled={pending||disabled||!desktop} onclick={()=>load(!!mode)}><RotateCcw size={14}/>{$t('Status erneut laden')}</button><button class="button primary small" disabled={!mutable||!!mode} onclick={()=>begin('create')}><Plus size={14}/>{$t('Zugang hinzufügen')}</button></div></div>
  <p class="field-note">{$t('Ein Anbieterstandard gilt nur für denselben Dienst, Endpunkt und Anmeldetyp. Zusätzliche Konten sind möglich. Profilexporte enthalten keine Schlüssel oder globalen Zugänge.')}</p>
  <p class="field-note">{$t('Laufende YouTube-Verbindungen behalten ihren bisherigen Schlüssel. Nach dem Ersetzen eines Schlüssels, Wechseln des Standards oder Entfernen des Zugangs bitte trennen und neu verbinden.')}</p>
  {#if !desktop}<p class="inline-note">{$t('API-Zugänge werden in der Desktop-App verwaltet.')}</p>
  {:else if !snapshot&&pending}<p role="status">{$t('Lädt API-Zugänge …')}</p>
  {:else if !snapshot||snapshot.availability!=='ready'}<p class="error-note" role="alert">{$t('Die zentrale Zugangsliste ist nicht verfügbar. Bestehende Daten wurden nicht zurückgesetzt. Lade den Status erneut.')}</p>
  {:else}
    {#if snapshot.legacyUnboundCount}<p class="inline-note">{$t('{count} frühere Verweise konnten keinem Zugang zugeordnet werden. Wähle betroffene Verbindungen ausdrücklich neu.',{count:snapshot.legacyUnboundCount})}</p>{/if}
    <div class="provider-list">
      {#each apiServices as provider}
        <section class="provider-group" aria-label={$t(names[provider])}>
          <div class="provider-heading"><h3>{$t(names[provider])}</h3><span>{snapshot.connections.filter(item=>item.service===provider).length} {$t('Zugänge')}</span></div>
          {#each snapshot.connections.filter(item=>item.service===provider) as account (account.id)}
            {@const selectedDefault=snapshot.defaults.find(item=>sameApiScope(item,account))}
            <article class="account-row"><div class="account-identity"><strong>{account.label}</strong><span class="endpoint">{account.destination}</span><div class="account-meta"><span class:missing={account.keyStatus==='missing'}>{account.keyStatus==='registered'?$t('Schlüssel registriert'):$t('Schlüssel fehlt')}</span>{#if selectedDefault?.connectionId===account.id}<span class="default-tag"><Check size={12}/>{$t('Anbieterstandard')}</span>{/if}</div>
              {#if account.writePolicy==='shared_legacy_read_only'}<p class="field-note">{$t('Dieser frühere Schlüssel wird von mehreren Verbindungen oder anderen Integrationen verwendet. Er wird hier nicht überschrieben. Lege für einen neuen Schlüssel einen unabhängigen Zugang an.')}</p>{/if}
            </div><div class="account-actions"><button class="button secondary small" disabled={!mutable||!!mode||account.writePolicy!=='replaceable'} onclick={()=>begin('key',account)}>{$t('Schlüssel ersetzen')}</button><button class="button small secondary" disabled={!mutable||!!mode||selectedDefault?.connectionId===account.id} onclick={()=>setDefault(account)}>{$t('Als Standard')}</button><button class="icon-button" title={$t('Zugang umbenennen')} aria-label={$t('Zugang umbenennen')} disabled={!mutable||!!mode} onclick={()=>begin('rename',account)}><Pencil size={15}/></button><button class="icon-button danger-text" title={$t('Verbindung entfernen')} aria-label={$t('Verbindung entfernen')} disabled={!mutable||!!mode} onclick={()=>removing=account}><Trash2 size={15}/></button></div></article>
          {:else}<p class="empty-provider">{$t('Noch kein Zugang. Füge bei Bedarf einen Schlüssel hinzu.')}</p>{/each}
          {#each snapshot.defaults.filter(item=>item.service===provider&&item.connectionId) as item}<button class="text-link clear-default" disabled={!mutable||!!mode} onclick={()=>clearDefault(item)}>{$t('Standard für {endpoint} aufheben',{endpoint:item.destination})}</button>{/each}
        </section>
      {/each}
    </div>
    <p class="field-note">{$t('„Registriert“ beschreibt die lokale Schlüsselliste. Das Öffnen dieser Seite prüft weder den Systemzugriff noch die Gültigkeit beim Anbieter.')}</p>
  {/if}
  {#if mode}
    <form class="account-form" onsubmit={event=>{event.preventDefault();void saveDraft();}} aria-labelledby="api-draft-heading">
      <div class="section-heading"><KeyRound size={18}/><div><h3 id="api-draft-heading">{$t(mode==='create'?'Neuer API-Zugang':mode==='key'?'Schlüssel ersetzen':'Zugang umbenennen')}</h3><p>{$t('Änderungen an einem Zugang gelten für alle Bots, die ihn auswählen.')}</p></div></div>
      {#if mode!=='key'}<label>{$t('Name des Zugangs')}<input bind:this={labelInput} bind:value={label} maxlength="120" disabled={pending||!!entryId&&mode==='create'} required placeholder={$t('Zum Beispiel: mein Hauptkonto')}/></label>{:else}<p class="selected-account"><strong>{selected?.label}</strong><span>{selected?.destination}</span></p>{/if}
      {#if mode==='create'}<div class="field-row"><label>{$t('Dienst')}<select value={service} disabled={pending||!!entryId} onchange={event=>scopeDefaults(event.currentTarget.value as ApiService)}>{#each apiServices as item}<option value={item}>{$t(names[item])}</option>{/each}</select></label><label>{$t('Anmeldetyp')} <HelpTip text={$t('Der Anmeldetyp muss zur API passen. Bei eigenen Servern dürfen Schlüssel nur an den genau eingerichteten Endpunkt gesendet werden.')}/><select bind:value={authStyle} disabled={pending||!!entryId||service!=='compatible'}><option value="bearer">Bearer</option><option value="anthropic">Anthropic · x-api-key</option><option value="elevenlabs">ElevenLabs · xi-api-key</option>{#if service==='youtube_data'}<option value="youtube_api_key">YouTube · API key</option>{/if}</select></label></div><label>{$t('Genauer API-Endpunkt')}<input bind:value={destination} disabled={pending||!!entryId} required inputmode="url" spellcheck="false" maxlength="4096"/></label><p class="field-note">{$t('Dienst, Endpunkt und Anmeldetyp sind nach dem Anlegen fest. Für ein anderes Ziel legst du einen neuen Zugang an.')}</p>{/if}
      {#if mode!=='rename'}<label>{$t('Neuer API-Schlüssel')} <span>{$t(mode==='create'?'Optional · später ergänzbar':'Wird nicht angezeigt')}</span><input bind:this={password} type="password" autocomplete="new-password" spellcheck="false" disabled={pending||selected?.writePolicy==='shared_legacy_read_only'} required={mode==='key'}/></label><p class="field-note">{$t('Gespeicherte Schlüssel werden nie angezeigt. Eine neue Eingabe bleibt nur in diesem Formular, bis du sie erfolgreich speicherst oder verwirfst.')}</p>{/if}
      {#if mode==='create'}<label class="default-choice"><input type="checkbox" bind:checked={makeDefault} disabled={pending}/><span>{$t('Als Anbieterstandard für diesen Endpunkt verwenden')}</span></label>{/if}
      <div class="button-row"><button class="button primary" type="submit" disabled={!mutable||selected?.writePolicy==='shared_legacy_read_only'&&mode==='key'}>{pending?$t('Speichert …'):$t('Zugang speichern')}</button><button class="button secondary" type="button" disabled={pending} onclick={discardDraft}>{$t('Entwurf verwerfen')}</button></div>
    </form>
  {/if}
  {#if notice}<p class="field-note" role="status">{$t(notice)}</p>{/if}
  {#if error}<p class="error-note" role="alert">{$t(error)}</p>{/if}
</section>
<CredentialSessionSettings {onLocked}/>
{#if removing}<ConfirmDialog title={$t('Verbindung entfernen?')} message={$t('Bots mit diesem Zugang können ihn danach nicht mehr verwenden. Der Schlüsselbund-Eintrag wird nicht gelöscht und kein anderer Zugang wird automatisch eingesetzt.')} confirmLabel={$t('Verbindung entfernen')} danger={true} {pending} onCancel={()=>{if(!pending)removing=undefined;}} onConfirm={()=>void remove()}/>{/if}

<style>
  .api-access{min-width:0}.manager-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:1rem;flex-wrap:wrap}.manager-heading .section-heading{flex:1 1 24rem}.manager-heading p{max-width:66ch}
  .provider-list{margin-top:1.2rem}.provider-group{border-top:1px solid var(--line);padding:1rem 0}.provider-heading{display:flex;gap:1rem;align-items:baseline;justify-content:space-between}.provider-heading h3{font-size:1rem;margin:0}.provider-heading>span,.empty-provider{color:var(--muted);font-size:.82rem}
  .account-row{display:flex;gap:1rem;align-items:center;justify-content:space-between;padding:.9rem 0}.account-row+.account-row{border-top:1px dashed var(--line)}.account-identity{display:grid;gap:.35rem;min-width:0;flex:1}.endpoint,.selected-account span{font-size:.82rem;color:var(--muted);overflow-wrap:anywhere}.account-meta{display:flex;gap:.8rem;align-items:center;flex-wrap:wrap;font-size:.75rem;color:var(--muted)}.default-tag{display:inline-flex;gap:.2rem;align-items:center;color:var(--accent)}.missing,.error-note{color:var(--danger)}.account-actions{display:flex;gap:.4rem;align-items:center;flex-wrap:wrap}.clear-default{margin:.4rem 0;text-align:left;overflow-wrap:anywhere;white-space:normal}
  .account-form{background:var(--surface-raised);border:1px solid var(--line);padding:1.1rem;border-radius:var(--radius);margin:1rem 0;max-width:48rem}.account-form label{min-width:0}.selected-account{display:grid;gap:.3rem}.default-choice{display:flex;align-items:flex-start;gap:.6rem;margin:1rem 0}.default-choice input{width:auto;margin:.15rem 0 0}.api-access .field-note{overflow-wrap:anywhere;max-width:75ch}.error-note{font-size:.85rem;overflow-wrap:anywhere}
  @media(max-width:700px){.account-row{align-items:stretch;flex-direction:column}.account-actions{justify-content:flex-start}.manager-heading>.button-row{width:100%}.account-form .field-row{grid-template-columns:minmax(0,1fr)}.account-form{padding:.85rem}.account-actions .button{white-space:normal}.provider-heading{align-items:center}}
</style>
