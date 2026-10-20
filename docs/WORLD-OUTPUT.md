# World output authority foundation (#72, still open)

This packet provides native registration, receipt validation and checked storage
for a future selected-bot Studio `MoveTo` adapter. It adds no mounted 3D World
view, movement control, Avatar adapter or renderer bridge. The ordinary App,
character view and overlay rendering are unchanged. With no registered renderer,
normal application flows keep `visualAvailable:false` and cannot plan a visual
action. The existing explicit quiet-rest path is unchanged.

The protocol below describes requirements for that future adapter. Life must
already be active, the bot must explicitly belong to the shared stage, and the
bundled renderer must register and report a successful exposed frame. Registration
starts no session or action. Other visible participants would be mirrors. Native
overlay/OBS execution, nonselected Studio execution and other visual activities
remain unsupported. Issue #72 remains open.

## Ownership and protocol

`Core` owns the bounded output registry and action executor. The main window
alone may invoke `register_world_output`, `report_world_measurement`,
`report_world_action`, `unregister_world_output`, `get_world_output_status` and
`start_life_move`; the Control MCP exposes none of these. An authenticated route
and fresh native UI connection are required in addition to the opaque render
binding. A renderer cannot supply a private Life session, native Store permit,
source World authority, capture grant or provider credential.

Registration returns a native-issued challenge with the exact asset, layout,
measurement and reference revisions. Reports echo the challenge ID and provide
observations. The host creates all binding revisions and the render-session ID.
A same-mount renewal reuses the unchanged binding and one unexpired challenge;
a metadata call does not refresh successful-frame age or cancel that binding.
Only a newer matching frame refreshes its age. The original frame expires after
five seconds; a later measurement cannot resume a cancelled old action.

The direct `cohost://world-action` event is targeted to `main` and carries the
closed MoveTo Offer/Pause/Resume/Terminal message. `start_life_move` returns the
ordinary per-bot confirmed/runtime action status after a genuine checked Plan;
it does not claim Started. Renderer reports are ordered Delivered, Started,
Progress, Completed. Started is acknowledged only after the real Start commit.
Exact duplicates are idempotent and cannot refresh time, advance position or
apply another completion. Changed payloads with a reused receipt sequence are
rejected. Late old-session/binding/action messages cannot acknowledge a successor.

## Measurements and movement

Coordinates in measurements and motion commands are local CSS pixels. Canonical
stage positions remain integer values from zero to one million. The stable
placement anchor is distinct from absolute posed bounds. Nonnegative asymmetric
envelope extents surround that anchor; the anchor need not lie inside the body
hull. The host computes conservative symmetric safe-center bounds using the
maximum horizontal/vertical extent, intersects them with the stage safe region
and viewport clip, and rejects targets that do not fit. It does not clamp or
silently replace an operator's target.

`world_iso_v1` uses elevation π/6, camera yaw 0, distance 20, vertical span 8,
near 0.05 and far 100. Positive camera Z maps CSS down to positive ground Z. The
host validates the closed inverse basis against viewport height and the actual
reported uniform placement-root scale: right `(8 / h / scale, 0)`, down
`(0, 16 / h / scale)`. The basis is independent of facing yaw. Persisted and
native preview scales are independently read; reference, observed basis, model
forward yaw and applied scale are frozen for the binding. The reported viewport
cannot exceed the sampled main-window logical dimensions. Matrix, calibration,
posed bounds and successful-render claims remain observations from the trusted
bundled renderer; the native process does not attest GPU pixels or independently
recover its matrices.

Each accepted frame is checked against its exact current binding, increasing
frame/receipt sequence, viewport, clip, envelope, safe center and native arrival
time. Movement is bounded by the native straight-line target and speed policy.
A nontrivial anchor displacement requires the observed walking flag. A successful
render bridge must derive that flag from committed movement, not intent. The
renderer-provided idle counter is diagnostic only. Native arrival needs at least
three distinct accepted stationary target frames spanning 100 ms, no more than
250 ms apart. A gap, movement, breach or rebind resets this evidence. Completion
must reference a new frame; no integer counter can manufacture arrival.

## Durable admission and cancellation

The lock order is Core → WorldHub → Store. GUI/window/topology sampling, async
probes and event emission run outside those locks. Native window/profile/scale
invalidation has a short cancellation-only latch before it waits for Core or
SQLite. It cannot grant or renew output presence.

An explicit move uses the existing one-use native operator-intent mechanism:
fresh truthful UI connection/sequence, current selected Life session/profile,
short expiry and single consumption. Dirty drafts, modal dialogs, updates,
permissions and other real blockers remain effective. The command's own native
synthetic operator barrier may be recognized; an arbitrary clear report cannot
bypass a real blocker. Human activity retains the existing conservative global
interruption policy; this first slice makes no parallel multi-bot UI guarantee.

Plan, Start and Complete use the actual checked immediate SQLite transaction.
After acquiring its write lock, the callback rechecks current native session,
UI/system generation, output cancellation, exact selected profile and persisted
World record. Start/Complete additionally require the current successful frame
to be no more than 250 native milliseconds old after the DB wait. Only the
executor's one-use permit exposes the exact event/revision for commit. A genuine
committed result is installed once; a late acknowledgement cannot undo it.

Accepted progress updates the exact active World context and its canonical
position under Core → WorldHub, retaining the existing member/lease identity.
It does not invalidate itself by comparing a later frame with the initial
position. Observed movement can remain visible if a subsequent completion write
fails; that position is not a claim of a committed Life reward. Failure and Stop
cancel the exact action, release resources, retain bounded terminal tombstones
and reject old permits/receipts. Restart has no registrations or action authority.

## Native window route foundation

A separate private registry tracks each real native avatar window by its exact
`WindowSignals` instance and host-issued generation. Its bounded preparatory
tickets expire after five seconds; Stop, profile/presentation changes and native
geometry/close events invalidate retained tokens before Core waits. Retired
window callbacks cannot revoke a replacement in this new registry. Counter
exhaustion and registry poison cancel retained tokens and grant no successors.

This foundation exposes no native registration IPC, prepared-layout promotion or
rendering capability. Its Hidden event is currently a synthetic/API seam, not
an exercised avatar-hide callback. The legacy label-scoped output cancellation
path is unchanged; whole-host replacement isolation must be reconciled before
native visual actions. Confirmed layout/DPI/restoration, renderer measurement and
speech ownership handoff remain separate implementation and platform work.

## Validation status

The actual production Tauri/Store suite passed **549 tests**, including native
registry/geometry/arrival, real Core/WorldHub/SQLite lifecycle and held-write-lock
cases, plus all 15 native-route regressions. That gate ran on source checkpoint
`cfe752f`. Follow-up `c1b31f4` only removed one terminal newline byte from each
of the two new route files; byte comparison verified
`final == tested.rstrip(b'\n') + b'\n'`. The suite was not rerun for that
whitespace-only change. The foundation composition preserves those native bytes;
its combined checks are a separate gate.

These results do not establish successful WebGL rendering, real motion quality,
native/OBS execution or multi-actor authority. A separate experimental actual-
renderer integration failed its acceptance exercise and is excluded from this
packet; its repair and rerun remain integration work. The pure helpers are also
unmounted and need explicit wire reconciliation with this native protocol. The
real Avatar bridge, eight-direction gait/scale/topology exercises, renderer
performance and platform/output acceptance (#75) remain required before #72 can
close. No 3D view or successful visible MoveTo is claimed by this foundation.
