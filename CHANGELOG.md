# Changelog

## 0.9.0 — 2026-10-19

- Expand native character windows to a selected monitor while Autopilot wandering is active. Use deterministic two-dimensional paths, pause for speech and emotes, and restore each bot's fixed physical window bounds and cursor preference when stopped. Keep the monitor-sized window transparent and click-through; the output-monitor choice grants no screen capture permission.
- Preview character size immediately in Studio, native overlays and connected OBS sources without saving unrelated profile drafts or permissions. Include revisioned presentation state in initial snapshots and live events. Keep the normal Studio/OBS perspective cameras and frame native desktop characters with a separate orthographic camera, measured model bounds, a turning envelope and figure-following captions.
- Handle delayed/coalesced window requests, partial setter failures, negative monitor origins, DPI rounding and stale callbacks. Preserve saved bounds through initialization and, when restoration cannot be confirmed, safely replace the old click-through handle without replaying speech. Real multi-monitor/compositor acceptance remains outstanding; unusual authored skeletal motion can exceed neutral asset bounds.
- Make profile package imports transactional: validate and stage the entire package before applying it, preserving existing data when an import fails.
- Bound MCP/CLI output and retain complete UTF-8 server-sent events instead of splitting multibyte text across chunks.
- Commit assistant history and presentation only when audio playback actually starts. Retain queued voice snapshots, expire abandoned speech jobs, cancel late results and bound cached audio.
- Bind written Twitch/YouTube replies to their original connection and account so delayed responses cannot be posted through a replacement connection.

## 0.8.0 — 2026-10-19

- Add English, German, French and Spanish interfaces with a saved app-wide selection, reactive labels/help, localized native tray menus and number/date formatting. Keep conversation content, bot instructions, voice/STT choices and extension permissions independent of the interface language.
- Import glTF 2.0 JSON packages with local relative BIN and PNG/JPEG resources or matching Base64 data. Bundle them into a validated portable GLB while preserving scenes, rigs, morph targets and authored animations. Bound resources and reject missing files, external URLs, folder escapes, redirected files and unsupported compressed buffers.
- Document curated avatar creators/libraries, export paths and per-model licensing; include the macOS microphone fix described below.

- Rename the product, repository, executable and new release artifacts to OrbitPal while retaining installed data, bundle/keychain IDs, profile formats and bot grants. Preserve old clients with fixed legacy initial update URLs, keep signed-version verification, pin Windows installer identity, and validate installer migration on disposable Windows CI runners.

- Fix macOS microphone capture in hardened-runtime release packages by signing the required Audio Input entitlement alongside the usage description. Verify the entitlement in each actual signed macOS release bundle.
- Check/request the native macOS microphone grant only on an explicit recording/listening start. Expose the real OS status and a microphone-settings link, distinguish WebView denial from native authorization, and preserve Stop/bot-change cancellation without silent recording restarts or permission resets.

## 0.7.0 — 2026-10-19

- Add native, per-bot screen context shared by normal chats and Autopilot. Attach one fresh in-memory screenshot immediately before each dialog-provider call, including tool follow-ups, without MCP servers, Skills or tool permissions. Share the existing image adapters with API, local, Codex and Claude Code providers.
- Add primary/explicit monitor selection, image-permission and cloud-transfer information, bounded JPEG encoding, capture cancellation and per-bot grant revocation. Reject unavailable monitors or permissions explicitly; do not retain images in history or memory. Imported/copied bots and old MCP screen settings keep native capture disabled.
- Support native macOS, Windows and Linux X11 capture; report unsupported Wayland sessions without falling back to incomplete XWayland images or filesystem screenshot portals.

- Restore embedded idle animations after Autopilot walking fades out. A completed fade disables the idle action; explicitly reactivate it without resetting its playback phase so characters do not fall back to their bind/T-pose. Preserve paused idle poses and active emote priority.
- Blend imported emotes back into idle before they finish, restore idle immediately on cancellation, and avoid fading idle against itself during its explicit preview. Remove brief bind-pose gaps at the start/end of these transitions.

## 0.6.0 — 2026-10-19

- Add an explicitly started per-bot Autopilot with independent roaming, silent gestures and spontaneous speech, randomized quiet periods, fair serialized turns and native rolling hourly limits. Stop cancels all sessions; saved/imported profiles cannot start them.
- Optionally capture one operator-selected MCP observation with fixed arguments before an autonomous turn, forwarding real images through the existing provider adapters. Keep general MCP actions and Skill scripts unavailable, support silent decisions and suppress platform-chat posts.
- Inspect VRM 0/1 and GLB 2 containers and declared capabilities before import or legacy asset delivery, report unsupported required features, and expose real model clips for preview and emote assignment. Preserve authored body/face clips alongside VRM physics and speech mouth movement.
- Fix production OBS asset routing so JavaScript/CSS/fonts are served separately from private avatar imports.

- Add app-wide automatic stable-release checks at startup and every six hours, an opt-out setting, manual checking, release notes and a confirmation dialog. Choosing later never downloads a package.
- Download/install only after explicit confirmation, with progress, signature and signed-version verification, native candidate expiry, fixed GitHub release URLs and rejection of equal/older/prerelease versions. Block installation during ongoing work, microphone capture or unsaved edits; restart only after successful installation.
- Publish updater archives/signatures and a complete `latest.json` after all four platform builds pass. Preserve Windows MSI/NSIS installer type; support Linux in-app installation through AppImages and explain manual Debian updates.
- Document update operations, signing-key handling and contributor invariants. Updater signatures are separate from OS code signing; Windows Authenticode and Apple notarization remain unavailable.
- Reset imported credential references to fresh unbound identifiers, clear imported CLI executable overrides and disable imported automatic/platform replies. Imported profiles require explicit credential setup; existing bots and operator-requested bot copies keep their own bindings.
- Cancel late microphone permission results after Stop, bot/STT changes or update confirmation; prevent duplicate pending captures. Cancel prepared gestures/sounds before an update starts.
- Save profile instructions before exporting, and abort export if saving fails.
- Remove the unused direct ZIP dependency, eliminating its three reported RustSec advisories without suppressing the scanner.
- Disable the updater's unused Tauri-v1 ZIP compatibility feature as well, retaining TLS, system proxies and the signed v2 package formats without the transitive ZIP dependency.
- Derive VRM rest/gesture axes from the actual normalized skeleton instead of fixed VRM-version assumptions. Correct raised resting arms and downward waves in all ten bundled legacy OSA models; preserve modern rig orientation and cap strong gestures before arm overrotation.
- Show one book and one wand variant on the KayKit Mage; hide the simultaneously visible alternate book and staff without changing the original asset or animation clips.
- Merge the reviewed base64/rusqlite/Three.js/development-tool updates, retain supported TypeScript 6 checking, and add dependency advisory CI, branch protection and a documented audit backlog.

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
