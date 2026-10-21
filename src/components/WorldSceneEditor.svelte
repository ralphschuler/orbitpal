<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Check, Cookie, Gamepad2, Maximize2, Moon, Plus, Trash2, UserRound, Users } from '@lucide/svelte';
  import { t, locale, formatNumber } from '../lib/i18n';
  import type {PackObjectTemplate} from '../lib/content-templates';
  import {sameContentRef,type ContentRef} from '../lib/content-reference';
  import {lifeSceneParticipants,LIFE_SCENE_LIMIT_HINT} from '../lib/life-scene-participants';
  import { anchorKinds, type AnchorKind, type WorldDraft, type WorldSnapshot } from '../lib/world-state';
  import { migrateEditorObject,addEditorObject,canvasPoint, cloneEditorModel, editorActors, editorRegion, editorSpawn, moveEditorTarget, nudgeEditorTarget, readEditorPoint, removeEditorTarget, targetKey, targetPoint, writeEditorPoint, type ActorPositions, type CanvasRect, type EditorCorner, type EditorModel, type EditorTarget } from '../lib/world-editor';

  interface Props {
    draft: WorldDraft; positions: ActorPositions; snapshot?: WorldSnapshot;
    bots: { id: string; name: string }[]; objectTemplates?:PackObjectTemplate[]; disabled?: boolean; active?: boolean;
    onChange: (draft: WorldDraft, positions: ActorPositions) => void;
    onGestureChange: (active: boolean) => void;
  }
  let { draft, positions, snapshot, bots, objectTemplates = [], disabled = false, active = true, onChange, onGestureChange }: Props = $props();
  const uid = $props.id();
  let tab = $state<'actors' | 'objects'>('actors');
  let selected = $state<EditorTarget | null>(null);
  let surface = $state<HTMLDivElement>();
  interface Gesture { pointer: number; element: HTMLButtonElement; rect: CanvasRect; origin: { x: number; y: number }; point: { x: number; y: number }; model: EditorModel; actors: ActorPositions; target: EditorTarget }
  let gesture = $state<Gesture | null>(null);
  const region = $derived(editorRegion(draft));
  const actors = $derived(editorActors(draft, snapshot, positions));
  const participants = $derived(lifeSceneParticipants(draft.members));
  const slots = $derived(draft.anchors.reduce((count, anchor) => count + anchor.slots.length, 0));
  const selectedPoint = $derived(selected ? targetPoint({ draft, positions }, selected, actors) : null);
  const chosenAnchor = $derived.by(() => {
    const target = selected;
    return target?.kind === 'anchor' ? draft.anchors.find(anchor => anchor.id === target.id)
      : target?.kind === 'slot' ? draft.anchors.find(anchor => anchor.id === target.anchorId) : undefined;
  });
  const migration=$derived(chosenAnchor?.contentRef?objectTemplates.find(t=>t.kind===chosenAnchor!.kind&&t.reference.packageId===chosenAnchor!.contentRef!.packageId&&t.reference.templateId===chosenAnchor!.contentRef!.templateId&&t.reference.packageVersion>chosenAnchor!.contentRef!.packageVersion):undefined);
  function migrateObject(){if(disabled||!active||!chosenAnchor||!migration)return;update(migrateEditorObject(model(),chosenAnchor.id,migration.reference,migration.kind));}
  const missing = $derived(draft.members.filter(id => !bots.some(bot => bot.id === id)));
  const corners: { id: EditorCorner; label: string }[] = [
    { id: 'nw', label: 'Obere linke Ecke' }, { id: 'ne', label: 'Obere rechte Ecke' },
    { id: 'sw', label: 'Untere linke Ecke' }, { id: 'se', label: 'Untere rechte Ecke' },
  ];
  const name = (id: string) => bots.find(bot => bot.id === id)?.name ?? id;
  const same = (target: EditorTarget) => targetKey(selected) === targetKey(target);
  const model = (): EditorModel => ({ draft, positions });
  function update(next: EditorModel) { onChange(next.draft, next.positions); }
  function select(target: EditorTarget) { if (!disabled && active) selected = target; }
  function member(id: string) {
    if (disabled || !active) return;
    if (!draft.members.includes(id)) {
      if (!participants.canAdd) return;
      const next = cloneEditorModel(model());
      if (!snapshot?.actors.some(actor => actor.actorId === id)) next.positions = { ...next.positions, [id]: editorSpawn(region, next.draft.members.length) };
      next.draft.members.push(id); update(next);
    }
    selected = { kind: 'actor', id };
  }
  function addObject(kind: AnchorKind,reference?:ContentRef) {
    if(disabled||!active)return;
    const suffix=crypto.randomUUID(),base=model(),next=addEditorObject(base,kind,suffix,reference);
    if(next===base)return;
    selected={kind:'anchor',id:`anchor_${suffix}`};update(next);
  }
  function objectName(anchor:WorldDraft['anchors'][number]):string {
    return anchor.contentRef?objectTemplates.find(t=>sameContentRef(t.reference,anchor.contentRef!))?.name??$t('Paketobjekt · Vorlage derzeit nicht aktiv'):$t(anchorKinds[anchor.kind]);
  }
  function addSeat() {
    if (disabled || !active || !chosenAnchor || slots >= 128) return;
    const next = cloneEditorModel(model()), anchor = next.draft.anchors.find(item => item.id === chosenAnchor!.id)!;
    const origin = readEditorPoint(anchor.position), seat = { x: Math.min(region.max.x, origin.x + (anchor.slots.length % 3 - 1) * 65_000), y: Math.min(region.max.y, origin.y + 80_000) };
    seat.x = Math.max(region.min.x, seat.x);
    const id = `slot_${crypto.randomUUID()}`; anchor.slots.push({ id, position: writeEditorPoint(seat) });
    selected = { kind: 'slot', anchorId: anchor.id, id }; update(next);
  }
  async function remove(target: EditorTarget) {
    if (disabled || !active || target.kind === 'boundary') return;
    if (target.kind === 'slot' && (draft.anchors.find(anchor => anchor.id === target.anchorId)?.slots.length ?? 0) <= 1) return;
    update(removeEditorTarget(model(), target)); selected = null;
    await tick(); if (active) surface?.focus();
  }
  function begin(event: PointerEvent, target: EditorTarget) {
    if (disabled || !active || !event.isPrimary || event.button !== 0 || gesture || !surface) return;
    const bounds = surface.getBoundingClientRect(), rect = { left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height };
    const origin = canvasPoint(event.clientX, event.clientY, rect), base = cloneEditorModel(model());
    const point = targetPoint(base, target, actors); if (!origin || !point) return;
    const element = event.currentTarget as HTMLButtonElement;
    try { element.setPointerCapture(event.pointerId); } catch { return; }
    event.preventDefault(); element.focus(); selected = target;
    gesture = { pointer: event.pointerId, element, rect, origin, point: { ...point }, model: base,
      actors: Object.fromEntries(Object.entries(actors).map(([id, point]) => [id, { ...point }])), target };
    onGestureChange(true);
  }
  function drag(event: PointerEvent) {
    if (!gesture || gesture.pointer !== event.pointerId) return;
    const point = canvasPoint(event.clientX, event.clientY, gesture.rect); if (!point) { cancel(); return; }
    update(moveEditorTarget(gesture.model, gesture.target,
      { x: gesture.point.x + point.x - gesture.origin.x, y: gesture.point.y + point.y - gesture.origin.y }, gesture.actors));
  }
  function finish(event: PointerEvent) {
    if (!gesture || gesture.pointer !== event.pointerId) return;
    drag(event); const held = gesture; if (!held) return;
    gesture = null; onGestureChange(false);
    if (held.element.hasPointerCapture(held.pointer)) held.element.releasePointerCapture(held.pointer);
  }
  function cancel() {
    const held = gesture; if (!held) return;
    gesture = null; update(held.model); onGestureChange(false);
    if (held.element.hasPointerCapture(held.pointer)) held.element.releasePointerCapture(held.pointer);
  }
  function lost(event: PointerEvent) { if (gesture?.pointer === event.pointerId) cancel(); }
  function key(event: KeyboardEvent, target: EditorTarget) {
    if (disabled || !active || event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.key === 'Escape' && gesture) { event.preventDefault(); event.stopPropagation(); cancel(); return; }
    if (event.key === 'Delete' || event.key === 'Backspace') {
      selected = target;
      if (target.kind !== 'boundary') { event.preventDefault(); event.stopPropagation(); void remove(target); } return;
    }
    if (event.key.startsWith('Arrow')) {
      event.preventDefault(); event.stopPropagation(); selected = target;
      update(nudgeEditorTarget(model(), target, event.key, event.shiftKey, actors));
    }
  }
  $effect(() => { if ((!active || disabled) && gesture) cancel(); });
  $effect(() => { if (selected && !targetPoint(model(), selected, actors)) selected = null; });
  onMount(() => {
    const observer = new ResizeObserver(() => {
      if (!gesture || !surface) return;
      const rect = surface.getBoundingClientRect();
      if (Math.abs(rect.width - gesture.rect.width) > .5 || Math.abs(rect.height - gesture.rect.height) > .5) cancel();
    });
    if (surface) observer.observe(surface);
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape' && gesture) { event.preventDefault(); event.stopPropagation(); cancel(); } };
    const blur = () => cancel();
    window.addEventListener('keydown', escape, true); window.addEventListener('blur', blur);
    return () => { cancel(); observer.disconnect(); window.removeEventListener('keydown', escape, true); window.removeEventListener('blur', blur); };
  });
