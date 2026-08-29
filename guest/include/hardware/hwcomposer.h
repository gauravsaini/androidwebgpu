#pragma once
#include <hardware/hardware.h>
#include <cutils/native_handle.h>
#include <stdint.h>
#include <stddef.h>
#if defined(__has_include)
# if __has_include(<sys/cdefs.h>)
#  include <sys/cdefs.h>
# endif
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define HWC_HARDWARE_MODULE_ID "hwcomposer"
#define HWC_HARDWARE_COMPOSER "composer"
#define HWC_MODULE_API_VERSION_0_1 HARDWARE_MAKE_API_VERSION(0, 1)
#define HWC_DEVICE_API_VERSION_1_4 HARDWARE_MAKE_API_VERSION(1, 4)

enum {
    HWC_FRAMEBUFFER = 0,
    HWC_OVERLAY = 1,
    HWC_BACKGROUND = 2,
    HWC_FRAMEBUFFER_TARGET = 3,
    HWC_SIDEBAND = 4,
    HWC_CURSOR_OVERLAY = 5
};

enum {
    HWC_EVENT_VSYNC = 0,
    HWC_EVENT_ORIENTATION = 1
};

typedef struct hwc_layer_1 {
    int32_t compositionType;
    uint32_t hints;
    uint32_t flags;
    buffer_handle_t handle;
    uint32_t transform;
    int32_t blending;
    int32_t sourceCrop[4];
    int32_t displayFrame[4];
    int acquireFenceFd;
    int releaseFenceFd;
    uint8_t planeAlpha;
    uint8_t _pad[3];
} hwc_layer_1_t;

typedef struct hwc_display_contents_1 {
    uint32_t flags;
    size_t numHwLayers;
    hwc_layer_1_t hwLayers[0];
} hwc_display_contents_1_t;

struct hwc_composer_device_1;
typedef struct hwc_procs {
    void (*invalidate)(const struct hwc_procs* procs);
    void (*vsync)(const struct hwc_procs* procs, int disp, int64_t timestamp);
    void (*hotplug)(const struct hwc_procs* procs, int disp, int connected);
} hwc_procs_t;

typedef struct hwc_composer_device_1 {
    struct hw_device_t common;
    int (*prepare)(struct hwc_composer_device_1* dev,
                    size_t numDisplays, hwc_display_contents_1_t** displays);
    int (*set)(struct hwc_composer_device_1* dev,
                size_t numDisplays, hwc_display_contents_1_t** displays);
    int (*eventControl)(struct hwc_composer_device_1* dev, int disp,
            int event, int enabled);
    int (*blank)(struct hwc_composer_device_1* dev, int disp, int blank);
    int (*query)(struct hwc_composer_device_1* dev, int what, int* value);
    void (*registerProcs)(struct hwc_composer_device_1* dev,
            hwc_procs_t const* procs);
    void* reserved_proc[6];
} hwc_composer_device_1_t;

typedef struct hwc_module {
    struct hw_module_t common;
} hwc_module_t;

#ifdef __cplusplus
}
#endif
