import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
const api = vi.hoisted(() => ({ call: vi.fn(), native: true }));
vi.mock('./api', () => api);
import { ensureMicrophonePermission, microphoneFailure, openMicrophone } from './microphone-permission';

import { systemPermission } from './system-permission';
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };

describe('explicit microphone permission and recovery', () => {
  beforeEach(() => { systemPermission.cancel(); api.call.mockReset(); api.native = true; microphoneFailure.set(''); });
  afterEach(() => { systemPermission.cancel(); vi.unstubAllGlobals(); });

  it('requests only the native grant and does not record audio', async () => {
    const capture = vi.fn(); vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: capture } });
    api.call.mockResolvedValue({ platform: 'macos', status: 'authorized' });
    expect(await ensureMicrophonePermission()).toBe(true);
    expect(api.call).toHaveBeenCalledExactlyOnceWith('get_microphone_permission');
    expect(capture).not.toHaveBeenCalled();
  });

  it.each(['denied', 'restricted', 'notDetermined', 'unknown'])('blocks capture on %s and exposes recovery', async status => {
    api.call.mockResolvedValue({ platform: 'macos', status });
    const action = ensureMicrophonePermission(); await flush();
    expect(get(systemPermission)?.permission?.status).toBe(status);
    systemPermission.cancel(); expect(await action).toBe(false);
    expect(api.call).toHaveBeenCalledTimes(1);
  });

  it('ignores a permission grant arriving after Stop or a bot change', async () => {
    let current = true;
    api.call.mockImplementation(async () => { current = false; return { platform: 'macos', status: 'authorized' }; });
    expect(await ensureMicrophonePermission(() => current)).toBe(false);
    api.call.mockClear();
    expect(await ensureMicrophonePermission(() => false)).toBe(false);
    expect(api.call).not.toHaveBeenCalled();
  });

  it('requires another explicit start when the native check fails', async () => {
    api.call.mockRejectedValue(new Error('Native permission request timed out'));
    const action = ensureMicrophonePermission(); await flush();
    expect(get(systemPermission)?.error).toMatchObject({ message: 'Native permission request timed out' });
    expect(api.call).toHaveBeenCalledOnce();
    systemPermission.cancel(); expect(await action).toBe(false);
  });

  it('does not claim native authorization on other platforms', async () => {
    api.call.mockResolvedValue({ platform: 'windows', status: 'webview' });
    expect(await ensureMicrophonePermission()).toBe(true);
    const denied = Object.assign(new Error('Permission denied'), { name: 'NotAllowedError' });
    const capture = vi.fn().mockRejectedValue(denied);
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: capture } });
    await expect(openMicrophone({ audio: true })).rejects.toMatchObject({ name: 'NotAllowedError' });
    expect(get(microphoneFailure)).toContain('WebView');
    expect(capture).toHaveBeenCalledOnce();
    await flush();
    expect(api.call.mock.calls.every(args => args[0] === 'get_microphone_permission')).toBe(true); // recovery only checks status, never silently requests or retries capture
  });

  it('does not reopen recovery for a late WebView denial after Stop or a changed configuration', async () => {
    let reject!: (error: unknown) => void; let current = true;
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: () => new Promise((_resolve, failure) => reject = failure) } });
    const opening = openMicrophone({ audio: true }, () => current);
    current = false;
    const error = Object.assign(new Error('Permission denied'), { name: 'NotAllowedError' });
    reject(error); await expect(opening).rejects.toBe(error);
    expect(systemPermission.active).toBe(false); expect(get(microphoneFailure)).toBe('');
  });

  it('preserves device errors instead of diagnosing every failure as a denied grant', async () => {
    const failure = Object.assign(new Error('No input device'), { name: 'NotFoundError' });
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: vi.fn().mockRejectedValue(failure) } });
    await expect(openMicrophone({ audio: true })).rejects.toBe(failure);
    expect(get(microphoneFailure)).toBe('');
  });
});
