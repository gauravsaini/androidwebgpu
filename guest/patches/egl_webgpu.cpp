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
#include <poll.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

#ifndef VIRTGPU_EXECBUF_FENCE
#define VIRTGPU_EXECBUF_FENCE 1
#endif
#ifndef VIRTGPU_EXECBUF_RING_IDX
#define VIRTGPU_EXECBUF_RING_IDX 2
#endif

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
    float clear_color[4];
};

static egl_context_t* g_current_ctx = NULL;

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

EGLAPI EGLBoolean EGLAPIENTRY eglChooseConfig(EGLDisplay dpy, const EGLint *attrib_list,
                                              EGLConfig *configs, EGLint config_size,
                                              EGLint *num_config) {
    if (!num_config) return EGL_FALSE;
    *num_config = 1;
    if (configs && config_size > 0) {
        configs[0] = (EGLConfig)(uintptr_t)1;
    }
    return EGL_TRUE;
}

EGLAPI EGLBoolean EGLAPIENTRY eglGetConfigAttrib(EGLDisplay dpy, EGLConfig config,
                                                EGLint attribute, EGLint *value) {
    if (!value) return EGL_FALSE;
    switch (attribute) {
        case EGL_BUFFER_SIZE:     *value = 32; break;
        case EGL_RED_SIZE:        *value = 8; break;
        case EGL_GREEN_SIZE:      *value = 8; break;
        case EGL_BLUE_SIZE:       *value = 8; break;
        case EGL_ALPHA_SIZE:      *value = 8; break;
        case EGL_DEPTH_SIZE:      *value = 24; break;
        case EGL_STENCIL_SIZE:    *value = 8; break;
        case EGL_CONFIG_ID:       *value = 1; break;
        case EGL_SURFACE_TYPE:    *value = EGL_WINDOW_BIT | EGL_PBUFFER_BIT; break;
        case EGL_RENDERABLE_TYPE: *value = EGL_OPENGL_ES2_BIT; break;
        case EGL_NATIVE_RENDERABLE: *value = EGL_TRUE; break;
        default: *value = 0; break;
    }
    return EGL_TRUE;
}

EGLAPI EGLSurface EGLAPIENTRY eglCreateWindowSurface(EGLDisplay dpy, EGLConfig config,
                                                    EGLNativeWindowType win,
                                                    const EGLint *attrib_list) {
    egl_display_t* d = (egl_display_t*)dpy;
    egl_surface_t* surf = (egl_surface_t*)malloc(sizeof(egl_surface_t));
    surf->width = 1280;
    surf->height = 720;
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
    ctx->clear_color[0] = 0.0f;
    ctx->clear_color[1] = 0.0f;
    ctx->clear_color[2] = 0.0f;
    ctx->clear_color[3] = 1.0f;
    return (EGLContext)ctx;
}

EGLAPI EGLBoolean EGLAPIENTRY eglDestroyContext(EGLDisplay dpy, EGLContext ctx) {
    if (ctx) {
        if (g_current_ctx == (egl_context_t*)ctx) {
            g_current_ctx = NULL;
        }
        free(ctx);
    }
    return EGL_TRUE;
}

EGLAPI EGLBoolean EGLAPIENTRY eglMakeCurrent(EGLDisplay dpy, EGLSurface draw,
                                            EGLSurface read, EGLContext ctx) {
    g_current_ctx = (egl_context_t*)ctx;
    return EGL_TRUE;
}

/*
 * GLES2 Stubs serializing to SUBMIT_3D opcode format
 */
GL_APICALL void GL_APIENTRY glClearColor(GLclampf red, GLclampf green, GLclampf blue, GLclampf alpha) {
    if (g_current_ctx) {
        g_current_ctx->clear_color[0] = red;
        g_current_ctx->clear_color[1] = green;
        g_current_ctx->clear_color[2] = blue;
        g_current_ctx->clear_color[3] = alpha;
    }
}

