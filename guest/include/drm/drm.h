#ifndef _DRM_H_
#define _DRM_H_

#include <stdint.h>
#include <sys/ioctl.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DRM_IOCTL_BASE 'd'
#define DRM_IO(nr) _IO(DRM_IOCTL_BASE, nr)
#define DRM_IOR(nr, type) _IOR(DRM_IOCTL_BASE, nr, type)
#define DRM_IOW(nr, type) _IOW(DRM_IOCTL_BASE, nr, type)
#define DRM_IOWR(nr, type) _IOWR(DRM_IOCTL_BASE, nr, type)

#define DRM_COMMAND_BASE 0x40
#define DRM_COMMAND_END 0xA0

struct drm_gem_close {
    uint32_t handle;
    uint32_t pad;
};

#define DRM_IOCTL_GEM_CLOSE DRM_IOW(0x09, struct drm_gem_close)

#ifdef __cplusplus
}
#endif

#endif
