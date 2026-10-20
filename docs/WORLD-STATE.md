# Shared native stage foundation

The pure `world_state` module describes a common stage with explicit actor
membership, canonical positions, safe regions, rest/meeting/play anchors and
exclusive fixed slots. It does not inspect the desktop or call capture,
permissions, network, Tauri, storage or rendering APIs.

This is the foundation for issue #70. It is not connected to a native WorldHub,
Store, operator IPC or the UI yet. It is not registered in the application crate
until native LifeRuntime integration is ready. No running character acquires a
new behavior from these files alone.

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

`WorldState` is session-only and has no deserializer. The native owner must mint
a fresh `RuntimeEpoch` on every restart. New state restores no memberships with
authority, positions, occupancy, leases, sessions or action acknowledgements.
The runtime cannot create or mutate a LifeState. Actor binding accepts a native
session stamp and an optional already-validated LifeState. A not-initialized actor
can have public placement without being implicitly initialized. Its native
session operation is distinct from a Life action and cannot grant a Life effect.

The future native adapter must validate that a session stamp is current under
Core before calling this module. These types are internal contracts, not a
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
The future native integration must also invoke these boundaries for action
cancel/finish, output loss, shutdown and membership removal.

`validate_reservation` only confirms a place and returns its canonical target.
It does not start or complete a Life action. A future executor must additionally
check native Core authority and commit the original ActionId through the existing
Store compare-and-swap reducer. Persistence failure grants no reward. Sequence
or identity exhaustion fails closed and revokes runtime authority rather than
wrapping/reusing an identifier.

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

Native persistence remains to be implemented as a separate optional versioned
subsystem. It must save only validated WorldConfig under an atomic CAS, keep
ordinary profile/manual repair usable on unsupported/corrupt world data and never
reset existing per-bot Life storage. Ordinary bot export/copy/import must not
inherit global membership, runtime state or reservations. Rehydration requires
existing actor IDs and fresh native source-world/session checks. World sync
must not grant screen access or start recording.

## Validation status

Meaningful local fixture source covers two-actor lineage preservation, exclusive
reservation races, stale sessions/actions/config revisions, timeout and backward
clock behavior, actor removal/rebinding, reload without authority, geometry
invalidation, typed/bounded deserialization, counter exhaustion, public projection
privacy, negative origins, DPI conversion and deterministic monitor fallback.

The initial foundation has not yet been compiled while the shared native
LifeRuntime suite owns the resource slot. Full native registration, Store/Hub/API
integration, four-locale UI, exact combined frontend/native checks and supported
OS presentation acceptance remain required before closing issue #70.
