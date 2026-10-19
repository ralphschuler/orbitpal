export interface AutopilotConfig {
  speak: boolean;
  minSeconds: number;
  maxSeconds: number;
  maxTurnsPerHour: number;
}

export interface AutopilotCandidate {
  botId: string;
  seed: string;
  config: AutopilotConfig;
  eligible?: boolean;
}

export interface AutopilotSchedule {
  eligible: boolean;
  inFlight: boolean;
  waitingForBot?: string;
  dueAt: number;
  remainingSeconds: number;
  budgetLimited: boolean;
  attempts: number;
  limit: number;
}
export interface AutopilotClaim { readonly botId: string; readonly revision: number }

interface Session {
  seed: string;
  config: AutopilotConfig;
  eligible: boolean;
  dueAt: number;
}

interface Budget {
  attempts: number[];
  lastClaim: number;
}

const HOUR = 60 * 60 * 1000;

function bounded(value: number | undefined, fallback: number, minimum: number, maximum: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, value)) : fallback;
}

function normalize(config: AutopilotConfig | undefined): AutopilotConfig {
  const minSeconds = bounded(config?.minSeconds, 60, 15, 3600);
  return {
    speak: config?.speak === true,
    minSeconds,
    maxSeconds: Math.max(minSeconds, bounded(config?.maxSeconds, 180, 15, 3600)),
    maxTurnsPerHour: Math.floor(bounded(config?.maxTurnsPerHour, 12, 1, 120)),
  };
}

function sameConfig(left: AutopilotConfig, right: AutopilotConfig): boolean {
  return left.speak === right.speak && left.minSeconds === right.minSeconds
    && left.maxSeconds === right.maxSeconds && left.maxTurnsPerHour === right.maxTurnsPerHour;
}

/**
 * Per-bot quiet-time scheduling, without timers, providers or browser access.
 * The caller polls next() and owns cancellation plus the actual model/audio job.
 * A returned bot owns the single turn until finish()/stop(); failed attempts
 * count too. Clock values are milliseconds; configuration intervals are seconds.
 */
export class AutopilotScheduler {
  private sessions = new Map<string, Session>();
  private budgets = new Map<string, Budget>();
  private inFlight?: AutopilotClaim;
  private claimOrder = 0;
  private lastTime?: number;
  private readonly now: () => number;
  private readonly random: () => number;

  constructor(options: { now?: () => number; random?: () => number } = {}) {
    this.now = options.now ?? Date.now;
    this.random = options.random ?? Math.random;
  }

  private readTime(): number {
    const value = this.now();
    // A wall-clock correction cannot expire budgets or move the quiet period
    // backwards. An invalid injected reading retains the previous clock value.
    return Number.isFinite(value) ? Math.max(this.lastTime ?? value, value) : this.lastTime ?? 0;
  }

  private time(): number {
    this.lastTime = this.readTime();
    return this.lastTime;
  }

  /** Display only: does not claim a turn, draw randomness, move deadlines,
   * prune/create budgets or change the scheduler's monotonic clock. */
  snapshot(botId: string): AutopilotSchedule | undefined {
    const session = this.sessions.get(botId);
    if (!session) return undefined;
    const now = this.readTime();
    const attempts = (this.budgets.get(botId)?.attempts ?? []).filter(attempt => attempt > now - HOUR);
    const budgetAt = attempts.length >= session.config.maxTurnsPerHour
      ? attempts[attempts.length - session.config.maxTurnsPerHour] + HOUR : 0;
    const dueAt = Math.max(session.dueAt, budgetAt);
    return {
      eligible: session.eligible, inFlight: this.inFlight?.botId === botId,
      waitingForBot: this.inFlight && this.inFlight.botId !== botId ? this.inFlight.botId : undefined,
      dueAt, remainingSeconds: Math.max(0, Math.ceil((dueAt - now) / 1000)),
      budgetLimited: budgetAt > now, attempts: attempts.length, limit: session.config.maxTurnsPerHour,
    };
  }

  private due(config: AutopilotConfig, now: number): number {
    const value = this.random();
    const sample = Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0.5;
    return now + (config.minSeconds + sample * (config.maxSeconds - config.minSeconds)) * 1000;
  }

  private postpone(now: number): void {
    for (const session of this.sessions.values()) session.dueAt = this.due(session.config, now);
  }

