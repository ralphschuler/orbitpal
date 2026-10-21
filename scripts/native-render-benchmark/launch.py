#!/usr/bin/python3
"""Root pre-exec placement, then UID drop and ONLY a fixed session command.

Run from the trusted checkout, never a copy beneath the fixture-owned root.
No generic command, executable, environment or network-namespace argument.
"""
import os
from pathlib import Path
import pwd
import re
import stat
import sys

from collect import PinnedDirectory, Refusal, bounded, checked_root, measurement_mode, refuse
from compositor_diagnostic import checked_write_pipe


def environment(root, account, domain, mode, host_net):
    measurement_mode(mode)
    app = domain == "app"
    home = root if app else root / "compositor"
    values = {"PATH": "/usr/bin:/bin", "HOME": str(home / "home"),
              "USER": account, "LOGNAME": account, "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8",
              "GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux",
              "ORBITPAL_NATIVE_RENDER_BENCH": "disposable-linux-only", "ORBITPAL_BENCH_ROOT": str(root),
              "ORBITPAL_BENCH_HOST_NETNS": host_net, "ORBITPAL_BENCH_DOMAIN": domain,
              "ORBITPAL_BENCH_MODE": mode, "XDG_SESSION_TYPE": "wayland",
              # GNOME 50 constructs system-bus proxies during startup. Give only
              # the compositor an empty private bus, never the host system bus.
              "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(home / (
                  "runtime/nonexistent-system-bus" if app else "runtime/system-bus")),
              "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(home / "runtime/session-bus"),
              "LIBGL_ALWAYS_SOFTWARE": "1", "GALLIUM_DRIVER": "llvmpipe"}
    for key, suffix in (("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"),
                        ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime")):
        values[key] = str(home / suffix)
    if app:
        values.update(GDK_BACKEND="wayland", WAYLAND_DISPLAY="orbitpal-benchmark")
    else:
        values["XDG_CURRENT_DESKTOP"] = "GNOME"
    return values


def main():
    if os.geteuid() != 0 or sys.platform != "linux" or any(os.environ.get(key) != value for key, value in (
            ("GITHUB_ACTIONS", "true"), ("RUNNER_ENVIRONMENT", "github-hosted"), ("RUNNER_OS", "Linux"))):
        refuse("not_disposable_launcher")
    if len(sys.argv) not in (7, 8) or sys.argv[2] not in ("app", "compositor"):
        refuse("invalid_launcher_arguments")
    diagnostic_fd = None
    if len(sys.argv) == 8:
        if sys.argv[2] != "compositor" or re.fullmatch(r"[1-9][0-9]{0,9}", sys.argv[7]) is None:
            refuse("invalid_launcher_arguments")
        diagnostic_fd = int(sys.argv[7])
        checked_write_pipe(diagnostic_fd)
        os.set_inheritable(diagnostic_fd, True)
    measurement_mode(sys.argv[5])
    if not re.fullmatch(r"[1-9][0-9]{0,9}", sys.argv[3]) or not re.fullmatch(r"[1-9][0-9]{0,9}", sys.argv[4]):
        refuse("invalid_fixture_identity")
    uid, gid = int(sys.argv[3]), int(sys.argv[4])
    if uid <= 1000 or gid <= 0:
        refuse("invalid_fixture_identity")
    if re.fullmatch(r"[1-9][0-9]{0,9}", sys.argv[6]) is None or not 3 <= int(sys.argv[6]) <= 2**31 - 1:
        refuse("invalid_launcher_arguments")
    witness = int(sys.argv[6])
    os.set_inheritable(witness, False)
    # Both launches, including app after compositor startup, use the original
    # runner directory identity. No marker/namespace Path read precedes this.
    with PinnedDirectory(Path(sys.argv[1]), witness) as files:
        os.close(witness)
        root = checked_root(sys.argv[1], uid, files)
        run(root, uid, gid, sys.argv[2], sys.argv[5], files, diagnostic_fd)


def run(root, uid, gid, domain, mode, files, diagnostic_fd=None):
    identity = pwd.getpwuid(uid)
    if identity.pw_name != "opr" + root.name[-16:] or identity.pw_gid != gid or identity.pw_dir != str(root / "home"):
        refuse("foreign_fixture_account")
    source = Path(__file__).resolve().parent
    # App UID must never be able to replace the code that root executes.
    for path in (source, source / "launch.py", source / "session.sh", source / "compositor_diagnostic.py"):
        info = path.stat()
        if info.st_uid == uid or info.st_mode & 0o022:
            refuse("untrusted_launcher_source")
    # Open trusted session bytes before dropping UID, so a private checkout
    # parent need not become world-readable and no writable fixture copy runs.
    session_fd = os.open(source / "session.sh", os.O_RDONLY | os.O_NOFOLLOW)
    os.set_inheritable(session_fd, True)
    raw_net = files.read_metadata("host-netns.txt", 0)
    host_net = raw_net.decode("ascii") if raw_net is not None else ""
    if re.fullmatch(r"net:\[[0-9]+\]", host_net) is None or str(Path("/proc/self/ns/net").readlink()) != host_net:
        refuse("foreign_host_network_scope")
    name = root.name + ("-compositor" if domain == "compositor" else "")
    group = Path("/sys/fs/cgroup") / name
    info = group.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        refuse("foreign_process_scope")
    with (group / "cgroup.procs").open("w", encoding="ascii") as output:
        output.write(str(os.getpid()))
    if bounded(Path("/proc/self/cgroup"), 4096) != ("0::/" + name + "\n").encode("ascii"):
        refuse("process_scope_not_installed")
    if domain == "app":
        if not hasattr(os, "setns"):
            refuse("network_scope_unavailable")
        descriptor = os.open("/run/netns/" + root.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        try:
            os.setns(descriptor, 0)
        finally:
            os.close(descriptor)
        if str(Path("/proc/self/ns/net").readlink()) == host_net:
            refuse("network_scope_not_installed")
    # No PAM/login manager, inherited user groups, host environment or root
    # command from the app-writable fixture root. Session itself runs unprivileged.
    files.current()
    files.close()  # No privileged directory witness survives unprivileged exec.
    os.setgroups([])
    os.setgid(gid)
    os.setuid(uid)
    if os.geteuid() != uid or os.getegid() != gid or os.getgroups():
        refuse("fixture_uid_drop_failed")
    os.umask(0o077)
    os.chdir(root)
    arguments = ["/bin/bash", "/proc/self/fd/" + str(session_fd)]
    if diagnostic_fd is not None:
        arguments.append(str(diagnostic_fd))
    os.execve("/bin/bash", arguments, environment(root, identity.pw_name, domain, mode, host_net))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # No raw exception, argv, path, environment or process output.
        print("benchmark-launch-failed", file=sys.stderr)
        raise SystemExit(1)
