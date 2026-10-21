# Nonshipping native Linux renderer lane

This source checkpoint has not launched, compiled or qualified this harness.
Issue #66 stays open. The first executable acceptance is a **short smoke run**
on an explicitly disposable GitHub-hosted Linux VM. A successful smoke would
qualify this fixture's lifecycle and numeric scope, not hardware performance,
the full application or the broader benchmark matrix.

Build the separate benchmark frontend and feature-gated Cargo example using
their reviewed isolated configuration before entering the timed fixture. It
embeds the actual AvatarStage; it does not initialize the production App,
Store, credentials, providers, streams, MCP, capture or microphone backends.
Do not substitute the normal OrbitPal executable or a dev server.

On the qualified hosted VM only:

```sh
bash scripts/native-render-benchmark/run.sh smoke /absolute/path/native_render_bench
```

## Reviewed manual smoke workflow

`.github/workflows/native-render-benchmark-smoke.yml` is manual-dispatch only.
It accepts no command, ref, model count or mode input. Its sole input is the
reviewed40-lowercase-hex commit SHA, which must equal the workflow's GitHub SHA
and checked-out HEAD. Only the original `ralphschuler/orbitpal` repository and
`main` or exactly `test/native-render-benchmark-smoke` are allowed. Checkout does
not retain credentials, permissions are contents-read only, actions are pinned,
and no repository secrets/signing/release/provider credentials are requested.
GitHub requires the workflow file to exist on the default branch before
workflow_dispatch can run. The allowed test branch therefore does **not** enable
the very first pre-merge dispatch. First merge a reviewed opt-in, nonshipping
benchmark-plus-workflow PR after normal three-OS CI and the standalone example's
non-GUI compile/protocol tests pass. Then review the resulting immutable main SHA
(or a later exact allowed test-branch SHA) and dispatch with that SHA. A squash
merge gets a new commit SHA; the reviewed source/tree and final input must be
reconciled explicitly. Do not reuse another workflow's name or add push triggers
to bypass registration. See GitHub's [manual workflow requirements](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow?tool=cli).

The ordinary three-OS CI builds the standalone assets after dependency
installation and explicitly runs the feature-gated example's non-GUI tests:

```sh
node scripts/native-render-benchmark/build.mjs
cargo test --manifest-path src-tauri/Cargo.toml --example native_render_bench \
  --features native-render-benchmark --locked
```

The example's tests cover only numeric protocol, navigation and fixed mode/epoch
logic; its `main` is never called. The default Cargo test command skips this
required-feature example, so the separate CI step is necessary. The synthetic
collector and offline analysis fixtures also run in the ordinary Linux CI job.

The maintainer must review the exact source before dispatch. Merge/registration,
non-GUI tests and adding this workflow do not establish that the private Linux
runtime works; the first actual smoke remains a distinct qualification.

Build steps install packaged prerequisites on an actual GitHub-hosted
Ubuntu26.04 VM, then run these unprivileged build commands with `TAURI_CONFIG`
and `DISPLAY` unset:

```sh
pnpm install --frozen-lockfile
node scripts/native-render-benchmark/build.mjs
cargo build --manifest-path src-tauri/Cargo.toml --locked \
  --example native_render_bench --features native-render-benchmark
```

The separate Vite entry has publicDir disabled and builds `dist-benchmark`;
the example embeds its own `benchmark/tauri.conf.json`, distinct app identifier,
empty normal-window list and closed numeric invoke handler. No `pnpm build`,
`cargo run`, normal OrbitPal executable, App/Store/bootstrap or stream/provider
is launched. Cargo may compile shared library dependencies; compilation is not
application initialization. The only runtime is the standalone example through
the guarded `run.sh smoke` CLI above, inside the existing fresh UID/private
bus/app-network-namespace/owned-cgroup harness. The control WebView creates no
Stage, so the smoke alternates zero and one actual Nova Stage at30fps and runs
three **additional** orderly create/dispose/Destroyed cycles. It does not run
three simultaneous actors or claim the full Studio app baseline.

