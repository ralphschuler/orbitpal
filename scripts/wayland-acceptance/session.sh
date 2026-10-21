#!/usr/bin/env bash
set -euo pipefail

: "${ORBITPAL_ACCEPTANCE_ROOT:?}" "${ORBITPAL_ACCEPTANCE_DESKTOP:?}"
[[ "${ORBITPAL_WAYLAND_ACCEPTANCE:-}" == synthetic-desktop-only && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux && "${GITHUB_ACTIONS:-}" == true ]]
[[ $(id -u) -ne 0 && -O "$ORBITPAL_ACCEPTANCE_ROOT" && ! -L "$ORBITPAL_ACCEPTANCE_ROOT" ]]
umask 077
root="$ORBITPAL_ACCEPTANCE_ROOT"
# In particular, --replace below must never target an inherited host bus.
private_bus="unix:path=$root/runtime/session-bus"
session_bus="${DBUS_SESSION_BUS_ADDRESS:-}"
bus_suffix="${session_bus#"$private_bus"}"
if ! [[ "$session_bus" == "$private_bus" || "$session_bus" == "$private_bus,guid="* && "$bus_suffix" =~ ^,guid=[0-9a-f]{32}$ ]]; then
  printf 'Refusing a session bus outside the private acceptance root\n' >&2
  exit 1
