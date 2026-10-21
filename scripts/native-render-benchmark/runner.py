#!/usr/bin/python3
"""Disposable hosted-Linux fixture provisioning. No normal app is launched.

Root code stays in the trusted checkout. The fresh account never runs a root
script from its writable root. Raw subprocess output is discarded, not logged.
"""
import configparser
import grp
import hashlib
import json
import os
from pathlib import Path
import platform
import pwd
import re
import secrets
import shutil
import signal
import stat
import subprocess
import sys
import time

from collect import PinnedDirectory, Refusal, bounded, checked_root, measurement_mode, membership, numeric_pids, refuse, scoped_pids, uid_inventory


SOURCE = Path(__file__).resolve().parent
CGROUP = Path("/sys/fs/cgroup")
MAX_SECONDS = 55 * 60
MAX_ARTIFACT = 32 * 1024 * 1024
SERVICES = {
    "org.a11y.Bus": ("/usr/libexec/at-spi-bus-launcher", "/usr/lib/at-spi2-core/at-spi-bus-launcher"),
    "ca.desrt.dconf": ("/usr/libexec/dconf-service", "/usr/lib/dconf/dconf-service"),
}
_interrupted = False


class BusServiceRefusal(Refusal):
    """Closed packaged-file diagnostics; never include paths or service text."""
    def __init__(self, service, info):
        self.diagnostic = {"serviceCode": list(SERVICES).index(service) + 1,
                           "regularFile": stat.S_ISREG(info.st_mode),
                           "symbolicLink": stat.S_ISLNK(info.st_mode),
                           "rootOwned": info.st_uid == 0,
                           "writeBits": info.st_mode & 0o022}
        super().__init__("untrusted_bus_service")


def hosted_root():
    if sys.platform != "linux" or os.geteuid() != 0 or any(os.environ.get(key) != value for key, value in (
            ("GITHUB_ACTIONS", "true"), ("RUNNER_ENVIRONMENT", "github-hosted"), ("RUNNER_OS", "Linux"))):
        refuse("disposable_hosted_linux_required")


def command(argv):
    result = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, timeout=30, check=False)
    if result.returncode != 0:
        refuse("fixture_command_failed")


def trusted_sources():
    for name in ("runner.py", "launch.py", "collect.py", "session.sh"):
        path = SOURCE / name
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o022:
            refuse("untrusted_fixture_source")


def service_entry(service, raw):
    if service not in SERVICES or len(raw) > 16384:
        refuse("unsupported_bus_activator")
    parser = configparser.ConfigParser(interpolation=None, strict=True)
    parser.read_string(raw.decode("utf-8"))
    entry = parser["D-BUS Service"]
    executable = entry.get("Exec", "")
    if entry.get("Name") != service or executable not in SERVICES[service]:
        refuse("unsupported_bus_activator")
    return executable, "[D-BUS Service]\nName=" + service + "\nExec=" + executable + "\n"


def checked_service(service):
    if service not in SERVICES:
        refuse("unsupported_bus_activator")
    source = Path("/usr/share/dbus-1/services") / (service + ".service")
    info = source.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise BusServiceRefusal(service, info)
    executable, text = service_entry(service, bounded(source, 16384))
    metadata = Path(executable).stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
        refuse("untrusted_bus_activator")
    return text


def install_bus(root, directory, address, uid):
    directory.mkdir(mode=0o700)
    for service in SERVICES:
        text = checked_service(service)
        # Deliberately omit SystemdService/User and any other packaged directives.
        (directory / (service + ".service")).write_text(text, encoding="ascii")
    # Only the fresh account reads the two explicit service files.
    os.chown(directory, uid, -1)
    for entry in directory.iterdir():
        os.chown(entry, uid, -1)
        entry.chmod(0o600)
    return ('<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" '
            '"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">\n'
            '<busconfig><type>session</type><listen>unix:path=' + str(address) + '</listen>'
            '<auth>EXTERNAL</auth><servicedir>' + str(directory) + '</servicedir>'
            '<policy context="default"><allow own="*"/><allow send_destination="*"/>'
            '<allow receive_sender="*"/></policy></busconfig>\n')


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for _ in range(2048):
            part = source.read(1024 * 1024)
            if not part:
                return digest.hexdigest()
            digest.update(part)
    refuse("fixture_binary_limit")