The job is capped at45minutes for cold package/build work. The smoke step has a
root-owned twelve-minute watchdog so it can actually signal the root runner.
TERM lets its existing interruption handler perform exact bounded cleanup;
a further120seconds permits cleanup before forced KILL, under a fifteen-minute
step cap. Timeout/KILL/cancellation is failure and never confirms cleanup.
No cgroup controller, hardware/DRM permission, sandbox, renderer quality or
55-minute inner harness policy is relaxed. Only smoke is invoked; pilot/40-minute
or120-minute benchmark acceptance cannot be selected in this workflow.

The workflow uploads only the six explicit exported metadata filenames (including
available validated failure evidence), never raw app/compositor/build logs,
HOME/XDG contents, binary payloads, screenshots, audio or frames. A successful
smoke additionally checks the fixed one-Stage/two-WebView scope, three additional
cycles, zero live figure/Stage at completion and confirmed cleanup. Missing
artifacts or an incomplete teardown cannot become success. Software-compositor
qualification is separate from GPU/present timing, hardware performance and
full issue#66 acceptance. No actual smoke has run at this documentation point.

The explicit `pilot`, `pilot3` and `pilot5` modes are separate. Do not run them before the real short smoke
and isolation checks pass. The existing native driver owns phase times, three
additional orderly-close cycles for smoke versus twenty for pilot, fixed Nova
loading, unique window epochs and exact numeric IPC. `pilot` owns one figure at
30fps, `pilot3` owns three simultaneous figures at30fps, and `pilot5` owns five
at20fps. Each figure has its own actual AvatarStage and WebView, requesting fixed
480×480 logical content dimensions and a three-column/two-row grid. Wayland
compositor placement and native occlusion are not confirmed by this fixture. A separate control WebView never constructs a Stage. These are isolated
renderer windows, not production Studio/overlay or OBS exposure measurements.
The smoke remains one figure at30fps and cannot select a long mode.

All figure windows must independently report a fresh post-load submission
before warmup. Each phase starts only after every current epoch has confirmed
the DOM command; restoring in-view needs a fresh submission from every figure.
One stalled, destroyed, failed or unacknowledged figure fails the bank. Each
orderly cycle disposes every Stage and observes every exact native window's
Destroyed event before creating a new bank with fresh epochs. CPU/RSS continue
to cover the entire app cgroup and cannot be attributed to only one figure.

The separate manual `native-render-benchmark-pilot.yml` workflow accepts exactly
one closed pilot mode, its reviewed immutable SHA and a successful manual smoke
run ID. Before privileged setup and again immediately before launch, it reads
only that public run's metadata from the fixed GitHub repository and requires
the exact smoke workflow path, matching source/repositories and completed
success. A smoke from another head, a fork, a running rerun or the ordinary CI
workflow cannot qualify it. Its read-only job token is not forwarded through
the privileged runner's empty environment. There is no automatic dispatch or
three-job matrix. This prerequisite confirms only that the same-source smoke
workflow succeeded; it does not turn the pilot into hardware, production Studio
or full issue#66 acceptance. First registration on the default branch and
maintainer source review follow the same requirements as the smoke workflow.

Each pilot requests ten
minutes of warmup followed by four 7.5-minute measurement phases, plus control
baselines and close cycles. Neither the elapsed duration nor the software-GPU
runner establishes a measured hardware baseline or a performance target.

## Owned process and network scope

`run.sh` rejects non-hosted/non-Linux environments before sudo. Trusted checkout
`runner.py` creates a random `/tmp/orbitpal-render-<16 lowercase hex>` root and
an OS user/group that **must not already exist**. A newly allocated passwd UID
is not sufficient: before any fixture ownership transfer or workload launch,
the runner must prove that the numeric UID has no live processes and retain the
exact name, UID, GID, home, shell and private-group identity. Failed or uncertain
vacancy proof never grants UID-wide kill or name-deletion rights. Its marker is
precisely the nonce with no newline. Bootstrap stays beneath a root-owned0700
ancestor until setup completes; root/home/data/config/cache/runtime are then
private0700, owned by that fixture UID. Nothing is copied from an existing installation,
account, display or session. All workload environment variables are explicitly
built; host DISPLAY, TAURI_CONFIG, session/system buses and secrets are absent.

