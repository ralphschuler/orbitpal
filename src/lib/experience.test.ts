import {describe,it,expect} from 'vitest';
import {factualRecap,parseExperience,parsePreferences,parseInteraction,type ActivityFact} from './experience';
const preferences=()=>({version:1,preset:'focus',volume:35,initiativeCooldownMs:60000,movementRadius:250,allowedAnimations:['idle','walk'],intentVisible:true,desktopAnchors:false,cursorPlay:false,streamImpulses:false,impulseParticipants:[]});
const fact=(id:string,phase:ActivityFact['phase']='completed',shared=false):ActivityFact=>({id,actorIds:['nova'],kind:'rest',phase,atMs:86400000,sourceEventIds:[`source_${id}`],shared});
const snapshot=()=>({version:1,botId:'nova',settings:{version:1,revision:2,preferences:preferences()},paused:false,facts:[fact('one')],desktopCapabilities:{externalWindows:false,cursor:false,reason:'no_verified_source'},impulses:{queued:0,accepted:0,dropped:0}});
describe('experience native projections',()=>{
 it('copies only preferences without session activation or private payload',()=>{const p=parsePreferences({...preferences(),active:true,paused:true,secret:'PRIVATE'});expect(p).not.toHaveProperty('active');expect(p).not.toHaveProperty('paused');expect(JSON.stringify(p)).not.toContain('PRIVATE');});
 it('rejects malformed limits, duplicate participants and unknown animation rights',()=>{for(const patch of [{volume:101},{volume:.1},{initiativeCooldownMs:4999},{movementRadius:1001},{allowedAnimations:['shell']},{allowedAnimations:['idle','idle']},{impulseParticipants:['nova','nova']},{preset:'unrestricted'}])expect(()=>parsePreferences({...preferences(),...patch})).toThrow();});
 it('requires exact bot, native source IDs, bounded facts and capabilities',()=>{expect(parseExperience(snapshot(),'nova').facts).toHaveLength(1);expect(()=>parseExperience(snapshot(),'other')).toThrow();for(const patch of [{facts:[{...fact('one'),actorIds:['other']}]},{facts:[{...fact('one'),sourceEventIds:[]}]},{facts:[fact('one'),fact('one')]},{facts:Array(257).fill(fact('one'))},{desktopCapabilities:{cursor:'yes',externalWindows:false,reason:'no_verified_source'}}])expect(()=>parseExperience({...snapshot(),...patch},'nova')).toThrow();});
 it('never turns admission into completion or accepts a stale interaction result',()=>{const result={version:1,botId:'nova',kind:'play',status:'accepted',reason:null};expect(parseInteraction(result,'nova','play').status).toBe('accepted');expect(()=>parseInteraction(result,'other','play')).toThrow();expect(()=>parseInteraction({...result,status:'completed'},'nova','play')).toThrow();expect(()=>parseInteraction(result,'nova','home')).toThrow();});
 it('aggregates only unique completed facts, never offline empty days or running actions',()=>{const groups=factualRecap([fact('one'),fact('one'),fact('two'),fact('running','started'),fact('cancel','cancelled')]);expect(groups).toEqual([{day:1,kind:'rest',actorIds:['nova'],count:2,eventIds:['one','two']}]);expect(factualRecap([])).toEqual([]);});
 it('public preview uses explicit sharing only and does not mutate stored facts',()=>{const facts=[fact('private'),fact('shared','completed',true)];expect(factualRecap(facts,true)[0].eventIds).toEqual(['shared']);expect(facts[0].shared).toBe(false);expect(factualRecap(facts)[0].count).toBe(2);});
 it('validates closed numeric vote windows without accepting voters, chat text or extra platforms',()=>{
   const vote={platform:'twitch',rest:2,play:3,closesInMs:12000};const s=snapshot();
   const parsed=parseExperience({...s,impulses:{...s.impulses,voteWindows:[{...vote,voters:['PRIVATE'],text:'PRIVATE'}]}},'nova');
   expect(parsed.impulses.voteWindows).toEqual([vote]);expect(JSON.stringify(parsed)).not.toContain('PRIVATE');
   for(const voteWindows of [[{...vote,platform:'chat'}],[{...vote,rest:257}],[{...vote,rest:200,play:200}],[{...vote,closesInMs:0}],[vote,vote]])expect(()=>parseExperience({...s,impulses:{...s.impulses,voteWindows}},'nova')).toThrow();
 });

});
