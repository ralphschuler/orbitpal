# Backlog integration

The integration combines the reviewed feature branches with the desktop presentation and Autopilot changes merged in [PR #51](https://github.com/ralphschuler/orbitpal/pull/51). Earlier branch checks do not replace validation of the combined source.

| Original PR | Scope | Issue |
| --- | --- | --- |
| [#32](https://github.com/ralphschuler/orbitpal/pull/32) | OBS loopback security headers | [#14](https://github.com/ralphschuler/orbitpal/issues/14) |
| [#33](https://github.com/ralphschuler/orbitpal/pull/33) | Cryptographic updater payload verification before publication | [#16](https://github.com/ralphschuler/orbitpal/issues/16) |
| [#34](https://github.com/ralphschuler/orbitpal/pull/34) | Cancellation during update download and verification | [#15](https://github.com/ralphschuler/orbitpal/issues/15) |
| [#35](https://github.com/ralphschuler/orbitpal/pull/35) | TypeScript 7 checking with TypeScript 6 editor compatibility | [#18](https://github.com/ralphschuler/orbitpal/issues/18) |
| [#36](https://github.com/ralphschuler/orbitpal/pull/36) | Rust dependency maintenance evidence | [#19](https://github.com/ralphschuler/orbitpal/issues/19) |
| [#37](https://github.com/ralphschuler/orbitpal/pull/37) | Build provenance and signing recovery preparation | [#17](https://github.com/ralphschuler/orbitpal/issues/17) |
| [#38](https://github.com/ralphschuler/orbitpal/pull/38) | Per-bot VRMA import and retargeting | [#20](https://github.com/ralphschuler/orbitpal/issues/20) |
| [#39](https://github.com/ralphschuler/orbitpal/pull/39) | Explicit Wayland ScreenCast/PipeWire sessions | [#26](https://github.com/ralphschuler/orbitpal/issues/26) |
| [#41](https://github.com/ralphschuler/orbitpal/pull/41) | Guided OS permissions and persistent macOS signing identity | [#40](https://github.com/ralphschuler/orbitpal/issues/40) |
| [#42](https://github.com/ralphschuler/orbitpal/pull/42) | A bounded embedded model-audio compatibility profile | [#22](https://github.com/ralphschuler/orbitpal/issues/22) |
| [#44](https://github.com/ralphschuler/orbitpal/pull/44) | Locally bundled compression decoders | [#21](https://github.com/ralphschuler/orbitpal/issues/21) |
| [#46](https://github.com/ralphschuler/orbitpal/pull/46) | Current-character emote and sound admission | [#43](https://github.com/ralphschuler/orbitpal/issues/43) |
| [#47](https://github.com/ralphschuler/orbitpal/pull/47) | OrbitPal branding and compatibility audit | [#45](https://github.com/ralphschuler/orbitpal/issues/45) |

## Integration boundaries

- Autopilot keeps its exact per-attempt claim across permission preflight and rechecks that claim and the original session before dispatch. Pausing or restarting cannot turn an old pending permission inspection into a new-session request. Its displayed status and scheduling use the same blocker calculation. Guided permission management and explicit Wayland source selection remain separate from each bot's capture grant.
- A desktop character returns to a fully visible resting point before a new successful rendered frame releases prepared speech, text or explicit gestures. Current model and motion bindings are checked before sound preparation and again at presentation. Only admitted gestures determine presentation hold durations; VRMA uses its validated natural duration up to 120 seconds.
- Compression validation retains access to the GLB binary offset when sharing container parsing with VRMA and emote metadata. Local decoder assets and their third-party notices remain bundled.
- Native pending-emote admission runs before the transactional speech start, so cancellation during metadata validation is observed before history and presentation are committed. Voice snapshots and original chat routes remain attached to their exact queued response.
- The renderer distinguishes its own ambient walking and automatically generated gesture sounds from explicit foreground gestures. These ambient sounds do not restart the autonomous quiet period; actual audio still blocks microphone activation, ordinary chat playback and updater installation. Explicitly triggering the same walking or gesture clip as an emote keeps foreground ownership. External sound kinds have separate playback IDs; legacy reports remain foreground.
- The published [v0.10.0 workflow](https://github.com/ralphschuler/orbitpal/actions/runs/39322457335) successfully attested all four platform builds, verified their provenance and all five updater payload signatures, and used the pinned persistent self-signed identity for both Mac architectures. Private signing inputs stay out of publication and diagnostics. Published versions through 0.9.0 used ad-hoc signing.

## Acceptance still requiring external evidence

- [#26](https://github.com/ralphschuler/orbitpal/issues/26): real GNOME and KDE Wayland capture, source changes and permission revocation. Local lifecycle fixtures and Linux compilation do not establish desktop-compositor acceptance.
- [#17](https://github.com/ralphschuler/orbitpal/issues/17): protected signing environments with an independent human reviewer, recoverable key custody and real platform/update/privacy acceptance remain pending. Actual tagged-release signing/provenance succeeded in v0.10.0. Optional Developer ID/notarization and Windows Authenticode are separate future migrations; they do not block the current free self-signed mode. Persistent self-signing does not establish notarization or guaranteed macOS permission retention.
- [#19](https://github.com/ralphschuler/orbitpal/issues/19): supported upstream migrations for the remaining transitive maintenance advisories, tracked in [DEPENDENCY-MAINTENANCE.md](DEPENDENCY-MAINTENANCE.md). Maintenance warnings are not evidence of a fixed or exploitable vulnerability.

Automated checks use local fixtures and mocks. They do not exercise paid providers, real microphones, live stream accounts, the user's OS privacy settings or an installed OrbitPal update. The later v0.10.0 release is recorded separately above; source integration alone does not publish or install the app. Substantial AI assistance was used for implementation, fixtures, documentation and independent reviews.
