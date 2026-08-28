#pragma once
#ifdef __cplusplus
extern "C" {
#endif

typedef struct native_handle {
    int version;        /* sizeof(native_handle_t) */
    int numFds;         /* number of file-descriptors at &data[0] */
    int numInts;        /* number of ints at &data[numFds] */
    int data[0];        /* data[0..numFds-1] are fds, and data[numFds..numFds+numInts-1] are ints */
} native_handle_t;

typedef const native_handle_t* buffer_handle_t;

#ifdef __cplusplus
}
#endif
