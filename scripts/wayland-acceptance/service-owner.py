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


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(1)
    data = sys.stdin.buffer.read(1025)
    try:
        result = data.decode("ascii") if len(data) <= 1024 else ""
    except UnicodeDecodeError:
        result = ""
    raise SystemExit(0 if owner_matches(result, sys.argv[1]) else 1)
