import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { randomBytes, X509Certificate } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadSigningPolicy, quoteSecurityInput, designatedRequirement } from './macos-signing-policy.mjs';

export function requireDisposableMac(env = process.env, platform = process.platform) {
  assert.equal(platform, 'darwin', 'This operation requires a disposable macOS runner');
  assert.equal(env.GITHUB_ACTIONS, 'true', 'Never provision a release keychain on a user Mac');
  assert.equal(env.RUNNER_ENVIRONMENT, 'github-hosted', 'Release keys require an isolated GitHub-hosted runner');
  assert.ok(env.RUNNER_TEMP && /^\d+$/.test(env.GITHUB_RUN_ID ?? '') && /^\d+$/.test(env.GITHUB_RUN_ATTEMPT ?? ''), 'Missing trusted runner identity');
}

export function runNative(program, args, options = {}) {
  const { operation, ...processOptions } = options;
  const operations = ['list-keychains', 'create-keychain', 'unlock-keychain', 'import', 'set-keychain-settings', 'set-key-partition-list', 'find-certificate', 'add-trusted-cert', 'find-identity', 'remove-trusted-cert', 'delete-keychain'];
  const candidate = operation || args[0];
  const step = program === '/usr/bin/security' && operations.includes(candidate) ? `security/${candidate}` : program === '/usr/bin/clang' ? 'fixture/compile' : 'native/signature-check';
  console.log(`macOS signing operation: ${step}`);
  const result = spawnSync(program, args, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], timeout: 90_000, maxBuffer: 4 * 1024 * 1024, ...processOptions });
  // Do not include native output, environment or argv in failures. `security`
  // diagnostics may contain a command that was supplied on its stdin.
  if (result.error || result.status !== 0) {
    const outcome = result.error?.code === 'ETIMEDOUT' ? 'timed out' : Number.isInteger(result.status) ? `exit ${result.status}` : 'could not start';
    const error = new Error(`macOS signing operation failed: ${step} (${outcome}); native diagnostics are withheld`);
    error.operation = step; // Static allowlisted metadata only, never argv/output.
    throw error;
  }
  return result.stdout;
}

function privateFile(path, contents) { writeFileSync(path, contents, { flag: 'wx', mode: 0o600 }); }
function securityInput(args) { return runNative('/usr/bin/security', ['-i'], { operation: args[0], input: `${args.map(quoteSecurityInput).join(' ')}\n` }); }
function stateFile(directory) { return join(directory, 'state.json'); }
function saveState(directory, state) { writeFileSync(stateFile(directory), JSON.stringify(state), { mode: 0o600 }); }

export function validateSigningPassword(password) {
  // Plain errors deliberately omit actual/expected/cause fields: assertion
  // diffs can expose secret values that CI's exact-string masking misses.
  if (typeof password !== 'string' || !/^[A-Za-z0-9_-]{32,256}$/.test(password)) {
    throw new Error('The signing archive needs a generated, single-line strong password');
  }
}

export function decodeSigningArchive(encoded) {
  if (typeof encoded !== 'string' || !encoded || encoded.length > 1_400_000 || !/^[A-Za-z0-9+/]+={0,2}$/.test(encoded)) {
    throw new Error('Missing or invalid MACOS_CODESIGN_P12 secret');
  }
  const archive = Buffer.from(encoded, 'base64');
  if (archive.toString('base64') !== encoded || archive.length <= 100 || archive.length >= 1024 * 1024) {
    archive.fill(0);
    throw new Error('Invalid signing archive encoding or size');
  }
  return archive;
}

export function searchListRestoreCommand(state) {
  // A failed initial read must never be mistaken for an originally empty
  // keychain search list. No keychain has been created before this snapshot.
  if (state.snapshotTaken !== true) return null;
  assert.ok(Array.isArray(state.oldKeychains) && state.oldKeychains.every(value => typeof value === 'string' && !/[\r\n\0]/.test(value)), 'Invalid runner keychain snapshot');
  return ['list-keychains', '-d', 'user', '-s', ...state.oldKeychains];
}

