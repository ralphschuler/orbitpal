import {describe,it,expect} from 'vitest';
import {parseRenderSettings,RenderSettingsVersions} from './render-settings';
describe('detached render settings',()=>{
  const value={version:1,botId:'nova',revision:1,quality:'energy'};
  it('keeps bot-local revisions and rejects late/conflicting initial responses',()=>{
    const versions=new RenderSettingsVersions();expect(versions.accept(value,'nova')).toEqual(value);
    expect(versions.accept({...value,revision:0,quality:'balanced'})).toBeUndefined();
    expect(versions.accept({...value,quality:'balanced'})).toBeUndefined();
    expect(versions.accept({...value,botId:'friend',revision:0,quality:'balanced'})?.quality).toBe('balanced');
    expect(versions.accept({...value,revision:2,quality:'balanced'})?.quality).toBe('balanced');
  });
  it('rejects unknown schema, malformed scope and unknown quality without changing profiles',()=>{
    for(const bad of [null,{...value,version:2},{...value,botId:'../nova'},{...value,revision:-1},{...value,quality:'maximum'}])expect(()=>parseRenderSettings(bad)).toThrow();
    expect(()=>parseRenderSettings(value,'friend')).toThrow();
    const profile={id:'nova',output:{mode:'obs',fps:60}};parseRenderSettings(value);expect(profile).toEqual({id:'nova',output:{mode:'obs',fps:60}});
  });
});
