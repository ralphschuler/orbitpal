#!/usr/bin/env bash
set -euo pipefail

: "${ORBITPAL_ACCEPTANCE_ROOT:?}" "${ORBITPAL_ACCEPTANCE_DESKTOP:?}"
[[ "${ORBITPAL_WAYLAND_ACCEPTANCE:-}" == synthetic-desktop-only && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux && "${GITHUB_ACTIONS:-}" == true ]]
[[ $(id -u) -ne 0 && -O "$ORBITPAL_ACCEPTANCE_ROOT" && ! -L "$ORBITPAL_ACCEPTANCE_ROOT" ]]
umask 077
root="$ORBITPAL_ACCEPTANCE_ROOT"
export XDG_RUNTIME_DIR="$root/runtime" XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache"
export XDG_SESSION_TYPE=wayland LANG=C.UTF-8 LC_ALL=C.UTF-8
export LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe GTK_A11Y=atspi QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1
unset DISPLAY WAYLAND_DISPLAY WAYLAND_SOCKET PULSE_SERVER PIPEWIRE_REMOTE AT_SPI_BUS_ADDRESS DBUS_STARTER_ADDRESS DBUS_STARTER_BUS_TYPE XAUTHORITY
pids=()
cleanup() { trap - EXIT; for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done; wait || true; }
trap cleanup EXIT
trap 'exit 130' INT TERM
start() { local name="$1"; shift; "$@" >"$root/$name.txt" 2>&1 & pids+=("$!"); }
wait_for() { local label="$1"; shift; for _ in {1..150}; do if "$@"; then return; fi; sleep .2; done; printf 'Timeout: %s\n' "$label" >&2; return 1; }
dbus_name() { gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner "$1" | grep -q true; }
backend_binary() { local package="$1" name="$2" file; while IFS= read -r file; do if [[ "${file##*/}" == "$name" && -x "$file" && ! -d "$file" ]]; then printf '%s\n' "$file"; return; fi; done < <(dpkg-query -L "$package"); return 1; }

"$root/render-probe" "${ORBITPAL_ACCEPTANCE_DRM:?}" >"$root/render-capability.txt" 2>&1
grep -Fxq 'real-gbm-egl-render-complete' "$root/render-capability.txt"
mkdir -p "$XDG_CONFIG_HOME/xdg-desktop-portal" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
gsettings set org.gnome.desktop.interface toolkit-accessibility true
start pipewire pipewire
wait_for 'private PipeWire socket' test -S "$XDG_RUNTIME_DIR/pipewire-0"
start wireplumber wireplumber
if [[ "$ORBITPAL_ACCEPTANCE_DESKTOP" == gnome ]]; then
  export XDG_CURRENT_DESKTOP=GNOME
  start compositor gnome-shell --wayland --headless --virtual-monitor=1280x960 --wayland-display=orbitpal-acceptance --no-x11
  wait_for 'GNOME compositor' dbus_name org.gnome.Mutter.ScreenCast
  backend_package=xdg-desktop-portal-gnome
else
  [[ "$ORBITPAL_ACCEPTANCE_DESKTOP" == kde ]]
  export XDG_CURRENT_DESKTOP=KDE KDE_FULL_SESSION=true KDE_SESSION_VERSION=6 KWIN_COMPOSE=O2
  kbuildsycoca6 --noincremental >"$root/kde-cache.txt" 2>&1
  start compositor kwin_wayland --virtual --width 1280 --height 960 --output-count 1 --socket orbitpal-acceptance --no-lockscreen --no-global-shortcuts
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
start portal-backend "$(backend_binary "$backend_package" "$backend_package")"
export ORBITPAL_ACCEPTANCE_PORTAL_PID="${pids[-1]}"
start portal "$(backend_binary xdg-desktop-portal xdg-desktop-portal)"
wait_for 'real desktop portal' dbus_name org.freedesktop.portal.Desktop
gdbus introspect --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop >"$root/portal-interfaces.txt"
grep -q org.freedesktop.portal.ScreenCast "$root/portal-interfaces.txt"
start desktop /usr/bin/python3 "$root/harness/desktop.py"
wait_for 'synthetic surface/controller' test -f "$root/control/ready"
"$root/acceptance-test" --ignored --exact screen_portal::acceptance::real_portal_prototype --nocapture --test-threads=1
# libtest returns success for a mistyped filter that ran zero tests. Only the
# actual final assertion writes this fresh private receipt.
test -f "$root/control/prototype-passed"
grep -Fxq 'real-portal-prototype-complete' "$root/control/prototype-passed"
