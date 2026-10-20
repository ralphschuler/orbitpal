import { describe, expect, it, vi } from 'vitest';
import { prepareScreenSession, selectScreenSession, ScreenSessionCleanupError } from './screen-session';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}
const flush = async () => { await Promise.resolve(); await Promise.resolve(); };
const ready = (id = '11111111-1111-4111-8111-111111111111') => ({
  backend: 'wayland' as const, phase: 'ready', displayId: `wayland:${id}`, message: 'fixture ready',
});

describe('explicit screen selection preparation without a desktop', () => {
  it('saves the normalized visible CLI model before opening the chooser', async () => {
    const order: string[] = []; let model = ''; let saved = 'previous-model';
    const write = deferred<void>();
    const context = () => JSON.stringify({ botId: 'nova', model });
    const invoke = vi.fn(async () => { order.push('select'); expect(saved).toBe('default'); return ready(); });
    const action = selectScreenSession({ botId: 'nova', current: () => true, context, invoke,
      prepare: () => prepareScreenSession({ current: () => true, context,
        flush: async () => { order.push('flush'); },
        normalize: () => { order.push('normalize'); if (!model.trim()) model = 'default'; },
        save: async () => { order.push('save'); saved = model; await write.promise; },
      }),
    });
    await flush(); expect(order).toEqual(['flush', 'normalize', 'save']); expect(invoke).not.toHaveBeenCalled();
    write.resolve(); expect(await action).toEqual(ready()); expect(order).toEqual(['flush', 'normalize', 'save', 'select']);
    expect(invoke).toHaveBeenCalledWith('select_screen_source', { botId: 'nova' });
  });

  it.each(['model edit', 'bot switch', 'unmount', 'update'])('does not save or select after %s during preparation', async reason => {
    let active = true; let context = 'original'; const pending = deferred<void>();
    const normalize = vi.fn(), save = vi.fn(async () => {}), invoke = vi.fn();
    const action = selectScreenSession({ botId: 'nova', current: () => active, context: () => context, invoke,
      prepare: () => prepareScreenSession({ current: () => active, context: () => context,
        flush: () => pending.promise, normalize, save }),
    });
    if (reason === 'model edit') context = 'changed'; else active = false;
    pending.resolve(); expect(await action).toBeNull(); expect(normalize).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled(); expect(invoke).not.toHaveBeenCalled();
  });

  it('does not prepare an already invalidated action', async () => {
    const flush = vi.fn(async () => {}), normalize = vi.fn(), save = vi.fn(async () => {});
    expect(await prepareScreenSession({ current: () => false, context: () => '', flush, normalize, save })).toBeNull();
    expect(flush).not.toHaveBeenCalled(); expect(normalize).not.toHaveBeenCalled(); expect(save).not.toHaveBeenCalled();
  });

  it('keeps an edit made during save as an unsaved draft and never opens its chooser', async () => {
    let model = 'visible-model'; const pending = deferred<void>(); const saved: string[] = []; const invoke = vi.fn();
    const action = selectScreenSession({ botId: 'nova', current: () => true, context: () => model, invoke,
      prepare: () => prepareScreenSession({ current: () => true, context: () => model, flush: async () => {}, normalize: () => {},
        save: async () => { saved.push(model); await pending.promise; } }),
    });
    await flush(); model = 'newer-draft'; pending.resolve();
    expect(await action).toBeNull(); expect(saved).toEqual(['visible-model']); expect(model).toBe('newer-draft'); expect(invoke).not.toHaveBeenCalled();
  });

  it('does not select if saving fails or returns no prepared context', async () => {
    const invoke = vi.fn(); const options = { botId: 'nova', current: () => true, context: () => 'draft', invoke };
    await expect(selectScreenSession({ ...options, prepare: async () => { throw new Error('save failed'); } })).rejects.toThrow('save failed');
    expect(await selectScreenSession({ ...options, prepare: async () => null })).toBeNull(); expect(invoke).not.toHaveBeenCalled();
  });
});

