#!/usr/bin/python3
"""Check public run metadata only; never dispatches, downloads or invokes an app."""
import json
import re
import sys

REPOSITORY = "ralphschuler/orbitpal"
SMOKE_PATH = ".github/workflows/native-render-benchmark-smoke.yml"
MAX_METADATA = 256 * 1024


def qualified_smoke(value, reviewed_sha, run_id):
    if (type(reviewed_sha) is not str or re.fullmatch(r"[0-9a-f]{40}", reviewed_sha) is None
            or type(run_id) is not str or re.fullmatch(r"[1-9][0-9]{0,19}", run_id) is None):
        return False
    if type(value) is not dict:
        return False
    repository = value.get("repository")
    head_repository = value.get("head_repository")
    return (type(value.get("id")) is int and value["id"] == int(run_id)
            and value.get("event") == "workflow_dispatch" and value.get("status") == "completed"
            and value.get("conclusion") == "success" and value.get("head_sha") == reviewed_sha
            and value.get("path") == SMOKE_PATH
            and type(repository) is dict and repository.get("full_name") == REPOSITORY
            and type(head_repository) is dict and head_repository.get("full_name") == REPOSITORY)


def main():
    if len(sys.argv) != 3:
        raise ValueError()
    raw = sys.stdin.buffer.read(MAX_METADATA + 1)
    if len(raw) > MAX_METADATA or not qualified_smoke(json.loads(raw), sys.argv[1], sys.argv[2]):
        raise ValueError()
    print("Exact-source manual renderer smoke prerequisite passed; no performance acceptance.")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        raise SystemExit("Renderer smoke prerequisite unavailable.") from None
