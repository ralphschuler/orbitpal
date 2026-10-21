#!/usr/bin/python3
"""Synthetic Wayland surface and bounded, real AT-SPI consent actions.

Never fabricates portal responses, enables restore permissions or saves pixels.
The runner provisions a fresh account/private bus before this process exists.
"""
import faulthandler
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time
import uuid

import gi
gi.require_foreign("cairo")
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk
import pyatspi

ROOT = Path(os.environ["ORBITPAL_ACCEPTANCE_ROOT"])
CONTROL = ROOT / "control"
DESKTOP = os.environ["ORBITPAL_ACCEPTANCE_DESKTOP"]
KDE6 = os.environ.get("KDE_SESSION_VERSION") == "6"
APP_MODE = os.environ.get("ORBITPAL_ACCEPTANCE_MODE") == "app"
TITLES = {"gnome": "Share Screen", "kde": "Screen Sharing"}


def chooser_title(name):
    if DESKTOP == "kde" and KDE6:
        return str(name).startswith(("Share screen with ", "Choose which screen to share with "))
    return name == TITLES[DESKTOP]


class PhaseTrace:
    """Bounded metadata only; never issue an accessibility RPC for logging."""
    def __init__(self):
        self.active = False
        self.seen = set()
        self.sequence = 0
        self.command = ""

    def begin(self, sequence, command):
        faulthandler.cancel_dump_traceback_later()
        self.active = command in ("allow", "cancel")
        self.sequence, self.command = sequence, command
        self.seen.clear()
        if self.active:
            # Stack locations only: no locals, pixels or object representations.
            faulthandler.dump_traceback_later(20, repeat=False, file=sys.stderr)
            self.log("command.received")

    def log(self, phase, **metadata):
        if not self.active or len(self.seen) >= 48:
            return
        fields = {key: str(value)[:120] for key, value in metadata.items()}
        key = (phase, tuple(fields.items()))
        if key in self.seen:
            return
        self.seen.add(key)
        print(json.dumps({"trace": self.sequence, "command": self.command, "phase": phase, **fields}), flush=True)

    def finish(self):
        self.log("command.acknowledged")
        faulthandler.cancel_dump_traceback_later()
        self.active = False


TRACE = PhaseTrace()


def atomic_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value), encoding="utf-8")
    temporary.replace(path)


def children(node):
    try:
        # A stale AT-SPI node may expose a negative __len__ during teardown.
        # Do not ask list() for that length hint or allocate an unbounded tree.
        iterator, result = iter(node), []
        for _ in range(1200):
            try:
                result.append(next(iterator))
            except StopIteration:
                break
        return result
    except (RuntimeError, TypeError, ValueError, NotImplementedError):
        return []


def walk(root, limit=1200):
    pending = [root]
    seen = 0
    while pending and seen < limit:
        node = pending.pop(0)
        seen += 1
        yield node
        remaining = limit - seen - len(pending)
        if remaining > 0:
            pending.extend(children(node)[:remaining])


def visible(node):
    try:
        states = node.getState()
        return states.contains(pyatspi.STATE_SHOWING) and states.contains(pyatspi.STATE_VISIBLE)
    except (RuntimeError, AttributeError):
        return False


def sensitive(node):
    try:
        states = node.getState()
        return states.contains(pyatspi.STATE_SENSITIVE) and (DESKTOP == "gnome" or states.contains(pyatspi.STATE_ENABLED))
    except (RuntimeError, AttributeError):
        return False


def action(node):
    """Invoke only a real accessible press/click/toggle action, never a service API."""
    try:
        TRACE.log("action.query.begin")
        actions = node.queryAction()
        count = actions.nActions
        TRACE.log("action.query.end", count=count)
        for index in range(count):
            name = actions.getName(index).lower()
            if name in ("click", "press", "toggle"):
                TRACE.log("action.invoke.begin", action=name)
                result = bool(actions.doAction(index))
                TRACE.log("action.invoke.end", action=name, result=result)
                return result
    except (RuntimeError, AttributeError, NotImplementedError):
        pass
    return False


