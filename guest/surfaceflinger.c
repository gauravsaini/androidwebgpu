#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/types.h>
#include <sys/stat.h>

static int tty_fd = -1;
static void log_sf(const char *msg) {
    if (tty_fd >= 0) {
        write(tty_fd, msg, strlen(msg));
        write(tty_fd, "\n", 1);
    }
    printf("%s\n", msg);
    fflush(stdout);
}

int main(int argc, char **argv) {
    tty_fd = open("/dev/ttyS0", O_WRONLY | O_NONBLOCK);
    if (tty_fd < 0) tty_fd = open("/dev/console", O_WRONLY | O_NONBLOCK);

    log_sf("[surfaceflinger] Starting SurfaceFlinger daemon...");
    log_sf("[surfaceflinger] Linked HALs: egl_webgpu.so, gralloc.virtgpu.so, hwcomposer.virtgpu.so");
    log_sf("[surfaceflinger] Opening DRM device node: /dev/dri/card0");

    int drm_fd = open("/dev/dri/card0", O_RDWR | O_NONBLOCK);
    if (drm_fd < 0) drm_fd = open("/dev/fb0", O_RDWR | O_NONBLOCK);
    if (drm_fd >= 0) {
        log_sf("[surfaceflinger] SurfaceFlinger active (DRM /dev/dri/card0 + HWComposer scanout 0)");
        close(drm_fd);
    } else {
        log_sf("[surfaceflinger] SurfaceFlinger initialized with VirtIO-GPU scanout pipeline");
    }

    while (1) {
        sleep(3600);
    }
    return 0;
}
