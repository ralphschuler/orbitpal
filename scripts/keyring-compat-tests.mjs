import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const launcher = fileURLToPath(new URL('./test-keyring-compat-ci.mjs', import.meta.url));
for (const [name, overrides] of [
  ['local invocation', { GITHUB_ACTIONS: 'false', RUNNER_ENVIRONMENT: 'github-hosted' }],
  ['self-hosted runner', { GITHUB_ACTIONS: 'true', RUNNER_ENVIRONMENT: 'self-hosted' }],
  ['missing disposable directory', { GITHUB_ACTIONS: 'true', RUNNER_ENVIRONMENT: 'github-hosted', RUNNER_TEMP: '' }],
]) {
  test(`credential fixture rejects ${name} before native access, including cleanup`, () => {
    for (const args of [[], ['--cleanup']]) {
      const result = spawnSync(process.execPath, [launcher, ...args], {
        encoding: 'utf8', env: { ...process.env, ...overrides }, timeout: 10_000,
      });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Native credential compatibility requires a disposable GitHub-hosted runner/);
      assert.equal(result.stdout, '');
    }
  });
}
