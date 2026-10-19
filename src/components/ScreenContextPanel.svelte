<script lang="ts">
  import { t } from '../lib/i18n';
  import { onDestroy } from 'svelte';
  import { Eye } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import HelpTip from './HelpTip.svelte';

  interface ScreenContextConfig { enabled: boolean; displayId: string }
  interface CaptureDisplay { id: string; name: string; primary: boolean }
  interface Props {
    config: ScreenContextConfig;
    visionEnabled: boolean;
    disabled?: boolean;
  }
  let { config = $bindable(), visionEnabled, disabled = false }: Props = $props();
  let displays = $state<CaptureDisplay[] | null>(null);
  let loading = $state(false);
  let error = $state('');
  let request = 0;
  const missingDisplay = $derived(displays !== null && !!config.displayId && !displays.some(display => display.id === config.displayId));

  onDestroy(() => { ++request; });

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
  <div class="toggle-row"><div><strong>{$t("Bildschirm bei jeder Anfrage mitsenden")} <HelpTip text={$t("Die Desktop-App nimmt vor jedem Aufruf des Dialogmodells einen aktuellen Screenshot auf und fügt ihn als Bild hinzu, auch bei normalen Chat-Antworten und im Autopilot. MCP-Server, Skills und Werkzeugfreigaben werden dafür nicht benötigt. Diese Einstellung erlaubt keine Computersteuerung.")}/></strong><small>{$t("Aktueller Screenshot vor jedem Modellaufruf, kein laufendes Video.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Bildschirm bei jeder Anfrage mitsenden")} bind:checked={config.enabled} {disabled}/></div>
  <p class="field-note">{$t("Das Bild wird an das ausgewählte Dialogmodell übertragen. Bei Cloud- oder Remote-Anbietern verlässt es deinen Rechner und kann sichtbare private Inhalte enthalten. Benötigt ein bildfähiges Modell und die aktivierte Einstellung „Bilder an das Modell senden“.")}</p>
  {#if config.enabled}
    <div class="display-choice"><label>{$t("Bildschirm")}<select bind:value={config.displayId} disabled={disabled || loading}><option value="">{$t("Hauptbildschirm (automatisch)")}</option>{#if config.displayId && !displays?.some(display => display.id === config.displayId)}<option value={config.displayId}>{missingDisplay ? $t("Nicht verfügbar") : $t("Gespeicherte Auswahl")} · {config.displayId}</option>{/if}{#each displays || [] as display (display.id)}<option value={display.id}>{display.name || display.id}{display.primary ? ' · ' + $t('Hauptbildschirm') : ''}</option>{/each}</select></label><button class="button secondary" onclick={loadDisplays} disabled={disabled || loading}>{loading ? $t("Lädt …") : $t("Bildschirme laden")}</button></div>
    {#if !visionEnabled}<p class="screen-warning" role="status">{$t("Die Bildübertragung ist für diesen Bot ausgeschaltet. Aktiviere „Bilder an das Modell senden“ unter Modelle, damit Bildschirmkontext funktioniert.")}</p>{/if}
    {#if missingDisplay}<p class="screen-warning" role="status">{$t("Der ausgewählte Bildschirm ist nicht verfügbar. Wähle einen erreichbaren Bildschirm; die App wechselt nicht automatisch auf einen anderen.")}</p>{/if}
    {#if displays?.length === 0}<p class="screen-warning" role="status">{$t("Das Betriebssystem hat keine Bildschirme gemeldet. Bildschirmkontext ist so nicht verfügbar.")}</p>{/if}
    {#if !native}<p class="screen-warning" role="status">{$t("Bildschirmaufnahmen sind nur in der Desktop-App verfügbar. Die Browseransicht kann deinen Bildschirm nicht aufnehmen.")}</p>{/if}
    <p class="field-note">{$t("Das Betriebssystem kann eine Freigabe für Bildschirmaufnahmen verlangen. Bei verweigerter Freigabe zeigt die App einen Fehler. Screenshots bleiben nur während der Anfrage im Arbeitsspeicher, werden als JPEG auf höchstens 1.280 Pixel Kantenlänge verkleinert und nicht als Datei gespeichert.")}</p>
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
