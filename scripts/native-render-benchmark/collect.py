#!/usr/bin/python3
"""Numeric collector for ONE fresh disposable UID and its dedicated cgroup.

Never reads argv, environments, maps, windows, screenshots, audio or credentials.
The collector is outside the app cgroup/network namespace. It is not a desktop
profiler and refuses an existing account/session or an unqualified process scope.
"""
import json
import os
from pathlib import Path
import re
import stat
import sys
import time

MAX_LOG = 32 * 1024 * 1024
MAX_PIDS = 256
MAX_GROUPS = 16
MEASUREMENT_MODES = {"smoke": (1, 30), "pilot": (1, 30), "pilot3": (3, 30), "pilot5": (5, 20)}
# Supported cgroup-v2 cpu.stat keys, including CONFIG_SCHED_CORE's dotted key.
# Optional accounting/bandwidth fields never substitute for or add to usage_usec.
CPU_STAT_FIELDS = frozenset((
    b"usage_usec", b"user_usec", b"system_usec", b"nice_usec", b"core_sched.force_idle_usec",
    b"nr_periods", b"nr_throttled", b"throttled_usec", b"nr_bursts", b"burst_usec",
))
# Stable diagnostic-only manifest for the existing closed failure classification.
# Never renumber existing entries or derive a status from arbitrary error data.
# All are nonzero and below shell signal statuses; unclassified errors remain 1.
FAILURE_EXIT_CODES = {
    "unexpected_metadata_type": 64,
    "metadata_limit": 65,
    "invalid_fixture_root": 66,
    "foreign_fixture_root": 67,
    "invalid_fixture_marker": 68,
    "process_metadata_limit": 69,
    "invalid_process_metadata": 70,
    "process_count_limit": 71,
    "invalid_process_list": 72,
    "invalid_process_scope": 73,
    "unexpected_helper_uid": 74,
    "process_scope_changed": 75,
    "helper_network_scope_changed": 76,
    "process_identity_changed": 77,
    "invalid_group_cpu": 78,
    "group_cpu_unavailable": 79,
    "group_memory_unavailable": 80,
    "group_metadata_limit": 81,
    "group_clock_regressed": 82,
    "cgroup_tree_limit": 83,
    "process_scope_sample_raced": 84,
    "invalid_cgroup_child": 85,
    "collector_privilege_missing": 86,
    "dedicated_group_missing": 87,
    "compositor_scope_missing": 88,
    "app_pid_not_observed": 89,
    "invalid_app_pid": 90,
    "foreign_app_pid": 91,
    "network_scope_not_isolated": 92,
    "collector_deadline": 93,
    "unattributed_fresh_account_process": 94,
    "process_identity_limit": 95,
    "numeric_log_limit": 96,
    "app_not_live_at_ready": 97,
    "app_helpers_did_not_drain": 98,
    "native_completion_not_proven": 99,
    "not_disposable_collector": 100,
    "invalid_collector_arguments": 101,
    "invalid_fixture_uid": 102,
}
FAILURE_CODES = frozenset(FAILURE_EXIT_CODES)


class Refusal(Exception):
    pass


def refuse(code):
    raise Refusal(code)


def measurement_mode(mode):
    if type(mode) is not str or mode not in MEASUREMENT_MODES:
        refuse("invalid_benchmark_mode")
    return MEASUREMENT_MODES[mode]


def failure_summary(error):
    code = "metadata_unavailable"
    if type(error) is Refusal and len(error.args) == 1 and type(error.args[0]) is str and error.args[0] in FAILURE_CODES:
        code = error.args[0]
    return {"version": 1, "passed": False, "reason": code}


def failure_exit_code(error):
    return FAILURE_EXIT_CODES.get(failure_summary(error)["reason"], 1)


