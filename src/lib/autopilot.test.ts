import { describe, expect, it } from 'vitest';
import { AutopilotScheduler, type AutopilotCandidate } from './autopilot';
import { speechBlocker, speechBlockerMessages, speechStatus } from './autopilot-status';

function fixture(random = () => 0) {
  let now = 0;
  const scheduler = new AutopilotScheduler({ now: () => now, random });
  return { scheduler, at: (milliseconds: number) => { now = milliseconds; } };
}

function bot(botId = 'nova', changes: Partial<AutopilotCandidate> = {}): AutopilotCandidate {
  return { botId, seed: 'session-1', config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 12 }, ...changes };
}

describe('autopilot quiet-time scheduling', () => {
  it('discards old permission results after pause/restart without dispatching or disturbing the new same-bot claim', async () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(15_000); const oldClaim = scheduler.nextClaim()!;
    const dispatched: string[] = [];
    let permission!: (granted: boolean) => void;
    const oldAttempt = scheduler.runClaim(oldClaim, async () => {
      const granted = await new Promise<boolean>(resolve => permission = resolve);
      if (!granted || !scheduler.isCurrentClaim(oldClaim)) return;
      dispatched.push('obsolete');
    });
    scheduler.stop('nova'); scheduler.sync([bot('nova', {seed:'restarted'})]);
    expect(scheduler.isCurrentClaim(oldClaim)).toBe(false);
    at(30_000); const newClaim = scheduler.nextClaim()!;
    let finishNew!: () => void;
    const newAttempt = scheduler.runClaim(newClaim, () => new Promise<void>(resolve => finishNew = resolve));
    expect(scheduler.isCurrentClaim({...newClaim})).toBe(false);
    expect(scheduler.isCurrentClaim(newClaim)).toBe(true);
    permission(true); await oldAttempt;
    expect(dispatched).toEqual([]);
    expect(scheduler.snapshot('nova')).toMatchObject({inFlight:true,attempts:2});
    at(45_000); expect(scheduler.nextClaim()).toBeUndefined();
    finishNew(); await newAttempt;
    expect(scheduler.isCurrentClaim(newClaim)).toBe(false);
    at(59_999); expect(scheduler.nextClaim()).toBeUndefined();
    at(60_000); expect(scheduler.nextClaim()?.botId).toBe('nova');
  });

  it('projects exact pending claims into read-only owner and waiting-bot snapshots', async () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot('nova'), bot('james')]);
    at(15_000); const claim = scheduler.nextClaim()!;
    const other = claim.botId === 'nova' ? 'james' : 'nova';
    let complete!: () => void;
    const pending = scheduler.runClaim(claim, () => new Promise<void>(resolve => complete = resolve));
    expect(Object.isFrozen(claim)).toBe(true);
    expect(scheduler.snapshot(claim.botId)).toMatchObject({inFlight:true,waitingForBot:undefined,attempts:1});
    const waiting = scheduler.snapshot(other)!;
    expect(waiting).toMatchObject({inFlight:false,waitingForBot:claim.botId,attempts:0});
    waiting.waitingForBot = 'changed';
    expect(scheduler.snapshot(other)?.waitingForBot).toBe(claim.botId);
    at(20_000); expect(scheduler.nextClaim()).toBeUndefined();
    complete(); await pending;
    expect(scheduler.snapshot(claim.botId)?.inFlight).toBe(false);
    expect(scheduler.snapshot(other)?.waitingForBot).toBeUndefined();
  });

  it('uses the same permission blocker for displayed status and the full quiet period', () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]); at(15_000);
    const blocked = speechBlocker({permission:true});
    expect(scheduler.nextClaim(!!blocked)).toBeUndefined();
    expect(scheduler.snapshot('nova')).toMatchObject({inFlight:false,attempts:0,dueAt:30_000});
    expect(speechStatus({active:true,speak:true,outputAvailable:true,modelConfigured:true,blocked,schedule:scheduler.snapshot('nova')}))
      .toEqual({message:speechBlockerMessages.permission,waiting:true});
    at(29_999); expect(scheduler.nextClaim()).toBeUndefined();
    at(30_000); expect(scheduler.nextClaim()?.botId).toBe('nova');
  });

  it('reads countdowns without claiming turns, spending budgets or drawing another deadline', () => {
    let samples=0;
    const { scheduler, at }=fixture(()=>{samples++;return 0.5;});
    scheduler.sync([bot('nova',{config:{speak:true,minSeconds:20,maxSeconds:30,maxTurnsPerHour:1}})]);
    for(let second=0;second<=25;second++){
      at(second*1000);
      expect(scheduler.snapshot('nova')).toMatchObject({dueAt:25_000,remainingSeconds:25-second,attempts:0,inFlight:false,budgetLimited:false});
    }
    expect(samples).toBe(1);expect(scheduler.next()).toBe('nova');
    expect(scheduler.snapshot('nova')).toMatchObject({inFlight:true,attempts:1,budgetLimited:true});
  });

  it('does not let a display read mutate the scheduling clock or internal state', () => {
    const { scheduler, at }=fixture();scheduler.sync([bot()]);
    at(5000);const view=scheduler.snapshot('nova')!;view.eligible=false;view.dueAt=0;
    at(1000);scheduler.activity(); // Snapshot did not advance the scheduler's clock to 5000.
    expect(scheduler.snapshot('nova')).toMatchObject({eligible:true,dueAt:16_000,remainingSeconds:15});
    expect(scheduler.snapshot('missing')).toBeUndefined();
    at(16_000);expect(scheduler.next()).toBe('nova');
  });

  it('shows the real rolling-budget release time, including reduced limits', () => {
    const { scheduler, at }=fixture();
    const candidate=bot('nova',{config:{speak:true,minSeconds:15,maxSeconds:15,maxTurnsPerHour:2}});
    scheduler.sync([candidate]);at(15_000);scheduler.next();scheduler.finish('nova');
    at(30_000);scheduler.next();scheduler.finish('nova');
    scheduler.sync([{...candidate,config:{...candidate.config,maxTurnsPerHour:1}}]);
    expect(scheduler.snapshot('nova')).toMatchObject({budgetLimited:true,dueAt:3_630_000,attempts:2,limit:1,remainingSeconds:3600});
    at(3_615_000);expect(scheduler.snapshot('nova')).toMatchObject({budgetLimited:true,attempts:1,remainingSeconds:15});
    expect(scheduler.next()).toBeUndefined();
    at(3_630_000);expect(scheduler.snapshot('nova')).toMatchObject({budgetLimited:false,attempts:0,remainingSeconds:0});
    expect(scheduler.next()).toBe('nova');
  });

  it('distinguishes its own pending attempt from another bot and reports eligibility', () => {
    const { scheduler, at }=fixture();scheduler.sync([bot('a'),bot('b'),bot('c',{eligible:false})]);
    at(15_000);expect(scheduler.next()).toBe('a');
    expect(scheduler.snapshot('a')).toMatchObject({inFlight:true,waitingForBot:undefined,attempts:1});
    expect(scheduler.snapshot('b')).toMatchObject({inFlight:false,waitingForBot:'a',attempts:0});
    expect(scheduler.snapshot('c')).toMatchObject({eligible:false,waitingForBot:'a',attempts:0});
    scheduler.finish('a');expect(scheduler.snapshot('b')?.waitingForBot).toBeUndefined();
    scheduler.stop();expect(scheduler.snapshot('a')).toBeUndefined();
  });

  it('releases a cancelled async permission preflight and schedules another quiet-period turn', async () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(15_000); const claim = scheduler.nextClaim()!;
    let complete!: (granted: boolean) => void; let sent = false;
    const action = scheduler.runClaim(claim, async () => {
      const granted = await new Promise<boolean>(resolve => complete = resolve);
      if (!granted) return; // Changed configuration/manual preemption cancels before send.
      sent = true;
    });
    at(16_000); complete(false); await action;
    expect(sent).toBe(false);
    at(30_999); expect(scheduler.nextClaim()).toBeUndefined();
    at(31_000); expect(scheduler.nextClaim()?.botId).toBe('nova');
  });

  it('releases a failed asynchronous preflight without refunding its hourly attempt', async () => {
    const { scheduler, at } = fixture();
    scheduler.sync([bot('nova', { config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 1 } })]);
    at(15_000); const claim = scheduler.nextClaim()!;
    await expect(scheduler.runClaim(claim, async () => { throw new Error('Status disconnected'); })).rejects.toThrow('Status disconnected');
    at(30_000); expect(scheduler.nextClaim()).toBeUndefined();
    at(3_615_000); expect(scheduler.nextClaim()?.botId).toBe('nova');
  });

  it('cannot let stale preflight completion release a new same-bot claim after Stop and restart', async () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(15_000); const oldClaim = scheduler.nextClaim()!;
    let finishOld!: () => void;
    const oldAction = scheduler.runClaim(oldClaim, () => new Promise<void>(resolve => finishOld = resolve));
    scheduler.stop(); scheduler.sync([bot('nova', { seed: 'restarted' })]);
    at(30_000); const newClaim = scheduler.nextClaim()!;
    expect(newClaim.botId).toBe(oldClaim.botId); expect(newClaim.revision).not.toBe(oldClaim.revision);
    let finishNew!: () => void;
    const newAction = scheduler.runClaim(newClaim, () => new Promise<void>(resolve => finishNew = resolve));
    finishOld(); await oldAction;
    at(45_000); expect(scheduler.nextClaim()).toBeUndefined(); // New turn still owns the lease.
    finishNew(); await newAction;
    at(60_000); expect(scheduler.nextClaim()?.botId).toBe('nova');
  });

  it('never runs an already invalidated claim callback', async () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(15_000); const claim = scheduler.nextClaim()!; scheduler.stop();
    let called = false; await scheduler.runClaim(claim, async () => { called = true; });
    expect(called).toBe(false);
  });

  it('waits on initial start and does not reset the deadline during unchanged polling', () => {
    const { scheduler, at } = fixture();
    scheduler.sync([bot()]); expect(scheduler.next()).toBeUndefined();
    for (let second = 1; second < 15; second++) {
      at(second * 1000); scheduler.sync([bot()]); expect(scheduler.next()).toBeUndefined();
    }
    at(15_000); scheduler.sync([bot()]); expect(scheduler.next()).toBe('nova');
  });

  it.each([[0, 20_000], [0.5, 25_000], [1, 30_000]])('uses random %s within the configured interval', (sample, expected) => {
    const { scheduler, at } = fixture(() => sample);
    scheduler.sync([bot('nova', { config: { speak: true, minSeconds: 20, maxSeconds: 30, maxTurnsPerHour: 12 } })]);
    at(expected - 1); expect(scheduler.next()).toBeUndefined();
    at(expected); expect(scheduler.next()).toBe('nova');
  });

  it('restarts the full quiet period after activity, prolonged blocking and final audio completion', () => {
    const { scheduler, at } = fixture();
    scheduler.sync([bot(), bot('second')]);
    at(15_000); scheduler.activity();
    at(29_999); expect(scheduler.next()).toBeUndefined();
    at(300_000); expect(scheduler.next(true)).toBeUndefined();
    at(310_000); expect(scheduler.next(true)).toBeUndefined();
    at(324_999); expect(scheduler.next()).toBeUndefined();
    at(325_000); expect(scheduler.next()).toBe('nova');
    at(400_000); expect(scheduler.next()).toBeUndefined();
    scheduler.finish('nova');
    at(414_999); expect(scheduler.next()).toBeUndefined();
    at(415_000); expect(scheduler.next()).toBe('second');
  });

  it('never overlaps claims or releases one because a different bot finishes', () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot(), bot('second')]);
    at(15_000); expect(scheduler.next()).toBe('nova');
    at(30_000); scheduler.finish('second'); expect(scheduler.next()).toBeUndefined();
    scheduler.finish('nova');
    at(44_999); expect(scheduler.next()).toBeUndefined();
    at(45_000); expect(scheduler.next()).toBe('second');
  });

  it('does not charge blocked polls against the hourly attempt budget', () => {
    const { scheduler, at } = fixture();
    scheduler.sync([bot('nova', { config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 1 } })]);
    for (let second = 15; second <= 120; second++) { at(second * 1000); expect(scheduler.next(true)).toBeUndefined(); }
    at(135_000); expect(scheduler.next()).toBe('nova');
  });

  it('keeps the claimed turn during a seed change and waits from its final audio completion', () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(15_000); expect(scheduler.next()).toBe('nova');
    at(20_000); scheduler.sync([bot('nova', { seed: 'session-2' })]);
    at(60_000); expect(scheduler.next()).toBeUndefined();
    scheduler.finish('nova');
    at(74_999); expect(scheduler.next()).toBeUndefined();
    at(75_000); expect(scheduler.next()).toBe('nova');
  });

  it('starts changed seeds/configurations and newly eligible bots with a fresh interval', () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(14_000); scheduler.sync([bot('nova', { seed: 'session-2' })]);
    at(15_000); expect(scheduler.next()).toBeUndefined();
    at(28_000); scheduler.sync([bot('nova', { seed: 'session-2', eligible: false })]);
    at(100_000); expect(scheduler.next()).toBeUndefined();
    scheduler.sync([bot('nova', { seed: 'session-2' })]);
    at(114_999); expect(scheduler.next()).toBeUndefined();
    at(115_000); expect(scheduler.next()).toBe('nova'); scheduler.finish('nova');
    at(120_000); scheduler.sync([bot('nova', { seed: 'session-2', config: { speak: true, minSeconds: 30, maxSeconds: 30, maxTurnsPerHour: 12 } })]);
    at(149_999); expect(scheduler.next()).toBeUndefined();
    at(150_000); expect(scheduler.next()).toBe('nova');
  });

  it('counts failed attempts over a rolling hour through stop/start and seed changes', () => {
    const { scheduler, at } = fixture();
    const limited = bot('nova', { config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 1 } });
    scheduler.sync([limited]); at(15_000); expect(scheduler.next()).toBe('nova');
    scheduler.finish('nova'); // A provider failure consumes the claimed attempt too.
    scheduler.stop(); scheduler.sync([{ ...limited, seed: 'restarted' }]);
    at(30_000); expect(scheduler.next()).toBeUndefined();
    at(3_614_999); expect(scheduler.next()).toBeUndefined();
    at(3_615_000); expect(scheduler.next()).toBe('nova');
  });

  it('keeps budgets separate and releases a reduced rolling limit only after enough attempts expire', () => {
    const { scheduler, at } = fixture();
    const limited = bot('nova', { config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 2 } });
    scheduler.sync([limited]); at(15_000); expect(scheduler.next()).toBe('nova'); scheduler.finish('nova');
    at(30_000); expect(scheduler.next()).toBe('nova'); scheduler.finish('nova');
    scheduler.sync([{ ...limited, config: { ...limited.config, maxTurnsPerHour: 1 } }, bot('second')]);
    at(45_000); expect(scheduler.next()).toBe('second'); scheduler.finish('second');
    at(3_615_000); scheduler.sync([{ ...limited, config: { ...limited.config, maxTurnsPerHour: 1 } }]);
    expect(scheduler.next()).toBeUndefined();
    at(3_630_000); expect(scheduler.next()).toBe('nova');
  });

  it('serves least recently claimed bots fairly, including newly added bots', () => {
    const { scheduler, at } = fixture();
    scheduler.sync([bot('b'), bot('a')]); at(15_000); expect(scheduler.next()).toBe('a'); scheduler.finish('a');
    at(30_000); expect(scheduler.next()).toBe('b'); scheduler.finish('b');
    scheduler.sync([bot('a'), bot('b'), bot('new')]);
    at(45_000); expect(scheduler.next()).toBe('new'); scheduler.finish('new');
    at(60_000); expect(scheduler.next()).toBe('a');
  });

  it('removes disabled/absent candidates without releasing existing work during sync', () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(15_000); expect(scheduler.next()).toBe('nova');
    scheduler.sync([bot('second')]); at(60_000); expect(scheduler.next()).toBeUndefined();
    scheduler.finish('nova'); at(75_000); expect(scheduler.next()).toBe('second');
    scheduler.stop('second');
    scheduler.sync([bot('second', { config: { speak: false, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 12 } })]);
    at(120_000); expect(scheduler.next()).toBeUndefined();
  });

  it('cancels one session without clearing its attempt budget or other bots', () => {
    const { scheduler, at } = fixture();
    const limited = bot('nova', { config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 1 } });
    scheduler.sync([limited, bot('second')]); at(15_000); expect(scheduler.next()).toBe('nova');
    at(40_000); scheduler.stop('nova'); scheduler.sync([limited, bot('second')]);
    at(54_999); expect(scheduler.next()).toBeUndefined();
    at(55_000); expect(scheduler.next()).toBe('second'); scheduler.finish('second');
    scheduler.sync([limited]); at(70_000); expect(scheduler.next()).toBeUndefined();
  });

  it('uses safe defaults and caps for invalid numeric configuration and random values', () => {
    const { scheduler, at } = fixture(() => NaN);
    scheduler.sync([bot('nova', { config: { speak: true, minSeconds: NaN, maxSeconds: Infinity, maxTurnsPerHour: NaN } })]);
    at(119_999); expect(scheduler.next()).toBeUndefined();
    at(120_000); expect(scheduler.next()).toBe('nova');
    scheduler.stop();
    scheduler.sync([bot('nova', { config: { speak: true, minSeconds: -10, maxSeconds: -100, maxTurnsPerHour: 0 } })]);
    at(134_999); expect(scheduler.next()).toBeUndefined();
    at(135_000); expect(scheduler.next()).toBeUndefined(); // Prior attempt still exhausts capped budget of one.
    scheduler.sync([bot('second', { config: { speak: true, minSeconds: 5000, maxSeconds: 1, maxTurnsPerHour: 10000 } })]);
    at(3_734_999); expect(scheduler.next()).toBeUndefined();
    at(3_735_000); expect(scheduler.next()).toBe('second');
  });

  it('requires speak to be strictly true and ignores repeated candidate IDs', () => {
    const { scheduler, at } = fixture();
    scheduler.sync([bot('nova', { config: { speak: 'true' as unknown as boolean, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 12 } }), bot('nova'), bot('')]);
    at(15_000); expect(scheduler.next()).toBeUndefined();
  });

  it('does not expire budgets or shorten a pause if the wall clock moves backwards or becomes invalid', () => {
    const { scheduler, at } = fixture(); scheduler.sync([bot()]);
    at(14_000); scheduler.activity(); at(1_000); scheduler.activity();
    at(NaN); expect(scheduler.next()).toBeUndefined();
    at(28_999); expect(scheduler.next()).toBeUndefined();
    at(29_000); expect(scheduler.next()).toBe('nova');
  });
});
