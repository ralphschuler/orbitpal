import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { releaseVersion, verifyTag } from './release-info.mjs';
import { collect, platformForTarget } from './collect-release.mjs';
import { rustInventory, nodeInventory } from './dependency-inventories.mjs';
import { assertCanPublishLatest, createUpdaterManifest, updaterArtifacts, updaterSignature, canonicalRepository, legacyAssetRepository } from './updater-manifest.mjs';
import { assertMicrophonePermissions, assertMacosIdentity } from './verify-macos-permissions.mjs';
import { validateSigningPolicy, designatedRequirement, assertPinnedRequirement, signingCommand, quoteSecurityInput } from './macos-signing-policy.mjs';
import { requireDisposableMac } from './macos-release-signing.mjs';

test('persistent macOS signing fails closed before reading an unprovisioned or misdirected certificate', () => {
  const policy = { schemaVersion: 1, mode: 'self-signed', identifier: 'studio.streamer-ai.cohost', certificateSha256: 'a'.repeat(64), certificateFile: 'release/macos-signing-certificate.pem' };
  for (const change of [{ mode: 'adhoc' }, { certificateSha256: null }, { certificateSha256: 'name only' }, { certificateFile: '../private-key' }, { identifier: 'new.identity' }, { schemaVersion: 2 }]) {
    assert.throws(() => validateSigningPolicy({ ...policy, ...change }, 'certificate parsing must never be reached'), /policy|persistent|identifier|certificate|pin/i);
  }
});

test('macOS designated requirements reject name-only, binary-specific and weakened signing identities', () => {
  const identity = { identifier: 'studio.streamer-ai.cohost', sha1: 'a'.repeat(40) };
  const expected = designatedRequirement(identity);
  assert.doesNotThrow(() => assertPinnedRequirement(`# ${expected}\n`, identity));
  assert.doesNotThrow(() => assertPinnedRequirement(expected.replace('a'.repeat(40), 'A'.repeat(40)), identity));
  for (const invalid of [
    'designated => identifier "studio.streamer-ai.cohost"',
    'designated => identifier "studio.streamer-ai.cohost" and certificate leaf[subject.CN] = "OrbitPal Persistent Release Signing"',
    `designated => cdhash H"${identity.sha1}"`,
    expected.replace(identity.sha1, 'b'.repeat(40)),
    `${expected} or true`,
    `${expected}\n${expected}`,
  ]) assert.throws(() => assertPinnedRequirement(invalid, identity));
});

test('macOS signer pins the exact key before bundling and never puts passwords into executable arguments', () => {
  const context = { sha1: 'a'.repeat(40), releaseDirectory: '/build/release', app: '/build/release/bundle/macos/OrbitPal.app', keychain: '/private/identity.keychain-db', requirements: '/private/designated.req' };
  const args = ['--force', '-s', context.sha1, '--options', 'runtime', context.app];
  const result = signingCommand(args, context, context.app);
  assert.equal(result.at(-1), context.app);
  assert.deepEqual(result.slice(5, -1), ['--keychain', context.keychain, '--timestamp=none', '--identifier', 'studio.streamer-ai.cohost', '--requirements', context.requirements]);
  for (const change of [args.map(value => value === context.sha1 ? 'same certificate name' : value), ['-s', context.sha1, '-s', context.sha1, context.app], ['-s', context.sha1, '-r=identifier only', context.app]]) {
    assert.throws(() => signingCommand(change, context, context.app));
  }
  assert.throws(() => signingCommand(args, context, '/build/release-escape/other.app'));
  assert.equal(quoteSecurityInput('quote"slash\\'), '"quote\\"slash\\\\"');
  for (const input of ['line\nbreak', 'line\rbreak', 'zero\0byte']) assert.throws(() => quoteSecurityInput(input));
});

