import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real BrowserHost canvas policy from src/.
// (Node has no WebGPU: the fail-closed UNSUPPORTED path is the honest
// assertion here. Pixel presentation is proven in real browsers via G6.)
import { BrowserHost } from '../../../src/host/browser_host.js';

describe('Tier 1: F-GPU-03 Direct WebGPU Canvas Surface Present (production)', () => {
  test('F-GPU-03-01: missing canvas fails closed with UNSUPPORTED, never green', () => {
    const host = new BrowserHost({ canvas: null });
    let code = '';
    try {
      host.openCanvas();
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'UNSUPPORTED');
  });

  test('F-GPU-03-02: support probe reports booleans, never claims WebGPU in Node', () => {
    const host = new BrowserHost({});
    const sup = host.checkSupport();
    assertEqual(sup.webgpu, false);
    assertEqual(sup.wasm, true);
    assertOk(typeof sup.sab === 'boolean');
  });

  test('F-GPU-03-03: input sequencing is monotonic for frame/input correlation', () => {
    const host = new BrowserHost({});
    const a = host.readInput({ type: 'touch' });
    const b = host.readInput({ type: 'touch' });
    assertOk(b.seq > a.seq);
  });

  test('F-GPU-03-04: audio writes blocked before user activation', () => {
    const host = new BrowserHost({});
    let code = '';
    try {
      host.audioWrite(new Uint8Array(128));
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'AUDIO_BLOCKED');
  });

  test('F-GPU-03-05: monotonic clock is available for presentation timestamps', () => {
    const host = new BrowserHost({});
    const t = host.nowNs();
    assertEqual(typeof t, 'bigint');
    assertOk(t > 0n);
  });
});
