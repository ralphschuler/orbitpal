import { afterEach, describe, expect, it, vi } from 'vitest';
import { AutopilotScheduler } from './autopilot';
import { DesktopMotionController } from './desktop-motion';
import { sampleAmbientMotion } from './autopilot-motion';
import { ExternalEmotePlayback } from './emote-playback';
import { foregroundSoundPlaying, renderedModelSoundActivity, SoundPlaybackReports, type SoundActivity } from './sound-playback';

afterEach(() => vi.useRealTimers());

describe('actual sound ownership and autonomous quiet time', () => {
  it('keeps microphone/update audio busy while walking is exempt, but foreground overlap still blocks', () => {
    vi.useFakeTimers();
    const actual = vi.fn(), blocking = vi.fn();
    const receiver = new ExternalEmotePlayback(actual, blocking);
    let serial = 0;
    const sender = new SoundPlaybackReports(() => `id-${++serial}`);
    const accept = (foreground: boolean, walking: boolean) => {
      const reports = sender.update(foreground, walking);
      reports.forEach(report => receiver.accept({ ...report, botId: 'nova', output: 'overlay' }));
      return reports;
    };
    expect(accept(false, true)).toEqual([{ id: 'id-1', playing: true, activity: 'ambient' }]);
    expect(actual.mock.calls).toEqual([[true]]); expect(blocking).not.toHaveBeenCalled();
    vi.advanceTimersByTime(12_000); accept(false, true);
    expect(accept(true, true)).toEqual([
      { id: 'id-2', playing: true, activity: 'foreground' },
      { id: 'id-1', playing: true, activity: 'ambient' },
    ]);
    expect(blocking.mock.calls).toEqual([[true]]); expect(actual.mock.calls).toEqual([[true]]);
    expect(accept(false, true)).toContainEqual({ id: 'id-2', playing: false, activity: 'foreground' });
    expect(blocking.mock.calls).toEqual([[true], [false]]); expect(actual.mock.calls).toEqual([[true]]);
    accept(false, false); expect(actual.mock.calls).toEqual([[true], [false]]);
    expect(accept(false, false)).toEqual([]); expect(vi.getTimerCount()).toBe(0);
    expect(accept(true, false)[0].id).toBe('id-3');
    receiver.destroy();
  });

  it('starts a replacement kind before ending the old lease and never briefly unlocks actual audio', () => {
    vi.useFakeTimers();
    const actual = vi.fn(), blocking = vi.fn();
    const receiver = new ExternalEmotePlayback(actual, blocking);
    let id = 0; const sender = new SoundPlaybackReports(() => `id-${++id}`);
    const accept = (foreground: boolean, walking: boolean) => sender.update(foreground, walking)
      .forEach(report => receiver.accept({ ...report, botId: 'nova', output: 'overlay' }));
    accept(true, false); accept(false, true); accept(true, false);
    expect(actual.mock.calls).toEqual([[true]]);
    expect(blocking.mock.calls).toEqual([[true], [false], [true]]);
    receiver.destroy(); expect(actual.mock.calls).toEqual([[true], [false]]);
  });

  it('never lets wrong activity, bot, output, or stale ID release/reclassify an exact lease', () => {
    vi.useFakeTimers();
    const actual = vi.fn(), blocking = vi.fn();
    const receiver = new ExternalEmotePlayback(actual, blocking);
    const base = { botId: 'nova', output: 'obs', id: 'foreground', playing: true };
    receiver.accept(base); // Legacy reports always block autonomous speech.
    receiver.accept({ ...base, activity: 'ambient' });
    for (const changed of [{activity:'ambient'}, {botId:'other'}, {output:'overlay'}, {id:'old'}]) {
      receiver.accept({ ...base, ...changed, playing: false } as any);
    }
    receiver.accept({ ...base, activity: 'unknown' } as any);
    expect(actual.mock.calls).toEqual([[true]]); expect(blocking.mock.calls).toEqual([[true]]);
    receiver.accept({ ...base, playing: false });
    expect(blocking.mock.calls).toEqual([[true], [false]]);
    const ambient = { ...base, id: 'walk', activity: 'ambient' as const };
    receiver.accept(ambient);
    receiver.accept({ ...ambient, playing: false, activity: undefined });
    expect(actual.mock.calls).toEqual([[true], [false], [true]]);
    vi.advanceTimersByTime(16_000);
    expect(actual.mock.calls).toEqual([[true], [false], [true], [false]]);
    expect(blocking.mock.calls).toEqual([[true], [false]]);
    receiver.destroy();
  });

  it('expires each kind independently and ignores a queued old timer after renewal', () => {
    vi.useFakeTimers();
    const actual = vi.fn(), blocking = vi.fn(), timers = vi.spyOn(globalThis, 'setTimeout');
    const receiver = new ExternalEmotePlayback(actual, blocking);
    const ambient = { botId: 'nova', output: 'obs', id: 'walk', playing: true, activity: 'ambient' as const };
    receiver.accept(ambient);
    const old = timers.mock.calls[0][0] as () => void;
    receiver.accept({ ...ambient, id: 'emote', activity: 'foreground' });
    vi.advanceTimersByTime(8000); receiver.accept(ambient); old();
    vi.advanceTimersByTime(8000);
    expect(actual.mock.calls).toEqual([[true]]); expect(blocking.mock.calls).toEqual([[true], [false]]);
    vi.advanceTimersByTime(8000); expect(actual.mock.calls).toEqual([[true], [false]]);
    receiver.destroy(); timers.mockRestore();
  });

  it('does not starve spontaneous turns on real desktop walking, while explicit model emotes still reset quiet time', () => {
    let now = 1000;
    const scheduler = new AutopilotScheduler({ now: () => now, random: () => 0 });
    scheduler.sync([{ botId: 'nova', seed: 'sound-session', config: { speak: true, minSeconds: 15, maxSeconds: 15, maxTurnsPerHour: 120 } }]);
    const motion = new DesktopMotionController();
    let attempts = 0, walkingFrames = 0;
    for (let frame = 0; frame < 37_500; frame++) {
      now += 16;
      const position = motion.sample({ seed: 'audio-step-fixture', startedAt: 1000, now, delta: .016,
        viewport: { width: 1200, height: 800 }, figure: { width: 180, height: 300 }, margin: 24, paused: false });
      if (position.walking) walkingFrames++;
      const blocked = foregroundSoundPlaying(false, position.walking, 'ambient');
      const claim = scheduler.nextClaim(blocked);
      if (claim) { attempts++; scheduler.finish(claim.botId); }
    }
    expect(walkingFrames).toBeGreaterThan(1000); expect(attempts).toBeGreaterThan(20);
    // The same actual sound, classified as an emote or legacy track, blocks.
    for (const activity of ['foreground', undefined] as (SoundActivity | undefined)[]) {
      now += 30_000;
      expect(scheduler.nextClaim(foregroundSoundPlaying(false, true, activity!))).toBeUndefined();
      now += 14_999; expect(scheduler.nextClaim()).toBeUndefined();
      now += 1; const claim = scheduler.nextClaim()!; expect(claim.botId).toBe('nova'); scheduler.finish(claim.botId);
    }
    expect(foregroundSoundPlaying(true, true, 'ambient')).toBe(true);
  });

  it.each([false, true])('does not let automatic gesture sounds starve a 120s speech interval (wander=%s)', wander => {
    let now = 1000;
    const current = new AutopilotScheduler({ now: () => now, random: () => 0 });
    const old = new AutopilotScheduler({ now: () => now, random: () => 0 });
    const candidate = { botId: 'nova', seed: 'ambient-audio', config: { speak: true, minSeconds: 120, maxSeconds: 180, maxTurnsPerHour: 50 } };
    current.sync([candidate]); old.sync([candidate]);
    const motion = { active: true, wander, gestures: true, startedAt: now, seed: candidate.seed };
    let attempts = 0, oldAttempts = 0, gestures = 0, lastGesture = '';
    for (let frame = 0; frame < 24_000; frame++) {
      now += 50;
      const rendered = sampleAmbientMotion(motion, now);
      if (rendered.gesture && rendered.gesture.key !== lastGesture) { gestures++; lastGesture = rendered.gesture.key; }
      // A bounded 0.5s embedded sound at the gesture's start already suffices
      // to reproduce starvation; the entire gesture need not be audible.
      const sound = !!rendered.gesture && rendered.gesture.elapsed < .5;
      const activity = renderedModelSoundActivity(!!rendered.gesture, rendered.gesture ? 'ambient' : undefined);
      const claim = current.nextClaim(foregroundSoundPlaying(false, sound, activity));
      if (claim) { attempts++; current.finish(claim.botId); }
      const oldClaim = old.nextClaim(foregroundSoundPlaying(false, sound, 'foreground'));
      if (oldClaim) { oldAttempts++; old.finish(oldClaim.botId); }
    }
    expect(gestures).toBeGreaterThan(20); expect(oldAttempts).toBe(0); expect(attempts).toBeGreaterThanOrEqual(9);
    // Manual/reply ownership, including the very same Walk/Wave action, wins.
    expect(foregroundSoundPlaying(false, true, renderedModelSoundActivity(true, 'manual'))).toBe(true);
    expect(foregroundSoundPlaying(false, true, renderedModelSoundActivity(true, undefined))).toBe(true);
    expect(foregroundSoundPlaying(true, true, renderedModelSoundActivity(true, 'ambient'))).toBe(true);
  });
});