class PinnedDirectory:
    """No-follow ancestry retained before writes; leaf creation is always exclusive.

    A renamed/replaced parent is a refusal, not permission to reopen by pathname.
    The openat descriptor still confines a concurrent rename to the original
    directory; a final ancestry/leaf check refuses evidence from that race.
    """
    def __init__(self, path, expected_fd=None):
        self.path = Path(path)
        self.chain = []
        if not self.path.is_absolute() or ".." in self.path.parts or len(self.path.parts) > 32:
            refuse("invalid_write_directory")
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
        try:
            self.chain.append((None, os.open("/", flags)))
            for name in self.path.parts[1:]:
                self.chain.append((name, os.open(name, flags, dir_fd=self.chain[-1][1])))
            if expected_fd is not None and self.identity(os.fstat(expected_fd)) != self.identity(os.fstat(self.fd)):
                refuse("write_directory_changed")
            self.current()
        except BaseException:
            self.close()
            raise

    @staticmethod
    def identity(value):
        return value.st_dev, value.st_ino

    @property
    def fd(self):
        return self.chain[-1][1]

    def current(self):
        if not self.chain:
            refuse("write_directory_changed")
        for index, (name, descriptor) in enumerate(self.chain):
            actual = os.fstat(descriptor)
            if not stat.S_ISDIR(actual.st_mode):
                refuse("write_directory_changed")
            if index:
                linked = os.stat(name, dir_fd=self.chain[index - 1][1], follow_symlinks=False)
                if not stat.S_ISDIR(linked.st_mode) or self.identity(linked) != self.identity(actual):
                    refuse("write_directory_changed")
        return self

    def linked_file(self, name, descriptor):
        self.current()
        value = os.fstat(descriptor)
        linked = os.stat(name, dir_fd=self.fd, follow_symlinks=False)
        if (not stat.S_ISREG(value.st_mode) or value.st_nlink != 1
                or not stat.S_ISREG(linked.st_mode) or self.identity(value) != self.identity(linked)):
            refuse("write_leaf_changed")

    def open_new(self, name, owner=None):
        if re.fullmatch(r"[a-z][a-z0-9_.-]{0,95}", name) is None:
            refuse("invalid_write_leaf")
        self.current()
        descriptor = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
                             | os.O_CLOEXEC | os.O_NONBLOCK, 0o600, dir_fd=self.fd)
        try:
            self.linked_file(name, descriptor)
            if owner is not None:
                os.fchown(descriptor, owner, -1)
            return os.fdopen(descriptor, "w", encoding="ascii")
        except BaseException:
            os.close(descriptor)
            raise

    def write_new(self, name, value, owner=None):
        with self.open_new(name, owner) as output:
            output.write(value)
            output.flush()
            self.linked_file(name, output.fileno())

    def read_artifact(self, name, owner, limit):
        """Read only a bound single-link regular file of the expected producer.

        Absence is optional; changed ancestry, links, owner, size or metadata is
        a refusal. No caller may reopen a pathname after that refusal.
        """
        if name not in ("native.ndjson", "process.ndjson", "collector-summary.json", "collector-failure.json"):
            refuse("invalid_artifact_read")
        return self._read_bound(name, owner, limit)

    def read_metadata(self, name, owner):
        limits = {"fixture.marker": 16, "host-netns.txt": 64, "app.pid": 16, "completed.marker": 64}
        if name not in limits:
            refuse("invalid_metadata_read")
        return self._read_bound(name, owner, limits[name])

    def _read_bound(self, name, owner, limit):
        if (type(owner) is not int or not 0 <= owner <= 2**31 - 1
                or type(limit) is not int or not 0 <= limit <= MAX_LOG):
            refuse("invalid_artifact_read")
        self.current()
        try:
            descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC,
                                 dir_fd=self.fd)
        except FileNotFoundError:
            self.current()  # An ancestor disappearance is never a missing leaf.
            try:
                os.stat(name, dir_fd=self.fd, follow_symlinks=False)
            except FileNotFoundError:
                return None
            refuse("artifact_read_changed")
        try:
            before = os.fstat(descriptor)
            self.artifact_metadata(before, owner, limit)
            self.linked_file(name, descriptor)
            with os.fdopen(descriptor, "rb", closefd=False) as stream:
                value = stream.read(limit + 1)
            after = os.fstat(descriptor)
            self.artifact_metadata(after, owner, limit)
            self.linked_file(name, descriptor)
            if self.artifact_stamp(before) != self.artifact_stamp(after) or len(value) != after.st_size:
                refuse("artifact_read_changed")
            return value
        finally:
            os.close(descriptor)

    @staticmethod
    def artifact_metadata(value, owner, limit):
        if not stat.S_ISREG(value.st_mode) or value.st_nlink != 1 or value.st_uid != owner:
            refuse("artifact_read_identity")
        if not 0 <= value.st_size <= limit:
            refuse("artifact_read_size")

    @staticmethod
    def artifact_stamp(value):
        return (value.st_dev, value.st_ino, value.st_mode, value.st_nlink, value.st_uid,
                value.st_gid, value.st_size, value.st_mtime_ns, value.st_ctime_ns)

    def close(self):
        while self.chain:
            os.close(self.chain.pop()[1])

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def bounded(path, limit):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            refuse("unexpected_metadata_type")
        value = stream.read(limit + 1)
    if len(value) > limit:
        refuse("metadata_limit")
    return value


