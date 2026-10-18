import { describe, expect, it, vi } from 'vitest';
import { MicrophoneSession } from './microphone-session';
const stream = () => { const stop = vi.fn(); return { getTracks: () => [{ stop }], stop }; };
describe('explicit microphone sessions', () => {
  it('permits only one outstanding permission request', async () => {
    const pending = vi.fn(); const session = new MicrophoneSession(pending);
    let resolve!: (value: ReturnType<typeof stream>) => void;
    const open = vi.fn(() => new Promise<ReturnType<typeof stream>>(done => resolve = done));
    const first = session.request(open, () => true);
    expect(await session.request(open, () => true)).toBeNull(); expect(open).toHaveBeenCalledTimes(1);
    const media = stream(); resolve(media); const lease = await first;
    expect(lease?.stream).toBe(media); expect(pending.mock.calls).toEqual([[true], [false]]);
  });
  it('stops a permission result arriving after Stop or update confirmation', async () => {
    const session = new MicrophoneSession(vi.fn()); let resolve!: (value: ReturnType<typeof stream>) => void;
    const opening = session.request(() => new Promise(done => resolve = done), () => true);
    session.cancel(); const media = stream(); resolve(media);
    expect(await opening).toBeNull(); expect(media.stop).toHaveBeenCalledOnce();
  });
  it('stops a result if bot or STT context changed before permission resolved', async () => {
    const session = new MicrophoneSession(vi.fn()); let current = 'nova:local';
    const captured = current; const media = stream();
    const opening = session.request(async () => { current = 'other:cloud'; return media; }, () => current === captured);
    expect(await opening).toBeNull(); expect(media.stop).toHaveBeenCalledOnce();
  });
  it('keeps a newer capture independent of an older cancelled request', async () => {
    const session = new MicrophoneSession(vi.fn()); let resolve!: (value: ReturnType<typeof stream>) => void;
    const old = session.request(() => new Promise(done => resolve = done), () => true); session.cancel();
    const currentMedia = stream(); const current = await session.request(async () => currentMedia, () => true);
    const oldMedia = stream(); resolve(oldMedia); await old;
    expect(current && session.isCurrent(current)).toBe(true); expect(currentMedia.stop).not.toHaveBeenCalled();
    session.cancel(); expect(currentMedia.stop).toHaveBeenCalledOnce(); expect(current && session.isCurrent(current)).toBe(false);
  });
});
