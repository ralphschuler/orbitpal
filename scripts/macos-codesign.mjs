// Build-only entry point installed as `codesign` in a private temporary PATH.
// Tauri invokes this while bundling, before updater payloads are archived/signed.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { signingCommand } from './macos-signing-policy.mjs';

assert.equal(process.platform, 'darwin');
assert.equal(process.env.GITHUB_ACTIONS, 'true', 'Release signing runs only on a disposable GitHub runner');
assert.equal(process.env.RUNNER_ENVIRONMENT, 'github-hosted');
const path = process.env.ORBITPAL_MACOS_SIGNING_CONTEXT;
assert.ok(path, 'Missing protected signing context');
assert.equal(statSync(path).mode & 0o077, 0, 'The signing context must be private');
const context = JSON.parse(readFileSync(path, 'utf8'));
const args = process.argv.slice(2);
const target = args.at(-1);
assert.ok(target, 'Missing codesign target');
const command = signingCommand(args, context, realpathSync(target));
const result = spawnSync('/usr/bin/codesign', command, { stdio: 'inherit', timeout: 90_000 });
if (result.error) throw new Error('Native macOS signing did not complete');
process.exit(result.status ?? 1);
