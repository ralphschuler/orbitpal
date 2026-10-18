# Streamer AI Cohost

[![CI](https://github.com/ralphschuler/streamer-ai-cohost/actions/workflows/ci.yml/badge.svg)](https://github.com/ralphschuler/streamer-ai-cohost/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ralphschuler/streamer-ai-cohost)](https://github.com/ralphschuler/streamer-ai-cohost/releases)
[![License: MIT](https://img.shields.io/badge/code-MIT-blue.svg)](LICENSE)

Your own animated AI cohost for Twitch and YouTube. A native **Tauri 2 + Rust** desktop app with a **Svelte 5 + Three.js** control room, transparent character windows and OBS browser sources. The interface is currently German.

**Early release:** expect rough edges and review provider settings before going live. Model runtimes, accounts, API keys and voices are supplied by you.

## What it does

- Multiple independent bots with their own persona, voice, model, skills and permissions; show them together and queue their spoken turns.
- 17 included characters, plus VRM/GLB imports, expressions and custom emotes. Audio start synchronizes subtitles, gestures and optional stream-chat replies; compatible rigs animate while speaking. Subtitles expire 15 seconds after speech ends, with ElevenLabs voice tags kept only in the speech text.
- Twitch EventSub and YouTube live chat, manual selection or automatic reply modes, a bounded queue and a global stop.
- Local models through Ollama, LM Studio or compatible APIs; cloud models through OpenAI-compatible/Anthropic APIs; separately installed **Codex** and **Claude Code** CLIs with model and reasoning controls.
- System voices, ElevenLabs and compatible TTS; local Whisper, OpenAI Whisper, ElevenLabs Scribe and compatible STT. Configurable microphone activation phrases.
- MCP over stdio/Streamable HTTP, Agent Skills and editable per-bot `AGENTS.md` / `PERSONA.md`. The host enforces operator/viewer tool permissions.
- Optional **Hindsight** long-term memory with automatic recall and retention. Local PDF/text knowledge retrieval includes source references; recent chat stays local as short-term context.
- Secrets in the OS credential store, portable profiles and per-bot conversation storage.

## Download and start

Get prebuilt packages from [Releases](https://github.com/ralphschuler/streamer-ai-cohost/releases): Windows x64, macOS Apple Silicon/Intel, and Linux x64. Release notes identify the files and their checksums. Initial packages are **unsigned and not notarized**; platform trust prompts may appear. See [release details](docs/RELEASING.md).

1. Open the app and choose a dialog provider under **Modelle**. Local runtimes must be running; cloud/CLI providers need your own login or key.
2. Choose a character and voice under **Charakter & Stimme**.
3. Send a message in **Regiepult**, then configure Twitch/YouTube under **Verbindungen**.
4. Open a transparent character window or add the bot's **OBS-Link** as a browser source. Keep the desktop app running.

The app opens without an AI account. Nova and the included models work without downloading assets. Dialog and voice services require their respective configuration. CLI integrations usually use cloud inference; installing a CLI does not make its model local.

[German user guide](docs/USER_GUIDE.de.md) · [Character sources and licenses](CHARACTERS.md) · [Architecture](docs/ARCHITECTURE.md)

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

`pnpm tauri build` creates native packages on the current platform. A browser-only preview has no native integrations. CI runs type checks, unit tests and builds; tagged releases build platform installers after validation. See [CONTRIBUTING.md](CONTRIBUTING.md) and [release process](docs/RELEASING.md).

## Data and current limits

Cloud dialog providers receive the conversation context, relevant document excerpts and recalled memories. Cloud STT receives microphone segments, including segments before a wake phrase is detected. Hindsight may itself use cloud services even when its HTTP endpoint is local. All are independently configurable; see the [guide](docs/USER_GUIDE.de.md).

Knowledge retrieval uses bounded local keyword search, not embeddings or model training. Image-only PDFs require prior OCR. Speech mouth motion is an audio/word-boundary approximation, not phoneme-accurate lip sync. Speaking turns are serialized; multiple characters can remain visible. Linux overlay behavior depends on the compositor. OAuth token creation/refresh and model downloads are external to this app.

## Contributing and licensing

Contributions from people and AI-assisted workflows are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) and the [security policy](SECURITY.md).

Application code is [MIT licensed](LICENSE). Bundled character assets and fonts keep their original licenses. In particular, Fox includes **CC BY 4.0** material requiring attribution. Keep [asset attribution](public/characters/ATTRIBUTION.md) and embedded notices when redistributing. This project is independent and is not endorsed by its integration providers or asset creators.
