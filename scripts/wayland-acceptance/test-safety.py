#!/usr/bin/env python3
"""Pure local fixtures: no compositor, GUI API, VM, sudo or capture is invoked."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

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


if __name__ == "__main__":
    unittest.main()
