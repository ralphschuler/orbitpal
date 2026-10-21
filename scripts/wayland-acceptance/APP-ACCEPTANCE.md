# Opt-in actual Studio/application subset

The actual-app subset **has not passed**. In [run 39583461054](https://github.com/ralphschuler/orbitpal/actions/runs/39583461054),
head `46cea4417c8b47a8ed3f599d530a09be99004141`, both separate portal
baselines passed. The GNOME actual-app job refused PAM capability code 4 before
account/app creation. Its first four PAM file hashes matched the supported KDE
stack; only `common-password` differed. The exported hashes do not identify the
refused rule. KDE constructed a real, typed active/unlocked/not-preparing child
login1 session, reached native Compatible selection, then failed the showing-only
`API base URL` entry lookup. The child was alive until expected TERM cleanup;
PAM cleanup was confirmed. No app success receipt exists. Earlier failures remain
separate evidence. Issue #26 remains open. The default workflow/run.sh path still
runs the separate successful `real_portal_prototype` baseline. Neither local
harness tests nor that baseline establish the application cases below.

In [run 39590344043](https://github.com/ralphschuler/orbitpal/actions/runs/39590344043),
head `6702eb5e8152172af65eaaab4b5d86c834fa514e`, the GNOME baseline and
controlled app fixtures passed, then actual-app startup refused the installed
PAM stack before creating an account or launching the app. Its closed numeric
record now identifies the exact rejected form in `common-password`:
`password optional pam_gnome_keyring.so`, without arguments. The parser now
recognizes only that observed file/type/control/module/argument combination.
The [systemd 259.9 PAM service implementation](https://github.com/systemd/systemd/blob/v259.9/src/core/exec-invoke.c#L1299-L1307)
calls account, credential and session operations; it never calls the password
change operation `pam_chauthtok`. GNOME documents the module's separate
[password-management role](https://wiki.gnome.org/Projects/GnomeKeyring/Pam/Manual).
Auth/account/session keyring rules, other controls/files and all options remain
refused. No PAM file, account, system service or credential setting is changed.
This compatibility change still needs a new actual GNOME run.

The refused `/etc/pam.d/login` SHA-256 was
`71b58114fcdc0e037b9700a92f14638a4a24f8480f23b03cc26740511fdd56dc` on both
desktops. It matches the public Ubuntu `login`
`1:4.16.0-2+really2.41.3-3ubuntu2.2` package exactly. Its first session rule
closes the SELinux context and a later rule opens the fresh context. The launcher
now recognizes only that exact ordered control/argument pair, with only
`pam_loginuid`/`pam_motd` allowed between them. Both installed SELinux and systemd
modules still require the existing owned-file validation. This is a bounded
stock-grammar compatibility fix, not a general PAM control interpreter; the
remaining real stack, child login1 session, selector and cleanup still require
a fresh actual run.

The two fixed model fields now use fresh PID/executable/unique Studio-frame
acquisition, including VISIBLE offscreen entries. At most two real native
`scrollTo` requests may reveal a field within its original 12-second deadline.
Only a fresh SHOWING, enabled, editable exact field receives one native
`EditableText.setTextContents` call. No window focus, global key/pointer or
JavaScript route is added; ordinary message entry and capture/chooser gates stay
unchanged. Actual SQLite save readback still establishes the persisted settings.
A terminal `app-model-entry` line contains only bounded lookup/role/name-match
counts, reveal/write counts, a fixed field code (1 endpoint, 2 model), and expiry.
No text/value/tree is logged, and no post-deadline diagnostic AX query is made.
The previous receipt does not prove whether offscreen state, a native popup or
another accessibility condition caused its missing-field result.

PAM grammar remains fail-closed. Additional `pam-session-capability` numeric
fields locate a refused non-session rule without exporting a config line:
`stackFile` uses 1 login, 2 common-auth, 3 common-account, 4 common-session,
5 common-password; `stackRule` is 1 malformed rule or 2 unrecognized module.
`stackModule` recognizes only 1 gnome-keyring, 2 exec, 3 kwallet5, 4 kwallet6;
0 means unknown. `stackControl` is 1 required, 2 requisite, 3 sufficient,
4 optional, 5 bracket control; `stackArguments` is 0 absent or 1 present.
Unclassified failures and supported stacks leave these fields zero. The codes
are observations, not permission to accept another module or argument.

The manual workflow input `full_app` defaults to false. When explicitly enabled,
each existing Ubuntu26.04 GNOME/KDE job checks the installed stack with the same
read-only capability command immediately after package installation, before any
native compilation. An unsupported stack fails early and retains the same
bounded numeric report; it creates no account, bus connection or PAM session.
The actual-app launcher repeats the check before its existing account creation.
The job otherwise first runs its normal baseline, then builds
the actual embedded Tauri app with `pnpm tauri build --debug --no-bundle` and runs
a fresh isolated desktop/account using:

```sh
bash scripts/wayland-acceptance/run.sh gnome /absolute/native-libtest /absolute/orbitpal
```

This command is restricted to disposable GitHub-hosted Linux jobs. Never spoof
its environment to run it on a developer machine. It does not use an installed
app, existing account, existing GPU permissions, credentials or a real monitor.
The initial packaging gate must prove the debug binary embeds the built frontend
without a Vite/dev server. Packaging failure is a failure, not a mock-app fallback.

`app-session.sh` uses the existing hardened compositor/session setup, a new
1920×1200 private monitor, fresh app data and only an updater-off installation
boolean seeded before startup. All bot/model/voice/capture settings are changed
through the real PID-owned Studio UI via AT-SPI. A WebKit subprocess is accepted
only through its checked UID/ancestry, not by matching a window title alone.
Fresh English UI labels are expected; other UI languages remain outside this
fixture. Inaccessible/ambiguous fields and an app window covering the marker band
fail. No JavaScript/invoke injection, direct native grant command, fake portal
response, fake frame, restore token or production test backdoor is introduced.

The actual compatible provider transport connects only to an unpredictable
loopback port. Blank/NoKey needs no OS secret lookup. Free-form fixture model,
tools/MCP/scripts off, images on, voice off and normal Studio output are actual
configurable app settings. No microphone, cloud API, stream account or external
Hindsight is used. `provider.py` accepts bounded, exact-path HTTP JSON and decodes
one production JPEG per request in memory. It checks an exposed red/blue edge
marker and the output image limits, then retains only bounded numeric metadata
and an in-memory digest. It logs no headers, bodies, exceptions, pixels or Base64.
The current marker's GTK draw is acknowledged before requesting it; the actual
JPEG received through the real app is the final oracle. No capture file is made.

Required initial subset, in order on both desktops:

1. Actual PID-owned Studio cold start with capture off and no assistant history.
2. Explicit UI image/screen grant, real system source selection and real save.
3. Two distinct fresh marker JPEGs through actual ScreenState → agent → provider.
   The first normal SSE reply must be stored once and visibly presented. The
   second is a held positive control: release unchanged normal text and require
   actual assistant history plus Studio subtitle. It cannot be `[silent]`/empty.
4. Hold another ordinary reply, press real Stop, release it late and require no
   matching assistant history/subtitle. UI must show the source is closed.
5. Explicit reconnect; hold a normal reply, turn **Send images to the model** off
   and Save. This tests same-bot save-time model/image-grant revocation without a
   preceding disconnect. Release late: no matching assistant history/subtitle.
6. Restore images, Save and explicitly reconnect. Hold a normal reply, turn
   **Include the screen with every request** off and Save. That UI switch calls
   Disconnect immediately, so this is **UI disconnect + persisted disable**, not
   an isolated save-only revocation test. Release late: no matching reply.

Exactly five image-bearing HTTP requests and two presented assistant replies are
required. A cancellation may close the HTTP socket before the fixture releases
its normal SSE text; BrokenPipe/ConnectionReset on that write is expected. User
history rows may already exist before inference; only the unique assistant reply
and subtitle must be absent. Reconnect is a real new selection after each grant
revocation; no old response or saved display ID may restore a session.

`app-result.txt` is metadata-only and lists the unproven cases explicitly. The
fresh success receipt is mandatory, so a zero-test or prematurely exited process
cannot pass. Ordinary app stdout/stderr is never persisted or uploaded. In the
fresh fixture only, `app-runtime-diagnostic.py` launches the fixed validated
`acceptance-app` with the existing flags/environment unchanged. Its exact Popen
child PID is the controller's identity; `app-session.sh` instead owns the
supervisor. Child stdout and stderr share an anonymous pipe, consumed in memory
with a 4 KiB read/line bound, 1 MiB total bound and 8,192-line bound. An overlong
line is discarded as a whole. The final unterminated line is considered only on
actual EOF. No raw app-output file, FIFO payload, exception message, backtrace,
paths, headers, Base64 or captured bytes are retained.

`app-runtime.txt` contains only fixed message-category counts (Rust panic, GTK
display, bwrap namespace, WebKit sandbox, dynamic loader, EGL init and allocation
failure), bounded numeric counts and EOF/limit/drain status. These are **message
observations**, not a proven failure cause or an exhaustive classifier. A SIGKILL
is not classified as OOM. `app-exit.txt` records the exact signed Popen wait result
as exit code or signal, whether the child was already dead before cleanup, and
whether this supervisor requested TERM/KILL. Thus actual exit(137) differs from
signal 9. A natural nonzero exit/signal cannot pass because the controller happened
to finish first. Expected owned cleanup is reported separately.

The shell requests cleanup with one fixed atomic marker in its private control
directory. The supervisor signals/waits only its own child, with bounded TERM
then KILL grace periods; a dropped/failed controller still triggers this cleanup.
An output cap triggers explicitly **diagnostic-limit-induced** termination, so
resulting TERM/KILL/pipe behavior must not be attributed to the original app
failure. Child exit with a descendant-held pipe has a two-second drain deadline
and reports `drain_timeout`, not EOF or acceptance. A 240-second supervisor bound
sits inside the existing whole-session timeout. No sandbox, EGL, GPU or WebKit
flags are changed to collect this evidence. The existing fresh-account teardown
still owns any remaining descendant processes; child cleanup is not a claim
that all subprocess resources quiesced.

Only existing bounded desktop/service diagnostics and the fixed app
controller/result/binary-hash/runtime/exit text are uploaded. The private data
root is destroyed, never uploaded.

Still unproven: two real bot sessions, tool-round images, compositor user revoke,
source removal/change, multiple/scaled/rotated outputs, serial/version variations,
held-capture races, unsupported-buffer negotiation, frame-persistence scans,
actual Hindsight payloads, resource-count quiescence, real Quit/shutdown callback,
Autopilot and physical hotplug/suspend/resume. Cleanup terminates the generated
app process; that is **not** a claim that its native Quit handler was exercised.
Do not mark #26 complete or Wayland generally accepted from this subset.

Local `test-app.py` tests synthetic HTTP/JPEG/state/control metadata, hostile
byte filtering and controlled fake-child exit/cleanup races only. It opens no
app, GUI, compositor, portal or personal data and performs no capture. Controlled tests cover the native acquisition, parser/privacy and child
cleanup seams; they do not establish actual AT-SPI or portal success.
