import { test } from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseActivePort, waitForDevTools } from './chrome-devtools.mjs';
import { DECODER_PROGRESS_PREFIX, DECODER_PROGRESS_STAGE, encodeDecoderProgress, parseDecoderProgress, decoderProgressEvent, decoderProgressTimeout } from './avatar-decoder-progress.mjs';
const endpoint = 'ws://127.0.0.1:12345/devtools/browser/synthetic-browser';
function fixture() {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'orbitpal-devtools-unit-'));
  const browser = new EventEmitter();
  Object.assign(browser, { stdout: new PassThrough(), stderr: new PassThrough(), pid: 123, exitCode: null });
  return { profile, browser, close: () => fs.rmSync(profile, { recursive: true, force: true }) };
}
for (const stream of ['stdout', 'stderr']) test(`split ${stream} endpoint and listener cleanup`, async () => {
  const f = fixture();
  try {
    const pending = waitForDevTools(f.browser, f.profile);
    f.browser[stream].write('DevTools listen'); f.browser[stream].write(`ing on ${endpoint}\n`);
    assert.equal(await pending, endpoint);
    assert.equal(f.browser.listenerCount('exit'), 0); assert.equal(f.browser.stdout.listenerCount('data'), 0);
  } finally { f.close(); }
});
test('completed profile file after incomplete write without console output', async () => {
  const f = fixture();
  try {
    fs.writeFileSync(path.join(f.profile, 'DevToolsActivePort'), '12345\n');
    const pending = waitForDevTools(f.browser, f.profile, { timeoutMs: 1000, pollMs: 5 });
    fs.writeFileSync(path.join(f.profile, 'DevToolsActivePort'), '12345\r\n/devtools/browser/synthetic-browser\r\n');
    assert.equal(await pending, endpoint);
  } finally { f.close(); }
});
test('invalid ports, oversized files and non-browser/remote routes', () => {
  for (const value of ['0\n/devtools/browser/a', '65536\n/devtools/browser/a', '12345\nws://example.com/a', '12345\n/devtools/page/a', '12345\n/devtools/browser/a?x', '12345\n/devtools/browser/a\nextra', 'x'.repeat(4097)]) assert.equal(parseActivePort(value), undefined);
});
test('spawn errors and early exit fail immediately with bounded diagnostics', async () => {
  for (const kind of ['error', 'exit']) {
    const f = fixture();
    try {
      const pending = waitForDevTools(f.browser, f.profile);
      if (kind === 'error') f.browser.emit('error', Object.assign(new Error('synthetic'), { code: 'ENOENT' }));
      else { f.browser.stderr.write('synthetic browser failure'); f.browser.emit('exit', 9, null); }
      await assert.rejects(pending, kind === 'error' ? /ENOENT/ : /synthetic browser failure/);
      assert.equal(f.browser.listenerCount('exit'), 0);
    } finally { f.close(); }
  }
});
test('timeout rejects remote logs and includes bounded child output', async () => {
  const f = fixture();
  try {
    const pending = waitForDevTools(f.browser, f.profile, { timeoutMs: 25, pollMs: 5 });
    f.browser.stdout.write('DevTools listening on ws://example.com:12345/devtools/browser/a\n');
    f.browser.stderr.write('x'.repeat(20000) + 'synthetic-tail');
    await assert.rejects(pending, error => /did not start/.test(error.message) && /synthetic-tail/.test(error.message) && error.message.length < 5000);
  } finally { f.close(); }
});

