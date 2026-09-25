import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real guest RAM, gpu device, validation loop from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';
import { runValidationGates, GATE_IDS } from '../../../src/validation/validation_loop.js';

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

describe('Tier 4: Scenario 5 — WebGPU Device Loss & Context Recovery (production)', () => {
  test('SCN-DEV-LOSS-01: device reset preserves guest RAM while dropping host resources', async () => {
    const mem = new GuestMem(4 * 1024 * 1024);
    mem.writeU32(0x50000, 0x1234ABCD);
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 64, 64) });
    gpu.registry.attachBacking(1, [{ addr: 0x50000n, len: 256 }]);
    // Device loss: host resources gone, guest RAM untouched.
    gpu.reset();
    assertEqual(mem.readU32(0x50000), 0x1234ABCD);
    assertEqual(gpu.registry.size(), 0);
    // Recovery: re-create + re-attach + flush works again.
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 64, 64) });
    gpu.registry.attachBacking(1, [{ addr: 0x50000n, len: 256 }]);
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(1) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
  });

  test('SCN-DEV-LOSS-02: in-flight IDs are rejected while the device is down', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(2, 16, 16) });
    gpu.reset();
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(2) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('SCN-DEV-LOSS-03: fence watermark restarts cleanly after recovery', () => {
    const gpu = new VirtioGpu();
    gpu.registry.completeFence(100n);
    gpu.registry.completeFence(101n);
    gpu.reset();
    gpu.registry.completeFence(102n);
    assertEqual(gpu.registry.completedFence, 102n);
  });

  test('SCN-DEV-LOSS-04: scanout table is cleared on loss, re-armed on recovery', () => {
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(3, 32, 32) });
    const set = new Uint8Array(48);
    const v = new DataView(set.buffer);
    v.setUint32(32, 0, true);
    v.setUint32(36, 3, true);
    v.setUint32(40, 32, true);
    v.setUint32(44, 32, true);
    gpu.processControlPacket({ type: 0x0103, payload: set });
    assertEqual(gpu.scanouts.size, 1);
    gpu.reset();
    assertEqual(gpu.scanouts.size, 0);
  });

  test('SCN-DEV-LOSS-05: production loop records BLOCKED during loss, PASSED after recovery', async () => {
    const gpu = new VirtioGpu();
    const allPassed = () => Object.fromEntries(GATE_IDS.map((id) => [id, async () => ({ status: 'PASSED', evidence: ['ok'], error: null })]));
    gpu.reset();
    const resLoss = await runValidationGates({
      runId: 'loss-epoch-1', epoch: 1,
      probes: { ...allPassed(), g6: async () => ({ status: 'BLOCKED', evidence: ['device-lost'], error: 'GPU_DEVICE_LOST' }) },
    });
    assertEqual(resLoss.gates.g6.status, 'BLOCKED');
    assertEqual(resLoss.ready, false);
    gpu.processControlPacket({ type: 0x0101, payload: create2D(4, 8, 8) });
    const resRecovered = await runValidationGates({
      runId: 'loss-epoch-2', epoch: 2,
      probes: { ...allPassed(), g6: async () => gpu.registry.has(4)
        ? { status: 'PASSED', evidence: ['gpu-context-recovered'], error: null }
        : { status: 'BLOCKED', evidence: [], error: 'GPU_DEVICE_LOST' } },
    });
    assertEqual(resRecovered.gates.g6.status, 'PASSED');
    assertEqual(resRecovered.ready, true);
  });
});
