# Nonshipping native Linux renderer lane

Three-OS CI builds the standalone example and runs its non-GUI protocol tests.
The third actual hosted smoke passed the activator check and standalone build,
then failed during runtime bootstrap without qualifying the renderer or scope.
No qualified measurements or hardware baseline exist; issue #66
stays open. The first executable acceptance remains a **short smoke run** on an
explicitly disposable GitHub-hosted Linux VM. A successful smoke would
qualify this fixture's lifecycle and numeric scope, not hardware performance,
the full application or the broader benchmark matrix.

The failed run's retained GNOME 50 diagnostic identified a startup exception
while constructing a system-bus proxy. The compositor now receives a separate
real empty D-Bus transport on its own protected runtime socket, with no service
directories, activation helper, included host configuration or logind service.
Its daemon runs under the existing disposable UID/compositor cgroup and cleanup.
The app still cannot connect to a system bus. This supplies transport only:
missing services remain missing, and no login/permission observation is invented.
Controlled config/environment tests run everywhere; a small Linux-only real
D-Bus fixture checks that external services cannot be activated. A new genuine
GNOME smoke remains necessary before claiming renderer qualification.

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

The workflow uploads the six explicit exported metadata filenames (including
available validated failure evidence). A failed run with confirmed writer drain
may additionally export the separate bounded/redacted fixed-compositor startup
diagnostic described below. No raw app/build logs, HOME/XDG contents, binary
payloads, screenshots, audio or frames are uploaded. A successful
smoke additionally checks the fixed one-Stage/two-WebView scope, three additional
cycles, zero live figure/Stage at completion and confirmed cleanup. Missing
artifacts or an incomplete teardown cannot become success. Software-compositor
qualification is separate from GPU/present timing, hardware performance and
full issue#66 acceptance. No actual smoke has qualified this fixture.

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
qualify the smoke, any pilot, or issue #66. At that checkpoint the trust policy rejected
nonregular, non-root-owned or group/world-writable packaged service files.

Both manual workflows run the same read-only isolated-activator check before
the cold build. Packaged accessibility-descriptor refusals report only fixed
service code 1, regular-file/symlink/root-owner booleans and masked write bits;
the earlier diagnostic revision also used code 2 for the dconf descriptor. No
raw file, executable command, symlink target, path, process or home is exported.
The ordinary runtime repeats the same check and includes those closed fields in
its existing `fixture-result.json` if that exact refusal occurs. Diagnostics do
not allow another service, follow a new link, change permissions or qualify a
runtime. The preflight creates no fixture account, bus or application.


## Isolated dconf activation after the second actual refusal

Run `39589792944` at `33ce7de` failed the early read-only check: service code2
(dconf), regular file, root owned, not a symlink, masked group/world write bits
`0o022`. That run did not reach the cold build or renderer launch. The writable
package descriptor is never accepted, read, followed, copied or repaired by the
new dconf path.

