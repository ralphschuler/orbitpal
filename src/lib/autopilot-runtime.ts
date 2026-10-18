export interface AutopilotRuntime {
  botId: string;
  active: boolean;
  wander: boolean;
  gestures: boolean;
  startedAt: number;
  seed: string;
  revision: number;
}

function valid(runtime: AutopilotRuntime | undefined): runtime is AutopilotRuntime {
  return !!runtime && typeof runtime.botId === 'string' && /^[A-Za-z0-9_-]{1,96}$/.test(runtime.botId)
    && typeof runtime.active === 'boolean' && typeof runtime.wander === 'boolean' && typeof runtime.gestures === 'boolean'
    && Number.isSafeInteger(runtime.startedAt) && runtime.startedAt >= 0
    && Number.isSafeInteger(runtime.revision) && runtime.revision >= 0
    && typeof runtime.seed === 'string' && runtime.seed.length <= 128 && !/[\u0000-\u001f\u007f]/.test(runtime.seed)
    && (!runtime.active || runtime.seed.trim().length > 0);
}

function same(left: AutopilotRuntime, right: AutopilotRuntime): boolean {
  return left.botId === right.botId && left.active === right.active && left.wander === right.wander
    && left.gestures === right.gestures && left.startedAt === right.startedAt
    && left.seed === right.seed && left.revision === right.revision;
}

/** A stopped bot retains its highest revision: late events and snapshots cannot
 * remove that tombstone. Identical duplicates permit a command response to
 * authorize a start after its earlier passive event was intentionally ignored. */
export class AutopilotRuntimeVersions {
  private latest = new Map<string, AutopilotRuntime>();

  accept(runtime: AutopilotRuntime): boolean {
    if (!valid(runtime)) return false;
    const previous = this.latest.get(runtime.botId);
    if (previous && runtime.revision <= previous.revision) return same(previous, runtime);
    this.latest.set(runtime.botId, { ...runtime });
    return true;
  }
}

/** Apply separately from version ordering. Passive events may update an
 * authorized session, but only a current Studio command/snapshot may start one. */
export function mayUpdateAutopilot(current: AutopilotRuntime | undefined, incoming: AutopilotRuntime, allowStart = false): boolean {
  if (!valid(incoming) || (current !== undefined && !valid(current))) return false;
  if (current && (current.botId !== incoming.botId || incoming.revision < current.revision
    || (incoming.revision === current.revision && !same(current, incoming)))) return false;
  if (!incoming.active) return current === undefined || current.seed === incoming.seed;
  return allowStart || !!current?.active && current.seed === incoming.seed;
}