def checked_root(raw, uid, files):
    root = Path(raw)
    if root.parent != Path("/tmp") or re.fullmatch(r"orbitpal-render-[0-9a-f]{16}", root.name) is None:
        refuse("invalid_fixture_root")
    if not isinstance(files, PinnedDirectory) or files.path != root:
        refuse("foreign_fixture_root")
    files.current()
    meta = os.fstat(files.fd)
    if not stat.S_ISDIR(meta.st_mode) or meta.st_uid != uid or meta.st_mode & 0o077:
        refuse("foreign_fixture_root")
    if files.read_metadata("fixture.marker", uid) != root.name.removeprefix("orbitpal-render-").encode("ascii"):
        refuse("invalid_fixture_marker")
    return root


def parse_stat(raw):
    """Discard comm entirely; return only fixed documented numeric fields."""
    if len(raw) > 8192:
        refuse("process_metadata_limit")
    end = raw.rfind(b") ")
    if end < 0:
        refuse("invalid_process_metadata")
    fields = raw[end + 2:].split()
    if len(fields) < 22 or fields[0] not in (b"R", b"S", b"D", b"Z", b"T", b"t", b"X", b"I", b"P", b"W"):
        refuse("invalid_process_metadata")
    try:
        values = [int(fields[index]) for index in (11, 12, 19, 21)]
    except ValueError:
        refuse("invalid_process_metadata")
    if any(value < 0 or value > 2**63 - 1 for value in values):
        refuse("invalid_process_metadata")
    return {"cpuTicks": values[0] + values[1], "startTicks": values[2], "rssPages": values[3],
            "exited": fields[0] in (b"Z", b"X")}


def numeric_pids(raw):
    if len(raw) > 4096:
        refuse("process_count_limit")
    values = raw.splitlines()
    if len(values) > MAX_PIDS or any(re.fullmatch(rb"[1-9][0-9]{0,9}", value) is None for value in values):
        refuse("invalid_process_list")
    result = [int(value) for value in values]
    if len(set(result)) != len(result) or any(pid > 2**31 - 1 for pid in result):
        refuse("invalid_process_list")
    return result


def uid_inventory(uid):
    """Only directory owner metadata is consulted for other UIDs."""
    result = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isascii() or not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid != uid:
                continue
            birth = parse_stat(bounded(entry / "stat", 8192))["startTicks"]
            if entry.stat().st_uid != uid:
                continue
            result[entry.name] = birth
        except (FileNotFoundError, ProcessLookupError):
            continue
        if len(result) > MAX_PIDS:
            refuse("process_count_limit")
    return result