Only `ca.desrt.dconf` uses a repository-declared three-line `Name`/`Exec`
activation template. This matches the Name and argument-free executable in
[GNOME's upstream service definition](https://raw.githubusercontent.com/GNOME/dconf/master/service/ca.desrt.dconf.service.in),
with the existing two exact installation paths: `/usr/libexec/dconf-service`,
then `/usr/lib/dconf/dconf-service` only if the first path is absent. An existing
untrusted candidate fails; it cannot cause a fallback. No descriptor-provided
command, argument, service name, `User` or `SystemdService` is consumed. The
accessibility bus retains its original protected package-descriptor check.

Before returning the dconf template, the same preflight/runtime helper opens
only the fixed executable through retained no-follow parent-directory FDs. Each
ancestor must be a root-owned directory, traversable by the fresh UID and with
no group/world write bits; the
file must be a root-owned regular ELF executable, executable by the fresh UID,
with no group/world write bits or set-ID bits. Its retained FD and final named
identity must remain equal across the bounded header read. A link, bad mode,
foreign owner, script, substitution, missing candidates or failed read refuses.
No system permissions or packaged files are changed. Only this fresh private
bus receives the declared template, under the existing isolated UID/socket and
closed child environment. This source change is not a passed smoke or a native
performance measurement; all lifecycle/collector checks still have to pass in
an actual reviewed-source run.


## Third actual smoke and bounded bootstrap diagnostics

Run `39591285949`, exact source `1fc6b579bf864fce784b225e9fa780a4878f4849`,
passed the activator preflight and standalone cold build. Its digest-verified
single-file artifact records `fixture_scope_or_compositor_failed`, duration
2.42 seconds and `cleanupConfirmed:true`. That exact refusal occurs after an
owned Wayland socket was observed and collector/app launchers were created. It
folds collector and compositor exits together. Neither the failing child nor a
host capability defect, renderer initialization or a native accepted frame is
proved by this result. It is not a successful smoke.

A subsequent source-only diagnostic adds the fixed `bootstrap` record to the
existing result. No process output, arguments, paths or additional files are
read. The phases record only these host milestones:

| Phase | Observed milestone |
| --- | --- |
| 0 | Setup entered |
| 1 | Pre-workload scope and UID checks complete |
| 2 | Compositor launcher created |
| 3 | Owned Wayland socket observed |
| 4 | Collector launcher created |
| 5 | App launcher created |
| 6 | App launcher exited zero |
| 7 | Collector exited zero |
| 8 | Existing completion-marker and app-cgroup drain checks passed |

Owned child exit codes are sampled before the runner sends TERM/KILL. A null
means no exit observed (including not started);256 means status unavailable,
never success. `artifactExportRejected` marks an exception in the existing
numeric-export/manifest-write block; the original primary refusal remains
unchanged. All producer files still require the original pinned-directory,
owner, stopped-writer and all-source validation. A rejected early/empty numeric
file gains no fallback read, partial export or authority to pass. These fields
only narrow a later actual failure; synthetic tests do not qualify native work.

## Fourth actual smoke and closed collector exit diagnostics

Run `39594587871`, exact source `ea0ec1349fea5411803fd642a9ff1d5b982aa6bb`,
also passed the activator preflight and standalone cold build. Its digest-verified
single-file result records phase 5, `collectorExit:1`, no observed app/compositor
exit, `artifactExportRejected:true` and `cleanupConfirmed:true`. The collector
exited before the runner's cleanup, but the former generic status 1 does not
identify its exact refusal or prove whether it preceded a native startup error.
No renderer initialization, frame, host-capability defect or smoke qualification
is established by that result.

The collector now uses this fixed source-authored exit manifest for the same 39
reasons already accepted by `failure_summary`. It classifies only an exact
`Refusal` with one known string argument. Other exceptions, unknown/private
refusals, subclasses and malformed arguments remain status 1; their payloads are
never formatted. Existing JSON classification, the fixed stderr message and all
failure/cleanup/export checks remain unchanged. An OS signal is not a refusal:
the owned child's negative `Popen` signal status remains distinct. A success
still requires the original checks and status 0, never any diagnostic status.

| Collector exit | Closed refusal reason |
| --- | --- |
| 64 | `unexpected_metadata_type` |
| 65 | `metadata_limit` |
| 66 | `invalid_fixture_root` |
| 67 | `foreign_fixture_root` |
| 68 | `invalid_fixture_marker` |
| 69 | `process_metadata_limit` |
| 70 | `invalid_process_metadata` |
| 71 | `process_count_limit` |
| 72 | `invalid_process_list` |
| 73 | `invalid_process_scope` |
| 74 | `unexpected_helper_uid` |
| 75 | `process_scope_changed` |
| 76 | `helper_network_scope_changed` |
| 77 | `process_identity_changed` |
| 78 | `invalid_group_cpu` |
| 79 | `group_cpu_unavailable` |
| 80 | `group_memory_unavailable` |
| 81 | `group_metadata_limit` |
| 82 | `group_clock_regressed` |
| 83 | `cgroup_tree_limit` |
| 84 | `process_scope_sample_raced` |
| 85 | `invalid_cgroup_child` |
| 86 | `collector_privilege_missing` |
| 87 | `dedicated_group_missing` |
| 88 | `compositor_scope_missing` |
| 89 | `app_pid_not_observed` |
| 90 | `invalid_app_pid` |
| 91 | `foreign_app_pid` |
| 92 | `network_scope_not_isolated` |
| 93 | `collector_deadline` |
| 94 | `unattributed_fresh_account_process` |
| 95 | `process_identity_limit` |
| 96 | `numeric_log_limit` |
| 97 | `app_not_live_at_ready` |
| 98 | `app_helpers_did_not_drain` |
| 99 | `native_completion_not_proven` |
| 100 | `not_disposable_collector` |
| 101 | `invalid_collector_arguments` |
| 102 | `invalid_fixture_uid` |

These codes are retained by the existing pre-cleanup `bootstrap.collectorExit`
field even when numeric export is rejected. They add no process/file/log read,
raw payload, partial export or permission fallback. They describe an observed
collector refusal, not necessarily the first cause of a wider startup failure.
All nonzero statuses still fail. Controlled tests bind the documented table to
the source literals, exercise the real `__main__` boundary and deliver actual
SIGINT/SIGTERM only to an owned synthetic Python child; no app is launched.

## Fifth actual smoke and supported CPU-stat schema

Run `39596889579`, exact source `4993ed35c27770545ea613ae2e6fdb4f0d97bb43`,
passed the activator preflight and standalone build, then failed at phase 5 with
`collectorExit:78` (`invalid_group_cpu`). Its digest-verified one-file artifact
also records no observed app/compositor exit, rejected artifact export and
confirmed cleanup. This proves the collector's closed classification. The
rejected line, runner kernel configuration and whether another startup error
preceded the collector refusal remain unobserved; no CPU or renderer measurement
is qualified.

The former `[a-z_]+` field-name check rejects the upstream
`core_sched.force_idle_usec` key. Linux's
[v6.17 accounting writer](https://github.com/torvalds/linux/blob/v6.17/kernel/cgroup/rstat.c#L652-L690)
emits that field when core scheduling is compiled in, alongside its base usage,
user, system and nice accounting. The
[cgroup-v2 CPU interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files)
also defines the five controller-dependent CFS bandwidth fields below. This
establishes a source compatibility defect; it does not establish which actual
line Smoke5 rejected.

The collector accepts exactly these ten known keys in any order, with no
duplicate and no unknown key:

| Key | Meaning and use in this collector |
| --- | --- |
| `usage_usec` | Required cumulative CPU execution time; the sole input to CPU deltas |
| `user_usec` | User CPU time; validated, unused |
| `system_usec` | System CPU time; validated, unused |
| `nice_usec` | Nice CPU accounting; validated, unused |
| `core_sched.force_idle_usec` | Conditional core-scheduling forced-idle accounting; validated, unused |
| `nr_periods` | CFS bandwidth period count; validated, unused |
| `nr_throttled` | CFS throttled-period count; validated, unused |
| `throttled_usec` | CFS throttled duration; validated, unused |
| `nr_bursts` | CFS bandwidth burst count; validated, unused |
| `burst_usec` | CFS burst duration; validated, unused |

Time fields use microseconds; the three `nr_` fields are counts. The CFS fields
describe fair-class bandwidth accounting and are not added to execution time.
The other nine fields are optional to this parser; absence of `usage_usec`
always refuses instead of inventing zero. Each present row must contain exactly
one known key and one unsigned ASCII-decimal value of at most 20 digits, bounded
by `u64::MAX`. The existing 8192-byte CPU-stat read, 64-byte memory read and signed
63-bit limits on the used CPU/memory values remain. Unknown keys, duplicates,
malformed numbers and overflow remain failures. No cgroup/UID/namespace guard,
read path, zero-baseline rule, artifact validation or qualification requirement
changes. Controlled fixtures cover the real base-three/full-ten/reordered forms,
nonzero optional values, all per-field numeric boundaries and refusals; no live
host cgroup is read by those tests.

## Failed compositor startup diagnostic

Smoke6, run `39599165236` on `ce554c490cd6816c70ab5a9787eea0cbd263a967`,
passed the preflight/build and failed at phase 5 with compositor-launcher status
134, no observed app/collector exit and confirmed cleanup. Its digest-verified
one-file result contains no native error text. That status alone does not
establish a particular Mutter/GLib failure, renderer frame or usable capability.

A separate anonymous pipe now carries **only stderr of the fixed `gnome-shell`
command** from this fresh hosted-VM session. Root retains its read end; only the
compositor launcher receives the write descriptor. The trusted session script
closes it for the fixed UID/bus/sleep helpers, duplicates it onto the GNOME
child's stderr, and closes the extra parent descriptor. App stdout/stderr,
compositor stdout and all other process output remain discarded. The script
requires the hosted Linux Bash4+ for these FD-close redirections; macOS Bash3.2
cannot qualify that Linux-only shell path. This changes no namespace, sandbox,
cgroup, UID or bus policy. In particular, the compositor still uses the original
host network namespace; the app alone enters its dedicated network namespace.

A root-owned reader starts before launch and continuously drains the pipe,
retaining only the last 64KiB of bytes so terminal failures survive earlier
warnings. After that limit, older bytes are discarded and a closed truncation
flag is set. The reader uses the original absolute runner deadline and bounded
cleanup; no retry or deadline renewal is added. Its descriptor is closed only
by that reader, so a late reader cannot access a reused FD. Only after complete
owned-child/cgroup/UID drain, pipe EOF and bounded reader completion can text
be exported for a run that already failed. Missing EOF, a read error or elapsed
deadline yields only closed incomplete-capture metadata, never invented text.

The two supplementary names are `compositor-startup-diagnostic.json` and
`compositor-startup-diagnostic.txt`. They are written exclusively through the
existing pinned output directory after cleanup, separate from the unchanged
numeric producer export. The text decodes invalid UTF-8 with replacement,
removes ANSI and Unicode control/format codes (preserving LF/tab), replaces the
known fixture/checkout/runner-temp/runner-home paths with fixed labels, and is
again capped to a valid UTF-8 tail of at most 64KiB. Unredacted bytes and unknown
exceptions are never exported. App output, system logs and core files are never
read. Arbitrary
remaining GNOME error text is diagnostic data, never executable instructions.

The JSON labels this as a failed diagnostic with `measurementQualified:false`;
it records only closed capture/EOF/deadline/truncation flags and the bounded text
length. Neither file can satisfy native readiness, numeric evidence, cleanup,
same-source smoke success or pilot qualification. Existing `export_numeric`,
`require_complete`, pass and cleanup criteria are unchanged. Controlled tests
exercise actual anonymous pipes and owned synthetic Python/shell producers,
write-FD inheritance, tail/privacy boundaries, deadline/EOF/drain refusal and
continued rejection of invalid numeric evidence. They start no service or app,
read no host logs and do not qualify a real compositor.