export function runnerDirectory(target, suffix = '') {
  requireDisposableMac();
  assert.ok(['aarch64-apple-darwin', 'x86_64-apple-darwin'].includes(target), 'Unsupported macOS target');
  assert.ok(['', '-fixture-a', '-fixture-b'].includes(suffix));
  return join(realpathSync(process.env.RUNNER_TEMP), `orbitpal-signing-${process.env.GITHUB_RUN_ID}-${process.env.GITHUB_RUN_ATTEMPT}-${target}${suffix}`);
}

export function prepareIdentity(directory, identity, p12, password) {
  requireDisposableMac();
  validateSigningPassword(password);
  assert.ok(Buffer.isBuffer(p12) && p12.length > 100 && p12.length < 1024 * 1024, 'Invalid signing archive size');
  mkdirSync(directory, { mode: 0o700 });
  const state = { keychain: join(directory, 'identity.keychain-db'), certificate: join(directory, 'certificate.pem'), oldKeychains: [], snapshotTaken: false, trustAdded: false };
  saveState(directory, state);
  try {
    privateFile(state.certificate, identity.certificate.toString());
    const archive = join(directory, 'identity.p12');
    privateFile(archive, p12);
    const keychainPassword = randomBytes(32).toString('hex');
    state.oldKeychains = runNative('/usr/bin/security', ['list-keychains', '-d', 'user']).split(/\r?\n/).map(line => line.trim().replace(/^"|"$/g, '')).filter(Boolean);
    state.snapshotTaken = true;
    saveState(directory, state);
    // `security -p/-P/-k` normally leaks passwords through process argv. Its
    // documented interactive mode reads these commands from a pipe instead.
    securityInput(['create-keychain', '-p', keychainPassword, state.keychain]);
    chmodSync(state.keychain, 0o600);
    securityInput(['unlock-keychain', '-p', keychainPassword, state.keychain]);
    securityInput(['import', archive, '-k', state.keychain, '-f', 'pkcs12', '-t', 'agg', '-P', password, '-x', '-T', '/usr/bin/codesign']);
    rmSync(archive);
    runNative('/usr/bin/security', ['set-keychain-settings', '-lut', '3600', state.keychain]);
    securityInput(['set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:', '-s', '-k', keychainPassword, state.keychain]);
    const certificates = runNative('/usr/bin/security', ['find-certificate', '-a', '-p', state.keychain]).match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) ?? [];
    assert.equal(certificates.length, 1, 'The signing keychain must contain only the reviewed certificate');
    const imported = new X509Certificate(certificates[0]);
    assert.equal(imported.fingerprint256.replaceAll(':', '').toLowerCase(), identity.sha256, 'The imported archive does not contain the pinned certificate');
    // Limited to the code-signing policy in the disposable runner's user
    // domain. This does not install a trusted root on any app user's Mac.
    state.trustAdded = true;
    saveState(directory, state);
    runNative('/usr/bin/security', ['add-trusted-cert', '-r', 'trustRoot', '-p', 'codeSign', '-k', state.keychain, state.certificate]);
    const identities = runNative('/usr/bin/security', ['find-identity', '-v', '-p', 'codesigning', state.keychain]);
    assert.ok(identities.toLowerCase().includes(identity.sha1), 'The pinned certificate has no usable private signing key');
    return state;
  } catch (error) {
    try { cleanupIdentity(directory); } catch { throw new Error(`macOS identity setup and cleanup failed after ${error.operation || 'validation'}; discard the disposable runner`); }
    throw error;
  }
}

export function cleanupIdentity(directory) {
  requireDisposableMac();
  if (!existsSync(stateFile(directory))) { if (existsSync(directory)) rmSync(directory, { recursive: true, force: true }); return; }
  const state = JSON.parse(readFileSync(stateFile(directory), 'utf8'));
  assert.equal(state.keychain, join(directory, 'identity.keychain-db'));
  assert.equal(state.certificate, join(directory, 'certificate.pem'));
  const errors = [];
  if (state.trustAdded) {
    try { runNative('/usr/bin/security', ['remove-trusted-cert', state.certificate]); } catch { errors.push('temporary code-signing trust'); }
  }
  if (existsSync(state.keychain)) {
    try { runNative('/usr/bin/security', ['delete-keychain', state.keychain]); } catch { errors.push('temporary signing keychain'); }
  }
  try { const restore = searchListRestoreCommand(state); if (restore) runNative('/usr/bin/security', restore); } catch { errors.push('runner keychain search list'); }
  rmSync(directory, { recursive: true, force: true });
  assert.equal(errors.length, 0, `Failed to clean ${errors.join(', ')}; discard this disposable runner`);
}

