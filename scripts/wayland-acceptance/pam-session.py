#!/usr/bin/python3
"""Fixed public PAM capability and owned-session teardown observations only.

This helper never creates/stops a unit, changes PAM/login1 state, signals a
process or removes an account/runtime directory. run.sh owns those operations.
"""
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import stat
import subprocess
import sys
import threading
import time


PAM_FILES = ("login", "common-auth", "common-account", "common-session", "common-password")
PACKAGES = ("systemd", "libpam-systemd", "login", "util-linux")
SELINUX_CONTROL = "[success=ok ignore=ignore module_unknown=ignore default=bad]"
MODULE_DIRECTORY = Path("/usr/lib/x86_64-linux-gnu/security")
SESSION_ARGUMENTS = {
    "pam_systemd.so": {()}, "pam_unix.so": {()}, "pam_loginuid.so": {()},
    "pam_limits.so": {()}, "pam_umask.so": {()},
    "pam_keyinit.so": {("force", "revoke"), ("revoke",)},
    "pam_env.so": {(), ("readenv=1",), ("readenv=1", "envfile=/etc/default/locale")},
    "pam_motd.so": {(), ("motd=/run/motd.dynamic",), ("noupdate",)},
    "pam_mail.so": {("standard",), ("standard", "noenv")},
    "pam_lastlog.so": {(), ("showfailed",)},
    "pam_lastlog2.so": {(), ("showfailed",)},
}
NON_SESSION_MODULES = {"pam_unix.so", "pam_deny.so", "pam_permit.so", "pam_cap.so",
    "pam_faildelay.so", "pam_nologin.so", "pam_securetty.so", "pam_group.so",
    "pam_localuser.so", "pam_access.so", "pam_pwquality.so", "pam_pwhistory.so"}


class Refused(Exception):
    pass


class StackRefused(Refused):
    """Closed stock-parser metadata; never retain the actual line/arguments."""
    def __init__(self, file, rule, module=None, control=None, arguments=False):
        self.file = PAM_FILES.index(file) + 1
        self.rule = rule  # 1 malformed non-session rule, 2 unknown module.
        self.module = {"pam_gnome_keyring.so": 1, "pam_exec.so": 2,
                       "pam_kwallet5.so": 3, "pam_kwallet6.so": 4}.get(module, 0)
        self.control = {"required": 1, "requisite": 2, "sufficient": 3,
                        "optional": 4}.get(control, 5 if control and control.startswith("[") else 0)
        self.arguments = int(arguments)
        super().__init__()  # Drop BaseException's original raw constructor args.


class Absent(Exception):
    def __init__(self, kind):
        self.kind = kind  # 1 exact NoSuchUnit, 2 exact NoSuchUser; never text.


def require_job():
    expected = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux"}
    if any(os.environ.get(key) != value for key, value in expected.items()) or sys.platform != "linux":
        raise Refused()
    if any(key in os.environ for key in ("DBUS_SYSTEM_BUS_ADDRESS", "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE")):
        raise Refused()


def public_bytes(path, maximum):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != 0 or before.st_mode & 0o022 or not 0 < before.st_size <= maximum:
            raise Refused()
        data = os.read(fd, maximum + 1)
        after = os.fstat(fd)
        if len(data) != before.st_size or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise Refused()
        return data
    finally:
        os.close(fd)


