#!/usr/bin/env python3
"""Pure local fixtures: no compositor, GUI API, VM, sudo or capture is invoked."""
import ast
import json
import os
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
        self.directory = tempfile.TemporaryDirectory(prefix="orbitpal-consent-fixture-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        tree = ast.parse((DIRECTORY / "desktop.py").read_text(encoding="utf-8"))
        wanted = {"atomic_json", "sensitive", "select_single_monitor", "Desktop"}
        module = ast.Module(body=[item for item in tree.body if isinstance(item, (ast.FunctionDef, ast.ClassDef)) and item.name in wanted], type_ignores=[])
        self.api = SimpleNamespace(STATE_SENSITIVE="sensitive", STATE_ENABLED="enabled", STATE_PRESSED="pressed", STATE_CHECKED="checked", STATE_SELECTED="selected", ROLE_LIST_ITEM="card", ROLE_TOGGLE_BUTTON="toggle")
        self.namespace = {"CONTROL": self.root, "ROOT": self.root, "DESKTOP": "kde", "json": json,
                          "time": SimpleNamespace(monotonic=lambda: 0), "pyatspi": self.api,
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
        self.namespace["pointer_click"].assert_called_once_with(node)
        states.add("checked")
        self.assertEqual(select(node, "accessible"), (True, False))
        self.namespace["action"].assert_called_once_with(node)

    def test_sensitive_without_enabled_is_not_ready_for_input(self):
        self.assertFalse(self.namespace["sensitive"](self.node({"sensitive"})))
        self.assertTrue(self.namespace["sensitive"](self.node({"sensitive", "enabled"})))

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
        node = self.node({"sensitive", "enabled", "pressed"})
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


if __name__ == "__main__":
    unittest.main()
