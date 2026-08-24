/*
 * EGL / GLES Driver Dispatcher for Virtio-GPU
 * Handles DRM GEM buffer allocations, command execution, and eglSwapBuffers flushes.
 */

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

struct egl_display_t {
    int magic;
    int drm_fd;
};

struct egl_surface_t {
    int width;
    int height;
    uint32_t bo_handle;
    uint32_t res_id;
    void* mapped_ptr;
    size_t size;
};

struct egl_context_t {
    int ctx_id;
    int drm_fd;
    uint32_t current_program;
};

EGLAPI EGLDisplay EGLAPIENTRY eglGetDisplay(EGLNativeDisplayType display_id) {
    egl_display_t* dpy = (egl_display_t*)malloc(sizeof(egl_display_t));
    dpy->magic = 0x12345678;
    dpy->drm_fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
    if (dpy->drm_fd < 0) {
        dpy->drm_fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
    }
    return (EGLDisplay)dpy;
}

EGLAPI EGLBoolean EGLAPIENTRY eglInitialize(EGLDisplay dpy, EGLint *major, EGLint *minor) {
    if (major) *major = 1;
    if (minor) *minor = 4;
    return EGL_TRUE;
}

EGLAPI EGLBoolean EGLAPIENTRY eglTerminate(EGLDisplay dpy) {
    if (dpy) {
        egl_display_t* d = (egl_display_t*)dpy;
        if (d->drm_fd >= 0) close(d->drm_fd);
        free(d);
    }
    return EGL_TRUE;
}

EGLAPI EGLSurface EGLAPIENTRY eglCreateWindowSurface(EGLDisplay dpy, EGLConfig config,
                                                    EGLNativeWindowType win,
                                                    const EGLint *attrib_list) {
    egl_display_t* d = (egl_display_t*)dpy;
    egl_surface_t* surf = (egl_surface_t*)malloc(sizeof(egl_surface_t));
    surf->width = 1080;
    surf->height = 1920;
    surf->size = surf->width * surf->height * 4;
    surf->bo_handle = 0;
    surf->res_id = 1;
    surf->mapped_ptr = NULL;

    if (d && d->drm_fd >= 0) {
        struct drm_virtgpu_resource_create res_create = {
            .target = 2, // 2D Texture
            .format = 67, // R8G8B8A8_UNORM
            .bind = 0x02, // Render target
            .width = (uint32_t)surf->width,
            .height = (uint32_t)surf->height,
            .depth = 1,
            .array_size = 1,
            .last_level = 0,
            .nr_samples = 0,
            .flags = 0,
            .bo_handle = 0,
            .res_handle = 0,
            .size = (uint32_t)surf->size,
            .stride = (uint32_t)(surf->width * 4),
        };

        if (ioctl(d->drm_fd, DRM_IOCTL_VIRTGPU_RESOURCE_CREATE, &res_create) == 0) {
            surf->bo_handle = res_create.bo_handle;
            surf->res_id = res_create.res_handle;

            struct drm_virtgpu_map map_req = {
                .offset = 0,
                .handle = surf->bo_handle,
                .pad = 0,
            };
            if (ioctl(d->drm_fd, DRM_IOCTL_VIRTGPU_MAP, &map_req) == 0) {
                surf->mapped_ptr = mmap(NULL, surf->size, PROT_READ | PROT_WRITE,
                                        MAP_SHARED, d->drm_fd, map_req.offset);
            }
        }
    }

    return (EGLSurface)surf;
}

EGLAPI EGLBoolean EGLAPIENTRY eglDestroySurface(EGLDisplay dpy, EGLSurface surface) {
    egl_display_t* d = (egl_display_t*)dpy;
    egl_surface_t* s = (egl_surface_t*)surface;
    if (s) {
        if (s->mapped_ptr && s->mapped_ptr != MAP_FAILED) {
            munmap(s->mapped_ptr, s->size);
        }
        if (d && d->drm_fd >= 0 && s->bo_handle > 0) {
            struct drm_gem_close gem_close = { .handle = s->bo_handle, .pad = 0 };
            ioctl(d->drm_fd, DRM_IOCTL_GEM_CLOSE, &gem_close);
        }
        free(s);
    }
    return EGL_TRUE;
}

EGLAPI EGLContext EGLAPIENTRY eglCreateContext(EGLDisplay dpy, EGLConfig config,
                                              EGLContext share_context,
                                              const EGLint *attrib_list) {
    egl_context_t* ctx = (egl_context_t*)malloc(sizeof(egl_context_t));
    egl_display_t* d = (egl_display_t*)dpy;
    ctx->ctx_id = rand() % 0xFFFF + 1;
    ctx->drm_fd = d ? d->drm_fd : -1;
    ctx->current_program = 0;
    return (EGLContext)ctx;
}

EGLAPI EGLBoolean EGLAPIENTRY eglDestroyContext(EGLDisplay dpy, EGLContext ctx) {
    if (ctx) free(ctx);
    return EGL_TRUE;
}

EGLAPI EGLBoolean EGLAPIENTRY eglMakeCurrent(EGLDisplay dpy, EGLSurface draw,
                                            EGLSurface read, EGLContext ctx) {
    return EGL_TRUE;
}

EGLAPI EGLBoolean EGLAPIENTRY eglSwapBuffers(EGLDisplay dpy, EGLSurface surface) {
    egl_display_t* d = (egl_display_t*)dpy;
    egl_surface_t* s = (egl_surface_t*)surface;

    if (d && d->drm_fd >= 0 && s) {
        // Submit VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D followed by RESOURCE_FLUSH
        uint32_t cmds[16] = {
            // Transfer to host 2D
            0x0105, 0, 0, 0, 0, 0, (uint32_t)s->width, (uint32_t)s->height, 0, 0, s->res_id, 0,
            // Resource Flush
            0x0104, 0, 0, 0, 0, 0, (uint32_t)s->width, (uint32_t)s->height, s->res_id, 0
        };

        uint32_t bo_handles[1] = { s->bo_handle };

        struct drm_virtgpu_execbuffer exec = {
            .flags = VIRTGPU_EXECBUF_FENCE,
            .size = sizeof(cmds),
            .command = (uintptr_t)cmds,
            .fence_fd = -1,
            .num_bo_handles = (s->bo_handle > 0) ? 1u : 0u,
            .bo_handles = (uintptr_t)bo_handles,
        };

        ioctl(d->drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
    }
    return EGL_TRUE;
}
