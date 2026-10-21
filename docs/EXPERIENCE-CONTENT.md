# Everyday experience and declarative content

The Life stage stays the visual setup and explicit start point. Everyday preferences, social preferences and care settings are separate drafts with readback, revision checks, a shared save action and explicit discard. Navigating Studio keeps these drafts. A bot change requires saving or discarding them first. The implementation was substantially AI assisted.

## Preferences and interaction

`experience::Preferences` contains only saved choices. `ExperienceRuntime` holds process-only pause and pending impulse queues. Missing preferences inherit Stream for an existing profile with `autopilot.speak=true`, otherwise Focus. This preserves a profile's intended output policy without starting Life. Quiet and Focus deny unsolicited speech/text from every autonomous source; Stream still requires all existing activation, output, budget and privacy checks.

Studio requests pet, play, home and pause through `request_character_interaction`. The root native adapter uses the existing action executor and current native owner. UI feedback distinguishes admission from completion. These controls do not create screen, microphone, generic tool, provider or output permissions. The native avatar context menu and click-through restoration require separately qualified renderer integration; this Studio keyboard route is the fallback, not proof of that native interaction surface.

## Facts and album

Only the crate-private native `Store::record_experience_fact` hook records facts from actual confirmed transitions. Each fact needs stable actor IDs and native source event IDs. Use a stable distinct fact ID for each real transition (planned, started, completed, declined or cancelled). Never record offline reduction, restart recovery, guessed activity or model prose as a completed experience. No IPC inserts facts.

Facts start private, are deduplicated, limited to 256 per bot, and trimmed to 30 days on the next native event. Readback rejects corrupt/oversized data. The album groups only completed facts, without inventing empty days. Public preview includes explicitly shared facts only. Deletion/reset and explicit sharing are local Main operations. Root must emit a fresh `experience-state` projection after recording a fact. Social structured memories remain bot-scoped and use their separate explicit sharing controls.

## Native platform impulses

The authenticated platform adapter may construct a `NativeImpulse` only after its actual channel connection, origin and current generation are verified. There is deliberately no renderer/viewer IPC that submits a native event. A platform adapter translates a configured closed trigger into greeting, play or a vote; arbitrary chat text is never an authority or setting.

`ExperienceRuntime::admit_native_impulse` and `take_native_impulse` enforce current preferences/pause. Queues are isolated per owner, at most 16 pending, with 30-second maximum lifetime, bounded duplicate history and per-participant cooldown. Duplicate IDs stay duplicate across reconnect generations; a stale generation never executes. The root native consumer must recheck actual Life, participant availability, scene/action admission and speech priority before consuming a proposal. Focus, pause, Stop and profile/route changes cancel pending proposals. No queued proposal grants activation or implies completion. Actual Twitch/YouTube channel qualification remains separate from these controlled policy tests.

## Content packages

The example `public/content-packs/desk-garden.json` adds a character template, toy, existing-capability motion mapping and bounded activity as data. Preview validates a complete manifest through the same native parser used for installation. An installed pack starts inactive. Activation is an explicit revision-checked operation and does not start Life.

Schema/runtime version 1 accepts only closed actions/conditions, validated namespaced IDs, bounded weights/durations, declared dependencies and built-in model references. It does not accept file paths, remote assets, scripts, executable code, arbitrary effects or extra permissions. The package extends data and existing supported capabilities; importing new binary model assets continues through the existing explicit avatar importer. The preview lists actual templates, object offers, dependencies and local resources. There is no hidden download.

The native planner consumes active manifests from `Store::content_packs()` and must keep package ID/version on proposed work. The root integration maps supported typed offers onto real current slots/actions; it cannot treat the catalog as Ready, a lease or an ACK. Before deactivation, `content_pack_deactivating` cancels dependent real work and releases real resources, then the Store revision check commits the inactive state. A dependent active pack blocks removal of its required dependency. Upgrades require prior deactivation and a strictly newer manifest version. Unsupported schemas are rejected explicitly; saved references are preserved for fallback or pause rather than silently rewritten. Actual animated package qualification remains a native integration check.

## Desktop geometry boundary

The current capability projection reports no verified external-window/cursor provider. Disabled UI explains the manual visual-anchor fallback. It can always turn off an already-saved opt-in. `desktop_affordances::GeometryLease` is only a short-lived pure mapper for a future genuine native source. It stores no geometry on disk, no window names/content and no image. A source generation/display join, physical work area, nonregressing time and 1.5-second freshness gate every anchor. Uniform physical DPI scaling preserves canonical positions; negative monitor origins are supported. Cursor proposals additionally require Stream and separate opt-in. Unknown/expired/lost source produces no anchors. A native platform provider and real platform acceptance are still required; no foreign-window input or manipulation belongs to this feature.

## Shared integration hooks

Root integrates these declarations and registrations, keeping one shared native authority:

- `lib.rs`: declare `experience`, `experience_commands`, `content_packs`, `desktop_affordances`; manage `experience::ExperienceRuntime::default()` in the shared Builder.
- `storage.rs`: `#[path="experience_store.rs"] mod experience_store;` as a child so bounded SQLite methods keep the existing Store ownership.
- Register `experience_commands::{get_experience_snapshot,set_experience_preferences,set_experience_pause,request_character_interaction,delete_experience_fact,set_experience_fact_shared,list_content_packs,preview_content_pack,install_content_pack,set_content_pack_active,delete_content_pack}`. Main/updater checks are in the wrappers; the root operator classifier must permit only the appropriate read/write surface.
- Implement async `life_runtime::{experience_preferences_changed(&AppHandle,&str,&Preferences),experience_pause_changed(&AppHandle,&str,bool),content_pack_deactivating(&AppHandle,&str)}` returning `Result<(),String>`. Changed settings immediately cancel incompatible pending work; no resume or native receipt is fabricated. Persisted choices remain authoritative if an invalidation reports an error.
- Implement async `request_character_interaction(&AppHandle,&str,&InteractionRequest)->Result<Value,String>` using the existing executor. Closed result: `{version:1,botId,kind,status:'accepted'|'declined'|'waiting'|'cancelled',reason:null|'busy'|'resting'|'unavailable'|'operator'}`.
- Record native confirmed facts through the Store hook and publish a new Main projection. Consume native platform impulses through the runtime only after real origin validation. Unwired hooks must fail truthfully, never return a simulated success.
- Care UI uses root's `get_life_care_preferences`/`save_life_care_preferences`, including `restoreUnits`. It never applies rewards itself. Social UI uses the agreed bot-scoped `social_commands` endpoints; its writable development projection preserves earned hobby/goal progress.

Focused frontend tests cover scope/validation, phase distinctions, explicit public sharing, recap aggregation, care numerical bounds, social development/routines, Snack wire/editor presence, localization and absence of authority-grant calls. Full native/frontend/build checks are centrally batched after shared integration. No actual host GUI, provider/channel or native acceptance was exercised by this branch's author.