Two new root-owned cgroup-v2 leaves provide separate app and compositor domains.
App includes its session shell, private DBus daemon, native example, WebKit and
activated helpers, across **all UIDs**. Compositor and its separate private bus
remain in the sibling `<rootname>-compositor` domain on the VM's original network
namespace. The root collector lives outside both measured domains. The fixture
requires already enabled cpu/memory controllers and dedicated group accounting;
it never rewrites host controllers, moves an unrelated process or grants the
app permission to migrate groups. Unsupported accounting fails qualification.

The app domain enters a new network namespace before UID/group drop. It has no
configured external interface or route. Every observed live app-group helper
must retain that exact namespace. A legitimate further WebKit sandbox namespace
is currently an **unqualified scope**, not permission to disable sandboxing or
silently broaden attribution. The only compositor transport provided to app is
the owned Wayland Unix socket, linked into its private runtime. App and compositor
have separate HOME/XDG/bus directories and no bus bridge. This is explicit process
and transport isolation, not a claim of a separate filesystem namespace.

The app bus is started **inside** its cgroup and network namespace. Its config
has only the packaged `org.a11y.Bus` and `ca.desrt.dconf` service files, with exact
verified executable paths. No standard service directories, SystemdService,
systemd activation delegation or Secret Service is configured. System-bus
addresses point to nonexistent owned paths. The compositor also uses a private
bus and the same bounded activator whitelist. No host bus is used.

Root's pre-exec `launch.py` can select only app or compositor and smoke or pilot;
it accepts no generic command/env/executable. It joins the exact owned cgroup,
enters app netns if appropriate, drops supplementary groups/GID/UID, and executes
the fixed session through a preopened trusted script descriptor. Root never runs
a script copied to the app-writable fixture root. The normal app flags, renderer
quality and WebKit sandbox are unchanged. GNOME uses the existing headless
software-compositor command. No DRM node ownership, physical GPU permissions or
kernel modules are changed. If real EGL/Wayland cannot work in this scope, setup
fails; there is no screenshot, fake-RAF, QPainter or native-grant fallback.

## Numeric collector and meaning

At1Hz the root collector reads only dedicated cgroup CPU/memory and bounded
numeric `/proc` metadata. Recursive group PID lists include every descendant
UID, not a fresh-UID filter. The collector additionally inventories directory
ownership for fresh-UID processes and rejects any live process outside app and
compositor domains. It reads no argv, environment, maps, heap, windows, pixels,
audio or credentials. PID start identity, UID, membership and namespace are
checked around numeric reads. Exited/zombie processes cannot establish readiness.
Group CPU retains work from short-lived helpers missed between samples.

`collector.ready` is root-created, exact-content, single-use and readable by the
fixture UID. It proves **numeric process scope only**. Native must independently
wait for a post-load successful real Stage submission before frame qualification;
neither this marker nor app.pid is renderer readiness.

CPU100% means one logical core. The first interval is unknown, not an invented
zero. Collection gaps and cumulative CPU remain visible. Summed RSS explicitly
may double-count shared pages; cgroup charged memory is separately labeled and
is not RSS. RAF callbacks, successful CPU render-call submissions, CPU update+
submission elapsed time, native phase/IPC timing and DOM-out-of-view are distinct
metrics. They do not prove GPU completion, display-present timing, native window
visibility or OBS exposure. GPU/present metrics and instrumentation overhead
remain unqualified; no performance claim is inferred from this source.

After verifying each GitHub artifact digest and extracting only the six allowed
metadata files, `python3 scripts/native-render-benchmark/analyze.py /absolute/artifact-directory`
produces a numeric report from those files alone. It opens no app, network,
process metadata, device or credentials. Native and collector relative clocks
have different origins; the new explicit shared Linux `CLOCK_MONOTONIC`
microsecond stamps are required for phase/process correlation. Older records
without that coordinate report process cost unavailable, never an inferred
alignment or zero CPU. Summed RSS and charged memory remain separate.

