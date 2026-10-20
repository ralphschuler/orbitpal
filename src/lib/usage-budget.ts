export interface UsageLimits {
  requestAccounting: 'dispatch' | 'direct';
  dailyDialogRequests: number;
  dailyTtsCharacters: number;
  dailyHindsightRequests: number;
  dailyImageCount: number;
  dailyImageBytes: number;
  hardTokenLimit: number | null;
}
export interface UsageTotals {
  dialogRequests: number;
  ttsRequests: number;
  ttsCharacters: number;
  hindsightRecallRequests: number;
  hindsightRetainRequests: number;
  imageCount: number;
  imageBytes: number;
  chargedTokens: number;
  observedTokens: number;
  reservedTokens: number;
  unknownTokenRequests: number;
  pendingRequests: number;
}
export const usageDenials = {
  dialog_limit: 'Das gemeinsame Tageslimit für Dialoganfragen ist erreicht.',
  token_limit: 'Das gemeinsame Tokenlimit ist erreicht.',
  token_unsupported: 'Diese Integration unterstützt kein sicher begrenztes Tokenbudget.',
  tts_limit: 'Das gemeinsame Tageslimit für Sprachzeichen ist erreicht.',
  hindsight_limit: 'Das gemeinsame Tageslimit für Hindsight-Anfragen ist erreicht.',
  image_count_limit: 'Das gemeinsame Tageslimit für angehängte Bilder ist erreicht.',
  image_bytes_limit: 'Das gemeinsame Tageslimit für Bilddaten ist erreicht.',
  capacity: 'Die Höchstzahl gespeicherter Budgetvorgänge für diesen UTC-Tag ist erreicht.',
  accounting_unavailable: 'Die Verbrauchskontrolle ist nicht verfügbar. Autonome Anfragen bleiben gesperrt.',
  request_accounting_unsupported: 'Direkte Anfragezählung erlaubt keine autonomen CLI-Aufträge.',
  request_history_unbounded: 'Frühere CLI-Aufträge verhindern eine direkte Anfragegrenze bis zum nächsten UTC-Tag.',
  token_history_unbounded: 'Früherer Verbrauch ohne sichere Tokenobergrenze verhindert ein hartes Tokenlimit bis zum nächsten UTC-Tag.',
} as const;
export type UsageDenial = keyof typeof usageDenials;
export interface UsageBudgetSnapshot {
  limits: UsageLimits;
  day: string;
  clockAdjusted: boolean;
  total: UsageTotals;
  bots: { botId: string; total: UsageTotals }[];
  lastDenial: UsageDenial | null;
}
export const limitFields = [
  { key: 'dailyDialogRequests', label: 'Dialog-Aufrufe pro Tag', max: 1_000_000 },
  { key: 'dailyTtsCharacters', label: 'Sprachzeichen pro Tag', max: 1_000_000 },
  { key: 'dailyHindsightRequests', label: 'Hindsight-Anfragen pro Tag', max: 1_000_000 },
  { key: 'dailyImageCount', label: 'Angehängte Bilder pro Tag', max: 1_000_000 },
  { key: 'dailyImageBytes', label: 'Bilddaten pro Tag (Bytes)', max: 1_073_741_824 },
] as const;
export type LimitKey = typeof limitFields[number]['key'];
export interface UsageLimitsDraft {
  requestAccounting: UsageLimits['requestAccounting'];
  values: Record<LimitKey, string>;
  tokenEnabled: boolean;
  tokenLimit: string;
}
const invalidSnapshot = 'Verbrauchsdaten sind ungültig. Erneut laden; angezeigte Werte bleiben unbestätigt.';
export const invalidLimits = 'Gib ganze Zahlen innerhalb der angezeigten Grenzen ein.';
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const integer = (value: unknown, max = Number.MAX_SAFE_INTEGER): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= max;

