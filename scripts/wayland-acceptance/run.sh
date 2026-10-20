#!/usr/bin/env bash
# Provision only a disposable GitHub-hosted Linux VM; never a developer desktop.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux ]]
[[ $(uname -s) == Linux && $# == 2 && ( "$1" == gnome || "$1" == kde ) && -f "$2" && -x "$2" ]]
: "${RUNNER_TEMP:?}" "${GITHUB_RUN_ID:?}" "${GITHUB_RUN_ATTEMPT:?}"
umask 077
desktop="$1"
root=$(mktemp -d /tmp/orbitpal-wayland.XXXXXXXX)
account="opw${GITHUB_RUN_ID: -9}${GITHUB_RUN_ATTEMPT}"
[[ "$account" =~ ^opw[0-9]+$ ]]
if id "$account" >/dev/null 2>&1; then printf 'Refusing to reuse an existing account\n' >&2; exit 1; fi
cleanup() {
  result=$?
  trap - EXIT
  # Only the new account created by this invocation can own these processes.
  if [[ "${created:-}" == yes ]]; then
    sudo pkill -TERM -u "$account" 2>/dev/null || true
    sleep 1
    sudo pkill -KILL -u "$account" 2>/dev/null || true
    if ! sudo userdel "$account"; then printf 'Failed to remove the isolated test account\n' >&2; result=1; fi
  fi
  diagnostic="$RUNNER_TEMP/wayland-acceptance-$desktop"
  mkdir -p "$diagnostic"
  # Exact text-only files; never recursively collect a home, screenshot or frame.
  for name in compositor pipewire wireplumber portal portal-backend session desktop chooser-tree portal-interfaces kde-cache versions; do
    if sudo test -f "$root/$name.txt"; then sudo head -c 65536 "$root/$name.txt" >"$diagnostic/$name.txt"; fi
  done
  printf '%s\n' "$result" >"$diagnostic/exit-code.txt"
  if ! sudo rm -rf -- "$root"; then printf 'Failed to remove the private test root\n' >&2; result=1; fi
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
sudo useradd --no-user-group --create-home --home-dir "$root/home" --shell /bin/bash "$account"
created=yes
sudo install -d -m 0700 "$root/runtime" "$root/control" "$root/config" "$root/cache" "$root/data"
sudo cp -R scripts/wayland-acceptance "$root/harness"
sudo cp "$2" "$root/acceptance-test"
sudo chmod 0700 "$root/acceptance-test"
if [[ "$desktop" == kde ]]; then
  pointer="$RUNNER_TEMP/orbitpal-wayland-pointer"
  [[ -f "$pointer" && -x "$pointer" && ! -L "$pointer" ]]
  sudo cp "$pointer" "$root/pointer"
  sudo chmod 0700 "$root/pointer"
  sudo install -d -m 0700 "$root/data/applications"
  sudo tee "$root/data/applications/orbitpal-acceptance-pointer.desktop" >/dev/null <<EOF
[Desktop Entry]
Type=Application
Name=OrbitPal synthetic chooser input
Exec="$root/pointer"
NoDisplay=true
X-KDE-Wayland-Interfaces=org_kde_kwin_fake_input;
EOF
fi
sudo tee "$root/bus.conf" >/dev/null <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path=$root/runtime/session-bus</listen><auth>EXTERNAL</auth><standard_session_servicedirs/><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>
EOF
{
  printf 'source-commit: %s\ndesktop: %s\n' "$(git rev-parse HEAD)" "$desktop"
  cat /etc/os-release
  # A single matrix job deliberately lacks the other desktop's packages.
  dpkg-query -W gnome-shell mutter-common kwin-wayland plasma-workspace xdg-desktop-portal xdg-desktop-portal-gnome xdg-desktop-portal-kde pipewire wireplumber 2>/dev/null || true
} | sudo tee "$root/versions.txt" >/dev/null
sudo chown -R "$account" "$root"
sudo -H -u "$account" env -i PATH=/usr/bin:/bin HOME="$root/home" USER="$account" LOGNAME="$account" \
  GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux \
  ORBITPAL_WAYLAND_ACCEPTANCE=synthetic-desktop-only ORBITPAL_ACCEPTANCE_ROOT="$root" ORBITPAL_ACCEPTANCE_DESKTOP="$desktop" \
  bash -c 'exec timeout --signal=TERM --kill-after=10s 240s dbus-run-session --config-file="$ORBITPAL_ACCEPTANCE_ROOT/bus.conf" -- bash "$ORBITPAL_ACCEPTANCE_ROOT/harness/session.sh" >"$ORBITPAL_ACCEPTANCE_ROOT/session.txt" 2>&1'
