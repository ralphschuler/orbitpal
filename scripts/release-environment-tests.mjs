import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { validateSigningEnvironment, readSigningEnvironment, verifySigningEnvironment } from './release-environment.mjs';

const environment = () => ({ name: 'release-signing',
  deployment_branch_policy: { protected_branches: false, custom_branch_policies: true },
  protection_rules: [{ type: 'branch_policy' }] });
const policies = () => ({ total_count: 1, branch_policies: [{ id: 1, type: 'tag', name: 'v*' }] });
const sha = 'a'.repeat(40);
const context = tag => ({ repository: 'ralphschuler/orbitpal', tag, commit: sha, ref: `refs/tags/${tag}`, sha });
function publicApi() {
  const calls = [];
  const run = (command, args, options) => {
    calls.push({ command, args, options });
    return JSON.stringify(args.at(-1).includes('/deployment-branch-policies?') ? policies() : environment());
  };
  return { calls, run };
}

test('signing policy requires a real existing custom tag-only environment without prescribing reviewers', () => {
  assert.deepEqual(validateSigningEnvironment(environment(), policies()), { environment: 'release-signing', pattern: 'v*', type: 'tag' });
  // A maintainer may deliberately add stronger protection; this source does not
  // select a reviewer, invite a human, add a timer or weaken existing protections.
  const stronger = environment(); stronger.protection_rules.push({ type: 'required_reviewers', reviewers: [{ type: 'User', id: 1 }] });
  assert.doesNotThrow(() => validateSigningEnvironment(stronger, policies()));
});

test('missing, unrestricted, branch-typed, broad and malformed deployment policies fail closed', () => {
  const variants = [
    [null, policies()], [{ ...environment(), name: 'other' }, policies()],
    [{ ...environment(), deployment_branch_policy: null }, policies()],
    [{ ...environment(), deployment_branch_policy: { protected_branches: true, custom_branch_policies: false } }, policies()],
    [{ ...environment(), protection_rules: [] }, policies()],
    [environment(), { total_count: 0, branch_policies: [] }],
    [environment(), { total_count: 1, branch_policies: [{ name: 'v*', type: 'branch' }] }],
    [environment(), { total_count: 1, branch_policies: [{ name: '*', type: 'tag' }] }],
    [environment(), { total_count: 1, branch_policies: [{ name: 'v*' }] }],
    [environment(), { total_count: 2, branch_policies: [...policies().branch_policies, { name: 'main', type: 'branch' }] }],
    [environment(), { total_count: 1, branch_policies: [...policies().branch_policies, { name: '*', type: 'tag' }] }],
  ];
  for (const [value, rules] of variants) assert.throws(() => validateSigningEnvironment(value, rules), /release-signing/);
});

test('public preflight preserves stable and prerelease contexts and rejects another repo/ref/SHA before API access', () => {
  for (const tag of ['v0.12.1', 'v0.12.1-rc.1']) {
    const api = publicApi(); assert.equal(verifySigningEnvironment(context(tag), api.run).environment, 'release-signing');
    assert.equal(api.calls.length, 2);
  }
  for (const bad of [ { ...context('v0.12.1'), repository: 'other/orbitpal' },
    { ...context('v0.12.1'), ref: 'refs/heads/main' }, { ...context('v0.12.1'), sha: 'b'.repeat(40) },
    { ...context('v0.12.1'), tag: 'main' } ]) {
    const api = publicApi(); assert.throws(() => verifySigningEnvironment(bad, api.run)); assert.equal(api.calls.length, 0);
  }
});

test('public policy reads are fixed-host GETs only and API errors do not expose backend output', () => {
  const api = publicApi(); readSigningEnvironment(api.run);
  assert.equal(api.calls.length, 2);
  for (const { command, args, options } of api.calls) {
    assert.equal(command, 'gh'); assert.deepEqual(args.slice(0, 5), ['api', '--hostname', 'github.com', '--method', 'GET']);
    assert.match(args.at(-1), /^repos\/ralphschuler\/orbitpal\/environments\/release-signing(?:\/deployment-branch-policies\?per_page=100)?$/);
    assert.doesNotMatch(args.at(-1), /secrets|variables/); assert.equal(options.timeout, 15000); assert.equal(options.maxBuffer, 128 * 1024);
  }
  const backend = 'synthetic-private-backend-text';
  for (const run of [() => { throw new Error(backend); }, () => backend, () => JSON.stringify({ unexpected: backend })]) {
    let error; try { readSigningEnvironment(run); } catch (cause) { error = cause; }
    assert.ok(error); assert.ok(!String(error).includes(backend));
  }
});

test('workflow gates key access on the protected build while publisher keeps independent verification', () => {
  const workflow = readFileSync(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8');
  const validate = workflow.slice(workflow.indexOf('\n  validate:'), workflow.indexOf('\n  verify:'));
  const build = workflow.slice(workflow.indexOf('\n  build:'), workflow.indexOf('\n  publish:'));
  const publish = workflow.slice(workflow.indexOf('\n  publish:'));
  assert.doesNotMatch(validate, /secrets\.(?:TAURI_SIGNING|MACOS_CODESIGN)/);
  assert.match(validate, /run: node scripts\/release-environment\.mjs/);
  assert.match(build, /needs: \[validate, verify\]/); assert.match(build, /environment: release-signing/);
  const policy = build.indexOf('run: node scripts/release-environment.mjs');
  const key = build.indexOf('TAURI_SIGNING_PRIVATE_KEY:');
  assert.ok(policy > 0 && key > policy);
  assert.match(build, /Require the updater signing key/);
  assert.doesNotMatch(publish, /secrets\.(?:TAURI_SIGNING|MACOS_CODESIGN)|environment: release-signing|id-token: write/);
  const provenance = publish.indexOf('node scripts/release-provenance.mjs release-artifacts');
  const updater = publish.indexOf('node scripts/updater-manifest.mjs release-artifacts');
  const publication = publish.indexOf('gh release create');
  assert.ok(provenance > 0 && updater > provenance && publication > updater);
});
