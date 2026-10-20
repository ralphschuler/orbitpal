# Native action executor foundation

`src-tauri/src/action_executor.rs` is the pure state machine. The native
Core owns its process-only jobs and bounded operator intents. The LifeRuntime
loop polls it through `life_runtime/action_host.rs` under Core → WorldHub → Store;
`action_store.rs` provides transaction-scoped change fences. Models, MCP clients,
skills and imported profiles cannot start actions or supply effects.

The initial product path is an explicit **Quiet rest / Stille Pause** button in
an already started Life session. It is a ten-second local timed pause at the
actor's current canonical position, with **zero need effect**. It claims no
visible pose, animation, arrival or recharge. It neither invokes providers nor
requires a dialogue model, voice or usage-budget reservation. It does not start
Life, enqueue a successor or implement the later self-care planner.

The optional [Studio World adapter](WORLD-OUTPUT.md) supports a one-use operator
MoveTo for the selected Studio character in an already active Life session.
Registration, current measured geometry and native route authority are required
before Plan allocation. Arena metadata alone is not visual capability. Other
participants remain mirrors; native overlay/OBS and other visual activities stay
unavailable. Visual Offers never go to the legacy speech/character renderer.

## Fixed policy and persistent identity

The catalogue accepts only the existing MoveTo, Rest, Recharge, Play, LookAt and
Wave activities. Typed goals are a canonical point, explicit slot/kind, selected
actor, or current point. Rest/recharge require a Rest slot; play requires a Play
slot. A slot goal also requires an opaque native SlotLease. The host must validate
that exact lease and canonical target through WorldHub before dispatch and
completion; this pure executor cannot inspect current native topology.

Visual rest currently proposes +500 energy, recharge +1000 energy and play +500
engagement; move/look/wave propose zero effects. These are explicit native policy
constants, bounded by the existing fixed-point needs scale. They are not supplied
by a model or output, and new priority/self-care routines remain separate work.

`PreparedPlan` takes an actual current Life state/random snapshot, creates the
fixed Plan event and uses the pure domain reducer **only for Plan** to determine
the exact allocated ActionId/intent/policy. It never simulates Complete and never
changes the supplied state. The native host must commit that exact Plan through
Store CAS. `install_plan` requires an exactly matching confirmed Life state
before it can emit an Offer. Changed effects/variation, unconfirmed state and
wrong actor/source world are rejected. The source Life WorldId remains distinct
from shared StageId and is never rewritten.

## Execution and acknowledgements

The pure state machine distinguishes AwaitingDelivery, Delivered, Started,
Paused, Completed, Cancelled and Failed. Dispatch is not delivery. Started and
Completed require ordered actual output receipts, then separate native Store
Start/Complete transactions. No output receipt applies a need effect.

NativeAuthority is deliberately neither serialized nor deserialized. It binds a
native SessionStamp, source WorldId, stage/config/topology revision, selected
OutputStamp and asset revision. The host must derive and check a current
Mode::Life session and availability each time. Local execution must not reuse
the speech Claim/model/speak/due-time conditions or request/voice budgets.

Wire Offer/Pause/Resume/Terminal messages contain a generated delivery ID, exact
ActionId and public typed parameters. Output routing stays in a separate native
Dispatch envelope. No Life session stamp, credential, asset path, membership or
lease capability is serialized. An authenticated output route resolves the
native job; a wire `source`, `origin` or `session` string grants no authority.

Receipt sequence numbers are positive, bounded and monotonic per delivery.
Duplicates do not restart animation or update timers. Completed-before-Started,
wrong evidence, wrong arrival point, expired delivery and changed native fences
are rejected. MoveTo requires actual arrival evidence near its target; LookAt
requires settled orientation; the other visual activities require an actual
finished behavior plus native minimum active duration. The #72 adapter must
validate mapping, measured geometry, visibility and capabilities. The current
#70 arena-only payload is not a compatible visual adapter.

The selected output must send progress normally every second and never more than
five seconds apart. Pauses exclude active time. A long unobserved interval or
bounded deadline fails/cancels the job, without sleep catch-up or success.
Pause is at most five seconds and does not extend the total run deadline.

## Store boundary and effects

