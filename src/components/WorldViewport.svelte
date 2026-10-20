<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { Box, RefreshCw, X, Navigation } from '@lucide/svelte';
  import Avatar from './Avatar.svelte';
  import HelpTip from './HelpTip.svelte';
  import { call, native, onEvent, assetUrl, prettyError } from '../lib/api';
  import { t, locale, formatNumber } from '../lib/i18n';
  import { percentToCanonical, type WorldState } from '../lib/world-state';
  import { WorldOutputBridge, type WorldRenderAdapter } from '../lib/world-output-bridge';
  import type { CanonicalPoint, MeasurementChallenge, NativeMotionBinding as MotionBinding, RegisteredWorldActor,
    RegisterWorldActor, WorldAdapterIdentity, WorldAdapterReason } from '../lib/world-output';
  import type { LifeActionsSnapshot } from '../lib/life-actions';
  import type { AvatarModelInfo } from '../lib/avatar';
  interface Props {
    world: WorldState; bots: { id: string; name: string; profile?: any }[];
    selectedBotId: string; outputUrl: string; visible: boolean; blocked: boolean;
    register: (request: Omit<RegisterWorldActor, 'studioProof'>) => Promise<unknown>;
    onMove: (target: CanonicalPoint) => Promise<LifeActionsSnapshot | undefined>;
  }
  let { world, bots, selectedBotId, outputUrl, visible, blocked, register, onMove }: Props = $props();
  type Layer = { actorId: string; mountId: string; profile: any; avatar?: WorldRenderAdapter;
    bridge?: WorldOutputBridge; output?: RegisteredWorldActor; binding?: MotionBinding;
    challenge?: MeasurementChallenge; prepared: boolean; assetReady: boolean; registrationPending: boolean; reason?: WorldAdapterReason };
  const uid = $props.id();
  let active = $state(false), exposed = $state(false), layers = $state<Layer[]>([]);
  let revision = $state(0), moving = $state(false), error = $state('');
  let mountsReady = $state(false);
  let target = $state({ x: '50', y: '50' });
  let documentVisible = $state(true);
  let viewport = $state<HTMLDivElement>();
  let alive = false, generation = 0, refreshingGeneration: number | null = null;
  const snapshot = $derived(world.snapshot);
  const selected = $derived(layers.find(layer => layer.actorId === selectedBotId));
  const ownerParticipant = $derived(snapshot?.actors.some(actor => actor.actorId === selectedBotId) === true
    && bots.some(bot => bot.id === selectedBotId && !!bot.profile));
  const eligible = $derived(native && visible && documentVisible && !blocked && world.availability === 'ready' && !!snapshot && ownerParticipant);
  const profiles = $derived((snapshot?.actors ?? []).flatMap(actor => {
    const bot = bots.find(bot => bot.id === actor.actorId);
    return bot?.profile ? [{ actorId: actor.actorId, profile: bot.profile }] : [];
  }).sort((a, b) => a.actorId === selectedBotId ? -1 : b.actorId === selectedBotId ? 1 : 0));
  const sceneKey = $derived(JSON.stringify([active && eligible, revision, world.instanceId,
    snapshot?.stageId, snapshot?.configRevision, selectedBotId, profiles]));
  const canMove = $derived(active && eligible && exposed && mountsReady && !moving && selected?.output?.role === 'owner_candidate'
    && !!selected.binding && selected.binding.walkSupported && !selected.binding.reducedMotion && !selected.reason);
  const name = (id: string) => bots.find(bot => bot.id === id)?.name ?? id;
  function position(id: string): CanonicalPoint {
    return snapshot?.actors.find(actor => actor.actorId === id)?.position ?? { x: 500_000, y: 500_000 };
  }
  function current(layer: Layer, ticket: number): boolean {
    return alive && ticket === generation && active && eligible && !layer.reason
      && layers.some(value => value.mountId === layer.mountId);
  }
  function identityMatches(layer: Layer, identity: WorldAdapterIdentity): boolean {
    return identity.actorId === layer.actorId && identity.renderSessionId === layer.output?.renderSessionId
      && identity.bindingSequence === layer.output?.bindingSequence;
  }
  function clear() {
    ++generation; exposed = false; mountsReady = false;
    for (const layer of layers) layer.bridge?.dispose();
    layers = [];
  }
  async function openScene() {
    if (!eligible || moving) return;
    active = true; revision++;
    const expected = revision;
    await tick();
    // This explicit open may expand below the current scroll position. Make the
    // measured surface visible before its first calibration animation frame.
    if (alive && active && expected === revision) viewport?.scrollIntoView({ block: 'center', behavior: 'instant' });
  }
  async function prepared(layer: Layer, identity: WorldAdapterIdentity, ticket: number) {
    if (!current(layer, ticket) || !identityMatches(layer, identity) || layer.actorId !== selectedBotId) return;
    layer.prepared = true;
    // Removing the known preparation cover is a DOM barrier. Only a NEW frame
    // rendered after worldExposed becomes true can answer the native challenge.
    await tick();
    if (current(layer, ticket) && layer.prepared) exposed = true;
  }
  function rebuild() {
    clear(); error = '';
    if (!active || !eligible || !snapshot) return;
    const ticket = generation;
    layers = profiles.slice(0, 8).map(actor => ({ ...actor, mountId: crypto.randomUUID(), prepared: false, assetReady: false, registrationPending: false }));
    for (const layer of layers) {
      const adapter: WorldRenderAdapter = {
        acceptWorldCommand: command => layer.avatar?.acceptWorldCommand(command) ?? 'stale',
        beginWorldAfterDelivered: (fence, sequence) => layer.avatar?.beginWorldAfterDelivered(fence, sequence) ?? false,
        confirmWorldStarted: (fence, frame) => layer.avatar?.confirmWorldStarted(fence, frame) ?? false,
        cancelWorld: reason => layer.avatar?.cancelWorld(reason),
      };
      layer.bridge = new WorldOutputBridge({
        request: { version: 1, mountId: layer.mountId, actorId: layer.actorId,
          worldInstanceId: world.instanceId, stageId: snapshot.stageId, configRevision: snapshot.configRevision },
        register, invoke: call, adapter, current: () => current(layer, ticket),
        changed: (output, binding, challenge) => {
          if (!current(layer, ticket)) return;
          if (!layer.assetReady) { layer.bridge?.fail('asset_unavailable'); return; }
          layer.output = output; layer.binding = binding ?? undefined; layer.challenge = challenge ?? undefined;
        },
        failed: reason => { if (current(layer, ticket)) { layer.reason = reason;
          if (layer.actorId === selectedBotId) exposed = false; } },
      });
    }
    void refreshAll(ticket);
  }
  function modelLoaded(layer: Layer, info: AvatarModelInfo) {
    if (!current(layer, generation)) return;
    layer.assetReady = info.status === 'ready';
    if (info.status === 'error' || info.status === 'loading' && (!!layer.output || layer.registrationPending)) {
      layer.bridge?.fail('asset_unavailable');
    }
    if (info.status !== 'loading') void refreshAll(generation);
  }
  async function refreshAll(ticket: number) {
    // Asset loading must finish before requesting the native challenge.
    // This neither extends its deadline nor promotes an unacknowledged frame.
    if (layers.some(layer => !layer.assetReady && !layer.reason)) return;
    if (refreshingGeneration === ticket) return;
    refreshingGeneration = ticket;
    try {
      // A new native mount advances its cancellation generation. Await each
      // command result, without blocking the LifeUI proof queue, so unrelated
      // initial layers cannot invalidate one another's registration ticket.
      for (const layer of layers) {
        if (!alive || ticket !== generation || !active || !eligible) return;
        if (!layer.reason) {
          layer.registrationPending = true;
          try { await layer.bridge?.refresh(); }
          finally { layer.registrationPending = false; }
        }
      }
      if (alive && ticket === generation) mountsReady = true;
    } finally { if (refreshingGeneration === ticket) refreshingGeneration = null; }
  }
  async function move() {
    if (!canMove) return;
    const ticket = generation, actor = selectedBotId;
    let point: CanonicalPoint;
    try { point = { x: percentToCanonical(target.x), y: percentToCanonical(target.y) }; }
    catch (cause) { error = prettyError(cause); return; }
    moving = true; error = '';
    try { await onMove(point); }
    catch (cause) { if (alive && generation === ticket && selectedBotId === actor) error = prettyError(cause); }
    finally { if (alive) moving = false; }
  }
  $effect(() => { sceneKey; untrack(() => { if (alive) rebuild(); }); });
  $effect(() => { if (!visible) { active = false; exposed = false; } });
  onMount(() => {
    alive = true; documentVisible = document.visibilityState === 'visible';
    const visibility = () => { documentVisible = document.visibilityState === 'visible'; };
    document.addEventListener('visibilitychange', visibility);
    const listener = onEvent('cohost://world-action', raw => {
      if (!alive || !active || !eligible) return;
      for (const layer of layers) layer.bridge?.receive(raw);
    }).catch(() => { if (alive) { clear(); error = 'Die Verbindung zur sichtbaren Bühne ist nicht bestätigt.'; } return () => {}; });
    const timer = setInterval(() => { if (active && eligible) void refreshAll(generation); }, 3000);
    return () => { alive = false; clear(); clearInterval(timer); document.removeEventListener('visibilitychange', visibility); void listener.then(stop => stop()); };
  });
