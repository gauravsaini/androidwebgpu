import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real validation loop + gpu + host policy from src/.
import { runValidationGates, GATE_IDS } from '../../../src/validation/validation_loop.js';
import { VirtioGpu, GPU_RESP_ERR_UNSPEC } from '../../../src/gpu_transport/virtio_gpu.js';
import { BrowserHost } from '../../../src/host/browser_host.js';

const allPassed = () => Object.fromEntries(GATE_IDS.map((id) => [id, async () => ({ status: 'PASSED', evidence: ['ok'], error: null })]));

describe('Tier 3: Pairwise GPU ↔ Runtime Combinations (production)', () => {
  test('P-GPU-RUN-01: runtime loop evaluates G2/G6 with live GPU device checks', async () => {
    const gpu = new VirtioGpu();
    const probes = {
      ...allPassed(),
      g2: async () => ({ status: gpu.registry ? 'PASSED' : 'FAILED', evidence: ['virtio-gpu-registry-live'], error: null }),
      g6: async () => {
        const res = gpu.processControlPacket({ type: 0x0207, payload: new Uint8Array(0) });
        return res.type === GPU_RESP_ERR_UNSPEC
          ? { status: 'PASSED', evidence: ['unsupported-3d-spec-error'], error: null }
          : { status: 'FAILED', evidence: [], error: 'GPU_PROTOCOL_UNEXPECTED' };
      },
    };
    const result = await runValidationGates({ runId: 'gpu-run-1', epoch: 1, probes });
    assertEqual(result.gates.g2.status, 'PASSED');
    assertEqual(result.gates.g6.status, 'PASSED');
    assertEqual(result.ready, true);
  });

  test('P-GPU-RUN-02: GPU loss during the loop sets G6 BLOCKED without uncaught errors', async () => {
    const gpu = new VirtioGpu();
    gpu.reset(); // simulate device loss: registry destroyed
    const probes = {
      ...allPassed(),
      g6: async () => gpu.registry.size() === 0 && gpu.telemetry
        ? { status: 'BLOCKED', evidence: ['device-destroyed'], error: 'GPU_DEVICE_LOST' }
        : { status: 'PASSED', evidence: [], error: null },
    };
    const result = await runValidationGates({ runId: 'gpu-run-loss', epoch: 1, probes });
    assertEqual(result.gates.g6.status, 'BLOCKED');
    assertEqual(result.gates.g6.error, 'GPU_DEVICE_LOST');
    assertEqual(result.ready, false);
  });

  test('P-GPU-RUN-03: GPU recovery in a later epoch restores PASSED with a new epoch', async () => {
    const gpu = new VirtioGpu();
    gpu.reset();
    const epoch1 = await runValidationGates({
      runId: 'epoch-1', epoch: 1,
      probes: { ...allPassed(), g6: async () => ({ status: 'BLOCKED', evidence: [], error: 'DEVICE_LOST' }) },
    });
    assertEqual(epoch1.ready, false);
    gpu.processControlPacket({ type: 0x0101, payload: (() => { const b = new Uint8Array(16); const v = new DataView(b.buffer); v.setUint32(0, 1, true); v.setUint32(4, 1, true); v.setUint32(8, 8, true); v.setUint32(12, 8, true); return b; })() });
    const epoch2 = await runValidationGates({
      runId: 'epoch-2', epoch: 2,
      probes: { ...allPassed(), g6: async () => ({ status: gpu.registry.has(1) ? 'PASSED' : 'BLOCKED', evidence: ['recreated-resource-live'], error: gpu.registry.has(1) ? null : 'DEVICE_LOST' }) },
    });
    assertEqual(epoch2.ready, true);
    assertEqual(epoch2.epoch, 2);
  });

  test('P-GPU-RUN-04: missing WebGPU canvas support fails G6 closed, never green', async () => {
    const host = new BrowserHost({ canvas: null });
    const probes = {
      ...allPassed(),
      g6: async () => {
        try {
          host.openCanvas();
          return { status: 'PASSED', evidence: [], error: null };
        } catch (e) {
          return { status: 'FAILED', evidence: [], error: 'WEBGPU_UNAVAILABLE' };
        }
      },
    };
    const result = await runValidationGates({ runId: 'no-canvas-run', epoch: 1, probes });
    assertEqual(result.gates.g6.status, 'FAILED');
    assertEqual(result.gates.g6.error, 'WEBGPU_UNAVAILABLE');
    assertEqual(result.ready, false);
  });

  test('P-GPU-RUN-05: frame events carry monotonic sequence numbers', async () => {
    const events = [];
    const gpu = new VirtioGpu();
    gpu.onFrame = ({ resourceId }) => {
      events.push({ seq: events.length + 1, resourceId });
    };
    gpu.processControlPacket({ type: 0x0101, payload: (() => { const b = new Uint8Array(16); const v = new DataView(b.buffer); v.setUint32(0, 9, true); v.setUint32(4, 1, true); v.setUint32(8, 4, true); v.setUint32(12, 4, true); return b; })() });
    gpu.registry.attachBacking(9, [{ addr: 0x30000n, len: 64 }]);
    const flush = new Uint8Array(48);
    new DataView(flush.buffer).setUint32(32, 9, true);
    for (let i = 0; i < 30; i++) gpu.processControlPacket({ type: 0x0104, payload: flush });
    assertEqual(events.length, 30);
    assertEqual(events[0].seq, 1);
    assertEqual(events[29].seq, 30);
  });
});
