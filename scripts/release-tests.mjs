import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { releaseVersion, verifyTag } from './release-info.mjs';
import { collect, platformForTarget } from './collect-release.mjs';
import { rustInventory, nodeInventory } from './dependency-inventories.mjs';
import { assertCanPublishLatest, createUpdaterManifest, updaterArtifacts, updaterSignature } from './updater-manifest.mjs';

const keyId = Buffer.from('12345678');
const publicKey = Buffer.from(`untrusted comment: fixture key\n${Buffer.concat([Buffer.from('Ed'), keyId, Buffer.alloc(32, 1)]).toString('base64')}\n`).toString('base64');

// Structural packaging fixtures, deliberately not cryptographic signatures.
// Artifact verification is performed by the native updater before installation.
function signature(version = '0.4.0', id = keyId) {
  return Buffer.from(`untrusted comment: signature fixture\n${Buffer.concat([Buffer.from('ED'), id, Buffer.alloc(64, 2)]).toString('base64')}\ntrusted comment: timestamp:1\tfile:fixture\tversion:${version}\n${Buffer.alloc(64, 3).toString('base64')}\n`).toString('base64');
}

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'cohost-release-'));
  mkdirSync(join(root, 'src-tauri'));
  writeFileSync(join(root, 'package.json'), '{"version":"0.4.0"}');
  writeFileSync(join(root, 'src-tauri/tauri.conf.json'), JSON.stringify({ version: '0.4.0', bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: publicKey, requireSignedVersion: true } } }));
  writeFileSync(join(root, 'src-tauri/Cargo.toml'), '[package]\nname = "streamer-ai-cohost"\nversion = "0.4.0"\n[dependencies]\n');
  writeFileSync(join(root, 'src-tauri/Cargo.lock'), '[[package]]\nname = "streamer-ai-cohost"\nversion = "0.4.0"\n');
  return root;
}

