// Offline Tauri/minisign verification. No signing key, network or installer.
// Wire format and authenticated comment semantics follow minisign-verify 0.3.0
// and tauri-plugin-updater 2.14.0 src/updater.rs::verify_signature.
import { createHash, createPublicKey, verify } from 'node:crypto';
import { closeSync, constants, fstatSync, lstatSync, openSync, readSync } from 'node:fs';

export const MAX_LEGACY_PAYLOAD_BYTES = 512 * 1024 * 1024;

export function decodeBase64(value, description) {
  if (typeof value !== 'string' || !value || value.length > 16_384 || value.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(value)) {
    throw new Error('Invalid ' + description + ': expected Tauri base64 contents');
  }
  const bytes = Buffer.from(value, 'base64');
  if (bytes.toString('base64') !== value) throw new Error('Invalid ' + description + ': noncanonical base64');
  return bytes;
}

function decodedLines(encoded, description) {
  const text = new TextDecoder('utf-8', { fatal: true }).decode(decodeBase64(typeof encoded === 'string' ? encoded.trim() : encoded, description));
  return text.trimEnd().split(/\r?\n/);
}

export function parsePublicKey(encoded) {
  const lines = decodedLines(encoded, 'updater public key');
  if (lines.length !== 2 || !lines[0].startsWith('untrusted comment:')) throw new Error('Invalid updater public key format');
  const body = decodeBase64(lines[1], 'updater public key body');
  if (body.length !== 42 || body.subarray(0, 2).toString() !== 'Ed') throw new Error('Invalid updater public key body');
  return {
    keyId: body.subarray(2, 10),
    key: createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), body.subarray(10)]), format: 'der', type: 'spki' }),
  };
}

export function parseSignature(encoded) {
  const lines = decodedLines(encoded, 'updater signature');
  if (lines.length !== 4 || !lines[0].startsWith('untrusted comment:') || !lines[2].startsWith('trusted comment: ')) throw new Error('Invalid updater signature format');
  const body = decodeBase64(lines[1], 'updater signature body');
  const global = decodeBase64(lines[3], 'updater signature global body');
  const algorithm = body.subarray(0, 2).toString();
  if (body.length !== 74 || !['Ed', 'ED'].includes(algorithm) || global.length !== 64) throw new Error('Invalid updater signature body');
  return { encoded: encoded.trim(), keyId: body.subarray(2, 10), signature: body.subarray(10), global, algorithm, trustedComment: lines[2].slice(17) };
}

export function assertReleaseVersion(trustedComment, version) {
  // Publication keeps its existing stricter canonical rule: exactly one field,
  // equal to the checked release version. Call only after crypto verification
  // when this comment is used as an authenticated statement.
  const versions = trustedComment.split('\t').filter(field => field.startsWith('version:')).map(field => field.slice(8));
  if (versions.length !== 1 || versions[0] !== version) throw new Error('Updater signature must be bound to the release version');
}

function withRegularAsset(path, callback) {
  let info;
  try { info = lstatSync(path); } catch { throw new Error('Missing signed updater asset: ' + path); }
  if (!info.isFile() || info.isSymbolicLink() || info.size === 0) throw new Error('Updater asset is not a nonempty regular file: ' + path);
  const fd = openSync(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0));
  try {
    const opened = fstatSync(fd);
    if (!opened.isFile() || opened.size === 0 || opened.dev !== info.dev || opened.ino !== info.ino) throw new Error('Updater asset changed before verification: ' + path);
    const result = callback(fd, opened);
    const after = fstatSync(fd);
    if (after.size !== opened.size || after.mtimeMs !== opened.mtimeMs || after.ctimeMs !== opened.ctimeMs) throw new Error('Updater asset changed during verification: ' + path);
    return result;
  } finally { closeSync(fd); }
}

function readExact(fd, size) {
  const bytes = Buffer.allocUnsafe(size);
  let offset = 0;
  while (offset < size) {
    const count = readSync(fd, bytes, offset, size - offset);
    if (!count) throw new Error('Updater asset changed during verification');
    offset += count;
  }
  if (readSync(fd, Buffer.alloc(1), 0, 1)) throw new Error('Updater asset grew during verification');
  return bytes;
}

export function readSignatureAsset(path) {
  return withRegularAsset(path, (fd, info) => {
    if (info.size > 16_384) throw new Error('Updater signature file exceeds 16 KiB');
    return new TextDecoder('utf-8', { fatal: true }).decode(readExact(fd, info.size));
  });
}

export function verifyUpdaterPayload({ path, signature: encodedSignature, publicKey, version }) {
  const key = parsePublicKey(publicKey);
  const signature = parseSignature(encodedSignature);
  if (!signature.keyId.equals(key.keyId)) throw new Error('Updater signature does not match the configured public key');
  withRegularAsset(path, (fd, info) => {
    let message;
    if (signature.algorithm === 'ED') {
      const hash = createHash('blake2b512');
      const chunk = Buffer.allocUnsafe(64 * 1024);
      let received = 0, count;
      while ((count = readSync(fd, chunk, 0, chunk.length))) {
        received += count;
        if (received > info.size) throw new Error('Updater asset grew during verification');
        hash.update(chunk.subarray(0, count));
      }
      if (received !== info.size) throw new Error('Updater asset changed during verification');
      message = hash.digest();
    } else {
      // Minisign's older Ed mode signs the complete message, as supported by
      // Tauri with allow_legacy=true. Official current packages use ED instead.
      if (info.size > MAX_LEGACY_PAYLOAD_BYTES) throw new Error('Legacy Minisign Ed payload exceeds 512 MiB; re-sign the package using current streaming ED mode');
      message = readExact(fd, info.size);
    }
    if (!verify(null, message, key.key, signature.signature)) throw new Error('Updater payload cryptographic signature verification failed: ' + path);
    const authenticatedComment = Buffer.concat([signature.signature, Buffer.from(signature.trustedComment, 'utf8')]);
    if (!verify(null, authenticatedComment, key.key, signature.global)) throw new Error('Updater trusted-comment cryptographic signature verification failed: ' + path);
  });
  assertReleaseVersion(signature.trustedComment, version);
  return signature.encoded;
}
