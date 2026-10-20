# Release trust and signing activation

Status on 2026-10-20: **v0.10.0** shipped with the pinned persistent self-signed macOS identity, four platform build attestations and successful provenance/updater-signature verification before publication. Published macOS releases through **v0.9.0** remain ad-hoc signed. Protected signing environments, independent backup custody and real installation/permission acceptance remain pending in issue #17. The maintainer's current choice is free persistent self-signing; optional Developer ID/notarization and Windows Authenticode are separate future migrations.

## Signing and provenance assurances

| Mechanism | What it establishes | Current boundary |
| --- | --- | --- |
| Tauri/minisign updater signature | The installed app's trusted key authorized the payload and signed version | Required independently of OS signing |
| Authenticode | A Windows publisher identity signed executable/installer bytes | Optional future migration; no production identity configured |
| Persistent self-signed macOS certificate | The reviewed certificate key and stable certificate-bound designated requirement identify the app | Published for both Mac architectures in v0.10.0; real update/privacy acceptance remains pending |
| Developer ID and notarization | Apple identity and Apple's distribution assessment | Optional future migration; the current self-signed identity provides neither |
| GitHub/Sigstore provenance | A specific GitHub workflow/ref/commit produced matching file digests | All four platform attestations and pre-publication verification succeeded for v0.10.0 |

Provenance is not a malware scan, a reproducible-build claim or an assertion of a particular SLSA level. The release manifest, source ZIP and checksum list are generated later by the publication job and are not presented as platform build subjects.

## Published production evidence