def chooser():
    # A private AT-SPI bus contains only this generated desktop and its portal.
    # AdwWindow is a FRAME, not necessarily a DIALOG; match the upstream title.
    # App mode also contains Studio: a matching content/title cannot authorize
    # consent. Require the exact backend PID from this private session for both
    # desktops; an unexpected GNOME chooser owner fails instead of broadening.
    TRACE.log("chooser.registry.begin")
    desktop = pyatspi.Registry.getDesktop(0)
    TRACE.log("chooser.registry.end")
    TRACE.log("chooser.applications.begin")
    applications = children(desktop)
    TRACE.log("chooser.applications.end", count=len(applications))
    for index, application in enumerate(applications):
        TRACE.log("chooser.application.walk.begin", index=index)
        for node in walk(application):
            try:
                name = node.name
                if node is application:
                    TRACE.log("chooser.application.name", index=index, name=name)
                if chooser_title(name) and visible(node):
                    if (KDE6 or APP_MODE) and (node.getRole() not in (pyatspi.ROLE_FRAME, pyatspi.ROLE_DIALOG)
                                              or node.get_process_id() != int(os.environ["ORBITPAL_ACCEPTANCE_PORTAL_PID"])):
                        continue
                    TRACE.log("chooser.dialog.found", name=name)
                    return node
            except (RuntimeError, AttributeError):
                continue
        TRACE.log("chooser.application.walk.end", index=index)
    return None


def kde6_monitor(dialog):
    """Single-choice KDE6 cards are buttons; clicking the real output grants.

    Match its real compositor name, excluding Share virtual screen/Share region.
    The separate native assertion still requires a granted monitor and frames.
    """
    source = (CONTROL / "output-name.txt").read_text(encoding="utf-8")
    match = re.fullmatch(r"\('([A-Za-z0-9_-]{1,80})',\)\n?", source)
    if not match:
        raise RuntimeError("Private compositor did not name exactly one output")
    name = match.group(1)
    candidates = []
    for node in walk(dialog):
        try:
            label = str(node.name or "").strip()
            if node.getRole() == pyatspi.ROLE_PUSH_BUTTON and visible(node) and sensitive(node) \
                    and (label == name or label.endswith("(" + name + ")")):
                candidates.append(node)
        except (RuntimeError, AttributeError):
            continue
    return candidates[0] if len(candidates) == 1 else None


def button(dialog, name):
    TRACE.log("button.scan.begin", name=name)
    for node in walk(dialog):
        try:
            role = node.getRole()
            if role == pyatspi.ROLE_PUSH_BUTTON and node.name.replace("_", "") == name and visible(node):
                TRACE.log("button.scan.found", name=name, role=role)
                return node
        except (RuntimeError, AttributeError):
            continue
    TRACE.log("button.scan.end", name=name, found=False)
    return None


def select_single_monitor(dialog, attempt=None):
    TRACE.log("monitor.scan.begin")
    # Do not confuse KDE's always-visible restore-permission checkbox with an
    # output card. These roles are defined by the reviewed packaged choosers.
    roles = (pyatspi.ROLE_LIST_ITEM,) if DESKTOP == "kde" else (pyatspi.ROLE_TOGGLE_BUTTON,)
    candidates = []
    for node in walk(dialog):
        try:
            role = node.getRole()
            if role not in roles or not visible(node) or not sensitive(node):
                continue
            labels = " ".join(str(item.name or "") for item in walk(node, 40)).strip()
            # These are different resources, not our sole existing virtual monitor.
            if (not labels and DESKTOP != "kde") or any(word in labels.lower() for word in ("new virtual", "full workspace", "entire screen", "window", "remember", "restor")):
                continue
            candidates.append(node)
            TRACE.log("monitor.candidate", name=labels, role=role)
        except (RuntimeError, AttributeError):
            continue
    # Ambiguity fails instead of selecting a different resource by accident.
    TRACE.log("monitor.scan.end", candidates=len(candidates))
    if len(candidates) != 1:
        return False, False
    candidate = candidates[0]
    TRACE.log("monitor.state.begin")
    states = candidate.getState()
    selected = states.contains(pyatspi.STATE_PRESSED) or states.contains(pyatspi.STATE_CHECKED) or states.contains(pyatspi.STATE_SELECTED)
    TRACE.log("monitor.state.end", selected=selected)
    if selected:
        return True, False  # Never toggle an already selected monitor off.
    if attempt == "accessible":
        action(candidate)
        return False, True  # Accepted action is not an observed selection.
    if attempt == "pointer":
        pointer_click(candidate, dialog)
        return False, True
    return False, False



