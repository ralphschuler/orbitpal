import { execFileSync } from 'node:child_process';
import { appendFileSync, lstatSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseVersion, verifyTag } from './release-info.mjs';

const repository = 'ralphschuler/orbitpal';
const workflow = `${repository}/.github/workflows/release.yml`;

export function releaseContext({ tag, commit, ref, sha }) {
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag || '')) throw new Error('Invalid provenance release tag');
  if (!/^[a-f0-9]{40}$/.test(commit || '')) throw new Error('Provenance requires the full tagged commit');
  // GitHub's OIDC provenance identifies the workflow ref, not an arbitrary checkout.
  // A manual dispatch must therefore select the tag as its workflow ref too.
  if (ref !== `refs/tags/${tag}` || sha !== commit) {
    throw new Error('Release workflow ref/SHA must match the selected tag/commit; dispatch with --ref vVERSION -f tag=vVERSION');
  }
  return { tag, commit };
}

export function verifyBuildProvenance({ directory, tag, commit, run = execFileSync }) {
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag || '') || !/^[a-f0-9]{40}$/.test(commit || '')) {
    throw new Error('Provenance verification requires a release tag and full commit');
  }
  const entries = readdirSync(directory, { withFileTypes: true });
  if (entries.length === 0 || entries.length > 64) throw new Error('Expected 1–64 platform release artifacts before metadata generation');
  const prefix = `OrbitPal-${tag}-`;
  const files = entries.map(entry => {
    const path = join(directory, entry.name);
    const stat = lstatSync(path);
    if (!entry.isFile() || !stat.isFile() || stat.size === 0 || !entry.name.startsWith(prefix)
      || !/^(macos-arm64|macos-x64|windows-x64|linux-x64)[A-Za-z0-9.-]+$/.test(entry.name.slice(prefix.length))) {
      throw new Error(`Unexpected platform artifact before provenance verification: ${entry.name}`);
    }
    return path;
  }).sort();
  // Verify the actual downloaded bytes, not names, checksums or a workflow's assertion
  // about itself. gh checks the Sigstore certificate, transparency proof and subject digest.
  for (const file of files) {
    try {
      run('gh', ['attestation', 'verify', file,
        '--repo', repository, '--signer-workflow', workflow,
        '--source-ref', `refs/tags/${tag}`, '--source-digest', commit,
        '--signer-digest', commit, '--deny-self-hosted-runners',
        '--predicate-type', 'https://slsa.dev/provenance/v1', '--format', 'json'],
      { encoding: 'utf8', maxBuffer: 1024 * 1024, timeout: 90_000, stdio: ['ignore', 'pipe', 'pipe'] });
    } catch {
      throw new Error(`Build provenance did not verify: ${file}`);
    }
  }
  return files.length;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const tag = process.env.RELEASE_TAG;
  verifyTag(tag, releaseVersion());
  if (process.argv[2] === '--context') {
    const commit = execFileSync('git', ['rev-parse', '--verify', `refs/tags/${tag}^{commit}`], { encoding: 'utf8' }).trim();
    releaseContext({ tag, commit, ref: process.env.GITHUB_REF, sha: process.env.GITHUB_SHA });
    if (!process.env.GITHUB_OUTPUT) throw new Error('Missing GitHub output file');
    appendFileSync(process.env.GITHUB_OUTPUT, `commit=${commit}\n`);
    console.log(`Release provenance context: ${tag} at ${commit}`);
  } else {
    const count = verifyBuildProvenance({ directory: resolve(process.argv[2] || 'release-artifacts'), tag, commit: process.env.RELEASE_COMMIT });
    console.log(`Verified build provenance for ${count} platform artifacts`);
  }
}
