#!/usr/bin/python3
"""Actual PID-owned Studio UI → native portal → native model HTTP acceptance.

No JS/invoke injection, fake captures, credentials, screenshots or GUI tree dumps.
Failure of real WebKit accessibility or an ambiguous control is a hard failure.
"""
import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import sys
import time
import uuid

from provider import Receiver, Refusal


def isolated_root():
    for key, expected in (("ORBITPAL_WAYLAND_ACCEPTANCE", "synthetic-desktop-only"),
                          ("ORBITPAL_ACCEPTANCE_MODE", "app"), ("GITHUB_ACTIONS", "true"),
                          ("RUNNER_ENVIRONMENT", "github-hosted"), ("RUNNER_OS", "Linux")):
        if os.environ.get(key) != expected:
            raise Refusal("not_disposable_app_job")
    root = Path(os.environ.get("ORBITPAL_ACCEPTANCE_ROOT", ""))
    if not root.is_absolute() or root.parent != Path("/tmp") or not re.fullmatch(r"orbitpal-wayland\.[A-Za-z0-9]{8}", root.name):
        raise Refusal("invalid_private_root")
    mode = root.lstat()
    if not stat.S_ISDIR(mode.st_mode) or mode.st_uid != os.getuid() or mode.st_mode & 0o077:
        raise Refusal("invalid_private_owner")
    private_bus = "unix:path=" + str(root / "runtime/session-bus")
    if not re.fullmatch(re.escape(private_bus) + r"(?:,guid=[0-9a-f]{32})?", os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")):
        raise Refusal("foreign_session_bus")
    if os.environ.get("WAYLAND_DISPLAY") != "orbitpal-acceptance" or os.environ.get("DISPLAY"):
        raise Refusal("foreign_display")
    if not (root / "control/ready").is_file():
        raise Refusal("private_desktop_not_ready")
    return root


def process_parent(pid):
    if not isinstance(pid, int) or not 1 <= pid <= 2**31 - 1:
        raise Refusal("invalid_app_pid")
    path = Path("/proc") / str(pid)
    if path.stat().st_uid != os.getuid():
        raise Refusal("foreign_process")
    data = (path / "status").read_bytes()
    if len(data) > 16384:
        raise Refusal("process_metadata_limit")
    found = re.search(rb"^PPid:\s*([0-9]+)$", data, re.M)
    if found is None:
        raise Refusal("invalid_process_metadata")
    return int(found.group(1))


def owned_process(pid, app_pid):
    try:
        for _ in range(8):
            if pid == app_pid:
                process_parent(pid)  # Recheck UID/liveness, not just an old PID.
                return True
            pid = process_parent(pid)
    except (OSError, Refusal):
        pass
    return False


def walk(root, limit=1200):
    pending, seen = [root], 0
    while pending and seen < limit:
        node = pending.pop(0)
        seen += 1
        yield node
        remaining = limit - seen - len(pending)
        if remaining <= 0:
            continue
        try:
            iterator = iter(node)  # Never ask stale AT-SPI for a length hint.
            for _ in range(remaining):
                try:
                    pending.append(next(iterator))
                except StopIteration:
                    break
        except (RuntimeError, TypeError, ValueError, NotImplementedError):
            pass


class Studio:
    def __init__(self, root):
        try:
            import pyatspi
        except Exception:
            raise Refusal("app_accessibility_unavailable") from None
        self.a = pyatspi
        self.root = root
        raw = (root / "control/app-pid").read_text().strip()
        if not re.fullmatch(r"[0-9]{1,10}", raw):
            raise Refusal("invalid_app_pid")
        self.pid = int(raw)
        process_parent(self.pid)
        if (Path("/proc") / raw / "exe").resolve() != root / "acceptance-app":
            raise Refusal("unexpected_app_executable")
        self.sequence = 0
        self.database = root / "data/studio.streamer-ai.cohost/cohost.sqlite3"
        self.frame = None

    def nodes(self):
        desktop = self.a.Registry.getDesktop(0)
        verified = {}  # One traversal only; no stale PID ownership cache.
        for node in walk(desktop):
            try:
                pid = node.get_process_id()
                if pid not in verified:
                    verified[pid] = owned_process(pid, self.pid)
                if not verified[pid]:
                    continue
                state = node.getState()
                if state.contains(self.a.STATE_SHOWING) and state.contains(self.a.STATE_VISIBLE):
                    yield node
            except (RuntimeError, AttributeError):
                continue

    def find(self, name, roles):
        matches = []
        # The actual implicit label wraps both input and Load models button.
        # WebKit can include that button in the input's computed accessible name.
        names = {"Model", "Model Load models"} if name == "Model" else {name}
        for node in self.nodes():
            try:
                if node.name in names and node.getRole() in roles:
                    matches.append(node)
            except (RuntimeError, AttributeError):
                continue
        if len(matches) > 1:
            raise Refusal("ambiguous_app_control")
        return matches[0] if matches else None

    def wait(self, predicate, code, seconds=12):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            process_parent(self.pid)
            result = predicate()
            if result:
                return result
            time.sleep(0.1)
        raise Refusal(code)

    def one(self, name, roles):
        return self.wait(lambda: self.find(name, roles), "app_control_missing")

    def enabled(self, node):
        state = node.getState()
        return state.contains(self.a.STATE_SENSITIVE) and state.contains(self.a.STATE_ENABLED)

    def action(self, node):
        if not self.enabled(node):
            raise Refusal("app_control_disabled")
        try:
            node.queryComponent().scrollTo(self.a.SCROLL_ANYWHERE)
        except (RuntimeError, AttributeError, NotImplementedError):
            pass
        interface = node.queryAction()
        for index in range(min(interface.nActions, 8)):
            if interface.getName(index).lower() in ("click", "press", "toggle", "select"):
                if not interface.doAction(index):
                    raise Refusal("app_action_refused")
                return
        raise Refusal("app_action_unavailable")

    def button(self, name):
        self.action(self.one(name, {self.a.ROLE_PUSH_BUTTON}))

    def entry(self, name, value):
        node = self.one(name, {self.a.ROLE_ENTRY, self.a.ROLE_TEXT})
        if not self.enabled(node) or not node.getState().contains(self.a.STATE_EDITABLE):
            raise Refusal("app_entry_not_editable")
        node.queryComponent().grabFocus()
        if not node.queryEditableText().setTextContents(value):
            raise Refusal("app_entry_write_failed")

    def switch(self, name, enabled):
        roles = {self.a.ROLE_CHECK_BOX, self.a.ROLE_TOGGLE_BUTTON, getattr(self.a, "ROLE_SWITCH", -1)}
        node = self.one(name, roles)
        if node.getState().contains(self.a.STATE_CHECKED) != enabled:
            self.action(node)
        self.wait(lambda: self.one(name, roles).getState().contains(self.a.STATE_CHECKED) == enabled, "app_switch_not_changed")

    def compatible(self):
        node = self.one("Integration", {self.a.ROLE_COMBO_BOX})
        self.action(node)
        choices = {self.a.ROLE_MENU_ITEM, self.a.ROLE_LIST_ITEM, self.a.ROLE_RADIO_MENU_ITEM}
        self.action(self.one("Compatible API · custom endpoint", choices))

    def text_visible(self, text):
        if not isinstance(text, str) or not 0 < len(text) <= 512:
            return False
        roles = {getattr(self.a, name, -1) for name in ("ROLE_STATIC", "ROLE_TEXT", "ROLE_PARAGRAPH", "ROLE_LABEL")}
        roles.discard(-1)
        for node in self.nodes():
            try:
                if node.name == text:
                    return True
                if node.getRole() in roles:
                    interface = node.queryText()
                    count = interface.characterCount
                    if type(count) is int and count == len(text) and 0 < count <= 512:
                        value = interface.getText(0, count)
                        if isinstance(value, str) and len(value) == count and value == text:
                            return True
            except (RuntimeError, AttributeError, NotImplementedError):
                pass
        return False

    def profile(self):
        if not self.database.is_file() or self.database.is_symlink():
            raise Refusal("app_data_unavailable")
        with sqlite3.connect("file:" + str(self.database) + "?mode=ro", uri=True, timeout=2) as db:
            selected = db.execute("SELECT value FROM settings WHERE key='profile' AND length(CAST(value AS BLOB))<=65536").fetchone()
            if selected is None or json.loads(selected[0]).get("id") != "nova":
                raise Refusal("unexpected_selected_bot")
            # Scoped UI saves update bot_profiles, not the legacy selection
            # pointer's serialized copy. Read the same authoritative row as Store.
            row = db.execute("SELECT profile FROM bot_profiles WHERE id='nova' AND length(CAST(profile AS BLOB))<=65536").fetchone()
        if row is None:
            raise Refusal("app_profile_unavailable")
        return json.loads(row[0])

    def assistant_count(self, text=None):
        with sqlite3.connect("file:" + str(self.database) + "?mode=ro", uri=True, timeout=2) as db:
            if text is None:
                return db.execute("SELECT count(*) FROM messages WHERE role='assistant'").fetchone()[0]
            return db.execute("SELECT count(*) FROM messages WHERE role='assistant' AND text=?", (text,)).fetchone()[0]

    def save(self, predicate):
        node = self.one("Save", {self.a.ROLE_PUSH_BUTTON})
        if self.enabled(node):
            self.action(node)
        self.wait(lambda: predicate(self.profile()), "app_save_not_committed")

    def command(self, action):
        if action not in ("red", "blue", "allow"):
            raise Refusal("invalid_desktop_action")
        self.sequence += 1
        temporary = self.root / "control/app-command.tmp"
        temporary.write_text(json.dumps({"sequence": self.sequence, "action": action}))
        temporary.replace(self.root / "control/command.json")
        return self.sequence

    def acknowledged(self, sequence):
        def done():
            path = self.root / "control/ack.json"
            if not path.is_file() or path.stat().st_size > 2048:
                return False
            value = json.loads(path.read_text())
            if value.get("sequence") != sequence:
                return False
            if value.get("ok") is not True:
                raise Refusal("private_chooser_or_pattern_failed")
            return True
        self.wait(done, "private_desktop_action_timeout", 35)

    def clear_marker(self):
        frames = []
        for node in self.nodes():
            try:
                if node.getRole() in (self.a.ROLE_FRAME, self.a.ROLE_WINDOW) and node.name == "OrbitPal":
                    frames.append(node)
            except (RuntimeError, AttributeError):
                pass
        if len(frames) != 1:
            raise Refusal("ambiguous_app_window")
        if not frames[0].getState().contains(self.a.STATE_ACTIVE):
            raise Refusal("app_window_not_active")
        rect = frames[0].queryComponent().getExtents(self.a.DESKTOP_COORDS)
        if not (rect.width >= 960 and rect.height >= 700 and rect.x >= 0 and rect.y >= 0
                and rect.x + rect.width <= 1920 and rect.y + rect.height <= 1200):
            raise Refusal("app_window_outside_private_monitor")
        x, y = 1920 * 0.97, 1200 * 0.5
        if rect.x - 8 <= x <= rect.x + rect.width + 8 and rect.y - 8 <= y <= rect.y + rect.height + 8:
            raise Refusal("synthetic_marker_covered")

    def choose(self):
        self.button("Models")
        previous = self.profile()["screenContext"]["displayId"]
        sequence = self.command("allow")
        self.button("Select / reconnect Wayland source")
        self.acknowledged(sequence)
        self.wait(lambda: self.text_visible("Wayland source connected; frames only for model requests"), "real_session_not_ready", 15)
        self.save(lambda value: value["screenContext"]["enabled"] is True
                  and value["screenContext"]["displayId"] != previous
                  and re.fullmatch(r"wayland:[0-9a-f-]{36}", value["screenContext"]["displayId"]))

    def presented(self, nonce, case):
        text = "ORBITPAL_REPLY_" + case + "_" + nonce
        self.wait(lambda: self.assistant_count(text) == 1, "reply_not_presented")
        self.wait(lambda: self.text_visible(text), "actual_reply_not_visible")

    def not_presented(self, nonce, case):
        text = "ORBITPAL_REPLY_" + case + "_" + nonce
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            if self.assistant_count(text) or self.text_visible(text):
                raise Refusal("cancelled_response_presented")
            time.sleep(0.1)

    def send(self, receiver, nonce, case, color, held=False):
        receiver.state.arm(case, color, held)
        self.acknowledged(self.command(color))
        self.button("Control room")
        self.wait(lambda: self.enabled(self.one("Message to your cohost", {self.a.ROLE_ENTRY, self.a.ROLE_TEXT})), "previous_presentation_not_finished")
        self.entry("Message to your cohost", f"ORBITPAL_WAYLAND_{case}_{nonce}")
        self.clear_marker()
        self.button("Send message")
        metadata = receiver.state.wait_received(case)
        if not held:
            self.presented(nonce, case)
        return metadata


def run(root):
    studio = Studio(root)
    studio.wait(lambda: studio.find("Control room", {studio.a.ROLE_PUSH_BUTTON}), "actual_studio_cold_start_failed", 40)
    initial = studio.profile()
    if initial.get("screenContext", {}).get("enabled") is not False or studio.assistant_count() != 0:
        raise Refusal("unexpected_saved_app_state")
    checks = ["actual_pid_owned_studio_coldstart"]
    print("app-phase: actual_studio_coldstart", flush=True)
    nonce = uuid.uuid4().hex[:16]
    with Receiver(nonce) as receiver:
        print("app-phase: configure_real_voice_off", flush=True)
        studio.button("Character & voice")
        studio.switch("Enable speech output", False)
        studio.save(lambda value: value["voice"]["enabled"] is False)
        studio.button("Models")
        print("app-phase: configure_real_model_and_grants", flush=True)
        studio.compatible()
        studio.entry("API base URL", receiver.base_url)
        studio.entry("Model", "orbitpal-wayland-fixture")
        studio.switch("Enable tool calls", False)
        studio.switch("Send images to the model", True)
        studio.switch("Include the screen with every request", True)
        studio.save(lambda value: value["model"]["provider"] == "compatible"
                    and value["model"]["baseUrl"] == receiver.base_url
                    and value["model"]["model"] == "orbitpal-wayland-fixture"
                    and value["model"]["apiKeyId"] == ""
                    and value["model"]["toolsEnabled"] is False
                    and value["model"]["visionEnabled"] is True
                    and value["screenContext"]["enabled"] is True)
        if receiver.state.records:
            raise Refusal("inference_before_explicit_send")
        studio.choose()
        print("app-phase: actual_portal_ready", flush=True)
        checks.append("real_ui_grant_and_portal_selection")
        red = studio.send(receiver, nonce, "fresh_red", "red")
        print("app-phase: fresh_red_presented", flush=True)
        blue = studio.send(receiver, nonce, "fresh_blue", "blue", True)
        # Positive control for the SAME normal held SSE contribution/config.
        # Releasing without an operator mutation MUST visibly commit.
        receiver.state.release("fresh_blue")
        receiver.state.wait_finished("fresh_blue")
        studio.presented(nonce, "fresh_blue")
        print("app-phase: fresh_blue_presented", flush=True)
        if red["digest"] == blue["digest"] or len(receiver.state.records) != 2:
            raise Refusal("stale_or_repeated_screen")
        checks.append("two_fresh_real_jpegs_through_agent_provider")
        checks.append("held_positive_reply_commits_history_and_subtitle")
        studio.send(receiver, nonce, "held_stop", "red", True)
        print("app-phase: held_stop_received", flush=True)
        studio.button("Stop")
        studio.wait(lambda: studio.enabled(studio.one("Message to your cohost", {studio.a.ROLE_ENTRY, studio.a.ROLE_TEXT})), "stop_not_observed")
        receiver.state.release("held_stop")
        receiver.state.wait_finished("held_stop")
        studio.not_presented(nonce, "held_stop")
        studio.button("Models")
        studio.wait(lambda: studio.text_visible("No Wayland source connected. Requests with screen context are rejected; the app does not use another screen."), "stop_did_not_close_session")
        checks.append("held_provider_stop_rejects_late_reply")
        print("app-phase: held_stop_cancelled", flush=True)
        studio.choose()  # Explicit real reconnect; Stop never restores a lease.
        studio.send(receiver, nonce, "held_images_off", "red", True)
        print("app-phase: held_images_off_received", flush=True)
        studio.button("Models")
        # Unlike the screen switch, this switch does NOT call Disconnect.
        # The real same-bot save must revoke the model/image grant itself.
        studio.switch("Send images to the model", False)
        studio.save(lambda value: value["model"]["visionEnabled"] is False)
        receiver.state.release("held_images_off")
        receiver.state.wait_finished("held_images_off")
        studio.button("Control room")
        studio.wait(lambda: studio.enabled(studio.one("Message to your cohost", {studio.a.ROLE_ENTRY, studio.a.ROLE_TEXT})), "save_revocation_not_observed")
        studio.not_presented(nonce, "held_images_off")
        checks.append("held_provider_saved_image_grant_rejects_late_reply")
        studio.button("Models")
        studio.switch("Send images to the model", True)
        studio.save(lambda value: value["model"]["visionEnabled"] is True)
        studio.choose()  # A saved model grant change also needs a new selection.
        studio.send(receiver, nonce, "held_disable", "blue", True)
        print("app-phase: held_disable_received", flush=True)
        studio.button("Models")
        studio.switch("Include the screen with every request", False)
        studio.save(lambda value: value["screenContext"]["enabled"] is False)
        receiver.state.release("held_disable")
        receiver.state.wait_finished("held_disable")
        studio.button("Control room")
        studio.wait(lambda: studio.enabled(studio.one("Message to your cohost", {studio.a.ROLE_ENTRY, studio.a.ROLE_TEXT})), "disable_not_observed")
        studio.not_presented(nonce, "held_disable")
        if studio.assistant_count() != 2:
            raise Refusal("disabled_response_presented")
        if receiver.state.failure or len(receiver.state.records) != 5:
            raise Refusal("receiver_case_failed")
        checks.append("held_provider_ui_disconnect_and_saved_disable_reject_late_reply")
    result = {"version": 1, "scope": "actual_app_initial_subset_not_full_issue26",
              "checks": checks, "inferenceRequests": 5, "presentedReplies": 2,
              "receiverPersistsFrames": False, "remoteProvidersUsed": False,
              "unproven": ["two_bots", "compositor_revoke", "source_removal", "multiple_outputs",
                           "tool_rounds", "hindsight", "app_frame_persistence_scan", "resource_counts", "app_quit", "autopilot", "hardware_lifecycle"]}
    (root / "app-result.txt").write_text(json.dumps(result, sort_keys=True) + "\n")
    (root / "control/app-passed").write_text("real-app-initial-subset-complete\n")
    print("real-app-initial-subset-complete", flush=True)


if __name__ == "__main__":
    try:
        run(isolated_root())
    except Refusal as error:
        # Codes are authored fixed strings; never dump exceptions/tree/profile.
        print("app-acceptance-failed: " + error.code, file=sys.stderr, flush=True)
        raise SystemExit(1)
    except Exception:
        print("app-acceptance-failed: controller_failed", file=sys.stderr, flush=True)
        raise SystemExit(1)
