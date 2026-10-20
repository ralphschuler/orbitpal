import { describe,expect,it,vi } from 'vitest';
import { readFileSync } from 'node:fs';
import catalogue from './locales/world-state.json';
import { canonicalToPercent,draftWorld,newWorldDraft,parseWorldConfig,parseWorldSettings,parseWorldState,percentToCanonical,placeWorld,saveWorld,worldDraft,worldDraftChanged } from './world-state';
const instanceId='11111111-1111-1111-1111-111111111111';
const config=()=>({schemaVersion:1,stageId:'stage_1',revision:1,safeRegion:{min:{x:50000,y:50000},max:{x:950000,y:950000}},members:['_nova','james'],anchors:[{id:'rest',kind:'rest',position:{x:200000,y:700000},slots:[{id:'rest_1',position:{x:200000,y:700000}}]}]});
const settings=()=>({version:1,instanceId,availability:'ready',reason:null,record:{config:config(),outputBinding:{monitorId:null}}});
const snapshot=()=>({version:1,instanceId,availability:'ready',reason:null,snapshot:{schemaVersion:1,stageId:'stage_1',configRevision:1,sequence:4,safeRegion:config().safeRegion,anchors:config().anchors,actors:[{actorId:'_nova',position:{x:500000,y:500000},life:null},{actorId:'james',position:{x:600000,y:500000},life:null}],occupied:[{slotId:'rest_1',actorId:'_nova'}]}});
describe('Shared World editor boundary',()=>{
  it('converts understandable percentages exactly without DPI/physical coordinates or floating point drift',()=>{
    for(const [text,canonical] of [['0',0],['100',1000000],['50',500000],['0.0001',1],['99.9999',999999],['12,3456',123456]] as const){expect(percentToCanonical(text)).toBe(canonical);expect(percentToCanonical(canonicalToPercent(canonical))).toBe(canonical);}
    for(const bad of ['',' ','-1','100.0001','101','1e2','Infinity','NaN','1.00001','50%','1.','0x10'])expect(()=>percentToCanonical(bad)).toThrow();
    for(const bad of [NaN,Infinity,-1,1000001,.1])expect(()=>canonicalToPercent(bad)).toThrow();
  });
  it('roundtrips configuration drafts and preserves invalid edits without a fallback or silent save',()=>{
    const record=parseWorldSettings(settings()).record!;const draft=worldDraft(record);expect(draftWorld(draft)).toEqual(record);expect(worldDraftChanged(draft,record)).toBe(false);
    draft.safeRegion.min.x='invalid';expect(worldDraftChanged(draft,record)).toBe(true);expect(()=>draftWorld(draft)).toThrow();expect(draft.safeRegion.min.x).toBe('invalid');
    expect(newWorldDraft('stage_new').members).toEqual([]);expect(newWorldDraft('stage_new').revision).toBe(0);
  });
  it('rejects duplicate/oversized IDs, geometry, participants, resources, schemas and out-of-region targets',()=>{
    for(const change of [{schemaVersion:2},{stageId:'../stage'},{members:['_nova','_nova']},{members:Array(65).fill('_nova')},{revision:Infinity},{safeRegion:{min:{x:50,y:50},max:{x:50,y:100}}}])expect(()=>parseWorldConfig({...config(),...change})).toThrow();
    const c=config();c.anchors[0].slots.push({...c.anchors[0].slots[0]});expect(()=>parseWorldConfig(c)).toThrow();
    const outside=config();outside.anchors[0].position.x=0;expect(()=>parseWorldConfig(outside)).toThrow();
    const duplicate=config();duplicate.anchors.push(duplicate.anchors[0]);expect(()=>parseWorldConfig(duplicate)).toThrow();
  });
  it('allows only explicit initial creation and a typed missing-member repair, never a corrupt reset',()=>{
    expect(parseWorldSettings({version:1,instanceId,availability:'not_initialized',reason:null,record:null}).record).toBeNull();
    expect(parseWorldSettings({...settings(),availability:'unavailable',reason:'unknown_actor'}).record?.config.members).toEqual(['_nova','james']);
    for(const change of [{availability:'ready',record:null},{availability:'unavailable',reason:'corrupt_config'},{availability:'unavailable',reason:'future_reason'},{availability:'not_initialized'},{instanceId:'not-an-epoch'}])expect(()=>parseWorldSettings({...settings(),...change})).toThrow();
    expect(parseWorldSettings({...settings(),availability:'unavailable',reason:'corrupt_config',record:null}).record).toBeNull();
  });
  it('bounds private monitor bindings by UTF-8 bytes and all Unicode controls',()=>{
    const record=settings();
    for(const monitorId of ['', 'x'.repeat(513),'é'.repeat(257),'display\u0085bad'])expect(()=>parseWorldSettings({...record,record:{...record.record,outputBinding:{monitorId}}})).toThrow();
    expect(parseWorldSettings({...record,record:{...record.record,outputBinding:{monitorId:'é'.repeat(256)}}}).record?.outputBinding.monitorId).toHaveLength(256);
  });
  it('projects only public actor/occupancy data and rejects wrong actor/slot/duplicate ownership',()=>{
    const raw=snapshot();const parsed=parseWorldState({...raw,monitorId:'PRIVATE',leases:'PRIVATE',snapshot:{...raw.snapshot,credentials:'PRIVATE',actors:raw.snapshot.actors.map(actor=>({...actor,prompt:'PRIVATE'}))}});expect(JSON.stringify(parsed)).not.toContain('PRIVATE');
    expect(parsed.snapshot?.occupied).toEqual([{slotId:'rest_1',actorId:'_nova'}]);
    for(const occupied of [[{slotId:'missing',actorId:'_nova'}],[{slotId:'rest_1',actorId:'other'}],[raw.snapshot.occupied[0],raw.snapshot.occupied[0]]])expect(()=>parseWorldState({...raw,snapshot:{...raw.snapshot,occupied}})).toThrow();
    expect(()=>parseWorldState({...raw,snapshot:{...raw.snapshot,actors:[...raw.snapshot.actors,raw.snapshot.actors[0]]}})).toThrow();
  });
  it('saves only config/CAS/private binding and places only a checked canonical point, with zero activation/grants',async()=>{
    const invoke=vi.fn(async(command:string)=>{if(command==='save_world_config'){const value=settings();value.record.config.revision=2;return value;}const value=snapshot();value.snapshot.actors[0].position={x:250000,y:750000};return value;});
    const draft=worldDraft(parseWorldSettings(settings()).record!);await saveWorld(invoke,draft,1);expect(invoke).toHaveBeenCalledWith('save_world_config',{expectedRevision:1,...draftWorld(draft)});
    await placeWorld(invoke,1,'_nova',{x:'25',y:'75'});expect(invoke).toHaveBeenCalledWith('place_world_actor',{expectedRevision:1,actorId:'_nova',position:{x:250000,y:750000}});
    expect(invoke.mock.calls.map(call=>call[0])).toEqual(['save_world_config','place_world_actor']);
    await expect(saveWorld(invoke,draft,0)).rejects.toThrow();await expect(placeWorld(invoke,1,'../bot',{x:'50',y:'50'})).rejects.toThrow();expect(invoke).toHaveBeenCalledTimes(2);
    for(const reply of [settings(),{...settings(),record:null,availability:'unavailable',reason:'database'}])await expect(saveWorld(async()=>reply,draft,1)).rejects.toThrow();
    await expect(placeWorld(async()=>snapshot(),1,'_nova',{x:'25',y:'75'})).rejects.toThrow();
  });
});


