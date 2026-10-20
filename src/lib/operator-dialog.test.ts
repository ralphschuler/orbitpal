import { afterEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
const mocks=vi.hoisted(()=>({open:vi.fn(),save:vi.fn()}));
vi.mock('@tauri-apps/plugin-dialog',()=>mocks);
vi.mock('./api',()=>({native:true}));
import { open, save, operatorDialogActive, registerOperatorDialogBarrier } from './operator-dialog';
const held=()=>{let resolve!:()=>void;const promise=new Promise<void>(done=>resolve=done);return{promise,resolve};};
afterEach(()=>vi.clearAllMocks());
describe('Operator file dialogs wait for native autonomy acknowledgement',()=>{
  it('sets the blocker before awaiting acknowledgement and opens only afterwards',async()=>{
    const ack=held();const release=registerOperatorDialogBarrier(()=>ack.promise);mocks.open.mockResolvedValue('/synthetic/file.glb');
    const pending=open({multiple:false});expect(get(operatorDialogActive)).toBe(true);expect(mocks.open).not.toHaveBeenCalled();
    ack.resolve();await expect(pending).resolves.toBe('/synthetic/file.glb');expect(get(operatorDialogActive)).toBe(false);release();
  });
  it('does not open when both the status and approved native Stop fallback fail',async()=>{
    const release=registerOperatorDialogBarrier(async()=>{throw new Error('fixed acknowledgement unavailable');});
    await expect(save()).rejects.toThrow('fixed acknowledgement');expect(mocks.save).not.toHaveBeenCalled();expect(get(operatorDialogActive)).toBe(false);release();
  });
  it('rejects a stale registration even if its old acknowledgement arrives',async()=>{
    const old=held();const release=registerOperatorDialogBarrier(()=>old.promise);const pending=open();const rejected=expect(pending).rejects.toThrow('Studio-Neustart');
    const current=registerOperatorDialogBarrier(async()=>{});release();old.resolve();await rejected;expect(mocks.open).not.toHaveBeenCalled();current();
  });
  it('retains the blocker until all acknowledged dialogs close and refuses an unregistered native dialog',async()=>{
    const a=held(),b=held();const release=registerOperatorDialogBarrier(async()=>{});mocks.open.mockImplementationOnce(()=>a.promise).mockImplementationOnce(()=>b.promise);
    const one=open(),two=open();await Promise.resolve();await Promise.resolve();a.resolve();await one;expect(get(operatorDialogActive)).toBe(true);b.resolve();await two;expect(get(operatorDialogActive)).toBe(false);release();
    await expect(open()).rejects.toThrow('noch nicht bereit');expect(mocks.open).toHaveBeenCalledTimes(2);
  });
});