def translated_pointer(target, window, snapshot, nonce, pid, now_ms):
    """Fail closed unless fresh compositor metadata matches this Qt window."""
    def rect(value):
        if not isinstance(value, dict) or not all(key in value for key in ("x", "y", "width", "height")):
            raise RuntimeError("Missing synthetic window geometry")
        result = tuple(value[key] for key in ("x", "y", "width", "height"))
        if any(isinstance(item, bool) or not isinstance(item, (int, float)) or not math.isfinite(item) for item in result):
            raise RuntimeError("Invalid synthetic window geometry")
        if result[2] <= 0 or result[3] <= 0:
            raise RuntimeError("Empty synthetic window geometry")
        return result
    if not isinstance(snapshot, dict) or type(pid) is not int or not 0 < pid < 2**31:
        raise RuntimeError("Invalid synthetic chooser process")
    timestamp = snapshot.get("timestamp")
    if snapshot.get("nonce") != nonce or isinstance(timestamp, bool) or not isinstance(timestamp, (int, float)) or not math.isfinite(timestamp) or not 0 <= now_ms - timestamp <= 5000:
        raise RuntimeError("Stale synthetic compositor geometry")
    matches = snapshot.get("matches", [])
    if snapshot.get("count") != 1 or not isinstance(matches, list) or len(matches) != 1 or not isinstance(matches[0], dict) or matches[0].get("pid") != pid:
        raise RuntimeError("Synthetic chooser geometry is ambiguous")
    bx, by, bw, bh = rect(matches[0]["buffer"])
    rx, ry, rw, rh = rect(window)
    tx, ty, tw, th = rect(target)
    if (bw, bh) != (rw, rh) or tx < rx or ty < ry or tx + tw > rx + rw or ty + th > ry + rh:
        raise RuntimeError("Qt and compositor window bounds do not match")
    if bx < 0 or by < 0 or bx + bw > 1280 or by + bh > 960:
        raise RuntimeError("Synthetic chooser is outside the private output")
    x, y = bx + tx - rx + tw / 2, by + ty - ry + th / 2
    if not (0 <= x < 1280 and 0 <= y < 960):
        raise RuntimeError("Synthetic pointer destination is outside the output")
    return x, y


