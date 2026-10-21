import {describe,it,expect} from 'vitest';
import {parseDevelopment,parseSocial,groupSocialTrace,routineMinute,routineTime} from './social-experience';
const development=()=>({version:1,traits:{initiative:500,sociability:500,patience:500,restPreference:500},interests:{play:500,explore:500,company:500},hobbies:['play','explore','company'].map(kind=>({kind,experience:0,variation:0})),goal:null,routines:[]});
const social=()=>({version:1,botId:'nova',revision:1,relationships:[{otherId:'blue',familiarity:500,affection:-500}],development:development(),scenes:[],memories:[{eventId:'event',kind:'play',outcome:'completed',participants:['nova','blue'],atMs:100,origin:'own',shared:false}]});
const objectPreference=()=>({object:{stageId:'desk',objectId:'plant',kind:'play',contentRef:null as null|{packageId:string;packageVersion:number;templateId:string}},affinity:375,successfulUses:3});
describe('social projection boundaries',()=>{
 it('preserves negative affection without casting it into a high positive value',()=>expect(parseSocial(social(),'nova').relationships[0].affection).toBe(-500));
 it('rejects other bot memories and strips private history fields',()=>{expect(()=>parseSocial(social(),'blue')).toThrow();const s=social();expect(()=>parseSocial({...s,memories:[{...s.memories[0],participants:['other','blue']}]},'nova')).toThrow();expect(JSON.stringify(parseSocial({...s,privateHistory:'PRIVATE'},'nova'))).not.toContain('PRIVATE');});
 it('requires earned variation to match bounded actual experience',()=>{const d=development();d.hobbies[0].variation=3;expect(()=>parseDevelopment(d)).toThrow();d.hobbies[0].experience=5000;expect(parseDevelopment(d).hobbies[0].variation).toBe(3);});
 it('validates soft wrapping routines and forbids all-day ambiguous boundaries',()=>{expect(parseDevelopment({...development(),routines:[{startMinute:1380,endMinute:60,kind:'company',weight:100}]}).routines).toHaveLength(1);expect(()=>parseDevelopment({...development(),routines:[{startMinute:60,endMinute:60,kind:'play',weight:100}]})).toThrow();});
 it('validates immutable goal targets and progress',()=>{const goal={id:'goal',kind:'play',target:5,progress:4,status:'active'};expect(parseDevelopment({...development(),goal}).goal?.progress).toBe(4);expect(()=>parseDevelopment({...development(),goal:{...goal,status:'achieved'}})).toThrow();expect(()=>parseDevelopment({...development(),goal:{...goal,progress:6}})).toThrow();});
 it('converts only explicit minute-formatted times',()=>{for(const minute of [0,59,60,1439])expect(routineMinute(routineTime(minute))).toBe(minute);for(const s of ['24:00','9:00','12:60','-1:30'])expect(()=>routineMinute(s)).toThrow();});
 it('shows only scoped bounded actual trace order and retains invitation time without inventing phase timestamps',()=>{
   const scene={id:'scene_1',kind:'play',phase:'invitation',participants:['nova','blue'],inviterId:'nova',atMs:100,deadlineMs:5000,reason:null};
   const trace=['invitation','accepted','resources_reserved','approach','execution','completed'].map(phase=>({...scene,phase}));
   const parsed=parseSocial({...social(),trace},'nova');
   expect(groupSocialTrace(parsed.trace)[0].phases).toEqual(trace.map(s=>s.phase));expect(parsed.trace.every(s=>s.atMs===100)).toBe(true);
   expect(parsed.trace[0]).not.toHaveProperty('phaseAtMs');expect(parseSocial(social(),'nova').trace).toEqual([]);
 });
 it('rejects duplicated phases, actor-swapped trace identities and a trace outside the current bot scope',()=>{
   const scene={id:'scene_1',kind:'rest',phase:'accepted',participants:['nova','blue'],inviterId:'nova',atMs:100,deadlineMs:5000,reason:null};
   for(const trace of [[scene,scene],[scene,{...scene,phase:'execution',participants:['nova','third']}],[{...scene,participants:['blue','third'],inviterId:'blue'}],Array(257).fill(scene)])expect(()=>parseSocial({...social(),trace},'nova')).toThrow();
 });
 it('keeps old projections valid and accepts bounded local preferences without exposing unrelated private data',()=>{
   expect(parseSocial(social(),'nova').objectPreferences).toEqual([]);
   const reference={packageId:'garden',packageVersion:2,templateId:'garden:plant',privateNotes:'PRIVATE'};
   const preference={...objectPreference(),object:{...objectPreference().object,contentRef:reference,privateHistory:'PRIVATE'},shared:true,dialogue:'PRIVATE',action:'run'};
   const parsed=parseSocial({...social(),objectPreferences:[preference],peerObjectPreferences:[preference]},'nova');
   expect(parsed.objectPreferences).toEqual([{...objectPreference(),object:{...objectPreference().object,contentRef:{packageId:'garden',packageVersion:2,templateId:'garden:plant'}}}]);
   expect(parsed).not.toHaveProperty('peerObjectPreferences');expect(JSON.stringify(parsed)).not.toContain('PRIVATE');
   expect(parsed.objectPreferences[0]).not.toHaveProperty('shared');expect(parsed.objectPreferences[0]).not.toHaveProperty('action');
   reference.packageVersion=3;expect(parsed.objectPreferences[0].object.contentRef?.packageVersion).toBe(2);
   expect(()=>parseSocial({...social(),objectPreferences:[preference]},'blue')).toThrow();
 });
 it('distinguishes exact object identities across stages, kinds and package versions while rejecting duplicate identities',()=>{
   const first=objectPreference(),reference={packageId:'garden',packageVersion:1,templateId:'garden:plant'};
   const preferences=[first,{...first,object:{...first.object,stageId:'other_stage'}},{...first,object:{...first.object,kind:'rest'}},{...first,object:{...first.object,contentRef:reference}},{...first,object:{...first.object,contentRef:{...reference,packageVersion:2}}}];
   expect(parseSocial({...social(),objectPreferences:preferences},'nova').objectPreferences).toHaveLength(5);
   expect(()=>parseSocial({...social(),objectPreferences:[first,{...first,affinity:999,successfulUses:4}]},'nova')).toThrow();
   expect(()=>parseSocial({...social(),objectPreferences:[preferences[3],{...preferences[3],object:{...preferences[3].object,contentRef:{templateId:'garden:plant',packageVersion:1,packageId:'garden'}}}]},'nova')).toThrow();
 });
 it('rejects malformed, unbounded and unearned object preference projections',()=>{
   const preference=objectPreference();
   const invalid=[null,{},[null],Array.from({length:65},(_,i)=>({...preference,object:{...preference.object,objectId:`object_${i}`}}))];
   for(const objectPreferences of invalid)expect(()=>parseSocial({...social(),objectPreferences},'nova')).toThrow();
   for(const patch of [{affinity:-1},{affinity:1001},{affinity:1.5},{successfulUses:0},{successfulUses:10001},{successfulUses:1.5}])expect(()=>parseSocial({...social(),objectPreferences:[{...preference,...patch}]},'nova')).toThrow();
   for(const patch of [{stageId:'../desk'},{objectId:''},{kind:'cursor'},{contentRef:undefined},{contentRef:{packageId:'garden',packageVersion:0,templateId:'garden:plant'}},{contentRef:{packageId:'garden',packageVersion:1,templateId:'other:plant'}}])expect(()=>parseSocial({...social(),objectPreferences:[{...preference,object:{...preference.object,...patch}}]},'nova')).toThrow();
   const maximum=Array.from({length:64},(_,i)=>({...preference,affinity:i===0?0:1000,successfulUses:i===0?1:10000,object:{...preference.object,objectId:`object_${i}`}}));
   expect(parseSocial({...social(),objectPreferences:maximum},'nova').objectPreferences).toHaveLength(64);
 });

});
