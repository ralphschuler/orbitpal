#!/usr/bin/env bash
# Called only inside session.sh's validated private real desktop, never directly.
set -euo pipefail
[[ "${ORBITPAL_ACCEPTANCE_MODE:-}" == app && "${ORBITPAL_WAYLAND_ACCEPTANCE:-}" == synthetic-desktop-only && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux && "${GITHUB_ACTIONS:-}" == true ]]
: "${ORBITPAL_ACCEPTANCE_ROOT:?}" "${XDG_DATA_HOME:?}"
root="$ORBITPAL_ACCEPTANCE_ROOT"
[[ "$root" =~ ^/tmp/orbitpal-wayland\.[A-Za-z0-9]{8}$ ]]
[[ $(id -u) -ne 0 && -O "$root" && ! -L "$root" && "$XDG_DATA_HOME" == "$root/data" && -f "$root/acceptance-app" && -x "$root/acceptance-app" && ! -L "$root/acceptance-app" ]]
private_bus="unix:path=$root/runtime/session-bus"
session_bus="${DBUS_SESSION_BUS_ADDRESS:-}"
bus_suffix="${session_bus#"$private_bus"}"
[[ "$session_bus" == "$private_bus" || "$session_bus" == "$private_bus,guid="* && "$bus_suffix" =~ ^,guid=[0-9a-f]{32}$ ]]
[[ "${XDG_RUNTIME_DIR:-}" == "$root/runtime" && "${WAYLAND_DISPLAY:-}" == orbitpal-acceptance && -S "$root/runtime/orbitpal-acceptance" && -z "${DISPLAY:-}" ]]
umask 077
export NO_PROXY=127.0.0.1,localhost
# No copied installation, profiles, keys or capture permission. Only stop the
# default optional GitHub discovery before this fresh installation first starts.
/usr/bin/python3 - <<'PY'
import os
from pathlib import Path
import sqlite3
import sys
try:
    root = Path(os.environ['XDG_DATA_HOME']) / 'studio.streamer-ai.cohost'
    root.mkdir(mode=0o700)
    with sqlite3.connect(root / 'cohost.sqlite3') as db:
        db.execute('CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL)')
        db.execute("INSERT INTO settings VALUES('automatic_update_checks','false')")
except Exception:
    print('app_bootstrap_failed', file=sys.stderr)
    raise SystemExit(1)
PY
"$root/acceptance-app" >/dev/null 2>&1 &
app_pid=$!
printf '%s\n' "$app_pid" >"$root/control/app-pid"
cleanup() {
  trap - EXIT
  kill -TERM "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' INT TERM
/usr/bin/python3 "$root/harness/app.py" >"$root/app-controller.txt" 2>&1
test -f "$root/control/app-passed"
grep -Fxq 'real-app-initial-subset-complete' "$root/control/app-passed"
