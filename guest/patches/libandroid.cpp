/*
 * Android NDK libandroid.so Implementation
 * Implements ANativeWindow and Native Activity APIs for native apps (Gecko libxul.so, Unity, Godot)
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ANativeWindow {
    int32_t width;
    int32_t height;
    int32_t format;
    uint32_t flags;
    void* internal_surface;
} ANativeWindow;

typedef struct ANativeWindow_Buffer {
    int32_t width;
    int32_t height;
    int32_t stride;
    int32_t format;
    void* bits;
    uint32_t reserved[6];
} ANativeWindow_Buffer;

ANativeWindow* ANativeWindow_fromSurface(void* env, void* surface) {
    ANativeWindow* win = (ANativeWindow*)calloc(1, sizeof(ANativeWindow));
    if (!win) return NULL;
    win->width = 720;
    win->height = 1440;
    win->format = 1; // WINDOW_FORMAT_RGBA_8888
    win->internal_surface = surface;
    fprintf(stderr, "[libandroid] ANativeWindow_fromSurface: created ANativeWindow=%p (720x1440, format=RGBA_8888)\n", win);
    return win;
}

void ANativeWindow_acquire(ANativeWindow* window) {
    // Reference counted in full implementation
    (void)window;
}

void ANativeWindow_release(ANativeWindow* window) {
    if (window) {
        free(window);
    }
}

int32_t ANativeWindow_getWidth(ANativeWindow* window) {
    return window ? window->width : 720;
}

int32_t ANativeWindow_getHeight(ANativeWindow* window) {
    return window ? window->height : 1440;
}

int32_t ANativeWindow_getFormat(ANativeWindow* window) {
    return window ? window->format : 1;
}

int32_t ANativeWindow_setBuffersGeometry(ANativeWindow* window, int32_t width, int32_t height, int32_t format) {
    if (!window) return -1;
    if (width > 0) window->width = width;
    if (height > 0) window->height = height;
    if (format > 0) window->format = format;
    return 0;
}

int32_t ANativeWindow_lock(ANativeWindow* window, ANativeWindow_Buffer* outBuffer, void* inOutDirtyBounds) {
    if (!window || !outBuffer) return -1;
    outBuffer->width = window->width;
    outBuffer->height = window->height;
    outBuffer->stride = window->width;
    outBuffer->format = window->format;
    outBuffer->bits = NULL;
    return 0;
}

int32_t ANativeWindow_unlockAndPost(ANativeWindow* window) {
    if (!window) return -1;
    return 0;
}

#ifdef __cplusplus
}
#endif
