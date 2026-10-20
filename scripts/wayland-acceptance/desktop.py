#!/usr/bin/python3
"""Synthetic Wayland surface and bounded, real AT-SPI consent actions.

Never fabricates portal responses, enables restore permissions or saves pixels.
The runner provisions a fresh account/private bus before this process exists.
"""
import faulthandler
import json
import os
from pathlib import Path
import subprocess
import sys
import time

import gi
gi.require_foreign("cairo")
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk
import pyatspi

ROOT = Path(os.environ["ORBITPAL_ACCEPTANCE_ROOT"])
CONTROL = ROOT / "control"
DESKTOP = os.environ["ORBITPAL_ACCEPTANCE_DESKTOP"]
TITLES = {"gnome": "Share Screen", "kde": "Screen Sharing"}


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
        return list(node)
    except (RuntimeError, TypeError, NotImplementedError):
        return []


def walk(root, limit=1200):
    pending = [root]
    seen = 0
    while pending and seen < limit:
        node = pending.pop(0)
        seen += 1
        yield node
        pending.extend(children(node))


def visible(node):
    try:
        states = node.getState()
        return states.contains(pyatspi.STATE_SHOWING) and states.contains(pyatspi.STATE_VISIBLE)
    except (RuntimeError, AttributeError):
        return False


def sensitive(node):
    try:
        return node.getState().contains(pyatspi.STATE_SENSITIVE)
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
                if name == TITLES[DESKTOP] and visible(node):
                    TRACE.log("chooser.dialog.found", name=name)
                    return node
            except (RuntimeError, AttributeError):
                continue
        TRACE.log("chooser.application.walk.end", index=index)
    return None


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


def select_single_monitor(dialog):
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
            if not labels or any(word in labels.lower() for word in ("new virtual", "full workspace", "entire screen", "window", "remember", "restor")):
                continue
            candidates.append(node)
            TRACE.log("monitor.candidate", name=labels, role=role)
        except (RuntimeError, AttributeError):
            continue
    # Ambiguity fails instead of selecting a different resource by accident.
    TRACE.log("monitor.scan.end", candidates=len(candidates))
    if len(candidates) != 1:
        return False
    candidate = candidates[0]
    TRACE.log("monitor.state.begin")
    states = candidate.getState()
    selected = states.contains(pyatspi.STATE_PRESSED) or states.contains(pyatspi.STATE_CHECKED) or states.contains(pyatspi.STATE_SELECTED)
    TRACE.log("monitor.state.end", selected=selected)
    if selected:
        return False  # Do not toggle an already selected monitor off.
    if action(candidate):
        return True
    # Qt5 ItemDelegate can expose only a focus action. Send a real pointer
    # click through the private test compositor, using freshly observed bounds.
    # This does not synthesize a ScreenCast response or bypass its chooser.
    helper = ROOT / "pointer"
    if DESKTOP != "kde" or not helper.is_file() or helper.is_symlink():
        return False
    try:
        TRACE.log("pointer.geometry.begin")
        bounds = candidate.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        x, y = bounds.x + bounds.width / 2, bounds.y + bounds.height / 2
        TRACE.log("pointer.geometry.end", x=x, y=y, width=bounds.width, height=bounds.height)
        if bounds.width <= 0 or bounds.height <= 0 or not (0 <= x < 1280 and 0 <= y < 720):
            return False
        TRACE.log("pointer.invoke.begin")
        completed = subprocess.run([str(helper), str(x), str(y)], capture_output=True, timeout=6, check=False)
        TRACE.log("pointer.invoke.end", code=completed.returncode)
        if completed.returncode != 0:
            print("KDE pointer helper: " + completed.stderr.decode("utf-8", errors="replace")[:1000], flush=True)
            raise RuntimeError("Private compositor pointer input was rejected")
        print("Genuine KDE chooser card pointer click", flush=True)
        return True
    except (RuntimeError, AttributeError, NotImplementedError, subprocess.TimeoutExpired):
        return False


def text_tree():
    rows = []
    for app in children(pyatspi.Registry.getDesktop(0)):
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
                    TRACE.begin(self.sequence, self.pending)
                    if self.pending in ("red", "blue"):
                        self.color = (0.82, 0.12, 0.24) if self.pending == "red" else (0.12, 0.24, 0.82)
                        self.window.present()
                        self.surface.queue_draw()
                        GLib.timeout_add(250, self.ack)
            if self.pending in ("allow", "cancel"):
                TRACE.log("chooser.poll.begin")
                dialog = chooser()
                TRACE.log("chooser.poll.end", found=dialog is not None)
                if dialog is not None:
                    target = button(dialog, "Share" if self.pending == "allow" else "Cancel")
                    # Preserve short-circuit evaluation and its existing RPCs.
                    enabled = sensitive(target) if target is not None else False
                    TRACE.log("button.sensitive.end", exists=target is not None, enabled=enabled)
                    if target is not None and enabled and action(target):
                        print(f"Genuine {DESKTOP} chooser action: {self.pending}", flush=True)
                        self.ack()
                    elif self.pending == "allow" and not self.selection_attempted:
                        self.selection_attempted = select_single_monitor(dialog)
                if self.pending and time.monotonic() >= self.deadline:
                    TRACE.log("timeout.tree.begin")
                    (ROOT / "chooser-tree.txt").write_text(text_tree(), encoding="utf-8")
                    TRACE.log("timeout.tree.end")
                    self.ack("Real chooser/action unavailable; inspect bounded text-only chooser-tree.txt")
        except Exception as error:
            # This contains only synthetic UI metadata, never screen pixels.
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
