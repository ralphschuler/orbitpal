# Native Life and Ambient sessions

OrbitPal owns one native runtime for all explicitly started bot sessions. Starting
Ambient enables the existing wandering/gesture presentation and optional
spontaneous speech. Starting Life initializes or loads that bot's confirmed local
Life state and runs bounded native simulation checkpoints. Life disables Ambient
wandering/gesture ownership. Native care, exploration and social pulses now choose
bounded proposals and submit eligible work through the existing World and action
executor. A proposal is not a reservation, presentation acknowledgement or reward.

Sessions start only through the Studio operator command. Profiles retain their
configuration and confirmed per-bot Life state, but activation, session IDs,
quiet deadlines and pending turns are never restored after app restart. A bot
without a dialog model can run local Life. Ambient speech requires a model; CLI
providers may use their supported default model.

## Starting from the desktop layout

In Autopilot, choose characters in the visual editor and drag their handles to
arrange the layout. Add rest, snack, meeting or play places from the tray and resize the
movement area with its corner handles. Arrow keys move the selected handle;
Shift uses larger steps. Escape cancels an unfinished drag, and Delete removes
an eligible selected item. No coordinate entry is required.

**Save** keeps the layout without activating it. **Save and start Life** awaits
the configuration compare-and-swap, its readback and all pending actor placements
before asking the native host to start the saved participants. The first explicit
start creates a missing local Life state without an additional internal-state
checkbox. Failed saves keep the remaining edits. Stop is available during start.

Starting reconciles actual native character windows. The display route belongs
to the current session; it does not change the profile's saved output mode or
streaming flag. New session-created windows use the selected monitor and saved
relative placement, clamped to its real bounds. Existing independently opened
windows retain their placement. This is initial window placement, not a World
MoveTo or arrival acknowledgement. Life moves only through supported native-issued
World actions; Ambient retains its separate roaming behaviour.

Session-created constructors retain temporary placement/close provenance until
they are recreated. If one is explicitly retained for ordinary overlay output,
normal saved-bounds and close-mode tracking resumes with its next ordinary
constructor. Independently opened ordinary windows keep their normal tracking.

The host publishes ordered opening, visible, unavailable and inactive output
states. Visible means that the native window is visible; the existing renderer
presentation gate still decides when actual speech, subtitles and emotes begin.
Replies carry the resolved output and exact session route. Late jobs cannot use a
stopped or successor route, and Studio's model-audio mirror stays muted while the
desktop owns playback. A missing window gives an actionable start error.

A layout grants no screen capture, tools, skills or provider permission. Existing
per-bot grants, quiet periods and shared budgets still apply.

## Ownership, scheduling and cancellation

The native runtime keeps one active mode per bot and one spontaneous turn across
all bots. It selects an eligible due bot fairly and uses the existing
`AutopilotHub` rolling hourly admission and minimum spacing. Model/tool responses
do not grant more turns or reset those limits. The Studio displays actual native
attempts, limits, remaining quiet time, the pending turn ID and which other bot
owns a turn. Fair automatic platform-chat scheduling remains separate.

Foreground messages, chat activity, settings edits, dialogs, microphone activity,
updates, blocked output and availability changes postpone spontaneous work and
begin a fresh quiet period. Missing UI reports fail closed. Reload and reconnect
create a new native connection identity; old sequence numbers, sessions and turn
replies cannot reacquire a slot. The UI presents replies and acknowledges audio;
it never owns a second spontaneous provider loop.

Stop, mode replacement, profile/asset changes and shutdown invalidate the matching
native identity and cancellation flags. Late provider results and playback timers
cannot affect a newer turn with the same display identifiers. Repeated identical
Start commands preserve the existing session. An abandoned/failed Start removes
only its own pending intent. Replacing a Life session cancels any nonterminal
confirmed action without completion credit; restarting Life cancels interrupted
nonterminal actions before starting a fresh session.

Checkpoint writes use the per-bot persistence compare-and-swap boundary. Normal
simulation ticks do not invalidate a speech session. Deferred writes keep their
previous checkpoint. Storage failure pauses the affected bot; it does not reset
or fabricate another bot's state. Confirmed lock/sleep rebases time, clock gaps
advance at most one bounded checkpoint, and there are no catch-up speech bursts.

