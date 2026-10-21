import {describe,it,expect} from 'vitest';
import {lifeSceneParticipants,LIFE_SCENE_LIMIT_HINT,LIFE_SCENE_OVER_LIMIT} from './life-scene-participants';
import {draftWorld,newWorldDraft,worldDraft} from './world-state';
import {translate} from './i18n';

const members=(count:number)=>Array.from({length:count},(_,i)=>`actor_${i}`);
describe('Life scene participant capacity',()=>{
  it('allows the eighth participant but prevents a ninth and an empty start',()=>{
    expect(lifeSceneParticipants([])).toMatchObject({canAdd:true,canStart:false,overLimit:false});
    expect(lifeSceneParticipants(members(7))).toMatchObject({canAdd:true,canStart:true,overLimit:false});
    expect(lifeSceneParticipants(members(8))).toEqual({count:8,limit:8,canAdd:false,canStart:true,overLimit:false});
    expect(lifeSceneParticipants(members(9))).toEqual({count:9,limit:8,canAdd:false,canStart:false,overLimit:true});
  });
  it('keeps existing larger saved layouts intact and permits start after explicit removal',()=>{
    for(const count of [9,64]){
      const draft=newWorldDraft('stage_existing');draft.members=members(count);
      const saved=draftWorld(draft),before=structuredClone(saved);
      const state=lifeSceneParticipants(Object.freeze(saved.config.members));
      expect(state).toMatchObject({count,canAdd:false,canStart:false,overLimit:true});
      expect(saved).toEqual(before);
      expect(worldDraft(saved).members).toEqual(before.config.members);
      expect(lifeSceneParticipants(saved.config.members.slice(0,8)).canStart).toBe(true);
      expect(saved.config.members).toHaveLength(count);
    }
  });
  it('explains the same retained-layout limit in all four interface languages',()=>{
    for(const language of ['de','en','fr','es'] as const){
      const hint=translate(LIFE_SCENE_LIMIT_HINT,{limit:8},language);
      const reason=translate(LIFE_SCENE_OVER_LIMIT,{count:9,limit:8},language);
      expect(hint).toContain('8');expect(reason).toContain('9');expect(reason).toContain('8');
      expect(hint+reason).not.toMatch(/\{(?:count|limit)\}/);
      if(language!=='de'){
        expect(hint).not.toBe(translate(LIFE_SCENE_LIMIT_HINT,{limit:8},'de'));
        expect(reason).not.toBe(translate(LIFE_SCENE_OVER_LIMIT,{count:9,limit:8},'de'));
      }
    }
  });
});
