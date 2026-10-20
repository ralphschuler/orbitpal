<script lang="ts">
  import { Play, Pause, RefreshCw, Activity, ShieldCheck } from '@lucide/svelte';
  import { t } from '../lib/i18n';
  import { activityLabels, phaseLabels, lifeReasons, systemReasons, type LifeMode, type LifeBot, type LifeSnapshot, type LifeSession } from '../lib/life-runtime';
  import HelpTip from './HelpTip.svelte';
  import type { SpeechStatus } from '../lib/autopilot-status';
  interface Props {
    botId: string; botName: string; session?: LifeSession; system?: LifeSnapshot['system']; bot?: LifeBot;
    desktopState?:'opening'|'visible'|'unavailable'|'inactive'; connected: boolean; error?: string; pending?: boolean; disabled?: boolean;
    speechStatus: SpeechStatus; lastAttemptSilent?: boolean; previewing?: boolean; onPreview: () => void;
    onStart: (mode: LifeMode, initialize: boolean) => void; onStop: () => void; onRefresh: () => void; onReconnect: () => void;
  }
  let { botId, botName, session, system, bot, desktopState, connected, error = '', pending = false, disabled = false, speechStatus, lastAttemptSilent = false, previewing = false, onPreview, onStart, onStop, onRefresh, onReconnect }: Props = $props();
  const uid = $props.id();
  let selected = $state<LifeMode>('life');
  let previousBot = '';
  $effect(() => { if (botId !== previousBot) { previousBot = botId; selected = session?.mode ?? 'life'; } });
  $effect(() => { if (session?.active) selected = session.mode; });
  const mode = $derived(session?.active ? session.mode : selected);
  const needsInitialization = $derived(mode === 'life' && bot?.availability === 'not_initialized');
  const actor = $derived(session?.actor ?? (bot?.availability === 'ready' ? bot.actor : undefined));
  const cannotStart = $derived(disabled || pending || mode === 'life' && (!bot || bot.availability === 'unavailable'));
</script>

