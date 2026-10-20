import { describe, expect, it } from 'vitest';
import catalogue from './locales/life-runtime.json';
import { readFileSync } from 'node:fs';
import { lifeReasons, systemReasons, activityLabels, phaseLabels, currentLifeTurn, LifeSnapshots, nativeSchedule, parseLifeBot, parseLifeSnapshot, parseLifeTurn, publicActor } from './life-runtime';

const turnId = '12345678-1234-1234-1234-123456789abc';
const actor = () => ({ schemaVersion: 1, actorId: '_nova', worldId: 'desktop', revision: 2, simulationTime: 50, activity: { id: { actorId: '_nova', worldId: 'desktop', ordinal: 1 }, activity: 'rest', phase: 'completed' } });
const fixture = () => ({ version: 1, sequence: 3, system: { state: 'awake_unlocked', reason: 'ready', generation: 1, source: 'linux_logind' }, sessions: [{ botId: '_nova', mode: 'life', sessionId: 'session', revision: 4, active: true, paused: false, reason: 'ready', speech: { eligible: true, inFlight: true, turnId, remainingSeconds: 17, attempts: 7, limit: 12, budgetLimited: false, waitingForBot: null }, actor: actor() }] });
const event = (kind = 'started') => ({ kind, botId: '_nova', sessionId: 'session', revision: 4, turnId, reply: null });

describe('Native Life public projection and exact turn ownership', () => {
  it('uses actual native counters without synthesizing a renderer schedule', () => {
    const value = parseLifeSnapshot(fixture());
    expect(nativeSchedule(value.sessions[0])).toEqual({ ...value.sessions[0].speech, waitingForBot: undefined });
    expect(nativeSchedule(value.sessions[0])).not.toHaveProperty('dueAt');
    expect(nativeSchedule(undefined)).toBeUndefined();
    expect(currentLifeTurn(parseLifeTurn(event()), value)).toBe(true);
    for (const change of [{ botId: 'other' }, { sessionId: 'old' }, { revision: 3 }, { turnId: 'old-turn' }]) expect(currentLifeTurn(parseLifeTurn({ ...event(), ...change }), value)).toBe(false);
  });
  it('accepts equal-sequence countdown refreshes but rejects older native snapshots', () => {
    const ordered = new LifeSnapshots();
    expect(ordered.accept(fixture())?.sequence).toBe(3);
    const fresh = fixture(); fresh.sessions[0].speech.remainingSeconds = 4;
    expect(ordered.accept(fresh)?.sessions[0].speech.remainingSeconds).toBe(4);
    expect(ordered.accept({ ...fixture(), sequence: 2 })).toBeUndefined();
  });
  it('bounds identifiers, unknown schemas, counters, enums and malformed availability', () => {
    for (const bad of [NaN, Infinity, -1, .5, Number.MAX_SAFE_INTEGER + 1, '7']) {
      const value = fixture(); value.sessions[0].speech.attempts = bad as number;
      expect(() => parseLifeSnapshot(value)).toThrow('ungültige Daten');
    }
    for (const change of [{ version: 2 }, { sessions: Array(65).fill(fixture().sessions[0]) }, { system: { ...fixture().system, state: 'awakeUnlocked' } }, { system: { ...fixture().system, state: 'unknown' } }]) expect(() => parseLifeSnapshot({ ...fixture(), ...change })).toThrow();
    for (const botId of ['../nova', 'n\n', 'é', 'a'.repeat(97)]) {
      const value = fixture(); value.sessions[0].botId = botId;
      expect(() => parseLifeSnapshot(value)).toThrow();
    }
    const duplicate = fixture(); duplicate.sessions.push(duplicate.sessions[0]); expect(() => parseLifeSnapshot(duplicate)).toThrow();
    const mismatch = fixture(); mismatch.sessions[0].speech.turnId = null as unknown as string; expect(() => parseLifeSnapshot(mismatch)).toThrow();
    expect(() => publicActor({ ...actor(), schemaVersion: 2 })).toThrow();
    expect(() => publicActor({ ...actor(), activity: { ...actor().activity, id: { actorId: 'other', worldId: 'desktop', ordinal: 1 } } })).toThrow();
  });
  it('keeps local Life available without inventing a model or actor and strips private fields', () => {
    expect(parseLifeBot({ availability: 'not_initialized', private: 'secret' })).toEqual({ availability: 'not_initialized' });
    expect(parseLifeBot({ availability: 'unavailable' })).toEqual({ availability: 'unavailable' });
    expect(parseLifeBot({ availability: 'ready', actor: null })).toEqual({ availability: 'ready', actor: null });
    const value = fixture();
    const output = parseLifeSnapshot({ ...value, prompts: 'PRIVATE', sessions: [{ ...value.sessions[0], grants: 'PRIVATE', actor: { ...actor(), needs: 'PRIVATE' } }] });
    expect(JSON.stringify(output)).not.toContain('PRIVATE');
    const unknown = fixture(); unknown.system = { ...unknown.system, state: 'unknown', reason: 'observation_unavailable' }; unknown.sessions[0].reason = 'model_unavailable';
    expect(parseLifeSnapshot(unknown).sessions[0].paused).toBe(false);
  });
  it('projects ordinary replies without audio or provider secrets and rejects wrong ownership', () => {
    const value = { ...event('reply'), reply: { id: turnId, botId: '_nova', text: 'Hello', speechText: '[happy] Hello', voice: { provider: 'elevenlabs', voiceId: 'voice', language: 'en', enabled: true, rate: 1, volume: .8, pitch: 1, apiKey: 'PRIVATE' }, emotes: [{ action: 'Wave', duration: 2, private: 'PRIVATE' }], audio: 'PRIVATE', prompt: 'PRIVATE' } };
    const parsed = parseLifeTurn(value);
    expect(parsed.reply?.text).toBe('Hello'); expect(parsed.reply?.speechText).toBe('[happy] Hello'); expect(JSON.stringify(parsed)).not.toContain('PRIVATE');
    for (const change of [{ id: 'old' }, { botId: 'other' }, { emotes: Array(65).fill({ action: 'Wave' }) }]) expect(() => parseLifeTurn({ ...value, reply: { ...value.reply, ...change } })).toThrow();
    expect(parseLifeTurn({ ...event('reply'), reply: { skipped: true } }).reply?.skipped).toBe(true);
  });
});


describe('Life controls and four-language operator guidance',()=>{
  it('translates every controlled label and preserves interpolation placeholders',()=>{
    for(const language of ['en','fr','es'] as const){
      const messages=catalogue[language] as Record<string,string>;
      for(const key of [...Object.values(lifeReasons),...Object.values(systemReasons),...Object.values(activityLabels),...Object.values(phaseLabels)])expect(messages[key]?.trim(),`${language}: ${key}`).toBeTruthy();
      for(const [source,translated] of Object.entries(messages))expect([...translated.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort()).toEqual([...source.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort());
    }
  });
  it('keeps provider dispatch native and wraps every operator plugin file dialog',()=>{
    const app=readFileSync(new URL('../App.svelte',import.meta.url),'utf8');
    expect(app).not.toContain('new AutopilotScheduler');expect(app).not.toMatch(/call\(['"]send_autopilot/);
    expect(app).toContain("if(source==='autopilot'");
    expect(app).toContain('await ensureOperatorBarrier()');expect(app).toContain('beforeStart');
    for(const file of ['../App.svelte','../components/ExtensionsPanel.svelte'])expect(readFileSync(new URL(file,import.meta.url),'utf8')).not.toContain("from '@tauri-apps/plugin-dialog'");
  });
});
