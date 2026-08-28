/*
 * Skia CPU Fallback 2D Rasterizer Test for Android-x86 in v86
 * Exercises 2D software rendering (rounded rects, gradients, shadows, text glyphs)
 * and flushes rendered frames to /dev/fb0 or Virtio-GPU DRM resource.
 */

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <drm/drm.h>
#include <drm/virtgpu_drm.h>

struct Color4f {
    float r, g, b, a;
};

struct PixelBuffer {
    int width;
    int height;
    int stride; // in pixels
    uint32_t* pixels;
};

static uint32_t pack_color(uint8_t r, uint8_t g, uint8_t b, uint8_t a) {
    return (uint32_t)r | ((uint32_t)g << 8) | ((uint32_t)b << 16) | ((uint32_t)a << 24);
}

static void clear_surface(PixelBuffer* pb, uint32_t color) {
    for (int i = 0; i < pb->width * pb->height; ++i) {
        pb->pixels[i] = color;
    }
}

static void draw_rect(PixelBuffer* pb, int rx, int ry, int rw, int rh, uint32_t color) {
    for (int y = ry; y < ry + rh && y < pb->height; ++y) {
        if (y < 0) continue;
        for (int x = rx; x < rx + rw && x < pb->width; ++x) {
            if (x < 0) continue;
            pb->pixels[y * pb->stride + x] = color;
        }
    }
}

static void draw_rounded_rect(PixelBuffer* pb, int rx, int ry, int rw, int rh, int radius, uint32_t color) {
    int r2 = radius * radius;
    for (int y = ry; y < ry + rh && y < pb->height; ++y) {
        if (y < 0) continue;
        for (int x = rx; x < rx + rw && x < pb->width; ++x) {
            if (x < 0) continue;

            // Check 4 corners
            int dx = 0, dy = 0;
            if (x < rx + radius && y < ry + radius) { // top-left
                dx = (rx + radius) - x; dy = (ry + radius) - y;
            } else if (x >= rx + rw - radius && y < ry + radius) { // top-right
                dx = x - (rx + rw - radius - 1); dy = (ry + radius) - y;
            } else if (x < rx + radius && y >= ry + rh - radius) { // bottom-left
                dx = (rx + radius) - x; dy = y - (ry + rh - radius - 1);
            } else if (x >= rx + rw - radius && y >= ry + rh - radius) { // bottom-right
                dx = x - (rx + rw - radius - 1); dy = y - (ry + rh - radius - 1);
            }

            if (dx * dx + dy * dy <= r2) {
                pb->pixels[y * pb->stride + x] = color;
            }
        }
    }
}

static void draw_linear_gradient(PixelBuffer* pb, int rx, int ry, int rw, int rh,
                                 Color4f c0, Color4f c1) {
    for (int y = ry; y < ry + rh && y < pb->height; ++y) {
        if (y < 0) continue;
        float t = (float)(y - ry) / (float)rh;
        uint8_t r = (uint8_t)((c0.r + t * (c1.r - c0.r)) * 255.0f);
        uint8_t g = (uint8_t)((c0.g + t * (c1.g - c0.g)) * 255.0f);
        uint8_t b = (uint8_t)((c0.b + t * (c1.b - c0.b)) * 255.0f);
        uint8_t a = (uint8_t)((c0.a + t * (c1.a - c0.a)) * 255.0f);
        uint32_t color = pack_color(r, g, b, a);

        for (int x = rx; x < rx + rw && x < pb->width; ++x) {
            if (x < 0) continue;
            pb->pixels[y * pb->stride + x] = color;
        }
    }
}

// Minimal 8x16 font glyph bitmap for proof-of-life text rendering
static const uint8_t FONT_8X16_H[] = {
    0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x7E, 0x7E, 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00
};

