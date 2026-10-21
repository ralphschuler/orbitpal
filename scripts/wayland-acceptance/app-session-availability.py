#!/usr/bin/python3
"""Read-only login1 observations for the supervisor's exact private app child.

No outcome grants native availability or changes actual-app acceptance. Codes:
0 unknown, 1 eligible in both samples, 2 observed not-ready, 3 no session,
4 foreign session user, 5 identity changed, 6 deadline, 7 target exited,
8 private environment rejected. Missing output is not an availability result.
"""
import json
import os
from pathlib import Path
import re
import stat
import sys
import threading
import time


SECONDS = 4.0  # app-session.sh separately caps this helper's entire lifetime at 5s.
MAX_OUTPUT = 2048
OVERRIDES = ("DBUS_SYSTEM_BUS_ADDRESS", "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE")
FIELDS = ("version", "status", "samples", "knownMask", "active", "locked",
          "preparing", "state", "targetVerified", "sessionUserMatches",
          "ownerStable", "sessionStable", "elapsedMs")


class ObservationError(Exception):
    def __init__(self, code=0):
        self.code = code if type(code) is int and 0 <= code <= 8 else 0


class NoSession(Exception):
    pass


def record():
    return dict(zip(FIELDS, (1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)))


def encode_record(value):
    # Never serialize caller strings, exception data, identifiers or extra keys.
    limits = (1, 8, 2, 31, 2, 2, 2, 3, 1, 1, 1, 1, 5000)
    if type(value) is not dict or set(value) != set(FIELDS):
        raise ValueError()
    if any(type(value[key]) is not int or not 0 <= value[key] <= maximum
           for key, maximum in zip(FIELDS, limits)) or value["version"] != 1:
        raise ValueError()
    data = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")
    if len(data) > MAX_OUTPUT:
        raise ValueError()
    return data


def remaining(deadline, clock):
    left = deadline - clock()
    if left <= 0:
        raise ObservationError(6)
    return left


def proc_fields(raw, pid):
    """Only identity fields from one bounded /proc stat record; never emit it."""
    if type(raw) is not bytes or len(raw) > 8192:
        raise ObservationError(5)
    prefix, separator, tail = raw.rpartition(b") ")
    fields = tail.split()
    if not separator or not prefix.startswith(str(pid).encode("ascii") + b" (") or len(fields) < 20:
        raise ObservationError(5)
    if fields[0] in (b"Z", b"X", b"x"):
        raise ObservationError(7)
    if not fields[1].isdigit() or not fields[19].isdigit():
        raise ObservationError(5)
    return int(fields[1]), int(fields[19])


def owned_info(info, *, directory=False, private=True):
    expected = stat.S_ISDIR if directory else stat.S_ISREG
    if not expected(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & (0o077 if private else 0o022):
        raise ObservationError(8)
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_size, info.st_mtime_ns)


def read_at(directory, name, limit, *, private=True):
    fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
    try:
        identity = owned_info(os.fstat(fd), private=private)
        data = os.read(fd, limit + 1)
        if len(data) > limit or identity != owned_info(os.fstat(fd), private=private):
            raise ObservationError(5)
        return identity, data
    finally:
        os.close(fd)