export function parseUsageLimits(value: unknown): UsageLimits {
  if (!object(value)) throw new Error(invalidLimits);
  if (value.requestAccounting !== 'dispatch' && value.requestAccounting !== 'direct') throw new Error(invalidLimits);
  const limits = { requestAccounting: value.requestAccounting } as UsageLimits;
  for (const field of limitFields) {
    const amount = value[field.key];
    if (!integer(amount, field.max)) throw new Error(invalidLimits);
    limits[field.key] = amount;
  }
  const token = value.hardTokenLimit;
  if (token !== null && (!integer(token, 1_000_000_000) || token < 1)) throw new Error(invalidLimits);
  limits.hardTokenLimit = token;
  return limits;
}
const totalKeys = ['dialogRequests', 'ttsRequests', 'ttsCharacters', 'hindsightRecallRequests', 'hindsightRetainRequests', 'imageCount', 'imageBytes', 'chargedTokens', 'observedTokens', 'reservedTokens', 'unknownTokenRequests', 'pendingRequests'] as const;
function parseTotals(value: unknown): UsageTotals {
  if (!object(value)) throw new Error(invalidSnapshot);
  const total = {} as UsageTotals;
  for (const key of totalKeys) {
    if (!integer(value[key])) throw new Error(invalidSnapshot);
    total[key] = value[key];
  }
  return total;
}
export function parseUsageBudget(value: unknown): UsageBudgetSnapshot {
  try {
    if (!object(value) || typeof value.day !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value.day) ||
      new Date(value.day + 'T00:00:00Z').toISOString().slice(0, 10) !== value.day || typeof value.clockAdjusted !== 'boolean' ||
      !Array.isArray(value.bots) || value.bots.length > 1024 ||
      (value.lastDenial !== null && (typeof value.lastDenial !== 'string' || !Object.hasOwn(usageDenials, value.lastDenial)))) throw new Error();
    const seen = new Set<string>();
    const bots = value.bots.map((bot: unknown) => {
      if (!object(bot) || typeof bot.botId !== 'string' || !/^[A-Za-z0-9_-]{1,96}$/.test(bot.botId) || seen.has(bot.botId)) throw new Error();
      seen.add(bot.botId);
      return { botId: bot.botId, total: parseTotals(bot.total) };
    });
    return { limits: parseUsageLimits(value.limits), day: value.day, clockAdjusted: value.clockAdjusted, total: parseTotals(value.total), bots, lastDenial: value.lastDenial as UsageDenial | null };
  } catch { throw new Error(invalidSnapshot); }
}
export function limitsDraft(limits: UsageLimits): UsageLimitsDraft {
  return { requestAccounting: limits.requestAccounting, values: Object.fromEntries(limitFields.map(field => [field.key, String(limits[field.key])])) as Record<LimitKey, string>, tokenEnabled: limits.hardTokenLimit !== null, tokenLimit: String(limits.hardTokenLimit ?? 1_000_000) };
}
export function draftLimits(draft: UsageLimitsDraft): UsageLimits {
  const read = (text: string) => /^[0-9]+$/.test(text.trim()) ? Number(text) : NaN;
  return parseUsageLimits({ requestAccounting: draft.requestAccounting, ...Object.fromEntries(limitFields.map(field => [field.key, read(draft.values[field.key])])), hardTokenLimit: draft.tokenEnabled ? read(draft.tokenLimit) : null });
}
export function limitsEqual(a: UsageLimits, b: UsageLimits): boolean {
  return a.requestAccounting === b.requestAccounting && limitFields.every(field => a[field.key] === b[field.key]) && a.hardTokenLimit === b.hardTokenLimit;
}
// Explicit form proposal only; never presented as confirmed settings or usage.
export const usageLimitDefaults: Readonly<UsageLimits> = Object.freeze({ requestAccounting: 'dispatch', dailyDialogRequests: 100, dailyTtsCharacters: 20_000, dailyHindsightRequests: 200, dailyImageCount: 100, dailyImageBytes: 52_428_800, hardTokenLimit: null });
export function draftChanged(draft: UsageLimitsDraft, limits: UsageLimits): boolean {
  try { return !limitsEqual(draftLimits(draft), limits); } catch { return true; }
}
export type UsageInvoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;
export async function getUsageBudget(invoke: UsageInvoke): Promise<UsageBudgetSnapshot> {
  return parseUsageBudget(await invoke('get_usage_budget', {}));
}
export async function setUsageLimits(invoke: UsageInvoke, limits: UsageLimits): Promise<UsageBudgetSnapshot> {
  return parseUsageBudget(await invoke('set_usage_limits', { limits: parseUsageLimits(limits) }));
}
