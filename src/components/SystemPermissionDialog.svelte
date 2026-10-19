<script lang="ts">
  import { onMount } from 'svelte';
  import { ShieldCheck, X, Monitor, Mic } from '@lucide/svelte';
  import { t } from '../lib/i18n';
  import { prettyError } from '../lib/api';
  import { focusTrap } from '../lib/focus';
  import { systemPermission, usablePermission } from '../lib/system-permission';
  const state = $derived($systemPermission);
  const screen = $derived(state?.kind === 'screen');
  const ready = $derived(!!state && usablePermission(state.kind, state.permission));
  const mac = $derived(state?.permission?.platform === 'macos');
  const canRequest = $derived(mac && !ready && state?.permission?.status !== 'restricted');
  const canSettings = $derived(mac || !screen && state?.permission?.platform === 'windows');
  onMount(() => {
    // Returning from Settings updates the display only. It never opens a device
    // or resumes a suspended request.
    const refresh = () => { if (systemPermission.active) void systemPermission.refresh(); };
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  });
</script>

{#if state?.visible}
  <div class="permission-backdrop" role="presentation">
    <div class="permission-dialog" role="dialog" aria-modal="true" aria-labelledby="system-permission-title" aria-describedby="system-permission-description" tabindex="-1" use:focusTrap onkeydown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); systemPermission.cancel(); } }}>
      <div class="permission-heading"><span class="permission-icon">{#if screen}<Monitor size={23}/>{:else}<Mic size={23}/>{/if}</span><h2 id="system-permission-title">{screen ? $t('Bildschirmzugriff erlauben') : $t('Mikrofonzugriff erlauben')}</h2><button class="icon-button" data-autofocus onclick={() => systemPermission.cancel()} aria-label={$t('Dialog schließen')}><X size={20}/></button></div>
      <p id="system-permission-description">{screen ? $t('OrbitPal benötigt die Systemfreigabe, bevor dieser Bot einen Screenshot an sein Dialogmodell senden kann. Das Bild kann private Inhalte enthalten; bei Cloud-Modellen verlässt es deinen Rechner.') : $t('OrbitPal benötigt die Mikrofonfreigabe für deine Aufnahme. Audio wird vom gewählten Spracherkennungsanbieter verarbeitet; bei einem Cloud-Anbieter verlässt es deinen Rechner.')}</p>
      <p class="permission-status" role="status"><ShieldCheck size={16}/>{state.loading ? $t('Prüft …') : ready ? state.permission?.status === 'authorized' ? $t('Systemfreigabe erteilt') : $t('Die Freigabe wird vom System beim Zugriff verwaltet.') : state.permission?.status === 'unsupported' ? $t('Bildschirmaufnahmen sind nur in der Desktop-App verfügbar. Die Browseransicht kann deinen Bildschirm nicht aufnehmen.') : $t('Systemfreigabe erforderlich')}</p>
      {#if mac && !ready}<p>{screen ? $t('Klicke auf „Freigabe anfragen“ und bestätige die macOS-Abfrage. Nach einer Ablehnung öffne Datenschutz & Sicherheit → Bildschirmaufnahme beziehungsweise Bildschirm- & Systemaudioaufnahme und aktiviere OrbitPal. Ein App-Neustart kann nötig sein.') : state.permission?.status === 'restricted' ? $t('macOS beschränkt den Mikrofonzugriff. Prüfe Bildschirmzeit oder die Geräteverwaltung und starte die Aufnahme danach selbst erneut.') : $t('Klicke auf „Freigabe anfragen“ und bestätige die macOS-Abfrage. Nach einer Ablehnung öffne Datenschutz & Sicherheit → Mikrofon und aktiviere OrbitPal. Ein App-Neustart kann nötig sein.')}</p>{/if}
      {#if !mac && state.permission?.status !== 'unsupported'}<p>{$t('Die Betriebssystem- oder Browserfreigabe wird beim Zugriff geprüft. Geräte-, WebView- und Portalfehler werden separat angezeigt. Bei einer Ablehnung ändere die Freigabe in den Systemeinstellungen und starte die Aktion erneut.')}</p>{/if}
      <p class="permission-note">{state.recovery ? $t('Automatische Aktionen wurden pausiert. Nach der Freigabe starte den Cohost, Autopiloten oder das Mikrofon selbst erneut. Die fehlgeschlagene Anfrage wird nicht wiederholt.') : $t('Eine Statusprüfung nimmt weder Bild noch Ton auf. Nach der Rückkehr aus den Einstellungen klicke auf „Weiter“. Abbrechen und Stop verwerfen diese Aktion.')}</p>
      {#if state.error}<p class="permission-error" role="alert">{$t(prettyError(state.error))}</p>{/if}
      <div class="permission-actions">
        {#if canRequest}<button class="button primary" disabled={state.loading} onclick={() => systemPermission.request()}>{$t('Freigabe anfragen')}</button>{/if}
        {#if canSettings}<button class="button secondary" disabled={state.loading} onclick={() => systemPermission.settings()}>{$t('Systemeinstellungen öffnen')}</button>{/if}
        <button class="button secondary" disabled={state.loading} onclick={() => systemPermission.refresh()}>{$t('Status erneut prüfen')}</button>
        {#if !state.recovery && ready}<button class="button primary" disabled={state.loading} onclick={() => systemPermission.continue()}>{$t('Weiter')}</button>{/if}
        <button class="button secondary" onclick={() => systemPermission.cancel()}>{state.recovery ? $t('Schließen') : $t('Abbrechen')}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .permission-backdrop { position:fixed; inset:0; z-index:1200; display:grid; place-items:center; padding:24px; background:#0a1020aa; backdrop-filter:blur(5px); }
  .permission-dialog { width:min(590px,100%); max-height:90vh; overflow:auto; border:1px solid #dce3ee; border-radius:18px; background:#fbfcff; padding:25px; color:#40516a; box-shadow:0 22px 70px #06102255; }
  .permission-heading { display:flex; align-items:center; gap:12px; margin-bottom:18px; }
  .permission-heading h2 { flex:1; font-size:19px; line-height:1.3; margin:0; }
  .permission-icon { color:#758db4; }
  p { margin:12px 0; font-size:13px; line-height:1.7; }
  .permission-status { display:flex; align-items:center; gap:9px; padding:12px; background:#edf2f8; border-radius:9px; font-weight:600; }
  .permission-note { color:#687b95; font-size:12px; }
  .permission-error { color:#9e4351; white-space:pre-wrap; overflow-wrap:anywhere; }
  .permission-actions { display:flex; flex-wrap:wrap; gap:9px; margin-top:20px; }
</style>
