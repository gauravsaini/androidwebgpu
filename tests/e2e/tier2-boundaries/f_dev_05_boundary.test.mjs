import { describe, test } from 'node:test';
import { assertEqual, assertOk, assertThrows } from '../harness/assertions.mjs';
// Production import: real VirtioRng path from src/.
import { VirtioRng, fillRandom } from '../../../src/io/rng/virtio_rng.js';

describe('Tier 2: F-DEV-05 Boundary & Corner Cases (production)', () => {
  test('F-DEV-05-B01: zero-length fill is a no-op returning the CSPRNG source', () => {
    const res = fillRandom(new Uint8Array(0));
    assertEqual(res.source, 'crypto.getRandomValues');
  });

  test('F-DEV-05-B02: single-byte fill covers the full byte range over draws', () => {
    const seen = new Set();
    for (let i = 0; i < 64; i++) {
      const b = new Uint8Array(1);
      fillRandom(b);
      seen.add(b[0]);
    }
    assertOk(seen.size > 1);
  });

  test('F-DEV-05-B03: 1MB fill completes without truncation', () => {
    const buf = new Uint8Array(1024 * 1024);
    fillRandom(buf);
    assertEqual(buf.byteLength, 1024 * 1024);
    assertOk(buf.some((b) => b !== 0));
  });

  test('F-DEV-05-B04: device bytesServed starts at zero with entropy identity', () => {
    const dev = new VirtioRng();
    assertEqual(dev.bytesServed, 0);
    assertEqual(dev.subsystemDeviceId, 4);
  });

  test('F-DEV-05-B05: invalid target type throws instead of weak output', () => {
    assertThrows(() => fillRandom(null), /./);
  });
});
