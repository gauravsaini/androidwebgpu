/*
 * Gralloc HAL implementation for Virtio-GPU backed by DRM GEM ioctls
 * Handles DRM GEM buffer allocations, mmap mapping, lock/unlock, and resource tracking.
 */

#include <hardware/gralloc.h>
#include <hardware/hardware.h>
#include <cutils/log.h>
#include <cutils/native_handle.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

#define LOG_TAG "gralloc.virtgpu"

static int g_drm_fd = -1;

static int get_drm_fd() {
    if (g_drm_fd < 0) {
        g_drm_fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
        if (g_drm_fd < 0) {
            g_drm_fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
        }
    }
    return g_drm_fd;
}

struct virtgpu_buffer_handle_t {
    native_handle_t base;
    int fd;
    int magic;
    int flags;
    int size;
    int offset;
    int width;
    int height;
    int format;
    uint32_t bo_handle;
    uint32_t res_id;
    uint64_t map_offset;
    void* mapped_ptr;
};

static int gralloc_alloc(alloc_device_t* dev, int w, int h, int format, int usage,
                         buffer_handle_t* pHandle, int* pStride) {
    if (!dev || !pHandle || !pStride) return -EINVAL;

    int drm_fd = get_drm_fd();
    if (drm_fd < 0) return -ENODEV;

    int bpp = 4;
    int stride = (w + 31) & ~31;
    size_t size = (size_t)stride * h * bpp;

    struct drm_virtgpu_resource_create res_create = {
        .target = 2, // 2D Texture
        .format = 67, // R8G8B8A8_UNORM
        .bind = 0x02, // Render target
        .width = (uint32_t)w,
        .height = (uint32_t)h,
        .depth = 1,
        .array_size = 1,
        .last_level = 0,
        .nr_samples = 0,
        .flags = 0,
        .bo_handle = 0,
        .res_handle = 0,
        .size = (uint32_t)size,
        .stride = (uint32_t)(stride * bpp),
    };

    if (ioctl(drm_fd, DRM_IOCTL_VIRTGPU_RESOURCE_CREATE, &res_create) != 0) {
        return -errno;
    }

    struct drm_virtgpu_map map_req = {
        .offset = 0,
        .handle = res_create.bo_handle,
        .pad = 0,
    };
    if (ioctl(drm_fd, DRM_IOCTL_VIRTGPU_MAP, &map_req) != 0) {
        struct drm_gem_close gem_close = { .handle = res_create.bo_handle, .pad = 0 };
        ioctl(drm_fd, DRM_IOCTL_GEM_CLOSE, &gem_close);
        return -errno;
    }

    struct virtgpu_buffer_handle_t* handle = (struct virtgpu_buffer_handle_t*)malloc(sizeof(struct virtgpu_buffer_handle_t));
    memset(handle, 0, sizeof(*handle));
    handle->base.version = sizeof(native_handle_t);
    handle->base.numFds = 1;
    handle->base.numInts = 10;
    handle->fd = drm_fd;
    handle->magic = 0x76677075; // 'vgpu'
    handle->width = w;
    handle->height = h;
    handle->format = format;
    handle->size = size;
    handle->bo_handle = res_create.bo_handle;
    handle->res_id = res_create.res_handle;
    handle->map_offset = map_req.offset;
    handle->mapped_ptr = NULL;

    *pHandle = (buffer_handle_t)handle;
    *pStride = stride;
    return 0;
}

static int gralloc_free(alloc_device_t* dev, buffer_handle_t handle) {
    if (!dev || !handle) return -EINVAL;
    struct virtgpu_buffer_handle_t* hnd = (struct virtgpu_buffer_handle_t*)handle;
    int drm_fd = get_drm_fd();

    if (hnd->mapped_ptr && hnd->mapped_ptr != MAP_FAILED) {
        munmap(hnd->mapped_ptr, hnd->size);
        hnd->mapped_ptr = NULL;
    }

    if (drm_fd >= 0 && hnd->bo_handle > 0) {
        struct drm_gem_close gem_close = { .handle = hnd->bo_handle, .pad = 0 };
        ioctl(drm_fd, DRM_IOCTL_GEM_CLOSE, &gem_close);
    }

    free(hnd);
    return 0;
}

static int gralloc_lock(gralloc_module_t const* module, buffer_handle_t handle,
                        int usage, int l, int t, int w, int h, void** vaddr) {
    if (!handle || !vaddr) return -EINVAL;
    struct virtgpu_buffer_handle_t* hnd = (struct virtgpu_buffer_handle_t*)handle;
    int drm_fd = get_drm_fd();
    if (drm_fd < 0) return -ENODEV;

    if (!hnd->mapped_ptr) {
        void* ptr = mmap(NULL, hnd->size, PROT_READ | PROT_WRITE, MAP_SHARED, drm_fd, (off_t)hnd->map_offset);
        if (ptr == MAP_FAILED) {
            return -errno;
        }
        hnd->mapped_ptr = ptr;
    }

    *vaddr = hnd->mapped_ptr;
    return 0;
}

static int gralloc_unlock(gralloc_module_t const* module, buffer_handle_t handle) {
    return 0;
}

static struct hw_module_methods_t gralloc_module_methods = {
    .open = [](const struct hw_module_t* module, const char* id, struct hw_device_t** device) -> int {
        alloc_device_t* dev = (alloc_device_t*)malloc(sizeof(alloc_device_t));
        memset(dev, 0, sizeof(*dev));
        dev->common.tag = HARDWARE_DEVICE_TAG;
        dev->common.version = 0;
        dev->common.module = const_cast<hw_module_t*>(module);
        dev->common.close = [](struct hw_device_t* dev) -> int { free(dev); return 0; };
        dev->alloc = gralloc_alloc;
        dev->free = gralloc_free;
        *device = &dev->common;
        return 0;
    }
};

struct gralloc_module_t HAL_MODULE_INFO_SYM = {
    .common = {
        .tag = HARDWARE_MODULE_TAG,
        .module_api_version = GRALLOC_MODULE_API_VERSION_0_2,
        .hal_api_version = HARDWARE_HAL_API_VERSION_0_0,
        .id = GRALLOC_HARDWARE_MODULE_ID,
        .name = "Virtio-GPU WebGPU Gralloc Module",
        .author = "Android WebGPU Team",
        .methods = &gralloc_module_methods,
    },
    .registerBuffer = [](gralloc_module_t const*, buffer_handle_t) { return 0; },
    .unregisterBuffer = [](gralloc_module_t const*, buffer_handle_t) { return 0; },
    .lock = gralloc_lock,
    .unlock = gralloc_unlock,
};
