<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { Mic, MicOff, Settings2, Square, ChevronDown } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import { WakeWordListener, normalizeWakePhrases, type WakeWordState } from '../lib/wake-word';

  interface Props {
    config: any;
    botId: string;
    sttProvider: string;
    blocked?: boolean;
    stopSignal?: number;
    onTrigger: (text: string) => Promise<void> | void;
    onError: (message: string) => void;
    onConfigChange?: () => Promise<boolean | void> | boolean | void;
    onActiveChange?: (active: boolean) => void;
  }

  let { config = $bindable(), botId, sttProvider, blocked = false, stopSignal = 0, onTrigger, onError, onConfigChange, onActiveChange }: Props = $props();
  let status = $state<WakeWordState>('stopped');
  let expanded = $state(false);
  let phraseDraft = $state('');
  let failure = $state('');
  let saving = $state(false);
  let listener: WakeWordListener | undefined;
  let configSignature = '';
  let previousStop = 0;
  let epoch = 0;
  let destroyed = false;
  let lastNotification = '';
  let lastNotificationAt = 0;
  const active = $derived(status !== 'stopped');
  $effect(() => { onActiveChange?.(active); });
  const local = $derived(sttProvider === 'whisper-local');
  const providerName = $derived(({ 'whisper-local': 'Lokales Whisper', openai: 'OpenAI Whisper · Cloud', elevenlabs: 'ElevenLabs Scribe · Cloud', compatible: 'Konfigurierte STT-API' } as Record<string, string>)[sttProvider] || 'Keine Spracherkennung');
  const phrases = $derived(normalizeWakePhrases(config?.phrases));
  const labels: Record<WakeWordState, string> = {
    stopped: 'Zuhören beendet', starting: 'Mikrofon wird aktiviert …', listening: 'Wartet auf Aktivierungsphrase', capturing: 'Audioabschnitt wird aufgenommen …',
    transcribing: 'Sprachabschnitt wird erkannt …', 'follow-up': 'Phrase erkannt · jetzt deine Frage stellen (10 Sek.)', triggered: 'Frage an den Cohost übergeben', paused: 'Vorübergehend pausiert · hört nach der Antwort automatisch weiter', retrying: 'Verbindung unterbrochen · Zuhören wird automatisch erneut versucht',
  };

  function stopListening() {
    ++epoch;
    listener?.stop();
    listener = undefined;
    status = 'stopped';
    saving = false;
  }

  function showError(message: string) {
    failure = message;
    // Keep the inline error current without flooding notifications on a flaky
    // STT connection. A later explicit Start click allows a fresh notification.
    const recovering = status === 'retrying' || status === 'paused';
    if (!lastNotification || Date.now() - lastNotificationAt >= 30_000 || (!recovering && message !== lastNotification)) {
      lastNotification = message;
      lastNotificationAt = Date.now();
      onError(message);
    }
  }

  function updateConfig(next: any) {
    // Internal edits are already explicitly stopped/saved by their handler;
    // do not mistake the Start button's own commit for an external bot change.
    configSignature = JSON.stringify([botId, sttProvider, next?.enabled === true, next?.phrases]);
    config = next;
  }

  function editedPhrases(): string[] {
    const lines = phraseDraft.split('\n').map((phrase) => phrase.trim()).filter(Boolean);
    if (lines.length > 8 || lines.some((phrase) => phrase.length > 100)) throw new Error('Verwende höchstens acht Aktivierungsphrasen mit jeweils maximal 100 Zeichen.');
    const valid = normalizeWakePhrases(lines);
    if (!valid.length) throw new Error('Gib mindestens eine Aktivierungsphrase ein, zum Beispiel „hey nova“.');
    return valid;
  }

  async function saveSettings() {
    stopListening();
    failure = '';
    try {
      updateConfig({ ...config, phrases: editedPhrases() });
      saving = true;
      await onConfigChange?.();
    } catch (error) { showError(prettyError(error)); }
    finally { saving = false; }
  }

  async function startListening() {
    if (blocked || saving || destroyed) return;
    if (!native) { showError('Sprachaktivierung ist in der Desktop-App verfügbar.'); return; }
    if (!['whisper-local', 'openai', 'elevenlabs', 'compatible'].includes(sttProvider)) {
      showError('Wähle unter Modelle zuerst einen Spracherkennungsanbieter.'); return;
    }
    stopListening();
    failure = '';
    lastNotification = '';
    let next: WakeWordListener | undefined;
    try {
      const selected = editedPhrases();
      updateConfig({ ...config, enabled: true, phrases: selected });
      const currentBot = botId;
      const currentEpoch = epoch;
      // The captured ID belongs to this explicit listening session. Late audio
      // may never become a request for a newly selected bot.
      next = new WakeWordListener({
        phrases: selected,
        transcribe: (audio) => call<string>('transcribe_audio', { ...audio, botId: currentBot }),
        onTrigger: async (text) => {
          if (!destroyed && currentEpoch === epoch && botId === currentBot && !blocked) await onTrigger(text);
        },
        onState: (state) => {
          if (!destroyed && listener === next) {
            if (state === 'listening' || state === 'follow-up') failure = '';
            status = state;
          }
        },
        onError: (message) => { if (!destroyed && listener === next) showError(message); },
      });
      listener = next;
      await next.start(onConfigChange);
    } catch (error) {
      if (!destroyed && listener === next) { stopListening(); showError(prettyError(error)); }
      else if (!next && !destroyed) showError(prettyError(error));
    }
  }

  $effect(() => {
    const signature = JSON.stringify([botId, sttProvider, config?.enabled === true, config?.phrases]);
    untrack(() => {
      if (signature === configSignature) return;
      const changedBot = configSignature && JSON.parse(configSignature)[0] !== botId;
      configSignature = signature;
      stopListening();
      phraseDraft = normalizeWakePhrases(config?.phrases).join('\n');
      failure = '';
      lastNotification = '';
      if (changedBot) expanded = false;
    });
  });

  $effect(() => {
    const paused = blocked;
    untrack(() => listener?.setBlocked(paused));
  });

  $effect(() => {
    const signal = stopSignal;
    untrack(() => { if (signal !== previousStop) { previousStop = signal; stopListening(); } });
  });

  onDestroy(() => { destroyed = true; stopListening(); onActiveChange?.(false); });
