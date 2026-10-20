<script lang="ts">
  import { onMount } from 'svelte';
  import { Map, Plus, Trash2, RefreshCw, Users } from '@lucide/svelte';
  import { call, native, onEvent, prettyError } from '../lib/api';
  import { t, locale, formatNumber } from '../lib/i18n';
  import { anchorKinds, canonicalToPercent, draftWorld, newWorldDraft, parseWorldSettings, parseWorldState, placeWorld, saveWorld, worldDraft, worldDraftChanged, worldReasons, type AnchorKind, type PointDraft, type WorldDraft, type WorldRecord, type WorldSettings, type WorldState } from '../lib/world-state';
  import type { OutputDisplay } from '../lib/output-presentation';
  import HelpTip from './HelpTip.svelte';
  interface Props { bots:{id:string;name:string}[]; displays?:OutputDisplay[]; visible:boolean; disabled?:boolean; onDirtyChange:(dirty:boolean)=>void; onBusyChange:(busy:boolean)=>void; confirmDiscard:(action:()=>void)=>void; }
  let { bots, displays=[], visible, disabled=false, onDirtyChange, onBusyChange, confirmDiscard }:Props=$props();
  const uid=$props.id();
  let settings=$state<WorldSettings>();
  let state=$state<WorldState>();
  let baseline=$state<WorldRecord|null>(null);
  let draft=$state<WorldDraft>();
  let writing=$state(false);
  let pending=$state(false),error=$state(''),message=$state(''),unconfirmed=$state(false);
  let actor=$state(''),position=$state<PointDraft>({x:'50',y:'50'}),placementEdited=$state(false);
  let form=$state<HTMLFormElement>();
  let alive=false,operation=0;
  const configDirty=$derived(!!draft&&worldDraftChanged(draft,baseline));
  const dirty=$derived(configDirty||placementEdited);
  const snapshot=$derived(state?.snapshot);
  const parsedDraft=$derived.by(()=>{try{return draft?draftWorld(draft):undefined;}catch{return undefined;}});
  const name=(id:string)=>bots.find(bot=>bot.id===id)?.name??id;
  const number=(value:number)=>formatNumber(value,$locale);
  const missing=$derived((draft?.members??[]).filter(id=>!bots.some(bot=>bot.id===id)));
  const totalSlots=$derived(draft?.anchors.reduce((count,anchor)=>count+anchor.slots.length,0)??0);
  $effect(()=>onDirtyChange(dirty));
  $effect(()=>onBusyChange(writing));
  $effect(()=>{if(visible&&alive&&!settings&&!pending&&!error)void refresh();});
  function selectActor(id:string){actor=id;placementEdited=false;const current=snapshot?.actors.find(item=>item.actorId===id);position=current?{x:canonicalToPercent(current.position.x),y:canonicalToPercent(current.position.y)}:{x:'50',y:'50'};}
  function acceptPublic(value:unknown){
    const next=parseWorldState(value);
    if(!settings||next.instanceId!==settings.instanceId){unconfirmed=true;return;}
    if(next.snapshot&&(!settings.record||next.snapshot.stageId!==settings.record.config.stageId||next.snapshot.configRevision!==settings.record.config.revision)){unconfirmed=true;return;}
    if(state?.instanceId===next.instanceId&&state.snapshot&&next.snapshot&&next.snapshot.sequence<state.snapshot.sequence)return;
    state=next;unconfirmed=false;
    if(actor&&!next.snapshot?.actors.some(item=>item.actorId===actor))selectActor('');
    else if(actor&&!placementEdited)selectActor(actor);
  }
  async function refresh(){
    if(!native||pending||disabled)return;const ticket=++operation;pending=true;
    try{
      const [rawSettings,rawState]=await Promise.all([call('get_world_settings'),call('get_world_state')]);
      if(!alive||ticket!==operation)return;
      const next=parseWorldSettings(rawSettings);parseWorldState(rawState);
      const keep=configDirty;settings=next;
      if(!keep){baseline=next.record;draft=next.record?worldDraft(next.record):undefined;}
      acceptPublic(rawState);error='';message='';
    }catch(cause){if(alive&&ticket===operation){error=prettyError(cause);unconfirmed=true;}}
    finally{if(alive&&ticket===operation)pending=false;}
  }
  function prepare(){if(settings?.availability!=='not_initialized'||draft||pending||disabled)return;draft=newWorldDraft(`stage_${crypto.randomUUID()}`);baseline=null;message='';}
  function discard(){if(!dirty||pending)return;confirmDiscard(()=>{draft=settings?.record?worldDraft(settings.record):undefined;baseline=settings?.record??null;placementEdited=false;selectActor(actor);message='';error='';});}
  async function save(){
    if(!draft||!configDirty||pending||disabled||!settings||settings.availability==='unavailable'&&!settings.record)return;let nextDraft:WorldDraft;
    try{draftWorld(draft);nextDraft=structuredClone($state.snapshot(draft));}catch(cause){error=prettyError(cause);return;}
    const ticket=++operation;pending=true;writing=true;message='';
    try{
      const next=await saveWorld(call,nextDraft,baseline?.config.revision??null);
      if(!alive||ticket!==operation)return;
      settings=next;baseline=next.record;draft=next.record?worldDraft(next.record):undefined;state=undefined;unconfirmed=true;error='';message='Bühnenkonfiguration gespeichert. Es wurde kein Modus gestartet.';
      acceptPublic(await call('get_world_state'));
    }catch(cause){if(alive&&ticket===operation){error=prettyError(cause);unconfirmed=true;}}
    finally{if(alive&&ticket===operation){pending=false;writing=false;}}
  }
  function member(id:string,checked:boolean){if(!draft)return;draft.members=checked?[...new Set([...draft.members,id])]:draft.members.filter(value=>value!==id);message='';}
  function addAnchor(kind:AnchorKind){if(!draft||draft.anchors.length>=64||totalSlots>=128)return;const index=crypto.randomUUID();draft.anchors.push({id:`anchor_${index}`,kind,position:{x:'50',y:'50'},slots:[{id:`slot_${index}`,position:{x:'50',y:'50'}}]});message='';}
  function addSlot(index:number){if(!draft||totalSlots>=128)return;const anchor=draft.anchors[index];anchor.slots.push({id:`slot_${crypto.randomUUID()}`,position:{...anchor.position}});}
  async function place(){
    if(!snapshot||!actor||!placementEdited||configDirty||unconfirmed||pending||disabled)return;
    const ticket=++operation;pending=true;writing=true;message='';
    try{const next=await placeWorld(call,snapshot.configRevision,actor,$state.snapshot(position));if(!alive||ticket!==operation)return;acceptPublic(next);placementEdited=false;selectActor(actor);error='';message='Position nativ bestätigt. Die Figur wurde nicht automatisch bewegt.';}
    catch(cause){if(alive&&ticket===operation){error=prettyError(cause);unconfirmed=true;}}
    finally{if(alive&&ticket===operation){pending=false;writing=false;}}
  }
  export function saveDraft(){return configDirty?save():place();}
  function shortcut(event:KeyboardEvent){if(!visible||event.isComposing||!form?.contains(event.target as Node))return;if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'){event.preventDefault();event.stopPropagation();void save();}}
  onMount(()=>{alive=true;if(visible)void refresh();const stop=onEvent('cohost://world-state',value=>{if(!alive)return;try{acceptPublic(value);}catch(cause){error=prettyError(cause);unconfirmed=true;}});window.addEventListener('keydown',shortcut,{capture:true});const timer=setInterval(()=>{if(visible&&settings&&!error)void refresh();},5000);return()=>{alive=false;++operation;clearInterval(timer);void stop.then(unlisten=>unlisten());window.removeEventListener('keydown',shortcut,{capture:true});onDirtyChange(false);onBusyChange(false);};});
</script>

<section class="settings-panel world-panel" aria-labelledby={`${uid}-title`}>
  <div class="world-heading"><div class="section-heading"><Map size={20}/><div><h2 id={`${uid}-title`}>{$t('Gemeinsame Bühne')}</h2><p>{$t('Ein gemeinsamer Raum für ausdrücklich ausgewählte Bots dieser Installation.')}</p></div></div><button class="button secondary small" onclick={refresh} disabled={!native||pending||disabled}><RefreshCw size={14}/>{$t('Bühne aktualisieren')}</button></div>
  <p class="world-scope">{$t('Diese Einstellung gilt für alle Teilnehmer, unabhängig vom gerade ausgewählten Bot. Beitreten und Speichern starten weder Life noch Ambient und erteilen keinen Bildschirmzugriff.')}</p>
  <p class="field-note">{$t('Hier legst du die Weltbeschreibung fest. Die 2D-Vorschau ist ein Entwurf, keine laufende Figurenszene. Automatisches Gehen zu Objekten und 3D-Weltdarstellung folgen separat.')}</p>
  {#if !native}<p role="status">{$t('Die gemeinsame Bühne wird in der Desktop-App verwaltet.')}</p>{:else if !settings}<p role="status">{$t(pending?'Bühnenstatus wird geladen …':'Bühnenstatus nicht bestätigt. Bitte aktualisieren.')}</p>{/if}
  {#if error}<p class="world-error" role="alert">{error}</p>{/if}
  {#if settings?.reason}<p class="world-warning" role="status">{$t(worldReasons[settings.reason])}</p>{/if}
  {#if unconfirmed}<p class="world-warning" role="status">{$t('Der angezeigte Bühnenstand ist nicht aktuell bestätigt. Entwürfe bleiben erhalten; Positionen und Belegungen gelten nicht als aktuelle Zusage.')}</p>{/if}
  {#if settings?.availability==='not_initialized'&&!draft}<div class="empty-world"><Users size={24}/><p>{$t('Noch keine gemeinsame Bühne gespeichert. Erst ein bewusster Entwurf und Speichern legen sie an.')}</p><button class="button secondary" onclick={prepare} disabled={pending||disabled}>{$t('Bühne vorbereiten')}</button></div>{/if}
  {#if settings?.availability==='unavailable'&&!settings.record}<p class="field-note">{$t('Die gespeicherten Daten werden unverändert aufbewahrt. Es wird keine Ersatzbühne angelegt und nichts automatisch zurückgesetzt.')}</p>{/if}
  {#if draft}
    <form bind:this={form} onsubmit={event=>{event.preventDefault();void save();}} oninput={()=>message=''}>
      <fieldset disabled={pending||disabled}>
        <legend>{$t('Bühnenentwurf bearbeiten')}</legend>
        <p class="field-note">{$t('Der Entwurf bleibt beim Botwechsel und beim Wechsel in andere Bereiche erhalten. Selbstständige Anfragen warten, bis du speicherst oder den Entwurf bewusst verwirfst.')}</p>
        <div class="world-grid"><div>
          <h3>{$t('Teilnehmer')} <HelpTip text={$t('Nur hier ausgewählte Bots gehören zur gemeinsamen Bühne. Ihre eigenen Life-Verläufe bleiben getrennt. Kopierte und importierte Bots treten nicht automatisch bei.')}/></h3>
          <div class="world-members">{#each bots as bot(bot.id)}<label><input type="checkbox" checked={draft.members.includes(bot.id)} disabled={!draft.members.includes(bot.id)&&draft.members.length>=64} onchange={event=>member(bot.id,event.currentTarget.checked)}/><span>{bot.name}</span></label>{/each}{#each missing as id(id)}<label class="world-warning"><input type="checkbox" checked onchange={event=>member(id,event.currentTarget.checked)}/><span>{$t('Nicht mehr verfügbar: {id}',{id})}</span></label>{/each}</div>
          <h3>{$t('Zielmonitor')}</h3><label class="sr-only" for={`${uid}-monitor`}>{$t('Zielmonitor der gemeinsamen Bühne')}</label><select id={`${uid}-monitor`} bind:value={draft.monitorId}><option value="">{$t('Sicherer Ersatzmonitor · automatisch geprüft')}</option>{#if draft.monitorId&&!displays.some(display=>display.id===draft.monitorId)}<option value={draft.monitorId}>{$t('Gespeicherter Monitor · derzeit nicht verfügbar')}</option>{/if}{#each displays as display(display.id)}<option value={display.id}>{display.name}</option>{/each}</select><p class="field-note">{$t('Die Monitorauswahl bleibt lokal. Entfernte oder ungültige Monitore verwenden nativ einen sicheren Ersatzbereich. Es werden keine Fenster oder Bildschirminhalte erkannt.')}</p>
          <h3>{$t('Bewegungsbereich')} <HelpTip text={$t('Prozentangaben beziehen sich auf die gemeinsame Bühne: links oben ist 0 % / 0 %, rechts unten 100 % / 100 %. Der native Ausgang berücksichtigt Monitorursprung, Skalierung und Figurengröße.')}/></h3>
          <div class="coordinate-grid">{#each [['min','x','Linker Rand'],['min','y','Oberer Rand'],['max','x','Rechter Rand'],['max','y','Unterer Rand']] as [edge,axis,label]}<label>{$t(label)} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={draft.safeRegion[edge as 'min'|'max'][axis as 'x'|'y']} required/></label>{/each}</div>
        </div><div class="preview-column"><h3>{$t('2D-Entwurfsvorschau')}</h3><div class="world-preview" role="img" aria-label={$t('Vorschau des geplanten Bewegungsbereichs mit Ankern und festen Plätzen. Keine laufende Bewegung.')}>{#if parsedDraft}<div class="safe-region" style:left={`${parsedDraft.config.safeRegion.min.x/10000}%`} style:top={`${parsedDraft.config.safeRegion.min.y/10000}%`} style:width={`${(parsedDraft.config.safeRegion.max.x-parsedDraft.config.safeRegion.min.x)/10000}%`} style:height={`${(parsedDraft.config.safeRegion.max.y-parsedDraft.config.safeRegion.min.y)/10000}%`}></div>{#each parsedDraft.config.anchors as anchor,index(anchor.id)}<span class="anchor-marker" style:left={`${anchor.position.x/10000}%`} style:top={`${anchor.position.y/10000}%`} title={`${$t(anchorKinds[anchor.kind])} ${index+1}`}>{index+1}</span>{#each anchor.slots as slot(slot.id)}<span class="slot-marker" style:left={`${slot.position.x/10000}%`} style:top={`${slot.position.y/10000}%`}></span>{/each}{/each}{:else}<p>{$t('Für die Vorschau gültige Prozentangaben und einen geordneten Bewegungsbereich eingeben.')}</p>{/if}</div><p class="field-note">{$t('Nummern markieren Anker, kleine Punkte deren exklusive feste Plätze. Alle Angaben lassen sich per Tastatur bearbeiten.')}</p></div></div>
        <div class="anchors-heading"><h3>{$t('Anker und feste Plätze')}</h3><span>{$t('{count} von 128 Plätzen',{count:number(totalSlots)})}</span></div>
        {#each draft.anchors as anchor,index(anchor.id)}<article class="anchor-editor"><div class="anchor-title"><strong>{$t('Anker {number}',{number:index+1})}</strong><button class="icon-button" type="button" aria-label={$t('Anker {number} entfernen',{number:index+1})} onclick={()=>{if(draft)draft.anchors=draft.anchors.filter(item=>item.id!==anchor.id);}}><Trash2 size={15}/></button></div><div class="coordinate-grid anchor-fields"><label>{$t('Art')}<select bind:value={anchor.kind}>{#each Object.entries(anchorKinds) as [kind,label]}<option value={kind}>{$t(label)}</option>{/each}</select></label><label>{$t('Anker X')} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={anchor.position.x} required/></label><label>{$t('Anker Y')} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={anchor.position.y} required/></label></div>
          {#each anchor.slots as slot,slotIndex(slot.id)}<div class="slot-editor"><strong>{$t('Platz {number}',{number:slotIndex+1})}</strong><label>{$t('Platz X')} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={slot.position.x} aria-label={$t('Anker {anchor}, Platz {slot}, X in Prozent',{anchor:index+1,slot:slotIndex+1})} required/></label><label>{$t('Platz Y')} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={slot.position.y} aria-label={$t('Anker {anchor}, Platz {slot}, Y in Prozent',{anchor:index+1,slot:slotIndex+1})} required/></label><button class="icon-button" type="button" aria-label={$t('Platz {number} entfernen',{number:slotIndex+1})} disabled={anchor.slots.length<=1} onclick={()=>anchor.slots=anchor.slots.filter(item=>item.id!==slot.id)}><Trash2 size={14}/></button></div>{/each}
          <button class="text-link" type="button" onclick={()=>addSlot(index)} disabled={totalSlots>=128}><Plus size={14}/>{$t('Festen Platz hinzufügen')}</button></article>{/each}
        <div class="anchor-add">{#each Object.entries(anchorKinds) as [kind,label]}<button class="button secondary small" type="button" onclick={()=>addAnchor(kind as AnchorKind)} disabled={draft.anchors.length>=64||totalSlots>=128}><Plus size={14}/>{$t(label)}</button>{/each}</div>
        <p class="field-note">{$t('Jeder feste Platz kann nativ nur einer passenden Aktion gehören. Reservierungen sind vorübergehend; Speichern erzeugt keine Reservierung und ein Neustart stellt keine Belegung wieder her.')}</p>
        <div class="world-actions"><button class="button secondary" type="submit" disabled={!configDirty||!parsedDraft||settings?.availability==='unavailable'&&!settings.record}>{$t('Bühnenkonfiguration speichern')}</button><button class="text-link" type="button" onclick={discard} disabled={!dirty}>{$t('Bühnenentwurf verwerfen')}</button><span role="status">{dirty?$t('Nicht gespeicherter Bühnenentwurf'):$t('Bühnenentwurf gespeichert')}</span></div>
      </fieldset>
    </form>
  {/if}
  {#if snapshot}<div class="confirmed-world"><h3>{$t('Nativ bestätigte Bühne')}</h3><p>{$t('Konfiguration {revision} · Stand {sequence}',{revision:number(snapshot.configRevision),sequence:number(snapshot.sequence)})}</p><p class="field-note">{$t('Diese Positionen sind bestätigte Weltdaten, keine Bestätigung einer sichtbaren Bewegung oder Ankunft.')}</p>
    <dl class="actor-positions">{#each snapshot.actors as entry(entry.actorId)}<div><dt>{name(entry.actorId)}</dt><dd>{$t('X {x} % · Y {y} %',{x:canonicalToPercent(entry.position.x),y:canonicalToPercent(entry.position.y)})}</dd></div>{/each}</dl>
    {#if !snapshot.actors.length}<p class="field-note">{$t('Keine bestätigten Teilnehmerpositionen.')}</p>{/if}
    <p class="field-note">{$t('{count} Plätze sind derzeit nativ belegt.',{count:number(snapshot.occupied.length)})}</p>
    <form class="placement-form" onsubmit={event=>{event.preventDefault();void place();}} oninput={()=>placementEdited=true}><fieldset disabled={pending||disabled||configDirty||unconfirmed}><legend>{$t('Bestätigte Position ausdrücklich setzen')}</legend><label for={`${uid}-actor`}>{$t('Teilnehmer auswählen')}</label><select id={`${uid}-actor`} value={actor} onchange={event=>selectActor(event.currentTarget.value)}><option value="">{$t('Teilnehmer auswählen')}</option>{#each snapshot.actors as entry(entry.actorId)}<option value={entry.actorId}>{name(entry.actorId)}</option>{/each}</select><div class="coordinate-grid"><label>{$t('Figur X')} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={position.x} disabled={!actor}/></label><label>{$t('Figur Y')} (%)<input type="text" inputmode="decimal" maxlength="9" bind:value={position.y} disabled={!actor}/></label></div><button class="button secondary small" type="submit" disabled={!actor||!placementEdited}>{$t('Position nativ setzen')}</button></fieldset></form>
  </div>{/if}
  {#if message}<p class="world-message" role="status">{$t(message)}</p>{/if}
</section>

<style>
.world-panel{margin:20px 0}.world-heading,.anchors-heading,.anchor-title,.world-actions,.anchor-add{display:flex;align-items:center;justify-content:space-between;gap:14px;flex-wrap:wrap}.world-heading{align-items:flex-start}.world-heading button{flex-shrink:0}.world-scope{padding:12px 14px;background:var(--input-bg);border-left:3px solid var(--accent);font-size:12px}.field-note,.section-heading p{color:var(--muted);font-size:12px;line-height:1.7}.field-note{margin:10px 0}fieldset{border:0;padding:0;margin:22px 0 0;min-width:0}legend{font-weight:700;margin-bottom:10px}h3{margin:18px 0 10px}.world-grid{display:grid;grid-template-columns:minmax(0,1fr) minmax(240px,.8fr);gap:28px}.world-members{display:grid;gap:8px;margin:12px 0 20px}.world-members label{display:flex;align-items:center;gap:10px;margin:0}.world-members input{width:auto;margin:0}.world-members span{overflow-wrap:anywhere}.coordinate-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.coordinate-grid label{margin:0}.world-preview{position:relative;aspect-ratio:4/3;background:var(--input-bg);border:1px solid var(--line);overflow:hidden;margin-top:12px}.world-preview>p{padding:20px;color:var(--muted);font-size:12px}.safe-region{position:absolute;border:1px dashed var(--accent);background:#91cfc40a}.anchor-marker,.slot-marker{position:absolute;transform:translate(-50%,-50%)}.anchor-marker{display:grid;place-items:center;width:25px;height:25px;border:1px solid var(--accent);background:var(--panel);color:var(--accent);border-radius:50%;font-size:11px;z-index:1}.slot-marker{width:7px;height:7px;background:var(--warning);border:1px solid var(--panel);z-index:2}.anchors-heading{border-top:1px solid var(--line);margin-top:18px}.anchors-heading span{font-size:12px;color:var(--muted)}.anchor-editor{border-bottom:1px solid var(--line);padding:14px 0 18px}.anchor-fields{grid-template-columns:minmax(150px,1.2fr) minmax(0,1fr) minmax(0,1fr)}.slot-editor{display:grid;grid-template-columns:90px minmax(0,1fr) minmax(0,1fr) 32px;align-items:end;gap:12px;margin:12px 0}.slot-editor label{margin:0}.slot-editor strong{font-size:12px;padding-bottom:12px}.anchor-add{justify-content:flex-start;margin-top:18px}.world-actions{justify-content:flex-start;margin-top:20px}.world-actions span{color:var(--muted);font-size:12px}.confirmed-world{border-top:1px solid var(--line);margin-top:24px}.actor-positions{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px}.actor-positions>div{background:var(--input-bg);padding:10px 12px}.actor-positions dt{font-weight:600;overflow-wrap:anywhere}.actor-positions dd{margin:6px 0 0;color:var(--muted);font-size:12px}.placement-form{max-width:520px}.placement-form button{margin-top:14px}.world-error,.world-warning,.world-message{font-size:12px;padding:12px 14px;margin:14px 0;border-left:3px solid var(--warning);background:var(--input-bg);overflow-wrap:anywhere}.world-error{color:var(--danger);border-color:var(--danger)}.world-message{border-color:var(--accent)}.empty-world{display:grid;justify-items:start;gap:12px;margin:22px 0}.empty-world p{max-width:70ch;color:var(--muted);font-size:12px}@media(max-width:800px){.world-grid{grid-template-columns:minmax(0,1fr)}.preview-column{max-width:520px}.anchor-fields{grid-template-columns:repeat(2,minmax(0,1fr))}.anchor-fields>label:first-child{grid-column:1/-1}}@media(max-width:460px){.slot-editor{grid-template-columns:minmax(0,1fr) minmax(0,1fr) 28px}.slot-editor strong{grid-column:1/-1;padding:0}.actor-positions{grid-template-columns:minmax(0,1fr)}}
</style>
