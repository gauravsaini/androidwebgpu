/*
 * Proof-of-life C program issuing EGL/GLES2 calls via SUBMIT_3D to Virtio-GPU
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <EGL/egl.h>
#include <GLES2/gl2.h>

int main(int argc, char** argv) {
    printf("[test_triangle] Starting EGL / GLES2 Proof-of-Life Triangle Test...\n");

    EGLDisplay dpy = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    if (dpy == EGL_NO_DISPLAY) {
        fprintf(stderr, "[test_triangle] Failed to get EGL display\n");
        return 1;
    }

    EGLint major = 0, minor = 0;
    if (!eglInitialize(dpy, &major, &minor)) {
        fprintf(stderr, "[test_triangle] Failed to initialize EGL\n");
        return 1;
    }
    printf("[test_triangle] EGL initialized version %d.%d\n", major, minor);

    EGLint config_attribs[] = {
        EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
        EGL_RED_SIZE, 8,
        EGL_GREEN_SIZE, 8,
        EGL_BLUE_SIZE, 8,
        EGL_ALPHA_SIZE, 8,
        EGL_NONE
    };

    EGLConfig config;
    EGLint num_configs = 0;
    if (!eglChooseConfig(dpy, config_attribs, &config, 1, &num_configs) || num_configs < 1) {
        fprintf(stderr, "[test_triangle] Failed to choose EGL config\n");
        return 1;
    }

    EGLSurface surface = eglCreateWindowSurface(dpy, config, 0, NULL);
    if (surface == EGL_NO_SURFACE) {
        fprintf(stderr, "[test_triangle] Failed to create EGL window surface\n");
        return 1;
    }

    EGLint ctx_attribs[] = {
        EGL_CONTEXT_CLIENT_VERSION, 2,
        EGL_NONE
    };
    EGLContext ctx = eglCreateContext(dpy, config, EGL_NO_CONTEXT, ctx_attribs);
    if (ctx == EGL_NO_CONTEXT) {
        fprintf(stderr, "[test_triangle] Failed to create EGL context\n");
        return 1;
    }

    if (!eglMakeCurrent(dpy, surface, surface, ctx)) {
        fprintf(stderr, "[test_triangle] Failed to make EGL context current\n");
        return 1;
    }

    printf("[test_triangle] Rendering triangle with glClearColor and glDrawArrays via SUBMIT_3D...\n");

    // 1. Set Viewport (Opcode 0x04)
    glViewport(0, 0, 1280, 720);

    // 2. Clear color to blue (0.2, 0.3, 0.8, 1.0) and Clear (Opcode 0x01)
    glClearColor(0.2f, 0.3f, 0.8f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);

    // 3. Draw triangle (Opcode 0x02)
    glDrawArrays(GL_TRIANGLES, 0, 3);

    // 4. Swap buffers with fence sync
    if (!eglSwapBuffers(dpy, surface)) {
        fprintf(stderr, "[test_triangle] eglSwapBuffers failed\n");
        return 1;
    }

    printf("[test_triangle] Blue triangle rendered and presented to WebGPU swapchain successfully!\n");

    eglDestroyContext(dpy, ctx);
    eglDestroySurface(dpy, surface);
    eglTerminate(dpy);

    return 0;
}
