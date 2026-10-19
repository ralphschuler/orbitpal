// Real signatures, fixture keys and synthetic Mach-O apps only. Never execute
// the apps, query TCC, record audio, capture a screen or read production keys.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { provisionIdentity } from './provision-macos-identity.mjs';
import { validateSigningPolicy, designatedRequirement } from './macos-signing-policy.mjs';
import { cleanupIdentity, prepareIdentity, requireDisposableMac, runnerDirectory, runNative, signingContext } from './macos-release-signing.mjs';
import { verifyPinnedSignature } from './verify-macos-permissions.mjs';

requireDisposableMac();
for (const name of Object.keys(process.env)) assert.ok(!/^(?:MACOS_CODESIGN_|TAURI_SIGNING_PRIVATE_KEY|APPLE_CERTIFICATE)/.test(name), 'Signing fixtures must never receive production signing secrets');
const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const target = process.arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
const work = mkdtempSync(join(process.env.RUNNER_TEMP, 'orbitpal-public-signing-fixtures-'));
const directories = [runnerDirectory(target, '-fixture-a'), runnerDirectory(target, '-fixture-b')];

try {
  const identities = ['a', 'b'].map(label => {
    const material = join(work, `material-${label}`);
    const result = provisionIdentity(material, { root });
    const identity = validateSigningPolicy(result.publicPolicy, readFileSync(join(material, 'certificate.pem')));
    return { material, identity };
  });
  assert.notEqual(identities[0].identity.sha256, identities[1].identity.sha256, 'Fixture keys must actually differ, even though certificate display names match');
  const outputs = [];
  for (let index = 0; index < identities.length; index += 1) {
    const { identity, material } = identities[index];
    prepareIdentity(directories[index], identity, readFileSync(join(material, 'identity.p12')), readFileSync(join(material, 'password.txt'), 'utf8'));
    const releaseDirectory = join(work, `output-${index}`);
    const app = join(releaseDirectory, 'bundle/macos/OrbitPal.app');
    mkdirSync(join(app, 'Contents/MacOS'), { recursive: true });
    const signing = signingContext(root, target, directories[index], identity, releaseDirectory);
    const environment = { ...process.env, ...signing };
    const sign = () => {
      const result = spawnSync(process.execPath, [resolve(root, 'scripts/macos-codesign.mjs'), '--force', '-s', identity.sha1, '--options', 'runtime', app], { env: environment, stdio: ['ignore', 'pipe', 'pipe'], timeout: 90_000 });
      assert.equal(result.status, 0, 'Fixture app signing failed; native diagnostics are withheld to protect signing inputs');
      verifyPinnedSignature(app, identity);
    };
    const compile = version => {
      const source = join(work, `main-${index}-${version}.c`);
      writeFileSync(source, `int main(void) { return ${version}; }\n`);
      runNative('/usr/bin/clang', [source, '-o', join(app, 'Contents/MacOS/orbitpal')]);
      writeFileSync(join(app, 'Contents/Info.plist'), `<?xml version="1.0"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>studio.streamer-ai.cohost</string><key>CFBundleName</key><string>OrbitPal</string><key>CFBundleExecutable</key><string>orbitpal</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>0.0.${version}</string></dict></plist>`);
    };
    compile(1);
    sign();
    if (index === 0) {
      cpSync(app, join(work, 'version-one.app'), { recursive: true });
      compile(2);
      sign();
    }
    outputs.push(app);
  }
  const oldRequirement = join(work, 'version-one.req');
  writeFileSync(oldRequirement, designatedRequirement(identities[0].identity).replace(/^designated => /, ''));
  runNative('/usr/bin/codesign', ['--verify', '--strict', '-R', oldRequirement, outputs[0]]);
  const different = spawnSync('/usr/bin/codesign', ['--verify', '--strict', '-R', oldRequirement, outputs[1]], { stdio: ['ignore', 'pipe', 'pipe'], timeout: 30_000 });
  assert.notEqual(different.status, 0, 'The old requirement must reject another key with the same certificate name');
  assert.throws(() => verifyPinnedSignature(outputs[1], identities[0].identity), /reviewed certificate/);
  const info = join(outputs[0], 'Contents/Info.plist');
  writeFileSync(info, readFileSync(info, 'utf8').replace('<string>OrbitPal</string>', '<string>Changed after signing</string>'));
  const tampered = spawnSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', outputs[0]], { stdio: ['ignore', 'pipe', 'pipe'], timeout: 30_000 });
  assert.notEqual(tampered.status, 0, 'Tampered app resources must be rejected');
  console.log('Verified fixture A→B code identity, rejection of a different signing key with the same name, and tamper rejection. No app was run and no TCC acceptance is claimed.');
} finally {
  let failed = false;
  // Nested preparations snapshot the preceding search list; restore LIFO.
  for (const directory of [...directories].reverse()) { try { cleanupIdentity(directory); } catch { failed = true; } }
  rmSync(work, { recursive: true, force: true });
  assert.equal(failed, false, 'Fixture signing cleanup failed; discard this disposable runner');
  for (const directory of directories) assert.equal(existsSync(directory), false);
}
