export interface ScreenSession {
  backend: 'native' | 'wayland';
  phase: string;
  displayId: string;
  message: string;
}

interface Preparation {
  current: () => boolean;
  context: () => string;
  flush: () => Promise<void>;
  normalize: () => void;
  save: () => Promise<void>;
}

/** Only the explicit source-selection action calls this preparation. A bot or
 * draft change while flushing/saving must never open a chooser for stale data.
 * Normalization is deliberate (e.g. an empty CLI model becomes "default"). */
export async function prepareScreenSession(options: Preparation): Promise<string | null> {
  if (!options.current()) return null;
  const initial = options.context();
  await options.flush();
  if (!options.current() || options.context() !== initial) return null;
  options.normalize();
  const prepared = options.context();
  await options.save();
  return options.current() && options.context() === prepared ? prepared : null;
}

export class ScreenSessionCleanupError extends Error {
  constructor() {
    super('Die veraltete Bildschirmfreigabe konnte nicht geschlossen werden. Bitte die Bildschirmfreigabe trennen oder Stop drücken.');
    this.name = 'ScreenSessionCleanupError';
  }
}

interface Selection {
  botId: string;
  current: () => boolean;
  context: () => string;
  prepare: () => Promise<string | null>;
  invoke: (command: string, args: Record<string, unknown>) => Promise<unknown>;
  adopt?: (session: ScreenSession) => void;
}

function readySession(value: unknown): value is ScreenSession {
  if (!value || typeof value !== 'object') return false;
  const session = value as ScreenSession;
  return session.backend === 'wayland' && session.phase === 'ready'
    && typeof session.displayId === 'string' && /^wayland:[0-9a-f-]{36}$/.test(session.displayId)
    && typeof session.message === 'string';
}

/** Selection owns its returned lease, not every later lease belonging to a bot.
 * Never use an unconditional disconnect to dispose a late successful result. */
export async function selectScreenSession(options: Selection): Promise<ScreenSession | null> {
  const prepared = await options.prepare();
  const current = () => options.current() && options.context() === prepared;
  if (prepared === null || !current()) return null;
  let result: unknown;
  try {
    result = await options.invoke('select_screen_source', { botId: options.botId });
  } catch (cause) {
    if (!current()) return null;
    throw cause;
  }
  if (!readySession(result)) throw new Error('Die Wayland-Quelle wurde nicht verbunden.');
  if (!current()) {
    try {
      await options.invoke('disconnect_screen_source', { botId: options.botId, expectedDisplayId: result.displayId });
    } catch {
      throw new ScreenSessionCleanupError();
    }
    return null;
  }
  // Apply component state in the same stack as the final validity check, not
  // after another await/microtask boundary in its caller.
  options.adopt?.(result);
  return result;
}