<section class="settings-panel life-runtime" aria-labelledby={`${uid}-title`}>
  <div class="runtime-heading"><div class="section-heading"><Activity size={20}/><div><h2 id={`${uid}-title`}>{$t('{name} auf dem Desktop', { name: botName })}</h2><p>{$t('Starte deine Figur direkt auf dem Bildschirm. Sie bleibt aktiv, wenn du das Studio ausblendest.')}</p></div></div><button class="button secondary small" onclick={onRefresh} disabled={pending}><RefreshCw size={14}/>{$t('Status aktualisieren')}</button></div>
  <label for={`${uid}-mode`}>{$t('Modus')}</label>
  <select id={`${uid}-mode`} bind:value={selected} disabled={!!session?.active || pending}>
    <option value="ambient">{$t('Ambient · Bewegung und gelegentliche Beiträge')}</option>
    <option value="life">{$t('Life · eigene Aktivitäten und Bewegung')}</option>
  </select>
  <p class="field-note">{mode === 'life' ? $t('Deine Figur bleibt auf dem Desktop. Life merkt sich ihre Aktivitäten. Mit eingerichteter Sprache kann sie Gespräche beginnen.') : $t('Deine Figur läuft umher, macht kleine Gesten und spricht gelegentlich, wenn Sprechen eingerichtet ist.')}</p>
  {#if mode === 'life' && bot?.availability === 'unavailable'}<p class="runtime-warning" role="status">{$t('Der gespeicherte Life-Zustand dieses Bots ist nicht verfügbar. Andere Bots und manuelle Nachrichten bleiben nutzbar.')}</p>{/if}
  <div class="runtime-actions">
    {#if session?.active || pending}<button class="button primary" onclick={onStop}><Pause size={16}/>{$t(pending ? 'Start abbrechen' : 'Figur stoppen')}</button>{:else}<button class="button primary" onclick={() => onStart(mode, needsInitialization)} disabled={cannotStart}><Play size={16}/>{$t(pending ? 'Figur wird gestartet …' : 'Auf Desktop starten')}</button>{/if}
    <button class="button secondary" onclick={onPreview} disabled={disabled || pending || !!session?.active}>{previewing ? $t('Vorschau beenden') : $t('Bewegung ansehen')}</button>
    <span class="runtime-state" role="status">{pending ? $t('Figur wird gestartet …') : session?.active ? session.paused ? $t('Lokaler Modus pausiert') : mode === 'life' ? $t('Life läuft lokal') : $t('Ambient läuft') : $t('Kein selbstständiger Modus gestartet')}</span>
  </div>
  {#if desktopState==='unavailable'}<p class="runtime-warning" role="alert">{$t('Das Figurenfenster konnte nicht geöffnet werden. Stoppe den Modus und starte ihn erneut.')}</p>{:else if desktopState==='opening'}<p class="field-note" role="status">{$t('Das Figurenfenster wird geöffnet …')}</p>{/if}
  {#if session}<p class="field-note">{$t(lifeReasons[session.reason])}</p>{/if}
  {#if session?.active}<div class="speech-status"><strong>{$t('Sprechstatus')}</strong><p>{$t(speechStatus.message, speechStatus.parameters)}</p><p class="field-note">{$t('{attempts} von {limit} Sprechversuchen im rollenden Stundenfenster.',{attempts:session.speech.attempts,limit:session.speech.limit})}</p>{#if lastAttemptSilent}<small>{$t('Beim letzten Sprechversuch gab es keinen Beitrag.')}</small>{/if}</div>{/if}
  <p class="field-note">{$t('Stop oder Escape beendet die selbstständigen Aktivitäten. Nach einem App-Neustart startest du sie erneut.')}</p>
  <details class="availability"><summary>{$t('Status und Sprechfreigabe')}</summary><h3><ShieldCheck size={16}/>{$t('Freigabe für selbstständige Anfragen')}<HelpTip text={$t('Ein unbekannter Sperrstatus erlaubt keine selbstständigen Modell-, Gedächtnis- oder Sprachanfragen. Stop bleibt nutzbar. Lokales Life kann bei unbekanntem Status weiterlaufen; bei bestätigter Sperre oder Schlaf pausiert es.')}/></h3><p class:runtime-warning={!system || system.state !== 'awake_unlocked'}>{$t(system ? systemReasons[system.reason] : 'Systemstatus noch nicht bestätigt.')}</p>
    {#if !connected}<p class="runtime-warning">{$t('Studio-Verbindung nicht bestätigt. Selbstständige Anfragen bleiben gesperrt.')} <button class="text-link" onclick={onReconnect} disabled={pending}>{$t('Erneut verbinden')}</button></p>{/if}
    {#if system?.source === 'macos_cg_session_compatibility'}<p class="field-note">{$t('macOS verwendet eine Kompatibilitätsprüfung. Apple garantiert damit nicht alle Fälle von Sperre, Benutzerwechsel oder Bildschirmfreigabe. Unbekannter Status sperrt selbstständige Anfragen.')}</p>{:else if system?.source === 'linux_logind'}<p class="field-note">{$t('Unter Linux hängt die Sperrerkennung davon ab, ob dein Desktop den Sperrstatus an logind meldet.')}</p>{/if}
  </details>
  {#if mode === 'life' && actor}<div class="local-state"><h3>{$t('Bestätigter lokaler Zustand')}</h3><p>{actor.activity ? `${$t(activityLabels[actor.activity.activity])} · ${$t(phaseLabels[actor.activity.phase])}` : $t('Keine bestätigte Aktivität.')}</p><p class="field-note">{$t('Angezeigt wird nur gespeicherter oder nativ bestätigter Zustand. Ein Neustart startet den Modus nicht automatisch.')}</p></div>{/if}
  {#if error}<p class="runtime-error" role="alert">{error}</p>{/if}
</section>

<style>
  .life-runtime { margin-bottom: 20px; }
  .runtime-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; }
  .runtime-heading .section-heading { margin-bottom: 18px; }
  .runtime-heading button { flex-shrink: 0; }
  .section-heading p, .field-note { color: var(--muted); font-size: 12px; line-height: 1.7; }
  .runtime-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 16px; margin: 20px 0 10px; }
  .runtime-state { color: var(--muted); font-size: 12px; }
  .speech-status { margin: 16px 0; padding: 14px; background: var(--input-bg); border-left: 3px solid var(--accent); }
  .speech-status p, .speech-status small { font-size: 12px; line-height: 1.7; }
  .availability, .local-state { border-top: 1px solid var(--line); padding-top: 16px; margin-top: 20px; }
  summary { cursor: pointer; font-size: 12px; font-weight: 600; padding: 4px 0; }
  details[open] summary { margin-bottom: 14px; }
  h3 { display: flex; align-items: center; gap: 7px; }
  .availability p, .local-state p { font-size: 12px; line-height: 1.7; }
  .runtime-warning, .runtime-error { padding: 12px 14px; border-left: 3px solid var(--warning); background: var(--input-bg); font-size: 12px; line-height: 1.7; }
  .runtime-warning { color: var(--warning); }
  .runtime-error { color: var(--danger); border-color: var(--danger); }
  @media(max-width:680px) { .runtime-heading { flex-direction: column; gap: 0; } .runtime-heading button { margin-bottom: 18px; } }
</style>
