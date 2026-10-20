# Character-life domain contract

The pure Rust module `src-tauri/src/life_domain.rs` implements the contract for [#67](https://github.com/ralphschuler/orbitpal/issues/67). It defines data and deterministic transitions. It does not start autonomy, dispatch actions, open windows, save data, read clocks, request permissions or contact a provider. Persistence (#68), the single native runtime (#69), shared-world resources (#70) and actual execution (#71) build on this contract.

## Identity and state invariants

`ActorId` and `WorldId` are distinct validated types. Their opaque identifiers contain 1–96 ASCII bytes and permit letters, digits, `_` and `-`, matching the existing stored bot-ID contract (including leading `_`/`-`). Existing UUID-style bot IDs fit this grammar. Human names, monitor labels, paths and free-form content are separate data; they must not become IDs by silently trimming, truncating or normalizing invalid values. A future storage adapter must bind the state's ActorId to its actual bot row rather than trusting an imported document's owner claim.

`ActionId` combines an ActorId and WorldId with a positive monotonic ordinal. `EventId` combines the same actor with the accepted transition's revision. Both are data identities, not authorization tokens. Action IDs are allocated only by planning; ordinals never wrap or get reused within that actor lineage. Restore preserves its counters; `LifeState::new` is only for a genuinely new actor, not a restart/reset of an existing lineage. World binding also rejects acknowledgements from another world. The native host must still bind callbacks to its current session/output; replay protection is not an authorization mechanism. Revision, time and action counters are bounded by `9_007_199_254_740_991` so their JSON representation remains exact for future UI consumers. Exhaustion returns an error without changing input state or consuming randomness.

LifeState schema **1** contains only:

- Actor/world IDs, simulation time, revision and monotonic action watermarks.
- Four satisfaction values: energy, nourishment, engagement and social. Values use integer units from 0 to 10,000. No need weighting, decay, punishment or care routine is built into the domain.
- An optional current action and up to 128 recent structured committed events. A terminal action remains informational until the next plan replaces it.

Fields are private. Constructors and Serde deserialization validate IDs, ranges and schema versions. The reducer revalidates state and event values at its mutation boundary. Fixed-point state cannot represent NaN or infinity; `NeedLevel::from_normalized(f64)` explicitly rejects non-finite/out-of-range numeric input before converting a normalized value. Need effects accept integer deltas from −10,000 to +10,000; applying them clamps satisfaction to the valid range. Unknown schema versions, activities, event variants, cancellation reasons and fields fail rather than being guessed or ignored. Errors use fixed categories without echoing payload content.

A decoded state must have coherent actor/world references, action ordinal, timestamps, phase and terminal watermark. Retained event IDs form a strictly ordered contiguous suffix ending at the state revision. Within that suffix, lifecycle order must be valid and its last observed action/phase, plan and known transition timestamps must agree with the current action. A completion record cannot coexist with a rolled-back Started action. The separate resolved-action watermark remains after event eviction and also rejects a phase-only rollback. These checks validate data consistency; they do not authenticate a file against an adversary who can rewrite its entire lineage. Persistence must supply its own row ownership, transactional revision and input-byte limits.

## Pure reduction, time and randomness

The entry point is:

```rust
reduce(&state, elapsed_ms: i64, random: RandomState, event: LifeEvent)
    -> Result<Transition, LifeError>
```

The caller supplies elapsed **simulation milliseconds**, not a Unix timestamp. Simulation time starts at zero. Negative elapsed time contributes zero; one call applies at most 60,000 ms. The transition reports the applied duration and whether it was limited. Excess time is discarded, not saved as a debt or expanded into a loop of missed actions. A Tick changes only the clock and emits no action. This primitive therefore cannot turn absence into retroactive experiences or autonomous work. Future offline policies may adjust bounded values explicitly; they must not fabricate Completed events.

`RandomState::seeded(u64)` is an explicit caller-owned SplitMix64 state. Planning consumes exactly one draw to choose a 16-bit animation/activity variation. All other events leave the stream unchanged. The reducer returns the next random state; a caller that needs continuity must keep that returned value. There is no OS RNG, process-global seed, wall clock, thread, renderer, Tauri dependency, filesystem or network in the module. The same state, elapsed input, random state and event yield identical state, intents and next random state. Random variation never grants permissions or selects an unsupported animation.

One call processes one event atomically. Accepted changes create exactly one new revision and committed record. Zero-duration ticks and effects that change nothing at zero duration are no-ops. Rejected events leave the caller's borrowed state intact. Duplicate/stale acknowledgements are no-ops including their supplied elapsed time: a retransmitted completion cannot advance the simulation clock or produce another event. The native owner must serialize calls (or compare-and-swap revisions); the pure function itself is not a concurrent scheduler or database transaction.

## Lifecycle and example sequence

The closed Activity vocabulary is `move_to`, `rest`, `recharge`, `play`, `look_at`, `wave`. These are semantic categories, not a promise that an asset supports the corresponding animation. Target/reservation resolution and supported variants belong to WorldState and ActionExecutor. No arbitrary text, path, tool or executable can be placed into an intent.

| Input | Required current state | Result |
| --- | --- | --- |
| Plan | No action or a terminal action | Allocate a fresh actor-bound ActionId, store Planned, emit one immutable data intent |
| Start | Same action is Planned | Store Started and its timestamp; no completion credit |
| Complete | Same action is Started | Store Completed and apply its preplanned need effect exactly once |
| Cancel | Same action is Planned or Started | Store Cancelled/reason; apply no completion effect |
| Duplicate Start/Complete/Cancel | Same matching acknowledged phase | Return Duplicate without changing anything |
| Acknowledgement of an older allocated action | A later action is current | Return Stale without changing anything |
| Complete after Cancel / Cancel after Complete | Same terminal action | Return Stale without changing anything |
| Wrong actor/world, never-allocated ID, Complete before Start or Start after terminal | Invalid reference/transition | Return a fixed error |

Planning while an unfinished action exists is rejected. Starting, completing or cancelling never plans a follow-up. The executor will independently authorize the real actor/session/output, check capabilities and own resources; these data transitions are not proof that pixels moved or audio played.

Example, with an initial energy of 5,000, a deterministic seed of 7, and other needs at 5,000:

1. At simulation time 0, Plan Rest with completion energy +2,000. Revision 1 allocates `{actorId:"nova", worldId:"desktop-main", ordinal:1}`, emits one intent and records Planned. Energy stays 5,000.
2. After 1,000 ms, Start that action. Revision 2 records Started at 1,000. Energy stays 5,000.
3. After another 5,000 ms, Complete it after native confirmation. Revision 3 records Completed at 6,000; energy becomes 7,000 and its resolved watermark is 1.
4. Deliver the same Complete again. State, revision, clock, random source and energy remain unchanged, and no intent/event is added.
5. Plan a later action and cancel it. It is visibly Cancelled with no completion reward. Replaying action 1's completion is Stale even after its record has fallen out of the 128-entry history, because IDs are never recycled and terminal watermarks are retained.

The effect is fixed when the plan is accepted; a Complete event cannot supply a larger reward. `ApplyNeedEffect` is an explicit primitive for a future native time/need policy, not a model tool or externally registered command. The module does not attach it to user/viewer content.

## Ownership and public projection

`LifeState::public_projection()` creates an explicit `PublicActor` whitelist: schema, actor/world IDs, revision, simulation time and optional action ID/activity/phase. It omits needs, completion policy, variation and event history. PublicActor is serializable observation data; it cannot be deserialized as a LifeState or used as a permission/session grant. Sharing it later still requires the native world's explicit participation rules.

Neither state nor events contain active sessions, reservations, grants, credentials, screenshots, conversation text, persona/AGENTS instructions or prompts. Strict unknown-field rejection prevents these being silently round-tripped as extensions. Do not append complete bot profiles to these records or expose full LifeState as a social projection.

Deserializing a current Planned/Started action does not dispatch it. For #68/#69, restored pending work must be cancelled with `CancelReason::Restart` (or retained purely as explicitly interrupted history) before a new explicitly started session may plan work. Persisted life values and action identities do not reactivate autonomy, microphone, capture, MCP, Skills or model access.

## Validation boundaries

Focused tests use only the actual module, Serde/JSON, controlled integers/randomness and in-memory data. They cover deterministic replay and a fixed RNG vector, every lifecycle class, reward idempotency before/after retention eviction, cancelled completion, malformed imports, unknown fields/schema, non-finite numeric boundaries, time/counter exhaustion, actor isolation and public projection. A long deterministic trace checks bounded needs/history and ordered revisions after round-trip serialization.

The module is registered in the normal Rust library, so its tests run with `cargo test --manifest-path src-tauri/Cargo.toml --locked`. A small temporary crate may also import this exact source for a portable, renderer-free fixture run; copied or reimplemented reducers are not substitute evidence. Full frontend/native CI remains required for integration. Passing these pure tests does not establish storage migration, hidden-Studio scheduling, rendered actions, self-care, social scenes or native desktop lifecycle acceptance; those remain their separate issues.

Implementation and test preparation used substantial AI assistance.
