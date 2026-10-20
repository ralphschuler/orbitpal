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
    showControls?: boolean;
    onStart: () => void;
    onStop: () => void;
    onPreview: () => void;
  }
  let { config = $bindable(), botName, active, speechStatus, lastAttemptSilent = false, disabled = false, pending = false, previewing = false, showControls = true, onStart, onStop, onPreview }: Props = $props();
  const uid = $props.id();
</script>

{#if showControls}<section class="settings-panel autopilot-intro" aria-label={$t('Autopilot steuern')}>
  <div class="autopilot-heading">
    <div>
      <h2>{$t('Autopilot steuern')}</h2>
      <p>{$t('Herumlaufen, kleine Gesten und gelegentlich ein eigener Kommentar. Persönlichkeit, Skills, Wissen und Erinnerungen bleiben die dieses Bots.')}</p>
    </div>
    <span class="autopilot-status" class:active class:previewing>
      {pending ? $t('Wird gestartet …') : active ? $t('Autopilot läuft') : previewing ? $t('Bewegungsvorschau') : $t('Pausiert')}
    </span>
  </div>

  <div class="autopilot-actions">
    {#if active}
      <button class="button primary" onclick={onStop} disabled={pending}>
        <Pause size={16}/>{$t('Autopilot pausieren')}
      </button>
    {:else}
      <button class="button primary" onclick={onStart} disabled={disabled || pending}>
        <Play size={16}/>{pending ? $t('Wird gestartet …') : $t('Autopilot starten')}
      </button>
    {/if}
    <button class="button secondary" onclick={onPreview} disabled={disabled || active}>
      <Footprints size={16}/>{previewing ? $t('Vorschau beenden') : $t('Bewegung ansehen')}
    </button>
  </div>

  {#if active}
    <div class="speech-status" class:waiting={speechStatus.waiting} aria-label={$t('Selbstständige Sprachausgabe')}>
      <strong>{$t('Sprechstatus')}</strong>
      <p aria-live="off">{$t(speechStatus.message, speechStatus.parameters)}</p>
      {#if lastAttemptSilent}
        <small>{$t('Beim letzten Sprechversuch gab es keinen Beitrag.')}</small>
      {/if}
      <small>{$t('Der Countdown gilt für den nächsten KI-Versuch. Das Modell kann schweigen; Antworten, Aufnahmen und Bearbeitungen verschieben ihn. Bewegung und Gesten laufen unabhängig weiter.')}</small>
    </div>
  {/if}
  <p class="session-note">{$t('Start gilt nur für diese App-Sitzung. „Stop“ oder Escape beendet alle Autopiloten und laufenden Ausgaben. Die Bewegungsvorschau ist stumm und verwendet keine KI.')}</p>
</section>{/if}

<div class="settings-grid autopilot-settings">
  <section class="settings-panel activity-settings">
    <div class="section-heading">
      <Sparkles size={20}/>
      <div>
        <h2>{$t('Was darf die Figur tun?')}</h2>
        <p>{$t('Jede Aktivität ist für diesen Bot einzeln einstellbar.')}</p>
      </div>
    </div>

    <div class="toggle-row">
      <div>
        <strong><MessageCircle size={16}/>{$t('Von selbst sprechen')}</strong>
        <small>{$t('Gelegentlich einen Kommentar oder eine Frage einwerfen.')}</small>
      </div>
      <input type="checkbox" role="switch" aria-label={$t('Von selbst sprechen')} bind:checked={config.speak}/>
    </div>
    <p class="cost-note">{$t('Nach einer ruhigen Phase erzeugt das gewählte Dialogmodell einen kurzen spontanen Beitrag. Die normalen Kosten des Dialogmodells, der Stimme und gegebenenfalls Hindsight fallen an. Das Modell darf auch schweigen. Laufende Gespräche und Aufnahmen haben Vorrang.')}</p>

    <div class="toggle-row">
      <div>
        <strong>
          <Footprints size={16}/>{$t('Herumlaufen')}
          <HelpTip text={$t('Vorhandene Laufanimationen werden bevorzugt; unterstützte menschliche Skelette erhalten einen automatischen Gehzyklus. Im Desktopmodus darf die Figur kurz seitlich aus dem Bildschirm laufen. Vor Antworten und manuellen Emotes kehrt sie vollständig sichtbar zurück. Reduzierte Bewegung des Betriebssystems wird berücksichtigt.')}/>
        </strong>
        <small>{$t('Kurze Wege mit Ruhepausen, automatisch am Modell ausgerichtet.')}</small>
      </div>
      <input type="checkbox" role="switch" aria-label={$t('Im Autopilot herumlaufen')} bind:checked={config.wander}/>
    </div>
    <div class="toggle-row">
      <div>
        <strong>
          {$t('Kleine Gesten')}
          <HelpTip text={$t('Gelegentliche Bewegungen zwischen den Laufwegen. Eigene Animationen und unterstützte App-Gesten werden automatisch gewählt. Mitgelieferte Animationssounds können abgespielt werden. Manuelle Emotes und die Sprachausgabe haben Vorrang.')}/>
        </strong>
        <small>{$t('Auch zwischen Gesprächen lebendig bleiben.')}</small>
      </div>
      <input type="checkbox" role="switch" aria-label={$t('Gesten im Autopilot')} bind:checked={config.gestures}/>
    </div>
    <p class="session-note">{$t('Am besten mit „Ganze Figur“ unter Charakter & Stimme. Vor Antworten und manuellen Emotes kehrt die Desktopfigur vollständig ins Bild zurück.')}</p>
  </section>

  <section class="settings-panel timing-settings">
    <div class="section-heading">
      <MessageCircle size={20}/>
      <div>
        <h2>{$t('Wie oft spricht {name}?', { name: botName })}</h2>
        <p>{$t('Abstände beginnen nach der letzten Gesprächsaktivität.')}</p>
      </div>
    </div>
    <div class="field-row">
      <label for={`${uid}-min-seconds`}>
        {$t('Mindestens Sekunden')}
        <HelpTip text={$t('So lange bleibt der Bot nach Gespräch, Aufnahme oder eigenem Beitrag mindestens ruhig. Der nächste Zeitpunkt wird zufällig zwischen Minimum und Maximum gewählt.')}/>
        <input type="number" min="15" max="3600" step="1" id={`${uid}-min-seconds`} bind:value={config.minSeconds}/>
      </label>
      <label>
        {$t('Höchstens Sekunden')}
        <input type="number" min="15" max="3600" step="1" bind:value={config.maxSeconds}/>
      </label>
    </div>
    <div class="field-with-help">
      <div class="field-label">
        <label for={`${uid}-hourly-budget`}>{$t('Höchstens Sprechversuche pro Stunde')}</label>
        <HelpTip label={$t('Hilfe zum Stundenlimit')} text={$t('Rollendes Stundenlimit pro Bot: Stille und fehlgeschlagene Versuche zählen mit. Stop und Neustart des Autopiloten setzen es während derselben App-Laufzeit nicht zurück. Ein Versuch kann mehrere Modell- und Werkzeugrunden sowie weitere Anfragen für Stimme und Hindsight auslösen. Das Limit begrenzt weder einzelne Anbieteranfragen noch einen Geldbetrag. Bewegungen zählen nicht.')}/>
      </div>
      <input type="number" min="1" max="120" step="1" id={`${uid}-hourly-budget`} aria-describedby={`${uid}-hourly-budget-note`} bind:value={config.maxTurnsPerHour}/>
      <p class="field-note" id={`${uid}-hourly-budget-note`}>{$t('Pro Bot; auch reine Textbeiträge bei ausgeschalteter Stimme zählen als Sprechversuch.')}</p>
    </div>
    <details class="settings-details">
      <summary>{$t('Zusätzliche Hinweise für spontane Beiträge')}</summary>
      <label>
        {$t('Zusätzliche Hinweise für spontane Beiträge')}
        <textarea rows="4" maxlength="4000" bind:value={config.instructions} placeholder={$t('Zum Beispiel: Kommentiere trocken, stelle gelegentlich eine Frage und wiederhole dich nicht.')}></textarea>
      </label>
    </details>
    <p class="session-note">{$t('Bei mehreren Bots wird die Sprache nacheinander ausgegeben. Neue Chat-Antworten und deine Eingaben kommen zuerst. Autopilot-Beiträge werden nicht automatisch als Plattformnachricht gepostet.')}</p>
  </section>
</div>

<style>
  .autopilot-intro { margin-bottom: 20px; }
  .autopilot-heading { display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; }
  .autopilot-heading > div { min-width: 0; }
  h2 { margin: 0; font-size: 17px; font-weight: 600; color: var(--text); }
  .autopilot-heading p { max-width: 70ch; margin-top: 8px; color: var(--muted); font-size: 12px; line-height: 1.7; }
  .autopilot-status { display: inline-flex; align-items: center; min-height: 30px; font-size: 12px; color: var(--muted); white-space: nowrap; border: 1px solid var(--line); border-radius: 6px; padding: 5px 10px; background: var(--input-bg); }
  .autopilot-status.active, .autopilot-status.previewing { color: var(--accent); border-color: var(--accent); }
  .autopilot-actions { display: flex; flex-wrap: wrap; gap: 10px; margin-block: 20px 16px; }
  .speech-status { display: grid; gap: 7px; padding: 16px; margin-bottom: 16px; border-left: 3px solid var(--accent); border-radius: 6px; background: var(--surface-raised); }
  .speech-status.waiting { border-left-color: var(--warning); }
  .speech-status strong { color: var(--muted); font-size: 12px; }
  .speech-status p { margin: 0; color: var(--text); font-size: 14px; font-variant-numeric: tabular-nums; line-height: 1.6; }
  .speech-status small, .session-note, .cost-note { color: var(--muted); font-size: 12px; line-height: 1.7; }
  .session-note { margin: 16px 0 0; }
  .cost-note { margin: 12px 0 0; padding: 12px 14px; background: var(--input-bg); border-radius: 6px; }
  .autopilot-settings { margin-bottom: 20px; }
  .section-heading { margin-bottom: 20px; }
  .section-heading p { color: var(--muted); font-size: 12px; line-height: 1.6; }
  .toggle-row strong { display: flex; flex-wrap: wrap; gap: 7px; align-items: center; color: var(--text); font-size: 13px; }
  .toggle-row small { color: var(--muted); font-size: 12px; line-height: 1.65; }
  .toggle-row:first-of-type { margin-top: 0; }
  label { color: var(--text); font-size: 12px; line-height: 1.6; }
  input[type='number'], textarea { min-height: 40px; color: var(--text); background: var(--input-bg); border-color: var(--line); font-size: 13px; }
  .field-note { margin: 7px 0 0; overflow-wrap: anywhere; }
  .field-label > label { min-width: 0; }
  .settings-details { margin-top: 20px; border-top: 1px solid var(--line); }
  summary { padding: 16px 0; color: var(--text); font-size: 12px; font-weight: 600; line-height: 1.6; cursor: pointer; }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; border-radius: 3px; }
  .settings-details label { margin-top: 0; }
  @media (max-width: 650px) {
    .autopilot-heading { flex-direction: column; gap: 12px; }
    .field-row { flex-wrap: wrap; }
    .field-row label { min-width: 120px; }
  }
</style>