class Target:
    """Pin the private root/PID record and supervisor-child relationship."""
    def __init__(self, supervisor):
        from app import isolated_root
        if type(supervisor) is not int or not 1 <= supervisor <= 2**31 - 1 or os.getuid() == 0:
            raise ObservationError(8)
        if any(key in os.environ for key in OVERRIDES):
            raise ObservationError(8)
        self.root = isolated_root()  # Existing private-display/bus/account guard.
        self.fds = []
        self.root_fd = self.directory(str(self.root))
        self.root_identity = owned_info(os.fstat(self.root_fd), directory=True)
        self.control_fd = self.directory("control", self.root_fd)
        self.harness_fd = self.directory("harness", self.root_fd, private=False)
        self.pid_identity, raw = read_at(self.control_fd, "app-pid", 16)
        if not re.fullmatch(rb"[1-9][0-9]{0,9}\n", raw):
            raise ObservationError(8)
        self.child = int(raw)
        if self.child > 2**31 - 1:
            raise ObservationError(8)
        self.pid_raw = raw
        self.supervisor, self.wrapper = supervisor, os.getppid()
        self.wrapper_command = (b"/usr/bin/timeout\0--signal=KILL\x005s\0/usr/bin/python3\0" +
            os.fsencode(self.root / "harness/app-session-availability.py") + b"\0" + str(supervisor).encode("ascii") + b"\0")
        self.controller = self.process(self.wrapper, Path("/usr/bin/timeout"), self.wrapper_command)[0]
        self.executable = self.root / "acceptance-app"
        self.exe_identity = self.file_identity("acceptance-app", self.root_fd)
        self.script_identity = self.file_identity("app-runtime-diagnostic.py", self.harness_fd, private=False)
        self.initial = None
        self.check()

    def directory(self, name, parent=None, private=True):
        fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
        self.fds.append(fd)
        owned_info(os.fstat(fd), directory=True, private=private)
        return fd

    def file_identity(self, name, parent, private=True):
        return owned_info(os.stat(name, dir_fd=parent, follow_symlinks=False), private=private)

    def process(self, pid, executable, cmdline=None):
        directory = Path("/proc") / str(pid)
        try:
            if directory.stat().st_uid != os.getuid():
                raise ObservationError(5)
            with (directory / "stat").open("rb") as source:
                parent, start = proc_fields(source.read(8193), pid)
            with (directory / "status").open("rb") as source:
                status = source.read(16385)
            uid = re.search(rb"^Uid:\s+([0-9]+)\s+([0-9]+)\s+([0-9]+)\s+([0-9]+)\s*$", status, re.M)
            if len(status) > 16384 or uid is None or any(int(part) != os.getuid() for part in uid.groups()):
                raise ObservationError(5)
            info, expected = (directory / "exe").stat(), executable.stat()
            if (info.st_dev, info.st_ino) != (expected.st_dev, expected.st_ino):
                raise ObservationError(5)
            actual_root, expected_root = (directory / "root").stat(), Path("/").stat()
            if (actual_root.st_dev, actual_root.st_ino) != (expected_root.st_dev, expected_root.st_ino):
                raise ObservationError(5)
            if cmdline is not None:
                with (directory / "cmdline").open("rb") as source:
                    if source.read(4097) != cmdline:
                        raise ObservationError(5)
            return parent, start, info.st_dev, info.st_ino
        except (FileNotFoundError, ProcessLookupError):
            raise ObservationError(7) from None

    def paths(self):
        # Directory size/mtime can change legitimately; their owned inode cannot.
        if (owned_info(self.root.lstat(), directory=True)[:4] != self.root_identity[:4] or
                owned_info(os.stat("control", dir_fd=self.root_fd, follow_symlinks=False), directory=True)[:4] != owned_info(os.fstat(self.control_fd), directory=True)[:4] or
                owned_info(os.stat("harness", dir_fd=self.root_fd, follow_symlinks=False), directory=True, private=False)[:4] != owned_info(os.fstat(self.harness_fd), directory=True, private=False)[:4] or
                self.file_identity("acceptance-app", self.root_fd) != self.exe_identity or
                self.file_identity("app-runtime-diagnostic.py", self.harness_fd, private=False) != self.script_identity or
                read_at(self.control_fd, "app-pid", 16) != (self.pid_identity, self.pid_raw)):
            raise ObservationError(5)

    def check(self):
        self.paths()
        wrapper = self.process(self.wrapper, Path("/usr/bin/timeout"), self.wrapper_command)
        supervisor = self.process(self.supervisor, Path("/usr/bin/python3"),
            b"/usr/bin/python3\0" + os.fsencode(self.root / "harness/app-runtime-diagnostic.py") + b"\0")
        child = self.process(self.child, self.executable)
        controller = self.process(self.controller, Path("/usr/bin/bash"),
            b"bash\0" + os.fsencode(self.root / "harness/app-session.sh") + b"\0")
        identity = (wrapper, supervisor, child, controller)
        if (wrapper[0] != self.controller or supervisor[0] != self.controller or
                child[0] != self.supervisor or os.getppid() != self.wrapper):
            raise ObservationError(5)
        if self.initial is not None and identity != self.initial:
            raise ObservationError(5)
        self.initial = identity

    def write(self, value):
        # A failed/exited subject is still reportable; only the output root must
        # remain pinned here. main() separately performs the final subject check.
        self.paths()
        fd = os.open("app-session-availability.txt", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                     0o600, dir_fd=self.root_fd)
        try:
            data = encode_record(value)
            if os.write(fd, data) != len(data):
                raise ValueError()
        finally:
            os.close(fd)

    def close(self):
        for fd in reversed(self.fds):
            os.close(fd)


