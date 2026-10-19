// Offline preparation only: no keychain changes, trust changes, uploads or
// app signing. The maintainer explicitly chooses a new protected directory.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash, generateKeyPairSync, randomBytes, X509Certificate } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { MACOS_IDENTIFIER, validateSigningPolicy } from './macos-signing-policy.mjs';

export function provisionIdentity(directory, { root = resolve(dirname(fileURLToPath(import.meta.url)), '..'), openssl = 'openssl' } = {}) {
  assert.ok(isAbsolute(directory), 'Choose an absolute protected output directory outside the checkout');
  const parent = realpathSync(dirname(directory));
  const output = join(parent, directory.split('/').at(-1));
  const repository = realpathSync(root);
  assert.ok(output !== repository && !output.startsWith(`${repository}/`), 'Private signing material must stay outside the repository');
  assert.ok(!existsSync(output), 'The signing identity directory already exists; never regenerate or overwrite a persistent key');
  mkdirSync(output, { mode: 0o700 });
  const password = randomBytes(32).toString('hex');
  const passwordPath = join(output, 'password.txt');
  const keyPath = join(output, 'private-key.encrypted.pem');
  const certificatePath = join(output, 'certificate.pem');
  const archive = join(output, 'identity.p12');
  try {
    writeFileSync(passwordPath, password, { mode: 0o600, flag: 'wx' });
    const { privateKey } = generateKeyPairSync('rsa', { modulusLength: 4096, publicKeyEncoding: { type: 'spki', format: 'pem' }, privateKeyEncoding: { type: 'pkcs8', format: 'pem', cipher: 'aes-256-cbc', passphrase: password } });
    writeFileSync(keyPath, privateKey, { mode: 0o600, flag: 'wx' });
    const config = join(output, 'certificate.conf');
    writeFileSync(config, '[req]\nprompt = no\ndistinguished_name = subject\nx509_extensions = extensions\n[subject]\nCN = OrbitPal Persistent Release Signing\n[extensions]\nbasicConstraints = critical, CA:TRUE, pathlen:0\nkeyUsage = critical, digitalSignature, keyCertSign\nextendedKeyUsage = critical, codeSigning\nsubjectKeyIdentifier = hash\n', { mode: 0o600, flag: 'wx' });
    const commands = [
      ['req', '-new', '-x509', '-sha256', '-days', '3650', '-key', keyPath, '-passin', `file:${passwordPath}`, '-out', certificatePath, '-config', config],
      ['pkcs12', '-export', '-name', 'OrbitPal Persistent Release Signing', '-in', certificatePath, '-inkey', keyPath, '-passin', `file:${passwordPath}`, '-out', archive, '-passout', `file:${passwordPath}`],
    ];
    for (const args of commands) {
      const result = spawnSync(openssl, args, { stdio: ['ignore', 'pipe', 'pipe'], timeout: 120_000 });
      assert.ok(!result.error && result.status === 0, 'Offline certificate preparation failed; no private diagnostic output is printed');
    }
    chmodSync(certificatePath, 0o600);
    chmodSync(archive, 0o600);
    rmSync(config);
    const certificate = new X509Certificate(readFileSync(certificatePath));
    const certificateSha256 = createHash('sha256').update(certificate.raw).digest('hex');
    const policy = { schemaVersion: 1, mode: 'self-signed', identifier: MACOS_IDENTIFIER, certificateSha256, certificateFile: 'release/macos-signing-certificate.pem' };
    validateSigningPolicy(policy, certificate.toString());
    writeFileSync(join(output, 'public-policy.json'), `${JSON.stringify(policy, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
    for (const file of [passwordPath, keyPath, archive]) assert.equal(statSync(file).mode & 0o077, 0);
    return { directory: output, certificateSha256, validTo: certificate.validTo, publicPolicy: policy };
  } catch (error) {
    // Preparation creates a brand-new directory and has never imported a key.
    // Failed partial identities must not accidentally become release signers.
    rmSync(output, { recursive: true, force: true });
    throw error;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  assert.equal(process.argv.length, 3, 'Pass one new absolute protected directory; this command never uploads or imports keys');
  const result = provisionIdentity(process.argv[2]);
  console.log(`Created encrypted persistent identity outside the checkout. Public SHA-256: ${result.certificateSha256}. Expires: ${result.validTo}.`);
  console.log('Review and copy only certificate.pem and public-policy.json into the release/ public paths. Protect identity.p12, encrypted private key and password.txt independently; never commit or print them. No keychain, app or GitHub configuration was changed.');
}
