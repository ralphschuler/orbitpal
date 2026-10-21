#!/usr/bin/env bash
# Exact fixed commands, after trusted launch.py's cgroup/netns/UID transition.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux ]]
[[ "${ORBITPAL_NATIVE_RENDER_BENCH:-}" == disposable-linux-only && $(id -u) -gt 1000 ]]
: "${ORBITPAL_BENCH_ROOT:?}" "${ORBITPAL_BENCH_DOMAIN:?}" "${ORBITPAL_BENCH_MODE:?}"
root="$ORBITPAL_BENCH_ROOT"
[[ "$root" =~ ^/tmp/orbitpal-render-[0-9a-f]{16}$ && -O "$root" && ! -L "$root" ]]
[[ "$ORBITPAL_BENCH_MODE" == smoke || "$ORBITPAL_BENCH_MODE" == pilot ]]
[[ -z "${DISPLAY+x}" && -z "${TAURI_CONFIG+x}" ]]
umask 077
bus_pid= workload_pid=
cleanup() {
  result=$?
  trap - EXIT
  [[ -z "$workload_pid" ]] || kill -TERM "$workload_pid" 2>/dev/null || true
  [[ -z "$bus_pid" ]] || kill -TERM "$bus_pid" 2>/dev/null || true
  # Root runner owns the absolute watchdog and bounded cgroup cleanup even if
  # a helper cannot complete this ordinary unprivileged wait.
  wait || true
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
if [[ "$ORBITPAL_BENCH_DOMAIN" == compositor ]]; then
  home="$root/compositor"
  [[ "$DBUS_SESSION_BUS_ADDRESS" == "unix:path=$home/runtime/session-bus" ]]
  [[ "$XDG_RUNTIME_DIR" == "$home/runtime" && "$HOME" == "$home/home" ]]
  dbus-daemon --nofork --config-file="$root/compositor-bus.conf" >/dev/null 2>&1 &
  bus_pid=$!
  for ((attempt=0; attempt<150; attempt++)); do
    [[ -S "$home/runtime/session-bus" ]] && break
    kill -0 "$bus_pid" 2>/dev/null || exit 1
    sleep .1
  done
  [[ -S "$home/runtime/session-bus" ]]
  # Same real headless GNOME path as the separate Wayland acceptance baseline.
  # No host display/DRM permission, portal, capture or test grant is used.
  gnome-shell --wayland --headless --virtual-monitor=1280x960 \
    --wayland-display=orbitpal-benchmark --no-x11 >/dev/null 2>&1 &
  workload_pid=$!
  workload_status=0
  wait "$workload_pid" || workload_status=$?
  workload_pid=
  exit "$workload_status"
elif [[ "$ORBITPAL_BENCH_DOMAIN" == app ]]; then
  [[ "$DBUS_SESSION_BUS_ADDRESS" == "unix:path=$root/runtime/session-bus" ]]
  [[ "$XDG_RUNTIME_DIR" == "$root/runtime" && "$HOME" == "$root/home" ]]
  [[ "$GDK_BACKEND" == wayland && "$WAYLAND_DISPLAY" == orbitpal-benchmark && -S "$root/runtime/orbitpal-benchmark" ]]
  dbus-daemon --nofork --config-file="$root/app-bus.conf" >/dev/null 2>&1 &
  bus_pid=$!
  for ((attempt=0; attempt<150; attempt++)); do
    [[ -S "$root/runtime/session-bus" ]] && break
    kill -0 "$bus_pid" 2>/dev/null || exit 1
    sleep .1
  done
  [[ -S "$root/runtime/session-bus" && -x "$root/native-bench" && ! -L "$root/native-bench" ]]
  "$root/native-bench" --mode "$ORBITPAL_BENCH_MODE" >/dev/null 2>&1 &
  workload_pid=$!
  workload_status=0
  wait "$workload_pid" || workload_status=$?
  workload_pid=
  exit "$workload_status"
else
  exit 1
fi
