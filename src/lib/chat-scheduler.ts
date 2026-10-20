export interface ChatMessage {
  queueId: string;
  text: string;
  platform: string;
  replyRouteId?: string;
  createdAt: string;
  eventType?: string;
  answeredBy?: readonly string[];
}

export interface ChatParticipant {
  botId: string;
  eligible: boolean;
  mode: string;
  trigger: string;
  expirySeconds: number;
}

interface ChatProfile {
  id?: string;
  chat?: { mode?: string; trigger?: string; expirySeconds?: number };
  model?: { model?: string; provider?: string };
  output?: { mode?: string; streamingEnabled?: boolean };
}

/** Only the selected Studio figure and explicitly active external outputs join
 * automatic chat. CLI providers may use their default model selection. */
export function chatParticipants(bots: readonly { id: string; live?: boolean; profile?: ChatProfile }[], selected: ChatProfile): ChatParticipant[] {
  return bots.map(bot => {
    const config = bot.id === selected.id ? selected : bot.profile;
    const output = bot.id === selected.id && config?.output?.mode === 'studio'
      || bot.live === true || config?.output?.mode === 'obs' && config.output.streamingEnabled === true;
    const model = !!config?.model?.model?.trim() || ['claude-code', 'codex'].includes(config?.model?.provider ?? '');
    return { botId: bot.id, eligible: !!config && !!output && model, mode: config?.chat?.mode ?? 'manual',
      trigger: config?.chat?.trigger ?? '', expirySeconds: config?.chat?.expirySeconds ?? 0 };
  });
}

export interface ChatClaim<T extends ChatMessage = ChatMessage> {
  readonly botId: string;
  readonly message: T;
  readonly revision: number;
}

export interface ManualChatClaim { readonly botId: string; readonly revision: number }

/** A configured direct-address match takes precedence over catch-all replies.
 * Notifications keep the existing trigger bypass, without gaining priority. */
export function chatPriority(bot: ChatParticipant, message: ChatMessage, now: number): number | undefined {
  if (!bot.eligible || !bot.botId || bot.mode === 'manual' || !['cohost', 'afk'].includes(bot.mode)
    || message.answeredBy?.includes(bot.botId) || !Number.isFinite(now)
    || !Number.isFinite(bot.expirySeconds) || bot.expirySeconds <= 0) return undefined;
  const createdAt = Date.parse(message.createdAt);
  if (!Number.isFinite(createdAt) || now - createdAt >= bot.expirySeconds * 1000) return undefined;
  const addressed = !!bot.trigger && message.text.toLowerCase().includes(bot.trigger.toLowerCase());
  if (bot.trigger && !addressed && (!message.eventType || message.eventType === 'message')) return undefined;
  return addressed ? 1 : 0;
}

/** Native reconnect duplicates retain their answeredBy state. Routes remain part
 * of queueId, so unrelated Twitch/YouTube sessions never share consumption. */
export function appendChatMessage<T extends ChatMessage>(queue: readonly T[], message: T): T[] {
  return (queue.some(entry => entry.queueId === message.queueId) ? [...queue] : [...queue, message]).slice(-200);
}

/** One automatic chat turn owns the entire asynchronous preflight/model/playback
 * job. Within a priority class, least-recently served bots cannot be starved by a
 * continuously replenished queue. The operator may cancel a pending claim; its
 * late finally can never release a newer turn. No timers, providers or I/O. */
export class ChatScheduler {
  private lastServed = new Map<string, number>();
  private order = 0;
  private inFlight?: ChatClaim;
  private manual?: ManualChatClaim;

  get pending(): boolean { return this.inFlight !== undefined || this.manual !== undefined; }

  nextClaim<T extends ChatMessage>(participants: readonly ChatParticipant[], queue: readonly T[], now = Date.now()): ChatClaim<T> | undefined {
    if (this.pending) return undefined;
    const present = new Set(participants.map(bot => bot.botId));
    for (const id of this.lastServed.keys()) if (!present.has(id)) this.lastServed.delete(id);
    let chosen: { botId: string; message: T; priority: number; served: number } | undefined;
    const seen = new Set<string>();
    for (const bot of participants) {
      if (seen.has(bot.botId)) continue;
      seen.add(bot.botId);
      let candidate: { botId: string; message: T; priority: number; served: number } | undefined;
      for (const message of queue) {
        const priority = chatPriority(bot, message, now);
        if (priority === undefined || candidate && candidate.priority >= priority) continue;
        candidate = { botId: bot.botId, message, priority, served: this.lastServed.get(bot.botId) ?? 0 };
      }
      if (candidate && (!chosen || candidate.priority > chosen.priority
        || candidate.priority === chosen.priority && candidate.served < chosen.served)) chosen = candidate;
    }
    if (!chosen) return undefined;
    this.lastServed.set(chosen.botId, ++this.order);
    const claim = Object.freeze({ botId: chosen.botId, message: chosen.message, revision: this.order });
    this.inFlight = claim;
    return claim;
  }

  isCurrentClaim(claim: ChatClaim): boolean { return this.inFlight === claim; }

  /** Reserve operator/manual work before its first await, preempting only an
   * automatic intent that has not yet acquired the caller's busy/playback lock.
   * The caller checks that lock first. Repeated manual clicks cannot overlap. */
  beginManual(botId: string): ManualChatClaim | undefined {
    if (this.manual) return undefined;
    this.inFlight = undefined;
    this.manual = Object.freeze({ botId, revision: ++this.order });
    return this.manual;
  }

  isCurrentManual(claim: ManualChatClaim): boolean { return this.manual === claim; }

  finishManual(claim: ManualChatClaim): void { if (this.isCurrentManual(claim)) this.manual = undefined; }

  async runClaim<T extends ChatMessage>(claim: ChatClaim<T>, action: () => Promise<unknown>): Promise<void> {
    if (!this.isCurrentClaim(claim)) return;
    try { await action(); }
    finally { if (this.isCurrentClaim(claim)) this.inFlight = undefined; }
  }

  cancel(botId?: string): void {
    if (botId === undefined || this.inFlight?.botId === botId) this.inFlight = undefined;
    if (botId === undefined || this.manual?.botId === botId) this.manual = undefined;
  }

  cancelAutomatic(): void { this.inFlight = undefined; }
}
