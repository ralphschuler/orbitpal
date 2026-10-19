// Real signatures, fixture keys and synthetic Mach-O apps only. Never execute
// the apps, query TCC, record audio, capture a screen or read production keys.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { provisionIdentity } from './provision-macos-identity.mjs';
import { validateSigningPolicy, extractDesignatedRequirement } from './macos-signing-policy.mjs';
import { cleanupIdentity, prepareIdentity, requireDisposableMac, runnerDirectory, runNative, signingContext } from './macos-release-signing.mjs';
import { verifyPinnedSignature } from './verify-macos-permissions.mjs';

requireDisposableMac();
for (const name of Object.keys(process.env)) assert.ok(!/^(?:MACOS_CODESIGN_|TAURI_SIGNING_PRIVATE_KEY|APPLE_CERTIFICATE)/.test(name), 'Signing fixtures must never receive production signing secrets');
const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const target = process.arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
const work = mkdtempSync(join(process.env.RUNNER_TEMP, 'orbitpal-public-signing-fixtures-'));
const directories = [runnerDirectory(target, '-fixture-a'), runnerDirectory(target, '-fixture-b')];

let primaryFailure;
try {
  const identities = ['a', 'b'].map(label => {
    const material = join(work, `material-${label}`);
    const result = provisionIdentity(material, { root });
    const identity = validateSigningPolicy(result.publicPolicy, readFileSync(join(material, 'certificate.pem')));
    return { material, identity };
  });
  assert.notEqual(identities[0].identity.sha256, identities[1].identity.sha256, 'Fixture keys must actually differ, even though certificate display names match');
  const outputs = [];
  const oldRequirement = join(work, 'version-one.req');
  for (let index = 0; index < identities.length; index += 1) {
    const { identity, material } = identities[index];
    prepareIdentity(directories[index], identity, readFileSync(join(material, 'identity.p12')), readFileSync(join(material, 'password.txt'), 'utf8'));
    const releaseDirectory = join(work, `output-${index}`);
    const app = join(releaseDirectory, 'bundle/macos/OrbitPal.app');
    mkdirSync(join(app, 'Contents/MacOS'), { recursive: true });
    const signing = signingContext(root, target, directories[index], identity, releaseDirectory);
    const environment = { ...process.env, ...signing };
    const sign = () => {
      // Match tauri-macos-sign 2.4.2 as locked by CLI2.12.3: no timestamp
      // argument, with runtime and the configured entitlement path.
      const result = spawnSync(process.execPath, [resolve(root, 'scripts/macos-codesign.mjs'), '--force', '-s', identity.sha1, '--options', 'runtime', '--entitlements', resolve(root, 'src-tauri/Entitlements.plist'), app], { env: environment, stdio: ['ignore', 'pipe', 'pipe'], timeout: 90_000 });
      // This process sees only fixture public metadata/keychain paths, never
      // passwords/archive inputs or production secrets. Its bounded output
      // makes real codesign failures reviewable instead of masking them.
      if (result.error || result.status !== 0) {
        console.error(`Fixture-only codesign diagnostics:\n${result.stderr?.toString().slice(0, 6000) || '(no diagnostics)'}`);
        throw new Error(`Fixture app signing failed (exit ${Number.isInteger(result.status) ? result.status : 'unavailable'})`);
      }
      return verifyPinnedSignature(app, identity);
    };
    const compile = version => {
      const source = join(work, `main-${index}-${version}.c`);
      writeFileSync(source, `int main(void) { return ${version}; }\n`);
      runNative('/usr/bin/clang', [source, '-o', join(app, 'Contents/MacOS/orbitpal')]);
      writeFileSync(join(app, 'Contents/Info.plist'), `<?xml version="1.0"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>studio.streamer-ai.cohost</string><key>CFBundleName</key><string>OrbitPal</string><key>CFBundleExecutable</key><string>orbitpal</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>0.0.${version}</string></dict></plist>`);
    };
    compile(1);
    const firstSignature = sign();
    if (index === 0) {
      // Persist the actual requirement extracted from the sealed version-A
      // app. Do not synthesize another expected requirement for this proof.
      writeFileSync(oldRequirement, extractDesignatedRequirement(firstSignature).replace(/^designated => /, ''));
      cpSync(app, join(work, 'version-one.app'), { recursive: true });
      compile(2);
      sign();
    }
    outputs.push(app);
  }
  runNative('/usr/bin/codesign', ['--verify', '--strict', '-R', oldRequirement, outputs[0]]);
  const different = spawnSync('/usr/bin/codesign', ['--verify', '--strict', '-R', oldRequirement, outputs[1]], { stdio: ['ignore', 'pipe', 'pipe'], timeout: 30_000 });
  assert.notEqual(different.status, 0, 'The old requirement must reject another key with the same certificate name');
  assert.throws(() => verifyPinnedSignature(outputs[1], identities[0].identity), /reviewed certificate/);
  const info = join(outputs[0], 'Contents/Info.plist');
  writeFileSync(info, readFileSync(info, 'utf8').replace('<string>OrbitPal</string>', '<string>Changed after signing</string>'));
  const tampered = spawnSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', outputs[0]], { stdio: ['ignore', 'pipe', 'pipe'], timeout: 30_000 });
  assert.notEqual(tampered.status, 0, 'Tampered app resources must be rejected');
  console.log('Verified fixture A→B code identity, rejection of a different signing key with the same name, and tamper rejection. No app was run and no TCC acceptance is claimed.');
} catch (error) {
  primaryFailure = error;
} finally {
  let failed = false;
  // Nested preparations snapshot the preceding search list; restore LIFO.
  for (const directory of [...directories].reverse()) { try { cleanupIdentity(directory); } catch { failed = true; } }
  rmSync(work, { recursive: true, force: true });
  if (failed) {
    const cleanupFailure = new Error('Fixture signing cleanup failed; discard this disposable runner');
    if (primaryFailure) throw new AggregateError([primaryFailure, cleanupFailure], 'Fixture signing and cleanup failed');
    throw cleanupFailure;
  }
  for (const directory of directories) assert.equal(existsSync(directory), false);
}
if (primaryFailure) throw primaryFailure;
