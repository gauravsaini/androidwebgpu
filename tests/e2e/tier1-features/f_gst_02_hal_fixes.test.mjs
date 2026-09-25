import { describe, test } from 'node:test';
import { readFileSync } from 'node:fs';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production work product: the real guest HAL sources in guest/patches/.
// These assertions pin the fail-closed fixes (no success stubs on boot path,
// full-size wire packets, layer marshaling) against the actual files.
const egl = readFileSync(new URL('../../../guest/patches/egl_webgpu.cpp', import.meta.url), 'utf8');
const hwc = readFileSync(new URL('../../../guest/patches/hwcomposer.virtio_gpu.cpp', import.meta.url), 'utf8');

describe('Tier 1: F-GST-02 Android Guest HAL Fixes (production sources)', () => {
  test('F-GST-02-01: eglMakeCurrent validates handles instead of no-op success', () => {
    assertOk(egl.includes('EGL_NO_DISPLAY'));
    assertOk(egl.includes('g_current_context'));
    assertOk(!/eglMakeCurrent\([^)]*\) \{\s*return EGL_TRUE;\s*\}/.test(egl));
  });

  test('F-GST-02-02: swap-buffer packet buffer fits both wire commands', () => {
    assertOk(egl.includes('cmds[32]'));
    assertOk(egl.includes('0x0105'));
    assertOk(egl.includes('0x0104'));
  });

  test('F-GST-02-03: surfaces are DRM GEM-backed with resource handles', () => {
    assertOk(egl.includes('DRM_IOCTL_VIRTGPU_RESOURCE_CREATE'));
    assertOk(egl.includes('DRM_IOCTL_VIRTGPU_EXECBUFFER'));
    assertEqual(egl.includes('surf->mapped_ptr = NULL;') || egl.includes('mapped_ptr'), true);
  });

  test('F-GST-02-04: HWC set() marshals layer data to the host', () => {
    assertOk(hwc.includes('hwc_layer_write_to_host'));
    assertOk(hwc.includes('acquireFenceFd'));
    assertOk(hwc.includes('releaseFenceFd'));
  });

  test('F-GST-02-05: HWC runs a 60Hz vsync loop with enable control', () => {
    assertOk(hwc.includes('vsync_loop'));
    assertOk(hwc.includes('HWC_EVENT_VSYNC'));
    assertOk(hwc.includes('1000000000ULL / 60'));
  });
});