class SystemBus:
    def __init__(self, deadline, clock):
        from gi.repository import Gio, GLib
        self.Gio, self.GLib = Gio, GLib
        self.deadline, self.clock = deadline, clock
        self.cancel = Gio.Cancellable()
        self.timer = threading.Timer(remaining(deadline, clock), self.cancel.cancel)
        self.timer.daemon = True
        self.timer.start()
        try:
            # SYSTEM means the real default system bus; overrides were rejected.
            self.bus = Gio.bus_get_sync(Gio.BusType.SYSTEM, self.cancel)
        except Exception:
            self.close()
            raise

    def call(self, destination, path, interface, method, parameters, signature):
        milliseconds = max(1, int(remaining(self.deadline, self.clock) * 1000))
        result = self.bus.call_sync(destination, path, interface, method, parameters,
            self.GLib.VariantType.new(signature), self.Gio.DBusCallFlags.NO_AUTO_START,
            milliseconds, self.cancel)
        remaining(self.deadline, self.clock)
        return result

    def owner(self):
        owner = self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
            "GetNameOwner", self.GLib.Variant("(s)", ("org.freedesktop.login1",)), "(s)").unpack()[0]
        if type(owner) is not str or not re.fullmatch(r":[0-9]{1,20}\.[0-9]{1,20}", owner):
            raise ObservationError()
        return owner

    def session(self, owner, pid):
        try:
            path = self.call(owner, "/org/freedesktop/login1", "org.freedesktop.login1.Manager",
                "GetSessionByPID", self.GLib.Variant("(u)", (pid,)), "(o)").unpack()[0]
        except self.GLib.Error as error:
            if self.Gio.DBusError.get_remote_error(error) == "org.freedesktop.login1.NoSessionForPID":
                raise NoSession() from None
            raise
        if type(path) is not str or not re.fullmatch(r"/org/freedesktop/login1/session/[A-Za-z0-9_]{1,128}", path):
            raise ObservationError()
        return path

    def property(self, owner, path, interface, name, signature):
        result = self.call(owner, path, "org.freedesktop.DBus.Properties", "Get",
            self.GLib.Variant("(ss)", (interface, name)), "(v)")
        value = result.get_child_value(0).get_variant()
        if value.get_type_string() != signature:
            raise ObservationError()
        return value.unpack()

    def close(self):
        self.cancel.cancel()
        self.timer.cancel()


def observe(target, bus, deadline, clock=time.monotonic):
    output, scope, all_ready = record(), None, True
    try:
        for _ in range(2):
            remaining(deadline, clock)
            target.check()
            owner = bus.owner()
            try:
                path = bus.session(owner, target.child)
            except NoSession:
                if bus.owner() != owner:
                    raise ObservationError(5)
                target.check()
                remaining(deadline, clock)
                output = record()
                output.update(status=3, samples=1, targetVerified=1, ownerStable=1)
                return output
            interface = "org.freedesktop.login1.Session"
            user = bus.property(owner, path, interface, "User", "(uo)")
            locked = bus.property(owner, path, interface, "LockedHint", "b")
            active = bus.property(owner, path, interface, "Active", "b")
            state = bus.property(owner, path, interface, "State", "s")
            preparing = bus.property(owner, "/org/freedesktop/login1", "org.freedesktop.login1.Manager", "PreparingForSleep", "b")
            if (type(user) is not tuple or len(user) != 2 or type(user[0]) is not int or
                    not 0 <= user[0] <= 2**32 - 1 or type(user[1]) is not str or
                    not re.fullmatch(r"/org/freedesktop/login1/user/[A-Za-z0-9_]{1,128}", user[1]) or
                    any(type(value) is not bool for value in (locked, active, preparing)) or
                    type(state) is not str or len(state) > 16):
                raise ObservationError()
            if (bus.property(owner, path, interface, "User", "(uo)") != user or
                    bus.session(owner, target.child) != path or bus.owner() != owner):
                raise ObservationError(5)
            target.check()
            remaining(deadline, clock)
            current = (owner, path, user)
            if scope is not None and current != scope:
                raise ObservationError(5)
            scope = current
            output.update(samples=output["samples"] + 1, knownMask=31,
                active=2 if active else 1, locked=2 if locked else 1,
                preparing=2 if preparing else 1, state={"active": 1, "online": 2, "closing": 3}.get(state, 0),
                targetVerified=1, sessionUserMatches=int(user[0] == os.getuid()), ownerStable=1, sessionStable=1)
            if user[0] != os.getuid():
                output["status"] = 4
                return output
            all_ready = all_ready and not locked and active and state == "active" and not preparing
        output["status"] = 1 if all_ready else 2
    except Exception as error:
        # Discard partial/stale availability fields after every incomplete read.
        output = record()
        output["status"] = 6 if clock() >= deadline else (error.code if type(error) is ObservationError else 0)
    return output


def main(arguments):
    started, target, bus = time.monotonic(), None, None
    deadline = started + SECONDS
    try:
        if len(arguments) != 1 or not re.fullmatch(r"[1-9][0-9]{0,9}", arguments[0]):
            return 0
        target = Target(int(arguments[0]))
        bus = SystemBus(deadline, time.monotonic)
        output = observe(target, bus, deadline)
    except Exception as error:
        output = record()
        output["status"] = 6 if time.monotonic() >= deadline else (error.code if type(error) is ObservationError else 0)
    finally:
        if bus is not None:
            bus.close()
    if target is not None:
        try:
            try:
                target.check()
            except Exception as error:
                output = record()
                output["status"] = error.code if type(error) is ObservationError else 0
            output["elapsedMs"] = min(5000, max(0, int((time.monotonic() - started) * 1000)))
            target.write(output)
        except Exception:
            pass  # Missing output is unknown; never print exception or identifiers.
        finally:
            target.close()
    return 0  # An observation is never a #26 pass/fail or native availability grant.


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