[v0.10.0](https://github.com/ralphschuler/orbitpal/releases/tag/v0.10.0) was published on **2026-10-19 at 21:15:15 UTC**, from tag `v0.10.0` at commit `aa3f2e8923d50d1ed3e6a077515de67a39dedc21`. [Release run 39322457335](https://github.com/ralphschuler/orbitpal/actions/runs/39322457335) completed all nine jobs successfully:

- The [Apple Silicon](https://github.com/ralphschuler/orbitpal/actions/runs/39322457335/job/118304390361) and [Intel](https://github.com/ralphschuler/orbitpal/actions/runs/39322457335/job/118304390308) jobs signed with the persistent identity, removed their temporary identities and verified the actual app certificate, exact designated requirement, hardened runtime, strict signature and sealed microphone entitlement/usage description.
- All four platform jobs successfully attested their build outputs. The [publisher](https://github.com/ralphschuler/orbitpal/actions/runs/39322457335/job/118311288505) then verified their tagged build provenance and all five distinct updater payload signatures, including signed version binding, before generating the manifest and publishing.
- The tagged CI packaging/security checks passed, including the throwaway-key recovery/rotation rehearsal. They did not establish custody or restore readiness of the real maintainer backup.

These are actual production-pipeline results. They do not prove first launch, microphone recording, screen capture, native updater installation or TCC permission retention. No Developer ID/notarization or Authenticode identity was added.

## Artifact provenance

Each of the four hosted build jobs attests its collected packages, detached updater signatures and (on Linux) dependency inventories, before upload. The pinned [actions/attest v4.2.2](https://github.com/actions/attest/tree/1e69f48acb82d1966a394da916b4c1698aa569d6) creates SLSA v1 provenance. Only those jobs get `id-token: write` and `attestations: write`. No OCI storage records or `artifact-metadata` permission are requested.

The publisher downloads only `release-*` artifacts and runs `scripts/release-provenance.mjs` **before** manifest generation. GitHub CLI must verify every downloaded file against the official repository, release workflow, tagged source ref, full source/signer commit and hosted-runner policy. Missing provenance, changed bytes, verification errors or unavailable services stop publication. Its token receives explicit `attestations: read` for the authenticated [repository attestations endpoint](https://docs.github.com/en/rest/repos/attestations#list-attestations). Publication receives no updater/OS private key and no OIDC or attestation-write permission.

Manual dispatch must execute the workflow at the selected tag, because the workflow's GitHub identity must match its checkout:

```sh
gh workflow run release.yml --ref vVERSION -f tag=vVERSION --repo ralphschuler/orbitpal
```

Replace `VERSION` with an existing, reviewed version. Selecting `main` while entering a different tag is rejected. Existing versions without attestations are not retroactively attested or overwritten.

For each future artifact, verify with a reviewed tag and its independently obtained full commit:

```sh
gh attestation verify ./OrbitPal-vVERSION-windows-x64-setup.exe \
  --repo ralphschuler/orbitpal \
  --signer-workflow ralphschuler/orbitpal/.github/workflows/release.yml \
  --source-ref refs/tags/vVERSION --source-digest COMMIT --signer-digest COMMIT \
  --deny-self-hosted-runners --predicate-type https://slsa.dev/provenance/v1
```

The four `provenance-*` Actions artifacts retain Sigstore bundles for 14 days. For a lasting offline audit, download the per-file bundle from GitHub's attestation API and save fresh Sigstore trusted-root metadata as described in [GitHub's offline verification guide](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/verify-attestations-offline). Offline checks need `--bundle` and `--custom-trusted-root` plus the identity constraints above. When accepting a bundle from another source, compare the verified certificate's repository identifier with the independently recorded repository ID; names alone can be reused. Preserve the observed run URL, tag/commit, repository ID, policy, CLI version and verified bundle with the release audit.

## Release and tag immutability

On **2026-10-20**, repository release immutability was enabled and its API read-back confirmed `enabled: true`. [GitHub release immutability](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes) applies to future published release assets and tags. The existing complete-draft-then-publish workflow supports this policy. Confirm the next release's actual `immutable: true` result after publication.

The v0.10.0 release was published before activation and still reports `immutable: false`; enabling the setting does not retrofit its assets. Keep that release unchanged. Separately, the active [version-tag ruleset](https://github.com/ralphschuler/orbitpal/rules/25445829) rejects updates and deletions of `refs/tags/v*` without bypass actors. It permits new version tags, so failed or superseded versions must be replaced by a new version rather than by moving or deleting a tag. Tag protection also covers older version refs but does not make their release assets immutable.

Administrators can inspect the release setting through `GET /repos/ralphschuler/orbitpal/immutable-releases` and the tag rules through the repository's public rules page. The [official API](https://docs.github.com/en/rest/repos/repos#enable-immutable-releases) documents the setting. These recorded read-backs establish configuration activation; the first future immutable publication remains a separate production observation.

## Protected signing environment: maintainer activation

The 2026-10-20 configuration review found no environments and only `ralphschuler` as a configured human collaborator; the v0.10.0 release initiator was the same account. Self-review prevention needs a separate eligible human reviewer. An agent using that account does not provide independent approval. No reviewer has been invited or assigned by this source change.

The workflow deliberately does not name a new environment yet: referencing a missing environment can create it without the intended protection. A maintainer must first select the independent reviewer and create/inspect `release-signing` following [GitHub's environment documentation](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments).

1. Require a separate human reviewer, prevent self-review and disallow bypass of protection rules. Restrict deployment refs to reviewed `v*` release tags; maintain tag/ruleset protection so an arbitrary writer cannot retarget a reviewed version. Record the rules and a successful waiting-for-approval observation.
2. Populate that environment with the existing updater and persistent macOS signing material from protected maintainer backups; do not generate new identities. GitHub cannot return stored secret values. Remove duplicated repository/organization-scoped signing secrets once the migration is tested. Keep model/service credentials out of release jobs and never put private values in an issue or chat.
3. In one reviewed change, add `environment: release-signing` to the matrix build job and move the updater-secret presence check out of `validate` into the protected build job. Otherwise the current validation job cannot see environment-only secrets. [environment.yml.example](release-signing/environment.yml.example) is an inactive integration fragment.
4. Keep environment approval before secret access, checkout credentials disabled, least job permissions, full-SHA action pins and the independent publisher. If Windows signing uses provider federation, restrict its subject to this repository/environment and minimum signing role; do not broaden pull-request trust.
5. Exercise an explicit prerelease with the existing maintainer-approved identities. Record waiting-for-approval, independent approval, all platform verifications and provenance verification before using the protected route for a later stable release. Never bypass review or fall back to unsigned output to make a run pass.

This is a pending activation procedure, not evidence that a protected environment exists.

## Persistent macOS identity

The [persistent self-signed identity](RELEASING.md#persistent-self-signed-macos-identity) shipped for both architectures in v0.10.0. It pins the public certificate and exact designated requirement. The build uses a temporary private keychain, checks the actual signed output and cleans up the runner identity; signing failures stop publication without an ad-hoc fallback. The hosted CI fixture also demonstrates continuity between two synthetic app versions and rejects a different key or modified signed resources. Neither check establishes real OrbitPal updates or macOS microphone/screen permission retention. The transition from the ad-hoc releases through v0.9.0 may require renewed permission.

### Optional Developer ID and notarization

If the maintainer later chooses Apple-verified distribution, obtain a maintainer-owned **Developer ID Application** identity. This is not required for the current free self-signed mode. [Tauri's macOS signing guide](https://v2.tauri.app/distribute/sign/macos/) documents `APPLE_CERTIFICATE` (base64 PKCS#12), `APPLE_CERTIFICATE_PASSWORD` and the exact `APPLE_SIGNING_IDENTITY`. For App Store Connect API notarization use `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_PATH`. Materialize a protected API-key secret only in the runner's temporary directory with restrictive permissions, then remove it in an `always()` cleanup step. Do not print certificate/private-key contents.

The [macos.json.example](release-signing/macos.json.example) overlay is an inactive starting point for that separately reviewed migration. The current pipeline has no hardcoded ad-hoc override: it requires the pinned self-signed policy and signer wrapper. A Developer ID migration must deliberately adapt those identity/requirement checks and runner setup, preserving fail-closed signing, hardened runtime, `Info.plist`, microphone entitlements, cleanup and the stable bundle ID `studio.streamer-ai.cohost`. Apply OS signing/notarization before final updater payload/signature collection and build attestation; changing an app afterwards invalidates those proofs.

Before calling a release notarized, require successful submission, stapling and ticket validation following [Apple's notarization documentation](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution). Run `codesign --verify --deep --strict --verbose=2`, `xcrun stapler validate` and the appropriate `spctl --assess` for the app/distribution container. Keep `verify-macos-permissions.mjs`. Check downloaded Apple Silicon and Intel packages on clean supported Macs, including first launch, microphone prompt, screen capture and a real updater transition. Developer ID/notarization and native installation/privacy acceptance remain unverified; the completed self-signed code-signature checks for v0.10.0 are recorded separately above.

## Optional Windows Authenticode

If the maintainer later chooses Authenticode, select a maintainer-owned hardware-backed certificate or an eligible managed signing provider; account/region/identity approval remains the maintainer's task. This optional migration does not block the current updater-signed Windows distribution. [Tauri's Windows guide](https://v2.tauri.app/distribute/sign/windows/) supports `bundle.windows.signCommand`, whose `%1` argument is the file to sign. Its old exportable-OV-PFX recipe applies only to certificates obtained before June 2023. Signing alone does not guarantee that SmartScreen warnings disappear.

[windows.json.example](release-signing/windows.json.example) is an inactive contract for a reviewed signer wrapper, not a supplied or configured provider. Before activation, implement that wrapper with a pinned, verified provider CLI, minimum identity permissions, SHA-256 digest and a provider-supported trusted timestamp. It must propagate signing failure and reject paths outside the current build tree. [Microsoft Artifact Signing](https://learn.microsoft.com/en-us/azure/artifact-signing/overview) is one possible managed provider; no account, region or identity has been selected.

Integrate signing through Tauri during bundling so executable contents and both NSIS/MSI installers are signed **before** their updater signatures are created. Afterwards, use Windows SDK `signtool verify /pa /all /v` on the final exe/MSI and packaged executable, and inspect `Get-AuthenticodeSignature` for valid status, expected publisher and timestamp. Preserve MSI UpgradeCode and NSIS migration identities. Verify downloaded NSIS and MSI clean installation, upgrade from supported releases, uninstall and the matching updater route on disposable Windows hosts. Production signing and Windows acceptance remain untested.

## Updater backup, recovery and rotation

Back up the **existing Tauri/minisign private key in its original encrypted format**, its separately protected password, public key fingerprint and recovery instructions in independently controlled offline storage. Record owner/alternate custodian and last restore date without recording key values. A new key generated after a loss cannot sign updates for clients that trust the old key.

Run the offline, credential-independent exercise:

```sh
node scripts/rehearse-updater-keys.mjs
node --test scripts/release-tests.mjs
```

It generates real throwaway Ed25519 keys A/B, encrypts and restores A's PKCS#8 backup in memory, rejects a wrong password, signs a dummy bridge with restored A, verifies the next dummy release with B, and proves old A trust rejects B and tampered bytes. It never reads the configured production key. Optional `--public-fixtures NEW_DIRECTORY` exports only Tauri/minisign-formatted public keys, dummy JSON payloads and detached signatures for an independent verifier. PKCS#8 is used only in this exercise; production backup/recovery must retain Tauri's actual key format. The exercise does not prove backup-media durability or an installable multi-platform updater transition.

For planned rotation while A remains available, ship and retain a bridge app that embeds B's public key but whose **updater package is signed with A**. Clients installed before the bridge must still be offered an A-authorized route; switching the single stable manifest directly to B can strand them. The current app has one updater trust key and no staged per-client key-rotation routing. Before production rotation, design that routing, rehearse skipped-version/offline clients and native installation on all targets, then release the bridge and subsequent B-signed release deliberately. Never silently turn off signature/version checks or replace immutable published files.

If A is lost, restore its protected backup, verify its public fingerprint, and test recovery on throwaway artifacts before restoring CI access. If both A and its backup are lost, existing A-trusting installations require a separately authenticated manual reinstall; no unsigned remote bypass exists. If A is compromised, suspend publication, rotate/revoke CI/provider access, preserve evidence and communicate authenticated recovery instructions. Changing GitHub secrets or publishing B alone cannot revoke A in already installed applications.

Record the latest rehearsal with tool versions, pass/fail results and public fingerprints only. The v0.10.0 tagged CI passed the offline recovery/rotation model and public minisign fixture checks; actual recovery custodians/media, protected environments and native A→B installation remain maintainer acceptance work.
