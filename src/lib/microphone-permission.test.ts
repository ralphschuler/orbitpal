import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
const api = vi.hoisted(() => ({ call: vi.fn(), native: true }));
vi.mock('./api', () => api);
import { ensureMicrophonePermission, microphoneFailure, MicrophonePermissionError, openMicrophone } from './microphone-permission';

describe('explicit microphone permission and recovery', () => {
  beforeEach(() => { api.call.mockReset(); api.native = true; microphoneFailure.set(''); });
  afterEach(() => vi.unstubAllGlobals());

  it('requests only the native grant and does not record audio', async () => {
    const capture = vi.fn(); vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: capture } });
    api.call.mockResolvedValue({ platform: 'macos', status: 'authorized' });
    expect(await ensureMicrophonePermission()).toBe(true);
    expect(api.call).toHaveBeenCalledExactlyOnceWith('request_microphone_permission');
    expect(capture).not.toHaveBeenCalled();
  });

  it.each(['denied', 'restricted', 'notDetermined', 'unknown'])('blocks capture on %s and exposes recovery', async status => {
    api.call.mockResolvedValue({ platform: 'macos', status });
    await expect(ensureMicrophonePermission()).rejects.toBeInstanceOf(MicrophonePermissionError);
    expect(get(microphoneFailure).length).toBeGreaterThan(30);
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
    await expect(ensureMicrophonePermission()).rejects.toBeInstanceOf(MicrophonePermissionError);
    expect(api.call).toHaveBeenCalledOnce();
    expect(get(microphoneFailure)).toContain('timed out');
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
    expect(api.call).toHaveBeenCalledOnce(); // no silent request/retry from capture
  });

  it('preserves device errors instead of diagnosing every failure as a denied grant', async () => {
    const failure = Object.assign(new Error('No input device'), { name: 'NotFoundError' });
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: vi.fn().mockRejectedValue(failure) } });
    await expect(openMicrophone({ audio: true })).rejects.toBe(failure);
    expect(get(microphoneFailure)).toBe('');
  });
});
