#!/usr/bin/env bash
# Exact fixed commands, after trusted launch.py's cgroup/netns/UID transition.
set -euo pipefail
# Dynamic FD-close redirections require the hosted Linux Bash, never macOS3.2.
[[ "${BASH_VERSINFO[0]}" -ge 4 ]]
diagnostic_fd="${1:-}"
[[ $# -le 1 ]]
if [[ -n "$diagnostic_fd" ]]; then
  [[ "${ORBITPAL_BENCH_DOMAIN:-}" == compositor && "$diagnostic_fd" =~ ^[1-9][0-9]{0,9}$ && "$diagnostic_fd" -ge 3 ]]
fi
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux ]]
if [[ -n "$diagnostic_fd" ]]; then
  # The anonymous channel is never inherited by unrelated fixed helpers.
  [[ "${ORBITPAL_NATIVE_RENDER_BENCH:-}" == disposable-linux-only && $(id -u {diagnostic_fd}>&-) -gt 1000 ]]
else
  [[ "${ORBITPAL_NATIVE_RENDER_BENCH:-}" == disposable-linux-only && $(id -u) -gt 1000 ]]
fi
: "${ORBITPAL_BENCH_ROOT:?}" "${ORBITPAL_BENCH_DOMAIN:?}" "${ORBITPAL_BENCH_MODE:?}"
root="$ORBITPAL_BENCH_ROOT"
[[ "$root" =~ ^/tmp/orbitpal-render-[0-9a-f]{16}$ && -O "$root" && ! -L "$root" ]]
[[ "$ORBITPAL_BENCH_MODE" == smoke || "$ORBITPAL_BENCH_MODE" == pilot || "$ORBITPAL_BENCH_MODE" == pilot3 || "$ORBITPAL_BENCH_MODE" == pilot5 ]]
[[ -z "${DISPLAY+x}" && -z "${TAURI_CONFIG+x}" ]]
umask 077
bus_pid= system_bus_pid= workload_pid=
cleanup() {
  result=$?
  trap - EXIT
  [[ -z "$workload_pid" ]] || kill -TERM "$workload_pid" 2>/dev/null || true
  [[ -z "$bus_pid" ]] || kill -TERM "$bus_pid" 2>/dev/null || true
  [[ -z "$system_bus_pid" ]] || kill -TERM "$system_bus_pid" 2>/dev/null || true
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
  if [[ -n "$diagnostic_fd" ]]; then
    dbus-daemon --nofork --config-file="$root/compositor-bus.conf" >/dev/null 2>&1 {diagnostic_fd}>&- &
  else
    dbus-daemon --nofork --config-file="$root/compositor-bus.conf" >/dev/null 2>&1 &
  fi
  bus_pid=$!
  for ((attempt=0; attempt<150; attempt++)); do
    [[ -S "$home/runtime/session-bus" ]] && break
    kill -0 "$bus_pid" 2>/dev/null || exit 1
    if [[ -n "$diagnostic_fd" ]]; then sleep .1 {diagnostic_fd}>&-; else sleep .1; fi
  done
  [[ -S "$home/runtime/session-bus" ]]
  [[ "$DBUS_SYSTEM_BUS_ADDRESS" == "unix:path=$home/runtime/system-bus" ]]
  # A real empty private bus satisfies proxy transport construction only. No
  # logind/session service is mocked, activated or reached on the host.
  if [[ -n "$diagnostic_fd" ]]; then
    dbus-daemon --nofork --config-file="$root/compositor-system-bus.conf" >/dev/null 2>&1 {diagnostic_fd}>&- &
  else
    dbus-daemon --nofork --config-file="$root/compositor-system-bus.conf" >/dev/null 2>&1 &
  fi
  system_bus_pid=$!
  for ((attempt=0; attempt<150; attempt++)); do
    [[ -S "$home/runtime/system-bus" ]] && break
    kill -0 "$system_bus_pid" 2>/dev/null || exit 1
    if [[ -n "$diagnostic_fd" ]]; then sleep .1 {diagnostic_fd}>&-; else sleep .1; fi
  done
  [[ -S "$home/runtime/system-bus" ]]
  # Same real headless GNOME path as the separate Wayland acceptance baseline.
  # No host display/DRM permission, portal, capture or test grant is used.
  if [[ -n "$diagnostic_fd" ]]; then
    gnome-shell --wayland --headless --virtual-monitor=1280x960 \
      --wayland-display=orbitpal-benchmark --no-x11 >/dev/null 2>&"$diagnostic_fd" {diagnostic_fd}>&- &
  else
    gnome-shell --wayland --headless --virtual-monitor=1280x960 \
      --wayland-display=orbitpal-benchmark --no-x11 >/dev/null 2>&1 &
  fi
  workload_pid=$!
  if [[ -n "$diagnostic_fd" ]]; then exec {diagnostic_fd}>&-; diagnostic_fd=; fi
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