GL_APICALL void GL_APIENTRY glClear(GLbitfield mask) {
    if (!g_current_ctx || g_current_ctx->drm_fd < 0) return;

    // Opcode 0x01: CLEAR (mask: u32, r: f32, g: f32, b: f32, a: f32) -> 20 bytes payload + 8 bytes header
    uint32_t cmd[7];
    cmd[0] = 0x0001; // Opcode 0x01 (CLEAR)
    cmd[1] = 20;     // Payload size in bytes
    cmd[2] = (uint32_t)mask;
    memcpy(&cmd[3], &g_current_ctx->clear_color[0], sizeof(float));
    memcpy(&cmd[4], &g_current_ctx->clear_color[1], sizeof(float));
    memcpy(&cmd[5], &g_current_ctx->clear_color[2], sizeof(float));
    memcpy(&cmd[6], &g_current_ctx->clear_color[3], sizeof(float));

    struct drm_virtgpu_execbuffer exec = {
        .flags = 0,
        .size = sizeof(cmd),
        .command = (uintptr_t)cmd,
        .fence_fd = -1,
        .num_bo_handles = 0,
        .bo_handles = 0,
    };
    ioctl(g_current_ctx->drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
}

GL_APICALL void GL_APIENTRY glDrawArrays(GLenum mode, GLint first, GLsizei count) {
    if (!g_current_ctx || g_current_ctx->drm_fd < 0) return;

    // Opcode 0x02: DRAW_ARRAYS (mode: u32, first: u32, count: u32) -> 12 bytes payload + 8 bytes header
    uint32_t cmd[5];
    cmd[0] = 0x0002; // Opcode 0x02 (DRAW_ARRAYS)
    cmd[1] = 12;     // Payload size in bytes
    cmd[2] = (uint32_t)mode;
    cmd[3] = (uint32_t)first;
    cmd[4] = (uint32_t)count;

    struct drm_virtgpu_execbuffer exec = {
        .flags = 0,
        .size = sizeof(cmd),
        .command = (uintptr_t)cmd,
        .fence_fd = -1,
        .num_bo_handles = 0,
        .bo_handles = 0,
    };
    ioctl(g_current_ctx->drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
}

GL_APICALL void GL_APIENTRY glViewport(GLint x, GLint y, GLsizei width, GLsizei height) {
    if (!g_current_ctx || g_current_ctx->drm_fd < 0) return;

    // Opcode 0x04: VIEWPORT (x: i32, y: i32, w: u32, h: u32) -> 16 bytes payload + 8 bytes header
    uint32_t cmd[6];
    cmd[0] = 0x0004; // Opcode 0x04 (VIEWPORT)
    cmd[1] = 16;     // Payload size in bytes
    cmd[2] = (uint32_t)x;
    cmd[3] = (uint32_t)y;
    cmd[4] = (uint32_t)width;
    cmd[5] = (uint32_t)height;

    struct drm_virtgpu_execbuffer exec = {
        .flags = 0,
        .size = sizeof(cmd),
        .command = (uintptr_t)cmd,
        .fence_fd = -1,
        .num_bo_handles = 0,
        .bo_handles = 0,
    };
    ioctl(g_current_ctx->drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
}

EGLAPI EGLBoolean EGLAPIENTRY eglSwapBuffers(EGLDisplay dpy, EGLSurface surface) {
    egl_display_t* d = (egl_display_t*)dpy;
    egl_surface_t* s = (egl_surface_t*)surface;

    if (d && d->drm_fd >= 0 && s) {
        // Submit VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D followed by RESOURCE_FLUSH (total 22 words)
        uint32_t cmds[24] = {
            // Transfer to host 2D (0x0105, 12 words)
            0x0105, 0, 0, 0, 0, 0, (uint32_t)s->width, (uint32_t)s->height, 0, 0, s->res_id, 0,
            // Resource Flush (0x0104, 10 words)
            0x0104, 0, 0, 0, 0, 0, (uint32_t)s->width, (uint32_t)s->height, s->res_id, 0,
            0, 0
        };

        uint32_t bo_handles[1] = { s->bo_handle };

        struct drm_virtgpu_execbuffer exec = {
            .flags = VIRTGPU_EXECBUF_FENCE,
            .size = 22 * sizeof(uint32_t),
            .command = (uintptr_t)cmds,
            .fence_fd = -1,
            .num_bo_handles = (s->bo_handle > 0) ? 1u : 0u,
            .bo_handles = (uintptr_t)bo_handles,
        };

        int ret = ioctl(d->drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
        if (ret == 0 && exec.fence_fd >= 0) {
            struct pollfd pfd = { .fd = exec.fence_fd, .events = POLLIN, .revents = 0 };
            poll(&pfd, 1, 1000);
            close(exec.fence_fd);
        } else if (ret == 0 && s->bo_handle > 0) {
            struct drm_virtgpu_3d_wait wait_req = {
                .handle = s->bo_handle,
                .flags = 0,
            };
            ioctl(d->drm_fd, DRM_IOCTL_VIRTGPU_WAIT, &wait_req);
        }
    }
    return EGL_TRUE;
}
