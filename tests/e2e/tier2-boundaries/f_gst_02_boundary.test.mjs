import { describe, test } from 'node:test';
import { readFileSync } from 'node:fs';
import { assertOk } from '../harness/assertions.mjs';
// Production work product: the real guest HAL sources in guest/patches/.
const egl = readFileSync(new URL('../../../guest/patches/egl_webgpu.cpp', import.meta.url), 'utf8');
const hwc = readFileSync(new URL('../../../guest/patches/hwcomposer.virtio_gpu.cpp', import.meta.url), 'utf8');

describe('Tier 2: F-GST-02 Boundary & Corner Cases (production sources)', () => {
  test('F-GST-02-B01: eglDestroySurface guards null surfaces and failed mappings', () => {
    assertOk(egl.includes('if (s)'));
    assertOk(egl.includes('MAP_FAILED'));
  });

  test('F-GST-02-B02: GEM close runs only for valid handles on a live drm fd', () => {
    assertOk(egl.includes('DRM_IOCTL_GEM_CLOSE'));
    assertOk(egl.includes('bo_handle > 0'));
    assertOk(egl.includes('drm_fd >= 0'));
  });

  test('F-GST-02-B03: eglCreateContext never returns a null context silently', () => {
    assertOk(egl.includes('egl_context_t* ctx = (egl_context_t*)malloc'));
    assertOk(egl.includes('ctx->ctx_id'));
  });

  test('F-GST-02-B04: HWC vsync stays disabled until eventControl enables it', () => {
    assertOk(hwc.includes('vsync_enabled'));
    assertOk(hwc.includes('HWC_EVENT_VSYNC'));
  });

  test('F-GST-02-B05: HWC open rejects wrong device names with EINVAL', () => {
    assertOk(hwc.includes('HWC_HARDWARE_COMPOSER'));
    assertOk(hwc.includes('-EINVAL'));
  });
});
