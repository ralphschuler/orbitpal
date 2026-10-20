# World motion and render order

`src/lib/world-motion.ts` is the pure motion follower used by the selected
Studio World adapter for issue #72. The helper itself performs no Three.js,
timer, randomness, DOM query, capture, permission or provider work.
It consumes native-authorized **local CSS** coordinates. Native output code must
validate canonical targets, slot leases, measured footprint, safe region, monitor
origins/DPI and selected confirmation owner before constructing these inputs.

The [native output bridge](WORLD-OUTPUT.md) validates the required enclosure
policy before stripping that native wrapper into the pure binding contract.
`WorldAvatarController` connects the follower to actual submitted poses, a stable
reference and acknowledged delivery/start barriers. Fixed and Ambient modes keep
their existing routing; only an explicitly opened Studio World view selects this
adapter. Native overlay/OBS World execution remains unsupported.

## Binding and ordering

One follower owns one explicit actor attachment. A binding contains its public
render session and monotonically increasing registration sequence, actor source
WorldId, public world instance/StageId/config revision, topology/layout/asset/
measurement revisions and current measured geometry. It contains no private Life
session, tool permission, credential or lease capability. Ground right/down basis
vectors convert local CSS displacement into model-ground units for gait; they
must describe the current measured camera/model scale. They are not OS DPI.

Bind only the latest actual native registration reply. A new native connection
uses a fresh follower and disposes the old one. A stale registration sequence
cannot replace the current binding. Binding/scale/asset/layout/reduced-motion
changes invalidate all pending frames/work and require fresh native registration
and action authority; a local resize never silently remaps an old goal.

Strict version-1 Offer/Pause/Resume/Terminal records contain the exact attachment
fence, delivery ID, ActionId and native sequence. Offers support only `move_to`
in this slice: a confirmed start, exact safe target, bounded speed/acceleration
and bounded remaining presentation lifetime. No arbitrary paths, actions,
commands or physical coordinates are accepted. Unexpected fields, unsupported
versions, invalid IDs/numbers, unsafe counters, ambiguous geometry and getters
are rejected. Parser results are copied and frozen.

Sequences do not roll back. Terminal ordinal high-water protects the current
actor source lineage, including Stop arriving before its Offer and a reissued
higher-sequence old Offer. An older action's terminal cannot cancel its newer
replacement. Exact duplicate Offers, even with a higher sequence, do not restart
motion or extend a deadline. A conflicting target/speed/duration for one action
and delivery is rejected/cancelled; a fresh native action is required. There is
no local retarget, random waypoint loop or successor selection.

One native action has one delivery within a binding. A terminal for the same
action ordinal but another delivery conservatively cancels that action; a bridge
must use a fresh ActionId for a replacement delivery. The eventual native adapter
must preserve that invariant or add exact delivery correlation before integration.

Receiving an Offer does not start movement. `beginAfterDelivered` requires the
exact original action/delivery/Offer sequence after the native Delivered receipt
was accepted. Late promises cannot start paused/stopped/replaced work. These
typed frontend correlations are not native authority: the real host remains
responsible for authentication, current Life mode/session, resources and effects.

## Propose, render, then commit observation

`sample(now, deltaSeconds)` creates one immutable candidate frame. It proposes
straight-line acceleration/braking, local CSS displacement, ground vector,
eight-direction compass heading, shortest-angle yaw and candidate gait distance.
It never changes the committed position, speed, yaw or accumulated gait distance.
Until the pending frame is resolved, repeated sampling returns the same proposal.
No frame delta exceeds 50 ms, and a frame-stream gap beyond 250 ms stops the work
rather than teleporting or performing wall-clock catch-up.

The renderer applies that exact candidate transform/pose, renders successfully,
then measures its actual current viewport and posed bounds. Only then may it
call `afterSuccessfulRender` with the exact opaque token object, matching current
fence, fresh frame sequence, actual CSS centre, viewport and posed bounds.
Copying a numeric token is insufficient. Failed, old, superseded or mismatched
callbacks commit nothing. An obsolete token is checked before its timestamp,
so it cannot invalidate a replacement through a bad old callback.

Actual centre/bounds must fit the native-authorized safe geometry and measured
footprint envelope. A target outside that safe region is rejected, never clamped
to a different target while keeping the original action identity. A geometry/
visibility/WebGL failure in the real adapter must invalidate the binding; the
pure helper does not invent a usable default footprint or prove visibility.

The successful hook commits the **actual applied displacement**, including
bounded subpixel differences, and returns a FrameObservation. The observation's
ground distance is what may advance the existing `DistanceWalkClock`. Proposal
distance is preview data for a candidate pose only; failed frames must discard
temporary gait/mixer advancement. Both displacement components contribute to
ground distance and heading, so up/down/diagonal movement is not horizontal-only.
Authored walk/idle selection, root compensation and actual posed measurements
remain the Avatar adapter's responsibility.

Two successful stationary target frames can expose `targetIdleFrames:2`. This is
local evidence only. A mathematical endpoint, elapsed timer or this count is
**not arrival or Complete**. The native output bridge must independently validate
its exact owner/session/action, actual mapping/footprint/target and slot lease,
then use the real ActionExecutor/Store lifecycle. Only an incoming confirmed
native Terminal marks that semantic action resolved. The follower sends no
completion receipt or needs effect and does not update WorldState.

## Interruption and reduced motion

Pause invalidates outstanding frame tokens, stops proposed travel/gait and needs
a newer exact native Resume. Resume starts from the last committed position and
time without banking the paused interval. Stop/terminal, supersession, deadline,
render failure, reconnect and binding changes invalidate pending work. Disposed
attachments cannot be resurrected by callbacks.

Missing walk support or reduced motion rejects travel. This slice supports no
teleport/static-travel fallback and never chooses another clip or gesture.
The motion router selects exactly one of fixed/Ambient/World. Embedded clips
remain preferred for gait; procedural fallback uses actual ground distance.
The World view is muted and does not acquire normal speech/emote/SFX leases.

## Validation status

Pure source fixtures are written for all eight headings/distance, immutable
parsing, duplicate/conflicting/reordered Offers, exact render tokens, failed and
superseded frames, actual subpixel displacement, target observations without
completion, pauses, frame gaps, stale callback clocks, asset/layout/scale/
reconnect barriers, reduced motion, safe bounds and actor isolation.

Pure fixtures use synthetic after-render observations. A separate local actual
AvatarStage/Metal exercise passes all eight directions, actual distance-based
gait, stationary Start-ACK holds, pause/Resume/Stop and stale/context barriers.
Its native bindings and acknowledgements are synthetic. Native window/Store
integration, OBS execution and platform acceptance remain separate gates;
issue #72 remains open.
