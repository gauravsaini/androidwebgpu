import { describe, test } from 'node:test';
import { assertEqual } from '../harness/assertions.mjs';
// Production import: real VirtioGpu packet path from src/.
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

describe('Tier 2: F-GPU-02 Boundary & Corner Cases (production)', () => {
  test('F-GPU-02-B01: flush without attached backing returns INVALID_RESOURCE_ID', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 800, 600) });
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(1) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-02-B02: flush on unknown resource returns INVALID_RESOURCE_ID', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(4242) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-02-B03: flush after attach Backing succeeds', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(2, 64, 64) });
    gpu.registry.attachBacking(2, [{ addr: 0x10000n, len: 16384 }]);
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(2) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
  });

  test('F-GPU-02-B04: detach re-arms the flush guard', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(3, 64, 64) });
    gpu.registry.attachBacking(3, [{ addr: 0x10000n, len: 16384 }]);
    gpu.registry.detachBacking(3);
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(3) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-02-B05: zero-dimension create is rejected, never a NO_OP resource', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0101, payload: create2D(4, 0, 64) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
    assertEqual(gpu.registry.has(4), false);
  });
});
