#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <errno.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct native_handle {
    int version;        /* sizeof(native_handle_t) */
    int numFds;         /* number of file-descriptors at &data[0] */
    int numInts;        /* number of ints at &data[numFds] */
    int data[0];        /* numFds + numInts ints */
} native_handle_t;
typedef const native_handle_t* buffer_handle_t;

typedef struct ARect {
    int32_t left;
    int32_t top;
    int32_t right;
    int32_t bottom;
} ARect;

enum AIMapper_Version {
    AIMAPPER_VERSION_5 = 5,
};

enum AIMapper_Error {
    AIMAPPER_ERROR_NONE = 0,
    AIMAPPER_ERROR_BAD_DESCRIPTOR = 1,
    AIMAPPER_ERROR_BAD_BUFFER = 2,
    AIMAPPER_ERROR_BAD_VALUE = 3,
    AIMAPPER_ERROR_NO_RESOURCES = 5,
    AIMAPPER_ERROR_UNSUPPORTED = 7,
};

typedef struct AIMapper_MetadataType {
    const char* name;
    int64_t value;
} AIMapper_MetadataType;

typedef struct AIMapper_MetadataTypeDescription {
    AIMapper_MetadataType metadataType;
    const char* description;
    bool isGettable;
    bool isSettable;
    uint8_t reserved[32];
} AIMapper_MetadataTypeDescription;

typedef void (*AIMapper_DumpBufferCallback)(void* context,
                                            AIMapper_MetadataType metadataType,
                                            const void* value, size_t valueSize);
typedef void (*AIMapper_BeginDumpBufferCallback)(void* context);

typedef struct AIMapperV5 {
    int32_t (*importBuffer)(const native_handle_t* handle,
                            buffer_handle_t* outBufferHandle);
    int32_t (*freeBuffer)(buffer_handle_t buffer);
    int32_t (*getTransportSize)(buffer_handle_t buffer,
                                uint32_t* outNumFds,
                                uint32_t* outNumInts);
    int32_t (*lock)(buffer_handle_t buffer, uint64_t cpuUsage,
                    ARect accessRegion, int acquireFence,
                    void** outData);
    int32_t (*unlock)(buffer_handle_t buffer, int* releaseFence);
    int32_t (*flushLockedBuffer)(buffer_handle_t buffer);
    int32_t (*rereadLockedBuffer)(buffer_handle_t buffer);
    int32_t (*getMetadata)(buffer_handle_t buffer,
                           AIMapper_MetadataType metadataType,
                           void* destBuffer, size_t destBufferSize);
    int32_t (*getStandardMetadata)(buffer_handle_t buffer,
                                   int64_t standardMetadataType,
                                   void* destBuffer, size_t destBufferSize);
    int32_t (*setMetadata)(buffer_handle_t buffer,
                           AIMapper_MetadataType metadataType,
                           const void* metadata, size_t metadataSize);
    int32_t (*setStandardMetadata)(buffer_handle_t buffer,
                                   int64_t standardMetadataType,
                                   const void* metadata, size_t metadataSize);
    int32_t (*listSupportedMetadataTypes)(
            const AIMapper_MetadataTypeDescription** outDescriptionList,
            size_t* outNumberOfDescriptions);
    int32_t (*dumpBuffer)(buffer_handle_t buffer,
                          AIMapper_DumpBufferCallback dumpBufferCallback,
                          void* context);
    int32_t (*dumpAllBuffers)(
            AIMapper_BeginDumpBufferCallback beginDumpCallback,
            AIMapper_DumpBufferCallback dumpBufferCallback,
            void* context);
    int32_t (*getReservedRegion)(buffer_handle_t buffer,
                                 void** outReservedRegion,
                                 uint64_t* outReservedSize);
} AIMapperV5;

typedef struct AIMapper {
    _Alignas(max_align_t) uint32_t version;
    AIMapperV5 v5;
} AIMapper;

static void guest_log(const char* msg) {
    int fd = open("/dev/kmsg", O_WRONLY | O_CLOEXEC);
    if (fd >= 0) {
        char buf[256];
        int len = snprintf(buf, sizeof(buf), "<6>T637_MAPPER %s\n", msg);
        if (len > 0) {
            (void)write(fd, buf, (size_t)len);
        }
        close(fd);
    }
}

