#pragma once
#include <hardware/hardware.h>
#include <cutils/native_handle.h>

#ifdef __cplusplus
extern "C" {
#endif

#define GRALLOC_HARDWARE_MODULE_ID "gralloc"
#define GRALLOC_MODULE_API_VERSION_0_2 HARDWARE_MAKE_API_VERSION(0, 2)

typedef struct alloc_device_t {
    struct hw_device_t common;
    int (*alloc)(struct alloc_device_t* dev,
            int w, int h, int format, int usage,
            buffer_handle_t* handle, int* stride);
    int (*free)(struct alloc_device_t* dev,
            buffer_handle_t handle);
    void* reserved[7];
} alloc_device_t;

typedef struct gralloc_module_t {
    struct hw_module_t common;
    int (*registerBuffer)(struct gralloc_module_t const* module,
            buffer_handle_t handle);
    int (*unregisterBuffer)(struct gralloc_module_t const* module,
            buffer_handle_t handle);
    int (*lock)(struct gralloc_module_t const* module,
            buffer_handle_t handle, int usage,
            int l, int t, int w, int h,
            void** vaddr);
    int (*unlock)(struct gralloc_module_t const* module,
            buffer_handle_t handle);
    int (*perform)(struct gralloc_module_t const* module,
            int operation, ... );
    void* reserved_proc[6];
} gralloc_module_t;

#ifdef __cplusplus
}
#endif
