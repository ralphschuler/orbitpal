<script lang="ts">
  import { onMount } from 'svelte';
  import { KeyRound, LockKeyhole, UnlockKeyhole } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import { t } from '../lib/i18n';
  import { updateCredentialSession, type CredentialSessionSnapshot } from '../lib/credential-session';

  let { onLocked = async () => {} }: { onLocked?: () => Promise<void> } = $props();
  let status = $state<CredentialSessionSnapshot>();
  let pending = $state(false);
  let error = $state('');
  let alive = false;
  const request = (command:string,args?:Record<string,unknown>)=>call<CredentialSessionSnapshot>(command,args);

  async function load() {
    if (pending) return;
    pending = true; error = '';
    try { const result=await request('get_credential_session'); if(alive)status=result; }
    catch(problem){if(alive){status=undefined;error=prettyError(problem);}}
    finally{if(alive)pending=false;}
  }

  async function change(enabled:boolean) {
    if (pending) return;
    pending = true; error = '';
    const result=await updateCredentialSession(request,'set_credential_session',{enabled},prettyError);
    if(alive){status=result.status;error=result.error;pending=false;}
  }
  async function lock() {
    if (pending) return;
    pending = true; error = '';
    try {
      const result = await updateCredentialSession(request,'lock_credential_session',{},prettyError);
      if (alive) { status = result.status; error = result.error; }
      if (!result.error) await onLocked();
    } catch (problem) { if (alive) error = prettyError(problem); }
    finally { if (alive) pending = false; }
  }
  onMount(() => {
    alive = true;
    if (native) void load();
    return () => { alive = false; };
  });
</script>

{#if native && (status?.supported || error)}
  <section class="settings-panel credential-session" aria-labelledby="credential-session-heading">
    <div class="section-heading"><KeyRound size={20}/><div>
      <h2 id="credential-session-heading">{$t('Schlüssel für diese App-Sitzung')}</h2>
      <p>{$t('Gilt auf diesem Mac für alle Bots. Standardmäßig ausgeschaltet; wird nicht in Profile exportiert.')}</p>
    </div></div>
    {#if status?.supported}
      <div class="toggle-row"><div>
        <strong>{$t('Freigegebene Schlüssel in dieser Sitzung wiederverwenden')}</strong>
        <small>{$t('Nach dem ersten erfolgreichen Schlüsselbundabruf behält OrbitPal eine Kopie im Arbeitsspeicher. Dadurch entfallen weitere Abrufe desselben Schlüssels bis zum Sperren oder Beenden.')}</small>
      </div><input type="checkbox" role="switch" aria-label={$t('Freigegebene Schlüssel in dieser Sitzung wiederverwenden')} checked={status.enabled} disabled={pending} onchange={event=>change(event.currentTarget.checked)}/></div>
      <p class="field-note">{$t('Ein später gesperrter macOS-Schlüsselbund oder extern geänderte Zugriffsrechte löschen bereits geladene Kopien nicht.')}</p>
      {#if status.enabled}
        <div class="session-actions" aria-live="polite">
          <span>{status.locked ? $t('Gesperrt · neue Schlüsselabrufe sind blockiert.') : $t('Sitzungsfreigabe aktiv · Schlüssel werden bei Bedarf geladen.')}</span>
          {#if status.locked}<button class="button secondary" disabled={pending} onclick={()=>change(true)}><UnlockKeyhole size={15}/>{$t('Schlüssel erneut freigeben')}</button>
          {:else}<button class="button secondary" disabled={pending} onclick={lock}><LockKeyhole size={15}/>{$t('Schlüssel jetzt sperren')}</button>{/if}
        </div>
      {/if}
      <details class="session-details"><summary>{$t('Sperren, Neustart und technische Grenzen')}</summary>
        <p class="field-note">{$t('Sperren entfernt die von OrbitPal behaltenen Kopien und stoppt neue Abrufe bis zu deiner erneuten Freigabe. Die Schlüssel im Systemschlüsselbund bleiben gespeichert. Nach einem App-Neustart beginnt eine neue Sitzung.')}</p>
        <p class="field-note">{$t('Laufende Anfragen und bestehende Verbindungen können ihre bereits erhaltenen Schlüssel weiterverwenden. Zum erneuten Einlesen externer Änderungen zuerst hier sperren und dann wieder freigeben.')}</p>
        <p class="field-note">{$t('Es werden höchstens 64 Schlüssel mit zusammen 1 MiB behalten, pro Schlüssel höchstens 64 KiB. Größere Werte oder weitere Schlüssel werden weiterhin direkt aus dem Schlüsselbund gelesen.')}</p>
      </details>
    {/if}
    {#if error}<p class="error-note" role="alert">{error}</p><button class="text-link" disabled={pending} onclick={load}>{$t('Status erneut laden')}</button>{/if}
  </section>
{/if}

<style>
  .credential-session { margin-bottom: 1.25rem; }
  .session-actions { display:flex; align-items:center; justify-content:space-between; gap:1rem; flex-wrap:wrap; font-size:.85rem; color:var(--muted); }
  .error-note { color:var(--danger); font-size:.85rem; }
  .session-details { margin-top:1rem; font-size:.85rem; }
  summary { cursor:pointer; color:var(--muted); }
</style>
