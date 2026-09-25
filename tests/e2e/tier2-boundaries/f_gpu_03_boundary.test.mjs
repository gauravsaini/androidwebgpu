import { describe, test } from 'node:test';
import { assertEqual } from '../harness/assertions.mjs';
// Production imports: real BrowserHost + VirtioGpu from src/.
import { BrowserHost } from '../../../src/host/browser_host.js';
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_INVALID_PARAMETER, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';

describe('Tier 2: F-GPU-03 Boundary & Corner Cases (production)', () => {
  test('F-GPU-03-B01: canvas without WebGPU fails closed as UNSUPPORTED', () => {
    const host = new BrowserHost({ canvas: {} });
    let code = '';
    try {
      host.openCanvas();
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'UNSUPPORTED');
  });

  test('F-GPU-03-B02: nonzero scanout ID is rejected as invalid parameter', () => {
    const gpu = new VirtioGpu();
    const set = new Uint8Array(48);
    const v = new DataView(set.buffer);
    v.setUint32(32, 7, true); // scanoutId 7: only display 0 exists
    v.setUint32(36, 0, true);
    v.setUint32(40, 64, true);
    v.setUint32(44, 64, true);
    const res = gpu.processControlPacket({ type: 0x0103, payload: set });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_PARAMETER);
  });

  test('F-GPU-03-B03: scanout of a missing resource is rejected', () => {
    const gpu = new VirtioGpu();
    const set = new Uint8Array(48);
    const v = new DataView(set.buffer);
    v.setUint32(32, 0, true);
    v.setUint32(36, 31337, true);
    const res = gpu.processControlPacket({ type: 0x0103, payload: set });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('F-GPU-03-B04: support probe never claims WebGPU where absent', () => {
    const host = new BrowserHost({});
    assertEqual(host.checkSupport().webgpu, false);
  });

  test('F-GPU-03-B05: scanout zero detaches cleanly (resource 0 clears display)', () => {
    const gpu = new VirtioGpu();
    const set = new Uint8Array(48);
    const v = new DataView(set.buffer);
    v.setUint32(32, 0, true);
    v.setUint32(36, 0, true); // resource 0: detach
    const res = gpu.processControlPacket({ type: 0x0103, payload: set });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertEqual(gpu.scanouts.get(0).resourceId, 0);
  });
});
