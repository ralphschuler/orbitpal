import { describe, expect, it, vi } from 'vitest';
import { UpdateController, emptyUpdateState, type UpdateState } from './updates';

const candidate = { id: 'candidate-1', version: '0.7.0', currentVersion: '0.6.0', notes: 'Neue Version', date: null, releaseUrl: 'https://github.com/ralphschuler/streamer-ai-cohost/releases/tag/v0.7.0' };
function fixture(automaticChecks = true) {
  let state: UpdateState = emptyUpdateState();
  const call = vi.fn(async (command: string): Promise<any> => {
    if (command === 'get_update_settings') return { automaticChecks };
    if (command === 'check_update') return { currentVersion: '0.6.0', update: candidate, installSupported: true, installUnavailableReason: null };
  });
  const lock = vi.fn();
  const controller = new UpdateController({ call, changed: next => state = next, lock, errorText: String, now: () => 123 });
  return { controller, call, lock, state: () => state };
}
describe('confirmed app updates', () => {
  it('automatic checks never download, and an opted-out user is not checked', async () => {
    const f = fixture(); await f.controller.load(); await f.controller.automaticCheck();
    expect(f.state().available).toEqual(candidate);
    expect(f.call.mock.calls.map(call => call[0])).toEqual(['get_update_settings', 'check_update']);
    expect(f.lock).not.toHaveBeenCalled();
    const disabled = fixture(false); await disabled.controller.load(); await disabled.controller.automaticCheck();
    expect(disabled.call).toHaveBeenCalledTimes(1);
    await disabled.controller.check(); expect(disabled.state().available).toEqual(candidate);
  });
  it('requires the current candidate, an idle studio and supported installation', async () => {
    const f = fixture(); await f.controller.check();
    expect(await f.controller.confirm('old-candidate', '')).toBe(false);
    expect(await f.controller.confirm(candidate.id, 'Mikrofon läuft')).toBe(false);
    expect(f.call).toHaveBeenCalledTimes(1);
    expect(await f.controller.confirm(candidate.id, '')).toBe(true);
    expect(f.call).toHaveBeenLastCalledWith('install_update', { candidateId: candidate.id, confirmed: true });
    expect(f.lock).toHaveBeenCalledExactlyOnceWith(true);
    expect(await f.controller.confirm(candidate.id, '')).toBe(false);
  });
  it('does not install a Linux deb and preserves the manual release option', async () => {
    const f = fixture();
    f.call.mockResolvedValueOnce({ update: candidate, installSupported: false, installUnavailableReason: 'AppImage verwenden' });
    await f.controller.check();
    expect(await f.controller.confirm(candidate.id, '')).toBe(false);
    expect(f.state().available?.releaseUrl).toBe(candidate.releaseUrl);
    expect(f.lock).not.toHaveBeenCalled();
  });
  it('unlocks after installation errors and requires a fresh check before retry', async () => {
    const f = fixture(); await f.controller.check();
    f.call.mockRejectedValueOnce(new Error('Signatur ungültig'));
    expect(await f.controller.confirm(candidate.id, '')).toBe(false);
    expect(f.state().error).toContain('Signatur ungültig');
    expect(f.lock.mock.calls).toEqual([[true], [false]]);
    expect(await f.controller.confirm(candidate.id, '')).toBe(false);
    await f.controller.check();
    expect(await f.controller.confirm(candidate.id, '')).toBe(true);
  });
  it('ignores unrelated progress, prevents duplicate install and keeps byte counts monotonic', async () => {
    const f = fixture(); await f.controller.check();
    let complete!: () => void;
    f.call.mockImplementationOnce(() => new Promise(resolve => complete = () => resolve(undefined)));
    const install = f.controller.confirm(candidate.id, '');
    f.controller.progress({ candidateId: 'other', phase: 'installing', downloaded: 100 });
    expect(f.state().phase).toBe('downloading');
    f.controller.progress({ candidateId: candidate.id, phase: 'downloading', downloaded: 70, total: 100 });
    f.controller.progress({ candidateId: candidate.id, phase: 'downloading', downloaded: 50 });
    expect(f.state().downloaded).toBe(70);
    expect(await f.controller.confirm(candidate.id, '')).toBe(false);
    complete(); await install;
  });
  it('does not publish an outstanding check result after disposal', async () => {
    const f = fixture(); let resolve!: (value: any) => void;
    f.call.mockImplementationOnce(() => new Promise(done => resolve = done));
    const pending = f.controller.check(); f.controller.destroy();
    resolve({ update: candidate, installSupported: true }); await pending;
    expect(f.state().available).toBeNull(); expect(f.lock).not.toHaveBeenCalled();
  });
});
