import assert from 'node:assert/strict';
import { X509Certificate, createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const MACOS_IDENTIFIER = 'studio.streamer-ai.cohost';
export const MACOS_POLICY = 'release/macos-signing-policy.json';

export function validateSigningPolicy(policy, certificate, now = Date.now()) {
  assert.equal(policy.schemaVersion, 1, 'Unsupported macOS signing policy');
  assert.equal(policy.mode, 'self-signed', 'A persistent self-signed macOS identity is required; ad-hoc fallback is disabled');
  assert.equal(policy.identifier, MACOS_IDENTIFIER, 'Keep the installed macOS signing identifier');
  assert.equal(policy.certificateFile, 'release/macos-signing-certificate.pem', 'Unexpected public signing certificate path');
  assert.match(policy.certificateSha256 ?? '', /^[0-9a-f]{64}$/, 'Provision and review the public macOS certificate pin before a release');
  const cert = new X509Certificate(certificate);
  const sha256 = createHash('sha256').update(cert.raw).digest('hex');
  const sha1 = createHash('sha1').update(cert.raw).digest('hex');
  assert.equal(sha256, policy.certificateSha256, 'The macOS certificate differs from the reviewed SHA-256 pin');
  assert.ok(cert.checkIssued(cert) && cert.verify(cert.publicKey), 'The selected identity must be a cryptographically self-signed certificate');
  assert.ok(Date.parse(cert.validFrom) <= now && Date.parse(cert.validTo) > now, 'The macOS signing certificate is not currently valid');
  assert.ok(cert.keyUsage?.includes('1.3.6.1.5.5.7.3.3'), 'The certificate must explicitly allow code signing');
  return { ...policy, sha256, sha1, certificate: cert };
}

export function loadSigningPolicy(root) {
  const policy = JSON.parse(readFileSync(resolve(root, MACOS_POLICY), 'utf8'));
  // Check the pinned path before reading a file selected by source policy.
  assert.equal(policy.certificateFile, 'release/macos-signing-certificate.pem', 'Unexpected public signing certificate path');
  assert.match(policy.certificateSha256 ?? '', /^[0-9a-f]{64}$/, 'Provision and review the public macOS certificate pin before a release');
  return validateSigningPolicy(policy, readFileSync(resolve(root, policy.certificateFile)));
}

export function designatedRequirement(identity) {
  assert.equal(identity.identifier, MACOS_IDENTIFIER);
  assert.match(identity.sha1, /^[0-9a-f]{40}$/);
  // Apple's requirement language uses the certificate's SHA-1 digest. The
  // complete certificate is additionally pinned with SHA-256 in source policy.
  return `designated => identifier "${MACOS_IDENTIFIER}" and certificate leaf = H"${identity.sha1}"`;
}

export function assertPinnedRequirement(output, identity) {
  const lines = output.split(/\r?\n/).map(line => line.trim()).filter(line => /^(?:#\s*)?designated\s*=>/.test(line));
  assert.equal(lines.length, 1, 'Exactly one designated requirement must be present');
  const actual = lines[0].replace(/^#\s*/, '').replace(/\s+/g, ' ').replace(/H"([0-9A-Fa-f]{40})"/g, (_, hash) => `H"${hash.toLowerCase()}"`);
  assert.equal(actual, designatedRequirement(identity), 'The macOS designated requirement must bind exactly the app ID and pinned certificate; weaker/name-only/hash-only identities are rejected');
}

export function quoteSecurityInput(value) {
  assert.ok(typeof value === 'string' && !/[\r\n\0]/.test(value), 'Invalid security command input');
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

export function signingCommand(args, context, targetRealPath) {
  const copy = [...args];
  const signIndex = copy.findIndex(value => value === '-s' || value === '--sign');
  if (signIndex < 0) throw new Error('The release codesign wrapper accepts signing commands only');
  assert.equal(copy.filter(value => value === '-s' || value === '--sign' || value.startsWith('--sign=')).length, 1, 'Ambiguous signing identity');
  assert.equal(copy[signIndex + 1], context.sha1, 'Tauri selected an unexpected signing certificate');
  assert.ok(!copy.some(value => /^-r|^--(?:requirements|identifier|keychain|timestamp)(?:=|$)/.test(value)), 'Tauri attempted to override the reviewed macOS signing policy');
  assert.ok(targetRealPath.startsWith(`${context.releaseDirectory}/`), 'Signing target escapes the release build directory');
  const enforced = ['--keychain', context.keychain, '--timestamp=none'];
  if (targetRealPath === context.app || targetRealPath === `${context.app}/Contents/MacOS/orbitpal`) {
    enforced.push('--identifier', MACOS_IDENTIFIER, '--requirements', context.requirements);
  }
  copy.splice(copy.length - 1, 0, ...enforced);
  return copy;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const identity = loadSigningPolicy(resolve(fileURLToPath(new URL('..', import.meta.url))));
  console.log(`Validated persistent macOS public certificate policy (SHA-256 ${identity.sha256}).`);
}