static int32_t mapper_importBuffer(const native_handle_t* handle, buffer_handle_t* outBufferHandle) {
    guest_log("CALL method=importBuffer");
    if (!handle || !outBufferHandle) return AIMAPPER_ERROR_BAD_BUFFER;
    size_t total_ints = (size_t)(handle->numFds + handle->numInts);
    size_t alloc_sz = sizeof(native_handle_t) + total_ints * sizeof(int);
    native_handle_t* clone = (native_handle_t*)malloc(alloc_sz);
    if (!clone) return AIMAPPER_ERROR_NO_RESOURCES;
    clone->version = (int)sizeof(native_handle_t);
    clone->numFds = handle->numFds;
    clone->numInts = handle->numInts;
    for (int i = 0; i < handle->numFds; ++i) {
        clone->data[i] = (handle->data[i] >= 0) ? dup(handle->data[i]) : -1;
    }
    for (int i = 0; i < handle->numInts; ++i) {
        clone->data[handle->numFds + i] = handle->data[handle->numFds + i];
    }
    *outBufferHandle = clone;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_freeBuffer(buffer_handle_t buffer) {
    guest_log("CALL method=freeBuffer");
    if (!buffer) return AIMAPPER_ERROR_BAD_BUFFER;
    native_handle_t* h = (native_handle_t*)buffer;
    for (int i = 0; i < h->numFds; ++i) {
        if (h->data[i] >= 0) {
            close(h->data[i]);
            h->data[i] = -1;
        }
    }
    free(h);
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_getTransportSize(buffer_handle_t buffer, uint32_t* outNumFds, uint32_t* outNumInts) {
    if (!buffer || !outNumFds || !outNumInts) return AIMAPPER_ERROR_BAD_BUFFER;
    *outNumFds = (uint32_t)buffer->numFds;
    *outNumInts = (uint32_t)buffer->numInts;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_lock(buffer_handle_t buffer, uint64_t cpuUsage,
                           ARect accessRegion, int acquireFence,
                           void** outData) {
    (void)cpuUsage;
    (void)accessRegion;
    guest_log("CALL method=lock");
    if (acquireFence >= 0) {
        close(acquireFence);
    }
    if (!buffer || buffer->numFds < 1 || !outData) return AIMAPPER_ERROR_BAD_BUFFER;
    int fd = buffer->data[0];
    if (fd < 0) return AIMAPPER_ERROR_BAD_BUFFER;
    off_t size = lseek(fd, 0, SEEK_END);
    if (size <= 0) size = 1024 * 768 * 4;
    void* ptr = mmap(NULL, (size_t)size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (ptr == MAP_FAILED) {
        guest_log("ERROR method=lock mmap-failed");
        return AIMAPPER_ERROR_NO_RESOURCES;
    }
    *outData = ptr;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_unlock(buffer_handle_t buffer, int* releaseFence) {
    guest_log("CALL method=unlock");
    if (!buffer) return AIMAPPER_ERROR_BAD_BUFFER;
    if (releaseFence) *releaseFence = -1;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_flushLockedBuffer(buffer_handle_t buffer) {
    if (!buffer) return AIMAPPER_ERROR_BAD_BUFFER;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_rereadLockedBuffer(buffer_handle_t buffer) {
    if (!buffer) return AIMAPPER_ERROR_BAD_BUFFER;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_getStandardMetadata(buffer_handle_t buffer,
                                          int64_t standardMetadataType,
                                          void* destBuffer, size_t destBufferSize) {
    if (!buffer) return -2; /* -AIMAPPER_ERROR_BAD_BUFFER */

    switch (standardMetadataType) {
        case 1: { /* BUFFER_ID (uint64_t) */
            uint64_t val = (uint64_t)(uintptr_t)buffer;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 2: { /* NAME (string) */
            const char* name = "t637-buffer";
            size_t len = strlen(name) + 1;
            if (!destBuffer || destBufferSize < len) return (int32_t)len;
            memcpy(destBuffer, name, len);
            return (int32_t)len;
        }
        case 3: { /* WIDTH (uint64_t) */
            uint64_t val = 1024;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 4: { /* HEIGHT (uint64_t) */
            uint64_t val = 768;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 5: { /* LAYER_COUNT (uint64_t) */
            uint64_t val = 1;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 6: { /* PIXEL_FORMAT_REQUESTED (int32_t: 1 = RGBA_8888) */
            int32_t val = 1;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 7: { /* PIXEL_FORMAT_FOURCC (uint32_t) */
            uint32_t val = 0x34324152; /* 'RA24' */
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 8: { /* PIXEL_FORMAT_MODIFIER (uint64_t: 0 = LINEAR) */
            uint64_t val = 0;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 9: { /* USAGE (uint64_t) */
            uint64_t val = 0x303;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 10: { /* ALLOCATION_SIZE (uint64_t) */
            uint64_t val = 1024 * 768 * 4;
            if (buffer->numFds > 0 && buffer->data[0] >= 0) {
                off_t s = lseek(buffer->data[0], 0, SEEK_END);
                if (s > 0) val = (uint64_t)s;
            }
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 11: { /* PROTECTED_CONTENT (uint64_t) */
            uint64_t val = 0;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 14: { /* DATASPACE (int32_t) */
            int32_t val = 0;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 15: { /* BLEND_MODE (int32_t) */
            int32_t val = 1;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        case 23: { /* STRIDE (uint64_t) */
            uint64_t val = 1024;
            if (!destBuffer || destBufferSize < sizeof(val)) return (int32_t)sizeof(val);
            memcpy(destBuffer, &val, sizeof(val));
            return (int32_t)sizeof(val);
        }
        default:
            return -7; /* -AIMAPPER_ERROR_UNSUPPORTED */
    }
}

static int32_t mapper_getMetadata(buffer_handle_t buffer,
                                  AIMapper_MetadataType metadataType,
                                  void* destBuffer, size_t destBufferSize) {
    if (metadataType.name && strcmp(metadataType.name, "android.hardware.graphics.common.StandardMetadataType") == 0) {
        return mapper_getStandardMetadata(buffer, metadataType.value, destBuffer, destBufferSize);
    }
    return -7; /* -AIMAPPER_ERROR_UNSUPPORTED */
}

static int32_t mapper_setMetadata(buffer_handle_t buffer,
                                  AIMapper_MetadataType metadataType,
                                  const void* metadata, size_t metadataSize) {
    (void)buffer;
    (void)metadataType;
    (void)metadata;
    (void)metadataSize;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_setStandardMetadata(buffer_handle_t buffer,
                                          int64_t standardMetadataType,
                                          const void* metadata, size_t metadataSize) {
    (void)buffer;
    (void)standardMetadataType;
    (void)metadata;
    (void)metadataSize;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_listSupportedMetadataTypes(
        const AIMapper_MetadataTypeDescription** outDescriptionList,
        size_t* outNumberOfDescriptions) {
    if (!outDescriptionList || !outNumberOfDescriptions) return AIMAPPER_ERROR_BAD_VALUE;
    *outDescriptionList = NULL;
    *outNumberOfDescriptions = 0;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_dumpBuffer(buffer_handle_t buffer,
                                 AIMapper_DumpBufferCallback dumpBufferCallback,
                                 void* context) {
    (void)buffer;
    (void)dumpBufferCallback;
    (void)context;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_dumpAllBuffers(
        AIMapper_BeginDumpBufferCallback beginDumpCallback,
        AIMapper_DumpBufferCallback dumpBufferCallback,
        void* context) {
    (void)beginDumpCallback;
    (void)dumpBufferCallback;
    (void)context;
    return AIMAPPER_ERROR_NONE;
}

static int32_t mapper_getReservedRegion(buffer_handle_t buffer,
                                         void** outReservedRegion,
                                         uint64_t* outReservedSize) {
    (void)buffer;
    if (outReservedRegion) *outReservedRegion = NULL;
    if (outReservedSize) *outReservedSize = 0;
    return AIMAPPER_ERROR_NONE;
}

static AIMapper gMapper = {
    .version = AIMAPPER_VERSION_5,
    .v5 = {
        .importBuffer = mapper_importBuffer,
        .freeBuffer = mapper_freeBuffer,
        .getTransportSize = mapper_getTransportSize,
        .lock = mapper_lock,
        .unlock = mapper_unlock,
        .flushLockedBuffer = mapper_flushLockedBuffer,
        .rereadLockedBuffer = mapper_rereadLockedBuffer,
        .getMetadata = mapper_getMetadata,
        .getStandardMetadata = mapper_getStandardMetadata,
        .setMetadata = mapper_setMetadata,
        .setStandardMetadata = mapper_setStandardMetadata,
        .listSupportedMetadataTypes = mapper_listSupportedMetadataTypes,
        .dumpBuffer = mapper_dumpBuffer,
        .dumpAllBuffers = mapper_dumpAllBuffers,
        .getReservedRegion = mapper_getReservedRegion,
    }
};

__attribute__((visibility("default")))
int32_t AIMapper_loadIMapper(AIMapper** outImplementation) {
    guest_log("AIMapper_loadIMapper called; returning AIMapper v5 (T637 fix active)");
    if (!outImplementation) return AIMAPPER_ERROR_BAD_VALUE;
    *outImplementation = &gMapper;
    return AIMAPPER_ERROR_NONE;
}

#ifdef __cplusplus
}
#endif
