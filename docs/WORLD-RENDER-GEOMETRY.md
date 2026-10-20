# Submitted-frame World geometry

`world-render-geometry.ts` observes current CPU geometry through submitted draw
callbacks. The Studio World Avatar adapter invokes it around the actual renderer
call; the result supplies a footprint and ground mapping, never native action
authority. The [native output protocol](WORLD-OUTPUT.md) independently validates
registration, frame freshness, action receipts and durable lifecycle changes.

The local helper retains an exact default policy. The World adapter explicitly
selects `conservative_submitted_geometry_v1`, which permits current rigid-source
enclosures. These policies have different precision contracts; neither proves
visible pixels or arrival on its own. Issue #72 remains open for the remaining
output and integration acceptance work described below.

## Submitted draw and successful enclosing render

AvatarStage establishes the mixer/VRM/jaw/morph/gait/root-compensated pose before
rendering. Its desktop caption callback uses a neutral framing hull and runs
before the render; it is not a successful World-frame acknowledgement. World
observations instead come from the synchronous submitted-draw path, preserving
the existing actual-audio-start presentation rules.

One `WorldRenderGeometry` instance belongs to one loaded actor attachment. It
binds the actor/source-world/render-session/binding/world-instance/stage and
config/topology/layout/asset/measurement/reference revisions, actual root/camera,
ground frame and optional frozen local reference. Those public correlations do
not grant native authority. Replacement or disposal invalidates the old probe.

`renderFrame` receives the exact frame sequence, renderer/scene, fresh mounted
surface and current-identity callbacks, and an explicit monotonic clock. It
installs temporary `onAfterRender` hooks on a bounded actor subtree, invokes one
actual synchronous render and restores hooks in `finally`. Nonstandard figure or
scene hooks are not silently overridden. Only material groups actually submitted
through those callbacks contribute geometry; drawing no actor supplies no
footprint.

Both policies respect current material visibility/color-write/opacity, actual
layer selection, groups, indices and effective drawRange. Source points outside
the submitted range cannot expand its source enclosure. Camera/root and mesh
pose changes between draw and publication reject the frame. A throwing render,
lost context, stale binding/frame, changed viewport/clip, unsupported deformation
or invalid/empty/clipped geometry publishes nothing. Failed sequences are
consumed and cannot be replayed.

A full onscreen viewport is required. Surface dimensions, clipping and known
visibility come from the mounted element and its ancestors, not saved output
settings. Scissor, sub-viewport, render-target and perspective surfaces require a
separate contract. The geometry helper performs no DOM, OS or DPI queries.

A submitted callback and CPU enclosure are **not proof of GPU completion,
nonzero visible body pixels, desktop occlusion or OBS program output**. Alpha
maps/tests, fragment discard and shader opacity can remove pixels within the
geometry enclosure. The adapter checks its asset/context/surface policy; it does
not call `readPixels`. Native arrival validation remains separate.

## Exact default and explicit World enclosure

`new WorldRenderGeometry()` retains exact extrema of submitted CPU points. At
pinned Three.js **0.186.1**, `Mesh.getVertexPosition` evaluates current morph
positions; `SkinnedMesh.getVertexPosition` additionally applies current skin/bind
transforms. Cached geometry or skinned bounding boxes and neutral caption bounds
are not substitutes for these current positions. The exact support-query API and
its equality oracles remain separate and unchanged.

The World adapter explicitly constructs the conservative policy. For an eligible
rigid submitted range, bounded preparation inspects its actual local source
points and retains a coordinate-wise enclosure. At the current successful draw,
that enclosure is transformed by the current affine world matrix and then the
current orthographic projection/view matrix. The rounded interval graph preserves
the sequential Float64 multiplication/addition order and signed zero. It encloses
the corresponding current CPU point calculations; it does not claim its endpoints
are attained by a vertex. Intermediate nonfinite or out-of-world-limit bounds are
rejected before projection could conceal them.

The enclosure can be wider than the posed body. This deliberately reduces safe
travel space or rejects clipping even when tighter geometry might fit. It cannot
make outside geometry fit by clamping or guessing. Unsupported rigid shortcuts
retain the current exact path where supported: current morph/skin results and
rigid enclosures can be unioned under the common conservative claim.

Preparation is versioned by actual source/attribute storage and layout, indices,
groups, drawRange and materials. Expected animation matrices are sampled fresh;
old posed world/projected bounds are not cached. Source changes retire that
owner. An active output cannot silently prepare replacement geometry in place.
Covered preparation emits no measurement, calibration, action frame or gait
credit; completion requires a strictly newer submitted draw.

The native challenge, measurement, accepted binding and action-frame observation
require `enclosureKind: 'conservative_submitted_geometry_v1'`. The strict native
binding wrapper validates it before removing the tag for the pure motion follower.
The Avatar controller checks policy on every update, including unchanged revision
stamps. Missing/changed policy cancels the old output; a policy change needs a
fresh binding/reference revision. Local exact measurements carry
`exact_submitted_geometry_v1` and are not published as native conservative grants.

A prepared rigid enclosure may use **zero live leaf evaluations** on a later
draw. Its diagnostic count stays zero: source preparation is not relabelled as
current vertex evaluation. Availability still requires an actual submitted mesh
and finite, nonempty current bounds. Exact measurements retain their own nonzero
point-work requirement.

## Reference, footprint and ground units

The result contains current posed bounds, a stable projected reference, enclosing
extents/footprint, local reference coordinates, viewport/clip and inverse
CSS-to-model-ground basis. The first successfully accepted pose can define the
reference convention. The reference then remains frozen; it may lie outside a
later posed hull. Its current projection, rather than a changing box centre,
determines the actor anchor.

