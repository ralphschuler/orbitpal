#!/usr/bin/env python3
"""Match a real private D-Bus service owner, not mere name availability.

Pure parser only: reads a bounded gdbus result and never calls D-Bus or a GUI.
"""
import re
import sys


def owner_matches(result, expected):
    if not re.fullmatch(r"[1-9][0-9]{0,9}", expected):
        return False
    expected_pid = int(expected)
    match = re.fullmatch(r"\(uint32 ([1-9][0-9]{0,9}),\)\n?", result)
    return bool(match and 0 < expected_pid < 2**31 and int(match.group(1)) == expected_pid)


def owner_diagnostic(result, expected):
    valid_expected = bool(re.fullmatch(r"[1-9][0-9]{0,9}", expected)) and int(expected) < 2**31
    match = re.fullmatch(r"\(uint32 ([1-9][0-9]{0,9}),\)\n?", result)
    actual = match.group(1) if match and int(match.group(1)) < 2**31 else "unavailable"
    return f"portal backend owner: expected={expected if valid_expected else 'invalid'}; actual={actual}; matches={str(owner_matches(result, expected)).lower()}"


if __name__ == "__main__":
    diagnostic = len(sys.argv) == 3 and sys.argv[1] == "--diagnostic"
    if len(sys.argv) != 2 and not diagnostic:
        raise SystemExit(1)
    data = sys.stdin.buffer.read(1025)
    try:
        result = data.decode("ascii") if len(data) <= 1024 else ""
    except UnicodeDecodeError:
        result = ""
    if diagnostic:
        print(owner_diagnostic(result, sys.argv[2]))
        raise SystemExit(0)
    raise SystemExit(0 if owner_matches(result, sys.argv[1]) else 1)
