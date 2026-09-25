import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real registry + device from src/.
import { ResourceRegistry } from '../../../src/gpu_transport/resource_registry.js';
import { VirtioGpu, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';

function flushFor(id) {
  const b = new Uint8Array(48);
  new DataView(b.buffer).setUint32(32, id, true);
  return b;
}

describe('Tier 2: F-GPU-01 Boundary & Corner Cases (production)', () => {
  test('F-GPU-01-B01: duplicate resource ID is rejected', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 1, format: 1, width: 16, height: 16 });
    let rejected = false;
    try {
      reg.create2D({ id: 1, format: 1, width: 16, height: 16 });
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
  });

  test('F-GPU-01-B02: zero-dimension resource is rejected', () => {
    const reg = new ResourceRegistry();
    for (const [w, h] of [[0, 16], [16, 0], [0, 0]]) {
      let rejected = false;
      try {
        reg.create2D({ id: 100 + w + h, format: 1, width: w, height: h });
      } catch (_e) {
        rejected = true;
      }
      assertEqual(rejected, true);
    }
  });

  test('F-GPU-01-B03: empty backing list is rejected', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 2, format: 1, width: 8, height: 8 });
    let rejected = false;
    try {
      reg.attachBacking(2, []);
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
  });

  test('F-GPU-01-B04: scanout on unknown resource returns INVALID_RESOURCE_ID', () => {
    const gpu = new VirtioGpu();
    const set = new Uint8Array(48);
    const v = new DataView(set.buffer);
    v.setUint32(32, 0, true);
    v.setUint32(36, 4242, true);
    v.setUint32(40, 64, true);
    v.setUint32(44, 64, true);
    const res = gpu.processControlPacket({ type: 0x0103, payload: set });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-01-B05: detach without attach leaves null backing idempotently', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 3, format: 1, width: 8, height: 8 });
    reg.detachBacking(3);
    assertEqual(reg.get(3).backing, null);
    const gpu = new VirtioGpu();
    assertEqual(gpu.processControlPacket({ type: 0x0104, payload: flushFor(3) }).type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });
});
