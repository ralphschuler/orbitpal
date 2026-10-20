import { afterEach, describe, expect, it, vi } from 'vitest';
import { ExternalEmotePlayback } from './emote-playback';
import { canonicalSpeechId, SoundPlaybackReports } from './sound-playback';
const speechId='11111111-1111-1111-1111-111111abcdef';
const other='22222222-2222-2222-2222-222222222222';
const owner={botId:'nova',output:'overlay',speechId};
afterEach(()=>vi.useRealTimers());
describe('Host-associated native speech SFX retain exact independent leases',()=>{
  it('exempts only the matching bot, output and canonical parent while mixed foreign/manual leases block',()=>{
    vi.useFakeTimers();const changed=vi.fn(),revision=vi.fn();const receiver=new ExternalEmotePlayback(changed,undefined,revision);
    const own={botId:'nova',output:'overlay',id:'own-effect',playing:true,speechId};receiver.accept(own);
    expect(receiver.blocksExcept(owner)).toBe(false);expect(receiver.blocksExcept()).toBe(true);
    for(const difference of [{botId:'other'}, {output:'obs'}, {speechId:other}, {speechId:undefined}, {speechId:'invalid'}]){
      const event={...own,...difference,id:'foreign'};receiver.accept(event);
      expect(receiver.blocksExcept(owner)).toBe(true);receiver.accept({...event,playing:false});expect(receiver.blocksExcept(owner)).toBe(false);
    }
    expect(changed.mock.calls).toEqual([[true]]);expect(revision.mock.calls.length).toBe(11);
    receiver.accept({...own,playing:false});expect(changed.mock.calls).toEqual([[true],[false]]);receiver.destroy();
  });
  it('cannot relabel an existing lease by reusing its id and rejects a wrong-parent or wrong-owner finish',()=>{
    vi.useFakeTimers();const receiver=new ExternalEmotePlayback(()=>{});const event={...owner,id:'lease',playing:true};receiver.accept(event);
    receiver.accept({...event,speechId:other});expect(receiver.blocksExcept(owner)).toBe(false);
    receiver.accept({...event,playing:false,speechId:other});expect(receiver.blocksExcept()).toBe(true);
    receiver.accept({...event,playing:false,botId:'other'});receiver.accept({...event,playing:false,output:'obs'});expect(receiver.blocksExcept()).toBe(true);
    // Native host may no longer have the completed parent when reporting end;
    // the exact independent lease id/bot/output still releases only that lease.
    receiver.accept({...event,playing:false,speechId:undefined});expect(receiver.blocksExcept()).toBe(false);receiver.destroy();
  });
  it('starts a fresh independent lease when a parent changes and isolates a late old end',()=>{
    vi.useFakeTimers();let serial=0;const sender=new SoundPlaybackReports(()=>`effect-${++serial}`);const receiver=new ExternalEmotePlayback(()=>{});
    const apply=(reports:ReturnType<SoundPlaybackReports['update']>)=>reports.forEach(report=>receiver.accept({...report,botId:'nova',output:'overlay'}));
    apply(sender.update(true,false,speechId));const replacement=sender.update(true,false,other);
    expect(replacement).toEqual([{id:'effect-2',speechId:other,playing:true,activity:'foreground'},{id:'effect-1',speechId,playing:false,activity:'foreground'}]);apply(replacement);
    expect(receiver.blocksExcept({...owner,speechId:other})).toBe(false);expect(receiver.blocksExcept(owner)).toBe(true);
    receiver.accept({...owner,id:'effect-1',playing:false});expect(receiver.blocksExcept()).toBe(true);
    apply(sender.update(false,false));expect(receiver.blocksExcept()).toBe(false);receiver.destroy();
  });
  it('keeps ambient walking exempt and legacy/malformed parent associations conservative',()=>{
    vi.useFakeTimers();const receiver=new ExternalEmotePlayback(()=>{});let serial=0;const sender=new SoundPlaybackReports(()=>String(++serial));
    const reports=sender.update(false,true,speechId);expect(reports[0]).not.toHaveProperty('speechId');reports.forEach(report=>receiver.accept({...report,botId:'nova',output:'overlay'}));expect(receiver.blocksExcept()).toBe(false);
    for(const parent of [undefined,'INVALID',speechId.toUpperCase()]){
      // Only lowercase canonical UUIDs cross the association seam.
      expect(canonicalSpeechId(parent)).toBe(false);
      const report={botId:'nova',output:'overlay',id:'legacy',playing:true,speechId:parent};receiver.accept(report);expect(receiver.blocksExcept(owner)).toBe(true);receiver.accept({...report,playing:false});
    }
    receiver.destroy();expect(vi.getTimerCount()).toBe(0);
  });
});
