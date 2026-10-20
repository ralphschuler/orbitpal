import { describe, expect, it, vi } from 'vitest';
import { ChatScheduler, appendChatMessage, chatParticipants, chatPriority, type ChatMessage, type ChatParticipant } from './chat-scheduler';

const now = Date.parse('2026-10-20T12:00:00Z');
const bot = (botId: string, values: Partial<ChatParticipant> = {}): ChatParticipant => ({
  botId, eligible: true, mode: 'cohost', trigger: '', expirySeconds: 90, ...values,
});
const message = (id: string, values: Partial<ChatMessage> = {}): ChatMessage => ({
  queueId: JSON.stringify(['twitch', 'route-one', id]), text: `Viewer message ${id}`,
  platform: 'twitch', replyRouteId: 'route-one', createdAt: new Date(now).toISOString(), answeredBy: [], ...values,
});
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>(done => resolve = done);
  return { promise, resolve };
}

describe('automatic chat turn fairness', () => {
  it('selects each of three bots within every three completed turns while matching chat never runs out', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('nova'), bot('james'), bot('mushy', { mode: 'afk' })];
    let queue = [message('initial')];
    const selected: string[] = [];
    for (let turn = 0; turn < 24; turn++) {
      // The first bot always has more work. Fixed list-order selection fails
      // this regression even though a finite, eventually drained queue passes.
      queue = appendChatMessage(queue, message(`refill-${turn}`));
      expect(queue.some(item => chatPriority(participants[0], item, now) !== undefined)).toBe(true);
      const claim = scheduler.nextClaim(participants, queue, now)!;
      selected.push(claim.botId);
      await scheduler.runClaim(claim, async () => {
        queue = queue.map(item => item.queueId === claim.message.queueId
          ? { ...item, answeredBy: [...(item.answeredBy ?? []), claim.botId] } : item);
        await Promise.resolve(); // Synthetic provider/playback completion.
      });
    }
    expect(selected).toEqual(Array.from({ length: 8 }, () => ['nova', 'james', 'mushy']).flat());
    for (let first = 0; first <= selected.length - 3; first++) {
      expect(new Set(selected.slice(first, first + 3))).toEqual(new Set(['nova', 'james', 'mushy']));
    }
  });

  it('skips manual bots, missing models and disabled outputs using the production participant mapping', async () => {
    const chat = { mode: 'cohost', trigger: '', expirySeconds: 90 };
    const ready = { chat, model: { model: 'fixture' }, output: { mode: 'overlay' } };
    const profiles = [
      { id: 'manual', live: true, profile: { ...ready, chat: { ...chat, mode: 'manual' } } },
      { id: 'disabled', live: false, profile: ready },
      { id: 'no-model', live: true, profile: { ...ready, model: { model: '   ' } } },
      { id: 'obs-off', profile: { ...ready, output: { mode: 'obs', streamingEnabled: false } } },
      { id: 'nova', live: true, profile: ready },
      { id: 'james', profile: { ...ready, output: { mode: 'obs', streamingEnabled: true } } },
    ];
    const scheduler = new ChatScheduler();
    const participants = chatParticipants(profiles, { id: 'disabled', ...ready });
    const queue = [message('expired', { createdAt: new Date(now - 90_000).toISOString() }), message('fresh')];
    const selected = [];
    for (let turn = 0; turn < 4; turn++) {
      const claim = scheduler.nextClaim(participants, queue, now)!;
      selected.push(claim.botId);
      expect(claim.message.queueId).toBe(queue[1].queueId);
      await scheduler.runClaim(claim, async () => {});
    }
    expect(selected).toEqual(['nova', 'james', 'nova', 'james']);
  });

  it('allows the selected Studio bot and CLI defaults, while unselected Studio bots stay inactive', () => {
    const config = { chat: { mode: 'cohost', expirySeconds: 90 }, model: { provider: 'codex', model: '' }, output: { mode: 'studio' } };
    const participants = chatParticipants([
      { id: 'selected', profile: config }, { id: 'other', profile: config },
      { id: 'claude', live: true, profile: { ...config, model: { provider: 'claude-code', model: '' } } },
    ], { id: 'selected', ...config });
    expect(participants.map(entry => [entry.botId, entry.eligible])).toEqual([
      ['selected', true], ['other', false], ['claude', true],
    ]);
  });

  it('gives a newly joined eligible bot its turn despite a continuously refilled first bot', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('a'), bot('b')];
    let queue = [message('initial')];
    const selected = [];
    for (let turn = 0; turn < 9; turn++) {
      if (turn === 4) participants.push(bot('new'));
      queue = appendChatMessage(queue, message(`join-${turn}`));
      const claim = scheduler.nextClaim(participants, queue, now)!;
      selected.push(claim.botId);
      await scheduler.runClaim(claim, async () => {});
    }
    expect(selected).toEqual(['a', 'b', 'a', 'b', 'new', 'a', 'b', 'new', 'a']);
  });

  it('prioritizes configured direct addresses, rotates matching bots and preserves FIFO within that class', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('catch-all'), bot('a', { trigger: '@crew' }), bot('b', { trigger: '@crew' })];
    const queue = [message('ordinary'), message('direct-first', { text: 'Hello @CREW' }), message('direct-second', { text: '@crew hello' })];
    const first = scheduler.nextClaim(participants, queue, now)!;
    expect(first).toMatchObject({ botId: 'a', message: queue[1] });
    await scheduler.runClaim(first, async () => {});
    const second = scheduler.nextClaim(participants, queue, now)!;
    expect(second).toMatchObject({ botId: 'b', message: queue[1] });
  });

  it('retains per-bot triggers, expiry, answeredBy and notification bypass without changing platform routes', () => {
    const participant = bot('nova', { trigger: '@nova', expirySeconds: 10 });
    const scheduler = new ChatScheduler();
    const queue = [
      message('other-trigger', { text: '@james hi' }),
      message('expired', { text: '@nova hi', createdAt: new Date(now - 10_000).toISOString() }),
      message('answered', { text: '@nova hi', answeredBy: ['nova'] }),
      message('notification', { platform: 'youtube', replyRouteId: 'youtube-new-session', eventType: 'superChatEvent' }),
    ];
    expect(scheduler.nextClaim([participant], queue, now)).toMatchObject({
      botId: 'nova', message: { platform: 'youtube', replyRouteId: 'youtube-new-session', queueId: queue[3].queueId },
    });
    expect(chatPriority(participant, message('invalid', { createdAt: 'invalid', text: '@nova' }), now)).toBeUndefined();
    expect(chatPriority({ ...participant, expirySeconds: 0 }, message('valid', { text: '@nova' }), now)).toBeUndefined();
  });

  it('reserves one exact claim throughout permission, provider and playback waits', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('a'), bot('b')];
    const queue = [message('one')];
    const permission = deferred(), playback = deferred();
    const sends = vi.fn();
    const first = scheduler.nextClaim(participants, queue, now)!;
    const running = scheduler.runClaim(first, async () => {
      await permission.promise;
      sends(first.botId, first.message.replyRouteId);
      await playback.promise;
    });
    expect(scheduler.pending).toBe(true);
    expect(scheduler.nextClaim(participants, queue, now)).toBeUndefined();
    permission.resolve(); await Promise.resolve();
    expect(sends).toHaveBeenCalledExactlyOnceWith('a', 'route-one');
    expect(scheduler.nextClaim(participants, queue, now)).toBeUndefined();
    playback.resolve(); await running;
    expect(scheduler.pending).toBe(false);
    expect(scheduler.nextClaim(participants, queue, now)?.botId).toBe('b');
  });

  it('releases provider errors and never retries an already accepted message for that bot', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('a'), bot('b')];
    const queue = [message('one')];
    const first = scheduler.nextClaim(participants, queue, now)!;
    await expect(scheduler.runClaim(first, async () => {
      queue[0] = { ...queue[0], answeredBy: [first.botId] };
      throw new Error('Synthetic provider disconnected');
    })).rejects.toThrow('Synthetic provider disconnected');
    const second = scheduler.nextClaim(participants, queue, now)!;
    expect(second.botId).toBe('b');
    await scheduler.runClaim(second, async () => { queue[0] = { ...queue[0], answeredBy: ['a', 'b'] }; });
    expect(scheduler.nextClaim(participants, queue, now)).toBeUndefined();
  });

  it('releases a failed preflight and lets the next eligible bot proceed', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('a'), bot('b')];
    const queue = [message('one')];
    const claim = scheduler.nextClaim(participants, queue, now)!;
    await expect(scheduler.runClaim(claim, async () => { throw new Error('Permission status failed'); })).rejects.toThrow('Permission status failed');
    expect(scheduler.pending).toBe(false);
    expect(scheduler.nextClaim(participants, queue, now)?.botId).toBe('b');
  });

  it('lets manual/Stop preemption discard late permission results without releasing a newer turn', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('nova')];
    const queue = [message('one')];
    const old = scheduler.nextClaim(participants, queue, now)!;
    const permission = deferred(), playback = deferred();
    const send = vi.fn();
    const previous = scheduler.runClaim(old, async () => {
      await permission.promise;
      if (scheduler.isCurrentClaim(old)) send();
    });
    scheduler.cancel(); // The production manual-send and global Stop paths.
    const current = scheduler.nextClaim(participants, queue, now)!;
    const next = scheduler.runClaim(current, () => playback.promise);
    permission.resolve(); await previous;
    expect(send).not.toHaveBeenCalled();
    expect(scheduler.isCurrentClaim(current)).toBe(true);
    expect(scheduler.nextClaim(participants, queue, now)).toBeUndefined();
    playback.resolve(); await next;
    expect(scheduler.pending).toBe(false);
  });

  it('reserves manual priority across deferred preflight and playback, including repeated automatic polls', async () => {
    const scheduler = new ChatScheduler();
    const participants = [bot('a'), bot('b')];
    const queue = [message('one')];
    const autoPermission = deferred(), manualPermission = deferred(), playback = deferred();
    const sent: string[] = [];
    const automatic = scheduler.nextClaim(participants, queue, now)!;
    const automaticJob = scheduler.runClaim(automatic, async () => {
      await autoPermission.promise;
      if (scheduler.isCurrentClaim(automatic)) sent.push(`automatic:${automatic.botId}`);
    });
    // Same arbiter entry points as App.runMessage, before any permission await.
    const manual = scheduler.beginManual('b')!;
    const manualJob = (async () => {
      try {
        await manualPermission.promise;
        if (!scheduler.isCurrentManual(manual)) return;
        sent.push(`manual:${manual.botId}`);
        await playback.promise;
      } finally { scheduler.finishManual(manual); }
    })();
    expect(scheduler.beginManual('a')).toBeUndefined(); // Repeated operator click.
    for (let poll = 0; poll < 5; poll++) expect(scheduler.nextClaim(participants, queue, now + poll * 1000)).toBeUndefined();
    autoPermission.resolve(); await automaticJob;
    expect(sent).toEqual([]);
    expect(scheduler.pending).toBe(true); // Late automatic finally cannot release the manual owner.
    manualPermission.resolve(); await Promise.resolve();
    expect(sent).toEqual(['manual:b']);
    expect(scheduler.nextClaim(participants, queue, now)).toBeUndefined();
    playback.resolve(); await manualJob;
    expect(scheduler.pending).toBe(false);
    expect(scheduler.nextClaim(participants, queue, now)?.botId).toBe('b');
  });

  it('invalidates a stopped manual preflight and keeps a newer manual owner when its old finally arrives', async () => {
    const scheduler = new ChatScheduler();
    const old = scheduler.beginManual('nova')!;
    const permission = deferred();
    const send = vi.fn();
    const oldJob = (async () => {
      try {
        await permission.promise;
        if (scheduler.isCurrentManual(old)) send();
      } finally { scheduler.finishManual(old); }
    })();
    scheduler.cancel();
    const current = scheduler.beginManual('nova')!;
    permission.resolve(); await oldJob;
    expect(send).not.toHaveBeenCalled();
    expect(scheduler.isCurrentManual(current)).toBe(true);
    scheduler.cancelAutomatic(); // Pause automatic chat leaves this operator request alone.
    expect(scheduler.isCurrentManual(current)).toBe(true);
    scheduler.finishManual(current);
    expect(scheduler.pending).toBe(false);
  });

  it('never executes an already cancelled claim, and unrelated bot cancellation preserves the current owner', async () => {
    const scheduler = new ChatScheduler();
    const claim = scheduler.nextClaim([bot('a')], [message('one')], now)!;
    scheduler.cancel('b');
    expect(scheduler.isCurrentClaim(claim)).toBe(true);
    scheduler.cancel('a');
    const action = vi.fn(async () => {});
    await scheduler.runClaim(claim, action);
    expect(action).not.toHaveBeenCalled();
    expect(scheduler.pending).toBe(false);
  });

  it('keeps the 200-message bound and deduplicates only the exact platform route and message identity', () => {
    let queue: ChatMessage[] = [];
    for (let index = 0; index < 210; index++) queue = appendChatMessage(queue, message(`${index}`));
    expect(queue).toHaveLength(200);
    expect(queue[0].queueId).toBe(message('10').queueId);
    queue[0] = { ...queue[0], answeredBy: ['nova'] };
    const original = queue[0];
    queue = appendChatMessage(queue, message('10'));
    expect(queue).toHaveLength(200);
    expect(queue[0]).toBe(original);
    expect(queue[0].answeredBy).toEqual(['nova']);
    const nextRoute = message('10', { queueId: JSON.stringify(['twitch', 'new-route', '10']), replyRouteId: 'new-route' });
    queue = appendChatMessage(queue, nextRoute);
    expect(queue).toHaveLength(200);
    expect(queue.at(-1)).toBe(nextRoute);
  });
});