Care runs only for an explicitly started Life session with a current native owner,
fresh accepted output and the existing UI/system admission fences. Confirmed needs,
saved care preferences and currently free, reachable compatible places determine
its proposal. Supported work includes rest/recharge, snack, play and social care;
an allowed Here fallback does not claim a station visit and has a reduced effect.
When care has no choice and no need is urgent, an active Explore goal may propose
a reachable place through the same native MoveTo path. Local routines and earned
preferences affect choices without creating new capabilities. Quiet and Focus
still permit silent care while suppressing unsolicited conversational output.

The social pulse proposes bounded greetings, shared play or rest for two eligible
participants on the same current World. It uses needs, interests and local routines,
then rechecks actual owners, distinct compatible free places and busy state before
inviting. Acceptance, approach and execution remain separate phases; both exact
actions must have real committed completion evidence before shared success or
relationship/album credit. Cooldown/backoff survive temporary observation gaps,
and each execution binds its actual frozen native job deadline once. Stop, source
loss, timeout and participant changes retain the ordinary cancellation and resource
cleanup paths. These pulses require no model, voice, capture or provider account.
See [Social Life](SOCIAL-LIFE.md) and [World output](WORLD-OUTPUT.md) for the checked
action, duration and persistence boundaries. Their implementation does not establish
the real native window, hidden/background or OS lifecycle acceptance below.

## Window and presentation availability

While Studio is visible it reports a fresh native nonce, increasing sequence,
blocker state and operator activity. Reports expire after five seconds. Native
commands also revoke a clean hidden capability before foreground operations;
plugin-owned dialogs must first await a blocked Studio report.

A native tray hide may preserve a clear capability only after consuming a fresh
clear report, actually hiding that window and checking the unchanged generation.
A missing heartbeat or destroyed webview never implies clean hide. Showing,
reloading, reconnecting or foreground native activity revokes it. A webview cannot
activate a session through a heartbeat.

Studio playback for autonomous turns has the same bounded 180-second exact-job
watchdog as overlay/OBS. If no output completion arrives, the exact job is
cancelled, retained audio is released and the native slot can finish. A new job
or another bot is unaffected. Manual Studio playback keeps its existing lifetime.
Reconnect does not replay a missed reply. This bounds a lost hidden/reloading
Studio acknowledgement; it does **not** prove background hidden-webview audio
or rendering on every supported platform.

Emote SFX leases keep their own IDs. An optional parent `speechId` is associated
only after the host matches a live native speech job, bot, output and current
claim. Missing, incorrect, manual or stale parents retain conservative blocker
semantics. Renderer-provided IDs confer no control authority.

## Native availability and network admission

Availability is a closed public projection: state (`awake_unlocked`,
`suspended_locked`, `unknown`), reason, generation and source. Raw OS dictionaries,
user IDs, session names and native payloads are not retained or emitted.
Observers register before initial sampling. Blocking events invalidate pending
leases immediately. Clear events require a fresh fenced observation. Concurrent
refreshes are serialized; an older clear cannot discard a later blocking result.

Unknown permits local pure Life ticks, persistence and Stop. Confirmed lock/sleep
pauses local ticking and rebases elapsed time. **Both Unknown and blocked deny all
autonomous provider, TTS and Hindsight admissions**, including automatic chat,
CLI providers, loopback endpoints and configured proxies. A local-looking label
is not proof that a dispatch stays local. Explicit manual operations keep their
existing permission and cancellation rules.

An internally minted authority binds the trusted native origin, bot, cancellation
token and optional Life claim. Each dialog round, TTS cache miss and Hindsight
recall/retain obtains a fresh native observation after credential resolution and
preflight, immediately before metered transport. The actual operation rechecks
the generation/cancellation permit. Events revoke the same token while a request
is awaiting completion. Reused cached audio creates no new TTS network charge.
The pure usage ledger remains independent of app handles and OS adapters.

This is a best-effort userspace dispatch boundary. Sampling and sending are
separate operations; notification latency and already transmitted requests cannot
be recalled. Opaque CLI/provider-internal calls retain the accounting limitations
documented in [autonomous usage budgets](USAGE-BUDGETS.md).

### macOS compatibility boundary