describe('Shared stage UI scope and localisation',()=>{
  it('covers German source labels in English, French and Spanish without changing placeholders',()=>{
    const component=readFileSync(new URL('../components/WorldStatePanel.svelte',import.meta.url),'utf8');
    const literals=[...component.matchAll(/\$t\('([^']+)'/g)].map(match=>match[1]);
    for(const language of ['en','fr','es'] as const){const messages=catalogue[language] as Record<string,string>;for(const key of literals)expect(messages[key]?.trim(),`${language}: ${key}`).toBeTruthy();for(const[source,translated]of Object.entries(messages))expect([...translated.matchAll(/\{(\w+)\}/g)].map(match=>match[1]).sort()).toEqual([...source.matchAll(/\{(\w+)\}/g)].map(match=>match[1]).sort());}
  });
  it('keeps one shared draft mounted across navigation and includes it in native/updater unsaved guards',()=>{
    const app=readFileSync(new URL('../App.svelte',import.meta.url),'utf8');
    expect(app).toContain("hidden={page!=='autopilot'}><WorldStatePanel");expect(app).toContain('extensionDirty||worldDirty');expect(app).toContain('saving:saving||worldBusy');expect(app).toContain('if(active&&worldDirty)');
    const component=readFileSync(new URL('../components/WorldStatePanel.svelte',import.meta.url),'utf8');expect(component).not.toMatch(/call\(['"](?:set_life_mode|set_autopilot|.*capture|.*permission)/);expect(component).toContain('confirmDiscard(');
  });
});
