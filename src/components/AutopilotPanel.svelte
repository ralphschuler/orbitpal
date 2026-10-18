<script lang="ts">
  import { Play, Pause, Eye, Sparkles, Footprints, MessageCircle } from '@lucide/svelte';
  import HelpTip from './HelpTip.svelte';
  interface Props {
    config: any;
    botName: string;
    active: boolean;
    disabled?: boolean;
    pending?: boolean;
    previewing?: boolean;
    servers: any[];
    onStart: () => void;
    onStop: () => void;
    onPreview: () => void;
    onLoadTools: (serverId: string) => Promise<any[]>;
  }
  let { config = $bindable(), botName, active, disabled = false, pending = false, previewing = false, servers, onStart, onStop, onPreview, onLoadTools }: Props = $props();
  let tools = $state<any[]>([]);
  let loading = $state(false);
  let toolError = $state('');
  let loadedServer = $state('');
  const availableServers = $derived(servers.filter(server => server.enabled));
  const selectedServer = $derived(availableServers.find(server => server.id === config.screenServerId));
  const allowedTools = $derived(loadedServer === config.screenServerId ? tools.filter(tool => (selectedServer?.allowedTools || []).some((name: string) => name === '*' || name === tool.name)) : []);
  const argsError = $derived.by(() => {
    if (!config.screenContext) return '';
    try { const value = JSON.parse(config.screenToolArguments || '{}'); return value && typeof value === 'object' && !Array.isArray(value) ? '' : 'Die Werkzeugargumente müssen ein JSON-Objekt sein.'; }
    catch { return 'Die Werkzeugargumente enthalten kein gültiges JSON.'; }
  });
  async function loadTools() {
    const id = config.screenServerId;
    if (!id || loading) return;
    loading = true; toolError = '';
    try { const result = await onLoadTools(id); if (id === config.screenServerId) { tools = result; loadedServer = id; } }
    catch (error) { toolError = error instanceof Error ? error.message : String(error); }
    finally { loading = false; }
  }
</script>

