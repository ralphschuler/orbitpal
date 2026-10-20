import { test } from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseActivePort, waitForDevTools } from './chrome-devtools.mjs';
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
