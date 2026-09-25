import { describe, test } from 'node:test';
import { assertEqual } from '../harness/assertions.mjs';
// Production import: real VirtioGpu recovery from src/.
import { VirtioGpu, GPU_RESP_ERR_INVALID_RESOURCE_ID, GPU_RESP_ERR_INVALID_PARAMETER } from '../../../src/gpu_transport/virtio_gpu.js';

function flushFor(id) {
  const b = new Uint8Array(48);
  new DataView(b.buffer).setUint32(32, id, true);
  return b;
}

describe('Tier 2: F-GPU-06 Boundary & Corner Cases (production)', () => {
  test('F-GPU-06-B01: commands after destroyAll address an empty registry', () => {
    const gpu = new VirtioGpu();
    gpu.registry.destroyAll();
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(1) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-06-B02: repeated loss triggers stay safe (double destroy)', () => {
    const gpu = new VirtioGpu();
    gpu.registry.destroyAll();
    gpu.registry.destroyAll();
    assertEqual(gpu.registry.size(), 0);
  });

  test('F-GPU-06-B03: unref of a lost resource errors instead of crashing', () => {
    const gpu = new VirtioGpu();
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, 55, true);
    const res = gpu.processControlPacket({ type: 0x0102, payload: b });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-06-B04: truncated attach payload is rejected as invalid parameter', () => {
    const gpu = new VirtioGpu();
    const b = new Uint8Array(8);
    const v = new DataView(b.buffer);
    v.setUint32(0, 56, true);
    v.setUint32(4, 1, true); // claims 1 entry, sends none
    const res = gpu.processControlPacket({ type: 0x0106, payload: b });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_PARAMETER);
  });

  test('F-GPU-06-B05: reset restores a clean device ready for re-create', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(0) });
    gpu.reset();
    assertEqual(gpu.registry.size(), 0);
    assertEqual(gpu.telemetry.length, 1);
    assertEqual(gpu.name, 'virtio-gpu');
  });
});
