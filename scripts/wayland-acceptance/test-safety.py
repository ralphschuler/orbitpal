#!/usr/bin/env python3
"""Pure local fixtures: no compositor, GUI API, VM, sudo or capture is invoked."""
import ast
import json
import math
import os
import re
import stat
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock

DIRECTORY = Path(__file__).resolve().parent


class HarnessSafety(unittest.TestCase):
    def parser(self, events):
        return subprocess.run([sys.executable, str(DIRECTORY / "test-binary.py")],
                              input="".join(json.dumps(event) + "\n" for event in events),
                              text=True, capture_output=True, timeout=5)

    def test_requires_exactly_the_compiled_native_library_test_target(self):
        with tempfile.TemporaryDirectory(prefix="orbitpal-harness-safety-") as directory:
            executable = Path(directory) / "fixture-never-executed"
            executable.write_text("not executable code\n", encoding="utf-8")
            executable.chmod(0o700)
            event = {"reason": "compiler-artifact", "target": {"name": "orbitpal_lib"},
                     "profile": {"test": True}, "executable": str(executable)}
            accepted = self.parser([{"reason": "build-finished", "success": True}, event])
            self.assertEqual(accepted.returncode, 0, accepted.stderr)
            self.assertEqual(accepted.stdout.strip(), str(executable))
            for override in ({"target": {"name": "another_crate"}}, {"profile": {"test": False}},
                             {"executable": "relative-binary"}, {"executable": None}):
                self.assertNotEqual(self.parser([{**event, **override}]).returncode, 0)
            self.assertNotEqual(self.parser([]).returncode, 0)
            self.assertNotEqual(self.parser([event, {**event, "executable": "/different"}]).returncode, 0)

    def test_readiness_requires_our_real_service_pid_not_an_early_activated_owner(self):
        def check(result, expected="31180"):
            return subprocess.run([sys.executable, str(DIRECTORY / "service-owner.py"), expected],
                                  input=result, text=True, capture_output=True, timeout=5)
        self.assertEqual(check("(uint32 31180,)\n").returncode, 0)
        # Both names may be available, but the stale daemon has the wrong
        # activation environment and must never satisfy our readiness barrier.
        self.assertNotEqual(check("(uint32 31045,)\n").returncode, 0)
        for result in ("(true,)\n", "(uint32 31180, uint32 31045)", "(31180,)",
                       "(uint32 -1,)", "(uint32 0,)", "(uint32 2147483648,)",
                       "(uint32 31180,)\nextra", "x" * 1025, "\N{LATIN SMALL LETTER E WITH ACUTE}"):
            with self.subTest(result=result[:80]):
                self.assertNotEqual(check(result).returncode, 0)
        for expected in ("0", "-1", "true", "2147483648", "31180\n"):
            with self.subTest(expected=expected):
                self.assertNotEqual(check("(uint32 31180,)", expected).returncode, 0)

    def test_session_refuses_a_foreign_bus_before_invoking_any_device_probe(self):
        with tempfile.TemporaryDirectory(prefix="orbitpal-wayland.") as directory:
            root = Path(directory).resolve()
            sentinel = root / "probe-was-called"
            (root / "render-probe").write_text('#!/bin/sh\nprintf x >"$ORBITPAL_PROBE_SENTINEL"\nexit 73\n', encoding="utf-8")
            (root / "render-probe").chmod(0o700)
            expected = f"unix:path={root}/runtime/session-bus"
            base = {"PATH": os.defpath, "ORBITPAL_ACCEPTANCE_ROOT": str(root), "ORBITPAL_ACCEPTANCE_DESKTOP": "gnome",
                    "ORBITPAL_WAYLAND_ACCEPTANCE": "synthetic-desktop-only", "RUNNER_ENVIRONMENT": "github-hosted",
                    "RUNNER_OS": "Linux", "GITHUB_ACTIONS": "true", "ORBITPAL_ACCEPTANCE_DRM": "/never-opened",
                    "ORBITPAL_PROBE_SENTINEL": str(sentinel)}
            for desktop in ("gnome", "kde"):
                for bus in ("", "unix:path=/host/session-bus", expected + "-other", expected + ",guid=wrong"):
                    with self.subTest(desktop=desktop, bus=bus):
                        completed = subprocess.run(["bash", str(DIRECTORY / "session.sh")],
                                                   env={**base, "ORBITPAL_ACCEPTANCE_DESKTOP": desktop, "DBUS_SESSION_BUS_ADDRESS": bus}, capture_output=True, timeout=5)
                        self.assertNotEqual(completed.returncode, 73)
                        self.assertFalse(sentinel.exists())
            for bus in (expected, expected + ",guid=" + "a" * 32):
                with self.subTest(bus=bus):
                    completed = subprocess.run(["bash", str(DIRECTORY / "session.sh")],
                                               env={**base, "DBUS_SESSION_BUS_ADDRESS": bus}, capture_output=True, timeout=5)
                    self.assertEqual(completed.returncode, 73, completed.stderr)
                    self.assertTrue(sentinel.exists())
                    sentinel.unlink()

    def test_only_gnome_replaces_an_early_backend_without_changing_kde_arguments(self):
        source = (DIRECTORY / "session.sh").read_text(encoding="utf-8")
        function = re.search(r"^start_portal_backend\(\) \{.*?^\}", source, re.MULTILINE | re.DOTALL)
        self.assertIsNotNone(function)
        script = "set -eu\nstart() { printf '%s\\0' \"$@\"; }\n" + function.group() + '\nstart_portal_backend "$@"\n'
        for desktop, flags in (("gnome", ["--replace"]), ("kde", [])):
            result = subprocess.run(["bash", "-c", script, "fixture", desktop, "/synthetic path/backend"], capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.split(b"\0")[:-1], [b"portal-backend", b"/synthetic path/backend", *[flag.encode() for flag in flags]])
        for args in ([], ["gnome"], ["other", "/never-invoked"], ["gnome", "/never-invoked", "--extra"]):
            result = subprocess.run(["bash", "-c", script, "fixture", *args], capture_output=True, timeout=5)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")

    def test_owner_diagnostics_are_bounded_metadata_and_cannot_accept_a_wrong_owner(self):
        for raw, expected, actual in ((b"(uint32 123,)\n", "123", "123"), (b"(uint32 124,)\n", "123", "124"),
                                      (b"marker-secret-invalid", "123", "unavailable"), (b"x" * 1025, "123", "unavailable"),
                                      (b"\xff", "123", "unavailable"), (b"(uint32 2147483648,)\n", "123", "unavailable")):
            result = subprocess.run([sys.executable, str(DIRECTORY / "service-owner.py"), "--diagnostic", expected], input=raw, capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(f"expected={expected}; actual={actual}".encode(), result.stdout)
            self.assertLess(len(result.stdout), 120)
            self.assertNotIn(b"marker-secret", result.stdout)
            strict = subprocess.run([sys.executable, str(DIRECTORY / "service-owner.py"), expected], input=raw, capture_output=True, timeout=5)
            self.assertEqual(strict.returncode == 0, actual == expected)

    def test_kde_setup_requires_the_menu_and_publishes_only_its_prefix_before_activation(self):
        # Execute the actual setup function using a recorder instead of D-Bus.
        # No compositor, device, desktop API or real activation service is used.
        source = (DIRECTORY / "session.sh").read_text(encoding="utf-8")
        function = re.search(r"^prepare_kde_environment\(\) \{.*?^\}", source, re.MULTILINE | re.DOTALL)
        self.assertIsNotNone(function)
        with tempfile.TemporaryDirectory(prefix="orbitpal-wayland.") as directory:
            root = Path(directory).resolve()
            menu = root / "plasma-applications.menu"
            menu.write_text("<Menu/>\n", encoding="utf-8")
            link = root / "symlink.menu"
            link.symlink_to(menu)
            trace = root / "activation.txt"
            recorder = root / "dbus-update-activation-environment"
            recorder.write_text('#!/bin/sh\nprintf "%s\\n" "$XDG_MENU_PREFIX" "$DBUS_SESSION_BUS_ADDRESS" "$#" "$@" >"$ACTIVATION_TRACE"\n', encoding="utf-8")
            recorder.chmod(0o700)
            bus = f"unix:path={root}/runtime/session-bus"
            env = {"PATH": str(root) + os.pathsep + os.defpath, "ACTIVATION_TRACE": str(trace),
                   "DBUS_SESSION_BUS_ADDRESS": bus, "XDG_MENU_PREFIX": "unrelated-"}
            # Calling from an if deliberately disables Bash's implicit errexit
            # inside functions. The missing/linked menu must still fail closed.
            script = "set -eu\n" + function.group() + '\nif prepare_kde_environment "$1"; then exit 0; else exit 79; fi\n'
            result = subprocess.run(["bash", "-c", script, "fixture", str(menu)], env=env,
                                    text=True, capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(trace.read_text().splitlines(), ["plasma-", bus, "1", "XDG_MENU_PREFIX"])
            trace.unlink()
            for rejected in (root / "missing.menu", link, root):
                with self.subTest(menu=str(rejected)):
                    result = subprocess.run(["bash", "-c", script, "fixture", str(rejected)], env=env,
                                            text=True, capture_output=True, timeout=5)
                    self.assertEqual(result.returncode, 79, result.stderr)
                    self.assertFalse(trace.exists(), "invalid menu must not alter any activation environment")

    def test_runtime_refuses_missing_or_nonhosted_platform_markers_before_provisioning(self):
        # Each case fails at the first condition, before mktemp/useradd/sudo.
        hosted = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux"}
        for override in ({"GITHUB_ACTIONS": "false"}, {"RUNNER_ENVIRONMENT": "self-hosted"}, {"RUNNER_OS": "macOS"}):
            result = subprocess.run(["bash", str(DIRECTORY / "run.sh"), "gnome", "/never-used"],
                                    env={"PATH": os.defpath, **hosted, **override},
                                    capture_output=True, timeout=5)
            self.assertNotEqual(result.returncode, 0)


class ControllerConsent(unittest.TestCase):
    """Execute real controller methods with metadata-only fake accessibility."""
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="orbitpal-wayland.")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        tree = ast.parse((DIRECTORY / "desktop.py").read_text(encoding="utf-8"))
        wanted = {"atomic_json", "sensitive", "chooser_title", "kde6_monitor", "pointer_escape", "select_single_monitor", "translated_pointer", "kwin_geometry", "Desktop"}
        module = ast.Module(body=[item for item in tree.body if isinstance(item, (ast.FunctionDef, ast.ClassDef)) and item.name in wanted], type_ignores=[])
        self.api = SimpleNamespace(STATE_SENSITIVE="sensitive", STATE_ENABLED="enabled", STATE_PRESSED="pressed", STATE_CHECKED="checked", STATE_SELECTED="selected", ROLE_LIST_ITEM="card", ROLE_TOGGLE_BUTTON="toggle", ROLE_PUSH_BUTTON="button")
        self.namespace = {"CONTROL": self.root, "ROOT": self.root, "DESKTOP": "kde", "KDE6": False, "TITLES": {"gnome": "Share Screen", "kde": "Screen Sharing"}, "json": json,
                          "time": SimpleNamespace(monotonic=lambda: 0, time=lambda: 10), "pyatspi": self.api, "math": math, "re": re, "stat": stat, "sys": sys,
                          "TRACE": SimpleNamespace(log=lambda *args, **kwargs: None, begin=lambda *args: None, finish=lambda: None),
                          "action": Mock(return_value=True), "pointer_click": Mock(return_value=True),
                          "visible": lambda node: True, "walk": lambda node, *args: [node]}
        exec(compile(module, str(DIRECTORY / "desktop.py"), "exec"), self.namespace)

    def node(self, states):
        return SimpleNamespace(name="Virtual-1", getRole=lambda: "card", getState=lambda: SimpleNamespace(contains=lambda state: state in states))

    def controller(self, action="allow"):
        controller = self.namespace["Desktop"].__new__(self.namespace["Desktop"])
        controller.sequence = 0
        controller.pending = None
        controller.deadline = 0
        (self.root / "command.json").write_text(json.dumps({"sequence": 1, "action": action}), encoding="utf-8")
        self.namespace["chooser"] = Mock(return_value=object())
        self.namespace["button"] = Mock(return_value=self.node({"sensitive", "enabled"}))
        return controller

    def test_ready_requires_observed_checked_not_accepted_selection_action(self):
        states = {"sensitive", "enabled"}
        node = self.node(states)
        select = self.namespace["select_single_monitor"]
        self.assertEqual(select(node, "accessible"), (False, True))
        self.namespace["action"].assert_called_once_with(node)
        self.assertEqual(select(node), (False, False))
        self.assertEqual(select(node, "pointer"), (False, True))
        self.namespace["pointer_click"].assert_called_once_with(node, node)
        states.add("checked")
        self.assertEqual(select(node, "accessible"), (True, False))
        self.namespace["action"].assert_called_once_with(node)

    def geometry_fixture(self):
        target = {"x": 278, "y": 86, "width": 232, "height": 262}
        window = {"x": 0, "y": 0, "width": 600, "height": 400}
        snapshot = {"nonce": "fixture", "timestamp": 10000, "count": 1, "matches": [{"pid": 42, "caption": "Screen Sharing — portal", "buffer": {"x": 340, "y": 160, "width": 600, "height": 400}}]}
        return target, window, snapshot

    def test_pointer_uses_actual_global_buffer_and_window_local_offset(self):
        target, window, snapshot = self.geometry_fixture()
        self.assertEqual(self.namespace["translated_pointer"](target, window, snapshot, "fixture", 42, 10010), (734, 377))

    def test_geometry_refuses_stale_ambiguous_pid_size_nonfinite_and_outside(self):
        for case in ("nonce", "stale", "future", "ambiguous", "pid", "size", "nan", "outside", "target", "empty"):
            target, window, snapshot = self.geometry_fixture()
            if case == "nonce": snapshot["nonce"] = "old"
            if case == "stale": snapshot["timestamp"] = 4000
            if case == "future": snapshot["timestamp"] = 11000
            if case == "ambiguous": snapshot["count"] = 2
            if case == "pid": snapshot["matches"][0]["pid"] = 43
            if case == "size": snapshot["matches"][0]["buffer"]["width"] = 601
            if case == "nan": snapshot["matches"][0]["buffer"]["x"] = float("nan")
            if case == "outside": snapshot["matches"][0]["buffer"]["x"] = 1000
            if case == "target": target["x"] = 599
            if case == "empty": target["width"] = 0
            with self.subTest(case=case), self.assertRaises(RuntimeError):
                self.namespace["translated_pointer"](target, window, snapshot, "fixture", 42, 10010)

    def kwin_fixture(self, fail_run=False, fail_cleanup=False):
        runtime = self.root / "runtime"
        runtime.mkdir(mode=0o700)
        nonce = "a" * 32
        env = {"XDG_RUNTIME_DIR": str(runtime), "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime}/session-bus"}
        self.namespace["os"] = SimpleNamespace(geteuid=lambda: os.geteuid(), environ=env)
        self.namespace["uuid"] = SimpleNamespace(uuid4=lambda: SimpleNamespace(hex=nonce))
        self.namespace["subprocess"] = SimpleNamespace(run=Mock())
        snapshot = {"nonce": nonce, "timestamp": 10000, "count": 1, "matches": []}
        (self.root / "compositor.txt").write_text("js: ORBITPAL_KWIN_GEOMETRY:" + nonce + ":" + json.dumps(snapshot) + "\n", encoding="utf-8")
        def run(arguments, **kwargs):
            self.assertNotIn("shell", kwargs)
            self.assertTrue(0 < kwargs["timeout"] <= 1)
            method = arguments[arguments.index("--method") + 1]
            if method == "org.kde.kwin.Scripting.loadScript":
                script = Path(arguments[-2])
                self.assertIn("windows[i].pid === 42", script.read_text(encoding="utf-8"))
                self.assertIn("console.warn", script.read_text(encoding="utf-8"))
                return SimpleNamespace(stdout="(7,)\n")
            if method.endswith(".run") and fail_run:
                raise RuntimeError("fixture script failure")
            if method.endswith("unloadScript") and fail_cleanup:
                raise RuntimeError("fixture unload failure")
            return SimpleNamespace(stdout="()\n")
        self.namespace["subprocess"].run.side_effect = run
        return snapshot, nonce

    def test_kwin_reads_private_bus_and_unloads_then_removes_script(self):
        snapshot, nonce = self.kwin_fixture()
        self.assertEqual(self.namespace["kwin_geometry"](42), (snapshot, nonce))
        calls = self.namespace["subprocess"].run.call_args_list
        self.assertEqual(len(calls), 3)
        self.assertIn("/7", calls[1].args[0])
        self.assertTrue(calls[-1].args[0][-2].endswith("unloadScript"))
        self.assertEqual(list(self.root.glob("*.js")), [])

    def test_kwin6_uses_current_window_list_and_registered_script_path(self):
        self.namespace["KDE6"] = True
        snapshot, nonce = self.kwin_fixture()
        self.assertEqual(self.namespace["kwin_geometry"](42), (snapshot, nonce))
        calls = self.namespace["subprocess"].run.call_args_list
        self.assertIn("/Scripting/Script7", calls[1].args[0])

    def test_kwin_cleanup_runs_on_script_error_and_local_file_removed_on_unload_error(self):
        for fail_cleanup in (False, True):
            with self.subTest(fail_cleanup=fail_cleanup):
                if (self.root / "runtime").exists(): (self.root / "runtime").rmdir()
                self.kwin_fixture(fail_run=True, fail_cleanup=fail_cleanup)
                with self.assertRaises(RuntimeError): self.namespace["kwin_geometry"](42)
                calls = self.namespace["subprocess"].run.call_args_list
                self.assertEqual(len(calls), 3)
                self.assertTrue(calls[-1].args[0][-2].endswith("unloadScript"))
                self.assertEqual(list(self.root.glob("*.js")), [])

    def test_kwin_rejects_foreign_bus_without_any_dbus_call(self):
        self.kwin_fixture()
        self.namespace["os"].environ["DBUS_SESSION_BUS_ADDRESS"] = "unix:path=/host/session-bus"
        with self.assertRaises(RuntimeError): self.namespace["kwin_geometry"](42)
        self.namespace["subprocess"].run.assert_not_called()

    def test_gtk4_sensitive_without_enabled_and_kde_mapping(self):
        self.assertFalse(self.namespace["sensitive"](self.node({"sensitive"})))
        self.assertTrue(self.namespace["sensitive"](self.node({"sensitive", "enabled"})))
        self.namespace["DESKTOP"] = "gnome"
        self.assertTrue(self.namespace["sensitive"](self.node({"sensitive"})))
        self.assertFalse(self.namespace["sensitive"](self.node({"enabled"})))

    def test_sole_empty_kde_card_with_named_new_virtual_excluded(self):
        existing = self.node({"sensitive", "enabled", "checked"})
        existing.name = ""
        virtual = self.node({"sensitive", "enabled"})
        virtual.name = "New Virtual Output"
        dialog = self.node({"sensitive", "enabled"})
        dialog.getRole = lambda: "dialog"
        self.namespace["walk"] = lambda node, *args: [node, virtual, existing] if node is dialog else [node]
        self.assertEqual(self.namespace["select_single_monitor"](dialog), (True, False))
        self.namespace["action"].assert_not_called()
        self.namespace["pointer_click"].assert_not_called()
        second = self.node({"sensitive", "enabled", "checked"})
        second.name = ""
        self.namespace["walk"] = lambda node, *args: [node, virtual, existing, second] if node is dialog else [node]
        self.assertEqual(self.namespace["select_single_monitor"](dialog), (False, False))
        self.namespace["walk"] = lambda node, *args: [node, virtual] if node is dialog else [node]
        self.assertEqual(self.namespace["select_single_monitor"](dialog), (False, False))

    def test_selection_precedes_share_and_an_action_success_never_acks(self):
        controller = self.controller()
        self.namespace["select_single_monitor"] = Mock(side_effect=[(False, True), (True, False)])
        controller.poll()
        self.namespace["button"].assert_not_called()
        self.namespace["action"].assert_not_called()
        self.assertFalse((self.root / "ack.json").exists())
        controller.poll()
        self.namespace["action"].assert_called_once()
        self.assertFalse((self.root / "ack.json").exists())
        self.assertEqual(controller.pending, "allow")

    def test_noop_press_gets_one_pointer_retry_and_acks_only_after_dismissal(self):
        controller = self.controller()
        self.namespace["select_single_monitor"] = Mock(return_value=(True, False))
        controller.poll()
        controller.poll()
        controller.poll()
        self.namespace["action"].assert_called_once()
        self.namespace["pointer_click"].assert_called_once()
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["chooser"].return_value = None
        controller.poll()
        ack = json.loads((self.root / "ack.json").read_text(encoding="utf-8"))
        self.assertEqual(ack, {"sequence": 1, "ok": True, "error": ""})
        self.assertIsNone(controller.pending)

    def test_cancel_also_waits_for_real_dialog_dismissal(self):
        controller = self.controller("cancel")
        self.namespace["select_single_monitor"] = Mock()
        controller.poll()
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["select_single_monitor"].assert_not_called()
        self.namespace["chooser"].return_value = None
        controller.poll()
        self.assertTrue(json.loads((self.root / "ack.json").read_text(encoding="utf-8"))["ok"])

    def test_gnome_preselected_card_is_not_toggled_and_needs_no_pointer(self):
        self.namespace["DESKTOP"] = "gnome"
        node = self.node({"sensitive", "pressed"})
        node.getRole = lambda: "toggle"
        self.assertEqual(self.namespace["select_single_monitor"](node, "accessible"), (True, False))
        self.namespace["action"].assert_not_called()
        controller = self.controller()
        self.namespace["select_single_monitor"] = Mock(return_value=(True, False))
        controller.poll()
        controller.poll()
        self.namespace["action"].assert_called_once()
        self.namespace["pointer_click"].assert_not_called()
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["chooser"].return_value = None
        controller.poll()
        self.assertTrue((self.root / "ack.json").exists())

    def test_missing_dialog_before_any_action_does_not_ack(self):
        controller = self.controller()
        self.namespace["chooser"].return_value = None
        controller.poll()
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["action"].assert_not_called()

    def test_kde6_selects_only_the_named_existing_monitor_button(self):
        self.namespace["KDE6"] = True
        (self.root / "output-name.txt").write_text("('Virtual-1',)\n", encoding="utf-8")
        existing = self.node({"sensitive", "enabled"})
        existing.getRole = lambda: "button"
        virtual = self.node({"sensitive", "enabled"})
        virtual.name, virtual.getRole = "Share virtual screen", lambda: "button"
        region = self.node({"sensitive", "enabled"})
        region.name, region.getRole = "Share region", lambda: "button"
        dialog = object()
        self.namespace["walk"] = lambda node: [virtual, region, existing]
        self.assertIs(self.namespace["kde6_monitor"](dialog), existing)
        self.namespace["walk"] = lambda node: [virtual, region]
        self.assertIsNone(self.namespace["kde6_monitor"](dialog))
        self.namespace["walk"] = lambda node: [existing, existing]
        self.assertIsNone(self.namespace["kde6_monitor"](dialog))
        (self.root / "output-name.txt").write_text("('Virtual-1', 'other')", encoding="utf-8")
        with self.assertRaises(RuntimeError): self.namespace["kde6_monitor"](dialog)

    def test_kde6_single_choice_click_does_not_ack_until_actual_dismissal(self):
        self.namespace["KDE6"] = True
        controller = self.controller()
        self.namespace["kde6_monitor"] = Mock(return_value=self.node({"sensitive", "enabled"}))
        controller.poll()
        self.namespace["action"].assert_called_once()
        self.namespace["button"].assert_not_called()
        self.assertFalse((self.root / "ack.json").exists())
        controller.poll()
        self.namespace["pointer_click"].assert_called_once()
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["chooser"].return_value = None
        controller.poll()
        self.assertTrue(json.loads((self.root / "ack.json").read_text())["ok"])

    def test_kde6_escape_requires_fresh_focused_exact_chooser(self):
        self.namespace["KDE6"] = True
        (self.root / "pointer").write_text("fixture-never-executed")
        snapshot = {"nonce": "test", "timestamp": 10000, "count": 1, "matches": [{"pid": 42, "active": True}]}
        self.namespace["kwin_geometry"] = Mock(return_value=(snapshot, "test"))
        self.namespace["subprocess"] = SimpleNamespace(run=Mock(return_value=SimpleNamespace(returncode=0)))
        dialog = SimpleNamespace(name="Share screen with the requesting application", get_process_id=lambda: 42)
        self.assertTrue(self.namespace["pointer_escape"](dialog))
        self.namespace["subprocess"].run.assert_called_once_with([str(self.root / "pointer"), "--escape"], capture_output=True, timeout=6, check=False)
        for override in ({"timestamp": 0}, {"nonce": "wrong"}, {"count": 2}, {"count": True}, {"matches": [{"pid": 43, "active": True}]}, {"matches": [{"pid": 42, "active": "true"}]}):
            self.namespace["kwin_geometry"].return_value = ({**snapshot, **override}, "test")
            self.namespace["subprocess"].run.reset_mock()
            with self.assertRaises(RuntimeError): self.namespace["pointer_escape"](dialog)
            self.namespace["subprocess"].run.assert_not_called()
        for override in ({"count": 0, "matches": []}, {"matches": [{"pid": 42, "active": False}]}):
            self.namespace["kwin_geometry"].return_value = ({**snapshot, **override}, "test")
            self.namespace["subprocess"].run.reset_mock()
            self.assertFalse(self.namespace["pointer_escape"](dialog))
            self.namespace["subprocess"].run.assert_not_called()

    def test_kde6_cancel_waits_for_mapping_and_focus_without_claiming_consent(self):
        self.namespace["KDE6"] = True
        self.namespace["pointer_escape"] = Mock(side_effect=[False, False, True])
        controller = self.controller("cancel")
        for _ in range(2):
            self.assertTrue(controller.poll())
            self.assertFalse(controller.consent_attempted)
            self.assertFalse((self.root / "ack.json").exists())
        controller.poll()
        self.assertTrue(controller.consent_attempted)
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["chooser"].return_value = None
        controller.poll()
        self.assertTrue(json.loads((self.root / "ack.json").read_text())["ok"])

    def test_kde6_cancel_focus_wait_uses_existing_action_deadline(self):
        self.namespace["KDE6"] = True
        self.namespace["pointer_escape"] = Mock(return_value=False)
        self.namespace["text_tree"] = Mock(return_value="synthetic timeout metadata")
        controller = self.controller("cancel")
        controller.poll()
        controller.deadline = 0
        controller.poll()
        self.assertFalse(controller.consent_attempted)
        self.assertFalse(json.loads((self.root / "ack.json").read_text())["ok"])

    def test_kde6_cancel_uses_escape_and_waits_for_actual_dismissal(self):
        self.namespace["KDE6"] = True
        self.namespace["pointer_escape"] = Mock(return_value=True)
        controller = self.controller("cancel")
        controller.poll()
        self.namespace["pointer_escape"].assert_called_once()
        self.assertFalse((self.root / "ack.json").exists())
        self.namespace["chooser"].return_value = None
        controller.poll()
        self.assertTrue(json.loads((self.root / "ack.json").read_text())["ok"])


if __name__ == "__main__":
    unittest.main()