CommitRequest is internal and non-deserializable. It holds the exact event,
native job identity, current confirmed pre-state and expected revision. The host
must obtain a non-Clone, one-use CommitPermit through `authorize_commit` against
the exact current native Life state. Only this permit exposes the event and
revision for writing. Authorization checks the current high-water clock, pending
request, native fence, deadline and progress inside the IMMEDIATE transaction,
after acquiring the DB write lock. A borrowed read-only Connection also checks
the bounded, typed persisted profile SHA-256 token and complete validated World
record against the native job. A changed profile/stage/config/output binding
cannot slip through while the database lock is awaited. The callback never
reenters the Store mutex. No await, GUI,
provider call or Core re-entry is permitted between admission and that write.
Then `confirm_commit` consumes the permit with the actual result.
Current periodic ticks can be
accepted before making a request; the executor does not reuse a stale dispatch
revision. A single controlled same-action refresh can rebase a revision conflict;
it preserves the original proof/deadline and invalidates the old permit, without
allocating another Plan or effect. A receipt verifies actor/world, complete frozen policy intent, exact
new revision/time, phase, last committed event and expected needs change. This
verification does not apply an additional reward or replace the native Store.

The existing domain/SQLite implementation remains the durable once-only owner.
It prevents duplicate effects even after bounded event retention/restart.
Completed is installed only after a matching commit. Stop invalidates pending
success requests; a late acknowledgement cannot revive them. Native integration
must check current authority **before** writing Complete under the same Core
critical section. A candidate LifeState produced by a fixture or supplied on a
wire can never be represented as a real Store success by the host.

The authorization callback is the admission linearization point. Expired requests
cannot obtain a write permit, including when a rejected future receipt advanced
the clock and a later caller supplies a backward raw timestamp. A genuine result
from the admitted synchronous write remains committed if the clock crosses a
deadline while SQLite commits or the result is delivered; post-write handling
cannot retroactively undo its effect or pretend that it failed. Stop/expiry
revokes unconsumed permits. The host must never retain one across asynchronous
work or an unlocked interval. Existing Stop cancellation can be reconciled only
from the exact validated native terminal Cancel state for the same frozen intent.

Cancelled/Failed publish immediate terminal tombstones and return explicit
Cleanup for exact slot release and Store cancellation. They are not advertised
as durably committed until a separately confirmed cancellation transaction.
Admission and membership forgetting remain blocked until that confirmation and
resource cleanup. Failed maps to existing schema-1 Cancel reasons; no new Life
schema/phase is silently introduced. Restart/shutdown wiring must invalidate all
outputs and perform existing non-rewarded recovery; no jobs or leases persist.

## Bounds and supported fallback

There is at most one retained job per actor, at most 64 jobs, 256 ordered output
messages and 64 cleanup results. Admission fails when bounded queues cannot
reserve termination capacity. Controls for a job coalesce; Stop removes unsent
Offers and emits a higher-sequence terminal tombstone. Receivers must retain the
latest sequence/terminal state for the current connection, so a delayed Offer
cannot restart stopped work. No retry/successor action queue exists.
An old output's terminal remains queued even when the next action uses another
output; actor-wide Offer coalescing must never erase that tombstone.

The monotonic clock has a high-water mark even on rejected receipts. Clock or
identity exhaustion revokes jobs, clears offers, returns output invalidation and
resource cleanup, and refuses new work rather than wrapping identifiers.

NativeRestDwell is a separately labeled, explicit opt-in fallback. It supports
only Rest at a known current point, has zero need effect, emits no visual Offer,
claims no station arrival/animation and completes only after ten seconds of
bounded actively sampled native time plus the real Store Complete transaction.
It cannot substitute for movement, waving, play or recharge. Missing visual
capabilities otherwise fail honestly. Existing chat emotes retain their own playback identity. Native local jobs and
intents explicitly postpone autonomous speech/chat; a foreground dialogue
interrupts local work through its exact native cancellation/Store path first.

## Operator, output and lifecycle boundaries

`get_life_actions({botId})` and `start_life_rest({botId,ui:{connectionId,sequence}})`
are main-window commands; neither is in the Control-MCP allowlist. Read-only
status polling never counts as operator activity. `cohost://life-actions` is sent
only to the main window. The version-1 envelope includes a public observation
instance/sequence, bot ID, new-action readiness/reason, actual durable confirmed
action and separately the runtime status. It excludes session/lease authority,
asset paths, profile tokens, effects and internal completion permits. Unavailable
status may still retain truthful confirmed/runtime data.

An exact, fresh, clear Studio report is captured before ordinary command
revocation into a private, one-use intent lasting at most five seconds. Only
that command's own synthetic block can be recognized. Real edits, modal/busy
state, other operator activity, disconnect, Stop, profile/session changes and
system blockers revoke it. A later clear heartbeat cannot revive it. Final
native checks happen after asynchronous availability/topology observation and
again at transaction admission. UI pending-button state alone is not a fabricated
blocked report or a permission grant.