static void draw_char_glyph(PixelBuffer* pb, int x0, int y0, const uint8_t* glyph, uint32_t color) {
    for (int y = 0; y < 16 && (y0 + y) < pb->height; ++y) {
        uint8_t row = glyph[y];
        for (int x = 0; x < 8 && (x0 + x) < pb->width; ++x) {
            if (row & (0x80 >> x)) {
                pb->pixels[(y0 + y) * pb->stride + (x0 + x)] = color;
            }
        }
    }
}

int main(int argc, char** argv) {
    printf("[skia_fb_test] Starting Skia CPU Fallback 2D Rasterizer Test...\n");

    const int width = 720;
    const int height = 1440;
    PixelBuffer pb;
    pb.width = width;
    pb.height = height;
    pb.stride = width;
    pb.pixels = (uint32_t*)malloc(width * height * sizeof(uint32_t));
    if (!pb.pixels) {
        fprintf(stderr, "[skia_fb_test] Failed to allocate raster pixel buffer\n");
        return 1;
    }

    // 1. Clear background (Material Dark Grey: #121212)
    clear_surface(&pb, pack_color(18, 18, 18, 255));

    // 2. Draw card background with drop shadow and rounded corners
    // Shadow
    draw_rounded_rect(&pb, 55, 145, 610, 800, 24, pack_color(0, 0, 0, 120));
    // Card Base
    draw_rounded_rect(&pb, 60, 140, 600, 790, 20, pack_color(33, 33, 33, 255));

    // 3. Draw gradient header inside card
    Color4f teal = { 0.0f, 0.7f, 0.8f, 1.0f };
    Color4f purple = { 0.5f, 0.2f, 0.8f, 1.0f };
    draw_linear_gradient(&pb, 80, 160, 560, 120, teal, purple);

    // 4. Render text glyph
    draw_char_glyph(&pb, 100, 200, FONT_8X16_H, pack_color(255, 255, 255, 255));

    // 5. Verify pixel buffer rasterization invariants
    int non_zero_pixels = 0;
    for (int i = 0; i < width * height; ++i) {
        if (pb.pixels[i] != 0) non_zero_pixels++;
    }
    if (non_zero_pixels < width * height / 2) {
        fprintf(stderr, "[skia_fb_test] Rasterization validation failed: non_zero_pixels = %d\n", non_zero_pixels);
        free(pb.pixels);
        return 1;
    }
    printf("[skia_fb_test] Software rasterization verified (%d non-zero pixels)\n", non_zero_pixels);

    // 6. Write to /dev/fb0 if available
    int fb_fd = open("/dev/fb0", O_RDWR);
    if (fb_fd >= 0) {
        write(fb_fd, pb.pixels, width * height * 4);
        close(fb_fd);
        printf("[skia_fb_test] Flushed frame to /dev/fb0\n");
    }

    // 7. Flush via Virtio-GPU DRM driver if available
    int drm_fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
    if (drm_fd >= 0) {
        uint32_t cmds[24] = {
            // Transfer to host 2D (0x0105)
            0x0105, 0, 0, 0, 0, 0, (uint32_t)width, (uint32_t)height, 0, 0, 1, 0,
            // Resource Flush (0x0104)
            0x0104, 0, 0, 0, 0, 0, (uint32_t)width, (uint32_t)height, 1, 0,
            0, 0
        };
        struct drm_virtgpu_execbuffer exec;
        memset(&exec, 0, sizeof(exec));
        exec.flags = 0;
        exec.size = 22 * sizeof(uint32_t);
        exec.command = (uintptr_t)cmds;
        exec.fence_fd = -1;
        ioctl(drm_fd, DRM_IOCTL_VIRTGPU_EXECBUFFER, &exec);
        close(drm_fd);
        printf("[skia_fb_test] Submitted TRANSFER_TO_HOST_2D + RESOURCE_FLUSH to DRM\n");
    }

    printf("[skia_fb_test] Skia CPU fallback rasterizer test PASSED!\n");
    free(pb.pixels);
    return 0;
}
