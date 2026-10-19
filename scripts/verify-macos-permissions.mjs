import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function assertMicrophonePermissions(config, info, entitlements) {
  assert.equal(config.bundle?.macOS?.entitlements, 'Entitlements.plist', 'macOS releases must sign the microphone entitlement');
  assert.equal(config.bundle?.macOS?.infoPlist, 'Info.plist', 'macOS releases must include the microphone usage description');
  assert.equal(entitlements['com.apple.security.device.audio-input'], true, 'Hardened runtime blocks audio without audio-input');
  assert.ok(typeof info.NSMicrophoneUsageDescription === 'string' && info.NSMicrophoneUsageDescription.trim(), 'A microphone usage description is required');
}

export function verifyMacosBundle(app, root = resolve(fileURLToPath(new URL('..', import.meta.url)))) {
  assert.equal(process.platform, 'darwin', 'Bundle verification requires macOS codesign and plutil');
  const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
  const plist = file => JSON.parse(execFileSync('/usr/bin/plutil', ['-convert', 'json', '-o', '-', file], { encoding: 'utf8' }));
  const info = plist(join(app, 'Contents/Info.plist'));
  // Inspect the actual sealed signature, not just the source entitlement file.
  const signed = execFileSync('/usr/bin/codesign', ['-d', '--entitlements', '-', '--xml', app], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  assert.ok(signed.trim(), 'The built app has no sealed entitlements; microphone capture would be blocked');
  const entitlements = JSON.parse(execFileSync('/usr/bin/plutil', ['-convert', 'json', '-o', '-', '-'], { input: signed, encoding: 'utf8' }));
  assertMicrophonePermissions(config, info, entitlements);
  execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', app], { stdio: 'inherit' });
  console.log('Verified microphone usage description, sealed audio-input entitlement and app signature.');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('Pass the built .app path');
  verifyMacosBundle(resolve(process.argv[2]));
}
