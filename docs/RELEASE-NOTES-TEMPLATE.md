## New in 0.9.0

- Native desktop Autopilot can roam across a selected monitor in two dimensions, entering from either side and pausing for speech or emotes. Stop restores each bot's fixed window size, position and cursor preference. The expanded transparent window passes clicks through; monitor selection grants no screen capture permission.
- Character size updates immediately in Studio, native overlays and connected OBS sources. Narrow runtime previews preserve unrelated form drafts and permissions; Save changes keeps the setting. Captions follow the character, and a separate desktop camera preserves normal Studio/OBS framing.
- Handle delayed window changes, DPI rounding, partial failures and stale callbacks. If a compositor coalesces restoration acknowledgements, safely recreate the old click-through window from its fixed bounds without replaying audio.

## Fixed in 0.9.0

- Validate and stage complete profile packages before applying them so failed imports preserve existing data.
- Bound MCP/CLI output and retain complete UTF-8 server-sent events.
- Commit conversation history, subtitles and gestures on actual audio playback start. Preserve queued voice snapshots, expire abandoned speech jobs and bound cached audio.
- Bind written Twitch/YouTube replies to their original connection and account.

Real multi-monitor/compositor acceptance remains outstanding. Framing uses measured neutral model bounds and root-motion correction; unusually large authored skeletal deformation needs asset-specific acceptance. The automated checks use local fixtures and mocks, not live model, voice or streaming accounts.

## Downloads and updates

Windows x64 NSIS/MSI installers, macOS Apple Silicon and Intel app ZIPs/DMGs, and Ubuntu 24.04 x64 AppImage/DEB packages are attached below. The release also includes signed updater payloads, dependency inventories, source code and SHA-256 checksums.

Windows installers have no Authenticode signature. macOS apps are ad-hoc signed and not notarized by Apple. Updater payloads carry a separate Tauri signature, including the signed release version; that does not replace OS code signing. Download from this official release page and compare files with SHA256SUMS. In-app updates require explicit confirmation; Linux in-app installation uses AppImage, while DEB packages are updated manually.

Existing profiles, bot grants, credentials and the update signing identity are retained. CLI providers, local models, Twitch/YouTube credentials, Hindsight and external services must be configured separately. No credentials or model weights are bundled. Native screen capture supports macOS, Windows and Linux X11; Wayland screen capture is not included in this release.

See [CHANGELOG.md](https://github.com/ralphschuler/orbitpal/blob/v0.9.0/CHANGELOG.md), [Autopilot usage and limits](https://github.com/ralphschuler/orbitpal/blob/v0.9.0/docs/AUTOPILOT.md) and the [German guide](https://github.com/ralphschuler/orbitpal/blob/v0.9.0/docs/USER_GUIDE.de.md).
