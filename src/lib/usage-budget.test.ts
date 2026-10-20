import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { catalogues, translate } from './i18n';
import { draftChanged, draftLimits, getUsageBudget, limitFields, limitsDraft, parseUsageBudget, parseUsageLimits, setUsageLimits, usageDenials, type UsageLimits, type UsageTotals, usageLimitDefaults } from './usage-budget';

const limits: UsageLimits = { requestAccounting: 'dispatch', dailyDialogRequests: 100, dailyTtsCharacters: 20_000, dailyHindsightRequests: 200, dailyImageCount: 100, dailyImageBytes: 52_428_800, hardTokenLimit: null };
const totals: UsageTotals = { dialogRequests: 2, ttsRequests: 1, ttsCharacters: 27, hindsightRecallRequests: 1, hindsightRetainRequests: 1, imageCount: 2, imageBytes: 1024, chargedTokens: 900, observedTokens: 80, reservedTokens: 500, unknownTokenRequests: 1, pendingRequests: 1 };
const fixture = () => ({ limits: { ...limits }, day: '2026-10-20', clockAdjusted: false, total: { ...totals }, bots: [{ botId: '_nova', total: { ...totals } }], lastDenial: null });

describe('Shared daily usage UI contract', () => {
  it('keeps native counters separate, preserving conservative and observed values', () => {
    const value = parseUsageBudget(fixture());
    expect(value.total.chargedTokens).toBe(900);
    expect(value.total.reservedTokens).toBe(500);
    expect(value.total.observedTokens).toBe(80);
    expect(value.total.unknownTokenRequests).toBe(1);
    expect(value.bots[0].botId).toBe('_nova');
  });
  it('rejects malformed native counters and unavailable future denial categories with fixed errors', () => {
    for (const bad of [NaN, Infinity, -1, 0.5, Number.MAX_SAFE_INTEGER + 1, '3', null]) {
      const value = fixture(); value.total.dialogRequests = bad as number;
      expect(() => parseUsageBudget(value)).toThrow('Verbrauchsdaten sind ungültig.');
    }
    for (const change of [{ day: '2026-02-30' }, { day: 'not-a-day' }, { clockAdjusted: 'false' }, { lastDenial: 'unrecognized-secret-marker' }]) {
      expect(() => parseUsageBudget({ ...fixture(), ...change })).toThrow('Verbrauchsdaten sind ungültig.');
      try { parseUsageBudget({ ...fixture(), ...change }); } catch (error) { expect(String(error)).not.toContain('secret-marker'); }
    }
  });
  it('validates owner grammar and duplicate identities before displaying per-bot rows', () => {
    for (const id of ['../nova', 'n\n', 'bot/other', 'é', 'x'.repeat(97)]) {
      const value = fixture(); value.bots[0].botId = id;
      expect(() => parseUsageBudget(value)).toThrow();
    }
    const value = fixture(); value.bots.push(value.bots[0]);
    expect(() => parseUsageBudget(value)).toThrow();
    expect(() => parseUsageBudget({ ...fixture(), bots: Array(1025).fill({ botId: 'nova', total: totals }) })).toThrow();
  });
  it('projects only the agreed public operator fields rather than arbitrary native payloads', () => {
    const value = parseUsageBudget({ ...fixture(), rawPrompt: 'synthetic-private', session: 'synthetic-session', bots: [{ botId: 'nova', total: { ...totals, apiKey: 'synthetic-key' }, grants: ['screen'] }] });
    expect(JSON.stringify(value)).not.toContain('synthetic');
    expect(Object.keys(value)).toEqual(['limits', 'day', 'clockAdjusted', 'total', 'bots', 'lastDenial']);
  });
  it('accepts zero component limits, while requiring a positive optional hard token limit', () => {
    const draft = limitsDraft(limits);
    for (const field of limitFields) draft.values[field.key] = '0';
    expect(draftLimits(draft)).toEqual({ requestAccounting: 'dispatch', dailyDialogRequests: 0, dailyTtsCharacters: 0, dailyHindsightRequests: 0, dailyImageCount: 0, dailyImageBytes: 0, hardTokenLimit: null });
    expect(draft.tokenLimit).toBe('1000000');
    draft.tokenEnabled = true; draft.tokenLimit = '0';
    expect(() => draftLimits(draft)).toThrow();
    draft.tokenLimit = '1000000000'; expect(draftLimits(draft).hardTokenLimit).toBe(1_000_000_000);
    draft.tokenLimit = '1000000001'; expect(() => draftLimits(draft)).toThrow();
  });
  it('rejects partial, noninteger, nonfinite and out-of-range edits before invoking native mutation', async () => {
    const invoke = vi.fn();
    for (const text of ['', ' ', '-1', '1.5', '1e3', 'Infinity', 'NaN', '1000001']) {
      const draft = limitsDraft(limits); draft.values.dailyDialogRequests = text;
      expect(() => draftLimits(draft)).toThrow();
    }
    await expect(setUsageLimits(invoke, { ...limits, dailyImageBytes: 1_073_741_825 })).rejects.toThrow();
    await expect(setUsageLimits(invoke, { ...limits, hardTokenLimit: 0 })).rejects.toThrow();
    expect(invoke).not.toHaveBeenCalled();
  });
  it('keeps unfinished edits dirty and does not convert invalid input into a fallback limit', () => {
    const draft = limitsDraft(limits);
    expect(draftChanged(draft, limits)).toBe(false);
    draft.values.dailyDialogRequests = ''; expect(draftChanged(draft, limits)).toBe(true);
    draft.values.dailyDialogRequests = '101'; expect(draftChanged(draft, limits)).toBe(true);
    draft.values.dailyDialogRequests = '100'; expect(draftChanged(draft, limits)).toBe(false);
    draft.requestAccounting = 'direct'; expect(draftChanged(draft, limits)).toBe(true);
    draft.requestAccounting = 'dispatch'; draft.tokenEnabled = true; expect(draftChanged(draft, limits)).toBe(true);
    expect(parseUsageLimits(limits).hardTokenLimit).toBeNull();
  });
  it('offers explicit validated repair defaults without manufacturing usage or dropping accounting mode', () => {
    expect(parseUsageLimits(usageLimitDefaults)).toEqual(limits);
    expect(draftLimits(limitsDraft(usageLimitDefaults)).requestAccounting).toBe('dispatch');
    expect(() => parseUsageLimits({ ...limits, requestAccounting: 'unknown' })).toThrow();
    expect(() => parseUsageLimits({ ...limits, requestAccounting: undefined })).toThrow();
    for (const denial of ['request_accounting_unsupported', 'request_history_unbounded', 'token_history_unbounded']) {
      expect(parseUsageBudget({ ...fixture(), lastDenial: denial }).lastDenial).toBe(denial);
    }
  });
  it('uses the immutable global native commands and forwards no bot, reset, model or grant arguments', async () => {
    const invoke = vi.fn().mockResolvedValue(fixture());
    await getUsageBudget(invoke);
    expect(invoke).toHaveBeenLastCalledWith('get_usage_budget', {});
    await setUsageLimits(invoke, { ...limits, dailyDialogRequests: 5 });
    expect(invoke).toHaveBeenLastCalledWith('set_usage_limits', { limits: { ...limits, dailyDialogRequests: 5 } });
    const error = new Error('synthetic transport failure'); invoke.mockRejectedValueOnce(error);
    await expect(getUsageBudget(invoke)).rejects.toBe(error);
  });
});

describe('Usage panel localization', () => {
  it('covers dynamic field labels, denial reasons and fixed native errors in all interface languages', () => {
    const catalogue = JSON.parse(readFileSync(new URL('./locales/usage-budget.json', import.meta.url), 'utf8'));
    const keys = [...limitFields.map(field => field.label), ...Object.values(usageDenials), ...Object.keys(catalogue.en)];
    for (const key of keys) for (const language of ['de', 'en', 'fr', 'es'] as const) {
      const value = translate(key, {}, language);
      expect(value.trim()).not.toBe('');
      if (language !== 'de') { expect(value).toBe(catalogue[language][key]); expect(catalogues.some(item => Object.hasOwn(item[language], key))).toBe(true); }
    }
  });
});
