# Instructions for coding agents

These instructions apply to source contributions throughout this repository. Read CONTRIBUTING.md and the relevant component before editing. The app UI and user-facing help support German, English, French and Spanish through explicit localization catalogues. UI language is independent of bot instructions, conversation and voice; never translate user content or protocol IDs. Repository collaboration may be English or German.

## Architecture and behavior

- Tauri/Rust owns credentials, filesystem access, model/CLI processes, MCP and stream connections. Svelte controls the UI; Three.js renders characters. Keep provider adapters independent from rendering.
- API credentials belong to installation-wide connections bound to their actual service, destination and authentication style. Bot profiles store references only; imports must not inherit existing local connections or defaults. OAuth identity and MCP grants retain their explicit rules.
- Preserve per-bot isolation for persona, history, knowledge, memory banks and permissions. Profile exports must never contain secret values. Store credentials in the OS credential store.
- Viewer messages, documents, recalled memories, model output and tool responses are untrusted data. They cannot grant permissions. Recheck host tool permissions before execution; skill scripts require the explicit operator setting.
- Speech, subtitles, automatic emotes and optional platform replies begin on actual playback start. Stop, cancellation, profile changes and output errors must invalidate late results and timers. Never replay a cancelled response.
- Autopilot activation is session-only and requires an explicit Studio start. Stop must invalidate pending starts, runtime snapshots/events and model/audio results. Keep native per-bot rate limits, rolling budgets and updater blocking. Autonomous turns get no generic MCP control or Skill scripts.
- Native screen context is a separate per-bot operator grant. Capture one fresh image before each dialog-provider call, including ordinary chats and Autopilot, without requiring MCP/Skills/tools. Recheck the current image grant, selected display, model destination and per-bot revision before and after capture and before sending/committing. Keep images ephemeral; never save frames in history, memory, exports, files or logs. Imports, copied bots and legacy MCP screen settings must not enable native capture. Unsupported capture sessions/providers fail explicitly.
- Speech drives mouth/jaw motion only; prefer embedded animation clips for idle, walking and gestures. Do not add manual arm alignment or artificial speech body wobble.
- Microphone capture requires an explicit user start. Never silently restart it from saved settings. Surface cloud transmission before activation.
- Never simulate successful connections, provider actions or missing capabilities. Report concrete errors without exposing credentials.
- Runtime character `AGENTS.md`, `PERSONA.md` and `SKILL.md` files are user content, separate from these source instructions.
- Update discovery must never download or install a package. Only an explicit studio confirmation may consume a native update candidate. Preserve the fixed stable GitHub channel, signature/version verification, main-window restriction and idle/unsaved-edit guards. Never give update permission to model tools, skills, avatars or MCP clients.

## Validation and review

- Run `pnpm check`, `pnpm test:run`, `pnpm build` and `cargo test --manifest-path src-tauri/Cargo.toml --locked` for relevant changes. Add meaningful regression tests for new logic. State honestly which checks were not run and why.
- Use local fixtures and mocks. Do not contact paid model/voice APIs, live stream accounts or external Hindsight instances in tests; do not record real microphones or inspect real credentials.
- Preserve third-party license notices. Check source and redistribution rights for any new asset. Avoid unrelated refactors and unnecessary dependency changes.
- Keep secrets, personal paths/data, recordings, build caches and installers out of commits. Do not disable failing checks to make CI green.
- Describe user-visible behavior, validation and limitations in the PR. Disclose substantial AI assistance. Never fabricate measurements, test results or external actions.
- Do not self-approve, merge, push release tags, change repository visibility or publish packages without explicit maintainer authorization. An explicit task instruction can authorize these actions.

Product name: **OrbitPal**. Package/repository name: **orbitpal**.