def membership(raw):
    if re.fullmatch(rb"0::/[A-Za-z0-9_.\-/]{1,512}\n", raw) is None:
        refuse("invalid_process_scope")
    name = raw[4:-1].decode("ascii")
    parts = name.split("/")
    if len(parts) > 5 or any(not part or part in (".", "..") or len(part) > 96 for part in parts):
        refuse("invalid_process_scope")
    return name


def in_domain(name, root_name):
    return name == root_name or name.startswith(root_name + "/")


def sample_process(pid, expected_uid, group_name, namespace):
    path = Path("/proc") / str(pid)
    try:
        uid = path.stat().st_uid
        if expected_uid is not None and uid != expected_uid:
            refuse("unexpected_helper_uid")
        before = parse_stat(bounded(path / "stat", 8192))
        if before["exited"]:
            return None
        if membership(bounded(path / "cgroup", 4096)) != group_name:
            refuse("process_scope_changed")
        if str((path / "ns/net").readlink()) != namespace:
            refuse("helper_network_scope_changed")
        after = parse_stat(bounded(path / "stat", 8192))
        if before["startTicks"] != after["startTicks"] or path.stat().st_uid != uid:
            refuse("process_identity_changed")
        if membership(bounded(path / "cgroup", 4096)) != group_name or str((path / "ns/net").readlink()) != namespace:
            refuse("process_scope_changed")
        final = parse_stat(bounded(path / "stat", 8192))
        if final["startTicks"] != before["startTicks"] or path.stat().st_uid != uid:
            refuse("process_identity_changed")
        if final["exited"]:
            return None
        return {"pid": pid, "uid": uid, **final}
    except (FileNotFoundError, ProcessLookupError):
        return None  # Group CPU retains work from short-lived children.


def group_numbers(group):
    fields = {}
    for line in bounded(group / "cpu.stat", 8192).splitlines():
        pair = line.split()
        if len(pair) != 2 or pair[0] not in CPU_STAT_FIELDS or not pair[1].isdigit():
            refuse("invalid_group_cpu")
        if pair[0] in fields:
            refuse("invalid_group_cpu")
        if len(pair[1]) > 20:
            refuse("group_metadata_limit")
        value = int(pair[1])
        if value > 2**64 - 1:
            refuse("group_metadata_limit")
        fields[pair[0]] = value
    if b"usage_usec" not in fields:
        refuse("group_cpu_unavailable")
    memory = bounded(group / "memory.current", 64).strip()
    if not memory.isdigit():
        refuse("group_memory_unavailable")
    usage, memory_value = fields[b"usage_usec"], int(memory)
    if usage > 2**63 - 1 or memory_value > 2**63 - 1:
        refuse("group_metadata_limit")
    return usage, memory_value


def cpu_delta(previous, now, cpu):
    if previous is None:
        return None, None  # No invented zero-usage first interval.
    seconds, micros = now - previous[0], cpu - previous[1]
    if seconds <= 0 or micros < 0:
        refuse("group_clock_regressed")
    return micros / (seconds * 10000), seconds


def scoped_pids(group):
    """Include all UIDs and bounded descendant cgroups; never filter group CPU."""
    pending = [(group, group.name, 0)]
    result = {}
    groups = 0
    while pending:
        directory, name, depth = pending.pop()
        groups += 1
        if groups > MAX_GROUPS or depth > 4 or directory.is_symlink():
            refuse("cgroup_tree_limit")
        for pid in numeric_pids(bounded(directory / "cgroup.procs", 4096)):
            if pid in result:
                refuse("process_scope_sample_raced")
            result[pid] = name
            if len(result) > MAX_PIDS:
                refuse("process_count_limit")
        for entry in directory.iterdir():
            if entry.is_dir():
                if re.fullmatch(r"[A-Za-z0-9_.-]{1,96}", entry.name) is None or entry.name in (".", ".."):
                    refuse("invalid_cgroup_child")
                pending.append((entry, name + "/" + entry.name, depth + 1))
                if groups + len(pending) > MAX_GROUPS:
                    refuse("cgroup_tree_limit")
    return result


