import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseVersion, verifyTag } from './release-info.mjs';
import { assertReleaseVersion, parsePublicKey, parseSignature, readSignatureAsset, verifyUpdaterPayload } from './verify-updater-payload.mjs';

// Keep the generic Windows entry for compatibility. Current updaters prefer the
// installer-specific entry, so an MSI installation stays on MSI.
export const updaterArtifacts = {
  'darwin-aarch64': ['macos-arm64', '.app.tar.gz'],
  'darwin-x86_64': ['macos-x64', '.app.tar.gz'],
  'linux-x86_64': ['linux-x64', '.AppImage'],
  'windows-x86_64': ['windows-x64', '-setup.exe'],
  'windows-x86_64-nsis': ['windows-x64', '-setup.exe'],
  'windows-x86_64-msi': ['windows-x64', '.msi'],
};

// Keep initial download URLs acceptable to installed 0.6/0.7 clients after the
// repository rename. GitHub redirects this exact former slug to the same repo.
// Never recreate/transfer the legacy repository or accept an asset-URL override.
export const canonicalRepository = 'ralphschuler/orbitpal';
export const legacyAssetRepository = 'ralphschuler/streamer-ai-cohost';

export function releaseRepository(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository || '') || repository.split('/').some(part => ['.', '..'].includes(part))) {
    throw new Error('Expected a GitHub OWNER/REPOSITORY name');
  }
  if (repository !== canonicalRepository) throw new Error('Releases must come from the official ralphschuler/orbitpal repository');
  return repository;
}

export function publicKeyId(encoded) {
  return parsePublicKey(encoded).keyId;
}

// Structural helper for format fixtures. Publication always calls the complete
// payload verifier below before trusting any version metadata.
export function updaterSignature(encoded, keyId, version) {
  const signature = parseSignature(encoded);
  if (!signature.keyId.equals(keyId)) throw new Error('Updater signature does not match the configured public key');
  assertReleaseVersion(signature.trustedComment, version);
  return signature.encoded;
}

export function createUpdaterManifest({ root = process.cwd(), directory = join(root, 'release-artifacts'), repository, tag, date = new Date().toISOString(), notes }) {
  releaseRepository(repository);
  const version = releaseVersion(root);
  verifyTag(tag, version);
  if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(date) || Number.isNaN(Date.parse(date))) {
    throw new Error('Updater publication date must be RFC 3339');
  }
  if (typeof notes !== 'string' || Buffer.byteLength(notes, 'utf8') > 32_768) throw new Error('Updater notes must be text of at most 32 KiB');
  const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
  if (config.bundle?.createUpdaterArtifacts !== true) throw new Error('Releases require native Tauri v2 updater artifacts');
  if (config.plugins?.updater?.requireSignedVersion !== true) throw new Error('Releases require signed updater version binding');
  const publicKey = config.plugins?.updater?.pubkey;
  parsePublicKey(publicKey);
  const verified = new Map();
  const platforms = {};
  for (const [target, [platform, suffix]] of Object.entries(updaterArtifacts)) {
    const asset = `OrbitPal-v${version}-${platform}${suffix}`;
    const path = join(directory, asset);
    if (!verified.has(asset)) {
      const signature = readSignatureAsset(path + '.sig');
      verified.set(asset, verifyUpdaterPayload({ path, signature, publicKey, version }));
    }
    platforms[target] = {
      signature: verified.get(asset),
      url: `https://github.com/${legacyAssetRepository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(asset)}`,
    };
  }
  return { version, notes, pub_date: date, platforms };
}

// A delayed manual rerun must not replace a newer stable release at /latest.
// Pre-release versions never become the production update channel.
export async function assertCanPublishLatest({ repository, version, token, fetcher = fetch }) {
  releaseRepository(repository);
  if (!/^\d+\.\d+\.\d+$/.test(version)) return false;
  const response = await fetcher(`https://api.github.com/repos/${repository}/releases/latest`, {
    headers: { Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    signal: AbortSignal.timeout(20_000),
  });
  if (response.status === 404) return true;
  if (!response.ok) throw new Error(`Could not verify the current stable release (HTTP ${response.status})`);
  const latest = await response.json();
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(latest.tag_name || '');
  if (!match || latest.draft || latest.prerelease) throw new Error('Current latest release has unexpected version metadata');
  const current = match.slice(1).map(BigInt);
  const next = version.split('.').map(BigInt);
  for (let i = 0; i < next.length; i++) {
    if (next[i] > current[i]) return true;
    if (next[i] < current[i]) throw new Error('Refusing to replace a newer stable release with an older version');
  }
  throw new Error('Refusing to republish the current stable version');
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const root = process.cwd();
  const version = releaseVersion(root);
  const tag = process.env.RELEASE_TAG;
  verifyTag(tag, version);
  const repository = process.env.GITHUB_REPOSITORY;
  if (process.argv.includes('--check-latest')) {
    const latest = await assertCanPublishLatest({ repository, version, token: process.env.GH_TOKEN });
    console.log(latest ? 'Stable release version is newer than the current channel.' : 'Prerelease will not change the stable update channel.');
  } else {
    const directory = resolve(process.argv[2] || 'release-artifacts');
    const notes = readFileSync(join(root, 'docs/UPDATE-NOTES.md'), 'utf8');
    const manifest = createUpdaterManifest({ root, directory, repository, tag, notes });
    writeFileSync(join(directory, 'latest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`latest.json includes ${Object.keys(manifest.platforms).length} cryptographically verified updater targets for ${version}.`);
  }
}