def session_stack(read):
    """Recognize a bounded stock login include-chain, not arbitrary PAM logic."""
    hashes, modules, expanded, visiting = {}, set(), [], set()
    def visit(name, depth=0):
        if name not in PAM_FILES or name in visiting or depth > 4 or len(hashes) >= 5 and name not in hashes:
            raise Refused()
        visiting.add(name)
        raw = read(name)
        if type(raw) is not bytes or len(raw) > 16384:
            raise Refused()
        hashes[name] = hashlib.sha256(raw).hexdigest()
        lines = raw.decode("utf-8", "strict").splitlines()
        if len(lines) > 256:
            raise Refused()
        for line in lines:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            include = re.fullmatch(r"@include\s+([a-z-]+)", line)
            if include:
                visit(include[1], depth + 1)
                continue
            if line.startswith("@") or not re.match(r"^-?(?:auth|account|password|session)\s", line):
                raise Refused()
            if not re.match(r"^-?session\s", line):
                # Account rules do run. Unknown external/exec modules or custom
                # directives must not pass merely because session rules look safe.
                other = re.fullmatch(r"-?(auth|account|password)\s+(\[[^\]]{1,128}\]|required|requisite|sufficient|optional)\s+(pam_[a-z0-9_]+\.so)(?:\s+([^#]{1,256}))?", line)
                if not other:
                    raise StackRefused(name, 1)
                # Observed GNOME stock password-only rule. systemd v259.9's
                # PAMName path never calls pam_chauthtok; it still executes
                # account/session and auth credential hooks. Do not permit
                # this module in those stacks, another file, or with options.
                if not line.startswith("-") and (name, other[1], other[2], other[3], other[4]) == (
                        "common-password", "password", "optional", "pam_gnome_keyring.so", None):
                    continue
                if other[3] not in NON_SESSION_MODULES:
                    raise StackRefused(name, 2, other[3], other[2], bool(other[4]))
                continue
            match = re.fullmatch(r"-?session\s+(\[[^\]]{1,128}\]|[a-z]+)\s+(pam_[a-z0-9_]+\.so)(?:\s+([^#]{1,256}))?", line)
            if not match:
                raise Refused()
            expanded.append((match[1], match[2], tuple((match[3] or "").split())))
        visiting.remove(name)
    visit("login")
    if "common-session" not in hashes or len(expanded) > 64:
        raise Refused()
    index, systemd_count, selinux_phase = 0, 0, 0
    preamble = [("[default=1]", "pam_permit.so", ()), ("requisite", "pam_deny.so", ()), ("required", "pam_permit.so", ())]
    while index < len(expanded):
        control, module, arguments = expanded[index]
        # Ubuntu's packaged login stack closes a previous SELinux context as
        # its first session rule, then opens the fresh one before user-context
        # modules. This exact pair is not a general bracket/jump interpreter.
        if module == "pam_selinux.so":
            if control != SELINUX_CONTROL:
                raise Refused()
            if arguments == ("close",) and selinux_phase == 0 and index == 0:
                selinux_phase = 1
            elif arguments == ("open",) and selinux_phase == 1:
                selinux_phase = 2
            else:
                raise Refused()
            modules.add(module)
            index += 1
            continue
        if selinux_phase == 1 and module not in {"pam_loginuid.so", "pam_motd.so"}:
            raise Refused()
        if expanded[index:index + 3] == preamble:
            modules.update(("pam_permit.so", "pam_deny.so"))
            index += 3
            continue
        if control not in ("required", "optional", "requisite") or module not in SESSION_ARGUMENTS or arguments not in SESSION_ARGUMENTS[module]:
            raise Refused()
        if module == "pam_systemd.so":
            if control not in ("required", "optional"):
                raise Refused()
            systemd_count += 1
        modules.add(module)
        index += 1
    if systemd_count != 1 or selinux_phase == 1:
        raise Refused()
    return hashes, modules


def version_fact(raw):
    if type(raw) is not bytes or len(raw) > 128:
        raise Refused()
    value = raw.decode("ascii", "strict").strip()
    match = re.fullmatch(r"(?:[0-9]{1,3}:)?([0-9]{1,6})\.([0-9]{1,6})(?:\.([0-9]{1,6}))?(?:[-+~][A-Za-z0-9.+:~_-]{1,64})?", value)
    if not match:
        raise Refused()
    return [int(match[1]), int(match[2]), int(match[3] or 0)], hashlib.sha256(raw).hexdigest()


def capability(read=None, version=None, module=None):
    read = read or (lambda name: public_bytes(Path("/etc/pam.d") / name, 16384))
    module = module or (lambda name: public_bytes(MODULE_DIRECTORY / name, 16 * 1024 * 1024))
    def installed(name):
        result = subprocess.run(["/usr/bin/dpkg-query", "-W", "-f=${Version}", name], stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=1, check=False)
        if result.returncode != 0:
            raise Refused()
        return result.stdout
    version = version or installed
    cached, versions, file_mask, version_mask, code = {}, [], 0, 0, 0
    for index, name in enumerate(PAM_FILES):
        try:
            raw = read(name)
            if type(raw) is not bytes or not 0 < len(raw) <= 16384:
                raise Refused()
            cached[name] = raw
            file_mask |= 1 << index
        except Exception:
            code |= 1
    for index, name in enumerate(PACKAGES):
        try:
            versions.append(version_fact(version(name)))
            version_mask |= 1 << index
        except Exception:
            versions.append(([0, 0, 0], "0" * 64))
            code |= 2
    modules, systemd_hash = set(), "0" * 64
    stack = {"stackFile": 0, "stackRule": 0, "stackModule": 0, "stackControl": 0, "stackArguments": 0}
    try:
        _, modules = session_stack(lambda name: cached[name])
    except Exception as error:
        code |= 4
        if type(error) is StackRefused:
            stack.update(stackFile=error.file, stackRule=error.rule, stackModule=error.module,
                         stackControl=error.control, stackArguments=error.arguments)
    try:
        systemd_hash = hashlib.sha256(module("pam_systemd.so")).hexdigest()
        for name in modules - {"pam_systemd.so"}:
            module(name)  # Require each recognized stock module to be installed.
    except Exception:
        code |= 8
    return {"version": 1, "supported": int(code == 0), "code": code, **stack,
            "fileKnownMask": file_mask, "versionKnownMask": version_mask,
            "pamSourceSha256": [hashlib.sha256(cached[name]).hexdigest() if name in cached else "0" * 64 for name in PAM_FILES],
            "moduleCount": len(modules), "systemdModuleSha256": systemd_hash,
            "packageVersions": [item[0] for item in versions], "packageVersionSha256": [item[1] for item in versions]}


