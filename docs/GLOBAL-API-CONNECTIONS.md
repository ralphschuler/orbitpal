# Global API connections

Issue [#94](https://github.com/ralphschuler/orbitpal/issues/94) makes API accounts installation-wide. In **API connections**, create a named connection, save its key, and optionally choose it as the default for its service and destination. Assign a connection or its explicit default alias in the character's integration settings. Choosing no connection sends no stored API key; it never silently selects a default.

OpenAI dialog and transcription can share the same connection. ElevenLabs speech and transcription can share another. Anthropic, Hindsight, compatible HTTP endpoints and the YouTube Data API have their own destination-bound connections. Custom hosts, paths, non-default ports and authentication header styles form separate scopes. A local HTTP server can still use an explicitly assigned key. Codex/Claude CLI and system voice do not look up an unused HTTP key.

A character retains its own model, voice ID, instructions, conversation, knowledge, Hindsight bank and permissions. Twitch/YouTube OAuth identity, platform reply permissions, MCP credentials and grants retain their explicit existing rules. A shared YouTube Data API key does not authorize replies on someone else's account.

## Existing installations

The first migration registers existing credential references without reading, copying or replacing any secret. Keys remain in the existing OS credential namespace. Equal account/destination references deduplicate; different accounts remain separate, with their original character selections. An unambiguous account may become the destination default; conflicting accounts leave that default unset.

Malformed, oversized or invalid-UTF-8 legacy profile rows remain byte-for-byte intact and count as unbound. They do not supply a guessed destination. Registry corruption, unsupported schema or migration capacity exhaustion makes the optional manager unavailable; it is not reset to an empty healthy registry. Migration uses one SQLite transaction, so a failure cannot leave partly rewritten references.

If an old OS item is shared across different scopes or with MCP/OAuth references, its adopted connections are read-only in this manager. Uncertain legacy metadata ownership is conservative too. Create an independent connection with its own key instead of rotating an unrelated integration's credential. The public metadata reports sharing without exposing the OS item ID or secret.

Removing a connection removes its manager metadata and clears default targets; the OS item is retained. An existing character's removed reference remains visibly unavailable. It cannot fall back to a different account. This also preserves unrelated MCP/OAuth references to that item.

## New, copied and imported characters

An explicitly new character may use a matching configured global default. Copying preserves the source selection, including an unassigned or unavailable selection, and creates a separate Hindsight bank. It never upgrades a blank or invalid selection to a default.

Exports contain no managed API connection/default references and no global registry. Imports clear those references even if a package names a real local account or default alias. Importing never grants local account access or enables capture/MCP/OAuth permissions. The operator must bind an account after import.

## Native resolution and mutation

Before calling the OS credential backend, Rust resolves the requested purpose, actual normalized destination and authentication style against the selected connection. It checks that the underlying item is registered. After a potentially blocking OS read it checks registry revision, selection and item registration again; a removed account or changed default rejects the late result. A key is never redirected to a different destination as a fallback. Actual request builders consume the same normalized endpoint rules, including full transcription endpoint suffixes and Hindsight path prefixes.

The manager's metadata reads never query the OS credential backend. `registered` means an item reference exists locally; it does not claim the remote provider accepted the key. Secret writes use the native backend and invalidate existing in-process credential copies through the existing session broker. Metadata/key changes cancel known pending dialog, native autonomous and queued speech work. A request already transmitted cannot be recalled. An existing YouTube polling connection keeps its selected key until it is stopped or reconnected; changing/removing an account or its default does not hot-swap or stop that connection. Disconnect it to stop future polls, and explicitly reconnect it to resolve the current selection.

Manager commands are restricted to the Studio window and excluded from Control MCP/model tools. Metadata/default changes use a bounded schema and revision compare-and-swap. A secret write and SQLite metadata update cannot be one cross-system transaction: if a concurrent external database edit prevents the final revision update, the command reports failure without retargeting the key; the OS write may already have happened. The UI retains the created connection after a later write failure, so retry writes to that same account instead of creating duplicates.

## Validation scope

The focused pure/registry fixture imports the production normalizer and registry module with a minimal outer Store whose OS read/write functions panic. Its 16 scenarios cover metadata-only migration/round-trip, scope checks before and after an injected read, defaults/conflicts, shared-item protection, corrupt/invalid rows and atomic capacity rollback for 820 profiles / 4,100 references. This is not a substitute for production Store/Tauri or HTTP adapter tests. Their results are recorded separately when executed.

The separate 21-scenario STT fixture also imports the actual multipart adapter: injected secret resolution followed by loopback HTTP checks the normalized path and Bearer/ElevenLabs headers. Real local Whisper backends, full endpoint suffixes, and a redirect sink that receives no audio/key are covered. All 21 passed without OS access or paid APIs. Production Store/Tauri and the authored Hindsight/model/YouTube adapter regressions still require the combined native gate.
