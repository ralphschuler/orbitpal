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
from native_input import FixtureInput, MESSAGE


PHASES = frozenset((
    "controller_entry", "validate_private_environment", "studio_construct",
    "studio_import_accessibility", "studio_read_pid", "studio_verify_process",
    "studio_verify_executable", "studio_wait_cold_start", "studio_registry_query",
    "studio_read_initial_state", "actual_studio_coldstart", "configure_real_voice_off",
    "configure_voice_navigation", "configure_voice_navigation_done", "configure_voice_switch",
    "configure_real_model_and_grants", "configure_model_integration_lookup",
    "configure_model_integration_opened", "configure_model_compatible_lookup",
    "configure_model_compatible_selected", "configure_model_endpoint_entry",
    "configure_model_name_entry", "configure_model_tools_switch",
    "configure_model_images_switch", "configure_model_screen_switch",
    "configure_model_save", "configure_model_saved", "configure_model_source_selection",
    "actual_portal_ready", "fresh_red_presented",
    "fresh_blue_presented", "held_stop_received", "held_stop_cancelled",
    "held_images_off_received", "held_disable_received",
))
SWITCH_CONTROLS = frozenset((
    "Enable speech output", "Enable tool calls", "Send images to the model",
    "Include the screen with every request",
))
MODEL_ENTRIES = frozenset(("API base URL", "Model"))
TRACEBACK_METADATA_LIMIT = 32
OWNED_SOURCES = {
    str(Path(__file__).resolve()): "app.py",
    str(Path(__file__).with_name("provider.py").resolve()): "provider.py",
    str(Path(__file__).with_name("native_input.py").resolve()): "native_input.py",
}
EXCEPTION_TYPES = (
    (Refusal, "Refusal"), (FileNotFoundError, "FileNotFoundError"),
    (ProcessLookupError, "ProcessLookupError"), (PermissionError, "PermissionError"),
    (OSError, "OSError"), (RuntimeError, "RuntimeError"),
    (AttributeError, "AttributeError"), (TypeError, "TypeError"),
    (ValueError, "ValueError"), (NotImplementedError, "NotImplementedError"),
    (TimeoutError, "TimeoutError"), (ImportError, "ImportError"),
    (ModuleNotFoundError, "ModuleNotFoundError"),
    (sqlite3.OperationalError, "SqliteOperationalError"),
    (sqlite3.DatabaseError, "SqliteDatabaseError"), (sqlite3.Error, "SqliteError"),
)
REFUSAL_CODES = frozenset((
    'actual_reply_not_visible', 'actual_studio_cold_start_failed', 'ambiguous_app_control',
    'ambiguous_app_window', 'app_accessibility_unavailable', 'app_action_refused',
    'app_action_unavailable', 'app_control_disabled', 'app_control_missing',
    'app_data_unavailable', 'app_entry_not_editable', 'app_entry_write_failed',
    'app_executable_not_ready', 'app_pid_not_ready', 'app_process_exited',
    'app_profile_unavailable', 'app_save_not_committed', 'app_selection_not_changed', 'app_switch_not_changed',
    'app_window_not_active', 'app_window_outside_private_monitor', 'body_limit',
    'cancelled_response_presented', 'disable_not_observed', 'disabled_response_presented',
    'foreign_display', 'foreign_process', 'foreign_session_bus', 'fresh_marker_mismatch',
    'header_limit', 'held_response_deadline', 'image_limit', 'inference_before_explicit_send',
    'inference_not_received', 'invalid_app_pid', 'invalid_block', 'invalid_desktop_action',
    'invalid_diagnostic_phase', 'invalid_fixture_nonce', 'invalid_fixture_plan', 'invalid_headers',
    'invalid_image_role', 'invalid_json', 'invalid_length', 'invalid_message',
    'invalid_private_owner', 'invalid_private_root', 'invalid_process_metadata',
    'jpeg_decode_failed', 'jpeg_dimensions', 'message_limit', 'not_disposable_app_job',
    'not_production_jpeg', 'previous_presentation_not_finished',
    'private_chooser_or_pattern_failed', 'private_desktop_action_timeout',
    'private_desktop_not_ready', 'process_metadata_limit', 'real_session_not_ready',
    'receiver_case_failed', 'receiver_cleanup_failed', 'receiver_closed', 'receiver_failed',
    'reply_not_presented', 'request_deadline', 'request_incomplete', 'request_scope',
    'response_not_released', 'save_revocation_not_observed', 'stale_or_repeated_screen',
    'stop_did_not_close_session', 'stop_not_observed', 'synthetic_marker_covered',
    'tools_must_be_disabled', 'unexpected_app_executable', 'unexpected_auth_or_transport',
    'unexpected_content_type', 'unexpected_saved_app_state', 'unexpected_selected_bot',
    'unsolicited_inference', 'wrong_model_or_stream', 'wrong_release', 'wrong_turn_or_image_count',
    'private_input_deadline', 'private_input_scope', 'private_input_owner',
    'private_input_value', 'private_input_focus', 'private_input_readback', 'private_input_unavailable',
    'private_input_selection',
))
_current_phase = "controller_entry"
_emitted_phases = set()


