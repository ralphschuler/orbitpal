type Waiter = {
  promise: Promise<void>;
  resolve: () => void;
  reject: (error: unknown) => void;
  timer: ReturnType<typeof setTimeout>;
};

export type PlaybackIdentity = { id: string; botId: string; autopilot?: boolean };

export function matchesPlayback(target: { id?: string; botId?: string }, current: PlaybackIdentity): boolean {
  return !!current.id && target.id === current.id && target.botId === current.botId;
}

/** Release an autonomous lease before the caller invalidates its turn epoch. */
export function cancelPlaybackTurn(target: { id?: string; botId?: string }, current: PlaybackIdentity, finishAutopilot: (botId: string) => void, stop: () => void): boolean {
  if (!matchesPlayback(target, current)) return false;
  if (current.autopilot) finishAutopilot(current.botId);
  stop();
  return true;
}

/** A timeout cancels its exact native job before releasing the Studio waiter. */
export class PlaybackWaiters {
  private waiting = new Map<string, Waiter>();
  private finished = new Set<string>();

  wait(id: string, botId: string, cancel: (id: string, botId: string) => Promise<unknown>, onTimeout: () => void, milliseconds = 180000): Promise<void> {
    if (this.finished.has(id)) return Promise.resolve();
    const previous = this.waiting.get(id);
    if (previous) return previous.promise;
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<void>((done, fail) => { resolve = done; reject = fail; });
    const waiter: Waiter = { promise, resolve, reject, timer: setTimeout(() => { void expire(); }, milliseconds) };
    this.waiting.set(id, waiter);
    const expire = async () => {
      if (this.waiting.get(id) !== waiter) return;
      try {
        await cancel(id, botId);
        if (this.waiting.get(id) !== waiter) return;
        onTimeout();
        this.finish(id);
      } catch (error) {
        if (this.waiting.get(id) !== waiter) return;
        onTimeout();
        this.waiting.delete(id);
        waiter.reject(error);
      }
    };
    return promise;
  }

  finish(id?: string): void {
    if (!id) {
      for (const key of [...this.waiting.keys()]) this.finish(key);
      return;
    }
    this.finished.add(id);
    if (this.finished.size > 128) this.finished.delete(this.finished.values().next().value!);
    const waiter = this.waiting.get(id);
    if (!waiter) return;
    clearTimeout(waiter.timer);
    this.waiting.delete(id);
    waiter.resolve();
  }
}