export function signingContext(root, target, directory, identity, releaseDirectory = resolve(root, 'src-tauri/target', target, 'release')) {
  const realRelease = realpathSync(releaseDirectory);
  const context = { identifier: identity.identifier, sha1: identity.sha1, keychain: join(directory, 'identity.keychain-db'), requirements: join(directory, 'designated.req'), releaseDirectory: realRelease, app: join(realRelease, 'bundle/macos/OrbitPal.app') };
  privateFile(context.requirements, `${designatedRequirement(identity)}\n`);
  const contextPath = join(directory, 'context.json');
  privateFile(contextPath, JSON.stringify(context));
  const bin = join(directory, 'bin');
  mkdirSync(bin, { mode: 0o700 });
  const wrapper = join(bin, 'codesign');
  privateFile(wrapper, '#!/bin/sh\nexec "$ORBITPAL_SIGNING_NODE" "$ORBITPAL_SIGNING_WRAPPER" "$@"\n');
  chmodSync(wrapper, 0o700);
  return { ORBITPAL_MACOS_SIGNING_CONTEXT: contextPath, ORBITPAL_SIGNING_NODE: process.execPath, ORBITPAL_SIGNING_WRAPPER: resolve(root, 'scripts/macos-codesign.mjs'), APPLE_SIGNING_IDENTITY: identity.sha1, PATH: `${bin}${delimiter}${process.env.PATH}` };
}

export async function build(root, target, bundles) {
  requireDisposableMac();
  const directory = runnerDirectory(target);
  const identity = loadSigningPolicy(root);
  assert.equal(bundles, 'app,dmg', 'Unexpected macOS bundle selection');
  const encoded = process.env.MACOS_CODESIGN_P12?.trim() ?? '';
  const password = process.env.MACOS_CODESIGN_PASSWORD ?? '';
  validateSigningPassword(password);
  const archive = decodeSigningArchive(encoded);
  // Do not give private certificate material to Tauri's Apple-only importer or
  // to arbitrary build subprocesses. Only this parent process imports it.
  const environment = { ...process.env };
  for (const name of Object.keys(environment)) {
    if (name.startsWith('APPLE_') || name.startsWith('MACOS_CODESIGN_')) delete environment[name];
  }
  try {
    prepareIdentity(directory, identity, archive, password);
    const releaseDirectory = resolve(root, 'src-tauri/target', target, 'release');
    mkdirSync(releaseDirectory, { recursive: true });
    const signing = signingContext(root, target, directory, identity);
    const config = join(directory, 'tauri-signing.json');
    privateFile(config, JSON.stringify({ bundle: { macOS: { signingIdentity: identity.sha1, hardenedRuntime: true } } }));
    await new Promise((accept, reject) => {
      const child = spawn('pnpm', ['tauri', 'build', '--target', target, '--bundles', bundles, '--config', config], { cwd: root, env: { ...environment, ...signing }, stdio: 'inherit' });
      const terminate = () => child.kill('SIGTERM');
      process.once('SIGTERM', terminate);
      child.once('error', () => reject(new Error('The macOS build process could not start')));
      child.once('close', code => { process.removeListener('SIGTERM', terminate); code === 0 ? accept() : reject(new Error('The persistently signed macOS build failed')); });
    });
  } finally {
    archive.fill(0);
    cleanupIdentity(directory);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const action = process.argv[2];
  const target = process.env.RUST_TARGET;
  if (action === 'build') await build(root, target, process.env.BUNDLES);
  else if (action === 'cleanup') cleanupIdentity(runnerDirectory(target));
  else throw new Error('Use macos-release-signing.mjs build or cleanup on a disposable CI runner');
}
