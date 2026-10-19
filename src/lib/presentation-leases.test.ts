import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PresentationLeases } from './presentation-leases';
import { emoteDurationMilliseconds, emoteSequenceMilliseconds } from './presentation';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

describe('exact presentation ownership through return, speech and emote tails', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

  it('waits for the real renderer and holds until the visual presentation ends', async () => {
    const renderer = deferred<() => void>(); const release = vi.fn();
    const leases = new PresentationLeases(); const controller = new AbortController();
    const ticket = leases.request(() => renderer.promise, controller.signal);
    let ready = false; void ticket.ready.then(() => { ready = true; });
    await Promise.resolve(); expect(ready).toBe(false);
    renderer.resolve(release); await ticket.ready; expect(ready).toBe(true);
    leases.finish(ticket, 2500);
    vi.advanceTimersByTime(2499); expect(release).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1); expect(release).toHaveBeenCalledTimes(1);
    controller.abort(); leases.clear(); expect(release).toHaveBeenCalledTimes(1);
  });

  it('cancels a missing/late renderer immediately and releases its eventual lease', async () => {
    const renderer = deferred<() => void>(); const release = vi.fn();
    const leases = new PresentationLeases(); const controller = new AbortController();
    const ticket = leases.request(() => renderer.promise, controller.signal);
    const rejected = expect(ticket.ready).rejects.toMatchObject({ name: 'AbortError' });
    await Promise.resolve(); controller.abort(); await rejected;
    renderer.resolve(release); await Promise.resolve(); await Promise.resolve();
    expect(release).toHaveBeenCalledTimes(1);
  });

  it('never acquires a renderer for an already cancelled response', async () => {
    const prepare = vi.fn(async () => vi.fn()); const leases = new PresentationLeases();
    const controller = new AbortController(); controller.abort();
    await expect(leases.request(prepare, controller.signal).ready).rejects.toMatchObject({ name: 'AbortError' });
    expect(prepare).not.toHaveBeenCalled();
  });

  it('keeps an emote visible until both its visual duration and SFX have ended', async () => {
    let playing = true; const release = vi.fn();
    const leases = new PresentationLeases(() => playing);
    const ticket = leases.request(async () => release, new AbortController().signal);
    await ticket.ready; leases.finish(ticket, 1000);
    vi.advanceTimersByTime(1000); expect(release).not.toHaveBeenCalled();
    playing = false; leases.soundEnded(); expect(release).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('does not let an earlier sound-end shorten the remaining visual duration', async () => {
    const release = vi.fn(); const leases = new PresentationLeases();
    const ticket = leases.request(async () => release, new AbortController().signal);
    await ticket.ready; leases.finish(ticket, 3000); leases.soundEnded();
    vi.advanceTimersByTime(2999); expect(release).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1); expect(release).toHaveBeenCalledTimes(1);
  });

  it('bounds missing audio-end reports instead of pinning the character forever', async () => {
    const release = vi.fn(); const leases = new PresentationLeases(() => true);
    const ticket = leases.request(async () => release, new AbortController().signal);
    await ticket.ready; leases.finish(ticket);
    vi.advanceTimersByTime(15999); expect(release).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1); expect(release).toHaveBeenCalledTimes(1);
  });

  it('can replace emote tails without releasing the current speech lease', async () => {
    const speech = vi.fn(), emote = vi.fn(), next = vi.fn(); const leases = new PresentationLeases();
    const controller = new AbortController();
    const a = leases.request(async () => speech, controller.signal);
    const b = leases.request(async () => emote, new AbortController().signal);
    await Promise.all([a.ready, b.ready]); leases.finish(b, 5000); leases.clear(a);
    expect(emote).toHaveBeenCalledTimes(1); expect(speech).not.toHaveBeenCalled();
    const c = leases.request(async () => next, new AbortController().signal); await c.ready;
    leases.cancel(b); vi.advanceTimersByTime(5000);
    expect(next).not.toHaveBeenCalled(); expect(speech).not.toHaveBeenCalled();
    controller.abort(); leases.clear();
    expect(speech).toHaveBeenCalledTimes(1); expect(next).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('propagates a failed renderer without leaking a lease into the next response', async () => {
    const leases = new PresentationLeases();
    await expect(leases.request(async () => { throw new Error('asset unavailable'); }, new AbortController().signal).ready).rejects.toThrow('asset unavailable');
    const release = vi.fn(); const next = leases.request(async () => release, new AbortController().signal);
    await next.ready; leases.finish(next); expect(release).toHaveBeenCalledTimes(1);
  });

  it('uses the actual emote sequence timing for the remaining visible hold', () => {
    expect(emoteDurationMilliseconds({action:'wave',duration:0.01})).toBe(200);
    expect(emoteDurationMilliseconds({action:'wave',duration:100})).toBe(15000);
    expect(emoteSequenceMilliseconds([{action:'wave',duration:2},{action:'',duration:10},{action:'cheer'}])).toBe(4500);
  });
});
