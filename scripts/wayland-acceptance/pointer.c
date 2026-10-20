/*
 * Genuine pointer input for the disposable KDE acceptance desktop only.
 * This never calls a portal API, fabricates consent, or reads screen pixels.
 *
 * KWin 5.27 requires a desktop entry with this executable's exact absolute
 * Exec path and X-KDE-Wayland-Interfaces=org_kde_kwin_fake_input; it checks the
 * executable origin before exposing the interface. Do not disable that check.
 * https://github.com/KDE/kwin/blob/v5.27.11/src/wayland_server.cpp
 * https://github.com/KDE/kwin/blob/v5.27.11/src/utils/serviceutils.h
 * KWin then authenticates this bound input device and routes pointer events:
 * https://github.com/KDE/kwin/blob/v5.27.11/src/backends/fakeinput/fakeinputdevice.cpp
 * Protocol v4, shipped by Ubuntu 24.04's plasma-wayland-protocols 1.10.0:
 * https://github.com/KDE/plasma-wayland-protocols/blob/v1.10.0/src/protocols/fake-input.xml
 */
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#define _XOPEN_SOURCE 700

#include <errno.h>
#include <limits.h>
#include <linux/input-event-codes.h>
#include <math.h>
#include <signal.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>
#include <wayland-client.h>

#include "fake-input-client-protocol.h"

static struct org_kde_kwin_fake_input *input;

static void timed_out(int signal_number)
{
    (void)signal_number;
    static const char message[] = "Synthetic pointer input timed out\n";
    const ssize_t written = write(STDERR_FILENO, message, sizeof(message) - 1);
    (void)written;
    _exit(124);
}

static bool marker(const char *name, const char *expected)
{
    const char *value = getenv(name);
    return value && strcmp(value, expected) == 0;
}

static bool private_directory(const char *path)
{
    struct stat metadata;
    return lstat(path, &metadata) == 0 && S_ISDIR(metadata.st_mode)
        && metadata.st_uid == geteuid() && (metadata.st_mode & 0777) == 0700;
}

static bool coordinate(const char *value, double maximum, double *result)
{
    char *end;
    if (!value || !*value)
        return false;
    errno = 0;
    *result = strtod(value, &end);
    return errno == 0 && end != value && *end == '\0'
        && isfinite(*result) && *result >= 0 && *result < maximum;
}

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version)
{
    (void)data;
    if (!input && version >= 3 && strcmp(interface, "org_kde_kwin_fake_input") == 0)
        input = wl_registry_bind(registry, name, &org_kde_kwin_fake_input_interface,
                                 version < 4 ? version : 4);
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name)
{
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = global,
    .global_remove = global_remove,
};

int main(int argc, char **argv)
{
    struct sigaction timeout_action = {0};
    timeout_action.sa_handler = timed_out;
    sigemptyset(&timeout_action.sa_mask);
    if (sigaction(SIGALRM, &timeout_action, NULL) != 0)
        return 1;
    alarm(5); /* Includes connection, registry discovery and every round trip. */

    double x, y;
    if (argc != 3 || !coordinate(argv[1], 1280, &x) || !coordinate(argv[2], 720, &y)
        || geteuid() == 0 || getuid() != geteuid()
        || !marker("ORBITPAL_WAYLAND_ACCEPTANCE", "synthetic-desktop-only")
        || !marker("GITHUB_ACTIONS", "true")
        || !marker("RUNNER_ENVIRONMENT", "github-hosted")
        || !marker("RUNNER_OS", "Linux")
        || !marker("ORBITPAL_ACCEPTANCE_DESKTOP", "kde")) {
        fputs("Refusing pointer input outside the isolated KDE acceptance job\n", stderr);
        return 1;
    }

    const char *root = getenv("ORBITPAL_ACCEPTANCE_ROOT");
    const char *runtime = getenv("XDG_RUNTIME_DIR");
    const char *display_name = getenv("WAYLAND_DISPLAY");
    char canonical[PATH_MAX], expected_runtime[PATH_MAX];
    if (!root || root[0] != '/' || !realpath(root, canonical)
        || strcmp(root, canonical) != 0 || !private_directory(root)) {
        fputs("Invalid private acceptance root\n", stderr);
        return 1;
    }
    const char *basename = strrchr(root, '/');
    if (!basename || strncmp(basename + 1, "orbitpal-wayland.", sizeof("orbitpal-wayland.") - 1) != 0
        || snprintf(expected_runtime, sizeof(expected_runtime), "%s/runtime", root)
             >= (int)sizeof(expected_runtime)
        || !runtime || strcmp(runtime, expected_runtime) != 0
        || !private_directory(runtime)
        || !display_name || strncmp(display_name, "orbitpal-", 9) != 0
        || strspn(display_name, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_")
             != strlen(display_name)) {
        fputs("Invalid private acceptance Wayland destination\n", stderr);
        return 1;
    }

    struct sockaddr_un address = { .sun_family = AF_UNIX };
    int length = snprintf(address.sun_path, sizeof(address.sun_path), "%s/%s", runtime, display_name);
    struct stat metadata;
    if (length < 0 || length >= (int)sizeof(address.sun_path)
        || lstat(address.sun_path, &metadata) != 0 || !S_ISSOCK(metadata.st_mode)
        || metadata.st_uid != geteuid()) {
        fputs("Private acceptance Wayland socket is unavailable\n", stderr);
        return 1;
    }

    /* Connect to this exact filesystem socket. WAYLAND_SOCKET and inherited
     * display descriptors can never redirect this client to another session. */
    int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0 || connect(fd, (struct sockaddr *)&address,
                        offsetof(struct sockaddr_un, sun_path) + (size_t)length + 1) != 0) {
        if (fd >= 0)
            close(fd);
        fputs("Cannot connect to the private acceptance Wayland socket\n", stderr);
        return 1;
    }
    struct wl_display *display = wl_display_connect_to_fd(fd);
    if (!display) {
        /* The process exits here; no connection can be reused after failure. */
        fputs("Cannot initialize the private acceptance Wayland connection\n", stderr);
        return 1;
    }

    int result = 1;
    struct wl_registry *registry = wl_display_get_registry(display);
    if (!registry || wl_registry_add_listener(registry, &registry_listener, NULL) < 0
        || wl_display_roundtrip(display) < 0 || !input) {
        fputs("KWin fake-input v3/v4 is unavailable to the registered helper\n", stderr);
        goto cleanup;
    }
    org_kde_kwin_fake_input_authenticate(input, "OrbitPal synthetic acceptance pointer",
                                       "Select a monitor in the disposable acceptance dialog");
    if (wl_display_roundtrip(display) < 0)
        goto connection_failed;
    org_kde_kwin_fake_input_pointer_motion_absolute(input, wl_fixed_from_double(x), wl_fixed_from_double(y));
    if (wl_display_roundtrip(display) < 0)
        goto connection_failed;
    org_kde_kwin_fake_input_button(input, BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
    if (wl_display_roundtrip(display) < 0)
        goto connection_failed;
    org_kde_kwin_fake_input_button(input, BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
    if (wl_display_roundtrip(display) < 0)
        goto connection_failed;
    result = 0; /* Controller must still observe the real chooser selection. */
    goto cleanup;

connection_failed:
    fputs("Private acceptance pointer event delivery failed\n", stderr);
cleanup:
    if (input)
        wl_proxy_destroy((struct wl_proxy *)input); /* v3/v4 have no destructor request. */
    if (registry)
        wl_registry_destroy(registry);
    wl_display_disconnect(display);
    alarm(0);
    return result;
}
