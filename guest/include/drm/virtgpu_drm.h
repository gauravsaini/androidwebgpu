#ifndef _VIRTGPU_DRM_H
#define _VIRTGPU_DRM_H

#include <drm/drm.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DRM_VIRTGPU_MAP 0x01
#define DRM_VIRTGPU_EXECBUFFER 0x02
#define DRM_VIRTGPU_RESOURCE_CREATE 0x04
#define DRM_VIRTGPU_RESOURCE_INFO 0x05
#define DRM_VIRTGPU_WAIT 0x08

struct drm_virtgpu_map {
    uint64_t offset;
    uint32_t handle;
    uint32_t pad;
};

struct drm_virtgpu_execbuffer {
    uint32_t flags;
    uint32_t size;
    uint64_t command;
    uint64_t bo_handles;
    uint32_t num_bo_handles;
    int32_t fence_fd;
    uint32_t ring_idx;
    uint32_t pad;
};

#define VIRTGPU_EXECBUF_FENCE 0x01
#define VIRTGPU_EXECBUF_RING_IDX 0x02

struct drm_virtgpu_resource_create {
    uint32_t target;
    uint32_t format;
    uint32_t bind;
    uint32_t width;
    uint32_t height;
    uint32_t depth;
    uint32_t array_size;
    uint32_t last_level;
    uint32_t nr_samples;
    uint32_t flags;
    uint32_t bo_handle;
    uint32_t res_handle;
    uint32_t size;
    uint32_t stride;
};

struct drm_virtgpu_3d_wait {
    uint32_t handle;
    uint32_t flags;
};

#define DRM_IOCTL_VIRTGPU_MAP DRM_IOWR(DRM_COMMAND_BASE + DRM_VIRTGPU_MAP, struct drm_virtgpu_map)
#define DRM_IOCTL_VIRTGPU_EXECBUFFER DRM_IOWR(DRM_COMMAND_BASE + DRM_VIRTGPU_EXECBUFFER, struct drm_virtgpu_execbuffer)
#define DRM_IOCTL_VIRTGPU_RESOURCE_CREATE DRM_IOWR(DRM_COMMAND_BASE + DRM_VIRTGPU_RESOURCE_CREATE, struct drm_virtgpu_resource_create)
#define DRM_IOCTL_VIRTGPU_WAIT DRM_IOWR(DRM_COMMAND_BASE + DRM_VIRTGPU_WAIT, struct drm_virtgpu_3d_wait)

#ifdef __cplusplus
}
#endif

#endif
