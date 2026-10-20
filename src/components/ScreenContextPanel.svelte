<script lang="ts">
  import { t } from '../lib/i18n';
  import { onDestroy, onMount } from 'svelte';
  import { Eye } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import HelpTip from './HelpTip.svelte';
  import { systemPermission } from '../lib/system-permission';
  import { selectScreenSession, ScreenSessionCleanupError, type ScreenSession } from '../lib/screen-session';
  import ScreenCapturePermissions from './ScreenCapturePermissions.svelte';

  interface ScreenContextConfig { enabled: boolean; displayId: string }
  interface CaptureDisplay { id: string; name: string; primary: boolean }
  interface Props {
    botId: string;
    config: ScreenContextConfig;
    visionEnabled: boolean;
    disabled?: boolean;
    permissionContext?: () => string;
    prepareSelection: (botId: string, current: () => boolean) => Promise<string | null>;
    onError: (message: string) => void;
  }
  let { botId, config = $bindable(), visionEnabled, disabled = false, permissionContext = () => '', prepareSelection, onError }: Props = $props();
  let displays = $state<CaptureDisplay[] | null>(null);
  let loading = $state(false);
  let error = $state('');
  let session = $state<ScreenSession | null>(null);
  let disposed = false;
  let statusRevision = 0;
  let request = 0;
  let enabling = $state(false);
  let alive = true;
  async function toggleScreen(enabled: boolean) {
    if (!enabled) {
      config.enabled = false; systemPermission.invalidate();
      if (session?.backend === 'wayland') await disconnectWayland();
      return;
    }
    if (enabling || disabled) return;
    const expected = JSON.stringify(config); const context = permissionContext();
    const current = () => alive && !disabled && JSON.stringify(config) === expected && permissionContext() === context;
    enabling = true;
    try { if (await systemPermission.ensure('screen', current) && current()) config.enabled = true; }
    finally { if (alive) enabling = false; }
  }
  const missingDisplay = $derived(displays !== null && !!config.displayId && !displays.some(display => display.id === config.displayId));

  onMount(() => {
    void refreshSession();
    const timer = setInterval(() => { void refreshSession(); }, 1500);
    return () => clearInterval(timer);
  });
  onDestroy(() => { alive = false; disposed = true; ++request; ++statusRevision; systemPermission.invalidate(); });

  async function refreshSession() {
    if (!native) return;
    const targetBot = botId;
    const revision = ++statusRevision;
    const action = request;
    try {
      const result = await call<ScreenSession>('get_screen_session', { botId: targetBot });
      if (!disposed && targetBot === botId && revision === statusRevision && action === request) session = result;
    } catch (cause) { if (!disposed && targetBot === botId && revision === statusRevision && action === request) error = prettyError(cause); }
  }

  async function selectWayland() {
    if (loading || enabling || disabled || disposed) return;
    const targetBot = botId;
    const revision = ++request; loading = true; error = '';
    const current = () => !disposed && !disabled && targetBot === botId && revision === request;
    try {
      await selectScreenSession({ botId: targetBot, current, context: permissionContext,
        prepare: () => prepareSelection(targetBot, current), invoke: call,
        adopt: result => { ++statusRevision; config.displayId = result.displayId; session = result; } });
    } catch (cause) {
      if (cause instanceof ScreenSessionCleanupError) onError(prettyError(cause));
      else if (current()) error = prettyError(cause);
    }
    finally { if (!disposed && revision === request) loading = false; }
  }

  async function disconnectWayland() {
    const targetBot = botId; const revision = ++request; ++statusRevision; error = '';
    const current = () => !disposed && targetBot === botId && revision === request;
    try { await call('disconnect_screen_source', { botId: targetBot }); if (current()) await refreshSession(); }
    catch (cause) { if (current()) error = prettyError(cause); }
    finally { if (current()) loading = false; }
  }

  async function loadDisplays() {
    if (loading) return;
    const revision = ++request;
    loading = true; error = '';
    try {
      const result = await call<CaptureDisplay[]>('list_capture_displays');
      if (revision !== request) return;
      if (!Array.isArray(result) || !result.every(display => display && typeof display.id === 'string' && !!display.id && typeof display.name === 'string' && typeof display.primary === 'boolean')) {
        throw new Error('Die Desktop-App hat keine gültige Bildschirmliste geliefert.');
      }
      displays = result;
    } catch (cause) {
      if (revision === request) error = prettyError(cause);
    } finally {
      if (revision === request) loading = false;
    }
  }
</script>

