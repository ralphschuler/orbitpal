import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { releaseVersion, verifyTag } from './release-info.mjs';
import { collect, platformForTarget } from './collect-release.mjs';
import { rustInventory, nodeInventory } from './dependency-inventories.mjs';

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'cohost-release-'));
  mkdirSync(join(root, 'src-tauri'));
  writeFileSync(join(root, 'package.json'), '{"version":"0.4.0"}');
  writeFileSync(join(root, 'src-tauri/tauri.conf.json'), '{"version":"0.4.0"}');
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
    writeFileSync(join(bundle, 'msi/installer.msi'), 'msi fixture');
    const output = join(root, 'downloads');
    assert.deepEqual(collect(root, 'x86_64-pc-windows-msvc', output), ['Streamer-AI-Cohost-v0.4.0-windows-x64-setup.exe', 'Streamer-AI-Cohost-v0.4.0-windows-x64.msi']);
    assert.equal(readFileSync(join(output, 'Streamer-AI-Cohost-v0.4.0-windows-x64.msi'), 'utf8'), 'msi fixture');
    writeFileSync(join(bundle, 'msi/duplicate.msi'), 'duplicate');
    assert.throws(() => collect(root, 'x86_64-pc-windows-msvc', output), /exactly one/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