def context(root, account, raw_uid, unit):
    if os.geteuid() != 0 or not re.fullmatch(r"[1-9][0-9]{2,4}", raw_uid) or not re.fullmatch(r"opw[0-9]{1,16}", account):
        raise Refused()
    uid = int(raw_uid)
    if uid < 1000 or uid == int(os.environ.get("SUDO_UID", "0")):
        raise Refused()
    root = Path(root)
    if root.parent != Path("/tmp") or not re.fullmatch(r"orbitpal-wayland\.[A-Za-z0-9]{8}", root.name) or root.resolve() != root:
        raise Refused()
    info, user = root.lstat(), pwd.getpwnam(account)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != uid or info.st_mode & 0o077 or user.pw_uid != uid or user.pw_dir != str(root / "home") or user.pw_shell != "/bin/bash":
        raise Refused()
    if unit != "orbitpal-" + root.name + "-" + account + ".service":
        raise Refused()
    return root, uid


class Bus:
    def __init__(self, seconds=2):
        from gi.repository import Gio, GLib
        self.Gio, self.GLib = Gio, GLib
        self.deadline = time.monotonic() + seconds
        self.cancel = Gio.Cancellable()
        self.timer = threading.Timer(seconds, self.cancel.cancel)
        self.timer.daemon = True
        self.timer.start()
        try:
            self.bus = Gio.bus_get_sync(Gio.BusType.SYSTEM, self.cancel)
        except Exception:
            self.close()
            raise

    def call(self, owner, path, interface, method, signature, arguments, reply):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise Refused()
        try:
            value = self.bus.call_sync(owner, path, interface, method, self.GLib.Variant(signature, arguments),
                self.GLib.VariantType.new(reply), self.Gio.DBusCallFlags.NO_AUTO_START, max(1, int(remaining * 1000)), self.cancel)
        except self.GLib.Error as error:
            remote = self.Gio.DBusError.get_remote_error(error)
            if remote == "org.freedesktop.systemd1.NoSuchUnit":
                raise Absent(1) from None
            if remote == "org.freedesktop.login1.NoSuchUser":
                raise Absent(2) from None
            raise
        if time.monotonic() >= self.deadline:
            raise Refused()
        return value

    def owner(self, name):
        value = self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", "(s)", (name,), "(s)").unpack()[0]
        if type(value) is not str or not re.fullmatch(r":[0-9]{1,20}\.[0-9]{1,20}", value):
            raise Refused()
        return value

    def property(self, owner, path, interface, name, signature):
        value = self.call(owner, path, "org.freedesktop.DBus.Properties", "Get", "(ss)", (interface, name), "(v)").get_child_value(0).get_variant()
        if value.get_type_string() != signature:
            raise Refused()
        return value.unpack()

    def close(self):
        self.cancel.cancel()
        self.timer.cancel()


