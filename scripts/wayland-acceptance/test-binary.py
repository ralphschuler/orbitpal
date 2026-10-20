#!/usr/bin/env python3
"""Find exactly the native library test executable in Cargo's JSON output."""
import json
import os
import sys

executables = set()
for line in sys.stdin:
    event = json.loads(line)
    if (event.get("reason") == "compiler-artifact"
            and event.get("target", {}).get("name") == "orbitpal_lib"
            and event.get("profile", {}).get("test")
            and event.get("executable")):
        executables.add(event["executable"])
if len(executables) != 1:
    raise SystemExit("Expected exactly one compiled OrbitPal library test executable")
executable = executables.pop()
if not os.path.isabs(executable) or not os.path.isfile(executable) or not os.access(executable, os.X_OK):
    raise SystemExit("Cargo did not provide an executable regular test file")
print(executable)
