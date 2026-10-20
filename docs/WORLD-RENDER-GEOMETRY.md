# Submitted-frame World geometry foundation

`world-render-geometry.ts` is an **unwired** #72 slice. AvatarStage, App, overlay,
OBS, native output registration and arrival admission are untouched. This code
cannot make a character move or certify a completed native action on its own.

The separate [native output foundation](WORLD-OUTPUT.md) is also present, but no
bridge or 3D World view is mounted in the normal App. Reference revision/anchor
semantics, renderer lifecycle and native receipt ordering must be reconciled
before these independent contracts can form an executable adapter.

## Why the existing caption bounds are insufficient

At pinned Three.js **0.186.1**, `Mesh.getVertexPosition` evaluates current morph
positions and `SkinnedMesh.getVertexPosition` additionally evaluates current skin
transforms from bone matrices and bind matrices. These are the actual installed
CPU paths used here. Cached `geometry.boundingBox`, `SkinnedMesh.boundingBox` and
`Box3.setFromObject` without precise traversal can describe an earlier pose.

AvatarStage currently finishes mixer/VRM/jaw/morph/gait/root compensation and
controls before calling `renderer.render`. Its desktop caption anchor is emitted
**before** that render, from `desktopModelBounds`, a cached neutral framing hull.
That callback must not become a successful World-frame or arrival acknowledgement.
The later adapter must invoke this slice at the actual render point, after the
current pose has been established, and preserve existing audio/presentation rules.

## Submitted draw, then successful enclosing render

One `WorldRenderGeometry` instance belongs to one loaded actor attachment. Bind
its exact public actor/source-world/render-session/binding/world-instance/stage/
config/topology/layout/asset/measurement revisions, actual root and camera, current
ground-to-world model-unit frame and optional previously measured local reference.
The binding sequence increases even when an old registration reply arrives late.
New connection ownership disposes/invalidates the old instance; its numbers are
correlations, never native grants. The real native bridge must authenticate that
this root, asset, route, geometry and source lineage are still current.

`renderFrame` receives the exact current frame sequence, actual renderer/scene,
a fresh mounted-surface observation callback, a current-identity callback and an
explicit monotonic measurement clock. It installs temporary `onAfterRender`
hooks on a bounded actor subtree, calls the synchronous renderer, and restores
all its hooks in `finally`. An existing nonstandard figure/scene hook is not
silently overridden. Only groups actually submitted through those mesh callbacks
can contribute geometry; a mere call that draws no actor supplies no footprint.

Inside each draw callback, positions come from current morph/skin-aware
`getVertexPosition`, transformed by that draw's current matrixWorld and exact
camera projection/view. Material visibility, color-write/zero-opacity, layers
through actual draw selection, geometry groups, indices and drawRange are
respected. Unused/off-material vertices cannot expand the figure. No matrix,
skeleton or mixer is refreshed after drawing into a different pose. Camera/root
matrix changes between submitted groups or before publication reject the result.

A throwing enclosing render, lost context, replaced binding/frame, changed actual
viewport/clip, unsupported deformation or invalid/empty/clipped geometry publishes
nothing. The failed frame sequence is consumed so a late callback cannot replay
it. Fresh surface observations compare their known dimensions, clip and visibility
fields, independent of JavaScript property insertion order. A full onscreen viewport is required; scissor/sub-viewport/render-target
surfaces need a separate contract. Surface width/height/clip must come from the
actual mounted element and clipping ancestors; stored profile output dimensions
and a boolean guess do not establish them. There are no DOM/OS/DPI queries here.

A submitted draw callback is **not proof of nonzero visible pixels**, GPU finish,
unoccluded desktop display or OBS program output. Alpha tests/maps, fragment
discard, clipping and shader opacity can eliminate pixels within the conservative
geometry bounds. This slice does not inspect textures or call readPixels. The
actual adapter still needs its explicit asset/context/surface visibility policy,
and native arrival validation is independent of this local observation.

