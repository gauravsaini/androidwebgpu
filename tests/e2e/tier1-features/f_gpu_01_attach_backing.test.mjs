import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real resource registry + virtio-gpu device from src/.
import { ResourceRegistry } from '../../../src/gpu_transport/resource_registry.js';
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';

function create2DPayload(id, format, width, height) {
  const b = new Uint8Array(16);
  const v = new DataView(b.buffer);
  v.setUint32(0, id, true);
  v.setUint32(4, format, true);
  v.setUint32(8, width, true);
  v.setUint32(12, height, true);
  return b;
}

describe('Tier 1: F-GPU-01 Virtio-GPU DMA Backing Attach/Detach (production)', () => {
  test('F-GPU-01-01: resource create registers dimensions in the registry', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0101, payload: create2DPayload(1, 1, 64, 64) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertOk(gpu.registry.has(1));
  });

  test('F-GPU-01-02: duplicate resource IDs are rejected', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 7, format: 1, width: 32, height: 32 });
    let rejected = false;
    try {
      reg.create2D({ id: 7, format: 1, width: 32, height: 32 });
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
  });

  test('F-GPU-01-03: backing attach binds guest DMA entries; detach unmaps', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 3, format: 1, width: 16, height: 16 });
    const n = reg.attachBacking(3, [{ addr: 0x10000n, len: 4096 }]);
    assertEqual(n, 1);
    reg.detachBacking(3);
    assertEqual(reg.get(3).backing, null);
  });

  test('F-GPU-01-04: operations on unknown IDs return spec errors', () => {
    const gpu = new VirtioGpu();
    const res = gpu.processControlPacket({ type: 0x0102, payload: (() => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, 999, true); return b; })() });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-01-05: unref destroys the resource and releases backing', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 9, format: 1, width: 8, height: 8 });
    reg.attachBacking(9, [{ addr: 0x20000n, len: 512 }]);
    reg.unref(9);
    assertEqual(reg.has(9), false);
    assertEqual(reg.size(), 0);
  });
});
