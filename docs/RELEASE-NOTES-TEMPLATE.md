## New in 0.10.0

- Autopilot shows the actual reason it is waiting, the next-attempt countdown and silent attempts. It can begin a short persona-based conversation without inventing events. Automatic walking and gesture sounds no longer keep resetting its quiet period; pausing/restarting cannot revive an old pending request.
- Desktop characters can briefly walk out of view. Before prepared speech, text-only replies or explicit gestures start, the character returns fully into view, stops and renders successfully. Stop cancels a waiting presentation.
- Import separate VRMA 1.0 motions per bot for supported VRM 0/1 characters, including body, expression and gaze tracks at their natural duration. Load compressed Draco, Meshopt and BasisU/KTX2 avatars with bundled offline decoders. Only emotes supported by the current character are offered and played.
- Play embedded animation sounds through the bounded, versioned MSFT_audio_emitter draft compatibility profile. Sound follows rendered animation events, audio activation, the selected output, volume and Stop. Loading a model or playing idle does not start audio; this is not universal glTF/VRM audio support.
- Guided microphone/screen permission checks provide explicit requests, Settings and recheck actions. Returning from Settings never starts recording. Wayland adds explicit per-bot ScreenCast/PipeWire source selection, fresh in-memory frames and disconnect/reconnect; it remains experimental pending real GNOME/KDE acceptance.
- Cancel update downloads and pre-install verification, then check and confirm again to retry. Installation is non-cancellable after replacement starts. OBS security headers, cryptographic pre-publication payload checks and build-provenance verification strengthen the output and release paths.

Current OrbitPal branding is reconciled while installed compatibility identities remain stable. Development checks now use native TypeScript 7 alongside the TypeScript 6 editor API.

## Downloads and updates

Windows x64 NSIS/MSI installers, macOS Apple Silicon and Intel app ZIPs/DMGs, and Ubuntu 24.04 x64 AppImage/DEB packages are attached below. The release also includes signed updater payloads, dependency inventories, source code and SHA-256 checksums.

macOS packages introduce a pinned persistent self-signed identity. Updating from ad-hoc releases through 0.9.0 may require microphone/screen permission approval once again; actual permission retention across two OrbitPal releases remains unverified. Self-signing is not Apple notarization or an Apple-verified publisher identity. Windows installers still have no Authenticode signature.

Updater payloads carry a separate Tauri signature, including the signed release version; it does not replace OS code signing. Download from this official release page and compare files with SHA256SUMS. In-app updates require explicit confirmation; Linux in-app installation uses AppImage, while DEB packages are updated manually.

Existing profiles, bot grants, credentials, the installed app ID and updater trust key are retained. CLI providers, local models, Twitch/YouTube credentials, Hindsight and external services require separate configuration; no credentials or model weights are bundled.

## Remaining acceptance

Issues [#17](https://github.com/ralphschuler/orbitpal/issues/17), [#19](https://github.com/ralphschuler/orbitpal/issues/19) and [#26](https://github.com/ralphschuler/orbitpal/issues/26) remain open for operational signing/install acceptance, supported upstream Rust dependency migrations and real GNOME/KDE capture/revocation checks. Automated fixtures do not establish real desktop capture, physical multi-monitor roaming, live OBS/browser-engine/GPU acceptance or installed-update/privacy behavior.

See [CHANGELOG.md](https://github.com/ralphschuler/orbitpal/blob/v0.10.0/CHANGELOG.md), [Autopilot usage and limits](https://github.com/ralphschuler/orbitpal/blob/v0.10.0/docs/AUTOPILOT.md), [format support](https://github.com/ralphschuler/orbitpal/blob/v0.10.0/docs/AVATAR-SUPPORT.md) and the [German guide](https://github.com/ralphschuler/orbitpal/blob/v0.10.0/docs/USER_GUIDE.de.md).
