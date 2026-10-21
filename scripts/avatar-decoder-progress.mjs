// Shared by the local browser fixture and its runner. Never accept raw asset,
// browser, worker or exception text as timeout progress.
export const DECODER_PROGRESS_PREFIX = 'orbitpal-decoder-progress: ';
export const DECODER_PROGRESS_STAGE = Object.freeze({
  list: 0, factoryConstruct: 1, factoryLoad: 2, factoryRender: 3,
  factoryModelDispose: 4, factoryDispose: 5, negativeConstruct: 6, negativeLoad: 7,
  negativeModelDispose: 8, negativeDispose: 9, optionalConstruct: 10, optionalLoad: 11,
  optionalRender: 12, optionalModelDispose: 13, optionalDispose: 14,
  stageConstruct: 15, stageLoad: 16, stageRender: 17, stageDispose: 18, complete: 19,
});
const KEYS = ['version', 'stage', 'after', 'policyIndex', 'fixtureOrdinal', 'completedCount', 'rgbaFallback', 'transparent'];
const MAX_LINE = 256;
const integer = (value, maximum) => Number.isInteger(value) && value >= 0 && value <= maximum;

function data(object, key) {
  if (object === null || typeof object !== 'object') return undefined;
  const descriptor = Object.getOwnPropertyDescriptor(object, key);
  return descriptor && Object.hasOwn(descriptor, 'value') ? descriptor.value : undefined;
}

function record(value) {
  try {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) return undefined;
    const keys = Reflect.ownKeys(value);
    if (keys.length !== KEYS.length || keys.some((key) => !KEYS.includes(key))) return undefined;
    const version = data(value, 'version'), stage = data(value, 'stage'), after = data(value, 'after');
    const policyIndex = data(value, 'policyIndex'), fixtureOrdinal = data(value, 'fixtureOrdinal');
    const completedCount = data(value, 'completedCount'), rgbaFallback = data(value, 'rgbaFallback'), transparent = data(value, 'transparent');
    if (version !== 1 || !integer(stage, 19) || !integer(policyIndex, 1)
        || !integer(fixtureOrdinal, 1024) || !integer(completedCount, 8192)
        || typeof after !== 'boolean' || typeof rgbaFallback !== 'boolean' || typeof transparent !== 'boolean') return undefined;
    return Object.freeze({ version, stage, after, policyIndex, fixtureOrdinal, completedCount, rgbaFallback, transparent });
  } catch { return undefined; }
}

export function encodeDecoderProgress(value) {
  const safe = record(value);
  return safe ? DECODER_PROGRESS_PREFIX + JSON.stringify(safe) : undefined;
}

export function parseDecoderProgress(line) {
  if (typeof line !== 'string' || line.length > MAX_LINE || !line.startsWith(DECODER_PROGRESS_PREFIX)) return undefined;
  try { return record(JSON.parse(line.slice(DECODER_PROGRESS_PREFIX.length))); }
  catch { return undefined; }
}

export function decoderProgressEvent(message, sessionId, policyIndex) {
  try {
    if (typeof sessionId !== 'string' || !sessionId || !integer(policyIndex, 1)
        || data(message, 'method') !== 'Runtime.consoleAPICalled' || data(message, 'sessionId') !== sessionId) return undefined;
    const params = data(message, 'params'), args = data(params, 'args');
    if (data(params, 'type') !== 'info' || !Array.isArray(args) || data(args, 'length') !== 1) return undefined;
    const argument = data(args, '0');
    if (data(argument, 'type') !== 'string') return undefined;
    const safe = parseDecoderProgress(data(argument, 'value'));
    return safe?.policyIndex === policyIndex ? safe : undefined;
  } catch { return undefined; }
}

export function decoderProgressTimeout(value) {
  const safe = record(value);
  return 'Decoder/render fixtures timed out; lastProgress=' + (safe ? JSON.stringify(safe) : 'none');
}
