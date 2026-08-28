/*
 * Hardware Composer HAL for Virtio-GPU
 * Marshals SurfaceFlinger layers directly to host WebGpuCompositor via DRM RESOURCE_FLUSH ioctls.
 */

#include <hardware/hwcomposer.h>
#include <cutils/log.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/ioctl.h>
#include <sys/time.h>
#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

#define LOG_TAG "hwcomposer.virtgpu"

struct hwc_context_t {
    hwc_composer_device_1_t device;
    const hwc_procs_t* procs;
    pthread_t vsync_thread;
    bool vsync_enabled;
    int drm_fd;
};

static void* vsync_loop(void* data) {
    hwc_context_t* ctx = (hwc_context_t*)data;
    const uint64_t period_ns = 1000000000ULL / 60; // 60 Hz VSync

    while (true) {
        usleep(period_ns / 1000);
        if (ctx->vsync_enabled && ctx->procs && ctx->procs->vsync) {
            struct timespec ts;
            clock_gettime(CLOCK_MONOTONIC, &ts);
            uint64_t timestamp = (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
            ctx->procs->vsync(ctx->procs, 0, timestamp);
        }
    }
    return NULL;
}

static int hwc_prepare(hwc_composer_device_1_t* dev, size_t numDisplays,
                       hwc_display_contents_1_t** displays) {
    if (!displays || numDisplays == 0 || !displays[0]) return 0;
    hwc_display_contents_1_t* list = displays[0];

    // Mark all layers for Device/Host WebGPU composition
    for (size_t i = 0; i < list->numHwLayers; ++i) {
        hwc_layer_1_t* layer = &list->hwLayers[i];
        if (layer->compositionType == HWC_FRAMEBUFFER) {
            layer->compositionType = HWC_OVERLAY;
        }
    }
    return 0;
}

static int hwc_set(hwc_composer_device_1_t* dev, size_t numDisplays,
                   hwc_display_contents_1_t** displays) {
    if (!displays || numDisplays == 0 || !displays[0]) return 0;
    hwc_context_t* ctx = (hwc_context_t*)dev;
    hwc_display_contents_1_t* list = displays[0];

    // Emit RESOURCE_FLUSH to DRM driver on display present
    if (ctx && ctx->drm_fd >= 0) {
        uint32_t width = 720;
        uint32_t height = 1440;
        uint32_t res_id = 1;

        uint32_t cmds[24] = {
            // Transfer to host 2D (0x0105, 12 words)
            0x0105, 0, 0, 0, 0, 0, width, height, 0, 0, res_id, 0,
            // Resource Flush (0x0104, 10 words)
            0x0104, 0, 0, 0, 0, 0, width, height, res_id, 0,
            0, 0
        };

        struct drm_virtgpu_execbuffer exec = {
            .flags = 0,
            .size = 22 * sizeof(uint32_t),
            .command = (uintptr_t)cmds,
            .bo_handles = 0,
            .num_bo_handles = 0,
            .fence_fd = -1,
        };

        ioctl(ctx->drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
    }

    for (size_t i = 0; i < list->numHwLayers; ++i) {
        hwc_layer_1_t* layer = &list->hwLayers[i];
        if (layer->acquireFenceFd >= 0) {
            close(layer->acquireFenceFd);
            layer->acquireFenceFd = -1;
        }
        layer->releaseFenceFd = -1;
    }
    return 0;
}

static int hwc_event_control(struct hwc_composer_device_1* dev, int disp,
                             int event, int enabled) {
    hwc_context_t* ctx = (hwc_context_t*)dev;
    if (event == HWC_EVENT_VSYNC) {
        ctx->vsync_enabled = (enabled != 0);
        return 0;
    }
    return -EINVAL;
}

static void hwc_register_procs(struct hwc_composer_device_1* dev,
                              const hwc_procs_t* procs) {
    hwc_context_t* ctx = (hwc_context_t*)dev;
    ctx->procs = procs;
}

static int hwc_device_open(const struct hw_module_t* module, const char* name,
                           struct hw_device_t** device) {
    if (strcmp(name, HWC_HARDWARE_COMPOSER)) return -EINVAL;

    hwc_context_t* ctx = (hwc_context_t*)malloc(sizeof(hwc_context_t));
    memset(ctx, 0, sizeof(*ctx));

    ctx->drm_fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
    if (ctx->drm_fd < 0) {
        ctx->drm_fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
    }

    ctx->device.common.tag = HARDWARE_DEVICE_TAG;
    ctx->device.common.version = HWC_DEVICE_API_VERSION_1_4;
    ctx->device.common.module = const_cast<hw_module_t*>(module);
    ctx->device.common.close = [](struct hw_device_t* dev) -> int {
        hwc_context_t* c = (hwc_context_t*)dev;
        if (c->drm_fd >= 0) close(c->drm_fd);
        free(c);
        return 0;
    };

    ctx->device.prepare = hwc_prepare;
    ctx->device.set = hwc_set;
    ctx->device.eventControl = hwc_event_control;
    ctx->device.registerProcs = hwc_register_procs;

    pthread_create(&ctx->vsync_thread, NULL, vsync_loop, ctx);

    *device = &ctx->device.common;
    return 0;
}

static struct hw_module_methods_t hwc_module_methods = {
    .open = hwc_device_open
};

hwc_module_t HAL_MODULE_INFO_SYM = {
    .common = {
        .tag = HARDWARE_MODULE_TAG,
        .module_api_version = HWC_MODULE_API_VERSION_0_1,
        .hal_api_version = HARDWARE_HAL_API_VERSION_0_0,
        .id = HWC_HARDWARE_MODULE_ID,
        .name = "Virtio-GPU WebGPU HWComposer Module",
        .author = "Android WebGPU Team",
        .methods = &hwc_module_methods,
    }
};
