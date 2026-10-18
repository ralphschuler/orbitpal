# Changelog

## 0.5.0 — 2026-10-19

- Preserve explicitly started wake-phrase listening across output pauses and microphone-start races. Retry transient STT/recording errors with bounded backoff and report permanent microphone failures clearly.
- Share installed MCP definitions and Skill packages across bots, with independent bot/tool/viewer grants and a separate per-bot skill-script opt-in. Keep `AGENTS.md`, `PERSONA.md`, knowledge and history separate.
- Load skill metadata first and full instructions/resources on demand. Migrate previous installations with content deduplication and collision-safe IDs; preserve legacy skill folders. Export only the profile's assigned extensions and resources.
- Include a local authenticated control MCP server and `cohost-control` Skill, installed explicitly into the shared library without bot grants or script execution. Offer public status/bot/emote lists, allowed emote dispatch, idle-only native show/hide, a pause request and global stop. External clients can copy JSON/TOML configuration with a separate token that rotates on restart; no new dialog or arbitrary configuration/filesystem operations are exposed.
- Schedule internal control-MCP emotes only for the current bot at actual presentation start; preserve immediate dispatch for external control clients and require the emote's chat grant in both cases.
- Add searchable extension management, explicit global edit/delete actions, keyboard-focus/hover information buttons and shortcut help.
- Add keyboard actions: `?`/F1 for help, Ctrl/Cmd+S to save, Ctrl/Cmd+K for the composer, Ctrl/Cmd+Enter to send in the studio, Alt/Option+1–7 for sections, and Esc to dismiss visible help/dialogs before stopping output.
- Add optional emote sounds with independent volume: local wave/cheer/whoosh synthesis and bot-scoped WAV/MP3/OGG/WebM/M4A imports. Prepare audio before speech starts; support studio, native overlays and OBS, with bounded playback and cancellable loading.
- Forward validated MCP image results as real image inputs through OpenAI-compatible providers, suitable Ollama/LM Studio models, Anthropic, Codex App Server and Claude Code Stream-JSON. Add per-bot image forwarding and explicit errors for disabled or known unsupported vision inputs. Tool permissions remain unchanged.
- Bound visual context to four images, 5 MiB each and 12 MiB per turn; Claude Code also enforces a 10 MiB encoded-input envelope. Do not fetch arbitrary image URLs or local paths.

Automated validation uses local fixtures and mocks; these changes do not imply successful live model, speech, streaming-account or MCP-server validation. Existing unsigned/ad-hoc package limitations and the documented Linux GLib security backport remain in effect.

## 0.4.0 — 2026-10-18

First public desktop release.

- Tauri control room, multiple bot profiles, transparent avatar windows and per-bot OBS sources.
- Seventeen bundled characters, VRM/GLB import, customizable emotes and speech-driven animation.
- Twitch/YouTube chat, API/local/CLI dialog providers, configurable Codex/Claude models and reasoning effort.
- Local/cloud speech recognition, configurable activation phrases and synchronized audio/text/emote presentation.
- Separate display/ElevenLabs speech text; voice tags stay out of subtitles, which expire 15 seconds after playback ends.
- MCP permissions, Agent Skills and editable character instructions.
- Opt-in Hindsight recall/retention; local PDF/text knowledge retrieval with source metadata.
- Automated checks, platform installers, SHA-256 checksums and contributor/security policies.

Packages are unsigned on Windows and ad-hoc signed without notarization on macOS. External services and accounts are configured separately. See the user guide for provider, document and platform limitations.
