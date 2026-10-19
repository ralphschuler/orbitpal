#!/usr/bin/env node
// Real offline WebGL/worker/WASM integration, no browser automation dependency.
// Uses Chrome already installed on the developer machine / three CI runners.
import { build } from 'vite';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'orbitpal-decoders-'));
const candidates = [process.env.ORBITPAL_TEST_BROWSER, '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium', process.env.PROGRAMFILES && path.join(process.env.PROGRAMFILES, 'Google/Chrome/Application/chrome.exe'), process.env['PROGRAMFILES(X86)'] && path.join(process.env['PROGRAMFILES(X86)'], 'Google/Chrome/Application/chrome.exe')].filter(Boolean);
const executable = candidates.find((file) => fs.existsSync(file));
if (!executable) throw new Error('Chrome/Chromium required for offline decoder fixtures. Set ORBITPAL_TEST_BROWSER to its executable.');
let browser, server, socket;
try {
  await build({ configFile: false, root, publicDir: false, logLevel: 'warn', build: { outDir: path.join(tmp, 'dist'), target: 'es2022', emptyOutDir: true, chunkSizeWarningLimit: 2000, rollupOptions: { input: path.join(root, 'tests/browser/avatar-decoders.html') } } });
  const source = fs.readFileSync(path.join(root, 'src-tauri/src/overlay_http.rs'), 'utf8').split('const OBS_CSP: &str = concat!(')[1].split('\n);')[0];
  const obsCsp = [...source.matchAll(/"(?:[^"\\]|\\.)*"/g)].map(([string]) => JSON.parse(string)).join('');
  const nativeCsp = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'))).app.security.csp;
  const requests = [];
  let policy = obsCsp;
  server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://localhost'); requests.push(url.pathname);
    if (url.pathname === '/favicon.ico') { res.writeHead(204); res.end(); return; }

    if (url.pathname === '/fixture-list.json') {
      const original = fs.readdirSync(path.join(root, 'tests/fixtures/compressed-avatar')).filter((file) => /\.(glb|gltf)$/.test(file) && !file.startsWith('invalid-')).sort().map((file) => '/fixtures/' + file);
      const packed = process.env.ORBITPAL_NATIVE_FIXTURE_DIR ? fs.readdirSync(process.env.ORBITPAL_NATIVE_FIXTURE_DIR).filter((file) => file.endsWith('.glb')).sort().map((file) => '/packed/' + file) : [];
      if (process.env.CI && packed.length !== 10) throw new Error('Run the native compressed import fixture before the browser fixture');
      res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify([...original, ...packed])); return;
    }
    const prefix = url.pathname.startsWith('/packed/') ? process.env.ORBITPAL_NATIVE_FIXTURE_DIR : url.pathname.startsWith('/decoders/') ? 'public' : url.pathname.startsWith('/fixtures/') ? 'tests/fixtures/compressed-avatar' : null;
    const relative = url.pathname.startsWith('/packed/') ? url.pathname.slice('/packed/'.length) : url.pathname.startsWith('/fixtures/') ? url.pathname.slice('/fixtures/'.length) : url.pathname.slice(1);
    const base = prefix ? path.resolve(root, prefix) : path.join(tmp, 'dist');
    const file = path.resolve(base, relative || 'tests/browser/avatar-decoders.html');
    if (!file.startsWith(base + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { res.writeHead(404); res.end(); return; }
    const mime = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.glb': 'model/gltf-binary' }[path.extname(file)] || 'application/octet-stream';
    res.writeHead(200, { 'Content-Type': mime, 'Content-Security-Policy': policy, 'X-Content-Type-Options': 'nosniff' }); res.end(fs.readFileSync(file));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  browser = spawn(executable, ['--headless=new', '--no-sandbox', '--disable-dev-shm-usage', '--no-first-run', '--disable-background-networking', '--disable-component-update', '--disable-default-apps', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--remote-debugging-port=0', `--user-data-dir=${path.join(tmp, 'profile')}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
  const endpoint = await new Promise((resolve, reject) => {
    let output = ''; const timer = setTimeout(() => reject(new Error('Chrome DevTools did not start')), 20000);
    browser.stderr.on('data', (chunk) => { output += chunk; const match = output.match(/DevTools listening on (ws:\/\/\S+)/); if (match) { clearTimeout(timer); resolve(match[1]); } });
    browser.once('exit', (code) => { clearTimeout(timer); reject(new Error(`Chrome exited (${code}): ${output.slice(-2000)}`)); });
  });
  socket = new WebSocket(endpoint);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let sequence = 0; const pending = new Map(); const remote = [];
  socket.onmessage = ({ data }) => {
    const message = JSON.parse(data);
    if (message.id) { const promise = pending.get(message.id); if (promise) { pending.delete(message.id); message.error ? promise.reject(new Error(JSON.stringify(message.error))) : promise.resolve(message.result); } }
    if (message.method === 'Target.attachedToTarget') { call('Runtime.enable', {}, message.params.sessionId).catch(() => {}); call('Log.enable', {}, message.params.sessionId).catch(() => {}); call('Network.enable', {}, message.params.sessionId).catch(() => {}); }
    if (message.method === 'Log.entryAdded' && message.params.entry.level === 'error') console.error('Browser log:', message.params.entry.text);
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') console.error('Browser:', message.params.args.map((a) => a.value || a.description).join(' '));
    if (message.method === 'Runtime.exceptionThrown') console.error(JSON.stringify(message.params.exceptionDetails));
    if (message.method === 'Network.requestWillBeSent' && !message.params.request.url.startsWith(origin) && !/^(data|blob):/.test(message.params.request.url)) remote.push(message.params.request.url);
  };
  const call = (method, params = {}, sessionId) => new Promise((resolve, reject) => { const id = ++sequence; pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params, ...(sessionId && { sessionId }) })); });
  const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
  await call('Network.enable', {}, sessionId);
  await call('Runtime.enable', {}, sessionId);
  await call('Log.enable', {}, sessionId);
  await call('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true }, sessionId);
  for (const [name, csp] of [['OBS', obsCsp], ['native WebView policy', nativeCsp]]) {
    policy = csp;
    await call('Page.navigate', { url: origin + '/tests/browser/avatar-decoders.html?' + encodeURIComponent(name) }, sessionId);
    const deadline = Date.now() + 30000;
    while (true) {
      const ready = await call('Runtime.evaluate', { expression: 'typeof window.runAvatarDecoderFixtures === "function"', returnByValue: true }, sessionId);
      if (ready.result?.value) break;
      if (Date.now() > deadline) throw new Error('Fixture module failed to load');
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    const result = await Promise.race([call('Runtime.evaluate', { expression: 'window.runAvatarDecoderFixtures()', awaitPromise: true, returnByValue: true }, sessionId), new Promise((_, reject) => setTimeout(() => reject(new Error('Decoder/render fixtures timed out')), 120000).unref())]);
    assert.equal(result.exceptionDetails, undefined, JSON.stringify(result.exceptionDetails));
    assert.ok(result.result.value.results.length >= 79);
    assert.deepEqual(result.result.value.violations, []);
    console.log(`${name}: ${result.result.value.results.length} actual geometry/texture/stage renders; zero CSP violations.`);
  }
  assert.deepEqual(remote, [], 'Fixture page attempted external network access');
  for (const asset of ['draco/gltf/draco_wasm_wrapper.js', 'draco/gltf/draco_decoder.wasm', 'basis/basis_transcoder.js', 'basis/basis_transcoder.wasm']) assert.ok(requests.includes('/decoders/three-0.186.1/' + asset), `Decoder was never fetched locally: ${asset}`);
  console.log('All decoder assets fetched locally. No external requests.');
} finally {
  socket?.close(); browser?.kill();
  server?.closeAllConnections();
  await new Promise((resolve) => server ? server.close(resolve) : resolve());
  // Chromium needs a moment to release its profile files on Windows.
  fs.rmSync(tmp, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
}
