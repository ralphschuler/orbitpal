// Disposable offline test browsers only. Chromium documents both discovery paths:
// https://chromedevtools.github.io/devtools-protocol/
import fs from 'node:fs';
import path from 'node:path';
const LIMIT = 8192;
const browserPath = /^\/devtools\/browser\/[A-Za-z0-9_-]{1,128}$/;
export function parseActivePort(value) {
  if (typeof value !== 'string' || Buffer.byteLength(value) > 4096) return undefined;
  const lines = value.trim().split(/\r?\n/);
  if (lines.length !== 2 || !/^[1-9]\d{0,4}$/.test(lines[0]) || !browserPath.test(lines[1])) return undefined;
  return Number(lines[0]) <= 65535 ? `ws://127.0.0.1:${lines[0]}${lines[1]}` : undefined;
}
function logEndpoint(output) {
  for (const match of output.matchAll(/DevTools listening on (ws:\/\/\S+)/g)) {
    try {
      const url = new URL(match[1]);
      if (['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) && url.port && !url.username && !url.password && !url.search && !url.hash && browserPath.test(url.pathname)) return url.href;
    } catch { /* Wait for a valid local browser endpoint. */ }
  }
}
function readActivePort(profile) {
  let descriptor;
  try {
    const file = path.join(profile, 'DevToolsActivePort');
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 4096) return undefined;
    descriptor = fs.openSync(file, 'r');
    const data = Buffer.alloc(4097), count = fs.readSync(descriptor, data, 0, data.length, 0);
    return parseActivePort(data.subarray(0, count).toString('utf8'));
  } catch { return undefined; }
  finally { if (descriptor !== undefined) fs.closeSync(descriptor); }
}
export function waitForDevTools(browser, profile, { timeoutMs = 20000, pollMs = 50 } = {}) {
  return new Promise((resolve, reject) => {
    let stdout = '', stderr = '', settled = false;
    const cleanup = () => {
      clearTimeout(timer); clearInterval(poll);
      browser.stdout?.off('data', onStdout); browser.stderr?.off('data', onStderr);
      browser.stdout?.resume(); browser.stderr?.resume();
      browser.off('error', onError); browser.off('exit', onExit);
    };
    const finish = (error, endpoint) => { if (settled) return; settled = true; cleanup(); error ? reject(error) : resolve(endpoint); };
    const details = () => `pid=${browser.pid ?? 'none'}, exit=${browser.exitCode ?? 'pending'}; stdout=${stdout.slice(-2000)}; stderr=${stderr.slice(-2000)}`;
    const inspect = () => { const endpoint = logEndpoint(stdout) || logEndpoint(stderr) || readActivePort(profile); if (endpoint) finish(undefined, endpoint); };
    const tail = chunk => Buffer.from(chunk).subarray(-LIMIT).toString('utf8');
    const onStdout = chunk => { stdout = (stdout + tail(chunk)).slice(-LIMIT); inspect(); };
    const onStderr = chunk => { stderr = (stderr + tail(chunk)).slice(-LIMIT); inspect(); };
    const onError = error => finish(new Error(`Chrome could not start (${error.code ?? 'spawn error'}): ${details()}`));
    const onExit = (code, signal) => finish(new Error(`Chrome exited before DevTools (${code ?? signal}): ${details()}`));
    const timer = setTimeout(() => finish(new Error(`Chrome DevTools did not start within ${timeoutMs}ms: ${details()}`)), timeoutMs);
    const poll = setInterval(inspect, pollMs);
    browser.stdout?.on('data', onStdout); browser.stderr?.on('data', onStderr);
    browser.once('error', onError); browser.once('exit', onExit);
    if (browser.exitCode !== null && browser.exitCode !== undefined) onExit(browser.exitCode);
    else inspect();
  });
}
