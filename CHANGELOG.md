# Changelog

## Unreleased

- Avoid unused macOS Keychain reads for Codex/Claude Code dialog and model-list operations and browser/system speech, while preserving configured HTTP authentication. Explain per-item persistent authorization beside credential fields in all four interface languages.

- Distribute automatic chat turns fairly across eligible bots within each priority class, even while the queue keeps filling. Preserve direct-address and manual priority, reserve asynchronous preflight, and invalidate exact claims on Stop without replaying late work.
- Name the Autopilot hourly limit as contribution attempts in all four interface languages. Explain text-only, silent and failed attempts and distinguish the per-bot rolling limit from provider requests or monetary limits; preserve the existing counter and stored configuration.

## 0.11.1 — 2026-10-20

- Fix desktop Autopilot facing for vertical and diagonal paths with an elevated orthographic view and upright turns. Keep the full projected turning hull inside safe bounds and retain offscreen excursions, visible returns and live figure sizing.
- Drive embedded and procedural walking from actual traveled distance instead of session age. Calibrate imported walk strides from bounded root/foot sampling, account for live scale and projected depth, preserve phase across pauses, and synchronize embedded walking sound rate. Unknown clips use a body-height approximation rather than promising exact foot locking.
- Report empty, whitespace-only and emotion-only Autopilot responses as invalid output. Only an explicit silence signal is a successful quiet turn; preserve deliberate silence, existing intervals and hourly budgets without forcing extra attempts.

## 0.11.0 — 2026-10-20

- Redesign the Studio with consistent dark surfaces, readable typography and controls, one header save/status area and persistent selected-bot context. Move the composer directly below the character and keep the crew in a compact scrolling roster. Preserve full text navigation at smaller widths.
- Separate shared Skill/MCP installations from per-bot grants more clearly. Present Autopilot, screen permissions and updates with consistent sections and expandable technical details while keeping grants, costs, errors and pending actions visible. Retain all four interface languages independently of bot content.
- Confirm before discarding changed library drafts or deleting knowledge/history. Preserve unchanged drafts without unnecessary prompts, retain shared drafts across bot switches and keep errors/drafts when a save fails. Hold the same editor through compound profile/document/library saves so delayed completion cannot save a replacement draft.
- Fix Escape ordering for the bot picker, tooltips, keyboard help and editors. Keep global save/send/navigation shortcuts behind confirmations, preserve IME composition, restore focus after local discard and associate all edited form labels explicitly. Keep the narrow-window bot menu above the sticky header and prevent crew labels from collapsing into vertical text.
- Preserve installed identities, provider/permission behavior, updater trust and the persistent self-signed macOS certificate. Local UI validation covers 96 page views across four languages and three sizes, plus keyboard/dialog and transparent-overlay fixtures; these browser fixtures do not establish native installation, privacy retention or live provider acceptance. Existing signing/install, upstream maintenance and Wayland acceptance work remains tracked in #17, #19 and #26.

## 0.10.1 — 2026-10-20

- Replace the monolithic keyring dependency with supported explicit native stores while preserving the legacy service, credential IDs, macOS User Keychain, Windows credential encoding/target and Linux persistent Secret Service `target=default` attributes. Construct stores directly without a process-global default. Return fixed, localized credential error categories without backend payloads or malformed secret bytes.
- Save current bot/model settings before Wayland source selection and reject stale preparation/results after a bot, draft or panel change. Close a discarded result by its exact bot/session lease so cleanup cannot disconnect a newer selection or another bot. Surface cleanup failures even after leaving the panel. The new source display ID still requires an explicit save.
- Add a disposable three-OS old/new credential compatibility fixture. Keep the separate real-desktop ScreenCast/PipeWire prototype in draft PR #56 while its KDE test-compositor limitation is unresolved. Wayland remains experimental pending the full acceptance matrix in #26; a successful GNOME prototype does not establish KDE or full application acceptance.
- Remove three old optional keyring/zbus maintenance-warning package versions from the production lockfile through the supported credential-store migration. Five production maintenance warnings and the separately audited legacy fixture warnings remain documented in #19; no advisory exclusions are introduced.
- Record v0.10.0 production signing/provenance evidence and activate future release asset immutability plus version-tag update/deletion protection. Preserve the persistent self-signed macOS identity, updater key and installed compatibility identities. Native install/privacy acceptance and protected signing custody remain open in #17.

## 0.10.0 — 2026-10-19

- Show the Autopilot's actual speech blocker, next-attempt countdown and silent attempts. Allow short persona-based conversation starters without inventing screen, game or viewer events. Keep exact per-attempt claims across permission checks so pausing/restarting cannot revive an old request. Automatic walking and gesture sounds no longer restart autonomous quiet periods; actual audio still blocks microphone activation, ordinary chat playback and update installation.
- Let desktop characters leave the visible area briefly and return naturally. Prepared speech, text-only replies and explicit emotes wait until the whole character has returned, stopped and rendered successfully. Stop, profile changes and output closure cancel waiting presentations.
- Guide explicit microphone and screen-context starts through permission status, a system request, Settings and recheck actions. Returning from Settings never starts recording or replays an automatic conversation. Preserve separate per-bot image grants and current-request cancellation.
- Add explicit per-bot Wayland ScreenCast/PipeWire source selection with fresh bounded in-memory frames, disconnect/reconnect and session cleanup. This backend remains experimental pending real GNOME/KDE capture and revocation acceptance (#26); no XWayland or screenshot-file fallback is used.
- Import separate VRMA 1.0 motions per bot, retarget them to supported VRM 0/1 characters and include assigned motions in profile packages. Preserve body, expression, gaze and natural duration; arbitrary GLB/nonhumanoid targets are not supported by this motion workflow.
- Bundle offline Draco, Meshopt and BasisU/KTX2 decoders for compressed avatars, with resource limits and existing import protections. Use the same renderer-aware loader in Studio, native outputs and OBS.
- Offer and play only actions supported by the current character. Validate model/motion bindings before preparing and presenting emotes or sounds, and mark unavailable editor entries explicitly.
- Add a bounded, explicitly versioned compatibility profile for the draft MSFT_audio_emitter extension. Embedded sounds follow actually rendered animation events, explicit audio activation, output routing, volume and Stop; model loading and idle clips remain silent. This is not universal glTF/VRM audio support.
- Allow cancelling update download and pre-install verification, discard late callbacks and require a fresh check/confirmation for retry. Installation remains non-cancellable once replacement starts. Add OBS security headers, cryptographic updater-payload verification before publication and build-provenance verification.
- Require a pinned persistent self-signed macOS identity while retaining the installed app ID and separate updater key. The first transition from ad-hoc releases through 0.9.0 may require permission approval again. Self-signing provides no Apple notarization or guaranteed TCC retention; Windows installers still have no Authenticode signature. Operational signing/install acceptance remains tracked in #17.
- Complete current OrbitPal branding while preserving compatibility names, and add native TypeScript 7 checking alongside TypeScript 6 editor compatibility. Record the remaining upstream Rust maintenance migrations without suppressing advisories (#19).

Automated checks use local fixtures and mocks. Real GNOME/KDE capture, installed-update/privacy behavior, physical multi-monitor roaming and live OBS/browser-engine/GPU acceptance are not established by those checks. Issues #17, #19 and #26 remain open.

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