## Geometry and model-ground units

The result contains current projected posed bounds, a stable projected reference
point, a symmetric enclosing footprint about that reference, local reference
coordinates, viewport/clip and inverse CSS-to-model-ground basis. If no reference
is supplied, the first **successfully submitted and accepted pose** defines its
centre convention; no placeholder model size or neutral collision hull is used.
Later posed bounds are reevaluated. The stable reference is not a promise that
an animated figure's bounding-box centre stays still, nor a frozen footprint.
A reference leaving the current posed body fails unavailable rather than inventing
a centre. Native must confirm registration semantics before using that result.

Only the current orthographic view is supported. Rays derived from its exact
projection/view inverse intersect the supplied model-ground plane; transforming
those intersections through the inverse ground frame supplies both CSS-right
and CSS-down ground vectors. Scale, camera elevation/yaw and zoom therefore affect
distance and heading explicitly. The cached projectionMatrixInverse is not trusted.
Perspective travel needs a position-dependent mapping and remains unsupported.
The ground frame must match the native-approved scale/normalization and must not
include the figure's changing travel heading. This observation cannot derive a
private monitor origin or physical DPI factor.

`mapReachableCanonical` offers a local sanity check using the current measured
footprint, safe region, clip and padding. It maps 0..1,000,000 centre coordinates
to local CSS exactly and rejects a target that would need clamping. It does not
change the target, reserve a slot, grant authority, report arrival, emit Complete
or change WorldState. Native must independently validate the current registration,
canonical target, mapping, object/slot and full action fence. The helper is not a
replacement for that check and cannot make stale measurements current. After the
posed footprint and padding are applied, both safe-centre intervals must have
strictly positive extent, matching the motion binding. An exact fit with only one
possible centre is unavailable, even if that centre matches the requested target.

## Bounded support and limitations

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

Limits are 2,048 scene nodes, 256 meshes, 1,024 total draw groups (checked before
render), 200,000 submitted vertex visits, 64 morph
attributes per mesh, 512 bones and 2,000,000 estimated deformation operations per
measurement. Explicit CPU-clock checks bound measurement segments to eight ms;
there is no guarantee of preempting a single synchronous engine call or the
renderer itself. These are work limits, **not measured performance claims**.
No default per-frame measurement cadence is installed: the real adapter must
choose a bounded cadence, invalidate stale geometry and preserve action liveness.

## Validation status

Fixtures are written against actual Three CPU meshes, relative morphs, a real
AnimationMixer/bone pose, stale cached bounds, material groups/ranges/layers,
MToon body versus outline, frame/binding changes, failed/context-lost rendering,
clipping/empty/nonfinite geometry, bounded work and no-clamp target mapping. The
original focused run passed **28 of 29** fixtures and failed the real stock MToon
body positive because its own compile callback was rejected. That failure remains
recorded; the positive assertion has not been weakened. The follow-up adds stock
MToon morph/skin/copy and stock Three material positives, custom-material negatives,
reordered surface and collapsed-interval regressions. All **57 geometry fixtures
pass**; the combined focused gate passes **86 fixtures** with the 29 pure motion
fixtures. Current Svelte/TypeScript checking reports **zero errors and warnings**.
The first typecheck exposed two implicit fixture/hook types; explicit annotations
fixed them without changing behavior, and the failure log is retained.
Their draw port is deliberately synthetic; they create no WebGL context, native
window or arrival proof. Broad frontend/build/native checks were not run for this
isolated follow-up. Actual render acceptance and the final combined checks remain
required before integration.

A separate experimental actual-renderer integration failed its acceptance
exercise and is excluded from this foundation packet. The synthetic draw port
does not repair that failure or establish real-frame performance. Successful
mounted rendering, calibration, speech ownership and platform/OBS acceptance
remain follow-up work; issue #72 stays open.

Substantial AI assistance was used in preparing this source slice and fixtures.