test('private macOS signing cannot run on a user or self-hosted machine', () => {
  const env = { GITHUB_ACTIONS: 'true', RUNNER_ENVIRONMENT: 'github-hosted', RUNNER_TEMP: '/temporary', GITHUB_RUN_ID: '123', GITHUB_RUN_ATTEMPT: '1' };
  assert.doesNotThrow(() => requireDisposableMac(env, 'darwin'));
  assert.throws(() => requireDisposableMac({ ...env, GITHUB_ACTIONS: '' }, 'darwin'));
  assert.throws(() => requireDisposableMac({ ...env, RUNNER_ENVIRONMENT: 'self-hosted' }, 'darwin'));
  assert.throws(() => requireDisposableMac(env, 'linux'));
});

test('macOS packaging preserves app and code-signing identities across versions', () => {
  const config = { identifier: 'studio.streamer-ai.cohost', productName: 'OrbitPal', version: '0.9.0' };
  const info = { CFBundleIdentifier: config.identifier, CFBundleName: config.productName, CFBundleExecutable: 'orbitpal', CFBundleShortVersionString: config.version };
  const details = `Identifier=${config.identifier}\nSignature=adhoc\nTeamIdentifier=not set\n`;
  assert.equal(assertMacosIdentity(config, info, details).adHoc, true);
  assert.doesNotThrow(() => assertMacosIdentity({ ...config, version: '0.10.0' }, { ...info, CFBundleShortVersionString: '0.10.0' }, details));
  for (const field of ['CFBundleIdentifier', 'CFBundleName', 'CFBundleExecutable', 'CFBundleShortVersionString']) {
    assert.throws(() => assertMacosIdentity(config, { ...info, [field]: 'different' }, details));
  }
  assert.throws(() => assertMacosIdentity({ ...config, identifier: 'new.identifier' }, info, details));
  assert.throws(() => assertMacosIdentity(config, info, 'Identifier=another.signed.app\nSignature=adhoc\n'));
});

test('macOS microphone requires a signed audio-input entitlement as well as Info.plist', () => {
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  const info = readFileSync(new URL('../src-tauri/Info.plist', import.meta.url), 'utf8');
  const entitlements = readFileSync(new URL('../src-tauri/Entitlements.plist', import.meta.url), 'utf8');
  assert.match(info, /<key>NSMicrophoneUsageDescription<\/key>\s*<string>[^<]+<\/string>/);
  assert.match(entitlements, /<key>com\.apple\.security\.device\.audio-input<\/key>\s*<true\s*\/>/);
  const usage = { NSMicrophoneUsageDescription: 'Explicitly started voice input' };
  const signed = { 'com.apple.security.device.audio-input': true };
  assert.doesNotThrow(() => assertMicrophonePermissions(config, usage, signed));
  assert.throws(() => assertMicrophonePermissions(config, usage, {}), /audio-input/);
  assert.throws(() => assertMicrophonePermissions(config, usage, { ...signed, 'com.apple.security.device.audio-input': false }), /audio-input/);
  assert.throws(() => assertMicrophonePermissions(config, {}, signed), /usage description/);
  assert.throws(() => assertMicrophonePermissions({ bundle: { macOS: { infoPlist: 'Info.plist' } } }, usage, signed), /must sign/);
});

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
  writeFileSync(join(root, 'src-tauri/Cargo.toml'), '[package]\nname = "orbitpal"\nversion = "0.4.0"\n[dependencies]\n');
  writeFileSync(join(root, 'src-tauri/Cargo.lock'), '[[package]]\nname = "orbitpal"\nversion = "0.4.0"\n');
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