  private budget(botId: string, now: number): Budget {
    let budget = this.budgets.get(botId);
    if (!budget) {
      budget = { attempts: [], lastClaim: 0 };
      this.budgets.set(botId, budget);
    }
    budget.attempts = budget.attempts.filter(attempt => attempt > now - HOUR);
    return budget;
  }

  /** Unchanged polling never moves a due time. Re-enabled/changed sessions wait anew. */
  sync(candidates: readonly AutopilotCandidate[]): void {
    const now = this.time();
    const seen = new Set<string>();
    for (const candidate of candidates) {
      if (!candidate || typeof candidate.botId !== 'string' || !candidate.botId.trim() || seen.has(candidate.botId)) continue;
      const { botId } = candidate;
      seen.add(botId);
      const config = normalize(candidate.config);
      if (!config.speak) { this.sessions.delete(botId); continue; }
      const seed = typeof candidate.seed === 'string' ? candidate.seed : '';
      const eligible = candidate.eligible !== false;
      const previous = this.sessions.get(botId);
      if (!previous || previous.seed !== seed || !sameConfig(previous.config, config) || previous.eligible !== eligible) {
        this.sessions.set(botId, { seed, config, eligible, dueAt: this.due(config, now) });
      }
      this.budget(botId, now);
    }
    for (const botId of this.sessions.keys()) if (!seen.has(botId)) this.sessions.delete(botId);
    // Keep an outstanding lease even if its configuration changes: sync does
    // not cancel the caller's running model/audio work. finish/stop releases it.
  }

  /** Conversation, input and other activity require a fresh full quiet period. */
  activity(): void { this.postpone(this.time()); }

  /** Claim at most one due bot. Blocking never accumulates overdue catch-up turns. */
  next(blocked = false): string | undefined {
    const now = this.time();
    if (blocked || this.inFlight !== undefined) { this.postpone(now); return undefined; }
    const ready: { botId: string; session: Session; budget: Budget }[] = [];
    for (const [botId, session] of this.sessions) {
      if (!session.eligible || session.dueAt > now) continue;
      const budget = this.budget(botId, now);
      if (budget.attempts.length >= session.config.maxTurnsPerHour) {
        const expiry = budget.attempts[budget.attempts.length - session.config.maxTurnsPerHour] + HOUR;
        session.dueAt = Math.max(session.dueAt, expiry);
        continue;
      }
      ready.push({ botId, session, budget });
    }
    // Least-recently served gives new bots a fair turn, independent of list
    // order. Deadlines and stable IDs break ties between never-served bots.
    ready.sort((left, right) => left.budget.lastClaim - right.budget.lastClaim
      || left.session.dueAt - right.session.dueAt || (left.botId < right.botId ? -1 : left.botId > right.botId ? 1 : 0));
    const chosen = ready[0];
    if (!chosen) return undefined;
    chosen.budget.attempts.push(now);
    chosen.budget.lastClaim = ++this.claimOrder;
    this.inFlight = Object.freeze({ botId: chosen.botId, revision: this.claimOrder });
    this.postpone(now);
    return chosen.botId;
  }

  /** Capture the exact turn lease before starting any asynchronous preflight. */
  nextClaim(blocked = false): AutopilotClaim | undefined {
    return this.next(blocked) === undefined ? undefined : this.inFlight;
  }

  /** Read-only identity check for an intent resuming after asynchronous work. */
  isCurrentClaim(claim: AutopilotClaim): boolean { return this.inFlight === claim; }

  /** Own the full attempt, including permission checks and early returns. A late
   * finally after Stop/restart must never release a newer same-bot turn. */
  async runClaim(claim: AutopilotClaim, action: () => Promise<unknown>): Promise<void> {
    if (this.inFlight !== claim) return;
    try { await action(); }
    finally {
      if (this.inFlight === claim) {
        this.inFlight = undefined;
        this.postpone(this.time());
      }
    }
  }

  /** Call after playback finishes, or after a failed/cancelled claimed attempt. */
  finish(botId: string): void {
    if (this.inFlight?.botId !== botId) return;
    this.inFlight = undefined;
    this.postpone(this.time());
  }

  /**
   * Cancel sessions without clearing their rolling budgets. A subsequent sync
   * can start enabled candidates again, always with a new full quiet period.
   * The caller must separately cancel any model/audio job belonging to the lease.
   */
  stop(botId?: string): void {
    if (botId === undefined) { this.sessions.clear(); this.inFlight = undefined; return; }
    this.sessions.delete(botId);
    if (this.inFlight?.botId === botId) { this.inFlight = undefined; this.postpone(this.time()); }
  }
}
