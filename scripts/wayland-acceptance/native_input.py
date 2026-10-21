"""Fixed fixture text through genuine input on the disposable private desktop.

This is an explicit harness mode, never an EditableText exception fallback.
It neither requests nor acknowledges a ScreenCast grant. No clipboard is used.
"""
import os
from pathlib import Path
import re
import select
import stat
import subprocess
import threading
import time

from provider import CASES, Receiver, Refusal, State


MODEL = "orbitpal-wayland-fixture"
MESSAGE = "Message to your cohost"


def remaining(deadline):
    value = deadline - time.monotonic()
    if value <= 0:
        raise Refusal("private_input_deadline")
    return value


def private_environment(root):
    for key, value in (("ORBITPAL_WAYLAND_ACCEPTANCE", "synthetic-desktop-only"),
                       ("ORBITPAL_ACCEPTANCE_MODE", "app"), ("GITHUB_ACTIONS", "true"),
                       ("RUNNER_ENVIRONMENT", "github-hosted"), ("RUNNER_OS", "Linux")):
        if os.environ.get(key) != value:
            raise Refusal("private_input_scope")
    if (os.getuid() == 0 or os.getuid() != os.geteuid() or not root.is_absolute()
            or root.parent != Path("/tmp") or root.resolve(strict=True) != root
            or not re.fullmatch(r"orbitpal-wayland\.[A-Za-z0-9]{8}", root.name)):
        raise Refusal("private_input_scope")
    stamps = []
    for path in (root, root / "runtime"):
        metadata = path.lstat()
        if (not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid()
                or stat.S_IMODE(metadata.st_mode) != 0o700):
            raise Refusal("private_input_scope")
        stamps.append((metadata.st_dev, metadata.st_ino))
    bus = "unix:path=" + str(root / "runtime/session-bus")
    if (os.environ.get("ORBITPAL_ACCEPTANCE_ROOT") != str(root)
            or not re.fullmatch(re.escape(bus) + r"(?:,guid=[0-9a-f]{32})?", os.environ.get("DBUS_SESSION_BUS_ADDRESS", ""))
            or os.environ.get("XDG_RUNTIME_DIR") != str(root / "runtime")
            or os.environ.get("WAYLAND_DISPLAY") != "orbitpal-acceptance"
            or os.environ.get("DISPLAY") or os.environ.get("WAYLAND_SOCKET")):
        raise Refusal("private_input_scope")
    for name in ("session-bus", "orbitpal-acceptance"):
        metadata = (root / "runtime" / name).lstat()
        if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid():
            raise Refusal("private_input_scope")
    return tuple(stamps)


def compositor_identity():
    desktop = os.environ.get("ORBITPAL_ACCEPTANCE_DESKTOP")
    if desktop not in ("gnome", "kde"):
        raise Refusal("private_input_owner")
    raw = os.environ.get("ORBITPAL_ACCEPTANCE_COMPOSITOR_PID", "")
    if not re.fullmatch(r"[1-9][0-9]{0,9}", raw) or int(raw) >= 2**31:
        raise Refusal("private_input_owner")
    process = Path("/proc") / raw
    if process.stat().st_uid != os.getuid():
        raise Refusal("private_input_owner")
    with (process / "stat").open("rb") as stream:
        value = stream.read(4097)
    if len(value) > 4096 or not value.startswith(raw.encode("ascii") + b" (") or b") " not in value:
        raise Refusal("private_input_owner")
    fields = value[value.rfind(b")") + 2:].split()
    if len(fields) < 20 or fields[0] in (b"Z", b"X") or not fields[19].isdigit():
        raise Refusal("private_input_owner")
    wanted = Path("/usr/bin/gnome-shell" if desktop == "gnome" else "/usr/bin/kwin_wayland").resolve(strict=True)
    executable = (process / "exe").resolve(strict=True)
    metadata = wanted.stat()
    if (executable != wanted or not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0
            or metadata.st_mode & 0o022):
        raise Refusal("private_input_owner")
    return desktop, int(raw), fields[19], metadata.st_dev, metadata.st_ino


def key_sequence(value):
    if type(value) is not str or not 1 <= len(value) <= 128 or any(not 33 <= ord(c) <= 126 for c in value):
        raise Refusal("private_input_value")
    return [event for char in value for event in ((ord(char), True), (ord(char), False))]


