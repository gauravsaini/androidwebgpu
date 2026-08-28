#pragma once
#include <stdint.h>
#include <sys/cdefs.h>

#ifdef __cplusplus
extern "C" {
#endif

#define HARDWARE_MODULE_TAG 0x48574D54
#define HARDWARE_DEVICE_TAG 0x48574454
#define HARDWARE_MAKE_API_VERSION(maj,min) (((maj) << 8) | (min))
#define HARDWARE_HAL_API_VERSION_0_0 HARDWARE_MAKE_API_VERSION(0, 0)
#define HAL_MODULE_INFO_SYM HMI
#define HAL_MODULE_INFO_SYM_AS_STR "HMI"

struct hw_module_t;
struct hw_module_methods_t;
struct hw_device_t;

typedef struct hw_module_t {
    uint32_t tag;
    uint16_t module_api_version;
    uint16_t hal_api_version;
    const char *id;
    const char *name;
    const char *author;
    struct hw_module_methods_t* methods;
    void* dso;
    uint32_t reserved[25];
} hw_module_t;

typedef struct hw_module_methods_t {
    int (*open)(const struct hw_module_t* module, const char* id,
            struct hw_device_t** device);
} hw_module_methods_t;

typedef struct hw_device_t {
    uint32_t tag;
    uint32_t version;
    struct hw_module_t* module;
    uint32_t reserved[12];
    int (*close)(struct hw_device_t* device);
} hw_device_t;

#ifdef __cplusplus
}
#endif
