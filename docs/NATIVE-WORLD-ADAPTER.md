# Native World adapter

This is the private native renderer implementation for issue #72. Explicit Life start and actual page completion now feed a bounded native reconciliation worker, and its private document/measurement handlers are registered. Native movement and slot actions, mirrors and actual desktop acceptance remain incomplete. The visual Life editor and ordinary session-window startup are available independently.

## Ownership and lifetime

Tauri owns the actual window constructor, session output lease, document authentication, physical layout, World context and action admission. The frontend renders a host-issued scope and reports new measured frames; it cannot create a session, replace a native owner or commit an arrival. Studio and OBS must eventually mirror the elected owner rather than acknowledge duplicate movement.

A retained Normal owner becomes Prepared, then Pending calibration, then Live after an accepted measurement and actual promotion. A later genuinely new frame is required before admitting an action. Cancellation is terminal. A revoked owner retains its charge and replacement barrier until actual restoration/retirement finishes. Dropping an asynchronous waiter does not complete cleanup.

Physical page completion is distinct from semantic document authentication. A fact-only asset probe pins one mount within its original five seconds and observes a current matching Avatar load for at most sixty seconds from that first pin. Repeated observations do not extend either bound. Same-key Avatar callers join the real pending load, including its eventual failure.

Only a consumed, one-use ready observation can rotate an operational document after exact old cleanup. The original pre-wait operator/UI/system cancellation atom and exact once-bound session lease are transferred unchanged; semantic rotation cannot manufacture a new admission watch. Stop followed by Start in the same window cannot reuse an old capability. Actual reload, replacement and retirement revoke the physical page lineage too. No capability appears in URLs, public events or persistent state.

One lifecycle worker per participant retains its charge until queued Main work really drains and exact physical cleanup completes. Hints and polls do not repeat a failed unchanged context. Genuine session, profile, page, UI/system or dialog transitions may reconcile again after their own clean admission. Preparing, stale and restoring owners block dialog scheduling. An exact healthy Live handoff may allow scheduling, but the generation-owned dialog must still cancel and restore every owner before strict dialog/playback admission succeeds.

Expected layout geometry callbacks are classified independently from semantic operator/profile invalidation. The owner's own move/resize must not cancel its document. Any unclassified/failed geometry observation still invalidates layout evidence.

## Bounds and locks

The original native preparation deadline remains issued+5000ms, including its documented inclusive boundary. Document first registration has its own original five-second bound; retries never retime either. A received frame must be genuinely newer and at most250ms old at admission. Live freshness is renewed only by accepted new frames, never metadata or a heartbeat.

Native cancellation registries use short locks and release them before Core, Store, GUI work or awaits. Physical owner handling precedes Core; Core→World→Store is the persistent action order. Action permit checks under that order read only frozen identities and cancellation atoms; they cannot re-enter an executor or GUI. No lock crosses an asynchronous Main-thread operation.

The frontend pins the full actor/World/stage/config scope from a current parsed native response. Events are hints until a fresh scoped read confirms the pose epoch. Old App reads and foreign events cannot retire a newer mount. At most one uncancellable read is outstanding, even after local timeout; each native generation has a fixed bounded read budget.

## Remaining qualification

Complete real action and slot admission, exact retained-window command delivery, read-only Studio/OBS mirrors and one shared-object layer owner. Exercise two actual Wry actors for sustained new frames, eight-direction movement and distinct reserved arrivals; cancellation/reload/replacement and normal speech handoff must finish through real cleanup.

Controlled owner, Core/Store, transport and renderer tests support the implementation but do not prove desktop visibility, actual window callbacks or compositor output. Native app acceptance, reduced motion, mixed DPI and Windows/Linux/macOS limits must be reported separately.
