## New in 0.13.0

- Set up Desktop Life visually: drag characters, places and movement areas, then explicitly save and start the selected participants. Character windows use the native World output path; Stop invalidates pending work and late playback.
- Add experimental local care, exploration, coordinated social actions, per-character routines and development. Actions use checked reservations and actual completion before crediting outcomes.
- Explain current intentions and offer Quiet, Focus and Stream preferences, direct interaction controls, isolated relationships and a factual private memory album.
- Load declarative namespaced content packs and accept bounded Twitch/YouTube event proposals and votes. Optional short social dialogue respects native budgets and falls back locally.
- Provide these controls in German, English, French and Spanish. API connections remain installation-wide; character memories and permissions remain separate.
- Update Tauri to 2.12.5, improve isolated native diagnostics and cache acceptance-build dependencies.

## Known limits

Desktop Life is experimental. Complete native movement, paired scenes, hidden operation, sleep/reconnect, monitor/DPI and OBS acceptance remain open under #72/#75 and related issues. No measured 1/3/5-character performance baseline is claimed (#66). The latest nonshipping macOS/Wayland tests still stop in their native UI/input harnesses; the Linux renderer smoke stops during isolated GNOME startup.

The memory album currently shows only the first 40 retained facts for individual management (#82). The five unmaintained transitive dependency families remain tracked in #19; this patch update does not remove them. Wayland screen context remains experimental pending complete application acceptance (#26).

## Downloads and updates

Windows x64 includes NSIS and MSI installers. macOS includes Apple Silicon and Intel ZIPs/DMGs. Linux x64 includes AppImage and DEB packages. Publication requires all platform checks, tagged build provenance and cryptographic verification of the updater payloads. Source, dependency inventories and SHA256SUMS are included.

In-app updates require explicit confirmation. Linux in-app installation uses AppImage; update DEB packages manually. The existing installed identity, credential namespace and updater trust key are retained.

macOS uses the existing persistent self-signed identity and is not Apple-notarized. Windows installers have no Authenticode signature. Real installed-update permission retention remains unverified. MIT and third-party licenses are unchanged.

See the [changelog](https://github.com/ralphschuler/orbitpal/blob/v0.13.0/CHANGELOG.md), [native World scope](https://github.com/ralphschuler/orbitpal/blob/v0.13.0/docs/NATIVE-WORLD-ADAPTER.md), [dependency tracking](https://github.com/ralphschuler/orbitpal/blob/v0.13.0/docs/DEPENDENCY-MAINTENANCE.md) and [release trust](https://github.com/ralphschuler/orbitpal/blob/v0.13.0/docs/RELEASE-TRUST.md).