test('published v2 update formats keep the unused legacy ZIP decoder disabled', () => {
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  const cargo = readFileSync(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8');
  assert.equal(config.bundle.createUpdaterArtifacts, true, 'Only native Tauri v2 updater packages are supported');
  assert.ok(Object.values(updaterArtifacts).every(([, suffix]) => ['.app.tar.gz', '.AppImage', '-setup.exe', '.msi'].includes(suffix)), 'A legacy ZIP updater package would require the intentionally disabled compatibility decoder');
  const updater = cargo.match(/^tauri-plugin-updater\s*=\s*\{([^\n]+)\}/m)?.[1];
  assert.ok(updater, 'Keep updater feature selection explicit in Cargo.toml');
  assert.match(updater, /default-features\s*=\s*false/, 'The upstream defaults enable the unused ZIP v4 decoder');
  const features = updater.match(/features\s*=\s*\[([^\]]*)\]/)?.[1].match(/"([^"]+)"/g)?.map(value => JSON.parse(value));
  assert.deepEqual(features?.sort(), ['rustls-tls', 'system-proxy'], 'Keep TLS and system proxies without restoring legacy archive decoding');
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
    assert.deepEqual(collect(root, 'x86_64-pc-windows-msvc', output), ['OrbitPal-v0.4.0-windows-x64-setup.exe', 'OrbitPal-v0.4.0-windows-x64-setup.exe.sig', 'OrbitPal-v0.4.0-windows-x64.msi', 'OrbitPal-v0.4.0-windows-x64.msi.sig']);
    assert.equal(readFileSync(join(output, 'OrbitPal-v0.4.0-windows-x64.msi'), 'utf8'), 'msi fixture');
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
    assert.deepEqual(collect(root, 'x86_64-unknown-linux-gnu'), ['OrbitPal-v0.4.0-linux-x64.AppImage', 'OrbitPal-v0.4.0-linux-x64.AppImage.sig', 'OrbitPal-v0.4.0-linux-x64.deb']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function manifestFixture() {
  const root = fixture();
  const directory = join(root, 'downloads');
  mkdirSync(directory);
  for (const [platform, suffix] of Object.values(updaterArtifacts)) {
    const path = join(directory, `OrbitPal-v0.4.0-${platform}${suffix}`);
    writeFileSync(path, `${platform} fixture`);
    writeFileSync(`${path}.sig`, signature());
  }
  return { root, directory, repository: 'ralphschuler/orbitpal', tag: 'v0.4.0', date: '2026-10-19T10:00:00Z', notes: 'Fixture release notes.' };
}

test('manifest maps both mac architectures, Linux and both Windows installers to immutable release URLs', () => {
  const options = manifestFixture();
  try {
    const manifest = createUpdaterManifest(options);
    assert.equal(manifest.version, '0.4.0');
    assert.equal(manifest.pub_date, options.date);
    assert.deepEqual(Object.keys(manifest.platforms), Object.keys(updaterArtifacts));
    assert.equal(manifest.platforms['darwin-aarch64'].url, 'https://github.com/ralphschuler/streamer-ai-cohost/releases/download/v0.4.0/OrbitPal-v0.4.0-macos-arm64.app.tar.gz');
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
  const payload = join(options.directory, 'OrbitPal-v0.4.0-macos-x64.app.tar.gz');
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

test('renamed releases retain fixed legacy initial asset URLs for installed 0.7 clients', () => {
  const options = manifestFixture();
  try {
    assert.equal(options.repository, canonicalRepository);
    const manifest = createUpdaterManifest({ ...options, assetRepository: 'attacker/orbitpal' });
    for (const entry of Object.values(manifest.platforms)) {
      const url = new URL(entry.url);
      assert.equal(url.origin, 'https://github.com');
      assert.ok(url.pathname.startsWith(`/${legacyAssetRepository}/releases/download/v0.4.0/OrbitPal-v0.4.0-`));
      assert.equal(url.search + url.hash, '');
    }
    for (const repository of ['attacker/orbitpal', 'ralphschuler/other', legacyAssetRepository]) {
      assert.throws(() => createUpdaterManifest({ ...options, repository }), /official/);
    }
    const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
    assert.equal(config.identifier, 'studio.streamer-ai.cohost', 'Keep the installed app data/keychain identity');
    assert.equal(config.bundle.publisher ?? config.identifier.split('.')[1], 'streamer-ai', 'The locked Tauri Windows publisher default comes from the bundle identifier, not Cargo authors');
    assert.equal(config.productName, 'OrbitPal');
    assert.equal(config.bundle.windows.wix.upgradeCode, 'A9976BDA-0F16-5FD9-A9C4-BDD0372B8376', 'Preserve the former MSI upgrade identity');
    assert.deepEqual(config.bundle.linux.deb, {provides:['streamer-ai-cohost'],conflicts:['streamer-ai-cohost'],replaces:['streamer-ai-cohost']});
    assert.deepEqual(config.plugins.updater.endpoints, [`https://github.com/${canonicalRepository}/releases/latest/download/latest.json`]);
  } finally { rmSync(options.root, { recursive: true, force: true }); }
});

test('MSI rename removes only exact owned legacy shortcut names during a detected upgrade', () => {
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  assert.ok(config.bundle.windows.wix.fragmentPaths.includes('windows/legacy-msi-shortcuts.wxs'));
  assert.ok(config.bundle.windows.wix.componentGroupRefs.includes('OrbitPalLegacyMsiShortcuts'));
  const fragment = readFileSync(new URL('../src-tauri/windows/legacy-msi-shortcuts.wxs', import.meta.url), 'utf8');
  const removeNames = [...fragment.matchAll(/<RemoveFile\b[^>]*\bName="([^"]+)"/g)].map(match => match[1]);
  assert.deepEqual(removeNames, ['Streamer AI Cohost.lnk', 'Streamer AI Cohost.lnk', 'Uninstall Streamer AI Cohost.lnk']);
  assert.equal([...fragment.matchAll(/<Condition>/g)].length, 3);
  for (const condition of fragment.matchAll(/<Condition><!\[CDATA\[([^\]]+)\]\]><\/Condition>/g)) {
    assert.match(condition[1], /WIX_UPGRADE_DETECTED AND ORBITPAL_LEGACY_MSI_DIR AND ORBITPAL_LEGACY_MSI_/);
    assert.match(condition[1], /= "#1"$/);
  }
  for (const marker of ['InstallDir', 'Desktop Shortcut', 'Start Menu Shortcut', 'Uninstaller Shortcut']) { assert.ok(fragment.includes(`Name="${marker}"`)); }
  assert.ok(fragment.includes('Key="Software\\streamer-ai\\Streamer AI Cohost"'));
  assert.doesNotMatch(fragment, /<CustomAction\b|<Binary\b/);
});

test('the reviewed NSIS migration template stays tied to its locked upstream version', () => {
  const root = new URL('../', import.meta.url);
  const lock = readFileSync(new URL('pnpm-lock.yaml', root), 'utf8');
  const cliVersion = lock.match(/^  '@tauri-apps\/cli@([^']+)':/m)?.[1];
  assert.equal(cliVersion, '2.12.3', 'Review the vendored NSIS template and update its source/hash before changing the locked CLI');
  const config = JSON.parse(readFileSync(new URL('src-tauri/tauri.conf.json', root), 'utf8'));
  assert.equal(config.bundle.windows.nsis.template, 'windows/installer.nsi');
  assert.equal(config.bundle.windows.nsis.installerHooks, 'windows/branding-hooks.nsh');
  const template = readFileSync(new URL('src-tauri/windows/installer.nsi', root), 'utf8');
  const notice = readFileSync(new URL('public/licenses/TAURI-NSIS-TEMPLATE.txt', root), 'utf8');
  for (const source of [template, notice]) {
    assert.ok(source.includes('tauri-cli-v2.12.3/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi'));
    assert.ok(source.includes('8ae02c66f8adb4a0d15d97f984fbacd2b44262d8a4d03fefaf6177f67f7008e5'));
  }
  assert.match(notice, /Permission is hereby granted/);
  assert.match(template, /!define UNINSTKEY "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Streamer AI Cohost"/);
  assert.match(template, /!define MANUPRODUCTKEY "\$\{MANUKEY\}\\Streamer AI Cohost"/);
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
    const payload = join(options.directory, 'OrbitPal-v0.4.0-macos-x64.app.tar.gz');
    rmSync(payload);
    symlinkSync(join(options.directory, 'OrbitPal-v0.4.0-macos-arm64.app.tar.gz'), payload);
    assert.throws(() => createUpdaterManifest(options), /nonempty regular file/);
  } finally { rmSync(options.root, { recursive: true, force: true }); }
});

test('stable publication prevents downgrades, reruns, prerelease promotion and API failures', async () => {
  const repository = 'ralphschuler/orbitpal';
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