</script>

<section class="world-output" aria-labelledby={`${uid}-title`}>
  <div class="world-output-heading"><div><h3 id={`${uid}-title`}><Box size={17}/>{$t('Sichtbare 3D-Bühne')}</h3><p class="field-note">{$t('Zeigt gespeicherte Teilnehmer an einem gemeinsamen Ort. Öffnen startet keinen Modus und erteilt keine Tool- oder Bildschirmfreigabe.')}</p></div>
    {#if active}<div class="world-output-tools"><button class="button secondary small" onclick={openScene} disabled={!eligible || moving}><RefreshCw size={14}/>{$t('Darstellung neu verbinden')}</button><button class="icon-button" onclick={() => {active = false; clear();}} aria-label={$t('3D-Bühne schließen')}><X size={16}/></button></div>{:else}<button class="button secondary small" onclick={openScene} disabled={!eligible}><Box size={15}/>{$t('3D-Bühne anzeigen')}</button>{/if}
  </div>
  <p class="field-note">{$t('Bewegungen benötigen einen ausdrücklich gestarteten Life-Modus, Studio-Ausgabe und eine aktuell gemessene Figur. In dieser ersten Verbindung steuert der ausgewählte Bot seine Bewegung; weitere Teilnehmer erscheinen als Spiegelansichten.')}</p>
  {#if !ownerParticipant}<p class="field-note" role="status">{$t('Wähle diesen Bot als Teilnehmer einer gültigen gemeinsamen Bühne aus.')}</p>{/if}
  {#if active && eligible && snapshot}
    <div bind:this={viewport} class="world-viewport" aria-label={$t('Gemeinsame 3D-Darstellung der ausgewählten Teilnehmer')}>
      {#each layers as layer (layer.mountId)}<div class="world-actor-layer" data-world-actor={layer.actorId}>
        <Avatar bind:this={layer.avatar} profile={layer.profile} motionSource="world" transparent={true}
          worldOwner={layer.actorId === selectedBotId && layer.output?.role === 'owner_candidate' && !layer.reason}
          worldBinding={layer.binding} worldChallenge={layer.challenge} worldExposed={exposed && !layer.reason}
          worldMirrorPosition={position(layer.actorId)} audioEnabled={false} {outputUrl}
          assetUrl={assetUrl(layer.profile.avatar.assetPath, outputUrl)}
          onModelLoaded={info => modelLoaded(layer, info)}
          onWorldPrepared={identity => void prepared(layer, identity, generation)}
          onWorldMeasurement={report => layer.bridge?.measurement(report)}
          onWorldFrame={frame => layer.bridge?.frame(frame)}
          onWorldUnavailable={(identity, reason) => { if (identityMatches(layer, identity)) layer.bridge?.fail(reason); }}/>
      </div>{/each}
      <div class="world-object-layer" aria-hidden="true">
        {#each snapshot.anchors as anchor (anchor.id)}<span class="world-object" style:left={`${anchor.position.x / 10000}%`} style:top={`${anchor.position.y / 10000}%`} title={$t(anchor.kind === 'rest' ? 'Ruhe- oder Ladeplatz' : anchor.kind === 'meeting' ? 'Treffpunkt' : 'Spielobjekt')}>{anchor.kind === 'rest' ? '◒' : anchor.kind === 'meeting' ? '◎' : '◇'}</span>{/each}
      </div>
      {#if !selected?.prepared || selected?.reason}<div class="world-preparing" role="status"><span>{$t(selected?.reason ? 'Die Figur ist für diese Bewegung nicht verfügbar. Darstellung neu verbinden oder eine unterstützte Figur auswählen.' : 'Figur und Bewegungsbereich werden geprüft …')}</span></div>{/if}
    </div>
    <div class="world-participants" aria-label={$t('Sichtbare Teilnehmer')}>{#each layers as layer (layer.mountId)}<span><i class:ready={!!layer.binding && !layer.reason}></i>{name(layer.actorId)} · {$t(layer.reason ? 'Darstellung nicht bestätigt' : layer.binding ? layer.output?.role === 'owner_candidate' ? 'Bewegung verfügbar' : 'Spiegelansicht' : layer.output?.role === 'mirror' ? 'Spiegelansicht' : 'Wird geprüft …')}</span>{/each}</div>
    {#if profiles.length > 8}<p class="field-note">{$t('Diese Darstellung zeigt höchstens acht Teilnehmer gleichzeitig.')}</p>{/if}
    <form class="world-move" onsubmit={event => {event.preventDefault(); void move();}}>
      <fieldset disabled={!canMove}><legend>{$t('{name} einmal zum Ziel gehen lassen', {name: name(selectedBotId)})}<HelpTip text={$t('X und Y sind Prozent der gesamten sichtbaren Bühne. Das Ziel muss mit der ganzen Figur in den bestätigten Bewegungsbereich passen. Der Auftrag startet keine Folgeaktivität und verändert keine Bedürfnisse. Stop bricht ihn ab.')}/></legend>
        <div class="world-move-fields"><label for={`${uid}-x`}>{$t('Ziel X')} (%)<input id={`${uid}-x`} inputmode="decimal" maxlength="9" bind:value={target.x}/></label><label for={`${uid}-y`}>{$t('Ziel Y')} (%)<input id={`${uid}-y`} inputmode="decimal" maxlength="9" bind:value={target.y}/></label><button class="button secondary small" type="submit"><Navigation size={14}/>{$t(moving ? 'Bewegung wird angefordert …' : 'Zum Ziel gehen')}</button></div>
      </fieldset>
    </form>
    {#if selected?.binding}<p class="field-note">{$t('Bestätigte Zielmitte: X {minX}–{maxX} % · Y {minY}–{maxY} %.', {minX: formatNumber(selected.binding.safeCenters.left / selected.binding.viewport.width * 100, $locale, {maximumFractionDigits: 1}), maxX: formatNumber(selected.binding.safeCenters.right / selected.binding.viewport.width * 100, $locale, {maximumFractionDigits: 1}), minY: formatNumber(selected.binding.safeCenters.top / selected.binding.viewport.height * 100, $locale, {maximumFractionDigits: 1}), maxY: formatNumber(selected.binding.safeCenters.bottom / selected.binding.viewport.height * 100, $locale, {maximumFractionDigits: 1})})}</p>{/if}
  {:else if active}<p class="field-note" role="status">{$t('Die sichtbare Bühne wartet auf eine bestätigte Studio-Verbindung und freie Bedienoberfläche.')}</p>{/if}
  {#if error}<p class="world-render-error" role="alert">{$t(error)}</p>{/if}
</section>

<style>
  .world-output{border-top:1px solid var(--line);margin-top:24px;padding-top:18px}.world-output-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.world-output-heading h3{display:flex;align-items:center;gap:8px;margin:0}.world-output-tools{display:flex;align-items:center;gap:8px;flex-shrink:0}.field-note{font-size:12px;color:var(--muted);line-height:1.7;margin:10px 0}.world-viewport{position:relative;isolation:isolate;height:460px;min-height:360px;background:radial-gradient(ellipse at 50% 80%,#91cfc412,transparent 70%),var(--input-bg);border:1px solid var(--line);margin:18px 0}.world-actor-layer,.world-object-layer{position:absolute;inset:0;pointer-events:none}.world-object-layer{z-index:20}.world-object{position:absolute;color:var(--accent);font-size:24px;line-height:1}.world-preparing{position:absolute;inset:0;z-index:30;display:grid;place-items:center;padding:24px;background:var(--panel);color:var(--muted);font-size:12px;text-align:center}.world-participants{display:flex;flex-wrap:wrap;gap:8px 18px;font-size:11px;color:var(--muted)}.world-participants span{display:flex;align-items:center;gap:6px;overflow-wrap:anywhere}.world-participants i{width:6px;height:6px;border-radius:50%;background:var(--muted)}.world-participants i.ready{background:var(--accent)}fieldset{border:0;padding:0;margin:18px 0 0;min-width:0}legend{font-weight:600;font-size:12px;margin-bottom:10px}.world-move-fields{display:flex;align-items:end;gap:12px}.world-move-fields label{max-width:140px;margin:0}.world-move-fields input{margin-bottom:0}.world-move-fields button{margin-bottom:0;min-height:40px}.world-render-error{padding:12px;border-left:3px solid var(--danger);color:var(--danger);font-size:12px;overflow-wrap:anywhere}@media(max-width:600px){.world-output-heading{flex-direction:column}.world-move-fields{flex-wrap:wrap}.world-move-fields label{max-width:none;flex:1;min-width:90px}.world-move-fields button{width:100%}.world-viewport{height:380px}}
</style>
