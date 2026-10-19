# Release trust and signing activation

Status on 2026-10-19: updater signatures are mandatory. This change prepares platform signing and protected environments; it does **not** claim that Apple/Windows identities, notarization, environment rules or real installation acceptance have been configured or verified. Issue #17 remains open until those operational steps are recorded by a maintainer.

## Four different assurances

| Mechanism | What it establishes | Current boundary |
| --- | --- | --- |
| Tauri/minisign updater signature | The installed app's trusted key authorized the payload and signed version | Required independently of OS signing |
| Authenticode | A Windows publisher identity signed executable/installer bytes | No production identity configured |
| Developer ID and notarization | Apple identity and Apple's distribution assessment | Current packages use ad-hoc signing |
| GitHub/Sigstore provenance | A specific GitHub workflow/ref/commit produced matching file digests | New release workflow gate; first real tagged run still needs observation |

Provenance is not a malware scan, a reproducible-build claim or an assertion of a particular SLSA level. The release manifest, source ZIP and checksum list are generated later by the publication job and are not presented as platform build subjects.

## Artifact provenance

Each of the four hosted build jobs attests its collected packages, detached updater signatures and (on Linux) dependency inventories, before upload. The pinned [actions/attest v4.2.2](https://github.com/actions/attest/tree/1e69f48acb82d1966a394da916b4c1698aa569d6) creates SLSA v1 provenance. Only those jobs get `id-token: write` and `attestations: write`. No OCI storage records or `artifact-metadata` permission are requested.

The publisher downloads only `release-*` artifacts and runs `scripts/release-provenance.mjs` **before** manifest generation. GitHub CLI must verify every downloaded file against the official repository, release workflow, tagged source ref, full source/signer commit and hosted-runner policy. Missing provenance, changed bytes, verification errors or unavailable services stop publication. Publication receives no updater/OS private key and no OIDC or attestation-write permission.

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

## Protected signing environment: maintainer activation

The workflow deliberately does not name a new environment yet: referencing a missing environment can create it without the intended protection. Nobody has changed repository settings as part of this source change. A maintainer with repository administration access must first create and inspect `release-signing` following [GitHub's environment documentation](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments).

1. Require a separate human reviewer, prevent self-review and disallow bypass of protection rules. Restrict deployment refs to reviewed `v*` release tags; maintain tag/ruleset protection so an arbitrary writer cannot retarget a reviewed version. Record the rules and a successful waiting-for-approval observation.
2. Move the updater signing secrets into that environment deliberately; add OS secrets only after the identity/provider is verified. Remove duplicated repository/organization-scoped signing secrets once the migration is tested. Keep model/service credentials out of release jobs.
3. In one reviewed change, add `environment: release-signing` to the matrix build job and move the updater-secret presence check out of `validate` into the protected build job. Otherwise the current validation job cannot see environment-only secrets. [environment.yml.example](release-signing/environment.yml.example) is an inactive integration fragment.
4. Keep environment approval before secret access, checkout credentials disabled, least job permissions, full-SHA action pins and the independent publisher. If Windows signing uses provider federation, restrict its subject to this repository/environment and minimum signing role; do not broaden pull-request trust.
5. Exercise an explicit prerelease with a maintainer-approved identity. Record environment approval, all platform verifications and the provenance verification result before enabling stable publication. Never fall back to unsigned output when an enabled identity fails.

This is a pending activation procedure, not evidence that a protected environment exists.

## macOS identity and notarization

Use a maintainer-owned **Developer ID Application** identity for distribution outside the App Store. [Tauri's macOS signing guide](https://v2.tauri.app/distribute/sign/macos/) documents `APPLE_CERTIFICATE` (base64 PKCS#12), `APPLE_CERTIFICATE_PASSWORD` and the exact `APPLE_SIGNING_IDENTITY`. For App Store Connect API notarization use `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_PATH`. Materialize a protected API-key secret only in the runner's temporary directory with restrictive permissions, then remove it in an `always()` cleanup step. Do not print certificate/private-key contents.

The [macos.json.example](release-signing/macos.json.example) overlay requires the maintainer's verified identity. Replace the workflow's hardcoded ad-hoc `signingIdentity: "-"` configuration; leaving that override would defeat the intended identity. Preserve hardened runtime, `Info.plist`, microphone entitlements and stable bundle ID `studio.streamer-ai.cohost`. Apply OS signing/notarization before final updater payload/signature collection; changing an app after signing invalidates those proofs.

Before calling a release notarized, require successful submission, stapling and ticket validation following [Apple's notarization documentation](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution). Run `codesign --verify --deep --strict --verbose=2`, `xcrun stapler validate` and the appropriate `spctl --assess` for the app/distribution container. Keep `verify-macos-permissions.mjs`. Check downloaded Apple Silicon and Intel packages on clean supported Macs, including first launch, microphone prompt, screen capture and a real updater transition. None of these identity/acceptance checks has been run with production credentials here.

## Windows Authenticode

Select a maintainer-owned hardware-backed certificate or an eligible managed signing provider; account/region/identity approval remains the maintainer's task. [Tauri's Windows guide](https://v2.tauri.app/distribute/sign/windows/) supports `bundle.windows.signCommand`, whose `%1` argument is the file to sign. Its old exportable-OV-PFX recipe applies only to certificates obtained before June 2023. Signing alone does not guarantee that SmartScreen warnings disappear.

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

Record the latest rehearsal with tool versions, pass/fail results and public fingerprints only. This contribution tested the offline cryptographic model and independent minisign fixture verification; actual recovery custodians/media, protected environments and native A→B installation remain maintainer acceptance work.
