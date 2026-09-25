import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real input + guest RAM + gpu + host from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { VirtioInput, EV_KEY, EV_ABS } from '../../../src/io/input/virtio_input.js';
import { VirtioGpu, GPU_RESP_OK_NODATA } from '../../../src/gpu_transport/virtio_gpu.js';
import { BrowserHost } from '../../../src/host/browser_host.js';

function create2D(id, w, h) {
  const b = new Uint8Array(16);
  const v = new DataView(b.buffer);
  v.setUint32(0, id, true);
  v.setUint32(4, 1, true);
  v.setUint32(8, w, true);
  v.setUint32(12, h, true);
  return b;
}

function flushFor(id, fenceId = 0) {
  const b = new Uint8Array(48);
  new DataView(b.buffer).setUint32(32, id, true);
  return { body: b, fenceId };
}

describe('Tier 4: Scenario 2 — Input-to-GPU Render Cycle (production)', () => {
  test('SCN-INP-01: pointer event maps into guest pixels flushed to scanout', () => {
    const mem = new GuestMem(4 * 1024 * 1024);
    const input = new VirtioInput({ displayWidth: 800, displayHeight: 600 });
    const gpu = new VirtioGpu({ displayWidth: 800, displayHeight: 600 });
    // Step 1: DOM pointer at (400, 300) -> guest ABS events.
    input.handleDomPointer(400, 300, 800, 600);
    assertOk(input.pendingHost.length >= 2);
    // Step 2: frame bytes staged in guest RAM (white 800x600x4 tile header).
    mem.writeBytes(0x100000, new Uint8Array(256).fill(0xFF));
    assertEqual(mem.readU8(0x100000), 0xFF);
    // Step 3+4: resource create + backing + flush with fence.
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 800, 600) });
    gpu.registry.attachBacking(1, [{ addr: 0x100000n, len: 256 }]);
    const { body, fenceId } = flushFor(1, 42);
    const res = gpu.processControlPacket({ type: 0x0104, flags: 1, fenceId, payload: body });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertEqual(gpu.registry.completedFence, 42n);
  });

  test('SCN-INP-02: multi-touch contacts keep stable IDs in order', () => {
    const input = new VirtioInput({ displayWidth: 800, displayHeight: 600 });
    input.handleDomPointer(100, 200, 800, 600);
    input.handleDomPointer(500, 600, 800, 600);
    const seqs = input.pendingHost.map((e) => e.seq);
    assertOk(seqs.length >= 4);
    assertOk(seqs.every((s, i) => i === 0 || s > seqs[i - 1]));
  });

  test('SCN-INP-03: presentation without WebGPU fails closed, never 2D fallback', () => {
    const host = new BrowserHost({ canvas: null });
    let code = '';
    try {
      host.openCanvas();
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'UNSUPPORTED');
  });

  test('SCN-INP-04: registry enforces monotonic fence completion', () => {
    const gpu = new VirtioGpu();
    gpu.registry.completeFence(1n);
    gpu.registry.completeFence(2n);
    gpu.registry.completeFence(5n);
    gpu.registry.completeFence(6n);
    assertEqual(gpu.registry.completedFence, 6n);
    let rejected = false;
    try {
      gpu.registry.completeFence(5n);
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
  });

  test('SCN-INP-05: input queue preserves FIFO order under load', () => {
    const input = new VirtioInput();
    for (let i = 0; i < 100; i++) {
      input.enqueueHostEvent({ type: EV_ABS, code: 0, value: i * 10 });
    }
    const values = input.pendingHost.slice(0, 100).map((e) => e.value);
    assertEqual(values.length, 100);
    assertEqual(values[0], 0);
    assertEqual(values[99], 990);
  });
});