class FixtureInput:
    def __init__(self, root, receiver, nonce):
        if type(receiver) is not Receiver or type(receiver.state) is not State or type(nonce) is not str or not re.fullmatch(r"[0-9a-f]{16}", nonce):
            raise Refusal("private_input_value")
        self.root, self.receiver, self.state, self.nonce = root, receiver, receiver.state, nonce
        self.server, self.socket, self.thread = receiver.server, receiver.server.socket, receiver.thread
        self.environment, self.compositor = private_environment(root), compositor_identity()
        self.endpoint = receiver.base_url
        if not re.fullmatch(r"http://127\.0\.0\.1:[1-9][0-9]{0,4}/v1", self.endpoint):
            raise Refusal("private_input_value")
        self.port = int(self.endpoint.split(":")[2].split("/")[0])
        if self.port > 65535:
            raise Refusal("private_input_value")
        self.validate("Model", MODEL)

    def validate(self, name, value):
        if (private_environment(self.root) != self.environment or compositor_identity() != self.compositor
                or self.receiver.state is not self.state or self.state.nonce != self.nonce
                or self.receiver.server is not self.server or self.receiver.thread is not self.thread
                or self.server.socket is not self.socket or self.receiver.base_url != self.endpoint
                or not self.thread.is_alive() or self.socket.getsockname() != ("127.0.0.1", self.port)):
            raise Refusal("private_input_scope")
        wanted = ({"API base URL": (self.endpoint,), "Model": (MODEL,),
                   MESSAGE: tuple(f"ORBITPAL_WAYLAND_{case}_{self.nonce}" for case in CASES)}).get(name, ())
        if type(value) is not str or value not in wanted:
            raise Refusal("private_input_value")
        with self.state.condition:
            if self.state.closed or self.state.failure is not None:
                raise Refusal("private_input_scope")
            if name == MESSAGE and (self.state.plan is None
                    or value != f"ORBITPAL_WAYLAND_{self.state.plan['case']}_{self.nonce}"):
                raise Refusal("private_input_value")
        key_sequence(value)  # Bounds apply before opening either input transport.

    def open(self, deadline):
        return GnomeInput(self, deadline) if self.compositor[0] == "gnome" else KdeInput(self, deadline)

    def write(self, studio, name, value, node, locate, native, deadline, stats):
        native(lambda: self.validate(name, value))
        frame = native(lambda: studio.studio_frame(active=False))

        def same(left, right):
            return native(lambda: left is right or left == right) is True

        def read(waiting=False):
            native(lambda: self.validate(name, value))
            current_frame = native(studio.studio_frame)
            fresh = locate()
            if not same(frame, current_frame) or fresh is None or not same(node, fresh):
                raise Refusal("app_control_missing")
            state = native(fresh.getState)
            if (not native(lambda: studio.enabled(fresh))
                    or not native(lambda: state.contains(studio.a.STATE_EDITABLE))):
                raise Refusal("private_input_focus")
            if not native(lambda: state.contains(studio.a.STATE_FOCUSED)):
                if waiting:
                    return None
                raise Refusal("private_input_focus")
            text = native(fresh.queryText)
            if not same(initial_text, text):
                raise Refusal("private_input_readback")
            count = native(lambda: text.characterCount)
            if type(count) is not int or not 0 <= count <= 256:
                raise Refusal("private_input_readback")
            content = native(lambda: text.getText(0, count))
            if type(content) is not str or len(content) != count:
                raise Refusal("private_input_readback")
            return content, text, count

        def selected(text, count):
            number = native(text.getNSelections)
            if type(number) is not int or number != (1 if count else 0):
                raise Refusal("private_input_selection")
            if count:
                # pyatspi 2.58.1 Text.getSelection calls rangeToList(), which
                # returns the two offsets, not a raw GI TextRange object.
                bounds = native(lambda: text.getSelection(0))
                if (type(bounds) is not tuple or len(bounds) != 2
                        or any(type(offset) is not int for offset in bounds)
                        or bounds != (0, count)):
                    raise Refusal("private_input_selection")

        # Read-only Text must actually exist; do not type into an unobservable field.
        stats["nativeOperation"] = 3
        initial_text = native(node.queryText)
        stats["nativeOperation"] = 4
        if not native(lambda: node.getState().contains(studio.a.STATE_FOCUSED)):
            component = native(node.queryComponent)
            if native(component.grabFocus) is not True:
                raise Refusal("private_input_focus")
        # Focus request return is not success. Require fresh actual Main ACTIVE
        # and the same owned editor FOCUSED before preparing and sending keys.
        stats["nativeOperation"] = 5
        before = studio.wait(lambda: read(waiting=True), "private_input_focus",
                             seconds=remaining(deadline), retry_inactive=True)
        stats["nativeOperation"] = 6
        with self.open(deadline) as transport:
            content, text, count = read()  # Fresh after transport preparation.
            if content != before[0] or count != before[2] or not same(text, before[1]):
                raise Refusal("private_input_selection")
            stats["nativeOperation"] = 7
            if count:
                number = native(text.getNSelections)
                if type(number) is not int or number not in (0, 1):
                    raise Refusal("private_input_selection")
                # Select on the real Text interface, not with a version-specific
                # Ctrl+A keysym chord. At most one selection mutation; no retry.
                if native(lambda: text.addSelection(0, count) if number == 0
                          else text.setSelection(0, 0, count)) is not True:
                    raise Refusal("private_input_selection")
            fresh_content, fresh_text, fresh_count = read()
            if fresh_content != content or fresh_count != count or not same(text, fresh_text):
                raise Refusal("private_input_selection")
            selected(fresh_text, fresh_count)
            # A native getter is not an atomic UI transaction. Rejoin the exact
            # scope and unchanged value after the selection read, then require
            # the same full selection immediately before the bounded key burst.
            fresh_content, fresh_text, fresh_count = read()
            if fresh_content != content or fresh_count != count or not same(text, fresh_text):
                raise Refusal("private_input_selection")
            selected(fresh_text, fresh_count)
            stats["writeAttempts"] = 1
            transport.replace(value)
            stats["nativeOperation"] = 8
            read()  # Ownership/focus loss never permits another input attempt.
            studio.wait(lambda: read()[0] == value, "private_input_readback", seconds=remaining(deadline))
        native(lambda: None)  # Cleanup cannot turn a late result into acceptance.


