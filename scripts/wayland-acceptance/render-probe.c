/* Real GBM/EGL rendering on the disposable VM's newly provisioned vgem node.
 * No portal response or PipeWire buffer is supplied by this capability check.
 * The actual portal/consumer test still has to capture its own fresh frames.
 */
#define _GNU_SOURCE
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <fcntl.h>
#include <errno.h>
#include <inttypes.h>
#include <gbm.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <xf86drm.h>

static bool marker(const char *key, const char *expected)
{
    const char *value = getenv(key);
    return value && strcmp(value, expected) == 0;
}

int main(int argc, char **argv)
{
    alarm(10);
    if (argc != 2 || geteuid() == 0 || getuid() != geteuid()
        || !marker("ORBITPAL_WAYLAND_ACCEPTANCE", "synthetic-desktop-only")
        || !marker("GITHUB_ACTIONS", "true")
        || !marker("RUNNER_ENVIRONMENT", "github-hosted") || !marker("RUNNER_OS", "Linux")
        || strncmp(argv[1], "/dev/dri/card", 13) != 0 || !argv[1][13]
        || strspn(argv[1] + 13, "0123456789") != strlen(argv[1] + 13)) {
        fputs("Refusing render probe outside its disposable hosted job\n", stderr);
        return 1;
    }
    int result = 1;
    int fd = open(argv[1], O_RDWR | O_CLOEXEC | O_NOFOLLOW);
    struct stat metadata;
    struct gbm_device *gbm = NULL;
    struct gbm_bo *buffer = NULL;
    EGLDisplay display = EGL_NO_DISPLAY;
    EGLContext context = EGL_NO_CONTEXT;
    GLuint texture = 0, framebuffer = 0;
    if (fd < 0) {
        fprintf(stderr, "Cannot open the private vgem node (errno %d)\n", errno);
        goto cleanup;
    }
    if (fstat(fd, &metadata) != 0) {
        fprintf(stderr, "Cannot inspect the private vgem node (errno %d)\n", errno);
        goto cleanup;
    }
    if (!S_ISCHR(metadata.st_mode) || metadata.st_uid != geteuid()
        || (metadata.st_mode & 0777) != 0600) {
        fprintf(stderr, "Render probe requires the private owned vgem device: char=%d, uid=%" PRIuMAX ", expected uid=%" PRIuMAX ", mode=%03o\n",
                S_ISCHR(metadata.st_mode) ? 1 : 0, (uintmax_t)metadata.st_uid,
                (uintmax_t)geteuid(), (unsigned)(metadata.st_mode & 0777));
        goto cleanup;
    }
    drmVersionPtr version = drmGetVersion(fd);
    bool vgem = version && version->name_len == 4 && memcmp(version->name, "vgem", 4) == 0;
    if (version) drmFreeVersion(version);
    if (!vgem) {
        fputs("Refusing a DRM device other than genuine vgem\n", stderr);
        goto cleanup;
    }
    gbm = gbm_create_device(fd);
    if (!gbm) goto cleanup;
    buffer = gbm_bo_create(gbm, 16, 16, GBM_FORMAT_XRGB8888, GBM_BO_USE_RENDERING | GBM_BO_USE_LINEAR);
    if (!buffer) goto cleanup;
    int dma_fd = gbm_bo_get_fd(buffer);
    if (dma_fd < 0) goto cleanup;
    close(dma_fd);
    PFNEGLGETPLATFORMDISPLAYEXTPROC platform_display = (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    if (!platform_display) goto cleanup;
    display = platform_display(EGL_PLATFORM_GBM_KHR, gbm, NULL);
    EGLint major = 0, minor = 0;
    if (display == EGL_NO_DISPLAY || !eglInitialize(display, &major, &minor) || !eglBindAPI(EGL_OPENGL_ES_API)) goto cleanup;
    const char *extensions = eglQueryString(display, EGL_EXTENSIONS);
    if (!extensions || !strstr(extensions, "EGL_KHR_surfaceless_context") || !strstr(extensions, "EGL_KHR_no_config_context")) goto cleanup;
    const EGLint attributes[] = {EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE};
    context = eglCreateContext(display, EGL_NO_CONFIG_KHR, EGL_NO_CONTEXT, attributes);
    if (context == EGL_NO_CONTEXT || !eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, context)) goto cleanup;
    glGenTextures(1, &texture);
    glBindTexture(GL_TEXTURE_2D, texture);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, 16, 16, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    glGenFramebuffers(1, &framebuffer);
    glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);
    glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, texture, 0);
    if (glCheckFramebufferStatus(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE) goto cleanup;
    glViewport(0, 0, 16, 16);
    glClearColor(0.75f, 0.0f, 0.0f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    unsigned char pixel[4] = {0};
    glReadPixels(8, 8, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, pixel);
    if (glGetError() != GL_NO_ERROR || pixel[0] < 180 || pixel[1] > 10 || pixel[2] > 10) goto cleanup;
    printf("DRM driver: vgem\nEGL: %d.%d\nEGL vendor: %.160s\nGL renderer: %.160s\nreal-gbm-egl-render-complete\n",
           major, minor, eglQueryString(display, EGL_VENDOR), (const char *)glGetString(GL_RENDERER));
    result = 0;
cleanup:
    if (result != 0) fprintf(stderr, "Genuine GBM/EGL render capability failed (EGL error 0x%x)\n", eglGetError());
    if (context != EGL_NO_CONTEXT) {
        if (framebuffer) glDeleteFramebuffers(1, &framebuffer);
        if (texture) glDeleteTextures(1, &texture);
        eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        eglDestroyContext(display, context);
    }
    if (display != EGL_NO_DISPLAY) eglTerminate(display);
    if (buffer) gbm_bo_destroy(buffer);
    if (gbm) gbm_device_destroy(gbm);
    if (fd >= 0) close(fd);
    return result;
}
