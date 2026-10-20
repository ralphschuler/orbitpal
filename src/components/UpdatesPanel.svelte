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
  const phaseLabels = { downloading: 'Update wird heruntergeladen …', installing: 'Signatur geprüft · Update wird installiert …', restarting: 'App wird neu gestartet …', cancelling:'Download wird abgebrochen …', cancelled:'Update-Download abgebrochen.', idle: '', error: '' };

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
      <div class="modal-heading">
        <div>
          <h2 id="updates-heading">{$t('Updates')}</h2>
          <p>{$t('Neue Versionen direkt aus GitHub Releases.')}</p>
          <span class="installed-version">OrbitPal v{version}</span>
        </div>
        <button class="icon-button" disabled={installing} onclick={close} aria-label={$t('Updates schließen')}>
          <X size={20}/>
        </button>
      </div>

      {#if !native}
        <p class="update-note">{$t('Update-Prüfung und Installation sind in der Desktop-App verfügbar.')}</p>
      {:else}
        <div class="update-result" aria-live="polite">
          {#if installing}
            <div class="update-status">
              <Download size={24}/>
              <div>
                <h3>{$t(phaseLabels[state.phase])}</h3>
                <p>{state.phase === 'downloading' ? (state.total ? $t('{downloaded} MiB von {total} MiB', { downloaded: downloadedMiB, total: totalMiB }) : $t('{downloaded} MiB', { downloaded: downloadedMiB })) : $t('Bitte lasse die App geöffnet.')}</p>
              </div>
            </div>
            {#if state.phase === 'downloading'}
              <progress aria-label={$t('Update-Download')} max="100" value={percent ?? undefined}></progress>
            {/if}
          {:else if state.available}
            <div class="update-status">
              <Download size={24}/>
              <div>
                <h3>{$t('Version {version} ist verfügbar.', { version: state.available.version })}</h3>
                <p>{$t('Installiert: v{version}', { version: state.available.currentVersion })}</p>
                {#if state.available.date}
                  <p>{$t('Veröffentlicht: {date}', { date: formatDate(state.available.date, {}, $locale) })}</p>
                {/if}
              </div>
            </div>
            {#if state.available.notes}
              <details class="release-details">
                <summary tabindex="0">{$t('Release-Hinweise')}</summary>
                <pre class="update-notes">{state.available.notes}</pre>
              </details>
            {/if}
            <button class="text-link release-link" onclick={releaseNotes}>
              <ExternalLink size={14}/>{$t('Release auf GitHub ansehen')}
            </button>
          {:else}
            <div class="update-status">
              <ShieldCheck size={24}/>
              <div>
                <h3>{state.phase === 'cancelled' ? $t('Update-Download abgebrochen.') : state.checking ? $t('Suche nach einer neuen Version …') : state.lastChecked && !state.error ? $t('Du bist auf dem neuesten Stand.') : $t('Bereit für die nächste Version.')}</h3>
                <p>{$t('Installierte Version: {version}', { version })}</p>
              </div>
            </div>
          {/if}
        </div>

        {#if state.phase === 'downloading' || state.phase === 'cancelling'}
          <p class="update-note">{$t('Du kannst Download und Prüfung abbrechen. Sobald die Installation beginnt, ist kein Abbruch mehr möglich.')}</p>
        {/if}
        {#if state.phase === 'cancelled'}
          <p class="update-note" role="status">{$t('Für einen neuen Versuch bitte erneut prüfen und bestätigen.')}</p>
        {/if}
        {#if state.error}
          <p class="update-error" role="alert"><CircleHelp size={16}/>{$t(state.error)}</p>
        {/if}
        {#if state.available && !installing}
          <p class="update-note">
            <ShieldCheck size={16}/>{$t('Das Paket wird vor der Installation kryptografisch geprüft. Mit deiner Bestätigung beginnt der Download; anschließend wird die App installiert und neu gestartet. Deine gespeicherten Profile bleiben erhalten.')}
          </p>
          {#if !state.installSupported}
            <p class="update-warning">{state.unavailableReason ? $t(state.unavailableReason) : $t('Bitte installiere diese Version über GitHub Releases.')}</p>
          {:else if busyReason}
            <p class="update-warning">{busyReason}</p>
          {/if}
        {/if}

        <div class="update-preference">
          <div>
            <label for="automatic-updates">{$t('Automatisch nach Updates suchen')}</label>
            <HelpTip text={$t('Die automatische Suche lädt und installiert kein Update. Download und Installation starten erst nach deiner Bestätigung.')}/>
          </div>
          <input id="automatic-updates" type="checkbox" role="switch" checked={state.automaticChecks} disabled={!state.settingsLoaded || state.savingSettings || installing} onchange={event => controller.setAutomatic(event.currentTarget.checked)}/>
          <p>{$t('Prüft kurz nach dem Start und danach alle sechs Stunden die öffentliche GitHub-Release-Datei. Dabei erhält GitHub deine IP-Adresse. Es werden keine Bot-Daten gesendet. Download und Installation erfordern immer deine Bestätigung.')}</p>
        </div>

        <div class="modal-footer">
          {#if state.phase === 'downloading' || state.phase === 'cancelling'}
            <button class="button secondary" onclick={() => controller.cancel()} disabled={state.phase === 'cancelling'}>
              <X size={15}/>{state.phase === 'cancelling' ? $t('Wird abgebrochen …') : $t('Download abbrechen')}
            </button>
          {/if}
          <button class="button secondary" onclick={close} disabled={installing}>
            {state.available ? $t('Später') : $t('Schließen')}
          </button>
          <button class="button secondary" onclick={() => controller.check()} disabled={installing || state.checking}>
            <RefreshCw size={15}/>{state.checking ? $t('Prüft …') : $t('Jetzt prüfen')}
          </button>
          {#if state.available}
            <button class="button primary" onclick={() => state.available && controller.confirm(state.available.id, busyReason)} disabled={installing || state.checking || !state.installSupported || !!busyReason}>
              <Download size={15}/>{$t('Bestätigen & installieren')}
            </button>
          {/if}
        </div>
        {#if state.lastChecked}
          <p class="update-checked">{$t('Zuletzt geprüft: {date}', { date: formatDate(state.lastChecked, { dateStyle: 'medium', timeStyle: 'medium' }, $locale) })}</p>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .updates-dialog { width: min(700px, calc(100vw - 40px)); max-height: calc(100vh - 60px); overflow-y: auto; background: var(--panel); border-color: var(--line); }
  .modal-heading { margin-bottom: 22px; }
  .modal-heading h2 { font-size: 22px; color: var(--text); }
  .modal-heading p { font-size: 13px; line-height: 1.65; color: var(--muted); }
  .installed-version { display: inline-block; margin-top: 10px; padding: 4px 8px; border: 1px solid var(--line); border-radius: 5px; background: var(--input-bg); color: var(--muted); font-size: 12px; }
  .update-result { padding: 18px; border: 1px solid var(--line); border-radius: 7px; background: var(--input-bg); margin-bottom: 18px; }
  .update-status { display: flex; align-items: flex-start; gap: 14px; color: var(--accent); }
  .update-status > :global(svg) { flex-shrink: 0; margin-top: 2px; }
  .update-status h3 { font-size: 15px; font-weight: 600; margin: 0 0 6px; color: var(--text); line-height: 1.6; }
  .update-status p { font-size: 12px; color: var(--muted); margin: 0; line-height: 1.65; }
  .release-details { margin-top: 16px; border-top: 1px solid var(--line); }
  summary { padding-top: 14px; color: var(--text); font-size: 12px; font-weight: 600; cursor: pointer; line-height: 1.6; }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; border-radius: 3px; }
  .update-notes { font: inherit; font-size: 12px; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 230px; overflow-y: auto; color: var(--text); background: var(--surface-raised); border-radius: 6px; padding: 14px; margin: 14px 0 0; line-height: 1.75; }
  .release-link { margin-top: 16px; color: var(--accent); font-size: 12px; }
  .update-note, .update-error { display: flex; align-items: flex-start; gap: 9px; font-size: 12px; line-height: 1.75; color: var(--muted); margin: 14px 0; }
  .update-note :global(svg), .update-error :global(svg) { flex-shrink: 0; margin-top: 3px; }
  .update-warning, .update-error { padding: 12px 14px; border-left: 3px solid var(--warning); border-radius: 6px; background: var(--surface-raised); color: var(--warning); font-size: 12px; line-height: 1.7; }
  .update-error { border-left-color: var(--danger); color: var(--danger); }
  .update-preference { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 7px 18px; padding-top: 20px; margin-top: 20px; border-top: 1px solid var(--line); }
  .update-preference > div { display: flex; align-items: center; gap: 6px; }
  .update-preference label { margin: 0; font-size: 12px; font-weight: 600; color: var(--text); }
  .update-preference p { grid-column: 1 / -1; margin: 0; color: var(--muted); font-size: 12px; line-height: 1.7; }
  .modal-footer { display: flex; flex-wrap: wrap; gap: 9px; justify-content: flex-end; border-top-color: var(--line); }
  .modal-footer button { min-height: 40px; white-space: normal; }
  .update-checked { text-align: right; font-size: 12px; color: var(--muted); margin: 14px 0 0; line-height: 1.6; }
  progress { display: block; width: 100%; height: 8px; margin-top: 20px; accent-color: var(--accent); }
  @media (max-width: 650px) {
    .updates-dialog { padding: 20px; }
    .modal-footer { justify-content: stretch; }
    .modal-footer button { flex: 1 1 auto; }
  }
</style>