class KdeInput:
    def __init__(self, policy, deadline):
        self.policy, self.deadline, self.process = policy, deadline, None

    def __enter__(self):
        helper = self.policy.root / "pointer"
        metadata = helper.lstat()
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
                or stat.S_IMODE(metadata.st_mode) != 0o700):
            raise Refusal("private_input_scope")
        self.process = subprocess.Popen([str(helper), "--fixture-text"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            if not select.select([self.process.stdout], [], [], remaining(self.deadline))[0] or self.process.stdout.readline(16) != b"ready\n":
                raise Refusal("private_input_unavailable")
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def replace(self, value):
        key_sequence(value)
        output, _ = self.process.communicate(value.encode("ascii") + b"\n", timeout=remaining(self.deadline))
        if self.process.returncode != 0 or output != b"done\n":
            raise Refusal("private_input_unavailable")
        remaining(self.deadline)

    def __exit__(self, *_):
        if self.process is not None:
            try:
                if self.process.poll() is None:
                    self.process.kill()  # Only our retained helper, not compositor/app.
                self.process.wait(timeout=max(0, self.deadline - time.monotonic()))
            finally:
                for stream in (self.process.stdin, self.process.stdout):
                    if stream is not None:
                        stream.close()


class GnomeInput:
    NAME = "org.gnome.Mutter.RemoteDesktop"
    PATH = "/org/gnome/Mutter/RemoteDesktop"
    SESSION = NAME + ".Session"

    def __init__(self, policy, deadline):
        self.policy, self.deadline = policy, deadline
        self.connection, self.path, self.timer = None, None, None
        self.held = []

    def call(self, destination, path, interface, method, signature, arguments, reply):
        result = self.connection.call_sync(destination, path, interface, method,
                    self.GLib.Variant(signature, arguments) if signature else None,
                    self.GLib.VariantType.new(reply), self.Gio.DBusCallFlags.NO_AUTO_START,
                    max(1, int(remaining(self.deadline) * 1000)), self.cancel)
        remaining(self.deadline)
        if result.get_type_string() != reply:
            raise Refusal("private_input_owner")
        return result

    def bus(self, method, arguments, reply):
        return self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                         method, "(s)", arguments, reply).unpack()[0]

    def owner(self):
        owner = self.bus("GetNameOwner", (self.NAME,), "(s)")
        if type(owner) is not str or not re.fullmatch(r":[0-9]+\.[0-9]+", owner):
            raise Refusal("private_input_owner")
        pid = self.bus("GetConnectionUnixProcessID", (owner,), "(u)")
        uid = self.bus("GetConnectionUnixUser", (owner,), "(u)")
        if (type(pid) is not int or pid != self.policy.compositor[1]
                or type(uid) is not int or uid != os.getuid()
                or compositor_identity() != self.policy.compositor):
            raise Refusal("private_input_owner")
        return owner

    def __enter__(self):
        from gi.repository import Gio, GLib
        self.Gio, self.GLib, self.cancel = Gio, GLib, Gio.Cancellable()
        self.timer = threading.Timer(remaining(self.deadline), self.cancel.cancel)
        self.timer.daemon = True
        self.timer.start()
        try:
            # Dedicated connection: session ownership and disconnect cleanup are
            # ours. Never borrow Gio's shared session connection from the app.
            self.connection = Gio.DBusConnection.new_for_address_sync(os.environ["DBUS_SESSION_BUS_ADDRESS"],
                Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
                None, self.cancel)
            self.peer = self.owner()
            result = self.call(self.peer, self.PATH, "org.freedesktop.DBus.Properties", "Get", "(ss)",
                               (self.NAME, "SupportedDeviceTypes"), "(v)").get_child_value(0).get_variant()
            if result.get_type_string() != "u" or result.get_uint32() & 1 != 1:
                raise Refusal("private_input_unavailable")
            self.path = self.call(self.peer, self.PATH, self.NAME, "CreateSession", None, (), "(o)").unpack()[0]
            if (type(self.path) is not str or not self.path.startswith(self.PATH + "/Session/")
                    or len(self.path) > 256):
                raise Refusal("private_input_owner")
            self.call(self.peer, self.path, self.SESSION, "Start", None, (), "()")
            if self.owner() != self.peer:
                raise Refusal("private_input_owner")
            return self
        except BaseException as error:
            self.__exit__(type(error), error, error.__traceback__)
            raise

    def key(self, symbol, pressed):
        if pressed:
            self.held.append(symbol)  # Uncertain call completion still needs release.
        self.call(self.peer, self.path, self.SESSION, "NotifyKeyboardKeysym", "(ub)", (symbol, pressed), "()")
        if not pressed:
            self.held.remove(symbol)

    def replace(self, value):
        if self.owner() != self.peer:
            raise Refusal("private_input_owner")
        try:
            for symbol, pressed in key_sequence(value):
                self.key(symbol, pressed)
        finally:
            self.release()
        if self.owner() != self.peer:
            raise Refusal("private_input_owner")

    def release(self):
        failed = False
        for symbol in tuple(reversed(self.held)):
            try:
                self.key(symbol, False)
            except Exception:
                failed = True
        if failed:
            raise Refusal("private_input_unavailable")

    def __exit__(self, kind, *_):
        failed = False
        try:
            if self.connection is not None and self.path is not None:
                try:
                    self.release()
                except Exception:
                    failed = True
                try:
                    self.call(self.peer, self.path, self.SESSION, "Stop", None, (), "()")
                except Exception:
                    failed = True
        finally:
            if self.connection is not None:
                closed = False
                try:
                    remaining(self.deadline)
                    closed = self.connection.close_sync(self.cancel) is True
                    remaining(self.deadline)
                    failed = failed or not closed
                except Exception:
                    failed = True
                finally:
                    if not closed:
                        # Own-connection disconnect still happens if the original
                        # deadline/cancellable disallows waiting for close. This
                        # cleanup request never sends more input or extends it.
                        self.connection.close(None, None, None)
            if self.timer is not None:
                self.timer.cancel()
        if failed and kind is None:
            raise Refusal("private_input_unavailable")
