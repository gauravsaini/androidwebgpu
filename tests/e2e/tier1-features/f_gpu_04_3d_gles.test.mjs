import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioGpu 3D-protocol gate from src/.
// Declared protocol is android-webgpu-gles-v1 for 2D/scanout; virgl/
// gfxstream opcodes correctly return spec errors + telemetry here.
import { VirtioGpu, GPU_RESP_ERR_UNSPEC } from '../../../src/gpu_transport/virtio_gpu.js';

describe('Tier 1: F-GPU-04 3D Opcode Slice & GLES Translation (production)', () => {
  test('F-GPU-04-01: SUBMIT_3D returns a spec error, never silent success', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(24) });
    assertEqual(res.type, GPU_RESP_ERR_UNSPEC);
  });

  test('F-GPU-04-02: CTX_CREATE is gated with telemetry', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0200, payload: new Uint8Array(0) });
    assertEqual(res.type, GPU_RESP_ERR_UNSPEC);
    assertEqual(gpu.telemetry[0].type, 0x0200);
  });

  test('F-GPU-04-03: unknown 3D-range opcodes are rejected uniformly', () => {
    const gpu = new VirtioGpu();
    for (const opcode of [0x0204, 0x0205, 0x0206]) {
      const res = gpu.processControlPacket({ type: opcode, payload: new Uint8Array(0) });
      assertEqual(res.type, GPU_RESP_ERR_UNSPEC);
    }
    assertEqual(gpu.telemetry.length, 3);
  });

  test('F-GPU-04-04: telemetry is bounded under opcode flood', () => {
    const gpu = new VirtioGpu();
    for (let i = 0; i < 600; i++) {
      gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(0) });
    }
    assertOk(gpu.telemetry.length <= 512);
  });

  test('F-GPU-04-05: 2D path is unaffected by 3D rejections', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(0) });
    const info = gpu.processControlPacket({ type: 0x0100, payload: new Uint8Array(0) });
    assertEqual(info.type, 0x1101);
  });
});
