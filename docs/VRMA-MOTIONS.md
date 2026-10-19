# Separate VRMA motions

**Character → Separate motions → Import VRMA** imports a self-contained `.vrma` file for the selected bot. This is separate from importing a character. Choose a VRM 0 or VRM 1 character, then preview a clip silently or add it as an emote. The panel lists its name, original duration and available body, expression and gaze tracks. No pose calibration is required.

OrbitPal uses the official `@pixiv/three-vrm-animation` 3.5.5 loader and `createVRMAnimationClip` retargeter, matching the installed `@pixiv/three-vrm` version. Body tracks target normalized humanoid bones and are transferred to the raw rig without erasing unowned embedded animation properties. Authored expression and look-at tracks take precedence over ambient defaults; live speech keeps its existing mouth priority. Each target is retargeted independently, including the VRM-0 coordinate conversion provided by the library.

The native host accepts the completed [VRMA 1.0 specification](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm_animation-1.0/README.md), not older draft versions. The supported subset is animation-only binary glTF 2, with embedded BIN or base64 buffers, valid humanoid/expression/look-at mappings, float accessors and linear, step or cubic-spline tracks. Sparse/interleaved animation accessors, geometry, images, extra loader extensions and external resources are rejected. Declared accessor min/max values are not trusted as a substitute for real key times.

Limits: 32 files per bot, 16 MiB per file, 64 MiB of motion data per bot, 32 clips per file, 120 seconds per clip, 2,048 nodes, 4,096 accessors/views, 256 channels/samplers per clip and a shared two-million-value processing budget per file. Profile packages retain their shared 64 MiB resource budget, including knowledge, Skills, sounds, voice references and motions. Invalid graph references, cycles, unsupported versions, nonfinite transforms and a nonpositive humanoid rest height fail before storage.

## Playback and storage

VRMA emotes keep their natural duration and speed. The duration field is read-only for these emotes; ordinary embedded-model emotes keep their existing adjustable duration. Emotes can still have an independently assigned sound and viewer permission. Automatic emotes begin at the existing actual presentation start. The renderer preloads motion clips with the avatar, so there is no motion download at speech start. Stop, replacement, asset/bot changes and disposal invalidate the active action or pending load. Removing the runtime emote also ends its animation.

Assets live at `profiles/<bot>/motions/<uuid>.vrma`. Assigned metadata is recomputed from the actual bytes when saving, exporting, importing and serving. The private HTTP route requires the current session and exact bot assignment. Copying a bot copies its motion resources; profile export includes only assigned motions. Import validates and stages motions with the rest of the transactional profile package, before publishing the new bot. Imported files do not grant chat, tool, microphone, memory or screen permissions.

Removing a motion removes its assignment and matching emotes from the profile; save the profile to persist that change. It does not delete a previously imported disk file. Unassigned files are not served or exported. Import failures may leave a valid unassigned file, as with independently imported emote sounds; these files grant no runtime access.

## Compatibility and verification limits

VRMA targets the standardized VRM humanoid. Arbitrary GLB rigs, nonhumanoid characters and the procedural Nova character cannot use it; their embedded animations and existing gestures remain available. A particular target must provide the bones/expressions needed by a motion. Motion files do not add missing mouth shapes or universal embedded audio support.

The original MIT fixtures exercise the real official VRMA loader, real VRM-0 and VRM-1 loaders, retargeting, body/expression/gaze evaluation, natural timing, stream limits, cancellation and native profile round trips. These checks do not claim pixel rendering on every GPU or compatibility with every third-party motion. No paid provider, real microphone, user profile or stream account is involved.

Primary API reference: [three-vrm-animation](https://pixiv.github.io/three-vrm/docs/modules/three-vrm-animation.html).
