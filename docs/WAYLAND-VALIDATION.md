# Wayland screen context: implementation and acceptance

Refs [issue #26](https://github.com/ralphschuler/orbitpal/issues/26). The ScreenCast/PipeWire backend and local lifecycle fixtures are implemented. **The real GNOME and KDE capture acceptance criteria remain open.** A macOS fixture run does not validate a Wayland desktop.

## Runtime and build requirements

Linux needs the session D-Bus, PipeWire, xdg-desktop-portal and its desktop-specific ScreenCast backend. The compositor controls which monitors can be selected. Build hosts additionally need `libpipewire-0.3-dev` and `libclang-dev`; CI and Linux release builds install these alongside the existing Tauri prerequisites. No capture helper process, screenshot file, global PipeWire socket, XWayland fallback or external model service is used.

The Studio's explicit selection opens one monitor with `persist_mode=0` where supported. Restore tokens are neither requested for reuse nor stored. A private D-Bus connection owns the session and observes `Session::Closed` and connection loss. The PipeWire connection is created exclusively from `OpenPipeWireRemote`'s restricted FD. Registry/listener/core/FD handles are owned by its bounded-poll worker. Between requests the worker observes source removal but has no video stream or pixel cache.

Portal v6's `pipewire-serial` targets `PW_KEY_TARGET_OBJECT` (`target.object`). A v6 response without a valid serial is rejected. Older portals resolve the selected node's `object.serial` once from the restricted registry during explicit connection. All later streams use that fixed serial and `DONT_RECONNECT`. Source destruction closes the session; a recycled numeric node ID cannot silently select a different monitor. [ScreenCast specification](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html).

Every inference request creates a new video stream, samples a newly delivered CPU frame and destroys the stream. Selection alone produces no frame. Only mapped MemPtr/MemFd RGBx/RGBA/BGRx/BGRA buffers are accepted; encoded/GPU-only formats are rejected explicitly. Dimensions, stride, chunk extent and corruption are checked before copying. The existing 64-megapixel/16,384-edge source budget, 1,280-edge JPEG output and image/message budgets remain in force. A bounded five-second frame wait and cooperative cancellation release per-request streams. [PipeWire stream API](https://pipewire.pages.freedesktop.org/pipewire-rs/pipewire/stream/struct.Stream.html).

Session ownership is per bot. The host's persisted screen/image grants and model destination still authorize each request. Session closure also cancels attached turns so late images/model output cannot commit; profile revision checks remain active around capture and inference. Changing a different bot does not transfer its session. Global Stop, explicit disconnect and app exit close sessions. OS process teardown also closes the private D-Bus connection; no permission restore token survives.

## Evidence and remaining manual matrix

Local fixtures use synthetic colored images and in-memory queues. They cover separate lease/grant identity, bot/model/display binding, replacement, revocation, Stop, distinct per-request results and discarding a frame delivered after revocation. A two-bot fixture checks that switching Studio context cancels only its old tickets, retains both explicitly selected leases, prevents cross-bot source reuse and leaves the other bot intact on disconnect. Existing screen-context fixtures additionally cover off/on revisions, image budgets and fresh provider messages. These fixtures do not connect to a real portal or PipeWire desktop.

| Environment | Compilation / fixture evidence | Real screen capture acceptance |
| --- | --- | --- |
| macOS host | Portable session code compiled and initial lifecycle fixtures passed; no Wayland backend compiled on this host | Not applicable; no real screens captured |
| Linux CI | Native backend compile and fixture run required on this PR's exact final head | Headless CI cannot establish compositor acceptance |
| Current GNOME Wayland | Pending a real session with its installed portal backend | **Pending** |
| Current KDE Plasma Wayland | Pending a real session with its installed portal backend | **Pending** |

For each real desktop, record distro, desktop/compositor, portal/PipeWire versions, app commit and pass/fail for the following operator checks. Use a synthetic desktop containing no private content and a local image-capable model or transport fixture; do not publish screenshots in the issue.

1. Enable screen/image permissions for one bot, select exactly one monitor in the compositor dialog, save, then request two distinct synthetic desktop states. Verify the receiving fixture gets the corresponding new JPEG each time, with no image in history, Hindsight, profile export or temporary screenshot files.
2. Cancel the system dialog, deny access, stop the portal service and disconnect/revoke through the compositor. Each action must close/cancel the affected turn and show a truthful reconnect state. Reopening the settings or restarting OrbitPal must not restore a lease automatically.
3. While capture or model delivery is pending, disable capture, change its monitor or model destination, switch bots, press global Stop and quit. No late result may be committed or replayed. A newly selected session must not revive an old turn. Confirm a second bot cannot use the first bot's source ID. A Studio bot switch cancels the old request tickets while retaining each bot's independent, explicitly selected lease; configure two bots and verify each can make a fresh request using only its own source. Disconnecting one bot must leave the other's lease and requests intact.
4. Remove/reconfigure the selected monitor, suspend/resume and reconnect devices. Verify source removal closes the lease and a reused numeric PipeWire node cannot retarget it. Exercise portal v6 serial targeting where available and the legacy serial-discovery path on an older portal.
5. Exercise scaled/rotated and multiple monitors, static and changing desktops, wide resolutions, CPU-format negotiation and a backend that offers only unsupported buffers. Confirm bounded JPEGs or explicit errors, with no fallback monitor or stale cached frame.
6. After Stop/disconnect/quit, confirm the compositor sharing indicator disappears and PipeWire streams/FDs are released. While merely connected but idle, confirm no OrbitPal video-receive stream exists. Repeat selection/cancellation to check that resources do not accumulate.

Keep issue #26 open until both desktop rows and the applicable lifecycle cases have actual evidence. Do not replace pending rows with inferred success from compile logs or fixtures.
