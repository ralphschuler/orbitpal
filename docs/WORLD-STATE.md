# Shared native stage foundation

The pure `world_state` module describes a common stage with explicit actor
membership, canonical positions, safe regions, rest/meeting/play anchors and
exclusive fixed slots. It does not inspect the desktop or call capture,
permissions, network, Tauri, storage or rendering APIs.

The native WorldHub, optional SQLite Store adapter and main-window operator
commands integrate this foundation with the existing LifeRuntime. The pure
standalone fixture remains separate from those adapters. No running character
acquires authority from serialized state or membership alone.

## Stage identity and existing Life state

`StageId` is distinct from the existing `life_domain::WorldId`. The latter is
part of an actor's persistent action lineage. Each `ActionId` contains its actor,
source world and monotonically allocated ordinal. Joining a common stage never
rewrites that world, needs, history, RNG, revisions, ordinal counters or terminal
watermark. Two actors can therefore share a stage while retaining different
source worlds and all their existing wrong-world protections.

Persistent `WorldConfig` contains only its schema version, stage ID, configuration
revision, explicit member IDs, canonical safe region and anchors/slots. It rejects
unknown fields, unsupported versions, invalid identifiers, duplicate members or
resources, out-of-region targets and excessive entries. Limits are 64 members,
64 anchors and 128 total slots. Config edits require the current expected
revision and advance it; a caller cannot reset it with a draft revision.
`with_revision` constructs validated confirmed config for the Store adapter;
it grants no write permission and does not replace its atomic database CAS.

`WorldState` is session-only and has no deserializer. The native owner must mint
a fresh `RuntimeEpoch` on every restart. New state restores no memberships with
authority, positions, occupancy, leases, sessions or action acknowledgements.
The runtime cannot create or mutate a LifeState. Actor binding accepts a native
session stamp and an optional already-validated LifeState. A not-initialized actor
can have public placement without being implicitly initialized. Its native
session operation is distinct from a Life action and cannot grant a Life effect.

The native adapter derives session stamps from current Core while holding its
lock before calling this module. These types are internal contracts, not a
replacement for native authorization. No serialized renderer/model/MCP input
can be treated as a `MemberHandle`, `SlotLease`, session stamp or runtime epoch.
Source-world replacement, including uninitialized to initialized Life, requires
a different native session/revision. An identical stamp cannot replace its
lineage or bind again after unbind. For the same session ID, a rebind after
unbind requires a strictly greater revision; delayed lower revisions fail.
Only the bounded last session is retained per participant. A different UUID
still requires the native Core-current-session check; there is no unbounded
historical UUID blacklist. Ambient must load existing initialized Life when
binding, rather than pass None to discard that source-world association.

## Reservations and cancellation

The caller serializes mutable WorldState access under one native WorldHub lock.
Acquiring a fixed slot is a single mutation: one owner succeeds, the other sees
Occupied. An exact duplicate acquisition returns the same lease without extending
its lifetime. Renewal is explicit. TTL is between 1 and 120,000 milliseconds,
checked against a monotonic high-water clock; exact expiration removes the
lease, backward time cannot resurrect it and large gaps do not cause catch-up.

A lease binds the stage/runtime epoch, unique serial, actor membership
generation, native session/revision, source-world action or native operation,
slot and slot generation. Stale releases, renewals and arrival validations cannot
affect a replacement. Rebinding a session or source lineage clears old leases.
Removing and re-adding an actor cannot reuse its membership generation. Updating
a Life projection rejects revision rollback and releases leases for actions that
are no longer current/nonterminal.

Position updates advance the public snapshot sequence without invalidating an
unrelated slot. Configuration changes advance a separate CAS revision. Moving or
removing a slot invalidates its leases; unchanged slots retain theirs. Native
monitor/DPI topology changes must call `invalidate_geometry` before accepting
arrival on a new mapping. That invalidates all previous mapped places while
retaining actor membership. Stop/session replacement calls `unbind_session`.
The native integration invokes these boundaries for action observation, Stop,
output loss, profile/asset changes, shutdown and membership removal.

`validate_reservation` only confirms a place and returns its canonical target.
It does not start or complete a Life action. A future executor must additionally
check native Core authority and commit the original ActionId through the existing
Store compare-and-swap reducer. Persistence failure grants no reward. Sequence
or identity exhaustion fails closed and revokes runtime authority rather than
wrapping/reusing an identifier.

