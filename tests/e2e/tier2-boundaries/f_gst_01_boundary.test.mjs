import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real GuestImageStore from src/.
import { GuestImageStore } from '../../../src/storage/image_store.js';

function manifestWith(images) {
  return { androidVersion: '9.0-r2', abi: 'x86', images };
}

describe('Tier 2: F-GST-01 Boundary & Corner Cases (production)', () => {
  test('F-GST-01-B01: unknown image role throws IMAGE_ROLE_MISSING', async () => {
    const store = new GuestImageStore({ manifest: manifestWith({}) });
    let code = '';
    try {
      await store.verify('arm64-boot.img', new Uint8Array(512));
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'IMAGE_ROLE_MISSING');
  });

  test('F-GST-01-B02: missing product.img role blocks with the exact role', async () => {
    const store = new GuestImageStore({ manifest: manifestWith({}) });
    let message = '';
    try {
      store.get('product.img');
    } catch (e) {
      message = e.message;
    }
    assertOk(message.includes('product.img'));
  });

  test('F-GST-01-B03: tampered bytes fail SHA-256 with IMAGE_HASH_MISMATCH', async () => {
    const good = new Uint8Array(1024).map((_, i) => i & 0xff);
    const { createHash } = await import('node:crypto');
    const hash = createHash('sha256').update(good).digest('hex');
    const store = new GuestImageStore({
      manifest: manifestWith({ 'kernel': { file: 'kernel', size: 1024, sha256: hash } }),
    });
    const bad = Uint8Array.from(good);
    bad[0] ^= 0xff;
    let code = '';
    try {
      await store.verify('kernel', bad);
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'IMAGE_HASH_MISMATCH');
  });

  test('F-GST-01-B04: wrong byte size fails with IMAGE_SIZE_MISMATCH', async () => {
    const store = new GuestImageStore({
      manifest: manifestWith({ 'kernel': { file: 'kernel', size: 1024, sha256: 'x'.repeat(64) } }),
    });
    let code = '';
    try {
      await store.verify('kernel', new Uint8Array(512));
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'IMAGE_SIZE_MISMATCH');
  });

  test('F-GST-01-B05: null manifest blocks every role', async () => {
    const store = new GuestImageStore({ manifest: null });
    let code = '';
    try {
      await store.verify('kernel', new Uint8Array(512));
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'IMAGE_ROLE_MISSING');
  });
});
