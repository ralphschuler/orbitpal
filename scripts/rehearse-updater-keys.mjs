// Offline operational rehearsal only. Never imports a configured key, environment
// secret, keychain entry or production artifact. This does not install an updater.
import assert from 'node:assert/strict';
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, sign, verify } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

function throwawayKey() {
  const pair = generateKeyPairSync('ed25519');
  const id = randomBytes(8);
  const raw = pair.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32);
  const text = 'untrusted comment: throwaway rehearsal key\n' + Buffer.concat([Buffer.from('Ed'), id, raw]).toString('base64') + '\n';
  return { ...pair, id, tauriPublicKey: Buffer.from(text).toString('base64') };
}

function signedFixture(key, payload, version) {
  const digest = createHash('blake2b512').update(payload).digest();
  const signature = sign(null, digest, key.privateKey);
  const comment = `timestamp:1\tfile:throwaway-fixture\tversion:${version}`;
  const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), key.privateKey);
  const text = 'untrusted comment: throwaway rehearsal signature\n'
    + Buffer.concat([Buffer.from('ED'), key.id, signature]).toString('base64')
    + '\ntrusted comment: ' + comment + '\n' + global.toString('base64') + '\n';
  return { digest, signature, tauriSignature: Buffer.from(text).toString('base64') };
}

export function rehearseUpdaterKeys({ publicFixtures } = {}) {
  const a = throwawayKey();
  const b = throwawayKey();
  const passphrase = randomBytes(32);
  const wrongPassphrase = randomBytes(32);
  const backup = a.privateKey.export({ format: 'der', type: 'pkcs8', cipher: 'aes-256-cbc', passphrase });
  let restored;
  try {
    restored = createPrivateKey({ key: backup, format: 'der', type: 'pkcs8', passphrase });
    assert.throws(() => createPrivateKey({ key: backup, format: 'der', type: 'pkcs8', passphrase: wrongPassphrase }));
    assert.deepEqual(createPublicKey(restored).export({ format: 'der', type: 'spki' }), a.publicKey.export({ format: 'der', type: 'spki' }));
  } finally {
    // The encrypted backup and its password exist only for this test process.
    backup.fill(0);
    passphrase.fill(0);
    wrongPassphrase.fill(0);
  }
  const bridgeVersion = '9000.0.1';
  const nextVersion = '9000.0.2';
  // This models the trust transition. It is JSON, not an installable application:
  // the real bridge must embed B's public key in a tested native binary signed by A.
  const bridge = Buffer.from(JSON.stringify({ fixtureOnly: true, version: bridgeVersion, embeddedUpdaterPublicKey: b.tauriPublicKey }));
  const next = Buffer.from(JSON.stringify({ fixtureOnly: true, version: nextVersion }));
  const signedBridge = signedFixture({ ...a, privateKey: restored }, bridge, bridgeVersion);
  const signedNext = signedFixture(b, next, nextVersion);
  assert.equal(verify(null, signedBridge.digest, a.publicKey, signedBridge.signature), true);
  assert.equal(verify(null, signedBridge.digest, b.publicKey, signedBridge.signature), false);
  assert.equal(verify(null, signedNext.digest, a.publicKey, signedNext.signature), false);
  assert.equal(verify(null, signedNext.digest, b.publicKey, signedNext.signature), true);
  assert.equal(verify(null, createHash('blake2b512').update(Buffer.concat([bridge, Buffer.from('tampered')])).digest(), a.publicKey, signedBridge.signature), false);
  if (publicFixtures) {
    // A new directory prevents overwriting existing files. Only public keys,
    // dummy payloads and detached signatures leave process memory.
    mkdirSync(publicFixtures, { mode: 0o700 });
    for (const [name, content] of Object.entries({
      'a.pub': a.tauriPublicKey, 'b.pub': b.tauriPublicKey,
      'bridge.fixture': bridge, 'bridge.fixture.sig': signedBridge.tauriSignature,
      'next.fixture': next, 'next.fixture.sig': signedNext.tauriSignature,
    })) writeFileSync(join(publicFixtures, name), content, { mode: 0o600, flag: 'wx' });
  }
  return { throwawayOnly: true, encryptedBackupRestored: true, wrongPasswordRejected: true,
    recoveredKeySignsBridge: true, oldTrustRejectsNewKey: true, bridgeEnablesNewTrust: true,
    tamperRejected: true, installerTransitionTested: false };
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  if (process.argv.length > 2 && (process.argv[2] !== '--public-fixtures' || process.argv.length !== 4)) {
    throw new Error('Usage: node scripts/rehearse-updater-keys.mjs [--public-fixtures NEW_DIRECTORY]');
  }
  console.log(JSON.stringify(rehearseUpdaterKeys({ publicFixtures: process.argv[3] && resolve(process.argv[3]) })));
}
