import { existsSync, lstatSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseVersion, verifyTag } from './release-info.mjs';

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

function decodeBase64(value, description) {
  if (!value || value.length > 16_384 || value.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(value)) {
    throw new Error(`Invalid ${description}: expected Tauri base64 contents`);
  }
  const bytes = Buffer.from(value, 'base64');
  if (bytes.toString('base64') !== value) throw new Error(`Invalid ${description}: noncanonical base64`);
  return bytes;
}

export function publicKeyId(encoded) {
  const lines = decodeBase64(encoded?.trim(), 'updater public key').toString('utf8').trim().split(/\r?\n/);
  if (lines.length !== 2 || !lines[0].startsWith('untrusted comment:')) throw new Error('Invalid updater public key format');
  const key = decodeBase64(lines[1], 'updater public key body');
  if (key.length !== 42 || key.subarray(0, 2).toString() !== 'Ed') throw new Error('Invalid updater public key body');
  return key.subarray(2, 10);
}

export function updaterSignature(encoded, keyId, version) {
  const signature = encoded.trim();
  const lines = decodeBase64(signature, 'updater signature').toString('utf8').trim().split(/\r?\n/);
  if (lines.length !== 4 || !lines[0].startsWith('untrusted comment:') || !lines[2].startsWith('trusted comment:')) {
    throw new Error('Invalid updater signature format');
  }
  const body = decodeBase64(lines[1], 'updater signature body');
  const global = decodeBase64(lines[3], 'updater signature global body');
  if (body.length !== 74 || !['Ed', 'ED'].includes(body.subarray(0, 2).toString()) || global.length !== 64) {
    throw new Error('Invalid updater signature body');
  }
  if (!body.subarray(2, 10).equals(keyId)) throw new Error('Updater signature does not match the configured public key');
  const boundVersions = lines[2].split('\t').filter(field => field.startsWith('version:')).map(field => field.slice('version:'.length));
  if (boundVersions.length !== 1 || boundVersions[0] !== version) throw new Error('Updater signature must be bound to the release version');
  return signature;
}

function requireRegularAsset(path) {
  if (!existsSync(path)) throw new Error(`Missing signed updater asset: ${path}`);
  const info = lstatSync(path);
  if (!info.isFile() || info.isSymbolicLink() || info.size === 0) throw new Error(`Updater asset is not a nonempty regular file: ${path}`);
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
  const keyId = publicKeyId(config.plugins?.updater?.pubkey);
  const platforms = {};
  for (const [target, [platform, suffix]] of Object.entries(updaterArtifacts)) {
    const asset = `OrbitPal-v${version}-${platform}${suffix}`;
    const path = join(directory, asset);
    requireRegularAsset(path);
    requireRegularAsset(`${path}.sig`);
    platforms[target] = {
      signature: updaterSignature(readFileSync(`${path}.sig`, 'utf8'), keyId, version),
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
    console.log(`latest.json includes ${Object.keys(manifest.platforms).length} complete signed updater targets for ${version}.`);
  }
}
