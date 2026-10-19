// Tests only. Real throwaway Ed25519 keys stay in process memory and are never
// serialized, logged or made available to the publication verifier.
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';

export function createSigningFixture(keyId = randomBytes(8)) {
  const { publicKey: key, privateKey } = generateKeyPairSync('ed25519');
  const raw = key.export({ format: 'der', type: 'spki' }).subarray(-32);
  const encode = value => Buffer.from(value).toString('base64');
  const publicKey = encode('untrusted comment: throwaway test key\n' + Buffer.concat([Buffer.from('Ed'), keyId, raw]).toString('base64') + '\n');
  function signature(version = '0.4.0', id = keyId, data = Buffer.from('fixture payload'), { algorithm = 'ED', trustedComment } = {}) {
    const comment = trustedComment ?? 'timestamp:1\tfile:fixture\tversion:' + version;
    const message = algorithm === 'ED' ? createHash('blake2b512').update(data).digest() : data;
    const primary = sign(null, message, privateKey);
    const global = sign(null, Buffer.concat([primary, Buffer.from(comment)]), privateKey);
    return encode('untrusted comment: throwaway test signature\n' +
      Buffer.concat([Buffer.from(algorithm), id, primary]).toString('base64') +
      '\ntrusted comment: ' + comment + '\n' + global.toString('base64') + '\n');
  }
  return { keyId, publicKey, signature };
}