</script>

<section class="wake-word" class:listening={active && status !== 'paused'} aria-label="Sprachaktivierung">
  <div class="wake-bar">
    <span class="wake-icon" aria-hidden="true">{#if active}<Mic size={17}/>{:else}<MicOff size={17}/>{/if}</span>
    <div class="wake-copy">
      <strong>Sprachaktivierung <span>· {config?.enabled ? 'pro Bot' : 'ausgeschaltet'}</span></strong>
      <span class="wake-status" role="status">{active ? labels[status] : config?.enabled ? phrases.map((phrase) => `„${phrase}“`).join(' / ') || 'Noch keine Phrase eingerichtet' : 'Mit einer eigenen Phrase ansprechen.'}</span>
    </div>
    <span class="provider-badge" class:local title="Dieser Anbieter verarbeitet die erkannten Audioabschnitte">{providerName}</span>
    {#if active}
      <button class="wake-button stop" onclick={stopListening}><Square size={13}/>Zuhören stoppen</button>
    {:else if config?.enabled}
      <button class="wake-button start" disabled={blocked || saving || !native} onclick={startListening} title={blocked ? 'Wartet, bis Antwort und andere Mikrofonaufnahme beendet sind.' : ''}>
        <Mic size={14}/>{local ? 'Zuhören starten' : 'Zuhören starten · Audio senden'}
      </button>
    {:else}
      <button class="wake-button" onclick={() => expanded = true}>Einrichten</button>
    {/if}
    <button class="wake-settings" class:open={expanded} onclick={() => expanded = !expanded} aria-expanded={expanded} aria-controls={`wake-settings-${botId}`} aria-label="Sprachaktivierung einstellen"><Settings2 size={16}/><ChevronDown size={12}/></button>
  </div>
  {#if config?.enabled && !local}<p class="wake-disclosure">{active ? 'Audioübertragung aktiv: ' : ''}Auch Abschnitte vor der Aktivierungsphrase werden vom gewählten STT-Anbieter verarbeitet.</p>{/if}
  {#if failure}<p class="wake-error" role="alert">{failure}</p>{/if}
  {#if expanded}
    <div class="wake-details" id={`wake-settings-${botId}`}>
      <label class="wake-toggle"><input type="checkbox" checked={config?.enabled === true} onchange={(event) => { stopListening(); updateConfig({ ...config, enabled: event.currentTarget.checked }); }}/><span>Aktivierungsphrase für diesen Bot verwenden</span></label>
      <div class="wake-config">
        <label class="wake-phrases">Aktivierungsphrasen <span>Eine pro Zeile · bis zu acht</span><textarea rows="2" bind:value={phraseDraft} disabled={active} placeholder="hey nova" spellcheck="false"></textarea></label>
        <div class="wake-explanation">
          <p>Gesagt wird zum Beispiel <strong>„hey nova, wie geht es dir?“</strong>. Sagst du nur die Phrase, wartet der Cohost zehn Sekunden auf deine Frage. Ganze Wörter zählen; Großschreibung und Satzzeichen spielen keine Rolle.</p>
          <p>Der gewählte STT-Anbieter verarbeitet jeden erkannten Audioabschnitt, auch <strong>vor</strong> der Aktivierungsphrase. {local ? 'Lokales Whisper benötigt einen separat gestarteten Whisper-Server.' : 'Bei einem Cloud-Anbieter verlässt dieses Audio deinen Rechner.'} Anbieter und Zugang richtest du unter „Modelle“ ein.</p>
          <p>Das Mikrofon startet ausschließlich mit deinem Klick auf „Zuhören starten“. Bei Antworten, Emote-Sounds und anderen Aufnahmen pausiert es und hört danach automatisch weiter. Vorübergehende Fehler werden mit wachsendem Abstand bis zu fünfmal erneut versucht; sage den Abschnitt danach bitte noch einmal. „Stop“, ein Botwechsel oder entzogene Mikrofonrechte benötigen einen neuen Start.</p>
        </div>
      </div>
      <div class="wake-footer"><span>{active ? 'Zum Bearbeiten das Zuhören stoppen.' : 'Gespeicherte Einstellungen starten keine Aufnahme.'}</span><button class="wake-button" disabled={saving || active} onclick={saveSettings}>{saving ? 'Speichert …' : 'Einstellungen speichern'}</button></div>
    </div>
  {/if}
</section>

<style>
  .wake-word { margin: 0 0 20px; border: 1px solid #e1e5eb; background: #ffffffb8; border-radius: 13px; overflow: hidden; color: #3c485c; }
  .wake-bar { display: flex; align-items: center; gap: 11px; padding: 12px 15px; min-height: 58px; }
  .wake-icon { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 10px; color: #8390a3; background: #edf0f4; flex: 0 0 auto; }
  .listening .wake-icon { background: #e4f0e9; color: #53816b; }
  .wake-copy { min-width: 120px; flex: 1; display: flex; flex-direction: column; gap: 3px; }
  .wake-copy strong { font-size: 12px; font-weight: 650; }
  .wake-copy strong span { font-weight: 450; color: #8992a0; }
  .wake-status { font-size: 11px; color: #7d8797; line-height: 1.4; }
  .provider-badge { background: #f2edf9; color: #87749d; font-size: 10px; padding: 5px 8px; border-radius: 7px; white-space: nowrap; }
  .provider-badge.local { background: #edf1f6; color: #7c8a9e; }
  .wake-button, .wake-settings { display: inline-flex; align-items: center; justify-content: center; gap: 7px; font-family: inherit; font-size: 11px; font-weight: 600; border: 1px solid #dfe4ec; background: #fff; border-radius: 8px; color: #65758b; min-height: 32px; padding: 6px 10px; cursor: pointer; white-space: nowrap; }
  .wake-button:hover, .wake-settings:hover { background: #f5f7fa; }
  .wake-button:disabled { opacity: 0.5; cursor: default; }
  .wake-button.start { background: #edf2fb; border-color: #dce4f3; color: #6b80aa; }
  .wake-button.stop { color: #8c6d60; background: #faf4ef; border-color: #eee1d7; }
  .wake-settings { border-color: transparent; padding: 7px; color: #8b95a4; }
  .wake-settings.open { background: #f0f3f8; }
  .wake-disclosure, .wake-error { margin: 0; padding: 0 16px 12px 58px; color: #8c739b; font-size: 11px; line-height: 1.6; }
  .wake-error { color: #ab665b; }
  .wake-details { border-top: 1px solid #e8ebf0; padding: 17px 18px; background: #fbfcfe; }
  .wake-toggle { display: flex; align-items: center; gap: 9px; font-size: 12px; font-weight: 550; margin-bottom: 16px; }
  .wake-toggle input { width: 15px; height: 15px; margin: 0; accent-color: #839ac2; }
  .wake-config { display: grid; grid-template-columns: minmax(180px, 0.7fr) minmax(240px, 1.3fr); gap: 25px; }
  .wake-phrases { display: flex; flex-direction: column; gap: 6px; font-size: 11px; font-weight: 600; }
  .wake-phrases > span { font-size: 10px; font-weight: 450; color: #97a0ad; }
  .wake-phrases textarea { width: 100%; box-sizing: border-box; padding: 10px 12px; min-height: 76px; resize: vertical; font: inherit; font-size: 12px; line-height: 1.6; color: #56657a; background: #fff; border: 1px solid #dfe5ed; border-radius: 9px; outline: none; }
  .wake-phrases textarea:focus { border-color: #a4b5d0; box-shadow: 0 0 0 3px #8ea5c219; }
  .wake-explanation p { font-size: 11px; line-height: 1.7; color: #8791a0; margin: 0 0 8px; }
  .wake-explanation strong { color: #6e7c90; font-weight: 600; }
  .wake-footer { display: flex; justify-content: space-between; align-items: center; gap: 15px; padding-top: 12px; }
  .wake-footer > span { font-size: 10px; color: #99a1ae; }
  @media (max-width: 1000px) { .provider-badge { display: none; } .wake-config { grid-template-columns: 1fr; gap: 16px; } }
  @media (max-width: 680px) { .wake-bar { flex-wrap: wrap; } .wake-copy { flex-basis: calc(100% - 55px); } .wake-button { margin-left: 43px; } .wake-settings { margin-left: auto; } .wake-footer .wake-button { margin-left: 0; } .wake-disclosure, .wake-error { padding-left: 15px; } }
</style>