fi
export XDG_RUNTIME_DIR="$root/runtime" XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache"
export XDG_SESSION_TYPE=wayland LANG=C.UTF-8 LC_ALL=C.UTF-8
export LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe GTK_A11Y=atspi QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1
unset DISPLAY WAYLAND_DISPLAY WAYLAND_SOCKET PULSE_SERVER PIPEWIRE_REMOTE AT_SPI_BUS_ADDRESS DBUS_STARTER_ADDRESS DBUS_STARTER_BUS_TYPE XAUTHORITY
pids=()
cleanup() {
  trap - EXIT
  if (( ${#pids[@]} )); then
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  fi
  wait || true
}
trap cleanup EXIT
trap 'exit 130' INT TERM
start() { local name="$1"; shift; "$@" >"$root/$name.txt" 2>&1 & pids+=("$!"); }
wait_for() { local label="$1"; shift; for _ in {1..150}; do if "$@"; then return; fi; sleep .2; done; printf 'Timeout: %s\n' "$label" >&2; return 1; }
dbus_name() { gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner "$1" | grep -q true; }
dbus_owner() {
  gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.GetConnectionUnixProcessID "$1" \
    | /usr/bin/python3 "$root/harness/service-owner.py" "$2"
}
backend_binary() { local package="$1" name="$2" file; while IFS= read -r file; do if [[ "${file##*/}" == "$name" && -x "$file" && ! -d "$file" ]]; then printf '%s\n' "$file"; return; fi; done < <(dpkg-query -L "$package"); return 1; }
start_portal_backend() {
  [[ $# == 2 ]] || return 1
  if [[ "$1" == gnome ]]; then
    # GNOME Shell can auto-activate a backend before our display environment.
    # The real GNOME 50.0 backend supports replacing that instance. The private
    # bus guard above and exact spawned-PID gate below remain mandatory.
    # https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/blob/50.0/src/xdg-desktop-portal-gnome.c
    start portal-backend "$2" --replace
  elif [[ "$1" == kde ]]; then
    start portal-backend "$2"
  else
    return 1
  fi
}
prepare_kde_environment() {
  # Plasma normally supplies this before building KService's application cache.
  # Without it the packaged plasma-applications.menu is never loaded, so KWin
  # cannot resolve the real portal executable's restricted-interface declaration.
  [[ $# == 1 && -f "$1" && -r "$1" && ! -L "$1" ]] || return 1
  export XDG_MENU_PREFIX=plasma-
  # The entry guard above already established our exact private session bus.
  # No --systemd/--all: publish only this menu choice on that private bus.
  dbus-update-activation-environment XDG_MENU_PREFIX
}

"$root/render-probe" "${ORBITPAL_ACCEPTANCE_DRM:?}" >"$root/render-capability.txt" 2>&1
grep -Fxq 'real-gbm-egl-render-complete' "$root/render-capability.txt"
mkdir -p "$XDG_CONFIG_HOME/xdg-desktop-portal" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
gsettings set org.gnome.desktop.interface toolkit-accessibility true
start pipewire pipewire
wait_for 'private PipeWire socket' test -S "$XDG_RUNTIME_DIR/pipewire-0"
start wireplumber wireplumber
if [[ "$ORBITPAL_ACCEPTANCE_DESKTOP" == gnome ]]; then
  export XDG_CURRENT_DESKTOP=GNOME
  monitor=1280x960
  if [[ "${ORBITPAL_ACCEPTANCE_MODE:-prototype}" == app ]]; then monitor=1920x1200; fi
  start compositor gnome-shell --wayland --headless --virtual-monitor="$monitor" --wayland-display=orbitpal-acceptance --no-x11
  export ORBITPAL_ACCEPTANCE_COMPOSITOR_PID="${pids[-1]}"
  wait_for 'GNOME compositor' dbus_name org.gnome.Mutter.ScreenCast
  backend_package=xdg-desktop-portal-gnome
else
  [[ "$ORBITPAL_ACCEPTANCE_DESKTOP" == kde ]]
  export XDG_CURRENT_DESKTOP=KDE KDE_FULL_SESSION=true KDE_SESSION_VERSION=6 KWIN_COMPOSE=O2
  prepare_kde_environment /etc/xdg/menus/plasma-applications.menu
  kbuildsycoca6 --noincremental >"$root/kde-cache.txt" 2>&1
  # Exact categories only, retained through the existing bounded text collector.
  # Keep KWin's normal executable/interface permission filter enabled.
  width=1280; height=960
  if [[ "${ORBITPAL_ACCEPTANCE_MODE:-prototype}" == app ]]; then width=1920; height=1200; fi
  start compositor env QT_LOGGING_RULES='kwin_core.debug=true;KWIN_UTILS.debug=true' kwin_wayland --virtual --width "$width" --height "$height" --output-count 1 --socket orbitpal-acceptance --no-lockscreen --no-global-shortcuts
  export ORBITPAL_ACCEPTANCE_COMPOSITOR_PID="${pids[-1]}"
  wait_for 'KWin compositor' dbus_name org.kde.KWin
  # The exact real compositor must use OpenGL. QPainter is insufficient for its
  # output screencast implementation; never count that fallback as acceptance.
  gdbus call --session --dest org.kde.KWin --object-path /Compositor --method org.freedesktop.DBus.Properties.Get org.kde.kwin.Compositing compositingType >"$root/compositor-capability.txt"
  grep -Eq "'gl2'|'gles'" "$root/compositor-capability.txt"
  gdbus call --session --dest org.kde.KWin --object-path /KWin --method org.kde.KWin.activeOutputName >"$root/control/output-name.txt"
  backend_package=xdg-desktop-portal-kde
fi
export WAYLAND_DISPLAY=orbitpal-acceptance GDK_BACKEND=wayland QT_QPA_PLATFORM=wayland
wait_for 'private Wayland socket' test -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
printf '[preferred]\ndefault=%s\norg.freedesktop.impl.portal.ScreenCast=%s\n' "$ORBITPAL_ACCEPTANCE_DESKTOP" "$ORBITPAL_ACCEPTANCE_DESKTOP" >"$XDG_CONFIG_HOME/xdg-desktop-portal/portals.conf"
dbus-update-activation-environment XDG_RUNTIME_DIR XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME XDG_CURRENT_DESKTOP XDG_SESSION_TYPE WAYLAND_DISPLAY GDK_BACKEND QT_QPA_PLATFORM GTK_A11Y QT_LINUX_ACCESSIBILITY_ALWAYS_ON LANG LC_ALL
backend="$(backend_binary "$backend_package" "$backend_package")"
start_portal_backend "$ORBITPAL_ACCEPTANCE_DESKTOP" "$backend"
export ORBITPAL_ACCEPTANCE_PORTAL_PID="${pids[-1]}"
if ! wait_for 'private portal backend owner' dbus_owner "org.freedesktop.impl.portal.desktop.$ORBITPAL_ACCEPTANCE_DESKTOP" "$ORBITPAL_ACCEPTANCE_PORTAL_PID"; then
  # The parser emits only bounded numeric/fixed metadata into session.txt;
  # never copy an arbitrary D-Bus error or response into diagnostic evidence.
  gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.GetConnectionUnixProcessID "org.freedesktop.impl.portal.desktop.$ORBITPAL_ACCEPTANCE_DESKTOP" 2>/dev/null \
    | /usr/bin/python3 "$root/harness/service-owner.py" --diagnostic "$ORBITPAL_ACCEPTANCE_PORTAL_PID" || true
  exit 1
fi
if [[ "$ORBITPAL_ACCEPTANCE_DESKTOP" == kde ]]; then
  # Observe the real KWin authorization decision for the actual packaged backend,
  # not merely a frontend D-Bus interface or our separate synthetic input helper.
  authorization="kwin_core: authorized \"$(readlink -f "$backend")\" \"zkde_screencast_unstable_v1\""
  wait_for 'KWin authorized the real portal screencast interface' grep -Fxq -- "$authorization" "$root/compositor.txt"
  grep -Fx -- "$authorization" "$root/compositor.txt" >>"$root/compositor-capability.txt"
fi
# A compositor may activate the frontend before its display/config is ready.
# Replace only that daemon on our private session bus, using the real frontend.
# https://flatpak.github.io/xdg-desktop-portal/docs/building-and-running.html
start portal "$(backend_binary xdg-desktop-portal xdg-desktop-portal)" --replace
portal_pid="${pids[-1]}"
wait_for 'private desktop portal owner' dbus_owner org.freedesktop.portal.Desktop "$portal_pid"
{
  printf 'expected frontend pid: %s; expected backend pid: %s\n' "$portal_pid" "$ORBITPAL_ACCEPTANCE_PORTAL_PID"
  gdbus introspect --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop
} >"$root/portal-interfaces.txt"
grep -q org.freedesktop.portal.ScreenCast "$root/portal-interfaces.txt"
start desktop /usr/bin/python3 "$root/harness/desktop.py"
wait_for 'synthetic surface/controller' test -f "$root/control/ready"
if [[ "${ORBITPAL_ACCEPTANCE_MODE:-prototype}" == app ]]; then
  bash "$root/harness/app-session.sh"
  exit
fi
"$root/acceptance-test" --ignored --exact screen_portal::acceptance::real_portal_prototype --nocapture --test-threads=1
# libtest returns success for a mistyped filter that ran zero tests. Only the
# actual final assertion writes this fresh private receipt.
test -f "$root/control/prototype-passed"
grep -Fxq 'real-portal-prototype-complete' "$root/control/prototype-passed"
