import {describe,expect,it} from 'vitest';
import {currentRuntimeIntent,intentTargetLabel,parseRuntimeIntent} from './life-intent';
import {parseExperience} from './experience';
const action=()=>({version:1,botId:'nova',sessionId:'session_nova',source:'action',activity:'rest',phase:'execution',target:{type:'here'},reason:null,actorIds:['nova']});
const social=()=>({...action(),source:'social',activity:'play',phase:'invitation',target:null,actorIds:['nova','friend']});
describe('current native intention',()=>{
  it('keeps real Here fallback distinct from actual station use and invitation',()=>{
    expect(intentTargetLabel(parseRuntimeIntent(action(),'nova')!.target!)).toBe('Hier, ohne Objektplatz');
    expect(intentTargetLabel(parseRuntimeIntent({...action(),target:{type:'slot',anchorKind:'play'}},'nova')!.target!)).toBe('Ziel: Spielplatz');
    expect(parseRuntimeIntent(social(),'nova')).toMatchObject({source:'social',phase:'invitation',actorIds:['nova','friend'],target:null});
  });
  it('requires exact bot, bounded unique participants and closed source state',()=>{
    for(const patch of [{botId:'other'},{sessionId:'../PRIVATE'},{actorIds:['other']},{actorIds:['nova','nova']},
      {actorIds:['nova','friend','third']},{source:'model'},{activity:'shell'},{phase:'rewarded'},
      {reason:'PRIVATE'},{reason:'operator'},{target:{type:'slot',anchorKind:'private'}}])expect(()=>parseRuntimeIntent({...action(),...patch},'nova')).toThrow();
    expect(()=>parseRuntimeIntent({...social(),phase:'failed'},'nova')).toThrow();
    expect(()=>parseRuntimeIntent({...social(),actorIds:['nova']},'nova')).toThrow();
    expect(()=>parseRuntimeIntent({...action(),phase:'invitation'},'nova')).toThrow();
  });
  it('retains concrete host failure and cancellation without asserting completion',()=>{
    expect(parseRuntimeIntent({...action(),phase:'failed',reason:'output_lost'},'nova')).toMatchObject({phase:'failed',reason:'output_lost'});
    expect(parseRuntimeIntent({...social(),phase:'cancelled',reason:'timeout'},'nova')).toMatchObject({phase:'cancelled',reason:'timeout'});
    expect(parseRuntimeIntent({...action(),phase:'cancelling'},'nova')!.phase).toBe('cancelling');
  });
  it('projects no persona, coordinates, grants, chat or raw error strings',()=>{
    const value={...action(),persona:'PRIVATE',outputProof:'PRIVATE',rawError:'PRIVATE',chat:'PRIVATE',target:{type:'point',x:42,path:'PRIVATE'}};
    const result=parseRuntimeIntent(value,'nova');
    expect(JSON.stringify(result)).not.toContain('PRIVATE');expect(result?.target).toEqual({type:'point'});
    expect(value.persona).toBe('PRIVATE');
  });
  it('rejects accessors without invoking them, including actor-array accessors',()=>{
    let calls=0;const value={...action(),get persona(){calls++;return 'PRIVATE';}};
    expect(()=>parseRuntimeIntent(value,'nova')).toThrow();
    const actors=['nova'];Object.defineProperty(actors,'0',{get(){calls++;return 'nova';}});
    expect(()=>parseRuntimeIntent({...action(),actorIds:actors},'nova')).toThrow();expect(calls).toBe(0);
  });
  it('hides stale runtime intention after Stop, central pause or bot selection',()=>{
    const intent=parseRuntimeIntent(action(),'nova');
    expect(currentRuntimeIntent(intent,'nova',true,false)).toBe(intent);
    expect(currentRuntimeIntent(intent,'nova',false,false)).toBeNull();
    expect(currentRuntimeIntent(intent,'nova',true,true)).toBeNull();
    expect(currentRuntimeIntent(intent,'friend',true,false)).toBeNull();
  });
  it('never promotes an album fact into an absent intention',()=>{
    const snapshot={version:1,botId:'nova',settings:{version:1,revision:0,preferences:{version:1,preset:'focus',volume:35,initiativeCooldownMs:60000,movementRadius:250,allowedAnimations:['idle'],intentVisible:true,desktopAnchors:false,cursorPlay:false,streamImpulses:false,impulseParticipants:[]}},paused:false,
      facts:[{id:'fact_1',actorIds:['nova'],kind:'rest',phase:'completed',atMs:1,sourceEventIds:['event_1'],shared:false}],
      desktopCapabilities:{externalWindows:false,cursor:false,reason:'no_verified_source'},impulses:{queued:0,accepted:0,dropped:0}};
    const history=parseExperience(snapshot,'nova');expect(history.intent).toBeNull();expect(history.facts).toHaveLength(1);
    const live=parseExperience({...snapshot,facts:[],intent:social()},'nova');expect(live.intent?.phase).toBe('invitation');expect(live.facts).toEqual([]);
  });
});
