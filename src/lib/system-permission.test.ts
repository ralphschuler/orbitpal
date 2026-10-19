import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { SystemPermissionFlow } from './system-permission';
const flush = async () => { await Promise.resolve(); await Promise.resolve(); };
const granted = { platform: 'macos', status: 'authorized' };
const denied = { platform: 'macos', status: 'notGranted' };

describe('Guided system permission intent without devices or OS prompts', () => {
  it('keeps the initial status check hidden, then reveals missing-grant guidance only when needed', async () => {
    let done!: (value: unknown) => void;
    const flow = new SystemPermissionFlow(() => new Promise(resolve => done = resolve), () => true);
    const action = flow.ensure('screen');
    expect(get(flow)).toMatchObject({ loading: true, visible: false });
    done(denied); await flush(); expect(get(flow)).toMatchObject({ loading: false, visible: true });
    flow.cancel(); expect(await action).toBe(false);
  });
  it('uses status only when permission is already granted and never requests again', async () => {
    const invoke = vi.fn().mockResolvedValue(granted); const flow = new SystemPermissionFlow(invoke, () => true);
    expect(await flow.ensure('screen')).toBe(true);
    expect(invoke.mock.calls.map(args => args[0])).toEqual(['get_screen_capture_permission']);
    expect(get(flow)).toBeNull();
  });
  it('waits for explicit Request on a missing grant, with no screenshot or microphone commands', async () => {
    const invoke = vi.fn().mockResolvedValueOnce(denied).mockResolvedValueOnce(granted);
    const flow = new SystemPermissionFlow(invoke, () => true);
    const action = flow.ensure('screen'); await flush();
    expect(get(flow)).toMatchObject({ loading: false, permission: denied });
    expect(invoke).toHaveBeenCalledTimes(1);
    await flow.request(); expect(await action).toBe(true);
    expect(invoke.mock.calls.map(args => args[0])).toEqual(['get_screen_capture_permission', 'request_screen_capture_permission']);
  });
  it('does not resume on a focus/status refresh; Continue performs a fresh status check', async () => {
    const invoke = vi.fn().mockResolvedValueOnce(denied).mockResolvedValue(granted);
    const flow = new SystemPermissionFlow(invoke, () => true); let continued = false;
    const action = flow.ensure('screen').then(result => { continued = result; return result; }); await flush();
    await flow.refresh(); await flush(); expect(continued).toBe(false); expect(get(flow)?.permission).toEqual(granted);
    await flow.continue(); expect(await action).toBe(true);
    expect(invoke.mock.calls.map(args => args[0])).toEqual(Array(3).fill('get_screen_capture_permission'));
  });
  it('does not trust a previously granted status after permissions are revoked before Continue', async () => {
    const invoke = vi.fn().mockResolvedValueOnce(denied).mockResolvedValueOnce(granted).mockResolvedValue(denied);
    const flow = new SystemPermissionFlow(invoke, () => true); const action = flow.ensure('screen'); await flush();
    await flow.refresh(); await flow.continue(); expect(get(flow)?.permission).toEqual(denied);
    flow.cancel(); expect(await action).toBe(false);
  });
  it.each(['Stop', 'bot change', 'configuration change', 'unmount', 'update'])('invalidates late OS replies after %s', async () => {
    let done!: (value: unknown) => void; let current = true;
    const invoke = vi.fn().mockResolvedValueOnce(denied).mockImplementationOnce(() => new Promise(resolve => done = resolve));
    const flow = new SystemPermissionFlow(invoke, () => true); const action = flow.ensure('screen', () => current); await flush();
    const request = flow.request(); current = false; flow.invalidate(); expect(await action).toBe(false);
    done(granted); await request; expect(get(flow)).toBeNull();
  });
  it('coalesces starts and double request clicks and permits cancellation while the OS is waiting', async () => {
    let done!: (value: unknown) => void;
    const invoke = vi.fn().mockResolvedValueOnce(denied).mockImplementationOnce(() => new Promise(resolve => done = resolve));
    const flow = new SystemPermissionFlow(invoke, () => true); const action = flow.ensure('screen'); await flush();
    expect(await flow.ensure('microphone')).toBe(false);
    const request = flow.request(); await flow.request(); await flow.refresh();
    expect(invoke).toHaveBeenCalledTimes(2); flow.cancel(); expect(await action).toBe(false);
    done(granted); await request; expect(get(flow)).toBeNull();
  });
  it('keeps background failures in recovery mode without retaining or replaying a request', async () => {
    const invoke = vi.fn().mockResolvedValueOnce(denied).mockResolvedValue(granted);
    const flow = new SystemPermissionFlow(invoke, () => true); flow.recover('screen', new Error('Permission revoked')); await flush();
    expect(get(flow)).toMatchObject({ recovery: true, error: expect.any(Error) });
    flow.recover('screen'); expect(invoke).toHaveBeenCalledTimes(1);
    await flow.request(); await flow.continue(); expect(get(flow)).toMatchObject({ recovery: true, permission: granted });
    flow.cancel(); expect(get(flow)).toBeNull();
  });
  it('preserves permission and Settings failures and allows checking again', async () => {
    const invoke = vi.fn().mockRejectedValueOnce(new Error('Status unavailable')).mockRejectedValueOnce(new Error('Settings unavailable')).mockResolvedValue(granted);
    const flow = new SystemPermissionFlow(invoke, () => true); const action = flow.ensure('microphone'); await flush();
    expect(get(flow)?.error).toMatchObject({ message: 'Status unavailable' });
    await flow.settings(); expect(get(flow)?.error).toMatchObject({ message: 'Settings unavailable' });
    await flow.refresh(); expect(get(flow)?.permission).toEqual(granted);
    flow.cancel(); expect(await action).toBe(false);
  });
  it('does not equate system-managed capture with native authorization or support browser capture', async () => {
    const invoke = vi.fn().mockResolvedValue({ platform: 'linux', status: 'systemManaged' });
    const native = new SystemPermissionFlow(invoke, () => true); expect(await native.ensure('screen')).toBe(true);
    const browser = new SystemPermissionFlow(invoke, () => false);
    expect(await browser.ensure('microphone')).toBe(true);
    const action = browser.ensure('screen'); await flush();
    expect(get(browser)?.permission).toEqual({ platform: 'browser', status: 'unsupported' });
    browser.cancel(); expect(await action).toBe(false); expect(invoke).toHaveBeenCalledTimes(1);
  });
  it('never trusts malformed native grant status', async () => {
    const invoke = vi.fn().mockResolvedValue({ status: 'authorized' }); const flow = new SystemPermissionFlow(invoke, () => true);
    const action = flow.ensure('microphone'); await flush(); expect(get(flow)?.error).toBeInstanceOf(Error);
    flow.cancel(); expect(await action).toBe(false);
  });
});