The analyzer differences each figure's cumulative histograms within its own
current epoch/phase. It reports observed spans, sample gaps, per-figure submission
rates and percentile upper bounds. The >=256ms overflow bin has no finite
percentile upper bound. Missing owners, short or sparse windows and failed
cleanup cannot become a successful invisible-render suppression result. It
also checks every expected orderly window epoch once. These statistics do not
set a hardware budget, qualify GPU timing, measure instrumentation overhead or
close issue#66. Both manual workflows run the synthetic collector and analysis
tests before entering the native fixture; no measured result is asserted here.

Limits include55minutes from before provisioning, per-step timeouts,256 concurrent
PIDs,16 cgroups at depth≤4,4096 observed identities and32MiB per numeric log.
No cap silently drops processes or converts an empty interval into a pass. Stock
WebKit sandbox behavior, compositor startup, Tauri embedding and cleanup must
qualify on the actual runner before a long pilot is authorized.

## Output and cleanup

Only bounded, validated native/process NDJSON, collector summaries/fixed failure
JSON and explicit technical fixture metadata are copied to
`$RUNNER_TEMP/native-render-benchmark-<mode>`. Existing output directories are
refused. Raw app/compositor/bus output is discarded, not saved. No home, WebView
data directory, profile, argv/env dump, personal path, screenshot, recording or
frame enters the artifact allowlist.

Before the first workload, the runner pins the fixture and runtime directories
through open no-follow directory descriptors. The collector inherits the exact
pre-workload fixture descriptor and must match it before use. All later root
artifact writes use descriptor-relative, exclusive, no-follow leaf creation;
existing regular files and symlinks are refused, never overwritten. Every
ancestor/leaf identity is checked around writes. A parent replacement fails
closed; a race after the check still writes only through the retained original
descriptor, never a substituted target. There is no pathname-write fallback.
The Wayland socket bridge is also created relative to the pinned runtime.
Artifact destinations use the same exclusive writer. Fixture and artifact
paths must have canonical, non-symlink ancestors; unsupported paths refuse.

Artifact reads also require the exact root descriptor retained before the
workload. There is no Root-Path compatibility/fallback read, including after
early bootstrap failure. Export is allowed only after all owned cgroup drains,
child reaps and the proven account's UID drain/deletion are confirmed. Four
fixed artifact leaves are opened descriptor-relative with O_NOFOLLOW and
O_NONBLOCK; regular-file type, one link, expected producer UID and bounded size
are checked before reading. native.ndjson belongs to the fresh workload UID;
process.ndjson and the collector JSON files belong to root. Retained ancestry
and leaf identity are rechecked around the bounded read, as are size, ownership
and modification metadata. Replaced parents, leaf symlinks/hardlinks, changed
metadata and uncertain ownership refuse without a pathname retry. All four
files are read/validated before any raw artifact is copied (at most128MiB staged).
The numeric parser limits depth, counts, numeric ranges and string alphabets;
it is **not** a semantic content or secret allowlist. Producer/path binding and
process-stop proofs are required independently of that shape check.

All fixed privileged workload-root metadata reads use the same bound reader:
fixture.marker (fresh UID,16bytes), host-netns.txt (root,64bytes), app.pid
(fresh UID,16bytes) and completed.marker (fresh UID,64bytes). The launcher gets
the pre-workload root FD for both compositor and app, and the collector gets it
before any marker read. Each child first constructs a no-follow directory chain
matching that inherited identity; it then validates root/marker metadata. The
inherited witness is closed after binding, and launch closes its private chain
before UID-drop/exec. There is no post-handoff Root-Path metadata-read fallback.
The exact nonce, namespace, PID/process identity and completion-token checks
remain separate requirements. Waiting for app.pid uses optional FD reads,
never a symlink-following is_file probe. Kernel /proc and root-owned cgroup
reads remain separately scoped as above; a compositor socket lstat is only
metadata and grants no renderer readiness.

