#pragma once
#include <stdio.h>

#ifndef ALOGE
#define ALOGE(...) fprintf(stderr, __VA_ARGS__)
#endif
#ifndef ALOGD
#define ALOGD(...) ((void)0)
#endif
#ifndef ALOGI
#define ALOGI(...) ((void)0)
#endif
#ifndef ALOGW
#define ALOGW(...) fprintf(stderr, __VA_ARGS__)
#endif
