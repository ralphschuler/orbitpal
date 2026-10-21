import {describe,it,expect} from 'vitest';
import {refreshSnapshotDraft,refreshDevelopmentDraft} from './snapshot-draft';
import {parseDevelopment} from './social-experience';

const preferences=()=>({volume:60,participants:['nova']});
const development=()=>parseDevelopment({
  version:1,traits:{initiative:500,sociability:500,patience:500,restPreference:500},
  interests:{play:500,explore:500,company:500},routines:[],
  hobbies:['play','explore','company'].map(kind=>({kind,experience:0,variation:0})),
  goal:{id:'goal',kind:'play',target:5,progress:0,status:'active'},
});

describe('snapshot refreshes and independent preference edits',()=>{
  it('keeps an unsaved volume/participant edit through album and runtime refreshes',()=>{
    let previous=preferences();
    const draft={volume:20,participants:['nova','blue']};
    for(let i=0;i<3;i++){
      const incoming=preferences();
      expect(refreshSnapshotDraft(draft,previous,incoming)).toBe(draft);
      expect(incoming).toEqual(preferences());
      previous=incoming;
    }
  });
  it('initializes and refreshes a clean form without sharing mutable snapshot arrays',()=>{
    const incoming=preferences();
    const initial=refreshSnapshotDraft(undefined,undefined,incoming);
    initial.participants.push('blue');
    expect(incoming.participants).toEqual(['nova']);
    const next={volume:30,participants:['green']};
    expect(refreshSnapshotDraft(preferences(),preferences(),next)).toEqual(next);
  });
  it('replaces a dirty draft only when explicitly accepting a successful save or discard',()=>{
    const draft={volume:10,participants:['blue']},incoming=preferences();
    const saved=refreshSnapshotDraft(draft,preferences(),incoming,true);
    expect(saved).toEqual(incoming);expect(saved).not.toBe(draft);
    saved.participants.push('green');expect(incoming.participants).toEqual(['nova']);
  });
  it('keeps social preferences while accepting actual new hobby and goal progress',()=>{
    const previous=development(),draft=development(),incoming=development();
    draft.traits.sociability=800;draft.interests.play=900;
    draft.routines=[{startMinute:540,endMinute:600,kind:'play',weight:300}];
    incoming.hobbies[0]={kind:'play',experience:250,variation:1};incoming.goal!.progress=2;
    const refreshed=refreshDevelopmentDraft(draft,previous,incoming);
    expect(refreshed.traits.sociability).toBe(800);expect(refreshed.interests.play).toBe(900);
    expect(refreshed.routines).toEqual(draft.routines);
    expect(refreshed.hobbies[0].experience).toBe(250);expect(refreshed.goal!.progress).toBe(2);
    expect(incoming.traits.sociability).toBe(500);expect(incoming.routines).toEqual([]);
  });
  it('applies development reset progress without discarding unrelated dirty preferences',()=>{
    const previous=development(),draft=development(),incoming=development();
    previous.hobbies[0]={kind:'play',experience:1000,variation:2};draft.hobbies=structuredClone(previous.hobbies);
    draft.traits.initiative=700;incoming.goal=null;
    const refreshed=refreshDevelopmentDraft(draft,previous,incoming);
    expect(refreshed.traits.initiative).toBe(700);
    expect(refreshed.hobbies[0].experience).toBe(0);expect(refreshed.goal).toBeNull();
  });
  it('retains a failed explicit goal discard only for that exact native goal',()=>{
    const previous=development(),draft=development(),incoming=development();
    draft.goal!.status='discarded';incoming.goal!.progress=1;
    const refreshed=refreshDevelopmentDraft(draft,previous,incoming);
    expect(refreshed.goal).toEqual({...incoming.goal,status:'discarded'});
    expect(incoming.goal!.status).toBe('active');
    incoming.goal!.id='successor';
    expect(refreshDevelopmentDraft(draft,previous,incoming).goal).toEqual(incoming.goal);
    incoming.goal=null;expect(refreshDevelopmentDraft(draft,previous,incoming).goal).toBeNull();
  });
  it('a successful explicit social save replaces preferences and clears pending discard intent',()=>{
    const previous=development(),draft=development(),incoming=development();
    draft.traits.patience=900;draft.goal!.status='discarded';
    expect(refreshDevelopmentDraft(draft,previous,incoming,true)).toEqual(incoming);
  });
});
