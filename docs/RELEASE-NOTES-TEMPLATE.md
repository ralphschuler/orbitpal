## Features

- Explicit per-bot Autopilot with quiet periods, rolling speech budgets, roaming, silent gestures and optional fixed MCP screen observations.
- Automatic VRM 0/1 and GLB 2 inspection, actual model-clip previews and embedded walking animations; speech moves mouths/jaws without artificial body wobble.

- Multiple 3D cohosts with independent personas, voices, emotes and OBS outputs.
- Twitch/YouTube, local/cloud models, Codex and Claude Code model/reasoning controls.
- Configurable microphone activation phrases; speech-aligned text and gestures.
- MCP, Skills, character instructions, Hindsight memory and PDF/text knowledge retrieval.
- Automated unit checks, source and license inventories, and contribution rules for people and coding agents.
- Update notifications from GitHub Releases, with explicit confirmation before downloading and installing a newer version.

See [CHANGELOG.md](https://github.com/ralphschuler/streamer-ai-cohost/blob/main/CHANGELOG.md) and the [German guide](https://github.com/ralphschuler/streamer-ai-cohost/blob/main/docs/USER_GUIDE.de.md) for details and limitations.

## Downloads

Prebuilt desktop applications for Windows x64, macOS Apple Silicon, macOS Intel, and Ubuntu 24.04 x64 are attached below, together with source code and SHA-256 checksums.

These builds do not use a paid code-signing identity. Windows installers are unsigned. macOS applications use ad-hoc signing and are not notarized by Apple. Your operating system may therefore request an explicit trust decision. Check that you are downloading from this project's official release page and compare the file with `SHA256SUMS`.

Updater payloads carry a separate Tauri signature that the app verifies before installation, including the signed release version. This does not replace Windows Authenticode or Apple notarization. App versions before 0.6.0 need one manual installation to gain the updater. Linux in-app installation is supported for AppImage; Debian packages are updated manually using the release downloads.

CLI providers, local models, Twitch/YouTube credentials, Hindsight, and optional external services must be configured separately. No account credentials or model weights are bundled. The included character files and their attribution/license documents are part of the downloads.