def phase(code):
    """Finite authored progress only; never use UI/exception text as a code."""
    global _current_phase
    if code not in PHASES:
        raise Refusal("invalid_diagnostic_phase")
    _current_phase = code
    if code not in _emitted_phases:
        _emitted_phases.add(code)
        print("app-phase: " + code, flush=True)


def failure_diagnostic(error):
    """Read bounded traceback METADATA, never format frames/values or causes."""
    kind = next((name for exact, name in EXCEPTION_TYPES if type(error) is exact), "other")
    # Use the built-in descriptor, not a subclass's custom attribute access.
    trace = BaseException.__traceback__.__get__(error)
    source, line, count = None, None, 0
    while trace is not None and count < TRACEBACK_METADATA_LIMIT:
        candidate = OWNED_SOURCES.get(trace.tb_frame.f_code.co_filename)
        if candidate is not None and 0 < trace.tb_lineno <= 1_000_000:
            source, line = candidate, trace.tb_lineno
        trace = trace.tb_next
        count += 1
    return {"stage": _current_phase if _current_phase in PHASES else "controller_entry",
            "exceptionType": kind, "source": source, "line": line,
            "metadataFrames": count, "metadataTruncated": trace is not None}


def refusal_code(error):
    # Only exact authored code strings survive; no generic exception payload.
    if type(error) is Refusal and type(error.code) is str:
        for code in REFUSAL_CODES:
            if error.code == code:
                return code
    return "controller_refused"


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
    try:
        if path.stat().st_uid != os.getuid():
            raise Refusal("foreign_process")
        data = (path / "status").read_bytes()
    except (FileNotFoundError, ProcessLookupError):
        raise Refusal("app_process_exited") from None
    if len(data) > 16384:
        raise Refusal("process_metadata_limit")
    if re.search(rb"^State:\s*[ZX](?:\s|$)", data, re.M):
        raise Refusal("app_process_exited")
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
        phase("studio_import_accessibility")
        try:
            import pyatspi
        except Exception:
            raise Refusal("app_accessibility_unavailable") from None
        self.a = pyatspi
        self.root = root
        phase("studio_read_pid")
        try:
            raw = (root / "control/app-pid").read_text().strip()
        except FileNotFoundError:
            raise Refusal("app_pid_not_ready") from None
        if not re.fullmatch(r"[0-9]{1,10}", raw):
            raise Refusal("invalid_app_pid")
        self.pid = int(raw)
        phase("studio_verify_process")
        process_parent(self.pid)
        phase("studio_verify_executable")
        try:
            executable = (Path("/proc") / raw / "exe").resolve(strict=True)
        except (FileNotFoundError, ProcessLookupError):
            process_parent(self.pid)  # A terminal exit must not be retried.
            raise Refusal("app_executable_not_ready") from None
        if executable != root / "acceptance-app":
            raise Refusal("unexpected_app_executable")
        self.sequence = 0
        self.database = root / "data/studio.streamer-ai.cohost/cohost.sqlite3"
        self.frame = None
        self.native_input = None

    def bind_native_input(self, receiver, nonce):
        """Explicit private guest mode, bound to this exact live loopback fixture."""
        self.native_input = FixtureInput(self.root, receiver, nonce)

    def nodes(self, include_offscreen=False, frame=None):
        phase("studio_registry_query")
        desktop = frame if frame is not None else self.a.Registry.getDesktop(0)
        verified = {}  # One traversal only; no stale PID ownership cache.
        for node in walk(desktop):
            try:
                pid = node.get_process_id()
                if pid not in verified:
                    verified[pid] = owned_process(pid, self.pid)
                if not verified[pid]:
                    continue
                state = node.getState()
                if state.contains(self.a.STATE_VISIBLE) and (include_offscreen or state.contains(self.a.STATE_SHOWING)):
                    yield node
            except (RuntimeError, AttributeError):
                continue

    def find(self, name, roles, include_offscreen=False, frame=None):
        matches = []
        # The actual implicit label wraps both input and Load models button.
        # WebKit can include that button in the input's computed accessible name.
        names = {"Model", "Model Load models"} if name == "Model" else {name}
        nodes = self.nodes(include_offscreen=include_offscreen, frame=frame) if include_offscreen or frame is not None else self.nodes()
        for node in nodes:
            try:
                if node.name in names and node.getRole() in roles:
                    matches.append(node)
            except (RuntimeError, AttributeError):
                continue
        if len(matches) > 1:
            raise Refusal("ambiguous_app_control")
        return matches[0] if matches else None

    def wait(self, predicate, code, seconds=12, retry_inactive=False):
        deadline = time.monotonic() + seconds
        inactive = False
        while time.monotonic() < deadline:
            process_parent(self.pid)
            try:
                result = predicate()
            except Refusal as error:
                if (not retry_inactive or type(error) is not Refusal
                        or type(error.code) is not str or error.code != "app_window_not_active"):
                    raise
                # Reacquire through the same PID/executable/unique-active-frame
                # gate on the next poll. Never focus, act on or accept this frame.
                inactive = True
            else:
                inactive = False
                if result:
                    return result
            time.sleep(0.1)
        raise Refusal("app_window_not_active" if inactive else code)

    def one(self, name, roles):
        return self.wait(lambda: self.find(name, roles), "app_control_missing")

    def enabled(self, node):
        state = node.getState()
        return state.contains(self.a.STATE_SENSITIVE) and state.contains(self.a.STATE_ENABLED)

    def action(self, node, scroll=True, switch_target=None, switch_current=None):
        target_state = None
        if switch_target is not None:
            # Directional names require an explicit bool target and the SAME
            # fresh native locator, retaining PID/executable/unique-frame and
            # scroll-budget gates. The interface must actually offer the action.
            if type(switch_target) is not bool or scroll or not callable(switch_current):
                raise Refusal("app_action_unavailable")
            source_name = node.name
            roles = {self.a.ROLE_CHECK_BOX, self.a.ROLE_TOGGLE_BUTTON, getattr(self.a, "ROLE_SWITCH", -1)}
            roles.discard(-1)
            if source_name not in SWITCH_CONTROLS:
                raise Refusal("app_action_unavailable")
            node = switch_current()
            if node is None:
                raise Refusal("app_control_missing")
            if node.name != source_name or node.getRole() not in roles:
                raise Refusal("app_action_unavailable")
            if not owned_process(node.get_process_id(), self.pid):
                raise Refusal("foreign_process")
            target_state = node.getState()
            if not (target_state.contains(self.a.STATE_VISIBLE) and target_state.contains(self.a.STATE_SHOWING)):
                raise Refusal("app_control_missing")
        if not self.enabled(node):
            raise Refusal("app_control_disabled")
        # A fresh already-desired switch needs no input. switch() still performs
        # its mandatory checked-state readback through the same native locator.
        if target_state is not None and target_state.contains(self.a.STATE_CHECKED) == switch_target:
            return
        if scroll:
            try:
                node.queryComponent().scrollTo(self.a.SCROLL_ANYWHERE)
            except (RuntimeError, AttributeError, NotImplementedError):
                pass
        interface = node.queryAction()
        offered = interface.nActions
        examined, activate, check, uncheck, other = 0, 0, 0, 0, 0
        for index in range(min(offered, 8)):
            name = interface.getName(index).lower()
            examined += 1
            if (name in ("click", "press", "toggle", "select")
                    or (name == "uncheck" and switch_target is False)
                    or (name == "check" and switch_target is True)):
                if not interface.doAction(index):
                    raise Refusal("app_action_refused")
                return
            # Unsupported names for this request stay diagnostic buckets only.
            if name == "activate":
                activate += 1
            elif name == "check":
                check += 1
            elif name == "uncheck":
                uncheck += 1
            else:
                other += 1
        # One terminal, closed numeric line. Never retain or print AX names,
        # labels, nodes, exceptions or a global last-action observation. Getter
        # failures still follow the unchanged bounded failure_diagnostic path.
        count_valid = type(offered) is int and offered >= 0
        print("app-action-capabilities: " + json.dumps({
            "version": 1, "offeredActionCountValid": count_valid,
            "offeredActionCount": min(offered, 8) if count_valid else 0,
            "offeredActionCountCapped": count_valid and offered > 8,
            "examinedActionCount": examined, "activateCount": activate,
            "checkCount": check, "uncheckCount": uncheck, "otherCount": other,
        }, sort_keys=True), file=sys.stderr, flush=True)
        raise Refusal("app_action_unavailable")

    def button(self, name):
        self.action(self.one(name, {self.a.ROLE_PUSH_BUTTON}))

    def entry(self, name, value):
        if name in MODEL_ENTRIES or getattr(self, "native_input", None) is not None:
            return self.model_entry(name, value)
        node = self.one(name, {self.a.ROLE_ENTRY, self.a.ROLE_TEXT})
        if not self.enabled(node) or not node.getState().contains(self.a.STATE_EDITABLE):
            raise Refusal("app_entry_not_editable")
        node.queryComponent().grabFocus()
        if not node.queryEditableText().setTextContents(value):
            raise Refusal("app_entry_write_failed")

    def model_entry(self, name, value):
        """Bounded fixed-field reveal, with explicitly bound native guest input."""
        deadline = time.monotonic() + 12
        policy = getattr(self, "native_input", None)
        if name not in MODEL_ENTRIES and not (policy is not None and name == MESSAGE):
            raise Refusal("app_control_missing")
        roles = {self.a.ROLE_ENTRY, self.a.ROLE_TEXT}
        names = {"Model", "Model Load models"} if name == "Model" else {name}
        stats = {"version": 1, "control": 1 if name == "API base URL" else 3 if name == MESSAGE else 2,
                 "lookupCount": 0, "examinedNodeCount": 0, "exactNameCount": 0,
                 "entryRoleCount": 0, "exactEntryCount": 0,
                 "exactShowingCount": 0, "exactOffscreenCount": 0,
                 "normalizedNameOnlyCount": 0, "metadataUnavailableCount": 0,
                 "scrollAttempts": 0, "writeAttempts": 0, "expired": 0,
                 "nativeOperation": 0}

        def native(call, operation=None):
            if operation is not None:
                stats["nativeOperation"] = operation
            if time.monotonic() >= deadline:
                raise Refusal("app_control_missing")
            result = call()
            if time.monotonic() >= deadline:
                raise Refusal("app_control_missing")
            return result

        def locate():
            current = native(lambda: self.studio_frame(active=False))
            stats["lookupCount"] = min(stats["lookupCount"] + 1, 256)
            for key in ("examinedNodeCount", "exactNameCount", "entryRoleCount",
                        "exactEntryCount", "exactShowingCount", "exactOffscreenCount",
                        "normalizedNameOnlyCount", "metadataUnavailableCount"):
                stats[key] = 0
            matches = []
            # The same owned/VISIBLE traversal as find(), but remember only
            # bounded counts from the live lookup: no post-deadline AX query.
            for node in self.nodes(include_offscreen=True, frame=current):
                native(lambda: None)
                stats["examinedNodeCount"] += 1
                try:
                    label, role, state = native(lambda: node.name), native(node.getRole), native(node.getState)
                    if type(label) is not str:
                        stats["metadataUnavailableCount"] += 1
                        continue
                    expected_role = role in roles
                    stats["entryRoleCount"] += int(expected_role)
                    if label not in names:
                        if len(label) <= 128 and " ".join(label.split()) in names:
                            stats["normalizedNameOnlyCount"] += 1
                        continue
                    stats["exactNameCount"] += 1
                    if expected_role:
                        stats["exactEntryCount"] += 1
                        showing = native(lambda: state.contains(self.a.STATE_SHOWING))
                        stats["exactShowingCount" if showing else "exactOffscreenCount"] += 1
                        matches.append((node, showing))
                except (RuntimeError, AttributeError, NotImplementedError, TypeError, ValueError):
                    stats["metadataUnavailableCount"] += 1
            native(lambda: None)
            if len(matches) > 1:
                raise Refusal("ambiguous_app_control")
            if not matches:
                return None
            node, showing = matches[0]
            if showing:
                return node
            if stats["scrollAttempts"] >= 2:
                return None
            component = native(node.queryComponent)
            stats["scrollAttempts"] += 1
            if native(lambda: component.scrollTo(self.a.SCROLL_ANYWHERE)) is not True:
                raise Refusal("app_action_refused")
            # Return through a fresh traversal on the next wait poll. Never
            # write to the offscreen accessible that received scrollTo.
            return None

        try:
            if policy is not None:
                native(lambda: policy.validate(name, value))
            self.wait(locate, "app_control_missing", seconds=max(0, deadline - time.monotonic()))
            node = locate()  # Fresh frame/identity/state immediately before editing.
            if node is None:
                raise Refusal("app_control_missing")
            # Closed outer-call milestones only, not capability/operation ACKs:
            # 1 enabled, 2 editable state, 3 EditableText, 4 fresh acquisition,
            # 5 identity, 6 fresh enabled, 7 fresh editable state, 8 text write.
            if not native(lambda: self.enabled(node), 1) or not native(lambda: node.getState().contains(self.a.STATE_EDITABLE), 2):
                raise Refusal("app_entry_not_editable")
            if policy is not None:
                policy.write(self, name, value, node, locate, native, deadline, stats)
                return
            stats["nativeOperation"] = 3
            editor = native(node.queryEditableText)
            stats["nativeOperation"] = 4
            fresh = locate()
            if fresh is None or native(lambda: node is fresh or node == fresh, 5) is not True:
                raise Refusal("app_control_missing")
            if not native(lambda: self.enabled(fresh), 6) or not native(lambda: fresh.getState().contains(self.a.STATE_EDITABLE), 7):
                raise Refusal("app_entry_not_editable")
            stats["writeAttempts"] = 1
            if native(lambda: editor.setTextContents(value), 8) is not True:
                raise Refusal("app_entry_write_failed")
            # Existing save() still requires the actual committed SQLite values.
        except Exception:
            stats["expired"] = int(time.monotonic() >= deadline)
            print("app-model-entry: " + json.dumps(stats, sort_keys=True), file=sys.stderr, flush=True)
            raise

    def studio_frame(self, active=True):
        """Fresh owned/executable/unique showing Studio; ACTIVE remains the default."""
        process_parent(self.pid)
        try:
            executable = (Path("/proc") / str(self.pid) / "exe").resolve(strict=True)
        except (FileNotFoundError, ProcessLookupError):
            raise Refusal("app_process_exited") from None
        if executable != self.root / "acceptance-app":
            raise Refusal("unexpected_app_executable")
        frames = [node for node in self.nodes()
                  if node.getRole() in (self.a.ROLE_FRAME, self.a.ROLE_WINDOW) and node.name == "OrbitPal"]
        if len(frames) != 1:
            raise Refusal("ambiguous_app_window")
        if active and not frames[0].getState().contains(self.a.STATE_ACTIVE):
            raise Refusal("app_window_not_active")
        return frames[0]

    def switch(self, name, enabled):
        """Exact-object AT-SPI input/readback; no foreground or global input."""
        if type(enabled) is not bool:
            raise Refusal("app_action_unavailable")
        roles = {self.a.ROLE_CHECK_BOX, self.a.ROLE_TOGGLE_BUTTON, getattr(self.a, "ROLE_SWITCH", -1)}
        roles.discard(-1)
        # Native Action/Component operations address this exact owned object.
        # Keep foreground checks for geometry/capture/chooser paths separately.
        locate = self.switch_control(name, roles, active=False)
        node = self.wait(locate, "app_control_missing", retry_inactive=True)
        if node.getState().contains(self.a.STATE_CHECKED) != enabled:
            self.action(node, scroll=False, switch_target=enabled, switch_current=locate)
        self.wait(lambda: (node := locate()) is not None and node.getState().contains(self.a.STATE_CHECKED) == enabled, "app_switch_not_changed", retry_inactive=True)

    def switch_control(self, name, roles, active=True):
        """Reveal only fixed source controls, never activate an offscreen node."""
        if name not in SWITCH_CONTROLS:
            raise Refusal("app_control_missing")
        attempts = 0

        def locate():
            nonlocal attempts
            current = self.studio_frame(active=active)
            candidate = self.find(name, roles, include_offscreen=True, frame=current)
            if candidate is None:
                return None
            if candidate.getState().contains(self.a.STATE_SHOWING):
                return candidate
            if attempts >= 2:
                return None
            attempts += 1
            try:
                if candidate.queryComponent().scrollTo(self.a.SCROLL_ANYWHERE) is not True:
                    raise Refusal("app_action_refused")
            except (RuntimeError, AttributeError, NotImplementedError):
                raise Refusal("app_action_unavailable") from None
            # Recheck process/executable/frame and query a fresh SHOWING node;
            # the old offscreen accessible is never used for an action/state.
            node = self.find(name, roles, include_offscreen=True, frame=self.studio_frame(active=active))
            return node if node is not None and node.getState().contains(self.a.STATE_SHOWING) else None

        return locate

    def integration_control(self):
        """One fixed combo, one native reveal budget, no foreground input."""
        deadline = time.monotonic() + 12  # Earlier than the caller's unchanged wait.
        attempts = 0

        def within_deadline():
            if time.monotonic() >= deadline:
                raise Refusal("app_control_missing")

        def locate():
            nonlocal attempts
            within_deadline()
            current = self.studio_frame(active=False)
            within_deadline()
            candidate = self.find("Integration", {self.a.ROLE_COMBO_BOX}, include_offscreen=True, frame=current)
            within_deadline()
            if candidate is None:
                return None
            showing = candidate.getState().contains(self.a.STATE_SHOWING)
            within_deadline()
            if showing:
                return candidate
            if attempts >= 2:
                return None
            try:
                component = candidate.queryComponent()
                within_deadline()
                attempts += 1
                scrolled = component.scrollTo(self.a.SCROLL_ANYWHERE)
                within_deadline()
                if scrolled is not True:
                    raise Refusal("app_action_refused")
            except (RuntimeError, AttributeError, NotImplementedError):
                raise Refusal("app_action_unavailable") from None
            # The scroll may replace the accessible or its owning frame. Rejoin
            # the exact current owned frame; never act on the old offscreen node.
            current = self.studio_frame(active=False)
            within_deadline()
            node = self.find("Integration", {self.a.ROLE_COMBO_BOX}, include_offscreen=True, frame=current)
            within_deadline()
            showing = node is not None and node.getState().contains(self.a.STATE_SHOWING)
            within_deadline()
            return node if showing else None

        return locate

    def integration_diagnostic(self):
        """Terminal bounded counts only; normalized names never grant input."""
        current = self.studio_frame(active=False)
        counts = {"version": 1, "examinedNodeCount": 0, "exactNameCount": 0,
                  "expectedComboRoleCount": 0, "exactComboCount": 0,
                  "exactComboShowingCount": 0, "exactComboOffscreenCount": 0,
                  "normalizedNameOnlyCount": 0, "normalizedComboOnlyCount": 0,
                  "unavailableMetadataCount": 0, "oversizedNameCount": 0}
        # nodes() keeps its existing 1200-node walk limit and owned/VISIBLE gate.
        # This is a point-in-time diagnostic, never a selector or readiness proof.
        for node in self.nodes(include_offscreen=True, frame=current):
            counts["examinedNodeCount"] += 1
            try:
                name, role, state = node.name, node.getRole(), node.getState()
                if type(name) is not str:
                    counts["unavailableMetadataCount"] += 1
                    continue
                if not state.contains(self.a.STATE_VISIBLE):
                    continue
                expected_role = role == self.a.ROLE_COMBO_BOX
                counts["expectedComboRoleCount"] += int(expected_role)
                if name == "Integration":
                    counts["exactNameCount"] += 1
                    if expected_role:
                        counts["exactComboCount"] += 1
                        key = "exactComboShowingCount" if state.contains(self.a.STATE_SHOWING) else "exactComboOffscreenCount"
                        counts[key] += 1
                elif len(name) > 128:
                    counts["oversizedNameCount"] += 1
                elif " ".join(name.split()) == "Integration":
                    counts["normalizedNameOnlyCount"] += 1
                    counts["normalizedComboOnlyCount"] += int(expected_role)
            except (RuntimeError, AttributeError, NotImplementedError, TypeError, ValueError):
                counts["unavailableMetadataCount"] += 1
        print("app-model-selector: " + json.dumps(counts, sort_keys=True), file=sys.stderr, flush=True)

    def select_compatible(self):
        """Select one actual native child of this fixed combo, never an Action alias."""
        deadline = time.monotonic() + 12
        limit = 16
        stats = {"version": 1, "scopeCurrent": 0, "supportedContainerCount": 0,
                 "exactOptionCount": 0, "targetNonShowing": 0, "selectedCount": 0,
                 "selectAttempts": 0, "readbackPassed": 0, "expired": 0}
        choices = {self.a.ROLE_MENU_ITEM, self.a.ROLE_LIST_ITEM, self.a.ROLE_RADIO_MENU_ITEM}
        containers = {getattr(self.a, name, -1) for name in ("ROLE_MENU", "ROLE_LIST_BOX", "ROLE_LIST")}
        containers.discard(-1)

        def native(call):
            if time.monotonic() >= deadline:
                raise Refusal("app_control_missing")
            result = call()
            if time.monotonic() >= deadline:
                raise Refusal("app_control_missing")
            return result

        def same(first, second):
            return first is second or (first is not None and second is not None and native(lambda: first == second) is True)

        def owned(node):
            if node is None or not native(lambda: owned_process(node.get_process_id(), self.pid)):
                raise Refusal("foreign_process")

        def children(parent):
            size = native(lambda: parent.childCount)
            if type(size) is not int or not 0 <= size <= limit:
                raise Refusal("app_action_unavailable")
            result = []
            for index in range(size):
                child = native(lambda: parent.getChildAtIndex(index))
                owned(child)
                actual_index = native(child.getIndexInParent)
                if (type(actual_index) is not int or actual_index != index
                        or not same(native(lambda: child.parent), parent)):
                    raise Refusal("app_control_missing")
                result.append(child)
            return result

        def supported(container):
            try:
                return native(lambda: container.querySelection())
            except (NotImplementedError, AttributeError):
                return None

        def snapshot():
            stats.update(scopeCurrent=0, supportedContainerCount=0, exactOptionCount=0, targetNonShowing=0)
            frame = native(lambda: self.studio_frame(active=False))
            combo = native(lambda: self.find("Integration", {self.a.ROLE_COMBO_BOX}, frame=frame))
            if combo is None:
                return None
            owned(combo)
            state = native(combo.getState)
            if not (state.contains(self.a.STATE_VISIBLE) and state.contains(self.a.STATE_SHOWING)
                    and native(lambda: self.enabled(combo))):
                raise Refusal("app_control_disabled")
            direct = children(combo)
            menu = [child for child in direct if native(child.getRole) in containers]
            if len(menu) > 1:
                raise Refusal("ambiguous_app_control")
            matches = []
            for container, offered in [(combo, direct)] + [(child, None) for child in menu]:
                interface = supported(container)
                if interface is None:
                    continue
                stats["supportedContainerCount"] += 1
                if not native(lambda: self.enabled(container)):
                    raise Refusal("app_control_disabled")
                if offered is None:
                    offered = children(container)
                for index, option in enumerate(offered):
                    name, role = native(lambda: option.name), native(option.getRole)
                    if type(name) is not str or name != "Compatible API · custom endpoint" or role not in choices:
                        continue
                    stats["exactOptionCount"] += 1
                    state = native(option.getState)
                    if not state.contains(self.a.STATE_VISIBLE) or not native(lambda: self.enabled(option)):
                        raise Refusal("app_control_disabled")
                    # SHOWING is unnecessary only for this actual offered child
                    # of the fixed native Selection container; global gates stay.
                    stats["targetNonShowing"] = int(not state.contains(self.a.STATE_SHOWING))
                    matches.append((frame, combo, container, option, index, interface, offered))
            if not stats["supportedContainerCount"]:
                raise Refusal("app_action_unavailable")
            if len(matches) > 1:
                raise Refusal("ambiguous_app_control")
            if not matches:
                return None
            stats["scopeCurrent"] = 1
            return matches[0]

        def same_scope(first, second):
            return second is not None and first[4] == second[4] and all(same(first[i], second[i]) for i in range(4))

        def selected(scope):
            _, _, container, option, index, interface, offered = scope
            count = native(lambda: interface.nSelectedChildren)
            if type(count) is not int or not 0 <= count <= limit:
                raise Refusal("app_action_unavailable")
            stats["selectedCount"] = count
            if count > 1:
                raise Refusal("ambiguous_app_control")
            chosen = native(lambda: interface.isChildSelected(index))
            if type(chosen) is not bool:
                raise Refusal("app_action_unavailable")
            if count == 0:
                if chosen:
                    raise Refusal("app_selection_not_changed")
                return False
            child = native(lambda: interface.getSelectedChild(0))
            owned(child)
            if not same(native(lambda: child.parent), container) or not any(same(child, item) for item in offered):
                raise Refusal("app_control_missing")
            exact = same(child, option)
            if chosen != exact:
                raise Refusal("app_selection_not_changed")
            return exact

        try:
            scope = self.wait(snapshot, "app_control_missing", seconds=max(0, deadline - time.monotonic()))
            current = snapshot()  # Rejoin every identity/index immediately before input.
            if not same_scope(scope, current):
                raise Refusal("app_control_missing")
            already_selected = selected(current)
            if not already_selected:
                # Last blocking preflight read is followed by this same deadline.
                if time.monotonic() >= deadline:
                    raise Refusal("app_control_missing")
                before = snapshot()
                if not same_scope(current, before):
                    raise Refusal("app_control_missing")
                current = before
                def choose():
                    stats["selectAttempts"] = 1
                    return current[5].selectChild(current[4])
                if native(choose) is not True:
                    raise Refusal("app_action_refused")
            def readback():
                fresh = snapshot()
                if not same_scope(current, fresh):
                    raise Refusal("app_control_missing")
                if not selected(fresh):
                    return False
                final_frame = native(lambda: self.studio_frame(active=False))
                if not same(fresh[0], final_frame):
                    raise Refusal("app_control_missing")
                return native(lambda: True)
            self.wait(readback, "app_selection_not_changed", seconds=max(0, deadline - time.monotonic()))
            native(lambda: True)
            stats["readbackPassed"] = 1
        except Refusal:
            stats["scopeCurrent"] = 0
            stats["expired"] = int(time.monotonic() >= deadline)
            print("app-compatible-selection: " + json.dumps(stats, sort_keys=True), file=sys.stderr, flush=True)
            raise
        except (RuntimeError, AttributeError, NotImplementedError, TypeError, ValueError):
            stats["scopeCurrent"] = 0
            stats["expired"] = int(time.monotonic() >= deadline)
            print("app-compatible-selection: " + json.dumps(stats, sort_keys=True), file=sys.stderr, flush=True)
            raise Refusal("app_action_unavailable") from None

    def compatible(self):
        phase("configure_model_integration_lookup")
        locate = self.integration_control()
        try:
            node = self.wait(locate, "app_control_missing")
        except Refusal as error:
            if type(error) is Refusal and type(error.code) is str and error.code == "app_control_missing":
                try:
                    self.integration_diagnostic()
                except Exception:
                    # Optional metadata must neither expose errors nor replace
                    # the original failure if the fresh owned frame is gone.
                    pass
            raise
        self.action(node, scroll=False)  # The locator owns the only reveal budget.
        phase("configure_model_integration_opened")
        phase("configure_model_compatible_lookup")
        self.select_compatible()
        phase("configure_model_compatible_selected")

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
    phase("studio_construct")
    studio = Studio(root)
    phase("studio_wait_cold_start")
    studio.wait(lambda: studio.find("Control room", {studio.a.ROLE_PUSH_BUTTON}), "actual_studio_cold_start_failed", 40)
    phase("studio_read_initial_state")
    initial = studio.profile()
    if initial.get("screenContext", {}).get("enabled") is not False or studio.assistant_count() != 0:
        raise Refusal("unexpected_saved_app_state")
    checks = ["actual_pid_owned_studio_coldstart"]
    phase("actual_studio_coldstart")
    nonce = uuid.uuid4().hex[:16]
    with Receiver(nonce) as receiver:
        studio.bind_native_input(receiver, nonce)
        phase("configure_real_voice_off")
        phase("configure_voice_navigation")
        studio.button("Character & voice")
        phase("configure_voice_navigation_done")
        phase("configure_voice_switch")
        studio.switch("Enable speech output", False)
        studio.save(lambda value: value["voice"]["enabled"] is False)
        studio.button("Models")
        phase("configure_real_model_and_grants")
        studio.compatible()
        phase("configure_model_endpoint_entry")
        studio.entry("API base URL", receiver.base_url)
        phase("configure_model_name_entry")
        studio.entry("Model", "orbitpal-wayland-fixture")
        phase("configure_model_tools_switch")
        studio.switch("Enable tool calls", False)
        phase("configure_model_images_switch")
        studio.switch("Send images to the model", True)
        phase("configure_model_screen_switch")
        studio.switch("Include the screen with every request", True)
        phase("configure_model_save")
        studio.save(lambda value: value["model"]["provider"] == "compatible"
                    and value["model"]["baseUrl"] == receiver.base_url
                    and value["model"]["model"] == "orbitpal-wayland-fixture"
                    and value["model"]["apiKeyId"] == ""
                    and value["model"]["toolsEnabled"] is False
                    and value["model"]["visionEnabled"] is True
                    and value["screenContext"]["enabled"] is True)
        phase("configure_model_saved")
        if receiver.state.records:
            raise Refusal("inference_before_explicit_send")
        phase("configure_model_source_selection")
        studio.choose()
        phase("actual_portal_ready")
        checks.append("real_ui_grant_and_portal_selection")
        red = studio.send(receiver, nonce, "fresh_red", "red")
        phase("fresh_red_presented")
        blue = studio.send(receiver, nonce, "fresh_blue", "blue", True)
        # Positive control for the SAME normal held SSE contribution/config.
        # Releasing without an operator mutation MUST visibly commit.
        receiver.state.release("fresh_blue")
        receiver.state.wait_finished("fresh_blue")
        studio.presented(nonce, "fresh_blue")
        phase("fresh_blue_presented")
        if red["digest"] == blue["digest"] or len(receiver.state.records) != 2:
            raise Refusal("stale_or_repeated_screen")
        checks.append("two_fresh_real_jpegs_through_agent_provider")
        checks.append("held_positive_reply_commits_history_and_subtitle")
        studio.send(receiver, nonce, "held_stop", "red", True)
        phase("held_stop_received")
        studio.button("Stop")
        studio.wait(lambda: studio.enabled(studio.one("Message to your cohost", {studio.a.ROLE_ENTRY, studio.a.ROLE_TEXT})), "stop_not_observed")
        receiver.state.release("held_stop")
        receiver.state.wait_finished("held_stop")
        studio.not_presented(nonce, "held_stop")
        studio.button("Models")
        studio.wait(lambda: studio.text_visible("No Wayland source connected. Requests with screen context are rejected; the app does not use another screen."), "stop_did_not_close_session")
        checks.append("held_provider_stop_rejects_late_reply")
        phase("held_stop_cancelled")
        studio.choose()  # Explicit real reconnect; Stop never restores a lease.
        studio.send(receiver, nonce, "held_images_off", "red", True)
        phase("held_images_off_received")
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
        phase("held_disable_received")
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


def main():
    try:
        phase("validate_private_environment")
        run(isolated_root())
    except Refusal as error:
        # Codes are authored fixed strings; never dump exceptions/tree/profile.
        print("app-diagnostic: " + json.dumps(failure_diagnostic(error), sort_keys=True), file=sys.stderr, flush=True)
        print("app-acceptance-failed: " + refusal_code(error), file=sys.stderr, flush=True)
        return 1
    except Exception as error:
        print("app-diagnostic: " + json.dumps(failure_diagnostic(error), sort_keys=True), file=sys.stderr, flush=True)
        print("app-acceptance-failed: controller_failed", file=sys.stderr, flush=True)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