def kwin_geometry(pid):
    """Read-only KWin snapshot; never move a window or alter a portal grant."""
    runtime = ROOT / "runtime"
    bus = f"unix:path={runtime}/session-bus"
    address = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
    if DESKTOP != "kde" or os.geteuid() == 0 or ROOT.resolve() != ROOT or not ROOT.name.startswith("orbitpal-wayland."):
        raise RuntimeError("Refusing compositor metadata outside the private case")
    for directory in (ROOT, runtime):
        metadata = directory.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.geteuid() or stat.S_IMODE(metadata.st_mode) != 0o700:
            raise RuntimeError("Compositor metadata requires private owned directories")
    if os.environ.get("XDG_RUNTIME_DIR") != str(runtime) or not re.fullmatch(re.escape(bus) + r"(?:,guid=[0-9a-f]{32})?", address):
        raise RuntimeError("Compositor metadata requires the private session bus")
    if type(pid) is not int or not 0 < pid < 2**31:
        raise RuntimeError("Invalid real chooser process ID")
    nonce = uuid.uuid4().hex
    plugin = "orbitpal-geometry-" + nonce
    script = CONTROL / (plugin + ".js")
    prefix = "ORBITPAL_KWIN_GEOMETRY:" + nonce + ":"
    # KWin6 renamed clientList() to windowList() and moved script object paths.
    code = """(function () {
        var windows = workspace.WINDOW_LIST(), matches = [], titles = [];
        for (var i = 0; i < windows.length && i < 64; i++) {
            var title = String(windows[i].caption);
            if (titles.length < 8) titles.push(title.substring(0, 120));
            if (windows[i].pid === PID) {
                var b = windows[i].bufferGeometry;
                matches.push({pid: windows[i].pid, active: windows[i].active, caption: title.substring(0, 120), buffer: {x:b.x, y:b.y, width:b.width, height:b.height}});
            }
        }
        console.warn(PREFIX + JSON.stringify({nonce: NONCE, timestamp: Date.now(), count: matches.length, matches: matches.slice(0, 2), titles: titles}));
    })();""".replace("WINDOW_LIST", "windowList" if KDE6 else "clientList").replace("PREFIX", json.dumps(prefix)).replace("NONCE", json.dumps(nonce)).replace("PID", str(pid))
    deadline = time.monotonic() + 3
    loaded = False
    def call(path, method, *arguments, cleanup=False):
        remaining = 1 if cleanup else min(1, deadline - time.monotonic())
        if remaining <= 0:
            raise RuntimeError("Synthetic compositor metadata deadline exceeded")
        return subprocess.run(["gdbus", "call", "--session", "--dest", "org.kde.KWin", "--object-path", path,
                               "--method", method, *arguments], capture_output=True, text=True, timeout=remaining, check=True).stdout.strip()
    try:
        with script.open("x", encoding="utf-8") as stream:
            stream.write(code)
        result = call("/Scripting", "org.kde.kwin.Scripting.loadScript", str(script), plugin)
        match = re.fullmatch(r"\((\d+),\)", result)
        if not match:
            raise RuntimeError("KWin did not load the read-only metadata script")
        loaded = True
        call(("/Scripting/Script" if KDE6 else "/") + match.group(1), "org.kde.kwin.Script.run")
        log = ROOT / "compositor.txt"
        metadata = log.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.geteuid():
            raise RuntimeError("Invalid private compositor metadata log")
        with log.open("rb") as stream:
            stream.seek(max(0, metadata.st_size - 65536))
            lines = stream.read(65536).decode("utf-8", errors="replace").splitlines()
        replies = [line.partition(prefix)[2] for line in lines if prefix in line and len(line) <= 4096]
        if len(replies) != 1:
            raise RuntimeError("Missing or ambiguous fresh compositor metadata")
        snapshot = json.loads(replies[0])
        TRACE.log("pointer.compositor.geometry", snapshot=json.dumps(snapshot)[:120])
        return snapshot, nonce
    finally:
        try:
            if loaded:
                call("/Scripting", "org.kde.kwin.Scripting.unloadScript", plugin, cleanup=True)
        finally:
            script.unlink(missing_ok=True)


def pointer_click(node, dialog):
    # Qt5's accessibility bridge can accept an action without changing the UI.
    # Only use this fallback after an observed no-op, at fresh real UI bounds.
    # It is genuine input to the private compositor, never a portal response.
    helper = ROOT / "pointer"
    if DESKTOP != "kde" or not helper.is_file() or helper.is_symlink():
        return False
    try:
        TRACE.log("pointer.geometry.begin")
        bounds = node.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        window = dialog.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        if not chooser_title(dialog.name) or not visible(dialog):
            return False
        pid = dialog.get_process_id()
        snapshot, nonce = kwin_geometry(pid)
        fields = lambda rect: {key: getattr(rect, key) for key in ("x", "y", "width", "height")}
        TRACE.log("pointer.geometry.observed", qtRoot=str(fields(window)), target=str(fields(bounds)))
        x, y = translated_pointer(fields(bounds), fields(window), snapshot, nonce, pid, time.time() * 1000)
        TRACE.log("pointer.geometry.end", x=x, y=y, qtRoot=str(fields(window)), target=str(fields(bounds)))
        TRACE.log("pointer.invoke.begin")
        completed = subprocess.run([str(helper), str(x), str(y)], capture_output=True, timeout=6, check=False)
        TRACE.log("pointer.invoke.end", code=completed.returncode)
        if completed.returncode != 0:
            if APP_MODE:
                print("KDE pointer helper rejected input", flush=True)
            else:
                print("KDE pointer helper: " + completed.stderr.decode("utf-8", errors="replace")[:1000], flush=True)
            raise RuntimeError("Private compositor pointer input was rejected")
        print("Genuine KDE chooser pointer click", flush=True)
        return True
    except (RuntimeError, AttributeError, KeyError, ValueError, OSError, NotImplementedError, subprocess.TimeoutExpired, subprocess.CalledProcessError) as error:
        TRACE.log("pointer.rejected", reason="input_unavailable" if APP_MODE else f"{type(error).__name__}: {error}")
        return False