describe('exact ownership of late successful chooser results', () => {
  it.each(['context edit', 'unmount', 'bot switch', 'disconnect'])('releases only its returned lease after %s', async reason => {
    let active = true; let context = 'saved'; const pending = deferred<unknown>();
    const adopt = vi.fn();
    const invoke = vi.fn().mockImplementationOnce(() => pending.promise).mockResolvedValueOnce(undefined);
    const action = selectScreenSession({ botId: 'nova', current: () => active, context: () => context, prepare: async () => 'saved', invoke, adopt });
    await flush(); if (reason === 'context edit') context = 'changed'; else active = false;
    pending.resolve(ready()); expect(await action).toBeNull();
    expect(adopt).not.toHaveBeenCalled();
    expect(invoke.mock.calls).toEqual([
      ['select_screen_source', { botId: 'nova' }],
      ['disconnect_screen_source', { botId: 'nova', expectedDisplayId: ready().displayId }],
    ]);
  });

  it('cannot close the replacement returned by a newer operation', async () => {
    const old = deferred<unknown>(); let oldCurrent = true; const replacement = ready('22222222-2222-4222-8222-222222222222');
    let activeLease = ready().displayId;
    const invoke = vi.fn(async (command: string, args: Record<string, unknown>) => {
      if (command === 'select_screen_source') {
        if (oldCurrent) return old.promise;
        activeLease = replacement.displayId; return replacement;
      }
      if (args.expectedDisplayId === activeLease) activeLease = '';
      return undefined;
    });
    const first = selectScreenSession({ botId: 'nova', current: () => oldCurrent, context: () => 'saved', prepare: async () => 'saved', invoke });
    await flush(); oldCurrent = false;
    expect(await selectScreenSession({ botId: 'nova', current: () => true, context: () => 'saved', prepare: async () => 'saved', invoke })).toEqual(replacement);
    old.resolve(ready()); expect(await first).toBeNull(); expect(activeLease).toBe(replacement.displayId);
    expect(invoke).toHaveBeenLastCalledWith('disconnect_screen_source', { botId: 'nova', expectedDisplayId: ready().displayId });
  });

  it('surfaces cleanup failure after unmount, with no retries or unconditional disconnect', async () => {
    let active = true; const pending = deferred<unknown>();
    const invoke = vi.fn().mockImplementationOnce(() => pending.promise).mockRejectedValueOnce(new Error('IPC disconnected'));
    const action = selectScreenSession({ botId: 'nova', current: () => active, context: () => 'saved', prepare: async () => 'saved', invoke });
    const rejected = expect(action).rejects.toBeInstanceOf(ScreenSessionCleanupError);
    await flush(); active = false; pending.resolve(ready()); await rejected;
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('does not publish an old selection error into an edited or disposed context', async () => {
    let context = 'saved'; const pending = deferred<unknown>(); const adopt = vi.fn();
    const invoke = vi.fn(() => pending.promise);
    const action = selectScreenSession({ botId: 'nova', current: () => true, context: () => context, prepare: async () => 'saved', invoke, adopt });
    await flush(); context = 'changed'; pending.reject(new Error('old chooser cancelled'));
    expect(await action).toBeNull(); expect(adopt).not.toHaveBeenCalled(); expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('rejects malformed results and preserves a current selection without disconnecting', async () => {
    const options = { botId: 'nova', current: () => true, context: () => 'saved', prepare: async () => 'saved' };
    const invalid = vi.fn(async () => ({ ...ready(), displayId: 'unknown-source' }));
    await expect(selectScreenSession({ ...options, invoke: invalid })).rejects.toThrow('nicht verbunden'); expect(invalid).toHaveBeenCalledTimes(1);
    const valid = vi.fn(async () => ready()); const adopt = vi.fn();
    const selected = selectScreenSession({ ...options, invoke: valid, adopt });
    expect(await selected).toEqual(ready()); expect(adopt).toHaveBeenCalledExactlyOnceWith(ready()); expect(valid).toHaveBeenCalledTimes(1);
  });
});
