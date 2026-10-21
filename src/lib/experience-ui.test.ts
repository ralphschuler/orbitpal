import {describe,it,expect} from 'vitest';
import {readFileSync,readdirSync} from 'node:fs';
import {parse} from 'svelte/compiler';
import experience from './locales/experience.json';
import {anchorKinds,draftWorld,newWorldDraft,parseWorldConfig} from './world-state';
import {activityLabels} from './life-runtime';
const read=(path:string)=>readFileSync(new URL(path,import.meta.url),'utf8');
describe('Studio experience integration',()=>{
 it('keeps the visual stage before settings and preserves per-bot drafts in native guards',()=>{const app=read('../App.svelte');expect(app.indexOf('<WorldStatePanel')).toBeLessThan(app.indexOf('<ExperiencePanel'));expect(app).toContain("hidden={page!=='autopilot'}>{#key profile.id}");expect(app).toContain('apiAccessDirty||lifePreferencesDirty');expect(app).toContain('apiAccessBusy||lifePreferencesBusy');expect(app).toContain('if(lifePreferencesDirty||lifePreferencesBusy)');expect(app).toContain('experiencePanel?.discardDraft()');});
 it('retains snack as an ordinary visual resource and typed native activity',()=>{const draft=newWorldDraft('scene');draft.anchors.push({id:'snacks',kind:'snack',position:{x:'50',y:'50'},slots:[{id:'snack_seat',position:{x:'50',y:'50'}}]});expect(parseWorldConfig(draftWorld(draft).config).anchors[0].kind).toBe('snack');expect(anchorKinds.snack).toBe('Snackplatz');expect(activityLabels.snack).toBe('Snackpause');expect(activityLabels.socialize).toBe('Gemeinsame Zeit');const editor=read('../components/WorldSceneEditor.svelte');expect(editor).toContain("kind === 'snack'}<Cookie");expect(editor).not.toMatch(/type="number"/);});
 it('localizes literal UI labels and preserves every placeholder in all four languages',()=>{const catalogues=readdirSync(new URL('./locales/',import.meta.url)).filter(f=>f.endsWith('.json')).map(f=>JSON.parse(read(`./locales/${f}`)));for(const language of ['en','fr','es'] as const){const messages=Object.assign({},...catalogues.map(c=>c[language]));for(const file of ['ExperiencePanel','SocialPanel','ContentPacksPanel','CarePreferencesPanel','NativeInteractionControl','SocialDialoguePanel','RenderQualityControl']){const source=read(`../components/${file}.svelte`);for(const [,key]of source.matchAll(/\$t\('([^']+)'/g))expect(messages[key]?.trim(),`${language}: ${key}`).toBeTruthy();}for(const[key,value]of Object.entries(experience[language]))expect([...value.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort(),key).toEqual([...key.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort());}});
 it('offers no fact insertion, provider activation, foreign input or permission bypass from panels',()=>{for(const file of ['ExperiencePanel','SocialPanel','ContentPacksPanel','CarePreferencesPanel','NativeInteractionControl','SocialDialoguePanel','RenderQualityControl']){const source=read(`../components/${file}.svelte`);expect(source).not.toMatch(/call\(['"](?:record_experience_fact|set_life_mode|set_autopilot|.*capture|.*permission|connect_platform|.*mouse|.*keyboard)/);}});
 it('offers confirmed object preference recovery before any snapshot loads and scopes errors to the current bot request',()=>{
   const source=read('../components/SocialPanel.svelte'),ast=parse(source,{modern:true});
   const section=ast.fragment.nodes.find(node=>node.type==='RegularElement'&&node.name==='section');
   if(!section||section.type!=='RegularElement')throw Error('social section missing');
   const blocks=section.fragment.nodes.filter(node=>node.type==='IfBlock');
   const condition=(block:(typeof blocks)[number])=>{
     const test=block.test;
     if(test.type==='Identifier')return test.name;
     if(test.type==='LogicalExpression'&&test.operator==='&&'&&test.left.type==='Identifier'&&test.right.type==='Identifier')return `${test.left.name}&&${test.right.name}`;
     return '';
   };
   expect(blocks.some(block=>condition(block)==='projection&&draft')).toBe(true);
   const recovery=blocks.find(block=>condition(block)==='objectPreferencesRecovery&&native');
   const confirmation=blocks.find(block=>condition(block)==='confirmReset');
   expect(recovery).toBeDefined();expect(confirmation).toBeDefined();
   const recoveryUi=source.slice(recovery!.start,recovery!.end);
   expect(recoveryUi).toContain("onclick={()=>confirmReset='objects'}");expect(recoveryUi).not.toContain('action(');
   expect(source.slice(confirmation!.start,confirmation!.end)).toContain('action(resetCommands[confirmReset])');
   expect(source).toContain("objects:'reset_social_object_preferences'");
   expect(source).toContain("if(alive&&token===epoch&&id===botId){objectPreferencesRecovery=String(cause)==='social object preferences unavailable'");
   expect(source).toContain('confirmReset=null;objectPreferencesRecovery=false;void refresh()');
   expect(source).toContain('projection=value;objectPreferencesRecovery=false');
 });
});