def write_ready(root, uid):
    # Process-scope readiness ONLY, never renderer/frame readiness.
    if isinstance(root, PinnedDirectory):
        root.write_new("collector.ready", "numeric-process-scope-ready\n", uid)
    else:
        with PinnedDirectory(root) as directory:
            directory.write_new("collector.ready", "numeric-process-scope-ready\n", uid)


def inventory_samples(inventory, identities, uid, app_name, compositor_name, namespace, host_namespace):
    extra = []
    for pid, birth in inventory.items():
        if (int(pid), birth) in identities:
            continue
        path = Path("/proc") / pid
        try:
            domain = membership(bounded(path / "cgroup", 4096))
        except (FileNotFoundError, ProcessLookupError):
            continue
        is_app, is_compositor = in_domain(domain, app_name), in_domain(domain, compositor_name)
        if not is_app and not is_compositor:
            refuse("unattributed_fresh_account_process")
        value = sample_process(int(pid), uid, domain, namespace if is_app else host_namespace)
        if value is not None and value["startTicks"] == birth and is_app:
            extra.append(value)
    return extra


def collect(root, uid, files):
    if os.geteuid() != 0:
        refuse("collector_privilege_missing")
    group = Path("/sys/fs/cgroup") / root.name
    if group.is_symlink() or not group.is_dir():
        refuse("dedicated_group_missing")
    compositor_group = Path("/sys/fs/cgroup") / (root.name + "-compositor")
    raw_namespace = files.read_metadata("host-netns.txt", 0)
    host_namespace = raw_namespace.decode("ascii") if raw_namespace is not None else ""
    if (re.fullmatch(r"net:\[[0-9]+\]", host_namespace) is None
            or host_namespace != str(Path("/proc/self/ns/net").readlink())
            or not compositor_group.is_dir() or compositor_group.is_symlink()):
        refuse("compositor_scope_missing")
    start = time.monotonic()
    deadline = start + 55 * 60
    while (raw_pid := files.read_metadata("app.pid", uid)) is None:
        if time.monotonic() > start + 35:
            refuse("app_pid_not_observed")
        time.sleep(0.1)
    if re.fullmatch(rb"[1-9][0-9]{0,9}", raw_pid) is None:
        refuse("invalid_app_pid")
    app_pid = int(raw_pid)
    app_path = Path("/proc") / str(app_pid)
    if app_path.stat().st_uid != uid:
        refuse("foreign_app_pid")
    namespace = str((app_path / "ns/net").readlink())
    if namespace == str(Path("/proc/self/ns/net").readlink()):
        refuse("network_scope_not_isolated")
    app_start = parse_stat(bounded(app_path / "stat", 8192))["startTicks"]
    previous = None
    rows = 0
    bytes_written = 0
    observed = set()
    process_exits = 0
    previous_pids = set()
    drain_started = None
    page_size, clock_ticks = os.sysconf("SC_PAGE_SIZE"), os.sysconf("SC_CLK_TCK")
    with files.open_new("process.ndjson") as out:
        while True:
            now = time.monotonic()
            if now > deadline:
                refuse("collector_deadline")
            pids = scoped_pids(group)
            samples = [value for pid, domain in pids.items() if (value := sample_process(pid, None, domain, namespace)) is not None]
            identities = {(value["pid"], value["startTicks"]) for value in samples}
            inventory = uid_inventory(uid)
            # App-owned activation on a separate private bus remains inside the
            # app scope. No new fresh-account helper can silently escape it.
            samples.extend(inventory_samples(inventory, identities, uid, root.name, compositor_group.name, namespace, host_namespace))
            if len(samples) > MAX_PIDS:
                refuse("process_count_limit")
            identities = {(value["pid"], value["startTicks"]) for value in samples}
            cpu, charged = group_numbers(group)
            cpu_percent, gap = cpu_delta(previous, now, cpu)
            process_exits += len(previous_pids - identities)
            observed |= identities
            if len(observed) > 4096:
                refuse("process_identity_limit")
            previous_pids, previous = identities, (now, cpu)
            row = {"monotonicSeconds": now - start, "monotonicUsec": int(now * 1_000_000), "cpuUsecCumulative": cpu,
                   "cpuPercentOneCore": cpu_percent, "sampleGapSeconds": gap,
                   "summedRssBytesMayDoubleCountSharedPages": sum(value["rssPages"] * page_size for value in samples),
                   "cgroupChargedMemoryBytesNotRss": charged, "processes": samples,
                   "pageBytes": page_size, "clockTicksPerSecond": clock_ticks}
            encoded = json.dumps(row, separators=(",", ":"), allow_nan=False) + "\n"
            bytes_written += len(encoded)
            if bytes_written > MAX_LOG:
                refuse("numeric_log_limit")
            out.write(encoded)
            out.flush()
            files.linked_file("process.ndjson", out.fileno())
            rows += 1
            if rows == 1:
                if (app_pid, app_start) not in identities:
                    refuse("app_not_live_at_ready")
                write_ready(files, uid)
            live_app = (app_pid, app_start) in identities
            if not live_app:
                drain_started = drain_started or now
                if not scoped_pids(group):
                    break
                if now - drain_started > 15:
                    refuse("app_helpers_did_not_drain")
            time.sleep(max(0.0, 1.0 - (time.monotonic() - now)))
    if files.read_metadata("completed.marker", uid) != b"native-renderer-lane-only\n":
        refuse("native_completion_not_proven")
    summary = {"version": 1, "scope": "native_figure_bank_plus_control_and_dedicated_app_bus_helpers",
               "samples": rows, "observedProcessIdentities": len(observed), "observedExits": process_exits,
               "durationSeconds": time.monotonic() - start, "cpuDefinition": "100_percent_is_one_logical_cpu",
               "rssDefinition": "sum_may_double_count_shared_pages", "gpuDuration": None,
               "gpuUtilization": None, "actualPresentTiming": None, "missingMetricsReason": "not_qualified",
               "fullAppPerformanceOrIssue66Acceptance": False}
    files.write_new("collector-summary.json", json.dumps(summary, sort_keys=True) + "\n")


