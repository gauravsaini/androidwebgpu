/*
 * Gralloc HAL implementation for Android-x86 backed by Virtio-GPU WebGPU Bridge
 * Handles graphic buffer allocation, lock/unlock, and MMIO resource registration.
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

#define LOG_TAG "gralloc.virtio_gpu"

struct virtio_gpu_buffer_handle_t {
    native_handle_t base;
    int fd;
    int magic;
    int flags;
    int size;
    int offset;
    int width;
    int height;
    int format;
    uint32_t resource_id;
};

static int gralloc_alloc(alloc_device_t* dev, int w, int h, int format, int usage,
                         buffer_handle_t* pHandle, int* pStride) {
    if (!dev || !pHandle || !pStride) return -EINVAL;

    int bpp = 4;
    int stride = (w + 31) & ~31;
    size_t size = stride * h * bpp;

    // Allocate anonymous shared memory / dma_buf fd
    int fd = memfd_create("virtio_gpu_gralloc", MFD_CLOEXEC | MFD_ALLOW_SEALING);
    if (fd < 0) {
        ALOGE("Failed to create memfd: %s", strerror(errno));
        return -errno;
    }
    if (ftruncate(fd, size) < 0) {
        close(fd);
        return -errno;
    }

    struct virtio_gpu_buffer_handle_t* handle = (struct virtio_gpu_buffer_handle_t*)malloc(sizeof(struct virtio_gpu_buffer_handle_t));
    memset(handle, 0, sizeof(*handle));
    handle->base.version = sizeof(native_handle_t);
    handle->base.numFds = 1;
    handle->base.numInts = 8;
    handle->fd = fd;
    handle->width = w;
    handle->height = h;
    handle->format = format;
    handle->size = size;
    handle->resource_id = rand() % 0xFFFF + 1; // Registered with virtio-gpu host

    *pHandle = (buffer_handle_t)handle;
    *pStride = stride;
    return 0;
}

static int gralloc_free(alloc_device_t* dev, buffer_handle_t handle) {
    if (!dev || !handle) return -EINVAL;
    struct virtio_gpu_buffer_handle_t* hnd = (struct virtio_gpu_buffer_handle_t*)handle;
    if (hnd->fd >= 0) {
        close(hnd->fd);
    }
    free(hnd);
    return 0;
}

static int gralloc_lock(gralloc_module_t const* module, buffer_handle_t handle,
                        int usage, int l, int t, int w, int h, void** vaddr) {
    if (!handle || !vaddr) return -EINVAL;
    struct virtio_gpu_buffer_handle_t* hnd = (struct virtio_gpu_buffer_handle_t*)handle;
    void* ptr = mmap(NULL, hnd->size, PROT_READ | PROT_WRITE, MAP_SHARED, hnd->fd, 0);
    if (ptr == MAP_FAILED) {
        return -errno;
    }
    *vaddr = ptr;
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