A successful fixture additionally requires native.ndjson, process.ndjson and
collector-summary.json to be present, bound, bounded and valid before any raw
artifact is copied. If one is missing, the final result cannot pass even when
all fake/real child exit, completion-marker and drain conditions succeed.
Failure-path export may retain validated partial evidence after confirmed
writers-stop; early bootstrap without a pinned root never reads artifact paths.

The root owner retains the exact new cgroup inode, proven-vacant account and
namespace identity. Cleanup stops/reaps owned launchers and independently
attempts exact owned cgroup/namespace cleanup. A fresh full account/group check
is required immediately before any UID-wide kill or account deletion; account
absence and the original group are rechecked immediately before group deletion.
An observed mismatch permanently revokes that proof, even if the old values
later reappear. Partial useradd is not adopted from a matching home string.
Unproven identity never authorizes pkill/userdel/groupdel, and an exposed root
is retained if exact identity cleanup cannot be confirmed. Unknown ownership,
incomplete drain or any cleanup uncertainty makes the fixture fail. These
userspace checks do not atomically lock kernel identity/process creation with
passwd/group commands: the disposable VM must have no competing privileged
account-management actor. No isolation against a malicious concurrent root
administrator is claimed. Forced fixture teardown is separate
from the native driver's actual orderly Stage-dispose/Destroyed acknowledgments;
it cannot be used to claim normal App/raw-close/decoder cleanup.

`test-collector.py` uses synthetic metadata, fake `/proc` reads and temporary
regular files only. It never creates an account/cgroup/network namespace,
launches an app/compositor, samples a real user process or captures a device.
The first safety packet preserved the original27 test ASTs and added16 cases.
This read-boundary follow-up preserves42 of those43 ASTs exactly. The maintainer
approved only the original export-success fixture's call setup changing to an
explicit pre-pinned source and owner/stop context; every existing success,
completeness and negative assertion remains unchanged. Additional cases cover
leaf symlinks and preexisting files, parent/path replacement (including the
check/open interval), inherited-directory identity, occupied numeric UIDs before
first use, failed vacancy/partial useradd, sticky name/UID/GID/home/group changes,
and replacement immediately before UID/account/group cleanup. New read cases
also cover private accepted-shape sentinel JSON behind substituted ancestors,
check/open and read-time swaps, hardlinks, wrong producer UIDs, oversized files,
nonblocking FIFO refusal, all-source validation before copying, and no-bootstrap-
FD/no-stopped-writer fallback. The fixed-metadata/completeness follow-up preserves
all54 predecessor test methods and assertions exactly, adding private matching-
metadata ancestor/link cases, child witness rejection before marker reads, and
actual execute/export/result tests with every external kernel/process operation
mocked. That driver test checks each missing mandatory artifact and an all-present
control; it is not a native benchmark or an isolation qualification. The
maintainer subsequently ran the exact730c2e44 seven-file packet: all60 synthetic
methods, both shell syntax checks and Python AST checks passed with source hashes
unchanged. This does not execute the privileged Linux harness or qualify a native
renderer. The new workflow is source-only and still awaits independent review
and executable qualification; no workflow dispatch or benchmark result is claimed.

## First actual hosted smoke

Run `39587659424` at merged source `000cfe3c96a8f32db6efa21968da2897db831b8b`
built the standalone Linux example but refused before starting the renderer:
`untrusted_bus_service`, `cleanupConfirmed=true`, `passed=false`. It does not
qualify the smoke, any pilot, or issue #66. The unchanged trust policy rejects
nonregular, non-root-owned or group/world-writable packaged service files.

Both manual workflows now run the same read-only packaged-service check before
the cold build. A refusal reports only service code 1 (accessibility) or 2
(dconf), regular-file/symlink/root-owner booleans and the masked write bits. No
raw file, executable command, symlink target, path, process or home is exported.
The ordinary runtime repeats the same check and includes those closed fields in
its existing `fixture-result.json` if that exact refusal occurs. Diagnostics do
not allow another service, follow a new link, change permissions or qualify a
runtime. The preflight creates no fixture account, bus or application.
