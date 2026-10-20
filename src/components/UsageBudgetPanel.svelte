<script lang="ts">
  import { onMount } from 'svelte';
  import { Gauge, RefreshCw } from '@lucide/svelte';
  import { call, native, prettyError } from '../lib/api';
  import { t, locale, formatNumber } from '../lib/i18n';
  import { draftChanged, draftLimits, getUsageBudget, limitFields, limitsDraft, setUsageLimits, usageDenials, usageLimitDefaults, type UsageBudgetSnapshot, type UsageLimitsDraft, type UsageTotals } from '../lib/usage-budget';
  import HelpTip from './HelpTip.svelte';

  interface Props { bots?: { id: string; name: string }[]; disabled?: boolean; onDirtyChange?: (dirty: boolean) => void; }
  let { bots = [], disabled = false, onDirtyChange }: Props = $props();
  const uid = $props.id();
  let snapshot = $state<UsageBudgetSnapshot>();
  let draft = $state<UsageLimitsDraft>();
  let fetching = $state(false);
  let saving = $state(false);
  let error = $state('');
  let snapshotUnconfirmed = $state(false);
  let saved = $state(false);
  let repairOpen = $state(false);
  let repairEdited = $state(false);
  let formElement = $state<HTMLFormElement>();
  let alive = false;
  let operation = 0;
  const dirty = $derived(!!draft && (snapshot ? draftChanged(draft, snapshot.limits) : repairEdited));
  const canSave = $derived(!!draft && (snapshot ? dirty : repairOpen));
  $effect(() => { onDirtyChange?.(dirty); });
  const number = (value: number) => formatNumber(value, $locale);
  const bytes = (value: number) => value >= 1_048_576
    ? `${formatNumber(value / 1_048_576, $locale, { maximumFractionDigits: 2 })} MiB`
    : `${number(value)} B`;
  const hindsight = (total: UsageTotals) => total.hindsightRecallRequests + total.hindsightRetainRequests;
  const name = (id: string) => bots.find(bot => bot.id === id)?.name || id;

  async function refresh() {
    if (!native || fetching || saving || disabled) return;
    const ticket = ++operation;
    fetching = true;
    try {
      const next = await getUsageBudget(call);
      if (!alive || ticket !== operation) return;
      const keepDraft = !!draft && (snapshot ? draftChanged(draft, snapshot.limits) : repairEdited);
      snapshot = next;
      snapshotUnconfirmed = false;
      if (!keepDraft) draft = limitsDraft(next.limits);
      repairEdited = false;
      repairOpen = false;
      error = '';
    } catch (cause) { if (alive && ticket === operation) { error = prettyError(cause); snapshotUnconfirmed = !!snapshot; if (!snapshot && !draft) draft = limitsDraft(usageLimitDefaults); } }
    finally { if (alive && ticket === operation) fetching = false; }
  }
  async function save() {
    if (!native || !draft || !canSave || saving || disabled) return;
    saved = false;
    let limits;
    try { limits = draftLimits(draft); } catch (cause) { error = prettyError(cause); return; }
    const ticket = ++operation;
    saving = true;
    fetching = false;
    try {
      const next = await setUsageLimits(call, limits);
      if (!alive || ticket !== operation) return;
      snapshot = next;
      snapshotUnconfirmed = false;
      draft = limitsDraft(next.limits);
      error = '';
      saved = true;
      repairEdited = false;
      repairOpen = false;
    } catch (cause) { if (alive && ticket === operation) { error = prettyError(cause); snapshotUnconfirmed = !!snapshot; } }
    finally { if (alive && ticket === operation) saving = false; }
  }
  function saveShortcut(event: KeyboardEvent) {
    if (event.target instanceof Node && formElement?.contains(event.target) &&
      (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
      event.preventDefault();
      event.stopPropagation();
      void save();
    }
  }
  onMount(() => {
    alive = true;
    window.addEventListener('keydown', saveShortcut, { capture: true });
    void refresh();
    const timer = setInterval(() => { if (snapshot && !error && !document.hidden) void refresh(); }, 5000);
    return () => { alive = false; ++operation; clearInterval(timer); window.removeEventListener('keydown', saveShortcut, { capture: true }); onDirtyChange?.(false); };
  });
</script>

{#snippet limitsForm()}
  {#if draft}
    <form bind:this={formElement} onsubmit={event => { event.preventDefault(); void save(); }} oninput={() => { saved = false; if (!snapshot) repairEdited = true; }}>
      <fieldset disabled={saving || disabled}>
        <legend>{$t('Gemeinsame Tagesgrenzen ändern')}</legend>
        <p class="field-note">{$t('0 sperrt den jeweiligen Bereich. Gespeicherte Änderungen gelten sofort für neue autonome Anfragen und setzen Zähler nicht zurück.')}</p>
        <div class="accounting-mode"><label for={`${uid}-accounting`}>{$t('Anfragezählung')}</label><select id={`${uid}-accounting`} bind:value={draft.requestAccounting}><option value="dispatch">{$t('OrbitPal-Aufrufe und CLI-Aufträge')}</option><option value="direct">{$t('Nur direkte HTTP-Dialogaufrufe')}</option></select><p class="field-note">{$t('OrbitPal-Aufrufe zählt eigene Dialogaufrufe und gestartete CLI-Aufträge. Interne CLI-Modellanfragen bleiben unbekannt. Im Direktmodus sind autonome CLI-Aufträge gesperrt; gezählt werden nur eigene HTTP-Dialogaufrufe am konfigurierten Endpunkt, keine internen Provideraufrufe oder Rechnungsbeträge.')}</p><p class="field-note">{$t('Ein Wechsel setzt nichts zurück. Frühere CLI-Aufträge sperren den Direktmodus für den restlichen UTC-Tag; frühere Dialoge ohne sichere Tokenobergrenze sperren ein später aktiviertes hartes Tokenlimit bis zum nächsten UTC-Tag.')}</p></div>
        <div class="budget-fields">{#each limitFields as field}<div class="budget-field"><label for={`${uid}-${field.key}`}>{$t(field.label)}</label><input id={`${uid}-${field.key}`} type="text" inputmode="numeric" pattern="[0-9]+" maxlength="12" required bind:value={draft.values[field.key]} aria-describedby={`${uid}-${field.key}-bound`}/><small id={`${uid}-${field.key}-bound`}>{$t('Ganze Zahl von 0 bis {max}', { max: number(field.max) })}</small></div>{/each}</div>
        <div class="toggle-row"><div><strong>{$t('Zusätzlich ein hartes Tokenlimit')}</strong><small>{$t('Optional; ohne diese Option begrenzen weiterhin die einzelnen Tagesgrenzen.')}</small></div><input type="checkbox" role="switch" aria-label={$t('Zusätzlich ein hartes Tokenlimit')} bind:checked={draft.tokenEnabled}/></div>
        {#if draft.tokenEnabled}<p class="field-note">{$t('Pro Anfrage wird zunächst ein ganzes Kontextfenster plus Ausgabelimit reserviert. Bestätigter geringerer Verbrauch gibt den ungenutzten Teil frei.')}</p><div class="token-field"><label for={`${uid}-tokens`}>{$t('Tokens pro Tag')}</label><input id={`${uid}-tokens`} type="text" inputmode="numeric" pattern="[0-9]+" maxlength="12" required bind:value={draft.tokenLimit} aria-describedby={`${uid}-tokens-note`}/><small id={`${uid}-tokens-note`}>{$t('Ganze Zahl von 1 bis {max}', { max: number(1_000_000_000) })}</small></div><p class="budget-warning">{$t('Ein hartes Tokenlimit ist nur für ausdrücklich unterstützte direkte OpenAI-Modelle mit begrenzbarer Ausgabe verfügbar. Andere Modelle, CLI-Anbieter und kompatible Endpunkte werden dann vor autonomen Dialoganfragen abgelehnt. Es wird kein Modell automatisch gewechselt.')}</p>{/if}
        <div class="budget-save"><button class="button secondary" type="submit" disabled={!canSave || saving || disabled}>{saving ? $t('Wird gespeichert …') : $t('Tagesgrenzen speichern')}</button><span role="status">{saved ? $t('Tagesgrenzen gespeichert.') : dirty ? $t('Nicht gespeicherte Budgetänderungen') : ''}</span></div>
      </fieldset>
    </form>
  {/if}
{/snippet}

<section class="settings-panel usage-budget" aria-labelledby={`${uid}-title`}>
  <div class="budget-heading">
    <div class="section-heading"><Gauge size={20}/><div><h2 id={`${uid}-title`}>{$t('Tagesbudget für alle Bots')}</h2><p>{$t('Gemeinsame Grenzen für selbstständige Beiträge und automatische Chatantworten.')}</p></div></div>
    <button class="button secondary small" onclick={refresh} disabled={!native || fetching || saving || disabled} aria-label={$t('Verbrauch aktualisieren')}><RefreshCw size={14}/>{$t('Aktualisieren')}</button>
  </div>
  <p class="budget-scope">{$t('Gilt für diese Installation und alle Bots zusammen. Neuer Tag um 00:00 UTC; ein App-Neustart setzt den Verbrauch nicht zurück.')}</p>
  <p class="field-note">{$t('Manuelle Nachrichten, Mikrofonanfragen, bewusst ausgewählte Chatantworten und Stimmvorschauen zählen hier nicht. Lokale Bewegung und Stop bleiben möglich.')}</p>

  {#if !native}
    <p class="budget-message" role="status">{$t('Verbrauchsgrenzen und echte Zähler sind in der Desktop-App verfügbar.')}</p>
  {:else if !snapshot}
    <p class="budget-message" role="status">{fetching ? $t('Verbrauch wird geladen …') : $t('Verbrauch konnte nicht geladen werden. Erneut aktualisieren.')}</p>
  {/if}
  {#if error}<p class="budget-error" role="alert">{error}</p>{/if}
  {#if snapshot && draft}
    <div class="budget-day"><strong>{$t(snapshotUnconfirmed ? 'Letzter bekannter Verbrauch am {day} (UTC)' : 'Verbrauch am {day} (UTC)', { day: snapshot.day })}</strong><span>{$t('{count} Vorgänge noch offen', { count: number(snapshot.total.pendingRequests) })}</span></div>
    {#if snapshotUnconfirmed}<p class="budget-warning" role="status">{$t('Die angezeigten Zähler und Grenzen sind nicht aktuell bestätigt. Sie zeigen den letzten erfolgreichen Stand und sagen nichts über das aktuell verfügbare Restbudget aus. Erneut aktualisieren oder ausdrücklich speichern, um den Stand zu bestätigen.')}</p>{/if}
    {#if snapshot.clockAdjusted}<p class="budget-warning" role="status">{$t('Die Systemzeit wurde zurückgestellt. OrbitPal behält den bisherigen Budgettag bei und vergibt kein zusätzliches Budget.')}</p>{/if}
    {#if snapshot.lastDenial}<p class="budget-warning" role="status"><strong>{$t('Letzte Budgetablehnung:')}</strong> {$t(usageDenials[snapshot.lastDenial])}</p>{/if}
    <p class="field-note">{$t('Gespeicherte Anfragezählung: {mode}', { mode: snapshot.limits.requestAccounting === 'direct' ? $t('Nur direkte HTTP-Dialogaufrufe') : $t('OrbitPal-Aufrufe und CLI-Aufträge') })}</p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (The horizontal ledger needs keyboard focus for scrolling on narrow screens.) -->
    <div class="ledger-scroll" tabindex="0" role="region" aria-label={$t('Gemeinsamer Tagesverbrauch')}>
      <table><thead><tr><th scope="col">{$t('Bereich')}</th><th scope="col">{$t('Gezählt oder reserviert')}</th><th scope="col">{$t('Gespeichertes Tageslimit')}</th></tr></thead><tbody>
        <tr><th scope="row">{$t('Dialog-Aufrufe')}</th><td>{number(snapshot.total.dialogRequests)}</td><td>{number(snapshot.limits.dailyDialogRequests)}</td></tr>
        <tr><th scope="row">{$t('Sprachzeichen')}<small>{$t('{count} Sprachanfragen', { count: number(snapshot.total.ttsRequests) })}</small></th><td>{number(snapshot.total.ttsCharacters)}</td><td>{number(snapshot.limits.dailyTtsCharacters)}</td></tr>
        <tr><th scope="row">Hindsight<small>{$t('{recall} Abrufe, {retain} Speicheranfragen', { recall: number(snapshot.total.hindsightRecallRequests), retain: number(snapshot.total.hindsightRetainRequests) })}</small></th><td>{number(hindsight(snapshot.total))}</td><td>{number(snapshot.limits.dailyHindsightRequests)}</td></tr>
        <tr><th scope="row">{$t('Angehängte Bilder')}</th><td>{number(snapshot.total.imageCount)}</td><td>{number(snapshot.limits.dailyImageCount)}</td></tr>
        <tr><th scope="row">{$t('Bilddaten')}</th><td>{bytes(snapshot.total.imageBytes)}</td><td>{bytes(snapshot.limits.dailyImageBytes)}</td></tr>
      </tbody></table>
    </div>
    <p class="field-note">{$t('Jede Dialog-Folgerunde und jede erneute Bildübertragung zählt. Sprachanfragen zählen bei neuer Synthese; Hindsight umfasst Abruf und Speicherung. Reservierter oder möglicherweise gesendeter Verbrauch bleibt berücksichtigt.')}</p>
    <p class="field-note">{$t('Bildanzahl und Bildbytes umfassen alle angehängten Bildschirm-, MCP- und Werkzeugbilder pro Dialogrunde. Bei CLI-Aufträgen zählen die an den Auftrag übergebenen Bilder; interne erneute Übertragungen der CLI bleiben unbekannt.')}</p>
    <p class="field-note">{$t('Bei CLI-Anbietern zählt ein gestarteter CLI-Auftrag als Dialoganfrage. Interne Modellanfragen und deren Abrechnung bleiben unbekannt.')}</p>
    <div class="token-accounting">
      <h3>{$t('Tokenverbrauch einordnen')} <HelpTip text={$t('Angerechnete Tokens enthalten bereits offene Reservierungen und konservative Obergrenzen bei fehlenden Angaben. Bestätigte Tokens sind nur die vom Anbieter gemeldeten Werte. Diese Zahlen werden nicht addiert und sind keine Geldgarantie.')}/></h3>
      <dl><div><dt>{$t('Vom Anbieter bestätigt')}</dt><dd>{number(snapshot.total.observedTokens)}</dd></div><div><dt>{$t('Konservativ angerechnet')}</dt><dd>{number(snapshot.total.chargedTokens)}</dd></div><div><dt>{$t('Davon noch reserviert')}</dt><dd>{number(snapshot.total.reservedTokens)}</dd></div><div><dt>{$t('Anfragen ohne Tokenangabe')}</dt><dd>{number(snapshot.total.unknownTokenRequests)}</dd></div></dl>
      <p class="field-note">{snapshot.limits.hardTokenLimit === null ? $t('Kein zusätzliches Tokenlimit gespeichert.') : $t('Gespeichertes Tokenlimit: {count}', { count: number(snapshot.limits.hardTokenLimit) })}</p>
      <p class="field-note">{$t('Fehlende Angaben sind kein Nullverbrauch. CLI- und manche Anbieteraufrufe liefern keine verlässlichen Tokenzahlen. Ein Anfragelimit ist kein Euro- oder Rechnungslimit.')}</p>
    </div>
    {@render limitsForm()}
    <details class="settings-details per-bot"><summary>{$t('Verbrauch je Bot')}</summary>{#if !snapshot.bots.length}<p class="field-note">{$t('Heute wurde noch kein autonomer Verbrauch erfasst.')}</p>{:else}{#each snapshot.bots as bot (bot.botId)}<article><h3>{name(bot.botId)}</h3><dl><div><dt>{$t('Dialog-Aufrufe')}</dt><dd>{number(bot.total.dialogRequests)}</dd></div><div><dt>{$t('Sprachzeichen')}</dt><dd>{number(bot.total.ttsCharacters)}</dd></div><div><dt>Hindsight</dt><dd>{number(hindsight(bot.total))}</dd></div><div><dt>{$t('Angehängte Bilder')}</dt><dd>{number(bot.total.imageCount)}</dd></div><div><dt>{$t('Bilddaten')}</dt><dd>{bytes(bot.total.imageBytes)}</dd></div><div><dt>{$t('Konservativ angerechnet')}</dt><dd>{number(bot.total.chargedTokens)}</dd></div><div><dt>{$t('Vom Anbieter bestätigt')}</dt><dd>{number(bot.total.observedTokens)}</dd></div><div><dt>{$t('Anfragen ohne Tokenangabe')}</dt><dd>{number(bot.total.unknownTokenRequests)}</dd></div></dl></article>{/each}{/if}</details>
  {:else if native && draft}
    <p class="budget-warning" role="status"><strong>{$t('Verbrauch unbekannt.')}</strong> {$t('Ohne gültige Verbrauchsdaten zeigt OrbitPal keine Nullwerte und lässt keine neuen autonomen Anfragen zu.')}</p>
    <details class="settings-details repair-settings" bind:open={repairOpen}><summary>{$t('Tagesgrenzen reparieren oder neu festlegen')}</summary><p class="field-note">{$t('Die folgenden Standardwerte sind ein neuer Einstellungsentwurf, keine Bestätigung der gespeicherten Grenzen. Speichern ändert nur die Grenzen und löscht keinen bereits erfassten Verbrauch. Autonome Anfragen bleiben gesperrt, bis die Verbrauchskontrolle wieder gültig ist. Es wird nichts automatisch gespeichert oder erneut versucht.')}</p>{@render limitsForm()}</details>
  {/if}
</section>

<style>
  .usage-budget { margin-top: 22px; }
  .budget-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; }
  .budget-heading .section-heading { margin-bottom: 10px; }
  .budget-heading button { flex-shrink: 0; }
  .budget-scope { max-width: 78ch; color: var(--text); margin-bottom: 7px; }
  .budget-day { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; margin: 22px 0 12px; }
  .budget-day span, .budget-save span { color: var(--muted); font-size: 12px; }
  .ledger-scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; font-size: 13px; }
  th, td { padding: 12px 14px; border-bottom: 1px solid var(--line); text-align: right; }
  th:first-child { text-align: left; }
  thead th { font-weight: 500; color: var(--muted); font-size: 12px; }
  tbody th { font-weight: 500; }
  th small { display: block; margin-top: 4px; color: var(--muted); font-weight: 400; font-size: 11px; }
  .token-accounting { margin: 22px 0; padding: 16px 0; border-block: 1px solid var(--line); }
  h3 { display: flex; align-items: center; gap: 5px; }
  dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 24px; margin: 14px 0; }
  dl div { display: flex; justify-content: space-between; gap: 16px; align-items: baseline; }
  dt { color: var(--muted); font-size: 12px; }
  dd { margin: 0; font-variant-numeric: tabular-nums; }
  fieldset { padding: 0; border: 0; min-width: 0; margin: 0; }
  legend { font-family: 'Space Grotesk', sans-serif; font-size: 15px; margin-bottom: 8px; }
  .accounting-mode { display: grid; gap: 8px; margin: 18px 0; }
  .repair-settings { margin-top: 18px; }
  .repair-settings form { margin-top: 20px; }
  .budget-fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px 24px; margin: 18px 0; }
  .budget-field, .token-field { display: grid; gap: 7px; }
  .budget-field small, .token-field small { color: var(--muted); font-size: 11px; }
  .token-field { max-width: 320px; margin: 16px 0; }
  .budget-save { display: flex; align-items: center; flex-wrap: wrap; gap: 14px; margin-top: 18px; }
  .budget-message, .budget-warning, .budget-error { padding: 12px 14px; margin: 14px 0; background: var(--input-bg); border-left: 3px solid var(--line); font-size: 12px; }
  .budget-warning { color: var(--warning); border-color: var(--warning); }
  .budget-error { color: var(--danger); border-color: var(--danger); }
  .per-bot { margin-top: 24px; }
  .per-bot article { padding: 16px 0; border-bottom: 1px solid var(--line); }
  .per-bot h3 { overflow-wrap: anywhere; }
  @media(max-width: 680px) { .budget-heading { flex-direction: column; gap: 4px; } .budget-fields, dl { grid-template-columns: 1fr; } th, td { padding: 10px 8px; } table { min-width: 440px; } }
</style>
