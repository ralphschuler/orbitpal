<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Map, Play, RefreshCw, Save, Square, Users } from '@lucide/svelte';
  import { call, native, onEvent, prettyError } from '../lib/api';
  import { t, locale, formatNumber } from '../lib/i18n';
  import {lifeSceneParticipants,LIFE_SCENE_OVER_LIMIT} from '../lib/life-scene-participants';
  import { draftWorld, invalidWorld, newWorldDraft, parseWorldSettings, parseWorldState, placeWorld, saveWorld, worldDraft, worldDraftChanged, worldReasons, type WorldDraft, type WorldRecord, type WorldSettings, type WorldState } from '../lib/world-state';
  import { containsEditorPoint, writeEditorPoint, type ActorPositions } from '../lib/world-editor';
  import type { OutputDisplay } from '../lib/output-presentation';
  import WorldSceneEditor from './WorldSceneEditor.svelte';
  import WorldViewport from './WorldViewport.svelte';
  import type { CanonicalPoint, RegisterWorldActor } from '../lib/world-output';
  import type { LifeActionsSnapshot } from '../lib/life-actions';
  interface Props {
    bots: { id: string; name: string; profile?: any }[]; displays?: OutputDisplay[]; visible: boolean; disabled?: boolean;
    onDirtyChange: (dirty: boolean) => void; onBusyChange: (busy: boolean) => void; confirmDiscard: (action: () => void) => void;
    selectedBotId?: string; outputUrl?: string; executionBlocked?: boolean;
    registerWorld?: (request: Omit<RegisterWorldActor, 'studioProof'>) => Promise<unknown>;
    onMove?: (target: CanonicalPoint) => Promise<LifeActionsSnapshot | undefined>;
    onStart?: () => Promise<void>; onStop?: () => Promise<void>; active?: boolean; starting?: boolean;
  }
  const unavailable = () => Promise.reject(new Error('Die Verbindung zur sichtbaren Bühne ist nicht bestätigt.'));
  let { bots, displays = [], visible, disabled = false, onDirtyChange, onBusyChange, confirmDiscard,
    selectedBotId = '', outputUrl = '', executionBlocked = true, registerWorld = unavailable, onMove = unavailable,
    onStart, onStop, active = false, starting = false }: Props = $props();
  const uid = $props.id();
  let settings = $state<WorldSettings>();
  let confirmedState = $state<WorldState>();
  let baseline = $state<WorldRecord | null>(null);
  let draft = $state<WorldDraft>();
  let positions = $state<ActorPositions>({});
  let writing = $state(false), pending = $state(false), gestureActive = $state(false);
  let localStarting = $state(false), stopPending = $state(false);
  let error = $state(''), message = $state(''), unconfirmed = $state(false);
  let form = $state<HTMLFormElement>();
  let previewOpen=$state(false);
  let alive = false, operation = 0, startGeneration = 0;
  const participants = $derived(lifeSceneParticipants(draft?.members??[]));
  const configDirty = $derived(!!draft && worldDraftChanged(draft, baseline));
  const dirty = $derived(configDirty || Object.keys(positions).length > 0 || gestureActive);
  const snapshot = $derived(confirmedState?.snapshot ?? undefined);
  const parsedDraft = $derived.by(() => { try { return draft ? draftWorld(draft) : undefined; } catch { return undefined; } });
  const number = (value: number) => formatNumber(value, $locale);
  $effect(() => onDirtyChange(dirty));
  $effect(() => onBusyChange(writing || stopPending));
  $effect(() => { if (visible && alive && !settings && !pending && !error) void refresh(); });
  function acceptPublic(value: unknown): boolean {
    const next = parseWorldState(value);
    if (!settings || next.instanceId !== settings.instanceId) { unconfirmed = true; return false; }
    if (next.snapshot && (!settings.record || next.snapshot.stageId !== settings.record.config.stageId || next.snapshot.configRevision !== settings.record.config.revision)) { unconfirmed = true; return false; }
    if (confirmedState?.instanceId === next.instanceId && confirmedState.snapshot && next.snapshot && next.snapshot.sequence < confirmedState.snapshot.sequence) return true;
    confirmedState = next; unconfirmed = next.availability === 'unavailable'; return !unconfirmed;
  }
  async function refresh() {
    if (!native || pending || disabled || gestureActive) return;
    const ticket = ++operation; pending = true;
    try {
      const [rawSettings, rawState] = await Promise.all([call('get_world_settings'), call('get_world_state')]);
      if (!alive || ticket !== operation) return;
      const next = parseWorldSettings(rawSettings); parseWorldState(rawState);
      // Keep the original CAS baseline for every unsaved edit, including positions.
      const keep = dirty; settings = next;
      if (!keep) { baseline = next.record; draft = next.record ? worldDraft(next.record) : undefined; }
      acceptPublic(rawState); error = '';
    } catch (cause) { if (alive && ticket === operation) { error = prettyError(cause); unconfirmed = true; } }
    finally { if (alive && ticket === operation) pending = false; }
  }
  function prepare() {
    if (settings?.availability !== 'not_initialized' || draft || pending || disabled) return;
    draft = newWorldDraft(`stage_${crypto.randomUUID()}`); baseline = null; message = '';
  }
  function discard() {
    if (!dirty || pending || gestureActive) return;
    confirmDiscard(() => {
      draft = settings?.record ? worldDraft(settings.record) : undefined; baseline = settings?.record ?? null;
      positions = {}; message = ''; error = '';
    });
  }
  function edit(next: WorldDraft, nextPositions: ActorPositions) {
    draft = next;
    positions = Object.fromEntries(Object.entries(nextPositions).filter(([id, point]) => {
      if (!next.members.includes(id)) return false;
      const current = snapshot?.actors.find(actor => actor.actorId === id)?.position;
      return !current || current.x !== point.x || current.y !== point.y;
    }));
    message = '';
  }
  function currentReadback(value: unknown): WorldState {
    const next = parseWorldState(value);
    if (!settings?.record || next.availability !== 'ready' || !next.snapshot
      || next.instanceId !== settings.instanceId || next.snapshot.stageId !== settings.record.config.stageId
      || next.snapshot.configRevision !== settings.record.config.revision) {
      unconfirmed = true; throw new Error(next.reason ? worldReasons[next.reason] : 'Bühnenstatus nicht bestätigt. Bitte aktualisieren.');
    }
    acceptPublic(next); return next;
  }
  function stillCurrent(ticket: number) {
    if (!alive || ticket !== operation) throw new Error('Bühnenstatus nicht bestätigt. Bitte aktualisieren.');
  }
  async function persistAll(ticket: number) {
    if (!draft || !settings || settings.availability === 'unavailable' && !settings.record) throw new Error('Bühnenstatus nicht bestätigt. Bitte aktualisieren.');
    const payload = structuredClone($state.snapshot(draft)), parsed = draftWorld(payload);
    const placementDrafts = structuredClone($state.snapshot(positions));
    for (const [id, point] of Object.entries(placementDrafts)) {
      if (!payload.members.includes(id) || !containsEditorPoint(parsed.config.safeRegion, point)) throw invalidWorld();
    }
    if (configDirty) {
      const next = await saveWorld(call, payload, baseline?.config.revision ?? null); stillCurrent(ticket);
      settings = next; baseline = next.record; draft = next.record ? worldDraft(next.record) : undefined;
      // Position drafts survive the configuration transaction and readback.
      confirmedState = undefined; unconfirmed = true;
    } else if (!baseline || !settings.record || baseline.config.revision !== settings.record.config.revision || baseline.config.stageId !== settings.record.config.stageId) {
      throw new Error(worldReasons.stale_revision);
    }
    const readback = await call('get_world_state'); stillCurrent(ticket);
    let current = currentReadback(readback);
    for (const [id, point] of Object.entries(placementDrafts)) {
      if (!current.snapshot?.actors.some(actor => actor.actorId === id)) throw invalidWorld();
      const next = await placeWorld(call, current.snapshot.configRevision, id, writeEditorPoint(point)); stillCurrent(ticket);
      current = currentReadback(next);
      // Clear only a placement whose own native acknowledgement succeeded.
      positions = Object.fromEntries(Object.entries(positions).filter(([actor]) => actor !== id));
    }
    unconfirmed = false; error = ''; message = 'Anordnung gespeichert.';
    await tick(); stillCurrent(ticket);
  }
  async function runSave(start: boolean): Promise<void> {
    if (pending || disabled || gestureActive || !native) throw new Error('Bühnenstatus nicht bestätigt. Bitte aktualisieren.');
    const ticket = ++operation, startTicket = start ? ++startGeneration : startGeneration;
    pending = true; writing = true; localStarting = start; error = ''; message = '';
    try {
      if(start&&participants.overLimit)throw new Error($t(LIFE_SCENE_OVER_LIMIT,{count:participants.count,limit:participants.limit}));
      await persistAll(ticket);
      if (start) {
        if (startTicket !== startGeneration) return;
        if (!onStart || !visible || disabled || dirty || unconfirmed || !settings?.record?.config.members.length) throw new Error('Bühnenstatus nicht bestätigt. Bitte aktualisieren.');
        await onStart(); stillCurrent(ticket);
        if (startTicket === startGeneration) message = 'Life gestartet.';
      }
    } catch (cause) {
      if(start && startTicket!==startGeneration)return;
      if (alive && ticket === operation) { error = prettyError(cause); unconfirmed = true; }
      throw cause;
    } finally { if (alive && ticket === operation) { pending = false; writing = false; localStarting = false; } }
  }
  /** Resolves only after configuration AND every pending actor placement are confirmed. */
  export function cancelPendingStart(){++startGeneration;}
  export async function saveDraft(): Promise<void> { await runSave(false); }
  function save() { void saveDraft().catch(() => {}); }
  function start() { void runSave(true).catch(() => {}); }
  async function stop() {
    if (!onStop || stopPending) return;
    ++startGeneration; stopPending = true;
    try { await onStop(); if (alive) message = 'Life-Stopp angefordert.'; }
    catch (cause) { if (alive) error = prettyError(cause); }
    finally { if (alive) stopPending = false; }
  }
  function shortcut(event: KeyboardEvent) {
    if (!visible || event.isComposing || !form?.contains(event.target as Node)) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') { event.preventDefault(); event.stopPropagation(); if (!pending && !gestureActive) save(); }
  }
  onMount(() => {
    alive = true; if (visible) void refresh();
    const stopEvents = onEvent('cohost://world-state', value => { if (!alive) return; try { acceptPublic(value); } catch (cause) { error = prettyError(cause); unconfirmed = true; } });
    window.addEventListener('keydown', shortcut, { capture: true });
    const timer = setInterval(() => { if (visible && settings && !error) void refresh(); }, 5000);
    return () => { alive = false; ++operation; ++startGeneration; clearInterval(timer); void stopEvents.then(unlisten => unlisten()); window.removeEventListener('keydown', shortcut, { capture: true }); onDirtyChange(false); onBusyChange(false); };
  });