def numeric_tree(value, depth=0):
    """Bound numeric structure/string alphabet; not a semantic content allowlist."""
    if depth > 12:
        refuse("artifact_depth_limit")
    if value is None or type(value) is bool:
        return
    if type(value) in (int, float):
        if not (-2**64 < value < 2**64) or value != value:
            refuse("artifact_numeric_limit")
        return
    if type(value) is str:
        if re.fullmatch(r"[a-z0-9][a-z0-9_]{0,95}", value) is None:
            refuse("artifact_text_not_metadata")
        return
    if type(value) is list and len(value) <= 512:
        for item in value:
            numeric_tree(item, depth + 1)
        return
    if type(value) is dict and len(value) <= 128:
        for key, item in value.items():
            if type(key) is not str or re.fullmatch(r"[A-Za-z][A-Za-z0-9]{0,95}", key) is None:
                refuse("artifact_key_not_metadata")
            numeric_tree(item, depth + 1)
        return
    refuse("artifact_shape_limit")


def export_numeric(source, output, *, native_uid, writers_stopped, collector_uid=0, require_complete=False):
    # Only the actual pre-workload pinned instance is passed by execute. A Path
    # cannot obtain this privilege; there is deliberately no compatibility read.
    if not isinstance(source, PinnedDirectory):
        refuse("pinned_artifact_source_required")
    if writers_stopped is not True:
        refuse("artifact_writers_not_stopped")
    if type(require_complete) is not bool:
        refuse("invalid_artifact_export")
    artifacts = []
    for name in ("native.ndjson", "process.ndjson", "collector-summary.json", "collector-failure.json"):
        owner = native_uid if name == "native.ndjson" else collector_uid
        raw = source.read_artifact(name, owner, MAX_ARTIFACT)
        if raw is None:
            continue
        if not raw:
            refuse("empty_numeric_artifact")
        lines = raw.splitlines() if name.endswith("ndjson") else [raw]
        if len(lines) > 100_000:
            refuse("artifact_row_limit")
        for line in lines:
            if len(line) > 256 * 1024:
                refuse("artifact_line_limit")
            numeric_tree(json.loads(line))
        # Validate every allowed source before exporting any original bytes.
        # Four fixed files, each <=32MiB; this staging is bounded to128MiB.
        artifacts.append((name, raw.decode("ascii")))
    if require_complete and not {"native.ndjson", "process.ndjson", "collector-summary.json"}.issubset(
            {name for name, _ in artifacts}):
        refuse("required_artifact_missing")
    source.current()
    with PinnedDirectory(output) as destination:
        for name, raw in artifacts:
            source.current()
            destination.write_new(name, raw)


def wait_for(condition, deadline, children=(), honor_interrupt=True):
    while not condition():
        if honor_interrupt and _interrupted or time.monotonic() >= deadline:
            refuse("fixture_wait_expired")
        if any(child.poll() is not None for child in children):
            refuse("fixture_child_exited")
        time.sleep(0.1)


def owned_group_empty(group):
    return not scoped_pids(group)


def remove_group(group, expected_inode):
    if group.lstat().st_ino != expected_inode or group.lstat().st_uid != 0:
        refuse("cleanup_scope_changed")
    # cgroup.kill is hierarchical and applies only to this new owned group.
    (group / "cgroup.kill").write_text("1", encoding="ascii")
    wait_for(lambda: owned_group_empty(group), time.monotonic() + 5, honor_interrupt=False)
    directories, pending = [], [(group, 0)]
    while pending:
        parent, depth = pending.pop()
        for entry in parent.iterdir():
            if entry.is_symlink():
                refuse("cleanup_tree_limit")
            if entry.is_dir():
                if depth >= 4 or len(directories) >= 15:
                    refuse("cleanup_tree_limit")
                directories.append(entry)
                pending.append((entry, depth + 1))
    for entry in sorted(directories, key=lambda item: len(item.parts), reverse=True):
        entry.rmdir()
    group.rmdir()