def pointer_escape(dialog):
    """KDE6's single-choice dialog has no Cancel button; real Escape rejects it."""
    helper = ROOT / "pointer"
    if DESKTOP != "kde" or not KDE6 or not chooser_title(dialog.name) or not visible(dialog) \
            or not helper.is_file() or helper.is_symlink():
        return False
    pid = dialog.get_process_id()
    snapshot, nonce = kwin_geometry(pid)
    timestamp = snapshot.get("timestamp")
    if snapshot.get("nonce") != nonce or isinstance(timestamp, bool) or not isinstance(timestamp, (int, float)) \
            or not math.isfinite(timestamp) or not 0 <= time.time() * 1000 - timestamp <= 5000:
        raise RuntimeError("Cancel requires fresh private chooser metadata")
    count, matches = snapshot.get("count"), snapshot.get("matches")
    if isinstance(count, bool) or not isinstance(count, int) or not isinstance(matches, list):
        raise RuntimeError("Cancel requires valid private chooser metadata")
    # AT-SPI can expose the dialog before KWin has mapped/activated it. Wait
    # within the existing action deadline; do not send input or acknowledge it.
    if count == 0 and matches == []:
        return False
    if count != 1 or len(matches) != 1 or matches[0].get("pid") != pid \
            or not isinstance(matches[0].get("active"), bool):
        raise RuntimeError("Cancel requires the real private chooser to have focus")
    if not matches[0]["active"]:
        return False
    completed = subprocess.run([str(helper), "--escape"], capture_output=True, timeout=6, check=False)
    if completed.returncode != 0:
        raise RuntimeError("Private compositor rejected genuine Escape input")
    return True


def text_tree():
    rows = []
    for app in children(pyatspi.Registry.getDesktop(0)):
        if APP_MODE and app.get_process_id() != int(os.environ["ORBITPAL_ACCEPTANCE_PORTAL_PID"]):
            continue  # Never dump the real Studio's editable fields.
        for node in walk(app, 160):
            try:
                if visible(node):
                    actions = []
                    try:
                        interface = node.queryAction()
                        actions = [interface.getName(index) for index in range(min(8, interface.nActions))]
                    except (RuntimeError, AttributeError, NotImplementedError):
                        pass
                    rows.append(f"{node.getRoleName()}: {str(node.name or '')[:180]} sensitive={sensitive(node)} actions={actions}")
            except (RuntimeError, AttributeError):
                continue
    return "\n".join(rows[:240])


