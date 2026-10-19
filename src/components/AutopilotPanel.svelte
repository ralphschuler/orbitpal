<script lang="ts">
  import { t } from '../lib/i18n';
  import { Play, Pause, Sparkles, Footprints, MessageCircle } from '@lucide/svelte';
  import HelpTip from './HelpTip.svelte';
  import type { SpeechStatus } from '../lib/autopilot-status';
  interface Props {
    config: any;
    botName: string;
    active: boolean;
    speechStatus: SpeechStatus;
    lastAttemptSilent?: boolean;
    disabled?: boolean;
    pending?: boolean;
    previewing?: boolean;
    onStart: () => void;
    onStop: () => void;
    onPreview: () => void;
  }
  let { config = $bindable(), botName, active, speechStatus, lastAttemptSilent = false, disabled = false, pending = false, previewing = false, onStart, onStop, onPreview }: Props = $props();
</script>

<section class="settings-panel autopilot-intro" aria-label={$t("Autopilot steuern")}>
  <div class="autopilot-heading"><div><span class="eyebrow">{$t("EIN EIGENES KLEINES LEBEN")}</span><h2>{$t("{name} macht auch mal den Anfang.", { name: botName })}</h2><p>{$t("Herumlaufen, kleine Gesten und gelegentlich ein eigener Kommentar. Persönlichkeit, Skills, Wissen und Erinnerungen bleiben die dieses Bots.")}</p></div><span class="autopilot-status" class:active>{active ? $t("Autopilot läuft") : $t("Pausiert")}</span></div>
  <div class="autopilot-actions">{#if active}<button class="button primary" onclick={onStop} disabled={pending}><Pause size={16}/>{$t("Autopilot pausieren")}</button>{:else}<button class="button primary" onclick={onStart} disabled={disabled || pending}>{pending ? $t("Wird gestartet …") : $t("Autopilot starten")}<Play size={15}/></button>{/if}<button class="button secondary" onclick={onPreview} disabled={disabled || active}>{previewing ? $t("Vorschau beenden") : $t("Bewegung ansehen")}<Footprints size={15}/></button></div>
  {#if active}
    <div class="speech-status" class:waiting={speechStatus.waiting} aria-label={$t('Selbstständige Sprachausgabe')}>
      <strong>{$t('Sprechstatus')}</strong><p aria-live="off">{$t(speechStatus.message,speechStatus.parameters)}</p>
      {#if lastAttemptSilent}<small>{$t('Beim letzten Sprechversuch gab es keinen Beitrag.')}</small>{/if}
      <small>{$t('Der Countdown gilt für den nächsten KI-Versuch. Das Modell kann schweigen; Antworten, Aufnahmen und Bearbeitungen verschieben ihn. Bewegung und Gesten laufen unabhängig weiter.')}</small>
    </div>
  {/if}
  <p class="field-note">{$t("Start gilt nur für diese App-Sitzung. „Stop“ oder Escape beendet alle Autopiloten und laufenden Ausgaben. Die Bewegungsvorschau ist stumm und verwendet keine KI.")}</p>
</section>

<div class="settings-grid autopilot-settings">
  <section class="settings-panel"><div class="section-heading"><Sparkles size={20}/><div><h2>{$t("Was darf die Figur tun?")}</h2><p>{$t("Jede Aktivität ist für diesen Bot einzeln einstellbar.")}</p></div></div>
    <div class="toggle-row"><div><strong><MessageCircle size={14}/>{$t("Von selbst sprechen")} <HelpTip text={$t("Nach einer ruhigen Phase erzeugt das gewählte Dialogmodell einen kurzen spontanen Beitrag. Die normalen Kosten des Dialogmodells, der Stimme und gegebenenfalls Hindsight fallen an. Das Modell darf auch schweigen. Laufende Gespräche und Aufnahmen haben Vorrang.")}/></strong><small>{$t("Gelegentlich einen Kommentar oder eine Frage einwerfen.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Von selbst sprechen")} bind:checked={config.speak}/></div>
    <div class="toggle-row"><div><strong><Footprints size={14}/>{$t("Herumlaufen")} <HelpTip text={$t("Bewegt die Figur innerhalb ihrer Bühne. Vorhandene Laufanimationen werden bevorzugt; unterstützte menschliche Skelette erhalten einen automatischen Gehzyklus. Das Desktopfenster wird dabei nicht verschoben. Reduzierte Bewegung des Betriebssystems wird berücksichtigt.")}/></strong><small>{$t("Kurze Wege mit Ruhepausen, automatisch am Modell ausgerichtet.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Im Autopilot herumlaufen")} bind:checked={config.wander}/></div>
    <div class="toggle-row"><div><strong>{$t("Kleine Gesten")} <HelpTip text={$t("Gelegentliche stumme Bewegungen zwischen den Laufwegen. Eigene Animationen und unterstützte App-Gesten werden automatisch gewählt. Manuelle Emotes und die Sprachausgabe haben Vorrang.")}/></strong><small>{$t("Auch zwischen Gesprächen lebendig bleiben.")}</small></div><input type="checkbox" role="switch" aria-label={$t("Gesten im Autopilot")} bind:checked={config.gestures}/></div>
    <p class="field-note">{$t("Am besten mit „Ganze Figur“ unter Charakter & Stimme. Die Figur bleibt innerhalb ihrer Bühne.")}</p>
  </section>
  <section class="settings-panel"><div class="section-heading"><MessageCircle size={20}/><div><h2>{$t("Wie oft spricht {name}?", { name: botName })}</h2><p>{$t("Abstände beginnen nach der letzten Gesprächsaktivität.")}</p></div></div>
    <div class="field-row"><label>{$t("Mindestens Sekunden")} <HelpTip text={$t("So lange bleibt der Bot nach Gespräch, Aufnahme oder eigenem Beitrag mindestens ruhig. Der nächste Zeitpunkt wird zufällig zwischen Minimum und Maximum gewählt.")}/><input type="number" min="15" max="3600" step="1" bind:value={config.minSeconds}/></label><label>{$t("Höchstens Sekunden")}<input type="number" min="15" max="3600" step="1" bind:value={config.maxSeconds}/></label></div>
    <label>{$t("Höchstens KI-Aufrufe pro Stunde")} <HelpTip text={$t("Ein rollendes Stundenbudget pro Bot. Auch fehlgeschlagene oder stille Versuche zählen. Eine neue Sitzung setzt das Budget während derselben App-Laufzeit nicht zurück. Bewegungen verbrauchen keine KI-Aufrufe.")}/><input type="number" min="1" max="120" step="1" bind:value={config.maxTurnsPerHour}/></label>
    <label>{$t("Zusätzliche Hinweise für spontane Beiträge")}<textarea rows="4" maxlength="4000" bind:value={config.instructions} placeholder={$t("Zum Beispiel: Kommentiere trocken, stelle gelegentlich eine Frage und wiederhole dich nicht.")}></textarea></label>
    <p class="field-note">{$t("Bei mehreren Bots wird die Sprache nacheinander ausgegeben. Neue Chat-Antworten und deine Eingaben kommen zuerst. Autopilot-Beiträge werden nicht automatisch als Plattformnachricht gepostet.")}</p>
  </section>
</div>

<style>
  .autopilot-intro { margin-bottom: 20px; border-color: #42636f; background: radial-gradient(ellipse at top right, #223946 0%, #151f2d 68%); }
  .autopilot-heading { display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; }
  .eyebrow { color: #9fc7d4; font-size: 9px; letter-spacing: .12em; }
  h2 { margin: 10px 0; font-size: 21px; font-weight: 600; }
  p { font-size: 12px; color: #a3b1c6; line-height: 1.8; max-width: 650px; }
  .autopilot-status { font-size: 11px; color: #a4b3c9; white-space: nowrap; border: 1px solid #4b5b72; border-radius: 6px; padding: 6px 10px; }
  .autopilot-status.active { color: #a5ddc7; border-color: #568b77; }
  .autopilot-actions { display: flex; flex-wrap: wrap; gap: 10px; margin-block: 20px 14px; }
  .speech-status { display:grid; gap:6px; padding:13px 15px; margin:0 0 15px; border:1px solid #42636f; border-radius:9px; background:#142b35; }
  .speech-status.waiting { border-color:#6d6049; background:#302b23; }
  .speech-status strong { color:#d3e4e9; font-size:12px; }
  .speech-status p { margin:0; color:#d3e4e9; font-size:13px; font-variant-numeric:tabular-nums; }
  .speech-status small { color:#a3b1c6; font-size:11px; line-height:1.6; }
  .autopilot-settings { margin-bottom: 20px; }
  .toggle-row strong { display: flex; gap: 7px; align-items: center; }
  @media (max-width: 650px) { .autopilot-heading { flex-direction: column-reverse; gap: 8px; } }
</style>