function progressRecord(overrides = {}) {
  return { version: 1, stage: DECODER_PROGRESS_STAGE.stageLoad, after: false, policyIndex: 0, fixtureOrdinal: 1, completedCount: 0, rgbaFallback: false, transparent: false, ...overrides };
}
function progressEvent(value = encodeDecoderProgress(progressRecord()), overrides = {}) {
  return { method: 'Runtime.consoleAPICalled', sessionId: 'synthetic-main', params: { type: 'info', args: [{ type: 'string', value }] }, ...overrides };
}
test('decoder progress roundtrips closed numeric boundaries and returns an immutable copy', () => {
  for (const fields of [progressRecord({ stage: 0, fixtureOrdinal: 0 }), progressRecord({ stage: 19, after: true, policyIndex: 1, fixtureOrdinal: 1024, completedCount: 8192, rgbaFallback: true, transparent: true })]) {
    const line = encodeDecoderProgress(fields), parsed = parseDecoderProgress(line);
    assert.deepEqual(parsed, fields);
    assert.ok(line.length <= 256);
    assert.ok(Object.isFrozen(parsed));
    fields.completedCount = 9;
    assert.notEqual(parsed.completedCount, 9);
  }
});
test('decoder progress rejects unknown fields, invalid shapes and out-of-bound values', () => {
  const invalid = [null, [], {}, 'PRIVATE', progressRecord({ privatePath: 'PRIVATE' })];
  for (const [key, values] of Object.entries({ version: [0, 2, '1'], stage: [-1, 20, .5, NaN, Infinity], policyIndex: [-1, 2, .5], fixtureOrdinal: [-1, 1025, .5], completedCount: [-1, 8193, .5], after: [0, 'false', null], rgbaFallback: [0, 'false', null], transparent: [0, 'false', null] })) {
    for (const value of values) invalid.push(progressRecord({ [key]: value }));
  }
  for (const value of invalid) {
    assert.equal(encodeDecoderProgress(value), undefined);
    assert.equal(decoderProgressTimeout(value), 'Decoder/render fixtures timed out; lastProgress=none');
  }
});
test('decoder progress requires its exact prefix and rejects malformed or oversized lines', () => {
  const valid = encodeDecoderProgress(progressRecord());
  for (const line of [undefined, {}, 'PRIVATE ' + valid, valid.replace(DECODER_PROGRESS_PREFIX, 'orbitpal-other: '), DECODER_PROGRESS_PREFIX + 'PRIVATE', DECODER_PROGRESS_PREFIX + '{}', valid + 'PRIVATE', DECODER_PROGRESS_PREFIX + ' '.repeat(257)]) assert.equal(parseDecoderProgress(line), undefined);
  assert.equal(parseDecoderProgress(DECODER_PROGRESS_PREFIX + JSON.stringify(progressRecord({ privateError: 'PRIVATE' }))), undefined);
});
test('decoder progress never invokes record getters or inherited serialization hooks', () => {
  let reads = 0;
  for (const key of Object.keys(progressRecord())) {
    const input = progressRecord();
    Object.defineProperty(input, key, { get() { reads++; throw new Error('PRIVATE'); }, enumerable: true });
    assert.equal(encodeDecoderProgress(input), undefined);
    assert.equal(decoderProgressTimeout(input), 'Decoder/render fixtures timed out; lastProgress=none');
  }
  const input = Object.assign(Object.create({ get toJSON() { reads++; throw new Error('PRIVATE'); } }), progressRecord());
  assert.deepEqual(parseDecoderProgress(encodeDecoderProgress(input)), progressRecord());
  assert.equal(reads, 0);
});
test('decoder progress accepts only info events from the current main session and policy', () => {
  assert.deepEqual(decoderProgressEvent(progressEvent(), 'synthetic-main', 0), progressRecord());
  const events = [
    progressEvent(undefined, { method: 'Runtime.exceptionThrown' }),
    progressEvent(undefined, { sessionId: 'synthetic-worker' }),
    progressEvent(encodeDecoderProgress(progressRecord({ policyIndex: 1 }))),
    progressEvent(undefined, { params: { type: 'error', args: [{ type: 'string', value: encodeDecoderProgress(progressRecord()) }] } }),
    progressEvent(undefined, { params: { type: 'log', args: [{ type: 'string', value: encodeDecoderProgress(progressRecord()) }] } }),
  ];
  for (const event of events) assert.equal(decoderProgressEvent(event, 'synthetic-main', 0), undefined);
  assert.equal(decoderProgressEvent(progressEvent(), undefined, 0), undefined);
  assert.equal(decoderProgressEvent(progressEvent(), 'synthetic-main', -1), undefined);
});
test('decoder progress requires one actual string value and never falls back to descriptions', () => {
  const line = encodeDecoderProgress(progressRecord());
  for (const args of [[], [{ type: 'string', value: line }, { type: 'string', value: 'PRIVATE' }], [{ type: 'object', value: line }], [{ type: 'string', description: line }], [{ type: 'string', value: { toString() { throw new Error('PRIVATE'); } } }]]) {
    const event = progressEvent(undefined, { params: { type: 'info', args } });
    assert.equal(decoderProgressEvent(event, 'synthetic-main', 0), undefined);
  }
});
test('decoder progress ignores accessor-bearing event fields without invoking them', () => {
  let reads = 0;
  for (const [target, key] of [['event', 'method'], ['event', 'sessionId'], ['event', 'params'], ['params', 'type'], ['params', 'args'], ['args', '0'], ['argument', 'type'], ['argument', 'value']]) {
    const event = progressEvent();
    const object = target === 'event' ? event : target === 'params' ? event.params : target === 'args' ? event.params.args : event.params.args[0];
    Object.defineProperty(object, key, { get() { reads++; throw new Error('PRIVATE'); }, enumerable: true });
    assert.equal(decoderProgressEvent(event, 'synthetic-main', 0), undefined);
  }
  assert.equal(reads, 0);
});
test('decoder progress discards unrelated event metadata and prints only the closed record', () => {
  let reads = 0;
  const event = progressEvent();
  event.params.args[0].objectId = 'PRIVATE_OBJECT_ID';
  event.params.args[0].description = 'PRIVATE_PATH';
  Object.defineProperty(event.params, 'stackTrace', { get() { reads++; throw new Error('PRIVATE'); } });
  const safe = decoderProgressEvent(event, 'synthetic-main', 0);
  const timeout = decoderProgressTimeout(safe);
  assert.deepEqual(safe, progressRecord());
  assert.ok(timeout.includes('"stage":16'));
  assert.ok(timeout.length < 300);
  assert.ok(!timeout.includes('PRIVATE') && !timeout.includes('synthetic-main'));
  assert.equal(reads, 0);
  assert.equal(decoderProgressTimeout('PRIVATE_ERROR'), 'Decoder/render fixtures timed out; lastProgress=none');
});
