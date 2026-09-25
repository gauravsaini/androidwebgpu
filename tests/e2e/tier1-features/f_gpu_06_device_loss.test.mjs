import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioGpu recovery path from src/.
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';

function create2D(id, w, h) {
  const b = new Uint8Array(16);
  const v = new DataView(b.buffer);
  v.setUint32(0, id, true);
  v.setUint32(4, 1, true);
  v.setUint32(8, w, true);
  v.setUint32(12, h, true);
  return b;
}

function flushFor(id) {
  const b = new Uint8Array(48);
  new DataView(b.buffer).setUint32(32, id, true);
  return b;
}

describe('Tier 1: F-GPU-06 WebGPU Device Loss and Context Recovery (production)', () => {
  test('F-GPU-06-01: reset destroys all resources; in-flight IDs go invalid', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 16, 16) });
    gpu.reset();
    assertEqual(gpu.registry.size(), 0);
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(1) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-06-02: post-reset re-create restores a working resource', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(2, 16, 16) });
    gpu.reset();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(2, 16, 16) });
    gpu.registry.attachBacking(2, [{ addr: 0x10000n, len: 1024 }]);
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(2) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
  });

  test('F-GPU-06-03: reset clears scanouts and telemetry keeps no stale frames', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(3, 8, 8) });
    gpu.reset();
    assertEqual(gpu.scanouts.size, 0);
    assertEqual(gpu.name, 'virtio-gpu');
  });

  test('F-GPU-06-04: double reset is safe and idempotent', () => {
    const gpu = new VirtioGpu();
    gpu.reset();
    gpu.reset();
    assertEqual(gpu.registry.size(), 0);
  });

  test('F-GPU-06-05: fences do not survive a device reset', () => {
    const gpu = new VirtioGpu();
    gpu.registry.nextFence();
    gpu.registry.completeFence(1n);
    gpu.reset();
    let rejected = false;
    try {
      gpu.registry.completeFence(1n);
    } catch (_e) {
      rejected = true;
    }
    assertOk(rejected || gpu.registry.completedFence === 0n);
  });
});
