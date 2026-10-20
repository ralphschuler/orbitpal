# World output — selected Studio MoveTo (#72, still open)

The Studio now mounts an explicitly opened 3D World view, its Avatar adapter and
the native registration/receipt bridge. For the selected bot, the operator can
request one `MoveTo` target through the existing one-use native intent path.
Life must already be active, the bot must explicitly belong to the shared stage
and use Studio output, and its current renderer must register and report a
successful exposed frame before movement is available. Opening or registering
the view starts no mode or action. Other supported visible participants are
mirror views; they receive no action authority.

This first integration supports selected-bot Studio MoveTo only. Native overlay
and OBS execution, nonselected/multi-actor action ownership and other visual Life
activities remain unsupported. Issue #72 stays open for that full scope and its
platform/output acceptance. The existing explicit quiet-rest path is unchanged.

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
A mandatory `enclosureKind: "conservative_submitted_geometry_v1"` appears in the
challenge, measurement report, accepted motion binding and every frame report.
The host issues that policy, checks its echo, freezes it for the binding and
checks each action frame against it. Missing or unknown kinds are rejected;
there is no default or renderer-selected alternative. Same-binding renewal
cannot reset the policy, reference or original identity.

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
placement anchor and its explicit local reference are distinct from absolute
posed bounds. Once accepted, the reference remains frozen under its native
reference revision; a changed attachment requires a fresh binding. Nonnegative
asymmetric envelope extents surround that anchor; the anchor need not lie inside the body
hull. The host computes conservative symmetric safe-center bounds using the
maximum horizontal/vertical extent, intersects them with the stage safe region
and viewport clip, and rejects targets that do not fit. It does not clamp or
silently replace an operator's target.

`posedBounds` is a conservative enclosure of currently submitted CPU source
geometry under the current Three.js Float64 transform graph, not necessarily
tight vertex extrema, shader/GPU positions or rasterized visible pixels. A rigid
draw range may use a conservative source-AABB enclosure; supported skinned/morph
geometry uses its current CPU evaluation or fails unavailable. Mixed components
retain the same enclosing contract. The bounds may include empty space and
reject a target that tighter geometry could fit; they are never cropped or
shrunk to manufacture readiness. See [geometry support and limits](WORLD-RENDER-GEOMETRY.md).

The Avatar adapter collects 32 calibration observations across eight headings
and four gait phases, removes its preparation cover, then renders a new exposed
idle frame for the native challenge. A covered calibration frame cannot be
promoted into that measurement. Subsequent actual frames must still satisfy the
frozen envelope; calibration is not a promise that every imported animation fits.

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
A nontrivial anchor displacement requires the observed walking flag. The render
bridge derives that flag from committed movement, not intent. Its first behavior
frame primes the walk pose without displacement; further travel waits for the
native Start acknowledgement. Gait follows committed traveled distance and
eight-direction facing, while failed proposals cannot advance accepted travel.
Neither a conservative hull's center nor its overlap with the target is arrival.
The renderer-provided idle counter is diagnostic only. Native arrival needs at least
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

This native-window foundation exposes no avatar registration IPC, prepared-layout
promotion or rendering capability. It is not a native-window adapter for the
Studio bridge. Its Hidden event is currently a synthetic/API seam, not
an exercised avatar-hide callback. Native constructor and close callbacks use
the same registry lock to fence legacy output cancellation with replacement.
A new window cancels prior jobs for its label before its route is published;
repeated registration of the same instance is idempotent. A proven retired
callback changes neither the successor's jobs nor the output admission counter.
Current and missing-instance close events retain conservative cancellation under
that same lock. Registry poison and failed tracking remain unavailable and
conservative; they establish no whole-host replacement-isolation guarantee.
The main-window loss path and deferred exact-instance speech cleanup retain
their existing behavior. These host checks are not real OS event-timing proof.
Confirmed native-window layout/DPI/restoration, its own renderer measurement
and speech ownership handoff remain separate implementation and platform work.

### Private geometry observations

`WindowSignals` also has private typed request, event and getter-correlation
helpers. They have no production callers in this increment. One exact signal
instance holds at most one typed owner and two field records. Typed requests
start with empty legacy queues and issue fresh request IDs; they never adopt a
coalesced legacy request. Every legacy request invocation, including identical
coalescing, and every restore fence invalidates typed ownership while retaining
the existing legacy result, FIFO order and restoration behavior. Cancellation
and dropping an owner leave pending native requests queued for safe cleanup.

Opaque handles check their exact instance and current owner. Stale or foreign
handles cannot cancel a healthy successor. Poison, invalid typed dimensions and
exhausted counters return no usable typed fact. Matching an event to a queued
request establishes correlation only: the OS event carries no operation ID.
The host must sample a later fresh native getter; the one-use comparison checks
unchanged ownership, event/request counters, empty queues and the target bounds.
It cannot authenticate a caller-supplied getter value or establish setter
causality, continuing visibility, monitor/DPI identity or rendered size.

The future coordinator still owns the original deadline, exact window/context
checks, cleanup lease and actual native calls. These helpers expose no IPC,
LayoutPrepared promotion, Ready, renderer registration, action or speech grant.
Their synthetic tests establish no real Main-thread, event-order or OS timing
acceptance.

## Validation status

The actual production Tauri suite passed **552 tests**, including real
Core/WorldHub/SQLite lifecycle and held-write-lock cases, native-route cancellation
and the required enclosure policy. The added cases reject missing/unknown policy,
preserve binding/reference identity on renewal, keep conservative safe-center
restrictions and prevent a hull centered at the target from completing a
stationary actor's action. This native result validates protocol/storage behavior
independently of a GPU.

The separate actual Chromium/Metal AvatarStage exercise passed **eight checks**
with two builtin Nova figures, including all eight directions and three
captured views. It exercises the real renderer with **synthetic native bindings
and acknowledgements**. It does not prove real Tauri IPC, native Store commits,
native-window layout or OBS ownership in that rendered run. The focused frontend
gate passed **292 tests**, and both TypeScript checks reported zero errors and
warnings. Final composed application/CI gates remain separate results.

Reviewed World reports, Life UI heartbeat/status, playback reports and selected
metadata getters use Tauri asynchronous dispatch for their Core/SQLite waits.
This is not a guarantee of native Stop latency under worker starvation; actual
window/event scheduling remains part of #75 acceptance.

These distinct native and renderer results are not an end-to-end proof of every
asset, device, native window or output route. Native overlay/OBS rendering and
prepared-layout/speech ownership, nonselected/multi-actor execution, wider
scale/topology/asset exercises and real platform/output acceptance (#75) remain
required before #72 can close. A successful Studio frame grants none of those
missing capabilities.
