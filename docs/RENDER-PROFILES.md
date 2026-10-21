# Renderer profiles and measurement limits

The Studio's character and desktop setup controls offer a per-bot render profile.
Choose **Apply render profile** to save it locally and distribute it to that
bot's Studio, desktop and OBS outputs. Existing, imported and copied characters
default to Balanced. The choice is installation-local presentation data, stored
outside profile exports and imports.

| Profile | Frame cadence | Pixel density | Dynamic shadows |
| --- | --- | --- | --- |
| Balanced | Selected 20, 30 or 60 FPS | Device density, capped at 1.75 | Enabled |
| Energy saving | At most 20 FPS | Device density, capped at 1 | Disabled |

These are deterministic rendering settings, not measured power or performance
claims. The Stage applies a changed setting to its existing WebGL context,
without changing character scale or World coordinates. Antialiasing and the
context's power-preference hint remain unchanged. Native Life simulation and
cancellation retain their own clocks and do not rely on successful GPU submissions.

Applying render quality uses a separate Main-only command and revision-checked
per-bot Store setting. It does not mutate the character profile, alter its action
fingerprints, invalidate native output ownership or cancel speech. Renderer
settings events have their own revision checks and update only the rendering
option; they never enter the profile-change handler. The selected profile FPS
still belongs to ordinary profile editing. Other full profile saves retain their
existing conservative cancellation rules.

Ordinary fixed and ambient outputs skip GPU submissions when their DOM surface
is detached, zero-size, hidden, transparent or fully outside the viewport or a
clipping ancestor. Their animation/cancellation loop continues, so a returning
output requires a fresh actual draw before presentation becomes ready. Reduced
quality never fabricates a rendered frame or authorizes audio. World outputs
retain their existing stricter measured surface/geometry and readiness gates,
including the special checked preparation path. DOM exposure checks do not
establish native-window occlusion, GPU completion or visible pixels.

Store fixtures verify unchanged profiles, action fingerprints and native policy
receipts, stale writes, per-bot isolation, restart, copy/import defaults and bounded
schema handling. Detached settings get no model, capture, action or output grant.
The isolated native fixture permits their harmless read while retaining its write
restrictions. The standalone renderer benchmark uses Balanced without IPC.

Local tests exercise the actual Stage RAF/render path with synthetic WebGL/DOM
ports, profile changes, hidden-to-visible return and resource disposal. They
also retain failed World suppression and ordinary recovery. These tests do not
measure hardware CPU/GPU/RSS, display presentation or OBS behavior.

Issue [#66](https://github.com/ralphschuler/orbitpal/issues/66) remains open for
the full documented 1/3/5-character native hardware matrix, 10-minute warmup,
30-minute measurements, real Studio/overlay/OBS and offscreen distinctions, and
20-cycle resource baselines. The separate
[nonshipping Linux benchmark](../scripts/native-render-benchmark/README.md)
must qualify its actual short smoke before a longer pilot. No performance
target or memory threshold is inferred from its source, elapsed time or a
failed bootstrap.