</script>

<section class="settings-panel world-panel" aria-labelledby={`${uid}-title`}>
  <div class="world-heading"><div class="section-heading"><Map size={20}/><div><h2 id={`${uid}-title`}>{$t('Life einrichten')}</h2><p>{$t('Wähle deine Figuren und richte ihren Platz auf dem Desktop ein.')}</p></div></div><button class="button secondary small" onclick={refresh} disabled={!native || pending || disabled || gestureActive}><RefreshCw size={14}/>{$t('Bühne aktualisieren')}</button></div>
  {#if !native}<p role="status">{$t('Die gemeinsame Bühne wird in der Desktop-App verwaltet.')}</p>{:else if !settings}<p role="status">{$t(pending ? 'Bühnenstatus wird geladen …' : 'Bühnenstatus nicht bestätigt. Bitte aktualisieren.')}</p>{/if}
  {#if error}<p class="world-error" role="alert">{error}</p>{/if}
  {#if settings?.reason}<p class="world-warning" role="status">{$t(worldReasons[settings.reason])}</p>{/if}
  {#if confirmedState?.reason && confirmedState.reason !== settings?.reason}<p class="world-warning" role="status">{$t(worldReasons[confirmedState.reason])}</p>{/if}
  {#if unconfirmed}<p class="world-warning" role="status">{$t('Der angezeigte Bühnenstand ist nicht aktuell bestätigt. Entwürfe bleiben erhalten; Positionen und Belegungen gelten nicht als aktuelle Zusage.')}</p>{/if}
  {#if settings?.availability === 'not_initialized' && !draft}<div class="empty-world"><Users size={24}/><p>{$t('Wähle Figuren aus und lege ihre Anordnung fest.')}</p><button class="button primary" onclick={prepare} disabled={pending || disabled}>{$t('Desktop einrichten')}</button></div>{/if}
  {#if settings?.availability === 'unavailable' && !settings.record}<p class="field-note">{$t('Die gespeicherten Daten werden unverändert aufbewahrt. Es wird keine Ersatzbühne angelegt und nichts automatisch zurückgesetzt.')}</p>{/if}
  {#if draft}
    <form bind:this={form} onsubmit={event => { event.preventDefault(); save(); }}>
      <fieldset disabled={pending || disabled}>
        <legend class="sr-only">{$t('Bühnenentwurf bearbeiten')}</legend>
        <div class="editor-context"><p>{$t('Deine Anordnung gilt beim nächsten Start. Speichern allein startet Life nicht.')}</p><label for={`${uid}-monitor`}>{$t('Zielmonitor')}<select id={`${uid}-monitor`} bind:value={draft.monitorId} onchange={() => message = ''}><option value="">{$t('Sicherer Ersatzmonitor · automatisch geprüft')}</option>{#if draft.monitorId && !displays.some(display => display.id === draft?.monitorId)}<option value={draft.monitorId}>{$t('Gespeicherter Monitor · derzeit nicht verfügbar')}</option>{/if}{#each displays as display (display.id)}<option value={display.id}>{display.name}</option>{/each}</select></label></div>
        <WorldSceneEditor {draft} {positions} {snapshot} {bots} active={visible} disabled={pending || disabled} onChange={edit} onGestureChange={value => gestureActive = value}/>
      </fieldset>
      {#if participants.overLimit}<p id={`${uid}-participant-limit`} class="world-warning" role="status">{$t(LIFE_SCENE_OVER_LIMIT,{count:participants.count,limit:participants.limit})}</p>{/if}
      <div class="world-actions">
        <div class="save-actions"><button class="button secondary" type="submit" disabled={!dirty || !parsedDraft || pending || disabled || gestureActive}><Save size={15}/>{$t('Anordnung speichern')}</button>{#if onStart}<button class="button primary" type="button" onclick={start} aria-describedby={participants.overLimit?`${uid}-participant-limit`:undefined} disabled={!parsedDraft || !participants.canStart || pending || disabled || starting || stopPending || gestureActive}><Play size={15}/>{$t(localStarting || starting ? 'Life wird gestartet …' : 'Speichern und Life starten')}</button>{/if}</div>
        <button class="text-link" type="button" onclick={discard} disabled={!dirty || pending || gestureActive}>{$t('Bühnenentwurf verwerfen')}</button>
        <span class="save-status" role="status">{dirty ? $t('Nicht gespeicherter Bühnenentwurf') : $t('Anordnung gespeichert.')}</span>
      </div>
    </form>
  {/if}
  {#if onStop && (active || starting || localStarting)}<div class="world-stop"><button class="button secondary stop-button" type="button" onclick={stop} disabled={stopPending}><Square size={14}/>{$t(stopPending ? 'Stopp wird angefordert …' : 'Life stoppen')}</button><span>{$t('Stoppen bleibt auch während des Starts möglich.')}</span></div>{/if}
  {#if message}<p class="world-message" role="status">{$t(message)}</p>{/if}
  {#if snapshot && !unconfirmed}<details class="world-confirmed"><summary>{$t('Bestätigter Bühnenstand')}</summary><p>{$t('Konfiguration {revision} · Stand {sequence}', { revision: number(snapshot.configRevision), sequence: number(snapshot.sequence) })}</p><p>{$t('{count} Plätze sind derzeit nativ belegt.', { count: number(snapshot.occupied.length) })}</p></details>{/if}
  {#if confirmedState?.availability === 'ready' && !unconfirmed}<details class="world-confirmed" bind:open={previewOpen}><summary>{$t('3D-Vorschau und Laufziel')}</summary><WorldViewport world={confirmedState} {bots} {selectedBotId} {outputUrl} visible={visible&&previewOpen} blocked={executionBlocked || disabled || dirty || writing} register={registerWorld} {onMove}/></details>{/if}
</section>

<style>
  .world-panel{margin:20px 0}.world-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px;flex-wrap:wrap}.world-heading>button{flex-shrink:0}.section-heading p,.field-note{font-size:12px;line-height:1.7;color:var(--muted)}fieldset{border:0;padding:0;margin:20px 0 0;min-width:0}.editor-context{display:flex;align-items:end;justify-content:space-between;gap:20px;margin-bottom:16px}.editor-context>p{font-size:12px;line-height:1.7;color:var(--muted);max-width:52ch;margin:0}.editor-context>label{font-size:11px;margin:0;max-width:280px;min-width:190px}.editor-context select{margin-top:6px;font-size:12px}.world-actions{display:flex;align-items:center;gap:14px;flex-wrap:wrap;margin-top:18px}.save-actions{display:flex;align-items:center;gap:10px;flex-wrap:wrap}.save-status{font-size:11px;color:var(--muted);margin-left:auto}.world-stop{display:flex;align-items:center;gap:12px;margin-top:14px;flex-wrap:wrap}.stop-button{border-color:var(--danger);color:var(--danger)}.world-stop>span{font-size:11px;color:var(--muted)}.world-error,.world-warning,.world-message{font-size:12px;line-height:1.65;padding:10px 13px;margin:14px 0;border-left:3px solid var(--warning);background:var(--input-bg);overflow-wrap:anywhere}.world-error{color:var(--danger);border-color:var(--danger)}.world-message{border-color:var(--accent)}.empty-world{display:grid;justify-items:start;gap:12px;margin:22px 0}.empty-world p{max-width:65ch;color:var(--muted);font-size:12px}.world-confirmed{margin-top:20px;border-top:1px solid var(--line);padding-top:12px;font-size:11px;color:var(--muted)}.world-confirmed summary{cursor:pointer}.world-confirmed p{margin:8px 0}@media(max-width:680px){.editor-context{align-items:stretch;flex-direction:column;gap:12px}.editor-context>label{max-width:none}.save-status{margin-left:0}.save-actions{width:100%}.save-actions>.button{flex:1}.world-actions>.text-link{font-size:12px}}
</style>
