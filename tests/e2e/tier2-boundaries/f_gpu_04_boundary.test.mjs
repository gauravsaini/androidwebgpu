import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioGpu 3D gate from src/.
import { VirtioGpu, GPU_RESP_ERR_UNSPEC } from '../../../src/gpu_transport/virtio_gpu.js';

describe('Tier 2: F-GPU-04 Boundary & Corner Cases (production)', () => {
  test('F-GPU-04-B01: zero-length SUBMIT_3D is rejected, never executed', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(0) });
    assertEqual(res.type, GPU_RESP_ERR_UNSPEC);
  });

  test('F-GPU-04-B02: out-of-range 3D opcode is rejected with telemetry', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x02ff, payload: new Uint8Array(8) });
    assertEqual(res.type, GPU_RESP_ERR_UNSPEC);
    assertEqual(gpu.telemetry.length, 1);
  });

  test('F-GPU-04-B03: CTX_DESTROY without a context errors instead of no-op', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0201, payload: new Uint8Array(0) });
    assertEqual(res.type, GPU_RESP_ERR_UNSPEC);
  });

  test('F-GPU-04-B04: TRANSFER_3D/FROM_HOST_3D are unsupported on this stack', () => {
    const gpu = new VirtioGpu();
    for (const opcode of [0x0205, 0x0206]) {
      assertEqual(gpu.processControlPacket({ type: opcode, payload: new Uint8Array(0) }).type, GPU_RESP_ERR_UNSPEC);
    }
  });

  test('F-GPU-04-B05: 3D rejections do not disturb live 2D resources', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(64).fill(0xff) });
    const b = new Uint8Array(16);
    const v = new DataView(b.buffer);
    v.setUint32(0, 11, true);
    v.setUint32(4, 1, true);
    v.setUint32(8, 32, true);
    v.setUint32(12, 32, true);
    assertEqual(gpu.processControlPacket({ type: 0x0101, payload: b }).type, 0x1100);
    assertOk(gpu.registry.has(11));
  });
});
