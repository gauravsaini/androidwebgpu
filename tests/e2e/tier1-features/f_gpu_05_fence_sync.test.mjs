import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real registry + device from src/.
import { ResourceRegistry } from '../../../src/gpu_transport/resource_registry.js';
import { VirtioGpu, GPU_RESP_OK_NODATA } from '../../../src/gpu_transport/virtio_gpu.js';

describe('Tier 1: F-GPU-05 Monotonic Fence Completion & Work Sync (production)', () => {
  test('F-GPU-05-01: fence IDs are monotonic per allocation', () => {
    const reg = new ResourceRegistry();
    const a = reg.nextFence();
    const b = reg.nextFence();
    assertOk(b > a);
  });

  test('F-GPU-05-02: in-order fence completion advances the watermark', () => {
    const reg = new ResourceRegistry();
    reg.completeFence(1n);
    reg.completeFence(2n);
    assertEqual(reg.completedFence, 2n);
  });

  test('F-GPU-05-03: out-of-order fence completion is rejected', () => {
    const reg = new ResourceRegistry();
    reg.completeFence(5n);
    let rejected = false;
    try {
      reg.completeFence(5n);
    } catch (e) {
      rejected = e.code === 'GPU_FENCE_OUT_OF_ORDER';
    }
    assertEqual(rejected, true);
  });

  test('F-GPU-05-04: flush with fence flag completes the fence', () => {
    const gpu = new VirtioGpu();
    const b = new Uint8Array(16);
    const v = new DataView(b.buffer);
    v.setUint32(0, 1, true);
    v.setUint32(4, 1, true);
    v.setUint32(8, 8, true);
    v.setUint32(12, 8, true);
    gpu.processControlPacket({ type: 0x0101, payload: b });
    gpu.registry.attachBacking(1, [{ addr: 0x10000n, len: 256 }]);
    const f = new Uint8Array(48);
    const fv = new DataView(f.buffer);
    fv.setUint32(32, 1, true);
    const res = gpu.processControlPacket({ type: 0x0104, flags: 1, fenceId: 1, payload: f });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertEqual(gpu.registry.completedFence, 1n);
  });

  test('F-GPU-05-05: telemetry records unsupported 3D protocol attempts', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(0) });
    assertEqual(gpu.telemetry.length, 1);
    assertEqual(gpu.telemetry[0].code, 'GPU_PROTOCOL_UNSUPPORTED');
  });
});