`observe_actor` accepts a validated native Store observation for an explicit
participant even before first activation or after Stop. It updates public
activity and removes obsolete Life-action leases without minting a session.
Revision rollback fails. Unexpected source-world replacement revokes that
actor's authority and projection while retaining the source-world fence; an
observation cannot silently reset it. Explicit operator placement is a separate
`place_actor` path: it checks the config revision/member/safe region, invalidates
that actor's arrival leases and never starts a session or Life action.

The intended lock order is Core → WorldHub → Store, extended consistently where
the existing AutopilotHub/ScreenContext are also involved. Never call Core from a
WorldHub-held callback, wait for a native GUI getter or emit/await providers under
these locks. Projections are collected under locks and emitted after unlock.

## Coordinates and outputs

Canonical x/y are integers from 0 to 1,000,000, top-left origin, +x right and +y
down. Positions represent the figure centre. Output mapping uses an explicit
logical rectangle and figure footprint. Safe region, footprint and padding clamp
the centre before publication. Too-small/invalid geometry returns an error rather
than claiming success.

The pure bridge takes supplied monitor geometry as data. It never enumerates
monitors. Only the native output adapter may add physical origins, convert DPI
and round physical coordinates; Three.js/DOM receives local logical coordinates.
A monitor at physical (-2560, 0), size 2560×1600 and scale 2 has a local logical
1280×800 stage. Its canonical centre maps to logical (640, 400), physical
(-1280, 800). Studio/OBS can map the same canonical anchor into their bounded
logical stage. Private monitor IDs are not part of the public world snapshot.

`choose_output` selects a still-present preference or deterministic ID-sorted
fallback, and returns None when no validated output exists. The future native
bridge must revoke affected geometry leases, select/clamp a safe fallback point
and pause if no valid mapping exists. These helpers do not prove real desktop
arrival, pathfinding, physics or animation delivery.

## Public and persistence boundaries

Public snapshots contain the common stage ID, config revision, snapshot sequence,
safe geometry, anchors, selected actors' whitelisted Life observations/positions
and occupied-slot actor IDs. They omit session IDs, runtime epochs, membership
and lease capabilities, expiry/serial internals, RNG, needs, reward policy,
history, private instructions and credentials. Nonparticipants are absent.
Public snapshots have no authority-restoring deserializer.

Native persistence uses the separate optional versioned `shared_world_config`
SQLite singleton. It saves only validated WorldConfig and private monitor binding
under an IMMEDIATE transaction/CAS. Unsupported/corrupt data leaves ordinary
profile/manual repair and per-bot Life storage usable, without resetting the row.
Ordinary bot export/copy/import does not inherit global membership or runtime
capabilities. Import also reserves IDs referenced by orphaned saved membership;
an unreadable optional World layout conservatively allocates a fresh imported
bot ID rather than reviving an unknown member. Rehydration requires existing
actor IDs and fresh native source-world/session checks. World sync grants no
screen access and never starts recording.

## Native integration and operator contract

One AppState-owned WorldHub holds shared semantic positions, session-bound
membership and fixed-slot leases. The lock order is Core → WorldHub → Store.
Native GUI/monitor queries and event emission occur outside those locks.
`reserve_current` derives actual current Core/output eligibility again before
admission, then verifies any Life ActionId against the currently persisted
nonterminal action. A retained renderer/session string cannot reserve a slot.
Output eligibility transitions advance the native session revision, cancel old
membership/leases and require a fresh binding; they do not rewrite Life history.

The main operator window alone can invoke `get_world_settings`,
`get_world_state`, `save_world_config` and `place_world_actor`. Control-MCP has no
such tools. Read-only status polling is exempt from operator-activity cancellation;
save/place remain explicit operator activity. Saving configuration or positioning
an actor neither starts Life/Ambient nor initializes an actor or grants capture.

Settings version 1 has `instanceId`, `availability`, fixed `reason` and an optional
`record:{config,outputBinding}`. Availability is `ready`, `not_initialized` or
`unavailable`. A valid typed record is returned with `unknown_actor` only so the
operator can remove missing members explicitly. Corrupt/unsupported records have
no editable replacement payload and are never silently reset. Initial save sends
`expectedRevision:null` with draft revision 0 and confirms revision 1. Later CAS
requires both expected/draft revisions to match and increments the confirmed
revision. An existing StageId cannot be replaced. Private `monitorId` is null or
bounded to 512 UTF-8 bytes without control characters.

