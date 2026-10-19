<script lang="ts">
  import { t, locale, formatDate, formatNumber } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { Download, RefreshCw, ShieldCheck, ExternalLink, X, CircleHelp } from '@lucide/svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { call, native, onEvent, prettyError } from '../lib/api';
  import { focusTrap } from '../lib/focus';
  import { UpdateController, emptyUpdateState, updateInProgress } from '../lib/updates';
  import HelpTip from './HelpTip.svelte';
  import { version } from '../../package.json';

  interface Props {
    open?: boolean;
    busyReason: string;
    onInstalling: (installing: boolean) => void;
    onAvailable: (version: string) => void;
  }
  let { open = $bindable(false), busyReason, onInstalling, onAvailable }: Props = $props();
  let state = $state(emptyUpdateState());
  const installing = $derived(updateInProgress(state));
  const percent = $derived(state.total ? Math.min(100, Math.round(state.downloaded / state.total * 100)) : null);
  const downloadedMiB = $derived(formatNumber(state.downloaded / 1048576, $locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }));
  const totalMiB = $derived(formatNumber((state.total ?? 0) / 1048576, $locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }));
  let controller: UpdateController;
  const phaseLabels = { downloading: 'Update wird heruntergeladen …', installing: 'Signatur geprüft · Update wird installiert …', restarting: 'App wird neu gestartet …', idle: '', error: '' };

  function close() { if (!installing) open = false; }
  function keyboard(event: KeyboardEvent) {
    if (!open || event.key !== 'Escape') return;
    event.preventDefault(); event.stopImmediatePropagation(); close();
  }
  async function releaseNotes() {
    const url = state.available?.releaseUrl;
    // Release text is untrusted. Only the fixed repository's version page may open.
    if (!url || !/^https:\/\/github\.com\/ralphschuler\/(?:orbitpal|streamer-ai-cohost)\/releases\/tag\/v[0-9]+\.[0-9]+\.[0-9]+$/.test(url)) return;
    try { await openUrl(url); } catch (error) { state = { ...state, error: prettyError(error) }; }
  }
  onMount(() => {
    let disposed = false;
    let unlisten = () => {};
    let startup: ReturnType<typeof setTimeout> | undefined;
    let interval: ReturnType<typeof setInterval> | undefined;
    let announced = '';
    controller = new UpdateController({ call, lock: onInstalling, errorText: prettyError, changed: next => {
      state = next;
      if (next.available?.version !== announced) { announced = next.available?.version || ''; onAvailable(announced); }
    } });
    window.addEventListener('keydown', keyboard, { capture: true });
    if (native) {
      void onEvent('cohost://update', event => controller.progress(event)).then(stop => {
        if (disposed) stop(); else unlisten = stop;
      });
      void controller.load().then(() => {
        if (disposed) return;
        startup = setTimeout(() => void controller.automaticCheck(), 12_000);
        interval = setInterval(() => void controller.automaticCheck(), 6 * 60 * 60 * 1000);
      });
    }
    return () => { disposed = true; controller.destroy(); unlisten(); clearTimeout(startup); clearInterval(interval); window.removeEventListener('keydown', keyboard, { capture: true }); };
  });
</script>

