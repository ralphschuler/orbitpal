# Release trust and signing activation

Status on 2026-10-21: **v0.10.0** shipped with the pinned persistent self-signed macOS identity, four platform build attestations and successful provenance/updater-signature verification before publication. Published macOS releases through **v0.9.0** remain ad-hoc signed. The tag-restricted `release-signing` environment was created and read back on 2026-10-21. All four platform signing builds are bound to it; same-identity input migration and exact repository-fallback retirement were verified that day. A hosted tagged signing run through this route, independent backup-media restoration and real installation/permission acceptance remain separate unobserved follow-ups. The maintainer's current choice is free persistent self-signing; optional Developer ID/notarization and Windows Authenticode are separate future migrations.

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

## Tag-restricted signing environment: configured policy and workflow guard

The initial read-only repository check on **2026-10-21** returned `total_count: 0`. The maintainer-authorized follow-up then created `release-signing` and read back environment ID **24870421379** at **2026-10-21 00:37:31 UTC**: `protected_branches: false`, `custom_branch_policies: true`, with exactly one `type: tag`, `name: v*` deployment rule. The read-back also reports `can_admins_bypass: true` and only the branch-policy protection rule; no required reviewer or wait timer is configured. This establishes tag-policy configuration, not independent approval or exclusive key custody.

The workflow binds the four platform signing builds to `release-signing` and checks its public tag policy before packaging. Environment creation initially changed public policy only. A later, independently source-reviewed maintainer operation migrated the existing inputs from legitimate protected backups and verified scoped metadata: all four environment names are present, three former repository entries were removed, and the optional repository updater password was already absent. The updater key cryptographically signed a non-installable JSON challenge against the configured public key with an explicit empty password; the existing macOS archive/password passed pinned-certificate and encrypted-key-bag inspection. That archive inspection is not a codesigning rehearsal. Both identities and their public pins remain unchanged. No new release or keychain/trust change was performed. A hosted tagged signing run through this route has not yet been observed.

The workflow validates the existing environment's **public metadata** before scheduling packaging, and rechecks it inside the environment-bound build before mapping any signing inputs. Both checks require custom deployment policies with exactly one rule: `type: tag`, `name: v*`; branch rules, all-ref access, extra broad rules, missing configuration and unavailable metadata stop the run. This avoids treating GitHub's automatic creation of an unconfigured environment as successful activation. The checks use only the job's ephemeral `GITHUB_TOKEN` with read permissions; no personal access token, paid provider, wait timer or required reviewer is introduced.

The `v*` policy restricts **tag refs**, not semantic versions or stable-only releases. The existing manifest/version and tagged workflow/ref/full-commit checks remain authoritative. Both valid stable and prerelease tags retain their existing behavior, including that prereleases do not replace stable Latest. The one rule applies to later version tags without a new administrative allowlist edit for each release. See GitHub's [environment rules](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments) and [tag deployment-policy API](https://docs.github.com/en/rest/deployments/branch-policies?apiVersion=2026-03-10).

The following records and future operational checks remain distinct:

1. Preserve and re-inspect the already configured `release-signing` environment and single **tag-typed** `v*` rule before an authorized tagged run. The field is named `custom_branch_policies` even for tag rules. Do not recreate the environment or replace the rule for each version. Preserve the existing version-tag update/deletion rules. This baseline does not invent a second reviewer or claim independent operational approval; a maintainer may deliberately add stronger controls without changing the guard. Do not erase existing stronger protection.
2. Preserve the verified same-identity migration and exact repository-fallback retirement recorded above. GitHub cannot return stored secret values; name metadata alone does not establish stored bytes or their source. The completed writes, real updater challenge, public pins, environment-bound workflow and absent repository fallbacks establish the configured boundary. Keep model/service credentials separate. Historical workflow refs without environment binding can no longer sign through the retired repository fallbacks; use a new reviewed tag containing the guarded workflow for future releases.
3. Keep the updater-key presence check inside the environment-bound `build` job, after its public-policy recheck, in any future workflow changes. The secret-free `validate` job must not read updater or OS signing values. Retain the independent publisher, signature/provenance verification, full-SHA action pins, least permissions, disabled checkout credentials and hosted-runner cleanup. [environment.yml.example](release-signing/environment.yml.example) describes the source integration but does not configure GitHub.
4. When a new tagged release is explicitly authorized, observe that all four build jobs use the configured environment and pass the public-policy check before signing input checks, then record actual platform identity/signature/provenance verification and cleanup. Do not create a test release merely to activate this source. A prerelease remains possible under the existing version policy when separately authorized.
5. Record independent backup custody/restore evidence and native installed-update/privacy acceptance when separately performed. These are additional operational follow-ups beyond the original planning criteria of issue #17. Tag-only protection, source review and an agent using the maintainer account do not establish independent human approval, physical backup restoration or native installation.

Metadata checks establish the observed policy and exact-name presence/absence. Their combination with known successful encrypted writes, identity checks and guarded workflow source establishes configuration acceptance as defined below. They do not prove a hosted tagged signing run, independent backup custody, future administrator behavior, human approval or native installation. The original criteria of issue #17 cover available identities, independent updater signatures, provenance/protected signing configuration and recovery/rotation with test keys; retain the additional observations as explicit future limits.

### Secret-scope configuration acceptance

For the protected-signing-environment criterion of [issue #17](https://github.com/ralphschuler/orbitpal/issues/17), verify configuration and operational observations separately. Before retiring any repository fallback:

1. Read back the fixed official repository's `release-signing` environment and its single tag-typed `v*` rule. Retain any stronger existing protection and the version/ref/commit checks.
2. Use legitimate protected backups of the existing identities. Keep the configured updater public key and pinned macOS certificate unchanged. Verify a bounded, non-installable local updater signature against that public key. Inspect the existing encrypted macOS archive/password and pinned certificate; certificate/key-bag inspection alone is not a codesigning rehearsal.
3. Copy only the required signing inputs to that environment through encrypted secret writes, with values on stdin and diagnostics withheld. Verify each exact secret name afterward. Name metadata cannot return or independently prove stored bytes. An optional updater password may be empty only after the same-key challenge succeeds with that explicit empty password; eliminate any repository password fallback before relying on its absence.
4. Independently review that all four platform signing builds are environment-bound and recheck its public policy before accessing signing inputs. PR jobs and the publisher must receive no signing keys.
5. After those identity, write and workflow checks pass, retire only the exact reviewed repository duplicates and verify each is absent. Preserve the protected backups. On a failed or partial operation, retain its receipt and inspect the actual state before continuing; never blindly retry or replace an identity.

Together, the successful writes, scoped name metadata, workflow source and absent fallbacks establish the configured key-access boundary. They do not establish an observed environment-delivered signing run, independently controlled backup-media restoration, human operational approval or native installation/privacy acceptance. Record those observations when separately performed. Do not create a release merely to satisfy this configuration check; the first hosted run through this route belongs to the next explicitly authorized tagged release.

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

Record the latest rehearsal with tool versions, pass/fail results and public fingerprints only. The v0.10.0 tagged CI passed the offline recovery/rotation model and public minisign fixture checks; independent recovery custodians/media and native A→B installation remain future maintainer acceptance work. The protected environment configuration and same-identity scope migration are recorded above.
