import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { X509Certificate } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadSigningPolicy, assertPinnedRequirement } from './macos-signing-policy.mjs';

export function assertMicrophonePermissions(config, info, entitlements) {
  assert.equal(config.bundle?.macOS?.entitlements, 'Entitlements.plist', 'macOS releases must sign the microphone entitlement');
  assert.equal(config.bundle?.macOS?.infoPlist, 'Info.plist', 'macOS releases must include the microphone usage description');
  assert.equal(entitlements['com.apple.security.device.audio-input'], true, 'Hardened runtime blocks audio without audio-input');
  assert.ok(typeof info.NSMicrophoneUsageDescription === 'string' && info.NSMicrophoneUsageDescription.trim(), 'A microphone usage description is required');
}

// These are migration identities, rather than the current marketing name.
// Changing them can orphan macOS privacy grants and existing app data.
export function assertMacosIdentity(config, info, signingDetails) {
  const identifier = 'studio.streamer-ai.cohost';
  assert.equal(config.identifier, identifier, 'Keep the existing macOS bundle identity across updates');
  assert.equal(config.productName, 'OrbitPal', 'Keep the macOS product name stable');
  assert.equal(info.CFBundleIdentifier, identifier, 'The built app has a different bundle identity');
  assert.equal(info.CFBundleName, 'OrbitPal', 'The built app has a different product name');
  assert.equal(info.CFBundleExecutable, 'orbitpal', 'Keep the executable name stable');
  assert.equal(info.CFBundleShortVersionString, config.version, 'The built app has a different release version');
  const fields = new Map(signingDetails.split(/\r?\n/).filter(line => line.includes('=')).map(line => {
    const index = line.indexOf('='); return [line.slice(0, index), line.slice(index + 1)];
  }));
  assert.equal(fields.get('Identifier'), identifier, 'The signed code has a different signing identifier');
  return { adHoc: fields.get('Signature') === 'adhoc', teamIdentifier: fields.get('TeamIdentifier') || null };
}

export function verifyPinnedSignature(app, identity) {
  assert.equal(process.platform, 'darwin');
  const directory = mkdtempSync(join(tmpdir(), 'orbitpal-public-signature-'));
  try {
    const details = spawnSync('/usr/bin/codesign', ['-dv', '-r-', '--verbose=4', '--extract-certificates', join(directory, 'certificate'), app], { encoding: 'utf8', timeout: 30_000 });
    assert.equal(details.status, 0, 'The app must have an extractable signing certificate and requirement');
    const description = `${details.stdout || ''}\n${details.stderr || ''}`;
    assert.doesNotMatch(description, /Signature=adhoc/, 'A persistent release may never silently fall back to ad-hoc signing');
    assert.match(description, /flags=0x[0-9a-f]+\([^\n]*\bruntime\b/, 'Keep the macOS hardened runtime enabled');
    const certificate = new X509Certificate(readFileSync(join(directory, 'certificate0')));
    assert.equal(certificate.fingerprint256.replaceAll(':', '').toLowerCase(), identity.sha256, 'The actual app signer does not match the reviewed certificate SHA-256');
    assertPinnedRequirement(description, identity);
    execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', app], { stdio: 'inherit', timeout: 30_000 });
    return description;
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

export function verifyMacosBundle(app, root = resolve(fileURLToPath(new URL('..', import.meta.url)))) {
  assert.equal(process.platform, 'darwin', 'Bundle verification requires macOS codesign and plutil');
  const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
  const plist = file => JSON.parse(execFileSync('/usr/bin/plutil', ['-convert', 'json', '-o', '-', file], { encoding: 'utf8' }));
  const info = plist(join(app, 'Contents/Info.plist'));
  const details = spawnSync('/usr/bin/codesign', ['-dv', '--verbose=4', app], { encoding: 'utf8', timeout: 30_000 });
  assert.equal(details.status, 0, 'The built app must have readable code-signing metadata');
  const identity = assertMacosIdentity(config, info, `${details.stdout || ''}\n${details.stderr || ''}`);
  // Inspect the actual sealed signature, not just the source entitlement file.
  const signed = execFileSync('/usr/bin/codesign', ['-d', '--entitlements', '-', '--xml', app], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  assert.ok(signed.trim(), 'The built app has no sealed entitlements; microphone capture would be blocked');
  const entitlements = JSON.parse(execFileSync('/usr/bin/plutil', ['-convert', 'json', '-o', '-', '-'], { input: signed, encoding: 'utf8' }));
  assertMicrophonePermissions(config, info, entitlements);
  const pinned = loadSigningPolicy(root);
  verifyPinnedSignature(app, pinned);
  console.log('Verified stable app/signing identifiers, release version, sealed microphone entitlement, certificate SHA-256 and exact persistent certificate-bound designated requirement. The self-signed app is not Apple-notarized; TCC update acceptance is a separate manual check.');
  return { ...identity, mode: 'self-signed', certificateSha256: pinned.sha256 };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('Pass the built .app path');
  verifyMacosBundle(resolve(process.argv[2]));
}
