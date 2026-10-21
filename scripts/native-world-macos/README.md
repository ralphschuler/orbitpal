# Disposable hosted-macOS Native World UI acceptance

Source prepared; **no real GitHub-macOS fixture run has passed yet**. This manual
workflow is registered on the default branch only after normal source review and
the four required checks. A later dispatch must select the actual reviewed ref:
`reviewed_sha == github.sha == checkout HEAD`. The explicit allowed refs are main,
the current reviewed `feat/parallel-backlog` integration branch, and the dedicated
`test/native-world-macos-acceptance` branch. There is no alternate event trigger.
The current main branch may lack the opt-in Native World fixture; it reports
`sourcePresent:0` and fails rather than building/running the shipping application.

GitHub's published macOS image already provisions accessibility for
`/usr/bin/osascript` and Apple Events to System Events. Its [image template](https://github.com/actions/runner-images/blob/main/images/macos/templates/macOS-15.arm64.anka.pkr.hcl)
uses the upstream [TCC configuration](https://github.com/actions/runner-images/blob/main/images/macos/scripts/build/configure-tccdb-macos.sh)
and automatic login. **This workflow never executes those configuration scripts,
reads or edits a TCC database, requests consent, enables automation or changes OS
security/session settings.** Published image source is not actual job evidence.

The first five-second client probe checks existing `AXIsProcessTrusted`,
`AEDeterminePermissionToAutomateTarget(..., askUserIfNeeded:false)`, a current
same-UID console/login session and an available display. Missing/unknown native
bridging, permission or GUI capability fails closed. The source uses the existing
macOS compatibility session keys; it does not grant the application's independent
production AwakeUnlocked monitor any authority. The target's required background
launch below does not change its rights or substitute a different controller.
No local developer Mac is supported.

The first [actual VM probe, run 39594013069](https://github.com/ralphschuler/orbitpal/actions/runs/39594013069)
on `de48e0a537211c469bd2d7b04031aa0e407f95fd` reported AX trust but caught a
native bridge exception before the AppleEvents result. It did not build or launch
the fixture and does not establish a permission denial. The client now explicitly
binds the [documented permission function](https://developer.apple.com/documentation/coreservices/3025784-aedeterminepermissiontoautomatet?language=objc)
with its OSStatus/pointer/event-class/event-ID/Boolean ABI, retaining the native
descriptor for the call and still passing `askUserIfNeeded:false`. No descriptor
structure size/layout is fabricated. The next real probe remains required.
Two closed numeric fields distinguish failures: `aeFunction` records callable
presence after binding; `probePhase` is 0 initial/AX, 1 binding, 2 descriptor
factory, 3 descriptor pointer, 4 actual permission call, 5 actual session/display
checks, 6 all prerequisites completed, 7 fixed System Events source/identity
preparation or native readback. `targetRunning` is positive only after actual
same-client running-process identity checks. Code 8 remains an unknown native exception;
code 3 requires an actual completed permission query with a nonzero OSStatus.
No exception text or raw status/session data is exported. Controlled Node VM
tests exercise the production probe with doubles, never a host permission query.

The second [actual VM probe, run 39595025071](https://github.com/ralphschuler/orbitpal/actions/runs/39595025071)
on `5bcd6dd683bdc0f81b9d5d8b8e828ef052f30c5b` reached the real permission
call (`aeFunction:1`, `probePhase:4`) and returned code 3. Its artifact did not
retain the OSStatus category, so it proves neither denial nor an absent target.
`aeStatusKind` now records only 0 success, 1 consent required (`-1744`), 2 event
not permitted (`-1743`), 3 target process absent (`-600`), or 4 other/unknown.
Before a completed query, 4 means unobserved; code/phase still determine which
operations actually completed. Raw OSStatus and exception/session data remain
unexported. All nonzero results still fail without asking the user.

The third [actual VM probe, run 39596881584](https://github.com/ralphschuler/orbitpal/actions/runs/39596881584)
on `02217c145a962234af96fdc92edf25094944740f` positively observed the fixed
running target, AX trust and a zero permission-query result (`aeStatusKind:0`,
`appleEvents:1`). It refused at phase 5 (`code:4`, `gui:0`), before the fixture
build or any UI action. Its compound result does not identify which session or
display prerequisite failed. Apple's public `CGSession.h` confirms the exact
string keys used here: a CFNumber UID and CFBoolean console/login indicators.
Neither those types nor controlled JavaScript booleans prove how the actual
osascript bridge represented this VM's values.

Seven diagnostic-only fields now describe the already-read phase-5 values:
`screenPresent`, `sessionPresent`, `uidMatch`, `onConsole`, `loginDone`,
`lockKnown` and `lockState`. All use 0 for unknown/unread/wrong type, 1 for false
and 2 for true. `lockKnown:1` means the optional key is absent; `lockState` then
remains unknown. Missing/incorrect boolean types remain unknown, never truthy
success. The unchanged strict `gui` predicate remains the authority. Native
screen/session reads still occur once, and `getuid` retains its original
screen/session short circuit. No UID, dictionary, session key/value, raw native
exception or additional native sample is exported. The public [session API](https://developer.apple.com/documentation/coregraphics/cgsessioncopycurrentdictionary())
and [session property example](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPMultipleUsers/Concepts/FastUserSwitching.html)
provide source semantics, not evidence of the disposable VM's actual state.

The fourth [actual VM probe, run 39598095989](https://github.com/ralphschuler/orbitpal/actions/runs/39598095989)
on `9851ba8eec52633e3db30126d9e14b4a28b56259` again passed AX/AppleEvents and
observed a display (`screenPresent:2`), but the session deepUnwrap result was not
a plain JS dictionary (`sessionPresent:0`). UID, console and login remained
unknown; this does not establish an inactive, locked or missing native session.
No fixture build or UI action ran.

The public SDK declares `CFDictionaryRef` as bridged to `NSDictionary`; Apple's
[toll-free bridging table](https://developer.apple.com/library/archive/documentation/General/Conceptual/CocoaEncyclopedia/Toll-FreeBridgin/Toll-FreeBridgin.html)
confirms that pairing. The imported C function's opaque CFDictionary return is
now explicitly bound with the same pointer ABI as JXA `id`, which Apple's
[JXA release notes](https://developer.apple.com/library/archive/releasenotes/InterapplicationCommunication/RN-JavaScriptForAutomation/Articles/OSX10-10.html)
requires for objects. This preserves exactly one actual session call. Nil or an
unknown native class refuses; only an actual NSDictionary reaches deepUnwrap.
The [native Copy ownership](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFMemoryMgmt/Concepts/Ownership.html)
is balanced by exactly one `CFRelease` in `finally` after conversion, including
conversion failure. Borrowed children are never released. The strict UID and
Boolean predicates remain unchanged; no integer/string becomes a Boolean.

A separately authorized constructed-only Foundation probe successfully used an
`id` return binding for `CFDictionaryCreateCopy`, verified its NSDictionary and
CFDictionary identity, and balanced its own Copy reference. Genuine constructed
CFBooleans unwrapped to JS booleans while NSNumber integers remained numbers.
It queried no host session, GUI, process or permissions. That contract check does
not substitute for the next actual disposable VM session/fixture run.

Apple's public SDK `AppleEvents.h` documents that the address must identify an
already running target, and returns `procNotFound` otherwise. The same header
defines the two consent constants; `CarbonCore/MacErrors.h` defines
`procNotFound`. After the hosted/reviewed-SHA/clean-source guards, the runner now
checks only the fixed `/System/Library/CoreServices/System Events.app`: canonical
root-owned non-writable directories, retained no-follow regular Info.plist and
executable FDs, exact bundle/executable names and repeated file identity. It
verifies the signature with the explicit Apple requirement (not the broader
Apple-issued developer requirement), then issues one fixed `/usr/bin/open -g -j
-a` background launch. Both commands suppress all diagnostics and share the
original five-second probe deadline. No shell, URI, user argument, activation,
permission request, AppleEvent automation command or permission retry is added.

Before its first permission call, the same osascript client performs bounded
native readback of the unique running System Events bundle, fixed bundle and
executable URL and valid PID. It rejoins the same PID immediately before the
query. Absent, replaced, ambiguous or foreign targets fail before AppleEvents;
code 3 still stops on every nonzero result. The new source/launch failure record
has AX/permission/target observations zero, phase 7 and unknown status kind 4;
it cannot be mistaken for a completed native query. System Events is not killed
or given broader rights; its lifetime ends with disposal of the hosted VM.

The fifth [actual VM run, 39599169139](https://github.com/ralphschuler/orbitpal/actions/runs/39599169139)
on `881411401a59317bfa40914251dc405703b331d5` passed both strict native probes
and the actual isolated Wry build. Its first Main `inspect` ended with controller
`stage:1`, `code:10`, forced owned-child cleanup and no unconfirmed cleanup. The
old catch-all exception category cannot distinguish a native-client timeout from
decode/other failures, and no validated per-operation reply was retained. This
proves neither missing AX/window/menu support nor a product error. No Life/frame
or normal Stop/Quit qualification was obtained.

Controller diagnostics now retain only fixed numeric transport metadata:
`transportPhase` is 0 before transport, 1 existing deadline/owned-child checks,
2 existing native-client run/return checks, 3 reply decoding, 4 closed-schema
validation, or 5 a fully validated reply. `exceptionKind` is 0 no Python transport
exception, 1 `TimeoutExpired`, 2 JSON/Unicode decoding, 3 a schema guard refusal,
or 4 another exception/transport guard refusal. Existing controller refusal codes,
commands, one native client per attempt, five-second cap, original retry rules
and all deadlines are unchanged. Exceptions, their arguments/output/stderr and
UI text are never formatted or exported.

`lastUiObserved:1` adds `lastUiRecord`, copied only after every existing numeric
schema check passed, plus its `lastUiStage`. Initially the flag/stage are zero and
the record is absent. A later timeout never overwrites or relabels that earlier
reply; a structured nonzero UI reply is still a failure, never success. No extra
UI query obtains these fields. The existing two-KiB private controller-file bound
still applies. Controlled tests execute the real execute/perform/ui/parser path
with subprocess and clock doubles, including hostile timeout payloads, rejected
schemas, exact retry sleep and a later failure after a valid reply.

`ui.js` runs in `/usr/bin/osascript` as a **native JXA/System Events/ObjC client**,
not in a WKWebView. It never evaluates application JavaScript, calls native IPC,
injects ACKs or modifies Store state. Apple documents [native UI scripting](https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/AutomatetheUserInterface.html)
through exact accessibility objects. Each operation rejoins the retained child
PID, fixed bundle identifier, exact executable/bundle URL and unique Main window.
Only the disposable VM's owned fixture can be brought forward. Exact enabled
buttons must have a positive rectangle inside Main; inaccessible/offscreen or
ambiguous controls refuse, with no coordinate/key/action-alias fallback. The
first real `inspect` checks the actual Main Autopilot AX button and the owned status-item recovery route before any Life action.

The controller performs the five real Main actions: Autopilot, Set up desktop,
add Fixture Nova, add Fixture Blue, Save and start Life. Actor buttons require
their exact seeded name in native static text. It gives real frames 45 seconds,
closes only Main through its actual native close button, waits 12 seconds, uses
the owned status menu's Show studio, presses real Stop Life, then selects its
normal Quit OrbitPal. The second owned menu bar must be unique and expose both
fixed English production menu items. An unavailable status menu or native AX
tree is a capability failure, not grounds to add a new app command or fake Stop.
The small actual UI probe must succeed before considering any selector changes.

Wall waits prove nothing. Only the **unchanged existing fixture validator** can
qualify both real actors: genuine constructor/Prepared/accepted-frame spans,
35 seconds Live and 10 seconds Main hidden, frame age/gap, successful real operator
Stop, physical cleanup and Destroyed. The fixture's original 180-second deadline
and feature-level provider/credential refusals remain unchanged. The controller
has one 150-second deadline; each native client is killed after at most 5 seconds,
normal Quit gets at most 10 seconds, and failed owned-child cleanup gets 5 seconds
TERM plus 3 seconds KILL. Forced cleanup can never pass. This establishes no
general descendant-resource quiescence or physical-display performance result.

The example is copied from a retained regular owned executable FD into a new
private `.app` with the same separate fixture identity. No signing certificate,
installer, system registration or existing application data is used. Main and
avatar incognito behavior, compiled isolated evidence/Store and production
render/constructor paths remain untouched. No HOME/XDG replacement, credentials,
model/voice provider, MCP, screenshot, recording or raw AX tree is used. Only
fixed numeric capability/controller/validated fixture reports and public
source/binary fingerprints are uploaded; the new private Store/app bundle are
removed after the retained child exits. Unconfirmed cleanup fails the controller.

A passing fixture still lists native action arrival, speech handoff, OBS/CEF,
mixed DPI/lock/suspend and visible pixel attestation as remaining. It does not
close #72/#75 or turn an accepted-frame counter into a visual screenshot claim.
Native JXA bridging, AX button/status-menu exposure and the whole real workflow
remain unproved until the separately authorized actual hosted-VM run. Local
`test-runner.py` exercises only pure guards/protocol/packaging with doubles.
