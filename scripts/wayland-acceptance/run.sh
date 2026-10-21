#!/usr/bin/env bash
# Provision only a disposable GitHub-hosted Linux VM; never a developer desktop.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux ]]
[[ $(uname -s) == Linux && ( $# == 2 || $# == 3 ) && ( "$1" == gnome || "$1" == kde ) && -f "$2" && -x "$2" ]]
mode=prototype
if [[ $# == 3 ]]; then
  [[ -f "$3" && -x "$3" && ! -L "$3" ]]
  mode=app
fi
: "${RUNNER_TEMP:?}" "${GITHUB_RUN_ID:?}" "${GITHUB_RUN_ATTEMPT:?}"
umask 077
desktop="$1"
root=$(mktemp -d /tmp/orbitpal-wayland.XXXXXXXX)
account="opw${GITHUB_RUN_ID: -9}${GITHUB_RUN_ATTEMPT}"
[[ "$account" =~ ^opw[0-9]+$ ]]
if id "$account" >/dev/null 2>&1; then printf 'Refusing to reuse an existing account\n' >&2; exit 1; fi
# The privileged observer stays in the reviewed checkout, never in the copied
# harness that is writable by the isolated account.
pam_helper="$(pwd -P)/scripts/wayland-acceptance/pam-session.py"
[[ -f "$pam_helper" && ! -L "$pam_helper" ]]
pam_unit="orbitpal-${root##*/}-$account.service"
private_uid=""
pam_read() {
  sudo /usr/bin/timeout --signal=KILL 8s /usr/bin/env GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux \
    /usr/bin/python3 "$pam_helper" "$@"
}
cleanup() {
  result=$?
  trap - EXIT
  safe_to_remove=yes
  # A client exit/timeout does not establish transient service/session cleanup.
  if [[ "${pam_attempted:-}" == yes ]]; then
    unit_status=0
    pam_read unit "$root" "$account" "$private_uid" "$pam_unit" >/dev/null 2>&1 || unit_status=$?
    if ((unit_status==0)); then
      if ! sudo /usr/bin/timeout --signal=KILL 6s /usr/bin/systemctl stop -- "$pam_unit" >/dev/null 2>&1; then safe_to_remove=no; result=1; fi
    elif ((unit_status!=3)); then safe_to_remove=no; result=1
    fi
  fi
  # Only the fresh account created by this invocation can own these processes.
  if [[ "${created:-}" == yes ]]; then
    if [[ "$mode" == app && "${pam_identity_clear:-}" != yes ]] || ! [[ "$private_uid" =~ ^[1-9][0-9]{2,4}$ && "$(id -u "$account" 2>/dev/null)" == "$private_uid" ]]; then
      safe_to_remove=no; result=1
    else
      sudo pkill -TERM -u "$account" 2>/dev/null || true
      sleep 1
      sudo pkill -KILL -u "$account" 2>/dev/null || true
      if [[ "${pam_attempted:-}" == yes ]]; then
        residue_clear=no
        residue_deadline=$((SECONDS+15))
        while ((SECONDS<residue_deadline)); do
          if pam_read residue "$root" "$account" "$private_uid" "$pam_unit" >/dev/null 2>&1 && ((SECONDS<=residue_deadline)); then residue_clear=yes; break; fi
          sleep .2
        done
        if [[ "$residue_clear" != yes ]]; then safe_to_remove=no; result=1; fi
      fi
      if [[ "$safe_to_remove" == yes ]]; then
        if ! sudo userdel "$account"; then printf 'Failed to remove the isolated test account\n' >&2; result=1; safe_to_remove=no; fi
      fi
    fi
  fi
  diagnostic="$RUNNER_TEMP/wayland-acceptance-$desktop"
  if [[ "$mode" == app ]]; then diagnostic="$RUNNER_TEMP/wayland-app-$desktop"; fi
  mkdir -p "$diagnostic"
  # Exact text-only files; never recursively collect a home, screenshot or frame.
  for name in compositor pipewire wireplumber portal portal-backend session desktop chooser-tree portal-interfaces kde-cache versions vgem render-capability compositor-capability; do
    if sudo test -f "$root/$name.txt" && ! sudo test -L "$root/$name.txt"; then sudo head -c 65536 "$root/$name.txt" >"$diagnostic/$name.txt"; fi
  done
  if [[ "$mode" == app ]]; then
    for name in app-controller app-result app-binary app-exit app-runtime; do
      if sudo test -f "$root/$name.txt" && ! sudo test -L "$root/$name.txt"; then sudo head -c 65536 "$root/$name.txt" >"$diagnostic/$name.txt"; fi
    done
    for name in pam-session-capability pam-session-cleanup; do
      if sudo test -f "$root/$name.txt" && ! sudo test -L "$root/$name.txt"; then sudo head -c 8192 "$root/$name.txt" >"$diagnostic/$name.txt"; fi
    done
    # This additional observation is a closed numeric record, capped separately.
    if sudo test -f "$root/app-session-availability.txt" && ! sudo test -L "$root/app-session-availability.txt"; then
      sudo head -c 2048 "$root/app-session-availability.txt" >"$diagnostic/app-session-availability.txt"
    fi
  fi
  if [[ "$safe_to_remove" == yes ]]; then
    if ! sudo rm -rf -- "$root"; then printf 'Failed to remove the private test root\n' >&2; result=1; fi
  else
    printf 'Private PAM/session cleanup unconfirmed; preserving isolated resources for VM disposal\n' >&2
  fi
  # Only unload the software device this invocation itself loaded. No existing
  # Hyper-V/physical device permissions, module or host desktop are changed.
  if [[ "${vgem_created:-}" == yes && "$safe_to_remove" == yes ]]; then
    if ! sudo modprobe -r vgem; then printf 'Failed to unload the isolated vgem device\n' >&2; result=1; fi
  fi
  printf '%s\n' "$result" >"$diagnostic/exit-code.txt"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
# Only actual-app mode changes launch route; the portal baseline stays intact.
if [[ "$mode" == app ]]; then
  pam_read capability >"$root/pam-session-capability.txt" 2>/dev/null
fi
sudo useradd --no-user-group --create-home --home-dir "$root/home" --shell /bin/bash "$account"
created=yes
private_uid="$(id -u "$account")"
[[ "$private_uid" =~ ^[1-9][0-9]{2,4}$ && "$private_uid" -ge 1000 ]]
sudo install -d -m 0700 "$root/runtime" "$root/control" "$root/config" "$root/cache" "$root/data"
sudo cp -R scripts/wayland-acceptance "$root/harness"
sudo cp "$2" "$root/acceptance-test"
sudo chmod 0700 "$root/acceptance-test"
if [[ "$mode" == app ]]; then
  sudo cp "$3" "$root/acceptance-app"
  sudo chmod 0700 "$root/acceptance-app"
  sha256sum "$3" | cut -d ' ' -f 1 | sudo tee "$root/app-binary.txt" >/dev/null
fi
probe="$RUNNER_TEMP/orbitpal-wayland-render-probe"
[[ -f "$probe" && -x "$probe" && ! -L "$probe" ]]
sudo cp "$probe" "$root/render-probe"
sudo chmod 0700 "$root/render-probe"
# KWin 6's real virtual EGL backend requires an actual DRM/GBM device. A vgem
# primary node is explicitly supported upstream. Never reuse/chown another GPU.
[[ ! -d /sys/module/vgem ]]
sudo modprobe vgem
vgem_created=yes
for _ in {1..100}; do
  devices=()
  for entry in /sys/class/drm/card[0-9]*; do
    [[ "${entry##*/}" =~ ^card[0-9]+$ ]] || continue
    # Linux7 vgem uses faux_device_create(..., NULL): there is deliberately no
    # bound device/driver/module symlink. Match its actual faux bus/name instead;
    # the native probe separately requires DRM_IOCTL_VERSION's exact vgem name.
    if [[ $(basename "$(readlink -f "$entry/device")") == vgem && $(basename "$(readlink -f "$entry/device/subsystem")") == faux ]]; then devices+=("/dev/dri/${entry##*/}"); fi
  done
  if [[ ${#devices[@]} == 1 && -c "${devices[0]}" ]]; then break; fi
  sleep .1
done
[[ ${#devices[@]} == 1 && -c "${devices[0]}" && ! -L "${devices[0]}" ]]
device="${devices[0]}"
# Device-node creation can precede udev's owner/mode rules. Wait for the actual
# add event before granting this one new node; never trigger/rewrite other GPUs.
# https://www.freedesktop.org/software/systemd/man/latest/udevadm.html
sudo udevadm info --wait-for-initialization=10 --query=path "$device" >/dev/null
sudo udevadm settle --timeout=10
sudo chown "$account" "$device"
sudo chmod 0600 "$device"
{
  printf 'genuine isolated software DRM device: %s\nmodule: vgem\n' "$device"
  sudo stat --format='owner uid: %u; group gid: %g; mode: %a; device: %t:%T' "$device"
} | sudo tee "$root/vgem.txt" >/dev/null
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
X-KDE-Wayland-Interfaces=org_kde_kwin_fake_input
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
  uname -r
  dpkg-query -W gnome-shell mutter-common kwin-wayland plasma-workspace qt6-wayland xdg-desktop-portal xdg-desktop-portal-gnome xdg-desktop-portal-kde pipewire wireplumber libdrm2 libegl-mesa0 libgbm1 2>/dev/null || true
} | sudo tee "$root/versions.txt" >/dev/null
sudo chown -R "$account" "$root"
if [[ "$mode" == app ]]; then
  # A newly allocated numeric UID must not inherit old process/login1/runtime
  # residue. If this read cannot prove freshness, do not signal that UID later.
  pam_read fresh "$root" "$account" "$private_uid" "$pam_unit" >/dev/null 2>&1
  pam_identity_clear=yes
  pam_attempted=yes
  # One original240s wall deadline includes PAM startup. The service reserves
  # at most15s for startup and225s for work, never a fresh240s after PAM opens.
  /usr/bin/timeout --signal=TERM --kill-after=10s 240s sudo /usr/bin/systemd-run \
    --system --quiet --wait --pipe --collect --no-ask-password --expand-environment=no \
    --unit="$pam_unit" --uid="$account" --description="OrbitPal private PAM ${root##*/}" \
    --property=Type=exec --property=PAMName=login --property="WorkingDirectory=$root" \
    --property=TimeoutStartSec=15s --property=RuntimeMaxSec=225s --property=TimeoutStopSec=5s \
    --property=KillMode=control-group --property=Restart=no \
    /usr/bin/env -i PATH=/usr/bin:/bin HOME="$root/home" USER="$account" LOGNAME="$account" \
    GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux \
    ORBITPAL_WAYLAND_ACCEPTANCE=synthetic-desktop-only ORBITPAL_ACCEPTANCE_ROOT="$root" ORBITPAL_ACCEPTANCE_DESKTOP="$desktop" ORBITPAL_ACCEPTANCE_DRM="$device" ORBITPAL_ACCEPTANCE_MODE="$mode" \
    bash -c 'exec dbus-run-session --config-file="$ORBITPAL_ACCEPTANCE_ROOT/bus.conf" -- bash "$ORBITPAL_ACCEPTANCE_ROOT/harness/session.sh" >"$ORBITPAL_ACCEPTANCE_ROOT/session.txt" 2>&1' </dev/null >/dev/null 2>&1
else
sudo -H -u "$account" env -i PATH=/usr/bin:/bin HOME="$root/home" USER="$account" LOGNAME="$account" \
  GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux \
  ORBITPAL_WAYLAND_ACCEPTANCE=synthetic-desktop-only ORBITPAL_ACCEPTANCE_ROOT="$root" ORBITPAL_ACCEPTANCE_DESKTOP="$desktop" ORBITPAL_ACCEPTANCE_DRM="$device" ORBITPAL_ACCEPTANCE_MODE="$mode" \
  bash -c 'exec timeout --signal=TERM --kill-after=10s 240s dbus-run-session --config-file="$ORBITPAL_ACCEPTANCE_ROOT/bus.conf" -- bash "$ORBITPAL_ACCEPTANCE_ROOT/harness/session.sh" >"$ORBITPAL_ACCEPTANCE_ROOT/session.txt" 2>&1'
fi