def owned_unit(bus, root, account, unit, uid=None):
    name, interface = "org.freedesktop.systemd1", "org.freedesktop.systemd1.Unit"
    owner = bus.owner(name)
    try:
        path = bus.call(owner, "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager", "GetUnit", "(s)", (unit,), "(o)").unpack()[0]
    except Absent as error:
        if error.kind != 1 or bus.owner(name) != owner:
            raise Refused()
        return False
    expected = ((interface, "Id", "s", unit), (interface, "Transient", "b", True),
        (interface, "Description", "s", "OrbitPal private PAM " + root.name),
        ("org.freedesktop.systemd1.Service", "PAMName", "s", "login"),
        ("org.freedesktop.systemd1.Service", "Restart", "s", "no"),
        ("org.freedesktop.systemd1.Service", "WorkingDirectory", "s", str(root)))
    user = bus.property(owner, path, "org.freedesktop.systemd1.Service", "User", "s")
    if type(user) is not str or user not in ({account} if uid is None else {account, str(uid)}):
        raise Refused()
    for kind, prop, signature, value in expected:
        actual = bus.property(owner, path, kind, prop, signature)
        if type(actual) is not type(value) or actual != value:
            raise Refused()
    state = bus.property(owner, path, interface, "ActiveState", "s")
    if state not in ("active", "activating", "deactivating", "inactive", "failed") or bus.owner(name) != owner:
        raise Refused()
    return state


def no_user_scope(bus, uid):
    name = "org.freedesktop.login1"
    owner = bus.owner(name)
    try:
        bus.call(owner, "/org/freedesktop/login1", "org.freedesktop.login1.Manager", "GetUser", "(u)", (uid,), "(o)")
    except Absent as error:
        if error.kind != 2 or bus.owner(name) != owner:
            raise Refused()
        return True
    if bus.owner(name) != owner:
        raise Refused()
    return False  # Any retained fresh-user object is still residue, never clean.


def residue(bus, root, account, uid, unit, processes, runtime, require_unit_absent=False):
    state = owned_unit(bus, root, account, unit, uid)
    unit_clear = state is False or not require_unit_absent and state in ("inactive", "failed")
    user_clear = no_user_scope(bus, uid)
    processes_clear, runtime_clear = not processes(uid), not runtime(uid)
    return {"version": 1, "code": 0, "unitClear": int(unit_clear), "userClear": int(user_clear),
            "processesClear": int(processes_clear), "runtimeClear": int(runtime_clear),
            "clean": int(unit_clear and user_clear and processes_clear and runtime_clear)}


def root_record(root_fd, value):
    data = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")
    if len(data) > 2048:
        raise Refused()
    fd = os.open("pam-session-cleanup.txt", os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600, dir_fd=root_fd)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o077 or info.st_nlink != 1 or info.st_size > 2048:
            raise Refused()
        os.ftruncate(fd, 0)
        if os.write(fd, data) != len(data):
            raise Refused()
    finally:
        os.close(fd)


def main(arguments):
    bus, root_fd, pin_verified = None, None, False
    try:
        require_job()
        if arguments == ["capability"]:
            value = capability()
            print(json.dumps(value, sort_keys=True, separators=(",", ":")))
            return 0 if value["supported"] else 1
        if len(arguments) != 5 or arguments[0] not in ("unit", "fresh", "residue"):
            raise Refused()
        mode, raw_root, account, raw_uid, unit = arguments
        root, uid = context(raw_root, account, raw_uid, unit)
        root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        pin = os.fstat(root_fd)
        named = root.lstat()
        if not stat.S_ISDIR(pin.st_mode) or pin.st_uid != uid or pin.st_mode & 0o077 or (pin.st_dev, pin.st_ino) != (named.st_dev, named.st_ino):
            raise Refused()
        pin_verified = True
        bus = Bus()
        if mode == "unit":
            return 0 if owned_unit(bus, root, account, unit, uid) is not False else 3
        def processes(target):
            result = subprocess.run(["/usr/bin/pgrep", "-u", str(target)], stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1, check=False)
            if result.returncode not in (0, 1):
                raise Refused()
            return result.returncode == 0
        value = residue(bus, root, account, uid, unit, processes,
            lambda target: os.path.lexists("/run/user/" + str(target)), require_unit_absent=mode == "fresh")
        context(raw_root, account, raw_uid, unit)
        named = root.lstat()
        if (pin.st_dev, pin.st_ino) != (named.st_dev, named.st_ino):
            raise Refused()
        if mode == "residue":
            root_record(root_fd, value)
        return 0 if value["clean"] else 2
    except Exception:
        if arguments == ["capability"]:
            print('{"version":1,"supported":0,"code":1}')
        elif arguments and arguments[0] == "residue" and pin_verified:
            try:
                root_record(root_fd, {"version": 1, "code": 1, "unitClear": 0,
                    "userClear": 0, "processesClear": 0, "runtimeClear": 0, "clean": 0})
            except Exception:
                pass
        return 1  # Unknown is never absent, owned or cleaned; no raw diagnostics.
    finally:
        if bus is not None:
            bus.close()
        if root_fd is not None:
            os.close(root_fd)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