{#if open}
  <div class="modal-backdrop" role="presentation" onclick={event => { if (event.target === event.currentTarget) close(); }}>
    <div class="modal updates-dialog" role="dialog" aria-modal="true" aria-labelledby="updates-heading" tabindex="-1" use:focusTrap>
      <div class="modal-heading"><div><span class="updates-eyebrow">OrbitPal · v{version}</span><h2 id="updates-heading">{$t("Updates")}</h2><p>{$t("Neue Versionen direkt aus GitHub Releases.")}</p></div><button class="icon-button" disabled={installing} onclick={close} aria-label={$t("Updates schließen")}><X size={20}/></button></div>
      {#if !native}
        <p class="update-note">{$t("Update-Prüfung und Installation sind in der Desktop-App verfügbar.")}</p>
      {:else}
        <div class="update-preference"><div><label for="automatic-updates">{$t("Automatisch nach Updates suchen")}</label><HelpTip text={$t("Prüft kurz nach dem Start und danach alle sechs Stunden die öffentliche GitHub-Release-Datei. Dabei erhält GitHub deine IP-Adresse. Es werden keine Bot-Daten gesendet. Download und Installation erfordern immer deine Bestätigung.")}/></div><input id="automatic-updates" type="checkbox" role="switch" checked={state.automaticChecks} disabled={!state.settingsLoaded || state.savingSettings || installing} onchange={event => controller.setAutomatic(event.currentTarget.checked)}/></div>
        <div class="update-result" aria-live="polite">
          {#if installing}
            <div class="update-status"><Download size={24}/><div><h3>{$t(phaseLabels[state.phase])}</h3><p>{state.phase === 'downloading' ? (state.total ? $t('{downloaded} MiB von {total} MiB', { downloaded: downloadedMiB, total: totalMiB }) : $t('{downloaded} MiB', { downloaded: downloadedMiB })) : $t('Bitte lasse die App geöffnet.')}</p></div></div>
            {#if state.phase === 'downloading'}<progress aria-label={$t("Update-Download")} max="100" value={percent ?? undefined}></progress>{/if}
          {:else if state.available}
            <div class="update-status"><Download size={24}/><div><h3>{$t("Version {version} ist verfügbar.", { version: state.available.version })}</h3><p>{$t('Installiert: v{version}', { version: state.available.currentVersion })}{state.available.date ? ' · ' + $t('Veröffentlicht: {date}', { date: formatDate(state.available.date, {}, $locale) }) : ''}</p></div></div>
            {#if state.available.notes}<pre class="update-notes">{state.available.notes}</pre>{/if}
            <button class="text-link" onclick={releaseNotes}><ExternalLink size={14}/>{$t("Release auf GitHub ansehen")}</button>
          {:else}
            <div class="update-status"><ShieldCheck size={24}/><div><h3>{state.checking ? $t("Suche nach einer neuen Version …") : state.lastChecked && !state.error ? $t("Du bist auf dem neuesten Stand.") : $t("Bereit für die nächste Version.")}</h3><p>{$t("Installierte Version: {version}", { version })}</p></div></div>
          {/if}
        </div>
        {#if state.error}<p class="update-error" role="alert"><CircleHelp size={16}/>{$t(state.error)}</p>{/if}
        {#if state.available && !installing}
          <p class="update-note"><ShieldCheck size={16}/>{$t("Das Paket wird vor der Installation kryptografisch geprüft. Mit deiner Bestätigung beginnt der Download; anschließend wird die App installiert und neu gestartet. Deine gespeicherten Profile bleiben erhalten.")}</p>
          {#if !state.installSupported}<p class="update-warning">{state.unavailableReason ? $t(state.unavailableReason) : $t("Bitte installiere diese Version über GitHub Releases.")}</p>{:else if busyReason}<p class="update-warning">{busyReason}</p>{/if}
        {/if}
        <div class="modal-footer"><button class="button secondary" onclick={close} disabled={installing}>{state.available ? $t("Später") : $t("Schließen")}</button><button class="button secondary" onclick={() => controller.check()} disabled={installing || state.checking}><RefreshCw size={15}/>{state.checking ? $t("Prüft …") : $t("Jetzt prüfen")}</button>{#if state.available}<button class="button primary" onclick={() => state.available && controller.confirm(state.available.id, busyReason)} disabled={installing || state.checking || !state.installSupported || !!busyReason}><Download size={15}/>{$t("Bestätigen & installieren")}</button>{/if}</div>
        {#if state.lastChecked}<p class="update-checked">{$t('Zuletzt geprüft: {date}', { date: formatDate(state.lastChecked, { dateStyle: 'medium', timeStyle: 'medium' }, $locale) })}</p>{/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .updates-dialog { width: min(660px, calc(100vw - 40px)); max-height: calc(100vh - 60px); overflow-y: auto; }
  .updates-eyebrow { color: #aebce8; font-size: 10px; letter-spacing: .08em; }
  .updates-dialog h2 { margin-top: 8px; }
  .update-preference { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 18px 0; border-block: 1px solid #343f53; }
  .update-preference > div { display: flex; align-items: center; gap: 6px; }
  .update-preference label { font-size: 12px; font-weight: 650; }
  .update-result { padding: 25px 0 18px; }
  .update-status { display: flex; align-items: center; gap: 15px; color: #bdc9f3; }
  .update-status h3 { font-size: 15px; font-weight: 650; margin: 0 0 5px; color: #edf0f8; }
  .update-status p { font-size: 11px; color: #a0adbf; margin: 0; }
  .update-note, .update-error { display: flex; align-items: flex-start; gap: 9px; font-size: 11px; line-height: 1.8; color: #aab6ca; }
  .update-note :global(svg), .update-error :global(svg) { flex-shrink: 0; margin-top: 3px; }
  .update-notes { font: inherit; font-size: 11px; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 210px; overflow-y: auto; color: #c3ccdc; background: #161e2a; border-radius: 8px; padding: 14px; margin: 17px 0 12px; line-height: 1.7; }
  .update-warning { border-left: 2px solid #e8b99a; padding-left: 12px; color: #e8c5ab; font-size: 11px; line-height: 1.8; }
  .update-error { color: #ffb7c0; }
  .update-checked { text-align: right; font-size: 10px; color: #8795ab; margin: 12px 0 0; }
  progress { display: block; width: 100%; height: 7px; margin-top: 22px; accent-color: #b8c4fa; }
  .modal-footer { display: flex; flex-wrap: wrap; gap: 9px; justify-content: flex-end; }
</style>
