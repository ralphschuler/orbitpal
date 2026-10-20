## Fixed in 0.10.1

- Existing provider credentials retain their IDs and native storage mappings while the app switches to explicit native credential-store adapters. Credential errors show translated categories without exposing secret values or backend messages.
- Wayland source selection first saves the current bot/model settings. If settings or bots change while the chooser is open, a discarded result closes only its own sharing session, preserving newer selections and other bots. Save again after selecting the new source.
- Remove three legacy maintenance-warning package versions from the production dependency graph; the remaining warnings and the separately audited legacy test fixture remain documented.

Wayland remains experimental. A separate GNOME 46 prototype captured fresh synthetic frames; KDE acceptance is still blocked by the test compositor. That unfinished desktop-test prototype is tracked in draft PR #56 and is not included in this patch release.

## Downloads and updates

The release pipeline builds Windows x64 NSIS/MSI installers, macOS Apple Silicon and Intel ZIPs/DMGs, and Linux x64 AppImage/DEB packages. It requires platform checks, build-provenance verification and cryptographic verification of all five updater payloads before publication. Release files include dependency inventories, source code and SHA256SUMS.

macOS retains the persistent self-signed identity introduced in 0.10.0 and is not Apple-notarized. Windows installers have no Authenticode signature. Permission retention through installed updates remains unverified; upgrading from ad-hoc releases through 0.9.0 may require renewed microphone/screen approval.

In-app updates require explicit confirmation. Linux in-app installation uses AppImage; DEB packages update manually. Existing profiles, bot grants, credential IDs, the installed app ID and updater trust key remain unchanged.

Issues [#17](https://github.com/ralphschuler/orbitpal/issues/17), [#19](https://github.com/ralphschuler/orbitpal/issues/19) and [#26](https://github.com/ralphschuler/orbitpal/issues/26) remain open for signing/recovery and installed-update acceptance, remaining upstream dependency migrations, and full Wayland desktop/provider/lifecycle acceptance.

See the [changelog](https://github.com/ralphschuler/orbitpal/blob/v0.10.1/CHANGELOG.md), [credential compatibility scope](https://github.com/ralphschuler/orbitpal/blob/v0.10.1/scripts/fixtures/keyring-compat/README.md), [Wayland validation](https://github.com/ralphschuler/orbitpal/blob/v0.10.1/docs/WAYLAND-VALIDATION.md) and [release trust](https://github.com/ralphschuler/orbitpal/blob/v0.10.1/docs/RELEASE-TRUST.md).
