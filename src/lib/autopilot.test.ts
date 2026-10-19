import { describe, expect, it } from 'vitest';
import { AutopilotScheduler, type AutopilotCandidate } from './autopilot';

function fixture(random = () => 0) {
  let now = 0;
  const scheduler = new AutopilotScheduler({ now: () => now, random });
  return { scheduler, at: (milliseconds: number) => { now = milliseconds; } };
}

function bot(botId = 'nova', changes: Partial<AutopilotCandidate> = {}): AutopilotCandidate {
  return { botId, seed: 'session-1', config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 12 }, ...changes };
}

describe('autopilot quiet-time scheduling', () => {
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