</script>

<div class="scene-editor">
  <aside class="scene-tray" aria-label={$t('Figuren und Objekte')}>
    <div class="tray-tabs" role="group" aria-label={$t('Zur Bühne hinzufügen')}>
      <button type="button" class:active={tab === 'actors'} aria-pressed={tab === 'actors'} onclick={() => tab = 'actors'}><UserRound size={15}/>{$t('Figuren')}</button>
      <button type="button" class:active={tab === 'objects'} aria-pressed={tab === 'objects'} onclick={() => tab = 'objects'}><Gamepad2 size={15}/>{$t('Objekte')}</button>
    </div>
    {#if tab === 'actors'}
      <p class="tray-note">{$t('Wähle eine Figur und ziehe sie an ihren Platz.')}</p>
      <p class="tray-note">{$t(LIFE_SCENE_LIMIT_HINT,{limit:participants.limit})}</p>
      <div class="tray-list">{#each bots as bot (bot.id)}
        <button type="button" class="tray-item" class:chosen={selected?.kind === 'actor' && selected.id === bot.id} disabled={disabled || !draft.members.includes(bot.id) && !participants.canAdd} onclick={() => member(bot.id)}>
          <span class="actor-avatar">{bot.name.slice(0, 1)}</span><span class="tray-name">{bot.name}</span>
          {#if draft.members.includes(bot.id)}<Check size={14} aria-label={$t('Auf der Bühne')}/>{:else}<Plus size={14} aria-label={$t('Hinzufügen')}/>{/if}
        </button>
      {/each}{#each missing as id (id)}<button type="button" class="tray-item missing" disabled={disabled} onclick={() => select({ kind: 'actor', id })}><UserRound size={17}/><span>{$t('Nicht mehr verfügbar: {id}', { id })}</span></button>{/each}</div>
      {#if !bots.length}<p class="tray-note">{$t('Lege zuerst eine Figur in OrbitPal an.')}</p>{/if}
    {:else}
      <p class="tray-note">{$t('Stelle Ruheplätze, Snackplätze, Treffpunkte und Spielobjekte auf.')}</p>
      <div class="object-tools">{#each Object.entries(anchorKinds) as [kind, label]}
        <button type="button" class="tray-item" disabled={disabled || draft.anchors.length >= 64 || slots >= 128} onclick={() => addObject(kind as AnchorKind)}>
          {#if kind === 'rest'}<Moon size={18}/>{:else if kind === 'meeting'}<Users size={18}/>{:else if kind === 'snack'}<Cookie size={18}/>{:else}<Gamepad2 size={18}/>{/if}
          <span class="tray-name">{$t(label)}</span><Plus size={14}/>
        </button>
      {/each}</div>
      {#if objectTemplates.length}<p class="tray-note">{$t('Aus aktiven Inhaltspaketen')}</p><div class="object-tools">{#each objectTemplates as template(`${template.reference.packageId}:${template.reference.packageVersion}:${template.reference.templateId}`)}<button type="button" class="tray-item" disabled={disabled||draft.anchors.length>=64||slots>=128} onclick={()=>addObject(template.kind,template.reference)}><Gamepad2 size={18}/><span class="tray-name">{template.name}<small>{template.packageName}</small></span><Plus size={14}/></button>{/each}</div>{/if}
      <div class="tray-list placed-objects">{#each draft.anchors as anchor, index (anchor.id)}<button type="button" class="tray-item" class:chosen={chosenAnchor?.id === anchor.id} disabled={disabled} onclick={() => select({ kind: 'anchor', id: anchor.id })}><span class="object-number">{index + 1}</span><span class="tray-name">{objectName(anchor)}</span></button>{/each}</div>
      <p class="tray-note">{$t('Objekte beschreiben Plätze. Sie starten keine Animation und verändern keine Bedürfnisse.')}</p>
    {/if}
  </aside>

  <div class="scene-workspace">
    <div class="scene-toolbar"><span id={`${uid}-title`}>{$t('Desktop-Vorschau')}</span><button class="text-link" type="button" disabled={disabled} onclick={() => select({ kind: 'boundary', corner: 'se' })}><Maximize2 size={14}/>{$t('Bewegungsbereich')}</button></div>
    <div class="monitor-frame"><div bind:this={surface} class="scene-surface" class:dragging={!!gesture} role="group" aria-labelledby={`${uid}-title`} aria-describedby={`${uid}-help`} tabindex="-1">
      <div class="scene-grid" aria-hidden="true"></div>
      <button type="button" class="region-outline" class:selected={selected?.kind === 'boundary'}
        style:left={`${region.min.x / 10000}%`} style:top={`${region.min.y / 10000}%`} style:width={`${(region.max.x - region.min.x) / 10000}%`} style:height={`${(region.max.y - region.min.y) / 10000}%`}
        disabled={disabled} aria-label={$t('Bewegungsbereich auswählen')} onclick={() => select({ kind: 'boundary', corner: 'se' })} onkeydown={event => key(event, { kind: 'boundary', corner: 'se' })}></button>
      {#each corners as corner (corner.id)}
        {@const target = { kind: 'boundary', corner: corner.id } as EditorTarget}
        {@const point = targetPoint(model(), target, actors)!}
        <button type="button" class={`boundary-handle ${corner.id}`} class:selected={same(target)} style:left={`${point.x / 10000}%`} style:top={`${point.y / 10000}%`} disabled={disabled}
          aria-label={$t('Bewegungsbereich: {corner}', { corner: $t(corner.label) })} onclick={() => select(target)} onkeydown={event => key(event, target)}
          onpointerdown={event => begin(event, target)} onpointermove={drag} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={lost}></button>
      {/each}
      {#each draft.anchors as anchor, index (anchor.id)}
        {@const point = readEditorPoint(anchor.position)}
        {@const target = { kind: 'anchor', id: anchor.id } as EditorTarget}
        <button type="button" class={`object-handle ${anchor.kind}`} class:selected={same(target)} style:left={`${point.x / 10000}%`} style:top={`${point.y / 10000}%`} disabled={disabled}
          aria-label={$t('Objekt {number}: {kind}', { number: index + 1, kind: objectName(anchor) })} onclick={() => select(target)} onkeydown={event => key(event, target)}
          onpointerdown={event => begin(event, target)} onpointermove={drag} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={lost}>
          {#if anchor.kind === 'rest'}<Moon size={18}/>{:else if anchor.kind === 'meeting'}<Users size={18}/>{:else if anchor.kind === 'snack'}<Cookie size={18}/>{:else}<Gamepad2 size={18}/>{/if}<span>{index + 1}</span>
        </button>
        {#each anchor.slots as slot, seat (slot.id)}
          {@const position = readEditorPoint(slot.position)}
          {@const target = { kind: 'slot', anchorId: anchor.id, id: slot.id } as EditorTarget}
          <button type="button" class="seat-handle" class:selected={same(target)} style:left={`${position.x / 10000}%`} style:top={`${position.y / 10000}%`} disabled={disabled}
            aria-label={$t('Objekt {object}, Platz {seat}', { object: index + 1, seat: seat + 1 })} onclick={() => select(target)} onkeydown={event => key(event, target)}
            onpointerdown={event => begin(event, target)} onpointermove={drag} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={lost}><span>{seat + 1}</span></button>
        {/each}
      {/each}
      {#each draft.members as id (id)}
        {@const point = actors[id]}
        {@const target = { kind: 'actor', id } as EditorTarget}
        <button type="button" class="actor-handle" class:selected={same(target)} class:edited={Object.hasOwn(positions, id)} style:left={`${point.x / 10000}%`} style:top={`${point.y / 10000}%`} disabled={disabled}
          aria-label={$t('Figur platzieren: {name}', { name: name(id) })} onclick={() => select(target)} onkeydown={event => key(event, target)}
          onpointerdown={event => begin(event, target)} onpointermove={drag} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={lost}>
          <span class="actor-avatar">{name(id).slice(0, 1)}</span><span class="actor-label">{name(id)}</span>
        </button>
      {/each}
      {#if !draft.members.length && !draft.anchors.length}<p class="scene-empty">{$t('Füge links eine Figur oder ein Objekt hinzu.')}</p>{/if}
      <span class="draft-badge" aria-hidden="true">{$t('Entwurf')}</span>
    </div></div>
    <p id={`${uid}-help`} class="scene-help">{$t('Ziehen zum Platzieren. Pfeiltasten verschieben, Umschalt macht größere Schritte. Entf entfernt die Auswahl, Esc bricht das Ziehen ab.')}</p>
  </div>

  {#if selectedPoint}<p class="sr-only" role="status" aria-live="polite" aria-atomic="true">{$t('Position der Auswahl: {x} Prozent von links, {y} Prozent von oben.', { x: formatNumber(selectedPoint.x / 10000, $locale, { maximumFractionDigits: 4 }), y: formatNumber(selectedPoint.y / 10000, $locale, { maximumFractionDigits: 4 }) })}</p>{/if}
  <div class="scene-inspector">
    {#if selected?.kind === 'actor'}<div><strong>{name(selected.id)}</strong><p>{$t('Speichere, um die neue Position zu übernehmen.')}</p></div><button type="button" class="text-link remove" disabled={disabled} onclick={() => selected && remove(selected)}><Trash2 size={14}/>{$t('Von der Bühne entfernen')}</button>
    {:else if chosenAnchor && selected?.kind === 'anchor'}<div><strong>{objectName(chosenAnchor)}</strong><p>{$t('Beim Verschieben bleiben die zugehörigen Plätze zusammen.')}</p></div><div class="inspector-actions">{#if migration}<button type="button" class="button secondary small" disabled={disabled} onclick={migrateObject}>{$t('Auf Paketversion {version} umstellen',{version:migration.reference.packageVersion})}</button>{/if}<button type="button" class="button secondary small" disabled={disabled || slots >= 128} onclick={addSeat}><Plus size={14}/>{$t('Platz hinzufügen')}</button><button type="button" class="text-link remove" disabled={disabled} onclick={() => selected && remove(selected)}><Trash2 size={14}/>{$t('Objekt entfernen')}</button></div>
    {:else if chosenAnchor && selected?.kind === 'slot'}<div><strong>{$t('Fester Platz')}</strong><p>{$t('Ein Platz kann jeweils von einer passenden Aktion belegt werden.')}</p></div><button type="button" class="text-link remove" disabled={disabled || chosenAnchor.slots.length <= 1} onclick={() => selected && remove(selected)}><Trash2 size={14}/>{$t('Platz entfernen')}</button>
    {:else if selected?.kind === 'boundary'}<div><strong>{$t('Bewegungsbereich')}</strong><p>{$t('Ziehe die vier Eckgriffe. Vorhandene Figuren und Plätze bleiben innerhalb der Grenzen.')}</p></div>
    {:else}<div><strong>{$t('Wähle etwas auf der Bühne aus')}</strong><p>{$t('Hier findest du die Aktionen für Figuren, Objekte und Plätze.')}</p></div>{/if}
  </div>
</div>

<style>
  .scene-editor{display:grid;grid-template-columns:190px minmax(0,1fr);gap:0;border:1px solid var(--line);border-radius:12px;overflow:hidden;background:var(--panel)}
  .scene-tray{padding:14px;border-right:1px solid var(--line);background:var(--input-bg);min-width:0}.tray-tabs{display:flex;gap:4px}.tray-tabs button{display:flex;align-items:center;justify-content:center;gap:6px;flex:1;min-width:0;padding:9px 6px;border:0;border-radius:7px;background:transparent;color:var(--muted);font-size:12px;font-weight:650}.tray-tabs button.active{background:var(--panel);color:var(--text)}
  .tray-note,.scene-help,.scene-inspector p{font-size:11px;line-height:1.65;color:var(--muted)}.tray-note{margin:14px 2px}.tray-list,.object-tools{display:grid;gap:6px}.tray-item{display:flex;align-items:center;gap:8px;width:100%;min-height:42px;border:1px solid transparent;border-radius:8px;padding:8px;text-align:left;background:transparent;color:var(--text);font-size:12px}.tray-item:hover:not(:disabled),.tray-item.chosen{background:var(--panel);border-color:var(--line)}.tray-item.chosen{border-color:var(--accent)}.tray-name small{display:block;color:var(--muted);font-size:10px;margin-top:3px}.tray-name{flex:1;min-width:0;overflow-wrap:anywhere}.tray-item> :global(svg){flex-shrink:0;color:var(--accent)}.actor-avatar{display:grid;place-items:center;width:28px;height:28px;flex-shrink:0;background:var(--panel);border:1px solid var(--line);border-radius:50%;color:var(--accent);font-weight:700}.placed-objects{border-top:1px solid var(--line);margin-top:16px;padding-top:12px}.object-number{color:var(--accent);font-weight:700}.missing{color:var(--warning);overflow-wrap:anywhere}
  .scene-workspace{min-width:0;padding:16px}.scene-toolbar{display:flex;gap:10px;justify-content:space-between;align-items:center;flex-wrap:wrap;margin-bottom:12px;font-size:12px;font-weight:650}.scene-toolbar button{font-size:11px}.monitor-frame{padding:6px;background:var(--input-bg);border:1px solid var(--line);border-radius:11px}.scene-surface{position:relative;isolation:isolate;aspect-ratio:16/10;min-height:270px;overflow:hidden;border-radius:5px;background:#111820}.scene-grid{position:absolute;inset:0;pointer-events:none;background-image:linear-gradient(#91cfc409 1px,transparent 1px),linear-gradient(90deg,#91cfc409 1px,transparent 1px);background-size:10% 10%}.region-outline{position:absolute;z-index:1;background:#91cfc407;border:1px dashed #91cfc46b;border-radius:2px;padding:0}.region-outline.selected{border-color:var(--accent);background:#91cfc40c}
  .boundary-handle{position:absolute;z-index:6;width:18px;height:18px;transform:translate(-50%,-50%);border:2px solid var(--accent);border-radius:4px;background:var(--panel);padding:0;touch-action:none}.boundary-handle.nw,.boundary-handle.se{cursor:nwse-resize}.boundary-handle.ne,.boundary-handle.sw{cursor:nesw-resize}.boundary-handle.selected{background:var(--accent)}
  .object-handle,.seat-handle,.actor-handle{position:absolute;transform:translate(-50%,-50%);touch-action:none;cursor:grab;user-select:none}.object-handle{z-index:2;display:flex;align-items:center;gap:5px;padding:8px 10px;color:var(--accent);background:var(--panel);border:1px solid #91cfc478;border-radius:10px}.object-handle span{font-size:10px;font-weight:700}.object-handle.snack{color:#e5b58f;border-color:#e5b58f80}.object-handle.play{color:var(--warning);border-color:#e7c08380}.object-handle.meeting{color:#b8c5ef;border-color:#b8c5ef80}.seat-handle{z-index:3;display:grid;place-items:center;width:22px;height:22px;padding:0;border:1px solid var(--warning);border-radius:50%;background:var(--input-bg);color:var(--warning);font-size:9px}.actor-handle{z-index:4;display:flex;align-items:center;gap:7px;border:1px solid var(--line);border-radius:20px;padding:5px 10px 5px 5px;background:var(--panel);color:var(--text);max-width:150px;min-height:40px}.actor-label{max-width:96px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:11px;font-weight:650}.actor-handle.edited{border-style:dashed;border-color:var(--accent)}.actor-handle.selected,.object-handle.selected,.seat-handle.selected{outline:2px solid var(--accent);outline-offset:3px}.dragging .actor-handle,.dragging .object-handle,.dragging .seat-handle{cursor:grabbing}
  .actor-handle.selected,.object-handle.selected,.seat-handle.selected{z-index:8}.scene-surface :is(button):focus-visible{outline:3px solid var(--accent);outline-offset:4px;z-index:9}.scene-empty{position:absolute;inset:34% 18%;text-align:center;pointer-events:none;color:var(--muted);font-size:12px;line-height:1.8;z-index:2}.draft-badge{position:absolute;bottom:8px;right:10px;font-size:10px;color:var(--muted);pointer-events:none}.scene-help{margin:11px 2px 0;max-width:76ch}.scene-inspector{grid-column:1/-1;display:flex;justify-content:space-between;align-items:center;gap:14px;min-height:80px;border-top:1px solid var(--line);padding:14px 18px}.scene-inspector strong{font-size:13px;overflow-wrap:anywhere}.scene-inspector p{margin:4px 0 0}.inspector-actions{display:flex;align-items:center;gap:14px;flex-wrap:wrap}.remove{color:var(--danger);white-space:normal;text-align:left;flex-shrink:0}
  @media(max-width:800px){.scene-editor{grid-template-columns:minmax(0,1fr)}.scene-tray{border-right:0;border-bottom:1px solid var(--line)}.tray-list,.object-tools{grid-template-columns:repeat(2,minmax(0,1fr))}.tray-tabs{max-width:300px}.tray-note{margin:10px 2px}.placed-objects{max-height:180px;overflow:auto}.scene-inspector{flex-wrap:wrap}.scene-workspace{padding:12px}.scene-surface{min-height:250px}}
  @media(prefers-reduced-motion:reduce){.scene-editor *{scroll-behavior:auto}}
</style>
