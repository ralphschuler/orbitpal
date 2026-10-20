## New in 0.11.1

- Fix desktop Autopilot walking in vertical and diagonal directions. Characters turn upright along their actual path; a slightly elevated desktop view makes depth steps visible. Preserve offscreen excursions, visible returns before speech/emotes, live size changes and embedded idle poses.
- Match embedded and procedural walking to actual traveled distance instead of elapsed session time. Calibrate authored strides once from bounded root/foot sampling and account for figure size, acceleration and projected depth. Keep walking sound rate synchronized. Unusual rigs use a body-height approximation; this is not inverse-kinematic foot locking.
- Report empty, whitespace-only and emotion-only Autopilot responses as errors. Only an explicit model silence signal counts as a successful quiet turn. Deliberate silence, configured intervals and hourly budgets remain supported without forced extra attempts.

## Downloads and updates

Windows x64 includes NSIS and MSI installers. macOS includes Apple Silicon and Intel ZIPs/DMGs. Linux x64 includes AppImage and DEB packages. The pipeline requires platform checks, build-provenance verification and cryptographic verification of all five updater payloads before publication. Dependency inventories, source code and SHA256SUMS are included.

In-app updates require explicit confirmation. Linux in-app installation uses AppImage; DEB packages update manually. Existing profiles, bot grants, credential IDs, installed app identity and updater trust key stay compatible.

macOS retains the pinned persistent self-signed identity and is **not Apple-notarized**. Windows installers have **no Authenticode signature**. Real installed-update and microphone/screen permission retention remain unverified; updating from ad-hoc releases through 0.9.0 can require renewed grants.

Local validation uses fixture data: 436 frontend tests, 260 Rust unit tests and an offline Chromium review of nine characters, 72 directional states and nine settled idle poses. Browser checks use synthetic controller travel with the real renderer; they do not establish native overlays, operating-system permission retention, real audio playback or live-provider acceptance. Wayland remains experimental; unfinished desktop acceptance stays in draft PR #56 and issue #26. Signing/recovery/installed-update acceptance (#17) and remaining upstream dependency migrations (#19) also remain open.

See the [changelog](https://github.com/ralphschuler/orbitpal/blob/v0.11.1/CHANGELOG.md), [Autopilot guide](https://github.com/ralphschuler/orbitpal/blob/v0.11.1/docs/AUTOPILOT.md), [Wayland validation](https://github.com/ralphschuler/orbitpal/blob/v0.11.1/docs/WAYLAND-VALIDATION.md) and [release trust](https://github.com/ralphschuler/orbitpal/blob/v0.11.1/docs/RELEASE-TRUST.md).