class FixtureIdentity:
    """Retained exact passwd/group identity; uncertainty permanently revokes cleanup.

    Empty UID inventory is required before chown or the first workload. Merely
    allocating a new passwd name/UID does not establish process ownership.
    """
    def __init__(self, account, home):
        self.account, self.home = account, str(home)
        self.vacant = False
        self.revoked = False
        identity = pwd.getpwnam(account)
        group = grp.getgrnam(account)
        self.uid, self.gid, self.shell = identity.pw_uid, identity.pw_gid, identity.pw_shell
        if (not 1000 < self.uid <= 2**31 - 1 or not 0 < self.gid <= 2**31 - 1
                or identity.pw_dir != self.home or self.shell != "/usr/sbin/nologin"
                or group.gr_gid != self.gid or group.gr_mem):
            refuse("fixture_uid_not_fresh")
        self.verify(require_vacancy=False)

    def matches_account(self, value):
        return (value.pw_name == self.account and value.pw_uid == self.uid and value.pw_gid == self.gid
                and value.pw_dir == self.home and value.pw_shell == self.shell)

    def matches_group(self, value):
        return value.gr_name == self.account and value.gr_gid == self.gid and not value.gr_mem

    def verify(self, require_vacancy=True):
        try:
            if self.revoked or require_vacancy and not self.vacant:
                refuse("cleanup_identity_unproven")
            if (not self.matches_account(pwd.getpwnam(self.account))
                    or not self.matches_account(pwd.getpwuid(self.uid))
                    or not self.matches_group(grp.getgrnam(self.account))
                    or not self.matches_group(grp.getgrgid(self.gid))):
                refuse("cleanup_identity_changed")
        except BaseException:
            self.revoked = True
            raise

    def prove_vacant(self):
        try:
            self.verify(require_vacancy=False)
            if uid_inventory(self.uid):
                refuse("fixture_uid_already_live")
            self.verify(require_vacancy=False)
            self.vacant = True
        except BaseException:
            self.revoked = True
            raise

    def drained(self):
        self.verify()
        return not uid_inventory(self.uid)

    def group_after_userdel(self):
        """Account must remain absent; only the retained original group may go."""
        try:
            if self.revoked or not self.vacant:
                refuse("cleanup_identity_unproven")
            for lookup, key in ((pwd.getpwnam, self.account), (pwd.getpwuid, self.uid)):
                try:
                    lookup(key)
                except KeyError:
                    pass
                else:
                    refuse("cleanup_identity_changed")
            groups = []
            for lookup, key in ((grp.getgrnam, self.account), (grp.getgrgid, self.gid)):
                try:
                    groups.append(lookup(key))
                except KeyError:
                    groups.append(None)
            if groups == [None, None]:
                return False  # userdel may already have removed its private group.
            if any(value is None or not self.matches_group(value) for value in groups):
                refuse("cleanup_identity_changed")
            return True
        except BaseException:
            self.revoked = True
            raise


