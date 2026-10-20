#!/usr/bin/env node
// Explicit opt-in only. Never run on a developer machine or self-hosted runner.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

if (process.env.GITHUB_ACTIONS !== 'true' || process.env.RUNNER_ENVIRONMENT !== 'github-hosted' || !process.env.RUNNER_TEMP) {
  throw new Error('Native credential compatibility requires a disposable GitHub-hosted runner');
}
for (const name of Object.keys(process.env)) {
  if (/^(?:MACOS_CODESIGN_|TAURI_SIGNING_PRIVATE_KEY|APPLE_CERTIFICATE)/.test(name)) {
    throw new Error('Credential compatibility fixtures must not receive production signing inputs');
  }
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const runnerTemporary = realpathSync(process.env.RUNNER_TEMP);
const cleanupState = path.join(runnerTemporary, 'orbitpal-keyring-compat-cleanup.json');
function restoreMacKeychains() {
  if (!existsSync(cleanupState)) return;
  if (process.platform !== 'darwin') throw new Error('Unexpected macOS cleanup state');
  const state = JSON.parse(readFileSync(cleanupState, 'utf8'));
  if (state.version !== 1 || !path.isAbsolute(state.previousKeychain)
      || !Array.isArray(state.previousSearchList) || !state.previousSearchList.every(value => typeof value === 'string' && path.isAbsolute(value))
      || path.dirname(path.dirname(state.fixtureKeychain)) !== runnerTemporary
      || !path.basename(path.dirname(state.fixtureKeychain)).startsWith('orbitpal-keyring-compat-')
      || path.basename(state.fixtureKeychain) !== 'compat.keychain-db') throw new Error('Invalid fixture cleanup state');
  const failures = [];
  for (const args of [
    ['default-keychain', '-d', 'user', '-s', state.previousKeychain],
    ['list-keychains', '-d', 'user', '-s', ...state.previousSearchList],
    ...(existsSync(state.fixtureKeychain) ? [['delete-keychain', state.fixtureKeychain]] : []),
  ]) {
    try { execFileSync('security', args, { stdio: 'pipe', timeout: 30_000 }); }
    catch { failures.push('Failed to restore disposable runner keychain settings'); }
  }
  if (failures.length) throw new Error(failures.join('; '));
  rmSync(cleanupState);
}
if (process.argv.includes('--cleanup')) {
  restoreMacKeychains();
  process.exit(0);
}
if (existsSync(cleanupState)) throw new Error('A previous fixture needs cleanup before another run');
const temporary = mkdtempSync(path.join(runnerTemporary, 'orbitpal-keyring-compat-'));
const target = path.join(root, 'src-tauri', 'target', 'keyring-compat');
const env = { ...process.env, CARGO_TARGET_DIR: target, ORBITPAL_KEYRING_COMPAT_DISPOSABLE: '1' };
const binary = path.join(target, 'debug', `orbitpal-keyring-compat-fixture${process.platform === 'win32' ? '.exe' : ''}`);
const id = `orbitpal-keyring-compat-${randomUUID()}`;
function run(command, args, options = {}) {
  return execFileSync(command, args, { cwd: root, env, stdio: 'pipe', timeout: 120_000, ...options });
}
// Both directions and both deletion owners are checked across fresh processes.
// Unicode includes supplementary code points to verify Windows UTF-16 blobs.
const phases = [
  ['old-missing', ''], ['new-missing', ''],
  ['old-write', 'synthetic-old-ä漢字🪐'], ['new-read', 'synthetic-old-ä漢字🪐'],
  ['new-write', 'synthetic-new-é日本語🐸'], ['old-read', 'synthetic-new-é日本語🐸'],
  ['old-delete', ''], ['new-missing', ''],
  ['new-write', 'synthetic-new-first'], ['old-read', 'synthetic-new-first'],
  ['old-write', 'synthetic-old-update'], ['new-read', 'synthetic-old-update'],
  ['new-delete', ''], ['old-missing', ''], ['new-missing', ''],
];
try {
  execFileSync('cargo', ['build', '--manifest-path', 'scripts/fixtures/keyring-compat/Cargo.toml', '--locked'], {
    cwd: root, env, stdio: 'inherit', timeout: 600_000,
  });
  if (process.platform === 'darwin') {
    const previousKeychain = run('security', ['default-keychain', '-d', 'user']).toString().trim().replace(/^"|"$/g, '');
    if (!path.isAbsolute(previousKeychain)) throw new Error('Cannot record original CI default keychain');
    const previousSearchList = run('security', ['list-keychains', '-d', 'user']).toString().split('\n').map(line => line.trim()).filter(Boolean).map(line => JSON.parse(line));
    if (!previousSearchList.every(value => typeof value === 'string' && path.isAbsolute(value))) throw new Error('Cannot record original CI keychain search list');
    const fixtureKeychain = path.join(temporary, 'compat.keychain-db');
    // Persist before the first mutation so the workflow's always() cleanup can
    // recover the runner settings if this process is interrupted.
    writeFileSync(cleanupState, JSON.stringify({ version: 1, previousKeychain, previousSearchList, fixtureKeychain }));
    const password = randomUUID();
    run('security', ['create-keychain', '-p', password, fixtureKeychain]);
    run('security', ['set-keychain-settings', '-lut', '3600', fixtureKeychain]);
    run('security', ['unlock-keychain', '-p', password, fixtureKeychain]);
    run('security', ['default-keychain', '-d', 'user', '-s', fixtureKeychain]);
    for (const [operation, value] of phases) run(binary, [operation, id, value]);
  } else if (process.platform === 'linux') {
    // Private HOME, XDG state and a new DBus session ensure neither lookup nor
    // daemon activation can reach an existing desktop's Secret Service.
    const isolatedHome = path.join(temporary, 'home');
    const runtime = path.join(temporary, 'runtime');
    mkdirSync(isolatedHome, { mode: 0o700 });
    mkdirSync(runtime, { mode: 0o700 });
    const linuxEnv = { ...env, HOME: isolatedHome, XDG_RUNTIME_DIR: runtime,
      XDG_DATA_HOME: path.join(isolatedHome, '.local', 'share'), XDG_CONFIG_HOME: path.join(isolatedHome, '.config') };
    delete linuxEnv.DBUS_SESSION_BUS_ADDRESS;
    delete linuxEnv.GNOME_KEYRING_CONTROL;
    const script = path.join(temporary, 'fixture.sh');
    writeFileSync(script, `set -euo pipefail
printf '%s' 'public-synthetic-fixture-password' | gnome-keyring-daemon --unlock --components=secrets
for attempt in {1..30}; do
  if gdbus introspect --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets >/dev/null 2>&1; then break; fi
  if [ "$attempt" = 30 ]; then exit 1; fi
  sleep 0.1
done
node "$1" "$2" "$3"
`);
    const phaseFile = path.join(temporary, 'phases.mjs');
    writeFileSync(phaseFile, `import { execFileSync } from 'node:child_process';
const [binary, id] = process.argv.slice(2);
for (const [operation, value] of ${JSON.stringify(phases)}) execFileSync(binary, [operation, id, value], { stdio: 'pipe', timeout: 30000 });
`);
    run('dbus-run-session', ['--', 'bash', script, phaseFile, binary, id], { env: linuxEnv });
  } else if (process.platform === 'win32') {
    // Windows has one Credential Manager for this disposable runner account.
    // Only the exact random UUID target is read/written/deleted; no enumeration.
    try {
      for (const [operation, value] of phases) run(binary, [operation, id, value]);
    } finally {
      run(binary, ['new-delete', id, '']);
    }
  } else {
    throw new Error('Unsupported compatibility platform');
  }
  console.log('Native credential compatibility: 15 old/new operations passed across fresh processes');
} finally {
  // Keep recovery files when cleanup fails so the workflow always() step can
  // retry. Neither this directory nor the state contains production data.
  restoreMacKeychains();
  rmSync(temporary, { recursive: true, force: true });
}
