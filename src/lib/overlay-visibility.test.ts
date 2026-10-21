import {describe,it,expect} from 'vitest';
import {applyOverlayVisibility} from './overlay-visibility';

describe('physical overlay visibility',()=>{
  it('keeps a pending Life scene start current before any Life output projection arrives',()=>{
    const profile={id:'nova',output:{mode:'studio',streamingEnabled:false}};
    const documents={agents:'local fixture',persona:'local fixture'};
    const context=JSON.stringify([profile,documents]);
    const current=()=>context===JSON.stringify([profile,documents]);
    let bots=[{id:'nova',live:false},{id:'blue',live:false}],stops=0;
    const target={botId:profile.id,
      setLive:(id:string,active:boolean)=>{bots=bots.map(bot=>bot.id===id?{...bot,live:active}:bot);},
      stopPlayer:()=>{stops++;},playbackIdle:()=>{}};
    // Actual host event shape, emitted while start_life_scene still awaits its
    // two constructors. Neither event is a persisted output-setting update.
    applyOverlayVisibility({botId:'nova',active:true},target);
    applyOverlayVisibility({botId:'blue',active:true},target);
    expect(bots.every(bot=>bot.live)).toBe(true);
    expect(stops).toBe(1);
    expect(current()).toBe(true);
    expect(profile.output).toEqual({mode:'studio',streamingEnabled:false});
  });

  it('preserves configured overlay settings while keeping existing playback notifications',()=>{
    const profile={id:'nova',output:{mode:'overlay',streamingEnabled:true}};
    const saved=JSON.stringify(profile);const live:boolean[]=[];let starts=0,idles=0;
    const target={botId:profile.id,setLive:(_id:string,active:boolean)=>{live.push(active);},
      stopPlayer:()=>{starts++;},playbackIdle:()=>{idles++;}};
    applyOverlayVisibility({botId:'nova',active:true},target);
    applyOverlayVisibility({botId:'nova',active:false},target);
    expect(live).toEqual([true,false]);expect(starts).toBe(1);expect(idles).toBe(1);
    expect(JSON.stringify(profile)).toBe(saved);
  });

  it('ignores malformed events and cannot treat extra output fields as saved settings',()=>{
    const events:{botId:string;active:boolean}[]=[];let starts=0,idles=0;
    const target={botId:'nova',setLive:(botId:string,active:boolean)=>{events.push({botId,active});},
      stopPlayer:()=>{starts++;},playbackIdle:()=>{idles++;}};
    for(const value of [null,[],{}, {botId:'nova',active:1}, {botId:'../private',active:true},
      {botId:'',active:false},{active:true}])applyOverlayVisibility(value,target);
    expect(events).toEqual([]);
    applyOverlayVisibility({botId:'blue',active:true,output:{mode:'overlay',streamingEnabled:true}},target);
    expect(events).toEqual([{botId:'blue',active:true}]);
    expect(starts).toBe(0);expect(idles).toBe(0);
  });
});