Placement sends `expectedRevision`, actor ID and canonical integer x/y. It
read-through validates current persisted configuration first, changes only the
ephemeral semantic position/sequence and releases that actor's leases. It does
not increment config revision, grant membership or acknowledge physical arrival.
Restart restores safe semantic placements with zero leases/active sessions.

A public envelope carries a fresh observation `instanceId`, availability/reason
and the shared WorldSnapshot. The instance changes on native restart, external
config reload and reconstruction after repair, so a reset sequence cannot alias
an older observation. This UUID is distinct from private runtime/member/lease
capabilities. Only explicit participants receive the common projection; native
World events target their exact `avatar-<botId>` window, and OBS/SSE independently
filters bot scope. Removed participants get a null clearing event. The main
operator receives the separate `cohost://world-state` aggregate. Current Life
lineage/world/action IDs are public domain identities; private monitor names,
native session IDs, credentials, RNG and capability nonces stay out of projection.

## Monitor observation and rendering boundary

The native host enumerates only its own main window's available monitors on the
GUI thread before locking Core/WorldHub. Samples are bounded, generation-ordered
and fresh for at most five seconds. Private monitor choice uses a deterministic
validated fallback. Topology/DPI loss or change revokes old geometry reservations;
a late older sample cannot restore them. Mirrored displays with an aliased
origin/name but conflicting geometry are excluded; identical duplicates are
coalesced so enumeration order cannot change the selected arena. This enumerates output topology only,
without reading other apps, screens, screenshots or session/permission data.

Output events may describe a validated local logical `arena` size, together with
`presentationAvailable:false`. Arena metadata and pure coordinate conversions
are not renderer placement acknowledgements. The optional selected-bot
[Studio World adapter](WORLD-OUTPUT.md) separately registers actual submitted
geometry and reports fenced movement frames. Native fullscreen layout, physical
arrival and OBS World execution remain part of #72; fixed Life avatar windows
are not automatically expanded by this Studio view.

## Validation status

Meaningful local fixture source covers two-actor lineage preservation, exclusive
reservation races, stale sessions/actions/config revisions, timeout and backward
clock behavior, actor removal/rebinding, reload without authority, geometry
invalidation, typed/bounded deserialization, counter exhaustion, public projection
privacy, negative origins, DPI conversion and deterministic monitor fallback.

The standalone actual-source fixture passed all 40 tests (21 world cases and 19
imported Life-domain tests), offline with one build job and no OS calls. Source
hashes were captured before the run and verified unchanged afterward. This
establishes the pure module's behavior only. The composed implementation passed
443 native tests on macOS, including all 10 Hub and 6 production Store regressions
for CAS, live corruption, restart/repair epochs, scope/import isolation and
Core/output fences. All 502 frontend tests, both TypeScript checkers with zero
diagnostics and the production build passed. Existing and foundation dead-code
warnings and the bundle-size warning remain visible.

Synthetic browser fixtures executed the actual editor's save/place operations
and the Studio's navigation, bot switching, dirty-start guard and discard dialog.
Four-language checks used 360/1,100-pixel viewports. Invalid full-page captures
were excluded; raw bottom-frame clipping was not treated as proof of an entire
visible form. Successful centered-control geometry checks are recorded separately.
These fixtures use synthetic native responses and do not establish native
window/monitor delivery or actual 3D arrival.

The bounded native-adapter fixture passed 11/11 executable scenarios, locked and
offline with one build job. It imports the exact production WorldHub state,
WorldStore SQL, LifeStore SQL, Core, domain and coordinate sources. Its minimal
outer Store shell supplies only a private SQLite connection and profile-ID table;
it does not replace SQL/Hub/Core behavior. Cases cover two persisted lineages,
shared scope/privacy/restart, external CAS rollback/reload, live corruption,
Core Stop/output loss, topology/alias fencing, exclusive-slot race, repair epoch
and orphan-ID reservation. Source hashes were unchanged after execution. The
full production Store constructor/import regressions also passed in the composed
whole-crate suite. Real IPC delivery, GUI monitor changes and future measured
rendering/physical presentation remain controlled OS/#72/#75 gates; no user
desktop/session/capture actions were exercised by these fixtures. Platform CI
must validate the final combined head before merge.