The timer supports actual host-known Studio/native-overlay windows only. An OBS
link enabled in configuration is not a live connection, so OBS quiet rest is
unavailable until #72 supplies registered output presence. No GUI getter, await,
provider call or event publication occurs while Core/World/Store locks are held.
Output observation is checked before native polling; session/profile/asset and
atomic availability cancellation are checked before each write. A missing or
old UI heartbeat, clock gap, sleep/lock/unknown state, output loss, profile/world
change or Stop cancels without catch-up completion. No job/intent/output lease
is restored on restart; existing per-bot Life recovery cancels orphaned actions.

Per-bot native jobs and targeted Core/host Stop are isolated. The current App
reports **all genuine trusted human input as global foreground activity** before
handling individual controls. A UI click to start a second rest or stop one bot
therefore interrupts other local work through that global policy. The native
two-actor fixture establishes owner/cleanup isolation; it does not claim that
two independent UI rests survive subsequent human interaction.

If a failed optional Life store cannot persist cancellation, the actual pending
Life action remains visible rather than being reported as cancelled/completed.
The affected session is revoked. Only after deriving exact Core-session and
World unbinding and stopping the internal native timer may the host retire that
failed runtime job. Retirement preserves persistent Life bytes, ActionIds and
ordinals, queued terminal tombstones and monotonic delivery IDs. It retains at
most 64 public failure snapshots and gives no reusable authority to old keys or
permits. Reusing that actor still requires a healthy Store/new explicit session
and real orphan cancellation; retirement cannot skip an unresolved action into
a new Plan. Other healthy actors are not blocked forever by 64 failed stores.

## Validation status

Meaningful actual-source fixture tests are written for fixed policy, no-dialog/
voice dwell, lifecycle order, committed effects once, actual pure-world lease
cleanup, native fence isolation, duplicates/sequences, ticks/current revisions,
Stop/pause/pending completion, timeout, unsupported assets, bounded fallback,
clock/counter exhaustion and wire privacy.

The standalone offline actual-source fixture passed all 63 tests: 23 executor
cases, 21 imported World cases and 19 imported Life-domain cases. It used one
build job, an isolated target and only existing pinned serde dependencies. All
eight imported Rust file hashes matched the pre-test manifest afterward. The
initial module-path and Slot-constructor fixture compiler failures are retained
in the work evidence; no test failure was hidden. This gate establishes the pure
contract only. The native host/SQLite regression source additionally covers real zero-effect
Plan/Start/Complete and duplicate polling, intent revocation, targeted native
Stop isolation, current native fences, database-wait changes and cleanup failure.
Its execution receipt is recorded separately at integration; this paragraph does
not assert a repository-wide run. Full production Store/Tauri compilation, the
composed four-locale UI gates and real #72 output acceptance remain distinct
required checks.

The bounded native-host fixture subsequently passed **18/18** scenarios, using
production Core/action-host/Life-SQL/World-SQL bodies and the same regression
source with a minimal outer Store shell. It verifies the 64 failed-store
retirement case followed by a healthy actor, rejected old completion permit,
actual Plan-install failure and failed-cancel retention, as well as the cases
above. Additional actual SQLite-wait cases check a changed selected Studio bot
and output revocation during both Plan and Complete admission. A bounded
non-Core registry cancels exact native jobs synchronously on known native window
close/destroy, main hide/show, Stop and operator/UI interruption before those
paths wait for Core. GUI sampling is fenced by a registration generation; stale
window observations cannot grant a replacement job. The output generation is
retained from intent preparation, before asynchronous probes, so an intervening
window/Stop/connection revocation cannot be hidden by sampling a newer ticket.
Registration requires the same native connection and no blocked/activity report
after the intent proof; later clear heartbeats remain valid. A cancellation-only native
UI nonce/sequence mirror prevents old reporters, stale blocked reports or old
disconnects from revoking a new job; current real blockers still latch before
Core acquisition. No native-window control
API is executed by these synthetic tests. Successful terminal jobs also release
runtime slots after cleanup, demonstrated by 64 completed actors followed by a
healthy 65th, retaining at most 64 public terminal snapshots.

One missing test import and one test-draft CAS revision setup error were
corrected; the production assertions were not relaxed. This is not a substitute
for the composed full production Store/Tauri test run.
