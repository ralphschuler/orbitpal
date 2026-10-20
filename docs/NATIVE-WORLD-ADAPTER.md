# Native World adapter

This is the private implementation foundation for issue #72. The native IPC handlers are not registered and no automatic scene/action start is exposed yet. The visual Life editor and ordinary session-window startup are already available independently.

## Ownership and lifetime

Tauri owns the actual window constructor, session output lease, document authentication, physical layout, World context and action admission. The frontend renders a host-issued scope and reports new measured frames; it cannot create a session, replace a native owner or commit an arrival. Studio and OBS must eventually mirror the elected owner rather than acknowledge duplicate movement.

A retained Normal owner becomes Prepared, then Pending calibration, then Live after an accepted measurement and actual promotion. A later genuinely new frame is required before admitting an action. Cancellation is terminal. A revoked owner retains its charge and replacement barrier until actual restoration/retirement finishes. Dropping an asynchronous waiter does not complete cleanup.

Each actual document has one private capability and one cancellation atom. Duplicate Finished callbacks reuse both. Before delivery, native code watches operator/UI/system invalidations and binds the exact original session lease once. Stop followed by Start in the same window cannot reuse that capability. Only deliberate native rotation after old cleanup creates a new capability; no capability appears in URLs, public events or persistent state.

Expected layout geometry callbacks are classified independently from semantic operator/profile invalidation. The owner's own move/resize must not cancel its document. Any unclassified/failed geometry observation still invalidates layout evidence.

## Bounds and locks

The original native preparation deadline remains issued+5000ms, including its documented inclusive boundary. Document first registration has its own original five-second bound; retries never retime either. A received frame must be genuinely newer and at most250ms old at admission. Live freshness is renewed only by accepted new frames, never metadata or a heartbeat.

Native cancellation registries use short locks and release them before Core, Store, GUI work or awaits. Physical owner handling precedes Core; Core→World→Store is the persistent action order. Action permit checks under that order read only frozen identities and cancellation atoms; they cannot re-enter an executor or GUI. No lock crosses an asynchronous Main-thread operation.

The frontend pins the full actor/World/stage/config scope from a current parsed native response. Events are hints until a fresh scoped read confirms the pose epoch. Old App reads and foreign events cannot retire a newer mount. At most one uncancellable read is outstanding, even after local timeout; each native generation has a fixed bounded read budget.

## Remaining qualification

Before native World exposure: integrate explicit start/reconciliation, real action and slot admission, exact retained-window command delivery, read-only Studio/OBS mirrors, one shared-object layer owner, and cold-asset readiness. Then exercise two actual Wry actors for sustained new frames, eight-direction movement and distinct reserved arrivals; cancellation/reload/replacement and normal speech handoff must finish through real cleanup.

Controlled owner, Core/Store, transport and renderer tests support the implementation but do not prove desktop visibility, actual window callbacks or compositor output. Native app acceptance, reduced motion, mixed DPI and Windows/Linux/macOS limits must be reported separately.
