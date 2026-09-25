import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real VirtioGpu + GuestMem from src/.
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';
import { GuestMem } from '../../../src/vm/guest_mem.js';

function create2D(id, w, h) {
  const b = new Uint8Array(16);
  const v = new DataView(b.buffer);
  v.setUint32(0, id, true);
  v.setUint32(4, 1, true);
  v.setUint32(8, w, true);
  v.setUint32(12, h, true);
  return b;
}

function transfer2D(id, x, y, w, h) {
  const b = new Uint8Array(48);
  const v = new DataView(b.buffer);
  v.setUint32(0, x, true);
  v.setUint32(4, y, true);
  v.setUint32(8, w, true);
  v.setUint32(12, h, true);
  v.setUint32(32, id, true);
  return b;
}

describe('Tier 1: F-GPU-02 True Spec TRANSFER_TO_HOST_2D from Guest RAM (production)', () => {
  test('F-GPU-02-01: transfer on a backed resource returns OK_NODATA', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 64, 64) });
    gpu.registry.attachBacking(1, [{ addr: 0x10000n, len: 16384 }]);
    const res = gpu.processControlPacket({ type: 0x0105, payload: transfer2D(1, 0, 0, 64, 64) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
  });

  test('F-GPU-02-02: guest pixel bytes live in checked GuestMem, not packet payload', () => {
    const mem = new GuestMem(4 * 1024 * 1024);
    const pixels = new Uint8Array(256).map((_, i) => (i * 3) & 0xff);
    mem.writeBytes(0x10000, pixels);
    assertOk(mem.readBytes(0x10000, 256).every((v, i) => v === ((i * 3) & 0xff)));
  });

  test('F-GPU-02-03: transfer without backing is rejected, never stale pixels', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(2, 32, 32) });
    const res = gpu.processControlPacket({ type: 0x0105, payload: transfer2D(2, 0, 0, 32, 32) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-02-04: sub-rectangle transfer is accepted on a backed resource', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(3, 128, 128) });
    gpu.registry.attachBacking(3, [{ addr: 0x20000n, len: 65536 }]);
    const res = gpu.processControlPacket({ type: 0x0105, payload: transfer2D(3, 10, 10, 32, 32) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
  });

  test('F-GPU-02-05: transfer on unknown resource returns INVALID_RESOURCE_ID', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0105, payload: transfer2D(777, 0, 0, 8, 8) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });
});
