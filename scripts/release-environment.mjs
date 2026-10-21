import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseContext } from './release-provenance.mjs';

export const signingRepository = 'ralphschuler/orbitpal';
export const signingEnvironment = 'release-signing';
const endpoint = `repos/${signingRepository}/environments/${signingEnvironment}`;
const invalid = () => new Error('The release-signing environment must exist with only the custom v* tag deployment rule; inspect its public policy before signing.');

/** Public configuration only. A tag glob is not SemVer validation; the existing
 * tagged workflow/ref/commit checks remain mandatory and keep prereleases. */
export function validateSigningEnvironment(environment, policies) {
  if (!environment || environment.name !== signingEnvironment
    || environment.deployment_branch_policy?.protected_branches !== false
    || environment.deployment_branch_policy?.custom_branch_policies !== true
    || !Array.isArray(environment.protection_rules)
    || !environment.protection_rules.some(rule => rule?.type === 'branch_policy')
    || !policies || policies.total_count !== 1 || !Array.isArray(policies.branch_policies)
    || policies.branch_policies.length !== 1
    || policies.branch_policies[0]?.type !== 'tag' || policies.branch_policies[0]?.name !== 'v*') throw invalid();
  return { environment: signingEnvironment, pattern: 'v*', type: 'tag' };
}

/** Fixed official API, GET only, bounded output/time, no secret endpoints or
 * PAT. Workflow supplies only its ephemeral read-only GITHUB_TOKEN to gh. */
export function readSigningEnvironment(run = execFileSync) {
  const read = path => {
    try {
      const value = run('gh', ['api', '--hostname', 'github.com', '--method', 'GET',
        '-H', 'Accept: application/vnd.github+json', '-H', 'X-GitHub-Api-Version: 2026-03-10', path],
      { encoding: 'utf8', maxBuffer: 128 * 1024, timeout: 15_000, stdio: ['ignore', 'pipe', 'pipe'] });
      return JSON.parse(value);
    } catch { throw invalid(); } // Never attach CLI output, token or backend text.
  };
  return validateSigningEnvironment(read(endpoint), read(`${endpoint}/deployment-branch-policies?per_page=100`));
}

export function verifySigningEnvironment(context, run = execFileSync) {
  if (context.repository !== signingRepository) throw invalid();
  releaseContext(context); // Existing release/ref/SHA contract, including prereleases.
  return readSigningEnvironment(run);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  verifySigningEnvironment({ repository: process.env.GITHUB_REPOSITORY,
    tag: process.env.RELEASE_TAG, commit: process.env.RELEASE_COMMIT,
    ref: process.env.GITHUB_REF, sha: process.env.GITHUB_SHA });
  console.log('Verified existing release-signing environment and tag-only deployment policy');
}
