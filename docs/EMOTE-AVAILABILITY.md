# Emote availability

OrbitPal resolves an emote against the current bot and character before dispatch. The same host catalogue feeds agent tools, the authenticated Control MCP list and execution, and the desktop capability command. Arbitrary action strings do not create animations.

- Embedded clips need a unique exact name, a positive declared duration, and a channel with valid node/sampler references. Exact names take priority over case-insensitive matches. Ambiguous aliases are rejected. Action identity uses original GLB names, never shortened display metadata.
- App figures support their implemented gestures. VRM procedural gestures require the corresponding real, uniquely mapped humanoid nodes. Generic GLB skeleton names alone do not prove a coherent rig: these figures expose their authored clips and hiding. No manual arm settings are required.
- Assigned VRMA files are read and validated from this bot’s directory within the existing file limits. Body, expression or gaze tracks need an applicable target. VRM 0 expression presets use the official loader’s VRM 1 names, including joy → happy. Gaze needs a real head and an eye-bone or look-expression binding; a proxy alone is insufficient.
- Viewer and generic MCP calls retain their allowChat permission checks. A changed action, sound, grant, bot, avatar or motion assignment invalidates queued payloads. Speech-start acknowledgement revalidates the batch once against the current host catalogue.

The editor offers loaded actions and marks old unavailable entries for correction. At initial OBS/desktop-overlay startup, a bot/avatar/motion-bound host snapshot supplies the editor even before a local preview is mounted. Requests clear stale availability immediately; late responses and errors cannot restore a previous bot’s catalogue. Studio presentation still requires its current loaded renderer. A second check uses the actual loaded renderer, including real track properties, skeleton targets and array/morph indices. Loading failures and stale renderer generations cannot authorize a sound. Both Studio and Overlay/OBS check before sound preparation and again before presentation. Sound starts only after the renderer accepts the actual action; Stop and profile changes invalidate outstanding sequences.

Host metadata does not claim that a GPU rendered a frame. The final renderer gate is required even after host admission, because resource decoding or loading can fail independently. No universal procedural support is inferred for arbitrary GLB rigs. A stale emote is dropped; it never substitutes a different gesture or plays its sound alone.

Regenerate the bundled target metadata after replacing an asset:

```sh
node scripts/character-emote-targets.mjs
```

Native tests compare all sixteen bundled files with this generated catalogue. Local fixtures also cover actual VRM 0/1 targets, assigned and missing VRMA files, expression normalization, gaze targets, duplicate names/IDs, per-bot isolation and stale payloads. Frontend fixtures use the official loaders and a real animation mixer; they do not require live accounts, paid APIs or a physical audio device.