class Desktop:
    def __init__(self):
        self.sequence = 0
        self.pending = None
        self.deadline = 0
        self.selection_attempted = False
        self.selection_pointer_attempted = False
        self.consent_attempted = False
        self.consent_pointer_attempted = False
        self.color = (0.82, 0.12, 0.24)
        self.window = Gtk.Window(title="OrbitPal synthetic acceptance pattern")
        self.window.set_decorated(False)
        self.window.fullscreen()
        self.surface = Gtk.DrawingArea()
        self.surface.connect("draw", self.draw)
        self.window.add(self.surface)
        self.window.show_all()
        self.window.present()
        self.window.connect("destroy", Gtk.main_quit)
        GLib.timeout_add(100, self.poll)
        GLib.timeout_add(1000, self.ready)

    def ready(self):
        (CONTROL / "ready").write_text("synthetic desktop ready\n", encoding="utf-8")
        return False

    def draw(self, _widget, context):
        context.set_source_rgb(*self.color)
        context.paint()
        if APP_MODE and self.pending in ("red", "blue"):
            # App mode keeps this real surface behind the actual Studio. ACK
            # follows the actual GTK draw; the received JPEG is the final oracle.
            GLib.idle_add(self.draw_ack, self.sequence, self.pending)

    def draw_ack(self, sequence, color):
        if self.sequence == sequence and self.pending == color:
            self.ack()
        return False

    def ack(self, error=None):
        TRACE.log("ack.begin", error=bool(error))
        atomic_json(CONTROL / "ack.json", {"sequence": self.sequence, "ok": error is None, "error": error or ""})
        self.pending = None
        TRACE.finish()
        return False

    def poll(self):
        try:
            path = CONTROL / "command.json"
            if path.exists():
                command = json.loads(path.read_text(encoding="utf-8"))
                if command["sequence"] != self.sequence:
                    self.sequence = command["sequence"]
                    self.pending = command["action"]
                    self.deadline = time.monotonic() + 25
                    self.selection_attempted = False
                    self.selection_pointer_attempted = False
                    self.consent_attempted = False
                    self.consent_pointer_attempted = False
                    TRACE.begin(self.sequence, self.pending)
                    if self.pending in ("red", "blue"):
                        self.color = (0.82, 0.12, 0.24) if self.pending == "red" else (0.12, 0.24, 0.82)
                        if not APP_MODE:
                            self.window.present()
                        self.surface.queue_draw()
                        if not APP_MODE:
                            GLib.timeout_add(250, self.ack)
            if self.pending in ("allow", "cancel"):
                TRACE.log("chooser.poll.begin")
                dialog = chooser()
                TRACE.log("chooser.poll.end", found=dialog is not None)
                if dialog is None and self.consent_attempted:
                    # A successful AT-SPI call proves dispatch only. Observe the
                    # real chooser closing before acknowledging its UI action;
                    # the native test independently requires the portal result.
                    TRACE.log("chooser.dismissed")
                    self.ack()
                elif dialog is not None:
                    if DESKTOP == "kde" and KDE6:
                        if self.pending == "cancel":
                            if not self.consent_attempted:
                                self.consent_attempted = pointer_escape(dialog)
                        else:
                            target = kde6_monitor(dialog)
                            if target is not None:
                                if not self.consent_attempted:
                                    self.consent_attempted = True
                                    action(target)
                                elif not self.consent_pointer_attempted:
                                    self.consent_pointer_attempted = True
                                    pointer_click(target, dialog)
                        # Only the later observed dismissal can acknowledge it.
                        # Native select/capture separately verifies the response.
                        if self.pending and time.monotonic() >= self.deadline:
                            (ROOT / "chooser-tree.txt").write_text(text_tree(), encoding="utf-8")
                            self.ack("KDE6 real chooser action timed out")
                        return True
                    ready = self.pending == "cancel"
                    if not ready:
                        attempt = None
                        if not self.selection_attempted:
                            attempt = "accessible"
                        elif DESKTOP == "kde" and not self.selection_pointer_attempted:
                            attempt = "pointer"
                        ready, attempted = select_single_monitor(dialog, attempt)
                        if attempted:
                            if attempt == "accessible":
                                self.selection_attempted = True
                            elif attempt == "pointer":
                                self.selection_pointer_attempted = True
                    if ready:
                        target = button(dialog, "Share" if self.pending == "allow" else "Cancel")
                        enabled = sensitive(target) if target is not None else False
                        TRACE.log("button.sensitive.end", exists=target is not None, enabled=enabled)
                        if target is not None and enabled:
                            if not self.consent_attempted:
                                self.consent_attempted = True
                                action(target)
                            elif DESKTOP == "kde" and not self.consent_pointer_attempted:
                                # The same real chooser is still present on a
                                # subsequent poll: the accepted press did not
                                # dismiss it. One genuine pointer retry only.
                                self.consent_pointer_attempted = True
                                pointer_click(target, dialog)
                if self.pending and time.monotonic() >= self.deadline:
                    TRACE.log("timeout.tree.begin")
                    (ROOT / "chooser-tree.txt").write_text(text_tree(), encoding="utf-8")
                    TRACE.log("timeout.tree.end")
                    self.ack("Real chooser/action unavailable; inspect bounded text-only chooser-tree.txt")
        except Exception as error:
            # This contains only synthetic UI metadata, never screen pixels.
            if APP_MODE:
                print("Desktop controller failed", file=sys.stderr, flush=True)
            else:
                print(f"Desktop controller failed: {type(error).__name__}: {error}", file=sys.stderr, flush=True)
            if self.pending:
                self.ack("Synthetic controller failed; see text diagnostic")
            return False
        return True


if __name__ == "__main__":
    if os.environ.get("ORBITPAL_WAYLAND_ACCEPTANCE") != "synthetic-desktop-only" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise SystemExit("This controller is only for its disposable hosted acceptance job")
    Desktop()
    Gtk.main()