<section class="settings-panel screen-context" aria-label={$t('Bildschirm als Kontext')}>
  <div class="screen-heading">
    <div class="section-heading">
      <Eye size={20}/>
      <div>
        <h2>{$t('Bildschirm als Kontext')}</h2>
        <p>{$t('Eine gemeinsame Einstellung für Chats und Autopilot dieses Bots.')}</p>
      </div>
    </div>
    <span class="screen-state" class:enabled={config.enabled}>
      {enabling ? $t('Prüft …') : config.enabled ? $t('Für diesen Bot aktiviert') : $t('Nicht aktiviert')}
    </span>
  </div>

  <div class="toggle-row screen-toggle">
    <div>
      <strong>
        {$t('Bildschirm bei jeder Anfrage mitsenden')}
        <HelpTip text={$t('Die Desktop-App nimmt vor jedem Aufruf des Dialogmodells einen aktuellen Screenshot auf und fügt ihn als Bild hinzu, auch bei normalen Chat-Antworten und im Autopilot. MCP-Server, Skills und Werkzeugfreigaben werden dafür nicht benötigt. Diese Einstellung erlaubt keine Computersteuerung.')}/>
      </strong>
      <small>{$t('Aktueller Screenshot vor jedem Modellaufruf, kein laufendes Video.')}</small>
    </div>
    <input type="checkbox" role="switch" aria-label={$t('Bildschirm bei jeder Anfrage mitsenden')} checked={config.enabled} disabled={disabled || enabling} onchange={event => toggleScreen(event.currentTarget.checked)}/>
  </div>
  <div class="privacy-note">
    <strong>{$t('Anbieter & Datenschutz')}</strong>
    <p>{$t('Das Bild wird an das ausgewählte Dialogmodell übertragen. Bei Cloud- oder Remote-Anbietern verlässt es deinen Rechner und kann sichtbare private Inhalte enthalten. Benötigt ein bildfähiges Modell und die aktivierte Einstellung „Bilder an das Modell senden“.')}</p>
  </div>

  {#if config.enabled}
    <p class="screen-note">{$t('Vor dem Start wird die Systemfreigabe geprüft. Fehlende Rechte kannst du im Dialog anfragen und erneut prüfen.')}</p>
    <ScreenCapturePermissions {disabled} {permissionContext}/>

    {#if session?.backend === 'wayland'}
      <div class="source-settings">
        <h3>{$t('Bildschirm')}</h3>
        <p class="screen-note">{$t('Unter Wayland wählst du die Quelle im Freigabedialog deines Desktops. Diese Sitzung ersetzt nicht die Bildfreigabe dieses Bots. Die Auswahl bleibt nur für diese App-Sitzung bestehen; anschließend die Einstellungen speichern.')}</p>
        <p class="screen-note">{$t('Beim Klick auf „Quelle auswählen“ werden zuerst die aktuellen Bot-Einstellungen gespeichert. So gehört die Freigabe zum angezeigten Modell. Anschließend die neue Quellenauswahl speichern.')}</p>
        <div class="source-actions">
          <button class="button secondary" onclick={selectWayland} disabled={disabled || loading}>
            {loading ? $t('Wayland-Auswahl läuft …') : $t('Wayland-Quelle auswählen / erneut verbinden')}
          </button>
          <button class="button secondary" onclick={disconnectWayland} disabled={disabled || (!loading && session.phase === 'closed')}>
            {$t('Bildschirmfreigabe trennen')}
          </button>
        </div>
        <p class="source-status" role="status">{$t(session.message)}</p>
        {#if session.phase !== 'ready'}
          <p class="screen-warning" role="status">{$t('Keine verbundene Wayland-Quelle. Anfragen mit Bildschirmkontext werden abgewiesen; die App verwendet keinen anderen Bildschirm.')}</p>
        {/if}
      </div>
    {:else}
      <div class="display-choice">
        <label>
          {$t('Bildschirm')}
          <select bind:value={config.displayId} disabled={disabled || loading}>
            <option value="">{$t('Hauptbildschirm (automatisch)')}</option>
            {#if config.displayId && !displays?.some(display => display.id === config.displayId)}
              <option value={config.displayId}>{missingDisplay ? $t('Nicht verfügbar') : $t('Gespeicherte Auswahl')} · {config.displayId}</option>
            {/if}
            {#each displays || [] as display (display.id)}
              <option value={display.id}>{display.name || display.id}{display.primary ? ' · ' + $t('Hauptbildschirm') : ''}</option>
            {/each}
          </select>
        </label>
        <button class="button secondary" onclick={loadDisplays} disabled={disabled || loading}>
          {loading ? $t('Lädt …') : $t('Bildschirme laden')}
        </button>
      </div>
    {/if}

    {#if !visionEnabled}
      <p class="screen-warning" role="status">{$t('Die Bildübertragung ist für diesen Bot ausgeschaltet. Aktiviere „Bilder an das Modell senden“ unter Modelle, damit Bildschirmkontext funktioniert.')}</p>
    {/if}
    {#if session?.backend !== 'wayland' && missingDisplay}
      <p class="screen-warning" role="status">{$t('Der ausgewählte Bildschirm ist nicht verfügbar. Wähle einen erreichbaren Bildschirm; die App wechselt nicht automatisch auf einen anderen.')}</p>
    {/if}
    {#if displays?.length === 0}
      <p class="screen-warning" role="status">{$t('Das Betriebssystem hat keine Bildschirme gemeldet. Bildschirmkontext ist so nicht verfügbar.')}</p>
    {/if}
    {#if !native}
      <p class="screen-warning" role="status">{$t('Bildschirmaufnahmen sind nur in der Desktop-App verfügbar. Die Browseransicht kann deinen Bildschirm nicht aufnehmen.')}</p>
    {/if}
    <details class="capture-details">
      <summary>{$t('Aufnahme & Verarbeitung')}</summary>
      <p class="screen-note">{$t('Screenshots bleiben nur während der Anfrage im Arbeitsspeicher. Sie werden als JPEG auf höchstens 1.280 Pixel Kantenlänge verkleinert und nicht als Datei gespeichert.')}</p>
    </details>
  {/if}
  {#if error}
    <p class="screen-error" role="alert">{$t(error)}</p>
  {/if}
</section>

<style>
  .screen-context { margin-bottom: 20px; }
  .screen-heading { display: flex; justify-content: space-between; align-items: flex-start; gap: 20px; }
  .section-heading { margin-bottom: 0; }
  .section-heading h2 { color: var(--text); font-size: 17px; }
  .section-heading p { color: var(--muted); font-size: 12px; line-height: 1.6; }
  .screen-state { flex-shrink: 0; border: 1px solid var(--line); border-radius: 6px; background: var(--input-bg); padding: 6px 10px; color: var(--muted); font-size: 12px; line-height: 1.5; }
  .screen-state.enabled { color: var(--accent); border-color: var(--accent); }
  .screen-toggle { padding-block: 20px; margin-top: 20px; border-bottom: 1px solid var(--line); }
  .toggle-row strong { display: flex; flex-wrap: wrap; gap: 7px; align-items: center; color: var(--text); font-size: 13px; }
  .toggle-row small { color: var(--muted); font-size: 12px; line-height: 1.65; }
  .privacy-note { margin-top: 16px; padding: 14px 16px; background: var(--input-bg); border-left: 3px solid var(--accent); border-radius: 6px; }
  .privacy-note strong { color: var(--text); font-size: 12px; }
  .privacy-note p { margin-top: 6px; color: var(--muted); font-size: 12px; line-height: 1.7; }
  .screen-note { color: var(--muted); font-size: 12px; line-height: 1.7; margin: 12px 0 0; }
  .source-settings { margin-top: 20px; padding-top: 20px; border-top: 1px solid var(--line); }
  h3 { color: var(--text); font-size: 13px; }
  .source-actions { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 16px; }
  .source-actions button { white-space: normal; text-align: left; }
  .source-status { margin-top: 14px; color: var(--muted); font-size: 12px; line-height: 1.7; }
  .display-choice { display: flex; gap: 12px; align-items: flex-end; margin-top: 20px; }
  .display-choice label { flex: 1; min-width: 0; margin: 0; color: var(--text); font-size: 12px; }
  .display-choice select { min-height: 40px; margin-top: 8px; color: var(--text); background: var(--input-bg); border-color: var(--line); font-size: 13px; }
  .display-choice button { min-height: 40px; }
  .screen-warning, .screen-error { margin-top: 14px; padding: 12px 14px; border-left: 3px solid var(--warning); border-radius: 6px; background: var(--surface-raised); color: var(--warning); font-size: 12px; line-height: 1.7; }
  .screen-error { border-left-color: var(--danger); color: var(--danger); }
  .capture-details { margin-top: 20px; border-top: 1px solid var(--line); }
  summary { padding-top: 16px; color: var(--text); font-size: 12px; font-weight: 600; line-height: 1.6; cursor: pointer; }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; border-radius: 3px; }
  @media (max-width: 800px) {
    .screen-heading { flex-direction: column; gap: 12px; }
    .display-choice { align-items: stretch; flex-direction: column; }
    .display-choice button { align-self: flex-start; }
  }
</style>