test('release refuses mismatched manifests and nonversion tags', () => {
  const root = fixture();
  try {
    assert.equal(releaseVersion(root), '0.4.0');
    assert.throws(() => verifyTag('main', '0.4.0'));
    assert.throws(() => verifyTag('v0.3.3', '0.4.0'));
    assert.equal(verifyTag('v0.4.0-rc.1', '0.4.0-rc.1').prerelease, true);
    writeFileSync(join(root, 'src-tauri/tauri.conf.json'), '{"version":"0.3.3"}');
    assert.throws(() => releaseVersion(root), /differs/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('architecture names stay distinct and unsupported targets are rejected', () => {
  assert.equal(platformForTarget('aarch64-apple-darwin'), 'macos-arm64');
  assert.equal(platformForTarget('x86_64-apple-darwin'), 'macos-x64');
  assert.throws(() => platformForTarget('../../another-target'));
});

test('dependency inventories omit build-machine paths and URL credentials', () => {
  const rust = rustInventory({ packages: [{ name: 'fixture', version: '1.0.0', license: 'MIT', manifest_path: '/private/home/Cargo.toml', target_directory: '/private/build', license_file: '/private/home/LICENSE', repository: 'https://user:password@example.invalid/repo?token=private', source: 'registry+https://github.com/rust-lang/crates.io-index', authors: ['Fixture author'] }] });
  assert.equal(rust[0].license_file, 'LICENSE');
  assert.equal(rust[0].repository, 'https://example.invalid/repo');
  assert.ok(!JSON.stringify(rust).includes('/private'));
  assert.ok(!JSON.stringify(rust).includes('password'));
  const node = nodeInventory({ MIT: [{ name: 'fixture', versions: ['1.0.0'], paths: ['/private/node_modules/fixture'], license: 'MIT' }] });
  assert.ok(!JSON.stringify(node).includes('/private'));
});

test('collector selects exactly the requested platform packages', () => {
  const root = fixture();
  try {
    const bundle = join(root, 'src-tauri/target/x86_64-pc-windows-msvc/release/bundle');
    mkdirSync(join(bundle, 'nsis'), { recursive: true });
    mkdirSync(join(bundle, 'msi'));
    writeFileSync(join(bundle, 'nsis/installer.exe'), 'installer fixture');
    writeFileSync(join(bundle, 'nsis/installer.exe.sig'), signature());
    writeFileSync(join(bundle, 'msi/installer.msi'), 'msi fixture');
    writeFileSync(join(bundle, 'msi/installer.msi.sig'), signature());
    const output = join(root, 'downloads');
    assert.deepEqual(collect(root, 'x86_64-pc-windows-msvc', output), ['Streamer-AI-Cohost-v0.4.0-windows-x64-setup.exe', 'Streamer-AI-Cohost-v0.4.0-windows-x64-setup.exe.sig', 'Streamer-AI-Cohost-v0.4.0-windows-x64.msi', 'Streamer-AI-Cohost-v0.4.0-windows-x64.msi.sig']);
    assert.equal(readFileSync(join(output, 'Streamer-AI-Cohost-v0.4.0-windows-x64.msi'), 'utf8'), 'msi fixture');
    writeFileSync(join(bundle, 'msi/duplicate.msi'), 'duplicate');
    assert.throws(() => collect(root, 'x86_64-pc-windows-msvc', output), /exactly one/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('collector refuses unsigned packages and includes Linux AppImage signatures', () => {
  const root = fixture();
  try {
    const bundle = join(root, 'src-tauri/target/x86_64-unknown-linux-gnu/release/bundle');
    mkdirSync(join(bundle, 'appimage'), { recursive: true });
    mkdirSync(join(bundle, 'deb'));
    writeFileSync(join(bundle, 'appimage/cohost.AppImage'), 'appimage fixture');
    writeFileSync(join(bundle, 'deb/cohost.deb'), 'deb fixture');
    assert.throws(() => collect(root, 'x86_64-unknown-linux-gnu'), /exactly one .AppImage.sig/);
    writeFileSync(join(bundle, 'appimage/cohost.AppImage.sig'), signature());
    assert.deepEqual(collect(root, 'x86_64-unknown-linux-gnu'), ['Streamer-AI-Cohost-v0.4.0-linux-x64.AppImage', 'Streamer-AI-Cohost-v0.4.0-linux-x64.AppImage.sig', 'Streamer-AI-Cohost-v0.4.0-linux-x64.deb']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function manifestFixture() {
  const root = fixture();
  const directory = join(root, 'downloads');
  mkdirSync(directory);
  for (const [platform, suffix] of Object.values(updaterArtifacts)) {
    const path = join(directory, `Streamer-AI-Cohost-v0.4.0-${platform}${suffix}`);
    writeFileSync(path, `${platform} fixture`);
    writeFileSync(`${path}.sig`, signature());
  }
  return { root, directory, repository: 'ralphschuler/streamer-ai-cohost', tag: 'v0.4.0', date: '2026-10-19T10:00:00Z', notes: 'Fixture release notes.' };
}

test('manifest maps both mac architectures, Linux and both Windows installers to immutable release URLs', () => {
  const options = manifestFixture();
  try {
    const manifest = createUpdaterManifest(options);
    assert.equal(manifest.version, '0.4.0');
    assert.equal(manifest.pub_date, options.date);
    assert.deepEqual(Object.keys(manifest.platforms), Object.keys(updaterArtifacts));
    assert.equal(manifest.platforms['darwin-aarch64'].url, 'https://github.com/ralphschuler/streamer-ai-cohost/releases/download/v0.4.0/Streamer-AI-Cohost-v0.4.0-macos-arm64.app.tar.gz');
    assert.match(manifest.platforms['darwin-x86_64'].url, /macos-x64.app.tar.gz$/);
    assert.match(manifest.platforms['linux-x86_64'].url, /linux-x64.AppImage$/);
    assert.match(manifest.platforms['windows-x86_64-msi'].url, /windows-x64.msi$/);
    assert.match(manifest.platforms['windows-x86_64-nsis'].url, /windows-x64-setup.exe$/);
    assert.deepEqual(manifest.platforms['windows-x86_64'], manifest.platforms['windows-x86_64-nsis']);
    assert.equal(manifest.platforms['linux-x86_64'].signature, signature());
    assert.ok(!JSON.stringify(manifest).includes(options.root));
  } finally { rmSync(options.root, { recursive: true, force: true }); }
});

test('manifest fails closed for incomplete targets, bad keys, unsigned versions and invalid metadata', () => {
  const options = manifestFixture();
  const payload = join(options.directory, 'Streamer-AI-Cohost-v0.4.0-macos-x64.app.tar.gz');
  try {
    assert.throws(() => createUpdaterManifest({ ...options, repository: '../other/repository' }), /OWNER/);
    assert.throws(() => createUpdaterManifest({ ...options, tag: 'v0.4.1' }), /exactly/);
    assert.throws(() => createUpdaterManifest({ ...options, date: 'tomorrow' }), /RFC 3339/);
    assert.throws(() => createUpdaterManifest({ ...options, notes: 'a'.repeat(32_769) }), /32 KiB/);
    rmSync(`${payload}.sig`);
    assert.throws(() => createUpdaterManifest(options), /Missing signed/);
    writeFileSync(`${payload}.sig`, 'https://example.invalid/artifact.sig');
    assert.throws(() => createUpdaterManifest(options), /base64/);
    writeFileSync(`${payload}.sig`, signature('0.4.0', Buffer.from('otherkey')));
    assert.throws(() => createUpdaterManifest(options), /public key/);
    writeFileSync(`${payload}.sig`, signature('0.3.0'));
    assert.throws(() => createUpdaterManifest(options), /bound to the release version/);
    writeFileSync(`${payload}.sig`, signature());
    writeFileSync(payload, '');
    assert.throws(() => createUpdaterManifest(options), /nonempty regular file/);
    writeFileSync(payload, 'restored payload');
    const configPath = join(options.root, 'src-tauri/tauri.conf.json');
    const config = JSON.parse(readFileSync(configPath, 'utf8'));
    config.plugins.updater.requireSignedVersion = false;
    writeFileSync(configPath, JSON.stringify(config));
    assert.throws(() => createUpdaterManifest(options), /version binding/);
  } finally { rmSync(options.root, { recursive: true, force: true }); }
});

test('signatures require one signed release version and canonical minisign contents', () => {
  const noVersion = Buffer.from(Buffer.from(signature(), 'base64').toString().replace('\tversion:0.4.0', '')).toString('base64');
  assert.throws(() => updaterSignature(noVersion, keyId, '0.4.0'), /release version/);
  const duplicate = Buffer.from(Buffer.from(signature(), 'base64').toString().replace('\tversion:0.4.0', '\tversion:0.4.0\tversion:0.4.0')).toString('base64');
  assert.throws(() => updaterSignature(duplicate, keyId, '0.4.0'), /release version/);
  assert.throws(() => updaterSignature('not a signature', keyId, '0.4.0'), /base64/);
});

test('manifest rejects symlinked payloads on Unix', { skip: process.platform === 'win32' }, () => {
  const options = manifestFixture();
  try {
    const payload = join(options.directory, 'Streamer-AI-Cohost-v0.4.0-macos-x64.app.tar.gz');
    rmSync(payload);
    symlinkSync(join(options.directory, 'Streamer-AI-Cohost-v0.4.0-macos-arm64.app.tar.gz'), payload);
    assert.throws(() => createUpdaterManifest(options), /nonempty regular file/);
  } finally { rmSync(options.root, { recursive: true, force: true }); }
});

test('stable publication prevents downgrades, reruns, prerelease promotion and API failures', async () => {
  const repository = 'ralphschuler/streamer-ai-cohost';
  const stable = async () => ({ ok: true, status: 200, json: async () => ({ tag_name: 'v0.4.0', draft: false, prerelease: false }) });
  assert.equal(await assertCanPublishLatest({ repository, version: '0.4.1', fetcher: stable }), true);
  assert.equal(await assertCanPublishLatest({ repository, version: '0.10.0', fetcher: stable }), true);
  await assert.rejects(assertCanPublishLatest({ repository, version: '0.3.9', fetcher: stable }), /newer stable/);
  await assert.rejects(assertCanPublishLatest({ repository, version: '0.4.0', fetcher: stable }), /republish/);
  assert.equal(await assertCanPublishLatest({ repository, version: '0.5.0-rc.1', fetcher: async () => { throw new Error('Prereleases must not request latest'); } }), false);
  assert.equal(await assertCanPublishLatest({ repository, version: '0.1.0', fetcher: async () => ({ status: 404 }) }), true);
  await assert.rejects(assertCanPublishLatest({ repository, version: '0.5.0', fetcher: async () => ({ ok: false, status: 403 }) }), /HTTP 403/);
  await assert.rejects(assertCanPublishLatest({ repository, version: '0.5.0', fetcher: async () => ({ ok: true, status: 200, json: async () => ({ tag_name: 'main' }) }) }), /unexpected version/);
});
