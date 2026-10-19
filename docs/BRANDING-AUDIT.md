# OrbitPal rename audit

Audit date: 2026-10-19. Product name: **OrbitPal**; repository, package and executable name: **orbitpal**. The license remains MIT.

## Current product surfaces

| Surface | Audited source and result |
| --- | --- |
| App UI and four interface languages | `index.html`, `src/App.svelte`, components and localization catalogs use OrbitPal; generic words such as cohost describe the bot's role. Character names and user-authored content are not product names. |
| Native app and packages | `package.json`, both lockfiles, Rust package/library/main, Tauri product/window metadata, tray, permission text and new release download names use OrbitPal/orbitpal. |
| Repository and contributor entry points | README, contribution/security guidance, GitHub templates, workflows and normal documentation links use `ralphschuler/orbitpal`. The dated audit's issue/PR/workflow navigation now uses the canonical slug without rewriting its historical findings. |
| MCP, Skills and CLI workers | New copied MCP configs use `orbitpal`, the control server advertises `orbitpal-control`, and its display name is OrbitPal Control. Legacy installed display names are upgraded. New temporary CLI workspaces use `orbitpal-cli-<UUID>` and retain private permissions and automatic cleanup. |
| Hindsight | Newly retained turns use the human-readable context `OrbitPal conversation`. Existing memories are not rewritten. The established tag namespace stays unchanged. If an external integration explicitly filters the old **context label**, update it to accept both labels; tag-based filters keep working. |
| Installer and update presentation | New installer/shortcut/app names are OrbitPal. The existing installation and update identities below remain deliberately stable. |

The scan also covered the pending issue integration for decoder, VRMA, model-audio, update, signing, TypeScript, dependency and Wayland changes. No additional former product labels were found there. This is a source audit, not a claim that every pending branch is released.

## Deliberately retained names

| Retained name or family | Where and why |
| --- | --- |
| `studio.streamer-ai.cohost` | Tauri bundle/app-data identity, OS permission identity, installer fixtures and signing guidance. Changing it would create another application identity. |
| `streamer-ai-cohost` keyring service | `storage.rs`; existing credentials must stay accessible without copying or reading them for this audit. |
| `streamer-ai-cohost-profile` | Export package format identifier; existing exported files must remain recognizable. |
| `streamer-ai-cohost.ui-language` | Persisted UI preference key; users keep their chosen language. |
| `cohost://`, `cohost_*`, `cohost-control` | Event/protocol/tool IDs, installed control Skill and MCP grants. Existing clients and per-bot assignments keep their bindings. The MCP bearer realm `streamer-ai-cohost` is also retained as an authentication-protocol namespace. |
| Hindsight tag `streamer-ai-cohost` | Existing memory tags and operators' external filters remain compatible; bank, document and operation IDs do not change. |
| Former repository asset prefix | The updater manifest intentionally keeps `ralphschuler/streamer-ai-cohost/releases/download/...` for installed 0.6/0.7 clients. Discovery and normal navigation use the canonical repository. URL allowlists and historical update fixtures continue to recognize the old path. Keep GitHub's redirect intact. |
| Windows publisher/registry/UpgradeCode and former shortcuts | The legacy `streamer-ai` publisher, exact former keys, executable/shortcut matching and pinned MSI UpgradeCode implement an upgrade of an existing installation. Linux `Provides`/`Conflicts`/`Replaces` likewise retain the former package name. See the [migration details](MIGRATING-TO-ORBITPAL.md). |
| Old outer app/AppImage paths | An in-app update can preserve the old installation filename while replacing its contents with OrbitPal. Existing released asset filenames and version tags are historical artifacts. |
| Copyright and patch provenance | `LICENSE`, generated dependency notices and the version-pinned `public/licenses/LOCAL-PATCHES.txt` source reference retain the original attribution and history. Migration notes explicitly mention the former product name. Third-party/vendor source and character attribution are not rebranded. |

## Verification boundaries

Release fixtures check current package/window/repository metadata, canonical documentation navigation and the existing legacy updater/installer identities. A native CLI fixture checks independent private workspaces and cleanup. The existing local mock Hindsight fixture checks the new context label alongside the unchanged tags and operation ID. These checks use synthetic data and local resources; they do not inspect real credentials, rewrite memories, install updates or certify native OS installation behavior. Consult the PR's exact-head checks for executed results.
