## New in 0.12.0

- Manage named API connections centrally across all characters, including shared defaults and exact service/destination bindings. Characters retain their own models, voices, memory banks and permissions. Keys remain in the OS credential store; imported profiles do not inherit local connections.
- Optionally reuse successfully retrieved macOS keys for the current app session, with explicit Lock controls and invalidation on changes. This is off by default and does not bypass Keychain authorization or guarantee that permission prompts disappear.
- Add explicitly started native Ambient/Life sessions, durable per-bot Life state, an activity display and a shared stage editor in German, English, French and Spanish. Saved settings never activate autonomy.
- Open an optional, currently muted Studio 3D World view with one shared object layer and up to eight participants. In an already active Life session, the selected Studio character can follow one operator-requested MoveTo target; other participants are mirror views. Native World overlays, OBS execution, simultaneous multi-actor actions and broader visual Life activities remain unfinished under #72.
- Add durable installation-wide autonomous usage budgets with per-bot breakdowns. Count supported provider operations conservatively, keep usage across restarts and show unavailable receipts explicitly; this is not an invoice or monetary limit.
- Distribute automatic chat turns fairly among eligible bots, explain hourly contribution attempts, offer an explicit ten-second silent rest without an animation or needs reward, and preserve Stop, late-result cancellation and actual-playback presentation timing.
- Fix concurrent credential-operation bookkeeping and fence late native-window callbacks so retired windows cannot cancel their replacements' output jobs.

## Downloads and updates

Windows x64 includes NSIS and MSI installers. macOS includes Apple Silicon and Intel ZIPs/DMGs. Linux x64 includes AppImage and DEB packages. The pipeline requires checks for all three operating systems, tagged build provenance and cryptographic verification of all five updater payloads before publication. Dependency inventories, source code and SHA256SUMS are included.

In-app updates require explicit confirmation. Linux in-app installation uses AppImage; DEB packages update manually. Existing installed app identity, credential namespace and updater trust key remain compatible. API references migrate without reading or copying OS-store secret values.

macOS retains the pinned persistent self-signed identity and is **not Apple-notarized**. Windows installers have **no Authenticode signature**. Real installed-update and microphone/screen permission retention remain unverified; updating from ad-hoc releases through 0.9.0 can require renewed grants.

Validation uses local fixtures and mock providers. Renderer fixtures exercise actual submitted geometry and eight-direction movement but do not prove GPU pixels, native overlays, live providers or measured performance. Wayland remains experimental pending full application acceptance (#26). Installed-update/signing custody (#17), upstream dependency migrations (#19), resource measurements (#66) and the remaining desktop-life features stay open. Unmerged diagnostic and benchmark work is excluded from this release.

See the [changelog](https://github.com/ralphschuler/orbitpal/blob/v0.12.0/CHANGELOG.md), [global API connections](https://github.com/ralphschuler/orbitpal/blob/v0.12.0/docs/GLOBAL-API-CONNECTIONS.md), [World output scope](https://github.com/ralphschuler/orbitpal/blob/v0.12.0/docs/WORLD-OUTPUT.md) and [release trust](https://github.com/ralphschuler/orbitpal/blob/v0.12.0/docs/RELEASE-TRUST.md).
