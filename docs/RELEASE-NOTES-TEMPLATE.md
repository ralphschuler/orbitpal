## New in 0.8.0

- **OrbitPal** is the new name of Streamer AI Cohost. The repository and new downloads use OrbitPal; app data, credentials, bot grants and the update signing identity stay unchanged. Existing clients keep using fixed former download URLs through GitHub redirects.
- Choose the interface language independently of bot instructions, speech and conversation: English, German, French or Spanish.
- Preserve the former MSI UpgradeCode and NSIS install identity during the rename. Existing macOS/AppImage updates can retain the old outer file name. See [migration details](https://github.com/ralphschuler/orbitpal/blob/main/docs/MIGRATING-TO-ORBITPAL.md).

## Fixed in 0.7.1

- Restore macOS microphone capture by signing the required Audio Input entitlement in hardened-runtime packages. Validate the actual signed app in both macOS release jobs.
- Request the native microphone grant only when recording/listening is explicitly started. Show OS status and direct settings recovery; Stop, bot changes and updates cannot revive late permission results or automatically restart capture.
- Existing ad-hoc macOS signing can still require renewed permission after updates; no system permissions or signing requirements are reset or weakened.

## New in 0.7.0

- Enable **Bildschirm als Kontext** for a bot to attach a fresh native screenshot before every normal chat, Autopilot and tool-follow-up dialog-model call. No MCP server or Skill is required. Choose the primary or an explicit monitor under Models or Autopilot; both show the same setting.
- Screen images are processed in memory and sent to the selected provider. Cloud/remote providers receive the pixels; screenshot images are not retained in the app's history or Hindsight. Native capture supports macOS, Windows and Linux X11. Wayland screen sharing is not yet supported and fails with an explicit message.

## Fixed in 0.7.0

- Characters return to their embedded idle animation after Autopilot walking instead of falling into a T-pose. Repeated walks preserve the idle animation's phase and paused-state setting.
- Imported gestures blend back into idle without a bind-pose gap. Cancelling a gesture or previewing the idle clip itself also preserves a natural resting pose.

## Features

- Explicit per-bot Autopilot with quiet periods, rolling speech budgets, roaming, silent gestures and optional native screen context.
- Automatic VRM 0/1 and GLB 2 inspection, actual model-clip previews and embedded walking animations; speech moves mouths/jaws without artificial body wobble.

- Multiple 3D cohosts with independent personas, voices, emotes and OBS outputs.
- Twitch/YouTube, local/cloud models, Codex and Claude Code model/reasoning controls.
- Configurable microphone activation phrases; speech-aligned text and gestures.
- MCP, Skills, character instructions, Hindsight memory and PDF/text knowledge retrieval.
- Automated unit checks, source and license inventories, and contribution rules for people and coding agents.
- Update notifications from GitHub Releases, with explicit confirmation before downloading and installing a newer version.

See [CHANGELOG.md](https://github.com/ralphschuler/orbitpal/blob/main/CHANGELOG.md) and the [German guide](https://github.com/ralphschuler/orbitpal/blob/main/docs/USER_GUIDE.de.md) for details and limitations.

## Downloads

Prebuilt desktop applications for Windows x64, macOS Apple Silicon, macOS Intel, and Ubuntu 24.04 x64 are attached below, together with source code and SHA-256 checksums.

These builds do not use a paid code-signing identity. Windows installers are unsigned. macOS applications use ad-hoc signing and are not notarized by Apple. Your operating system may therefore request an explicit trust decision. Check that you are downloading from this project's official release page and compare the file with `SHA256SUMS`.

Updater payloads carry a separate Tauri signature that the app verifies before installation, including the signed release version. This does not replace Windows Authenticode or Apple notarization. App versions before 0.6.0 need one manual installation to gain the updater. Linux in-app installation is supported for AppImage; Debian packages are updated manually using the release downloads.

CLI providers, local models, Twitch/YouTube credentials, Hindsight, and optional external services must be configured separately. No account credentials or model weights are bundled. The included character files and their attribution/license documents are part of the downloads.
