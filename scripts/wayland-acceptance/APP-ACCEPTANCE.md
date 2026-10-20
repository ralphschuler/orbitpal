# Opt-in actual Studio/application subset

This harness is source prepared and **has not run on a desktop yet**. Issue #26
remains open. The default workflow/run.sh path still runs the separate successful
`real_portal_prototype` baseline. Neither local harness tests nor that baseline
establish the application cases below.

The manual workflow input `full_app` defaults to false. When explicitly enabled,
each existing Ubuntu26.04 GNOME/KDE job first runs its normal baseline, then builds
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
cannot pass. The ordinary app stdout/stderr is not collected. Only the existing
bounded desktop/service diagnostics and fixed app controller/result/binary-hash
text are uploaded. The private data root is destroyed, never uploaded.

Still unproven: two real bot sessions, tool-round images, compositor user revoke,
source removal/change, multiple/scaled/rotated outputs, serial/version variations,
held-capture races, unsupported-buffer negotiation, frame-persistence scans,
actual Hindsight payloads, resource-count quiescence, real Quit/shutdown callback,
Autopilot and physical hotplug/suspend/resume. Cleanup terminates the generated
app process; that is **not** a claim that its native Quit handler was exercised.
Do not mark #26 complete or Wayland generally accepted from this subset.

Local `test-app.py` tests synthetic HTTP/JPEG/state/control metadata only. It
opens no app, GUI, compositor, portal or personal data and performs no capture.
They are intended to run after `python3-pil` is installed in the opt-in CI job;
they have not been executed for this source checkpoint.