`CGSessionCopyCurrentDictionary` obtains the caller's session. Its public
console/login fields do not themselves guarantee screen unlock. OrbitPal isolates
the private `CGSSessionScreenIsLocked` encoding and implementation-detail
lock/unlock notifications. It adds public AppKit protected-data, workspace sleep
and session-active blockers. It adds no IORegistry fallback, screen capture,
Accessibility permission, Keychain probe item, root daemon or OS setting change.

The lock flag is explicitly `Absent`, `Boolean(false)`, `Boolean(true)` or
`Invalid`. A **present, exactly typed own-user session**, with console and login
exactly true, public protected data true, live observers and no sticky blocking
event, may accept Absent or false. True blocks. Nil dictionaries, foreign users,
missing required context, wrong CF types or unavailable public state remain
Unknown. A received lock event stays blocking until unlock plus a fresh sample;
a later absent flag cannot erase it. Fresh complete Absent/false supports ordinary
startup without requiring a future unlock notification.

This absence policy is an engineering compatibility inference, informed by a
bounded current-process observation and a maintained primary implementation; it
is **not an Apple documented screen-lock guarantee**. No third-party code is
copied. Primary references:

- [Apple current-session dictionary](https://developer.apple.com/documentation/coregraphics/cgsessioncopycurrentdictionary())
- [Apple protected-data availability](https://developer.apple.com/documentation/appkit/nsapplication/isprotecteddataavailable)
- [Apple DTS on lock/protected-data limitations](https://developer.apple.com/forums/thread/850327)
- [Telegram Desktop's typed initial lock test](https://raw.githubusercontent.com/telegramdesktop/tdesktop/v7.3.1/Telegram/SourceFiles/test/test_console_lock.cpp)
- [Apple XNU private console keys](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/iokit/IOKit/IOKitKeysPrivate.h)

### Windows and Linux

Windows owns a nonvisible top-level observer window on its dedicated thread. WTS
current-process-session and suspend/resume registration precede the first fresh
sample. Inactive, locked, malformed and failed observations deny admission. Stop
unregisters notifications and destroys only that owned window. No user window or
process is controlled.

Linux owns a bounded login1 monitor connection. It resolves the current process's
session, pins the service owner, subscribes before sampling, validates uncached
state and handles sleep, session removal and owner changes. Disconnects become
Unknown; reconnect requires a new fresh probe. Stop owns its tasks/streams and
closes that connection. No session/system-bus query is needed by synthetic tests.

## Operator IPC and public events

Only the main Studio window can call `set_life_mode`, `connect_life_ui`,
`report_life_ui`, `disconnect_life_ui`, `get_life_runtime` or `get_life_bot`.
Control-MCP/model tools cannot activate a Life or Ambient session. The legacy
`set_autopilot` operator command delegates to native Ambient ownership; the old
renderer `send_autopilot` dispatch is rejected.

`get_life_bot` returns confirmed public actor availability (`ready`,
`not_initialized`, `unavailable`). `get_app_state.lifeRuntime` and main-only
`cohost://life-runtime` carry current native session snapshots. A bot's output
receives only that bot's public projection. Main-only `cohost://life-turn` events
carry `started`, `reply`, or `finished`, with exact bot/session/revision/turn ID;
replies use the ordinary speech-audio retrieval and acknowledgement protocol.
No RNG state, private grants, instructions, history or credential values are
added to those projections.

## Verification boundaries

Pure fixtures cover identity, quiet periods, no-model Life, clocks, per-bot
isolation, stale starts, reconfiguration, bounded watchers, observer fences and
availability parsers. Private in-process mock login1 peers exercise Linux
protocols without the real system bus. macOS and Windows source/ABI compilation
do not execute their native session APIs.

Real controlled GUI acceptance remains required on minimum/current macOS with
Intel/Apple Silicon, Windows and supported Linux desktops: unlocked and already
locked cold start, lock/unlock, real sleep/resume, user switching, reconnect,
hide/show and output-client loss. Hidden Studio delivery and actual login-window
notification behavior have not been established by parser/compile fixtures.

The native ownership implementation is tracked by issue #69. The real-device OS
availability/notification matrix remains a separate acceptance gate in
[issue #75](https://github.com/ralphschuler/orbitpal/issues/75); fixture or compile
success must not be reported as completion of that matrix.
