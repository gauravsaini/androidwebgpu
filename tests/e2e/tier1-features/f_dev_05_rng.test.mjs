import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioRng entropy path from src/.
import { VirtioRng, fillRandom } from '../../../src/io/rng/virtio_rng.js';

describe('Tier 1: F-DEV-05 VirtioRng Entropy Device (production)', () => {
  test('F-DEV-05-01: fillRandom produces non-degenerate bytes via platform CSPRNG', () => {
    const buf = new Uint8Array(64);
    const res = fillRandom(buf);
    assertEqual(res.source, 'crypto.getRandomValues');
    assertOk(buf.some((b) => b !== 0));
  });

  test('F-DEV-05-02: consecutive fills differ (not a fixed pattern)', () => {
    const a = new Uint8Array(32);
    const b = new Uint8Array(32);
    fillRandom(a);
    fillRandom(b);
    assertOk(!a.every((v, i) => v === b[i]));
  });

  test('F-DEV-05-03: device carries the entropy subsystem identity', () => {
    const dev = new VirtioRng();
    assertEqual(dev.name, 'virtio-rng');
    assertEqual(dev.subsystemDeviceId, 4);
    assertEqual(dev.bytesServed, 0);
  });

  test('F-DEV-05-04: large requests are served across the full span', () => {
    const buf = new Uint8Array(4096);
    fillRandom(buf);
    const head = buf.slice(0, 2048).some((b) => b !== 0);
    const tail = buf.slice(2048).some((b) => b !== 0);
    assertOk(head && tail);
  });

  test('F-DEV-05-05: reset preserves identity and clears nothing sensitive', () => {
    const dev = new VirtioRng();
    dev.reset();
    assertEqual(dev.name, 'virtio-rng');
    const buf = new Uint8Array(16);
    assertOk(fillRandom(buf).source.length > 0);
  });
});
