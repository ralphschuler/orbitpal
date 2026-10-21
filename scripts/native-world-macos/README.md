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
production AwakeUnlocked monitor any authority. An absent System Events process
may also make the nonprompting probe fail; the harness does not change its rights
or substitute a different controller. No local developer Mac is supported.

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
checks, 6 all prerequisites completed. Code 8 remains an unknown native exception;
code 3 requires an actual completed permission query with a nonzero OSStatus.
No exception text or raw status/session data is exported. Controlled Node VM
tests exercise the production probe with doubles, never a host permission query.

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
