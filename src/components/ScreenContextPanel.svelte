<script lang="ts">
  import { t } from '../lib/i18n';
  import { onDestroy, onMount } from 'svelte';
  import { Eye } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import HelpTip from './HelpTip.svelte';
  import { systemPermission } from '../lib/system-permission';
  import ScreenCapturePermissions from './ScreenCapturePermissions.svelte';

  interface ScreenContextConfig { enabled: boolean; displayId: string }
  interface CaptureDisplay { id: string; name: string; primary: boolean }
  interface ScreenSession { backend: 'native' | 'wayland'; phase: string; displayId: string; message: string }
  interface Props {
    botId: string;
    config: ScreenContextConfig;
    visionEnabled: boolean;
    disabled?: boolean;
    permissionContext?: () => string;
  }
  let { botId, config = $bindable(), visionEnabled, disabled = false, permissionContext = () => '' }: Props = $props();
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
    const targetBot = botId; const context = permissionContext(); const expected = JSON.stringify(config);
    const revision = ++request; loading = true; error = '';
    const current = () => !disposed && !disabled && targetBot === botId && revision === request && context === permissionContext() && expected === JSON.stringify(config);
    try {
      const result = await call<ScreenSession>('select_screen_source', { botId: targetBot });
      if (!current()) return;
      if (result.backend !== 'wayland' || result.phase !== 'ready' || !/^wayland:[0-9a-f-]{36}$/.test(result.displayId)) throw new Error('Die Wayland-Quelle wurde nicht verbunden.');
      ++statusRevision; config.displayId = result.displayId; session = result;
    } catch (cause) { if (current()) error = prettyError(cause); }
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

<section class="settings-panel screen-context" aria-label={$t("Bildschirm als Kontext")}>
  <div class="section-heading"><Eye size={20}/><div><h2>{$t("Bildschirm als Kontext")}</h2><p>{$t("Eine gemeinsame Einstellung für Chats und Autopilot dieses Bots.")}</p></div></div>
  <div class="toggle-row"><div><strong>{$t("Bildschirm bei jeder Anfrage mitsenden")} <HelpTip text={$t("Die Desktop-App nimmt vor jedem Aufruf des Dialogmodells einen aktuellen Screenshot auf und fügt ihn als Bild hinzu, auch bei normalen Chat-Antworten und im Autopilot. MCP-Server, Skills und Werkzeugfreigaben werden dafür nicht benötigt. Diese Einstellung erlaubt keine Computersteuerung.")}/></strong><small>{$t("Aktueller Screenshot vor jedem Modellaufruf, kein laufendes Video.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Bildschirm bei jeder Anfrage mitsenden")} checked={config.enabled} disabled={disabled || enabling} onchange={event => toggleScreen(event.currentTarget.checked)}/></div>
  <p class="field-note">{$t("Das Bild wird an das ausgewählte Dialogmodell übertragen. Bei Cloud- oder Remote-Anbietern verlässt es deinen Rechner und kann sichtbare private Inhalte enthalten. Benötigt ein bildfähiges Modell und die aktivierte Einstellung „Bilder an das Modell senden“.")}</p>
  {#if config.enabled}
    <ScreenCapturePermissions {disabled} {permissionContext}/>
    {#if session?.backend === 'wayland'}
      <p class="field-note">{$t("Unter Wayland wählst du die Quelle im Freigabedialog deines Desktops. Diese Sitzung ersetzt nicht die Bildfreigabe dieses Bots. Die Auswahl bleibt nur für diese App-Sitzung bestehen; anschließend die Einstellungen speichern.")}</p>
      <div class="display-choice">
        <button class="button secondary" onclick={selectWayland} disabled={disabled || loading}>{loading ? $t("Wayland-Auswahl läuft …") : $t("Wayland-Quelle auswählen / erneut verbinden")}</button>
        <button class="button secondary" onclick={disconnectWayland} disabled={disabled || (!loading && session.phase === 'closed')}>{$t("Bildschirmfreigabe trennen")}</button>
      </div>
      <p class="field-note" role="status">{$t(session.message)}</p>
      {#if session.phase !== 'ready'}<p class="screen-warning" role="status">{$t("Keine verbundene Wayland-Quelle. Anfragen mit Bildschirmkontext werden abgewiesen; die App verwendet keinen anderen Bildschirm.")}</p>{/if}
    {:else}
    <div class="display-choice"><label>{$t("Bildschirm")}<select bind:value={config.displayId} disabled={disabled || loading}><option value="">{$t("Hauptbildschirm (automatisch)")}</option>{#if config.displayId && !displays?.some(display => display.id === config.displayId)}<option value={config.displayId}>{missingDisplay ? $t("Nicht verfügbar") : $t("Gespeicherte Auswahl")} · {config.displayId}</option>{/if}{#each displays || [] as display (display.id)}<option value={display.id}>{display.name || display.id}{display.primary ? ' · ' + $t('Hauptbildschirm') : ''}</option>{/each}</select></label><button class="button secondary" onclick={loadDisplays} disabled={disabled || loading}>{loading ? $t("Lädt …") : $t("Bildschirme laden")}</button></div>
    {/if}
    {#if !visionEnabled}<p class="screen-warning" role="status">{$t("Die Bildübertragung ist für diesen Bot ausgeschaltet. Aktiviere „Bilder an das Modell senden“ unter Modelle, damit Bildschirmkontext funktioniert.")}</p>{/if}
    {#if session?.backend !== 'wayland' && missingDisplay}<p class="screen-warning" role="status">{$t("Der ausgewählte Bildschirm ist nicht verfügbar. Wähle einen erreichbaren Bildschirm; die App wechselt nicht automatisch auf einen anderen.")}</p>{/if}
    {#if displays?.length === 0}<p class="screen-warning" role="status">{$t("Das Betriebssystem hat keine Bildschirme gemeldet. Bildschirmkontext ist so nicht verfügbar.")}</p>{/if}
    {#if !native}<p class="screen-warning" role="status">{$t("Bildschirmaufnahmen sind nur in der Desktop-App verfügbar. Die Browseransicht kann deinen Bildschirm nicht aufnehmen.")}</p>{/if}
    <p class="field-note">{$t("Das Betriebssystem kann eine Freigabe für Bildschirmaufnahmen verlangen. Vor dem Start wird die Freigabe geprüft; bei fehlender Freigabe hilft ein Dialog beim Anfragen und erneuten Prüfen. Screenshots bleiben nur während der Anfrage im Arbeitsspeicher, werden als JPEG auf höchstens 1.280 Pixel Kantenlänge verkleinert und nicht als Datei gespeichert.")}</p>
  {/if}
  {#if error}<p class="screen-warning" role="status">{$t(error)}</p>{/if}
</section>

<style>
  .screen-context { margin-bottom: 20px; }
  .toggle-row strong { display: flex; gap: 7px; align-items: center; }
  .display-choice { display: flex; gap: 12px; align-items: flex-end; }
  .display-choice label { flex: 1; min-width: 0; }
  .display-choice button { margin-bottom: 16px; white-space: nowrap; }
  .screen-warning { color: #f1b8a8; font-size: 12px; line-height: 1.7; }
  @media (max-width: 650px) { .display-choice { align-items: stretch; flex-direction: column; gap: 0; } }
</style>
