import {describe,it,expect} from 'vitest';
import example from '../../public/content-packs/desk-garden.json';
import {activeObjectTemplates} from './content-templates';
import {parseContentRef} from './content-reference';
import {draftWorld,newWorldDraft,worldDraft} from './world-state';
import {addEditorObject,cloneEditorModel,editorActors,moveEditorTarget,migrateEditorObject} from './world-editor';
const ref=()=>({packageId:'desk-garden',packageVersion:1,templateId:'desk-garden:seed'});
const pack=(active=true)=>({manifest:structuredClone(example),revision:1,active});
describe('active content templates in the visual editor',()=>{
 it('offers only actual active object templates with exact package version and display data',()=>{
   expect(activeObjectTemplates([pack(false)])).toEqual([]);
   expect(activeObjectTemplates([pack()])).toEqual([{reference:ref(),name:'Seed toy',packageName:'Desk Garden',kind:'play'}]);
   expect(()=>activeObjectTemplates([pack(),pack()])).toThrow();
 });
 it('validates declarative references without treating paths, unknown versions or other namespaces as templates',()=>{
   expect(parseContentRef({...ref(),active:true,tool:'PRIVATE'})).toEqual(ref());
   for(const patch of [{packageId:'../garden'},{packageVersion:0},{packageVersion:0x1_0000_0000},{packageVersion:1.1},{templateId:'other:seed'},{templateId:'desk-garden:../seed'},{templateId:'desk-garden:'}])expect(()=>parseContentRef({...ref(),...patch})).toThrow();
 });
 it('places a real bounded object and seat using the same visual helper without mutating the source or activating anything',()=>{
   const before={draft:newWorldDraft('stage'),positions:{}},reference=ref();
   const placed=addEditorObject(before,'play','unique',reference);
   expect(before.draft.anchors).toEqual([]);expect(placed.draft.anchors).toHaveLength(1);
   const anchor=draftWorld(placed.draft).config.anchors[0];
   expect(anchor).toMatchObject({id:'anchor_unique',kind:'play',contentRef:reference,slots:[{id:'slot_unique'}]});
   for(const point of [anchor.position,...anchor.slots.map(s=>s.position)]){expect(point.x).toBeGreaterThanOrEqual(50_000);expect(point.y).toBeLessThanOrEqual(950_000);}
   reference.templateId='desk-garden:changed';expect(anchor.contentRef).toEqual(ref());
   expect(anchor).not.toHaveProperty('active');expect(anchor).not.toHaveProperty('effect');
 });
 it('roundtrips unavailable saved references and moves the entire object without rewriting provenance',()=>{
   const initial=addEditorObject({draft:newWorldDraft('stage'),positions:{}},'play','unique',ref());
   const record=draftWorld(initial.draft);expect(draftWorld(worldDraft(record))).toEqual(record);
   expect(activeObjectTemplates([pack(false)])).toEqual([]);
   const moved=moveEditorTarget(initial,{kind:'anchor',id:'anchor_unique'},{x:700_000,y:500_000},editorActors(initial.draft,undefined,initial.positions));
   expect(draftWorld(moved.draft).config.anchors[0].contentRef).toEqual(ref());
   const clone=cloneEditorModel(moved);clone.draft.anchors[0].contentRef!.packageVersion=2;
   expect(moved.draft.anchors[0].contentRef).toEqual(ref());
 });
 it('keeps legacy base objects free of package references and preserves capacity',()=>{
   let model={draft:newWorldDraft('stage'),positions:{}};
   for(let n=0;n<64;n++)model=addEditorObject(model,'rest',`seat_${n}`);
   expect(model.draft.anchors).toHaveLength(64);expect(addEditorObject(model,'play','overflow',ref())).toBe(model);
   expect(model.draft.anchors.every(a=>!Object.hasOwn(a,'contentRef'))).toBe(true);
 });
 it('rejects malformed or executable tray data rather than converting it into a built-in object',()=>{
   for(const change of [{kind:'script'},{id:'other:object'},{name:'line\nbreak'}]){const p=pack();Object.assign(p.manifest.objects[0],change);expect(()=>activeObjectTemplates([p])).toThrow();}
 });
 it('migrates an exact compatible object only through an explicit newer reference and preserves all geometry',()=>{
   const initial=addEditorObject({draft:newWorldDraft('stage'),positions:{}},'play','unique',ref());
   const next=migrateEditorObject(initial,'anchor_unique',{...ref(),packageVersion:2},'play');
   expect(next.draft.anchors[0].contentRef?.packageVersion).toBe(2);expect(initial.draft.anchors[0].contentRef?.packageVersion).toBe(1);
   expect(next.draft.anchors[0].position).toEqual(initial.draft.anchors[0].position);expect(next.draft.anchors[0].slots).toEqual(initial.draft.anchors[0].slots);
   for(const [reference,kind]of [[ref(),'play'],[{...ref(),packageVersion:2},'rest'],[{...ref(),packageId:'other',templateId:'other:seed',packageVersion:2},'play']]as const)expect(migrateEditorObject(initial,'anchor_unique',reference,kind)).toBe(initial);
 });

});
