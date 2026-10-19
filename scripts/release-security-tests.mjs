import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { releaseContext, verifyBuildProvenance } from './release-provenance.mjs';
import { rehearseUpdaterKeys } from './rehearse-updater-keys.mjs';

const commit = '1234567890abcdef1234567890abcdef12345678';
const context = { tag: 'v0.8.0', commit, ref: 'refs/tags/v0.8.0', sha: commit };
function fixture() {
  const directory = mkdtempSync(join(tmpdir(), 'orbitpal-provenance-'));
  writeFileSync(join(directory, 'OrbitPal-v0.8.0-windows-x64-setup.exe'), 'dummy binary');
  writeFileSync(join(directory, 'OrbitPal-v0.8.0-windows-x64-setup.exe.sig'), 'dummy signature');
  return { directory, tag: context.tag, commit };
}

test('release provenance binds manual dispatch and tag pushes to the same workflow ref and commit', () => {
  assert.deepEqual(releaseContext(context), { tag: context.tag, commit });
  for (const overrides of [{ ref: 'refs/heads/main' }, { sha: 'a'.repeat(40) }, { commit: 'main' }, { tag: '../tag' }]) {
    assert.throws(() => releaseContext({ ...context, ...overrides }), /provenance|Provenance|workflow ref/);
  }
});

test('every downloaded platform byte stream is verified using the immutable GitHub provenance policy', () => {
  const options = fixture();
  try {
    const calls = [];
    const run = (...args) => { calls.push(args); return '{}'; };
    assert.equal(verifyBuildProvenance({ ...options, run }), 2);
    for (const [command, args, execution] of calls) {
      assert.equal(command, 'gh');
      assert.deepEqual(args.slice(0, 2), ['attestation', 'verify']);
      assert.ok(args[2].startsWith(options.directory));
      assert.deepEqual(args.slice(3), [
        '--repo', 'ralphschuler/orbitpal', '--signer-workflow', 'ralphschuler/orbitpal/.github/workflows/release.yml',
        '--source-ref', 'refs/tags/v0.8.0', '--source-digest', commit, '--signer-digest', commit,
        '--deny-self-hosted-runners', '--predicate-type', 'https://slsa.dev/provenance/v1', '--format', 'json',
      ]);
      assert.equal(execution.timeout, 90_000);
      assert.equal(execution.maxBuffer, 1024 * 1024);
      assert.equal(execution.shell, undefined);
    }
  } finally { rmSync(options.directory, { recursive: true, force: true }); }
});

test('missing, rejected or timed-out GitHub provenance prevents publication', () => {
  const options = fixture();
  try {
    for (const failure of ['digest mismatch', 'wrong signer/ref/commit', 'missing attestation', 'network timeout', 'gh missing']) {
      let calls = 0;
      assert.throws(() => verifyBuildProvenance({ ...options, run() { calls++; throw new Error(failure); } }), /did not verify/);
      assert.equal(calls, 1);
    }
    assert.throws(() => verifyBuildProvenance({ ...options, commit: 'main' }), /full commit/);
  } finally { rmSync(options.directory, { recursive: true, force: true }); }
});

test('provenance gate rejects empty directories, zero-byte files and metadata mixed into the build handoff', () => {
  const options = fixture();
  const run = () => { throw new Error('Must reject before invoking gh'); };
  try {
    for (const name of ['latest.json', 'OrbitPal-v0.7.0-linux-x64.AppImage', 'OrbitPal-v0.8.0-source.zip']) {
      writeFileSync(join(options.directory, name), 'unexpected');
      assert.throws(() => verifyBuildProvenance({ ...options, run }), /Unexpected platform/);
      rmSync(join(options.directory, name));
    }
    writeFileSync(join(options.directory, 'OrbitPal-v0.8.0-linux-x64.deb'), '');
    assert.throws(() => verifyBuildProvenance({ ...options, run }), /Unexpected platform/);
    rmSync(options.directory, { recursive: true });
    mkdirSync(options.directory);
    assert.throws(() => verifyBuildProvenance({ ...options, run }), /1–64/);
  } finally { rmSync(options.directory, { recursive: true, force: true }); }
});

test('provenance gate rejects symlinks before invoking GitHub', { skip: process.platform === 'win32' }, () => {
  const options = fixture();
  try {
    symlinkSync(join(options.directory, 'OrbitPal-v0.8.0-windows-x64-setup.exe'), join(options.directory, 'OrbitPal-v0.8.0-linux-x64.deb'));
    assert.throws(() => verifyBuildProvenance({ ...options, run() { throw new Error('Should not run'); } }), /Unexpected platform/);
  } finally { rmSync(options.directory, { recursive: true, force: true }); }
});

test('throwaway key rehearsal restores an encrypted backup and exercises the single-key trust transition', () => {
  assert.deepEqual(rehearseUpdaterKeys(), {
    throwawayOnly: true, encryptedBackupRestored: true, wrongPasswordRejected: true,
    recoveredKeySignsBridge: true, oldTrustRejectsNewKey: true, bridgeEnablesNewTrust: true,
    tamperRejected: true, installerTransitionTested: false,
  });
});

test('rehearsal exports only public fixtures for an independent minisign verifier', () => {
  const root = mkdtempSync(join(tmpdir(), 'orbitpal-key-rehearsal-'));
  const output = join(root, 'public');
  try {
    rehearseUpdaterKeys({ publicFixtures: output });
    assert.deepEqual(readdirSync(output).sort(), ['a.pub', 'b.pub', 'bridge.fixture', 'bridge.fixture.sig', 'next.fixture', 'next.fixture.sig']);
    for (const name of ['a.pub', 'b.pub']) assert.match(Buffer.from(readFileSync(join(output, name), 'utf8'), 'base64').toString(), /throwaway rehearsal key/);
    assert.equal(JSON.parse(readFileSync(join(output, 'bridge.fixture'), 'utf8')).fixtureOnly, true);
    assert.throws(() => rehearseUpdaterKeys({ publicFixtures: output }), /EEXIST/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('rehearsal ignores signing environment values and never prints them', () => {
  const marker = 'FAKE-TEST-SECRET-DO-NOT-PRINT';
  const output = execFileSync(process.execPath, [fileURLToPath(new URL('rehearse-updater-keys.mjs', import.meta.url))], {
    encoding: 'utf8', env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: marker, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: marker },
  });
  assert.equal(JSON.parse(output).throwawayOnly, true);
  assert.equal(output.includes(marker), false);
});
