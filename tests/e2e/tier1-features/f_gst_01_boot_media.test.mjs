import { describe, test } from 'node:test';
import { readFileSync } from 'node:fs';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real GuestImageStore + BootManager verify real media.
import { GuestImageStore } from '../../../src/storage/image_store.js';
import { BootManager } from '../../../src/boot/boot_manager.js';
import { GuestMem } from '../../../src/vm/guest_mem.js';

function loadPinnedManifest() {
  return JSON.parse(readFileSync(new URL('../../../images/manifest.json', import.meta.url), 'utf8'));
}

describe('Tier 1: F-GST-01 Android Guest Boot Media & Manifest (production)', () => {
  test('F-GST-01-01: pinned manifest targets android-x86 9.0-r2 / x86 / 4.19.110', () => {
    const manifest = loadPinnedManifest();
    assertEqual(manifest.androidVersion, '9.0-r2');
    assertEqual(manifest.abi, 'x86');
    assertEqual(manifest.kernelVersion, '4.19.110-android-x86_64');
    assertOk(manifest.sha256 && typeof manifest.sha256 === 'object');
  });

  test('F-GST-01-02: production GuestImageStore verifies the real kernel bytes', async () => {
    const manifest = loadPinnedManifest();
    const store = new GuestImageStore({ manifest });
    const kernel = new Uint8Array(readFileSync(new URL('../../../images/kernel', import.meta.url)));
    const receipt = await store.verify('kernel', kernel);
    assertEqual(receipt.role, 'kernel');
    assertEqual(receipt.bytes, manifest.images.kernel.size);
  });

  test('F-GST-01-03: production BootManager parses the real bzImage header', () => {
    const kernel = new Uint8Array(readFileSync(new URL('../../../images/kernel', import.meta.url)));
    const mem = new GuestMem(256 * 1024 * 1024);
    const header = new BootManager(mem).parseSetupHeader(kernel);
    assertOk(header.version >= 0x0202);
    assertEqual(header.headerMagic, 0x53726448);
  });

  test('F-GST-01-04: fixture roles are still labeled non-authentic (no false provenance)', () => {
    const manifest = loadPinnedManifest();
    for (const role of ['system.img', 'vendor.img', 'product.img']) {
      assertOk(!manifest.images[role]?.authentic, `${role} must not claim authentic`);
    }
    assertOk((manifest.note || '').length > 0);
  });

  test('F-GST-01-05: block-role image sizes are 512-byte aligned (RAM-loaded kernel/initrd exempt)', () => {
    const manifest = loadPinnedManifest();
    for (const [role, entry] of Object.entries(manifest.images)) {
      if (role === 'kernel' || role === 'initrd.img') continue;
      assertEqual(entry.size % 512, 0, `Image ${role} not 512-byte aligned`);
    }
  });
});