def cleanup_identity(identity):
    """Never infer UID-wide or name-delete rights from an attempted useradd."""
    if identity is None:
        refuse("cleanup_identity_unproven")
    identity.verify()
    escaped = uid_inventory(identity.uid)
    if escaped:
        identity.verify()  # Fresh proof immediately before the destructive call.
        subprocess.run(["/usr/bin/pkill", "-KILL", "-u", str(identity.uid)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
        wait_for(identity.drained, time.monotonic() + 3, honor_interrupt=False)
    if not identity.drained():
        refuse("fixture_uid_did_not_drain")
    identity.verify()  # No deletion of a name/UID that has since been replaced.
    command(["/usr/sbin/userdel", identity.account])
    if identity.group_after_userdel():
        # This rechecks account absence AND both group lookups just before delete.
        if identity.group_after_userdel():
            command(["/usr/sbin/groupdel", identity.account])
    return bool(escaped)


def execute(mode, binary, output):
    actors, fps = measurement_mode(mode)
    start = time.monotonic()
    deadline = start + MAX_SECONDS
    nonce = secrets.token_hex(8)
    root = Path("/tmp") / ("orbitpal-render-" + nonce)
    account = "opr" + nonce
    uid = gid = None
    identity_proof = None
    root_files = runtime_files = None
    root_created = account_created = account_attempted = net_created = net_attempted = False
    root_exposed = identity_cleaned = False
    net_inode = None
    groups = []
    children = []
    failure = "fixture_incomplete"
    cleanup_ok = True
    manifest = None
    bus_diagnostic = None
    try:
        for lookup in (pwd.getpwnam, grp.getgrnam):
            try:
                lookup(account)
            except KeyError:
                pass
            else:
                refuse("fixture_account_preexists")
        root.mkdir(mode=0o700)
        root_created = True
        account_attempted = True
        command(["/usr/sbin/useradd", "--user-group", "--no-create-home", "--home-dir", str(root / "home"),
                 "--shell", "/usr/sbin/nologin", account])
        account_created = True
        identity_proof = FixtureIdentity(account, root / "home")
        identity_proof.prove_vacant()  # Before any chown or first app/compositor use.
        uid, gid = identity_proof.uid, identity_proof.gid
        # Bootstrap beneath a root-owned 0700 ancestor. No workload-controlled
        # path is writable/traversable until all privileged setup is complete.
        owned_directories = []
        for directory in (root, root / "compositor"):
            if directory != root:
                directory.mkdir(mode=0o700)
            owned_directories.append(directory)
            for suffix in ("home", "data", "config", "cache", "runtime"):
                child = directory / suffix
                child.mkdir(mode=0o700)
                owned_directories.append(child)
        (root / "fixture.marker").write_bytes(nonce.encode("ascii"))
        os.chown(root / "fixture.marker", uid, gid)
        host_net = str(Path("/proc/self/ns/net").readlink())
        (root / "host-netns.txt").write_text(host_net, encoding="ascii")
        for domain, home in (("app", root), ("compositor", root / "compositor")):
            configuration = install_bus(root, home / "allowed-services", home / "runtime/session-bus", uid)
            config = root / (domain + "-bus.conf")
            config.write_text(configuration, encoding="ascii")
            config.chmod(0o444)
        # No enabling/moving host controllers. Unsupported dedicated accounting
        # is a qualification failure, not a root-process-only fallback.
        enabled = set(bounded(CGROUP / "cgroup.subtree_control", 4096).split())
        if not {b"cpu", b"memory"}.issubset(enabled):
            refuse("dedicated_controllers_unavailable")
        for name in (root.name, root.name + "-compositor"):
            group = CGROUP / name
            group.mkdir(mode=0o700)
            groups.append((group, group.lstat().st_ino))
            for field in ("cpu.stat", "memory.current", "cgroup.kill"):
                if not (group / field).is_file():
                    refuse("dedicated_accounting_unavailable")
        if Path("/run/netns", root.name).exists():
            refuse("network_scope_preexists")
        net_attempted = True
        command(["/usr/sbin/ip", "netns", "add", root.name])
        net_created = True
        net_inode = Path("/run/netns", root.name).stat().st_ino
        fixed_binary = root / "native-bench"
        with binary.open("rb") as source, fixed_binary.open("xb") as destination:
            shutil.copyfileobj(source, destination, 1024 * 1024)
        fixed_binary.chmod(0o700)
        os.chown(fixed_binary, uid, gid)
        manifest = {"version": 1, "mode": mode, "actors": actors, "framesPerSecond": fps, "backendRequested": "llvmpipe",
                    "nativeBinarySha256": sha256(fixed_binary), "logicalCpuCount": os.cpu_count(),
                    "kernelRelease": platform.release(), "architecture": platform.machine(),
                    "physicalGpuPermissionsChanged": False, "nativeLaneOnly": True,
                    "sourceHashes": {name.replace(".", "_"): sha256(SOURCE / name)
                                     for name in ("runner.py", "launch.py", "collect.py", "session.sh")}}
        def launch(domain):
            return subprocess.Popen(["/usr/bin/python3", str(SOURCE / "launch.py"), str(root), domain,
                                     str(uid), str(gid), mode, str(root_files.fd)], pass_fds=(root_files.fd,),
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # Pin every directory that root will write after the workload starts.
        root_files = PinnedDirectory(root)
        runtime_files = PinnedDirectory(root / "runtime")
        identity_proof.verify()
        for directory in sorted(owned_directories, key=lambda item: len(item.parts), reverse=True):
            os.chown(directory, uid, gid)  # Root ancestor is handed off last.
        root_exposed = True
        checked_root(str(root), uid, root_files)
        compositor = launch("compositor")
        children.append(compositor)
        socket = root / "compositor/runtime/orbitpal-benchmark"
        def socket_ready():
            try:
                value = socket.lstat()
                return stat.S_ISSOCK(value.st_mode) and value.st_uid == uid
            except FileNotFoundError:
                return False
        wait_for(socket_ready, min(deadline, time.monotonic() + 35), (compositor,))
        # The only transport bridged from compositor domain to app domain.
        runtime_files.current()
        os.symlink("../compositor/runtime/orbitpal-benchmark", "orbitpal-benchmark", dir_fd=runtime_files.fd)
        runtime_files.current()
        root_files.current()
        collector = subprocess.Popen(["/usr/bin/python3", str(SOURCE / "collect.py"), "--collect", str(root), str(uid), str(root_files.fd)],
                                     pass_fds=(root_files.fd,), stdin=subprocess.DEVNULL,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        children.append(collector)
        identity_proof.verify()
        app = launch("app")
        children.append(app)
        while app.poll() is None:
            if collector.poll() is not None or compositor.poll() is not None:
                refuse("fixture_scope_or_compositor_failed")
            if _interrupted or time.monotonic() >= deadline:
                refuse("fixture_deadline")
            time.sleep(0.1)
        if app.wait(timeout=2) != 0:
            refuse("native_benchmark_failed")
        if collector.wait(timeout=min(20, max(0.1, deadline - time.monotonic()))) != 0:
            refuse("numeric_collector_failed")
        if root_files.read_metadata("completed.marker", uid) != b"native-renderer-lane-only\n" or not owned_group_empty(groups[0][0]):
            refuse("native_completion_or_scope_drain_failed")
        failure = None
    except Refusal as error:
        # Only fixed authored refusal codes; no raw subprocess output/errors.
        failure = error.args[0] if type(error.args[0]) is str and re.fullmatch(r"[a-z_]{1,96}", error.args[0]) else "fixture_refused"
        if isinstance(error, BusServiceRefusal):
            bus_diagnostic = error.diagnostic
    except Exception:
        failure = "fixture_metadata_unavailable"
    finally:
        # Independent cleanup steps continue after a failure. Exact new group,
        # account and namespace identities are never inferred from process names.
        # Partial useradd has no vacancy/identity proof. Do not adopt/delete a
        # name or group based only on its home string; leave cleanup unconfirmed.
        if account_attempted and (not account_created or identity_proof is None):
            cleanup_ok = False
        for child in children:
            try:
                if child.poll() is None:
                    child.terminate()
            except Exception:
                cleanup_ok = False
        groups_drained = True
        for group, inode in reversed(groups):
            try:
                remove_group(group, inode)
            except Exception:
                groups_drained = False
                cleanup_ok = False
        children_reaped = True
        for child in children:
            try:
                child.wait(timeout=3)
            except Exception:
                children_reaped = False
                cleanup_ok = False
        if account_created:
            try:
                if cleanup_identity(identity_proof):
                    failure = failure or "unattributed_fresh_account_process"
                identity_cleaned = True
            except Exception:
                cleanup_ok = False
        # No artifact read without confirmed UID, child and all cgroup drains.
        # Early bootstrap failure has no pinned source, hence no fallback reads.
        try:
            if root_files is not None:
                export_numeric(root_files, output, native_uid=uid,
                               writers_stopped=groups_drained and children_reaped and identity_cleaned,
                               require_complete=failure is None and cleanup_ok)
            if manifest is not None:
                with PinnedDirectory(output) as destination:
                    destination.write_new("fixture-manifest.json", json.dumps(manifest, sort_keys=True) + "\n")
        except Exception:
            failure = failure or "numeric_artifact_invalid"
        if net_created:
            try:
                if Path("/run/netns", root.name).stat().st_ino != net_inode:
                    refuse("cleanup_network_identity_changed")
                command(["/usr/sbin/ip", "netns", "delete", root.name])
            except Exception:
                cleanup_ok = False
        elif net_attempted and Path("/run/netns", root.name).exists():
            # Failed ip setup may leave a partial mount/name. Without the
            # success-time inode proof, never delete an uncertain namespace or
            # report cleanup as confirmed. This is a hard fixture failure.
            cleanup_ok = False
        if root_created:
            try:
                if root_exposed and not identity_cleaned:
                    refuse("cleanup_identity_unproven")
                if root_files is not None:
                    root_files.current()
                if not shutil.rmtree.avoids_symlink_attacks:
                    refuse("cleanup_tree_unavailable")
                shutil.rmtree(root)
            except Exception:
                cleanup_ok = False
        for directory in (runtime_files, root_files):
            if directory is not None:
                directory.close()
    result = {"version": 1, "passed": failure is None and cleanup_ok, "reason": failure,
              "cleanupConfirmed": cleanup_ok, "durationSeconds": time.monotonic() - start,
              "fullIssue66Acceptance": False, "hardwarePerformanceBaseline": False}
    if bus_diagnostic is not None:
        result["busService"] = bus_diagnostic
    with PinnedDirectory(output) as destination:
        destination.write_new("fixture-result.json", json.dumps(result, sort_keys=True) + "\n")
    # The root collector owns writes; the CI artifact uploader is the original
    # RUNNER_TEMP owner, not this fresh workload UID.
    uploader = output.parent.stat()
    for artifact in output.iterdir():
        os.chown(artifact, uploader.st_uid, uploader.st_gid)
        artifact.chmod(0o600)
    os.chown(output, uploader.st_uid, uploader.st_gid)
    output.chmod(0o700)
    return 0 if result["passed"] else 1


def main():
    hosted_root()
    os.umask(0o077)
    if sys.argv[1:] == ["--check-services"]:
        trusted_sources()
        try:
            for service in SERVICES:
                checked_service(service)
        except BusServiceRefusal as error:
            print(json.dumps({"passed": False, "reason": "untrusted_bus_service", "busService": error.diagnostic}, sort_keys=True))
            return 1
        print(json.dumps({"passed": True, "servicesChecked": len(SERVICES)}, sort_keys=True))
        return 0
    if len(sys.argv) != 5 or sys.argv[1] != "--mode" or sys.argv[3] != "--binary":
        refuse("invalid_fixture_arguments")
    measurement_mode(sys.argv[2])
    binary = Path(sys.argv[4])
    info = binary.lstat()
    if not binary.is_absolute() or not stat.S_ISREG(info.st_mode) or not info.st_mode & 0o111:
        refuse("invalid_fixture_binary")
    temporary = Path(os.environ.get("RUNNER_TEMP", ""))
    if not temporary.is_absolute() or not temporary.is_dir() or temporary.is_symlink():
        refuse("invalid_artifact_directory")
    output = temporary / ("native-render-benchmark-" + sys.argv[2])
    output.mkdir(mode=0o700)  # Never mix/reuse artifacts from an earlier run.
    trusted_sources()
    signal.signal(signal.SIGTERM, lambda *_: globals().__setitem__("_interrupted", True))
    signal.signal(signal.SIGINT, lambda *_: globals().__setitem__("_interrupted", True))
    return execute(sys.argv[2], binary, output)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        print("benchmark-fixture-refused", file=sys.stderr)
        raise SystemExit(1)
