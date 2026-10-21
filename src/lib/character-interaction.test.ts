import {describe,it,expect} from 'vitest';
import {characterInteractionRequest} from './character-interaction';

describe('operator character interactions',()=>{
  const peers=[{id:'nova'},{id:'friend'},{id:'other'}];
  it('keeps the explicit peer instead of selecting another actor or solo activity',()=>{
    expect(characterInteractionRequest('nova','play','friend',peers)).toEqual({kind:'play',targetActorId:'friend'});
    expect(characterInteractionRequest('nova','play','other',peers)).toEqual({kind:'play',targetActorId:'other'});
  });
  it('rejects absent, self, removed and invalid peers before any native request',()=>{
    for(const target of [null,'','nova','removed','../friend'])expect(()=>characterInteractionRequest('nova','play',target,peers)).toThrow('experience_invalid');
    expect(()=>characterInteractionRequest('nova','play','friend',[{id:'nova'}])).toThrow('experience_invalid');
    // Changing the selected bot cannot turn its old peer into a self invitation.
    expect(()=>characterInteractionRequest('friend','play','friend',peers)).toThrow('experience_invalid');
  });
  it('does not attach a peer or permission to pet, home or pause',()=>{
    for(const kind of ['pet','home','pause'] as const)expect(characterInteractionRequest('nova',kind,'friend',peers)).toEqual({kind,targetActorId:null});
  });
});