<section class="settings-panel autopilot-intro" aria-label="Autopilot steuern">
  <div class="autopilot-heading"><div><span class="eyebrow">EIN EIGENES KLEINES LEBEN</span><h2>{botName} macht auch mal den Anfang.</h2><p>Herumlaufen, kleine Gesten und gelegentlich ein eigener Kommentar. Persönlichkeit, Skills, Wissen und Erinnerungen bleiben die dieses Bots.</p></div><span class="autopilot-status" class:active>{active ? 'Autopilot läuft' : 'Pausiert'}</span></div>
  <div class="autopilot-actions">{#if active}<button class="button primary" onclick={onStop} disabled={pending}><Pause size={16}/>Autopilot pausieren</button>{:else}<button class="button primary" onclick={onStart} disabled={disabled || pending || !!argsError}>{pending ? 'Wird gestartet …' : 'Autopilot starten'}<Play size={15}/></button>{/if}<button class="button secondary" onclick={onPreview} disabled={disabled || active}>{previewing ? 'Vorschau beenden' : 'Bewegung ansehen'}<Footprints size={15}/></button></div>
  <p class="field-note">Start gilt nur für diese App-Sitzung. „Stop“ oder Escape beendet alle Autopiloten und laufenden Ausgaben. Die Bewegungsvorschau ist stumm und verwendet keine KI.</p>
</section>

<div class="settings-grid autopilot-settings">
  <section class="settings-panel"><div class="section-heading"><Sparkles size={20}/><div><h2>Was darf die Figur tun?</h2><p>Jede Aktivität ist für diesen Bot einzeln einstellbar.</p></div></div>
    <div class="toggle-row"><div><strong><MessageCircle size={14}/>Von selbst sprechen <HelpTip text="Nach einer ruhigen Phase erzeugt das gewählte Dialogmodell einen kurzen spontanen Beitrag. Die normalen Kosten des Dialogmodells, der Stimme und gegebenenfalls Hindsight fallen an. Das Modell darf auch schweigen. Laufende Gespräche und Aufnahmen haben Vorrang."/></strong><small>Gelegentlich einen Kommentar oder eine Frage einwerfen.</small></div><input type="checkbox" role="switch" aria-label="Von selbst sprechen" bind:checked={config.speak}/></div>
    <div class="toggle-row"><div><strong><Footprints size={14}/>Herumlaufen <HelpTip text="Bewegt die Figur innerhalb ihrer Bühne. Vorhandene Laufanimationen werden bevorzugt; unterstützte menschliche Skelette erhalten einen automatischen Gehzyklus. Das Desktopfenster wird dabei nicht verschoben. Reduzierte Bewegung des Betriebssystems wird berücksichtigt."/></strong><small>Kurze Wege mit Ruhepausen, automatisch am Modell ausgerichtet.</small></div><input type="checkbox" role="switch" aria-label="Im Autopilot herumlaufen" bind:checked={config.wander}/></div>
    <div class="toggle-row"><div><strong>Kleine Gesten <HelpTip text="Gelegentliche stumme Bewegungen zwischen den Laufwegen. Eigene Animationen und unterstützte App-Gesten werden automatisch gewählt. Manuelle Emotes und die Sprachausgabe haben Vorrang."/></strong><small>Auch zwischen Gesprächen lebendig bleiben.</small></div><input type="checkbox" role="switch" aria-label="Gesten im Autopilot" bind:checked={config.gestures}/></div>
    <p class="field-note">Am besten mit „Ganze Figur“ unter Charakter & Stimme. Die Figur bleibt innerhalb ihrer Bühne.</p>
  </section>
  <section class="settings-panel"><div class="section-heading"><MessageCircle size={20}/><div><h2>Wie oft spricht {botName}?</h2><p>Abstände beginnen nach der letzten Gesprächsaktivität.</p></div></div>
    <div class="field-row"><label>Mindestens Sekunden <HelpTip text="So lange bleibt der Bot nach Gespräch, Aufnahme oder eigenem Beitrag mindestens ruhig. Der nächste Zeitpunkt wird zufällig zwischen Minimum und Maximum gewählt."/><input type="number" min="15" max="3600" step="1" bind:value={config.minSeconds}/></label><label>Höchstens Sekunden<input type="number" min="15" max="3600" step="1" bind:value={config.maxSeconds}/></label></div>
    <label>Höchstens KI-Aufrufe pro Stunde <HelpTip text="Ein rollendes Stundenbudget pro Bot. Auch fehlgeschlagene oder stille Versuche zählen. Eine neue Sitzung setzt das Budget während derselben App-Laufzeit nicht zurück. Bewegungen verbrauchen keine KI-Aufrufe."/><input type="number" min="1" max="120" step="1" bind:value={config.maxTurnsPerHour}/></label>
    <label>Zusätzliche Hinweise für spontane Beiträge<textarea rows="4" maxlength="4000" bind:value={config.instructions} placeholder="Zum Beispiel: Kommentiere trocken, stelle gelegentlich eine Frage und wiederhole dich nicht."></textarea></label>
    <p class="field-note">Bei mehreren Bots wird die Sprache nacheinander ausgegeben. Neue Chat-Antworten und deine Eingaben kommen zuerst. Autopilot-Beiträge werden nicht automatisch als Plattformnachricht gepostet.</p>
  </section>
</div>

<section class="settings-panel autopilot-screen"><div class="section-heading"><Eye size={20}/><div><h2>Bildschirm als Kontext</h2><p>Eine frische Beobachtung vor einem spontanen Beitrag.</p></div></div>
  <div class="toggle-row"><div><strong>Bildschirmkontext verwenden <HelpTip text="Ruft nur das hier festgelegte MCP-Beobachtungswerkzeug mit diesen festen Argumenten auf. Es erhält dadurch keine Erlaubnis zu klicken oder den Computer zu bedienen. Echte Werkzeugbilder gehen an das ausgewählte bildfähige Modell; bei Cloud-Modellen werden sie dorthin übertragen. Ohne passende Bildfreigabe oder Sehfähigkeit funktioniert die Bildanalyse nicht."/></strong><small>Benötigt einen für diesen Bot freigegebenen MCP-Server und ein bildfähiges Dialogmodell.</small></div><input type="checkbox" role="switch" aria-label="Bildschirmkontext im Autopilot" bind:checked={config.screenContext}/></div>
  {#if config.screenContext}
    <div class="field-row"><label>MCP-Server<select bind:value={config.screenServerId} onchange={() => { tools = []; loadedServer = ''; config.screenToolName = ''; }}><option value="">Freigegebenen Server wählen</option>{#each availableServers as server}<option value={server.id}>{server.name || server.id}</option>{/each}</select></label><div class="tool-load"><button class="button secondary" onclick={loadTools} disabled={!selectedServer || loading}>{loading ? 'Lädt …' : 'Werkzeuge laden'}</button></div></div>
    {#if !availableServers.length}<p class="field-note">Gib diesem Bot zuerst einen MCP-Server unter „Skills & MCP“ frei.</p>{/if}
    {#if allowedTools.length}<label>Beobachtungswerkzeug<select bind:value={config.screenToolName}><option value="">Nur lesendes Werkzeug wählen</option>{#each allowedTools as tool}<option value={tool.name}>{tool.name}</option>{/each}</select></label>{:else}<label>Beobachtungswerkzeug<input bind:value={config.screenToolName} placeholder="Exakter Werkzeugname, z. B. get_window_state"/></label>{/if}
    <label>Feste Werkzeugargumente (JSON)<textarea class="json-arguments" rows="4" maxlength="8000" spellcheck="false" bind:value={config.screenToolArguments} placeholder={'{"include_screenshot": true}'}></textarea></label>
    <p class="field-note">Wähle ein reines Beobachtungswerkzeug. Der Autopilot erhält keine weiteren MCP-Werkzeuge und kann diese Argumente nicht verändern. Ein Werkzeug ohne Bildausgabe liefert keine Sicht auf deinen Bildschirm.</p>
    {#if argsError}<p class="inline-error" role="status">{argsError}</p>{/if}{#if toolError}<p class="inline-error" role="status">{toolError}</p>{/if}
  {/if}
</section>

<style>
  .autopilot-intro { margin-bottom: 20px; border-color: #42636f; background: radial-gradient(ellipse at top right, #223946 0%, #151f2d 68%); }
  .autopilot-heading { display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; }
  .eyebrow { color: #9fc7d4; font-size: 9px; letter-spacing: .12em; }
  h2 { margin: 10px 0; font-size: 21px; font-weight: 600; }
  p { font-size: 12px; color: #a3b1c6; line-height: 1.8; max-width: 650px; }
  .autopilot-status { font-size: 11px; color: #a4b3c9; white-space: nowrap; border: 1px solid #4b5b72; border-radius: 6px; padding: 6px 10px; }
  .autopilot-status.active { color: #a5ddc7; border-color: #568b77; }
  .autopilot-actions { display: flex; flex-wrap: wrap; gap: 10px; margin-block: 20px 14px; }
  .autopilot-settings { margin-bottom: 20px; }
  .toggle-row strong { display: flex; gap: 7px; align-items: center; }
  .tool-load { align-self: flex-end; padding-bottom: 14px; }
  .json-arguments { font-family: ui-monospace, monospace; }
  .inline-error { color: #f1a8a8; }
  @media (max-width: 650px) { .autopilot-heading { flex-direction: column-reverse; gap: 8px; } }
</style>