def main():
    for name, value in (("GITHUB_ACTIONS", "true"), ("RUNNER_OS", "Linux"), ("RUNNER_ENVIRONMENT", "github-hosted")):
        if os.environ.get(name) != value:
            refuse("not_disposable_collector")
    if (len(sys.argv) != 5 or sys.argv[1] != "--collect"
            or any(not value.isascii() or not value.isdigit() or len(value) > 10 for value in sys.argv[3:])):
        refuse("invalid_collector_arguments")
    uid = int(sys.argv[3])
    if uid <= 1000 or uid > 2**31 - 1:
        refuse("invalid_fixture_uid")
    witness = int(sys.argv[4])
    if not 3 <= witness <= 2**31 - 1:
        refuse("invalid_collector_arguments")
    os.set_inheritable(witness, False)
    # Pin against the inherited pre-workload identity before reading any marker.
    with PinnedDirectory(Path(sys.argv[2]), witness) as files:
        os.close(witness)
        root = checked_root(sys.argv[2], uid, files)
        try:
            collect(root, uid, files)
        except Exception as error:
            files.write_new("collector-failure.json", json.dumps(failure_summary(error), sort_keys=True) + "\n")
            raise


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("benchmark-collector-failed", file=sys.stderr)
        raise SystemExit(failure_exit_code(error))