World readiness uses an initial reference pose and 32 actual calibration renders
(eight headings and four walk phases). The preparing cover must be removed before
a newer successful exposed idle render can provide readiness. Calibration does
not claim that every future clip fits: each later posed enclosure must remain
inside the accepted anchor-relative envelope or the output is cancelled.

Native footprint/safe-centre calculations consume those enclosing extents. A
larger enclosure only shrinks available travel space. Actual arrival still uses
the frozen reference anchor, the exact authorized target, current native fences
and observed movement/settling; neither a box centre nor reaching a local target
alone commits a Life effect.

The fixed elevated orthographic camera makes both ground axes observable. Rays
from its actual projection/view inverse intersect the supplied ground plane; its
inverse ground frame yields CSS-right and CSS-down model-unit vectors. The ground
frame includes approved scale/normalization but excludes the changing actor-facing
yaw. Scale is not converted twice, and no private monitor origin or physical DPI
is inferred. Perspective needs position-dependent mapping and stays unsupported.

`mapReachableCanonical` maps 0..1,000,000 centre coordinates to local CSS as a
sanity check. It rejects targets requiring clamping and safe-centre intervals
with zero extent after footprint/padding. It grants no slot or authority and
emits no arrival/Complete. Native independently validates the current registration,
full action fence, canonical target and footprint.

## Supported paths and material contracts

The CPU path supports ordinary mesh morphs, skinned meshes and the transforms
produced by embedded animation/VRM updates **when their actual draw uses these
known paths**. MToon body material uses those positions. MToon outline expands
positions in its shader, so an active outline material is explicitly unsupported
until an independently bounded outline contract exists. This slice does not
claim every VRM works.

The pinned **three-vrm 3.5.5** MToon constructor installs its own `onBeforeCompile`
arrow callback. That stock callback prepends the library defines and supports
older Three fragment color conversion; it does not add a body vertex displacement.
The compatibility exception requires the exact installed MToon prototype,
unchanged bundled vertex/fragment shaders, stock define generator, stock compile
and program-cache callback implementations, no custom defines, no material render
hook and `isOutline === false`. Ordinary texture, UV, shading, copied material and
CPU morph/skin changes remain supported. A shader flag alone grants no exception:
subclasses, replaced callbacks/shaders/generators, macro overrides (including
`OUTLINE`), displacement and wireframe remain unavailable. The per-instance arrow
implementations are compared through intrinsic `Function.prototype.toString`
against a private stock instance, without invoking candidate hooks. This is a
trusted application/pinned dependency compatibility check, not a sandbox for
hostile JavaScript. A dependency upgrade requires reviewing this shader contract.

Ordinary Three materials likewise accept only their stock define tags: empty for
Basic and the standard no-tag paths, `STANDARD` for Standard, `STANDARD`/`PHYSICAL`
for Physical, `TOON` for Toon and `MATCAP` for Matcap, with their stock empty values.
Additional, changed or inherited defines are rejected for both ordinary and MToon
materials: Three's WebGLProgram uses `for-in`, so an inherited custom macro can
change a shader even when the compile hook itself is unchanged.

Unknown ShaderMaterial, custom vertex hooks/onBeforeCompile,
GPU displacement, wireframe, instanced/batched geometry, sprites/points/lines and
perspective views likewise fail honestly instead of receiving guessed bounds.
Alpha-cutout bounds stay conservative; they do not prove visible body pixels.

## Work and lifecycle limits

Limits remain 2,048 scene nodes, 256 meshes, 1,024 draw groups, 64 morph attributes
per mesh and 512 bones. Measurement segments share the eight-ms CPU clock and
200,000 evaluated-vertex / 2,000,000 estimated-work limits. They cannot preempt a
single synchronous engine call or the renderer itself and are not a performance
benchmark or frame-rate guarantee.

Rigid preparation additionally shares one actor-owner budget of 200,000 source
vertex evaluations, 2,000,000 charged scan/copy/hierarchy/hint operations and
32 MiB of typed cache allocations, with bounded mesh/range metadata. Work is
cooperatively sliced under the same frame clock; retries cannot reset that owner's
counters. Preparation does not extend the native five-second challenge.

Stop, pause/Start-ACK ordering, generation changes, asset/scale/layout replacement,
context loss and source mutation retain their cancellation barriers. A failed
World mount remains suppressed until explicit reconnect creates a fresh Stage;
a plain fallback draw does not restore a visible output grant.

## Validation and remaining acceptance

CPU fixtures use actual Three meshes, morphs, AnimationMixer and bone poses with
a synthetic draw port. Exact equality oracles remain intact; separate enclosure
oracles check containment, signed zero, current transforms, submitted ranges,
source changes, intermediate-world failure and honest zero live leaf counts.
Other fixtures cover materials, stale frames, failed/context-lost renders,
clipping, bounded work and no-clamp target mapping.

A separate actual bundled AvatarStage/WebGL exercise on ANGLE Metal (Apple M1)
passed eight checks, including travel/heading/gait in all eight directions. It
used two builtin actors in one viewport/object layer, verified calibration and a
new exposed idle frame, Start-ACK hold, pause/Resume/Stop, resize/offscreen/context
barriers and stale local asset replacement. Its host bindings and
Delivery/Start/Terminal acknowledgements were **synthetic**. This establishes the
local renderer exercise, not a real native Store/route/arrival, OS/device/provider
or OBS delivery result, and is not a performance benchmark.

Issue #72 remains open for combined native output delivery, platform/OBS acceptance,
registered multi-actor action authority and the other output follow-ups. The local
WebGL exercise does not close those requirements or claim all VRM assets work.

Substantial AI assistance was used in preparing this implementation and fixtures.
