# OrbitPal

A desktop AI cohost for Twitch and YouTube, formerly **Streamer AI Cohost**. Existing profiles and credentials stay in place when updating. See [rename and update compatibility](docs/MIGRATING-TO-ORBITPAL.md).

[![CI](https://github.com/ralphschuler/orbitpal/actions/workflows/ci.yml/badge.svg)](https://github.com/ralphschuler/orbitpal/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ralphschuler/orbitpal)](https://github.com/ralphschuler/orbitpal/releases)
[![License: MIT](https://img.shields.io/badge/code-MIT-blue.svg)](LICENSE)

Your own animated AI cohost for Twitch and YouTube. A native **Tauri 2 + Rust** desktop app with a **Svelte 5 + Three.js** control room, transparent character windows and OBS browser sources. The interface supports English, German, French and Spanish; its language is independent of each bot’s conversation and voice.

**Early release:** expect rough edges and review provider settings before going live. Model runtimes, accounts, API keys and voices are supplied by you.

The current source version is **0.8.0** (unreleased). Consult the release notes for the version included in each published package.

## What it does

- Multiple independent bots with their own persona, voice, model, skills and permissions; show them together and queue their spoken turns.
- 17 included characters, plus VRM/GLB/glTF imports, expressions and custom emotes with optional sounds and volume. Audio start synchronizes subtitles, gestures and optional stream-chat replies; compatible rigs animate while speaking. Subtitles expire 15 seconds after speech ends, with ElevenLabs voice tags kept only in the speech text.
- Twitch EventSub and YouTube live chat, manual selection or automatic reply modes, a bounded queue and a global stop.
- Local models through Ollama, LM Studio or compatible APIs; cloud models through OpenAI-compatible/Anthropic APIs; separately installed **Codex** and **Claude Code** CLIs with model and reasoning controls.
- System voices, ElevenLabs and compatible TTS; local Whisper, OpenAI Whisper, ElevenLabs Scribe and compatible STT. Configurable microphone activation phrases.
- A shared MCP/Skill library: install once, grant each bot its own access. Skills load their full instructions and resources on demand; `AGENTS.md` / `PERSONA.md` remain separate for each bot. The host enforces operator/viewer tool permissions.
- An included control MCP server and `cohost-control` Skill expose public bot status, allowed emotes, character output, pause requests and global stop. Install them under **Skills & MCP**, then grant access explicitly; external agents can copy a local HTTP configuration.
- Real image inputs from permitted MCP tools through compatible APIs, Anthropic, Codex and Claude Code. Each bot controls image forwarding; the selected model must support vision.
- Optional **Hindsight** long-term memory with automatic recall and retention. Local PDF/text knowledge retrieval includes source references; recent chat stays local as short-term context.
- Secrets in the OS credential store, portable profiles and per-bot conversation storage.
- Optional automatic update checks through GitHub Releases. Download, signature verification, installation and restart require your explicit confirmation in **Updates & Version**.

Optional [Autopilot](docs/AUTOPILOT.md) gives each bot session-only roaming, silent gestures and occasional spontaneous speech, with quiet periods and a rolling hourly limit. Built-in [screen context](docs/SCREEN-CONTEXT.md) can attach a fresh native screenshot before every dialog-model call, including normal chats and Autopilot, without MCP or Skills. Autonomous turns receive no general MCP control or Skill-script execution.

## Download and start

Get published packages from the [latest release](https://github.com/ralphschuler/orbitpal/releases/latest). The release pipeline targets Windows x64, macOS Apple Silicon/Intel, and Linux x64; each release identifies its available files and checksums. Windows packages are **unsigned**; macOS packages use **ad-hoc signing without notarization**. Platform trust prompts may appear. See [release details](docs/RELEASING.md).

1. Open the app and choose a dialog provider under **Models**. Local runtimes must be running; cloud/CLI providers need your own login or key.
2. Choose a character and voice under **Character & voice**.
3. Send a message in **Control room**, then configure chat under **Twitch & YouTube**.
4. Open a transparent character window or add the bot's **OBS link** as a browser source. Keep the desktop app running.

The app opens without an AI account. Nova and the included models work without downloading assets. Dialog and voice services require their respective configuration. CLI integrations usually use cloud inference; installing a CLI does not make its model local.

Press **?** or **F1** for keyboard help. **Ctrl/Cmd+S** saves, **Ctrl/Cmd+K** focuses the composer, **Ctrl/Cmd+Enter** sends from the studio, and **Alt/Option+1–8** switches sections. **Esc** dismisses visible help or a dialog first; otherwise it stops output. Inline information buttons explain settings on hover or keyboard focus.

From version 0.6.0, **Updates & Version** checks the stable GitHub release channel shortly after startup and every six hours. You may disable automatic checks, check manually or choose **Later**. The app downloads and installs only after **Confirm & install**; save edits and stop ongoing output/microphone use first. Linux in-app installation requires an AppImage; Debian packages update manually. Earlier versions require one manual upgrade. See [updates](docs/UPDATING.md).

[German user guide](docs/USER_GUIDE.de.md) · [Character sources and licenses](CHARACTERS.md) · [Create or find characters](docs/CHARACTER-SOURCES.md) · [Model format support](docs/AVATAR-SUPPORT.md) · [Interface languages](docs/LOCALIZATION.md) · [Architecture](docs/ARCHITECTURE.md) · [Audit and follow-up issues](docs/AUDIT-2026-10-19.md)

## Development

Node.js 22.12+, the pnpm version pinned in `package.json`, stable Rust and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) are required.

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm check
pnpm test:run
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --locked
pnpm tauri dev
```

`pnpm tauri build` creates native packages on the current platform with the release signing key configured as described in the [release process](docs/RELEASING.md). For an unsigned local development package, use `pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'`; these packages are not project releases. A browser-only preview has no native integrations. CI runs type checks, unit tests and builds; tagged releases build platform installers after validation. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Data and current limits

Cloud dialog providers receive the conversation context, relevant document excerpts and recalled memories. Cloud STT receives microphone segments, including segments before a wake phrase is detected. Hindsight may itself use cloud services even when its HTTP endpoint is local. All are independently configurable; see the [guide](docs/USER_GUIDE.de.md).

With **Send images to the model** enabled, permitted MCP image results also reach the selected dialog provider. Images are bounded to four per turn, 5 MiB each and 12 MiB total; Claude Code additionally limits its encoded input envelope to 10 MiB. The app does not fetch arbitrary image URLs or local paths. Image content and tool results are untrusted and grant no new tool access.

External MCP, CLI and Skill process output is bounded while reading, including incomplete lines, chunked HTTP bodies and SSE streams. See the [transport budgets and cancellation behavior](docs/MCP-TRANSPORT-LIMITS.md).

Knowledge retrieval uses bounded local keyword search, not embeddings or model training. Image-only PDFs require prior OCR. Speech mouth motion is an audio/word-boundary approximation, not phoneme-accurate lip sync. Speaking turns are serialized; multiple characters can remain visible. Linux overlay behavior depends on the compositor. OAuth token creation/refresh and model downloads are external to this app.

The Linux GTK stack uses a documented [GLib security backport](vendor/glib/COHOST-BACKPORT.md), with an optimized iterator regression in Linux CI/release builds. See [SECURITY.md](SECURITY.md) for its scope and reporting policy. Automated checks use local fixtures and mocks; live model, speech and streaming-provider compatibility is not implied.

The control MCP endpoint requires its own short-lived Bearer token, distinct from OBS links. Copied external-client configurations expire when the app restarts. It provides no new dialog requests, arbitrary file/settings access or permission changes. Showing/hiding characters requires an idle dialog runtime and no pending speech for the target bot; emote calls require that emote's `allowChat` flag. Internal bot turns can schedule only their own emotes for actual presentation start; external control clients dispatch emotes immediately.

## Contributing and licensing

Contributions from people and AI-assisted workflows are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) and the [security policy](SECURITY.md).

Application code is [MIT licensed](LICENSE). Bundled character assets and fonts keep their original licenses. In particular, Fox includes **CC BY 4.0** material requiring attribution. Keep [asset attribution](public/characters/ATTRIBUTION.md) and embedded notices when redistributing. This project is independent and is not endorsed by its integration providers or asset creators.
