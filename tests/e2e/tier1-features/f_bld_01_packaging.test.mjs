import { describe, test } from 'node:test';
import { readFileSync, existsSync } from 'node:fs';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production artifacts: the real generated bundle on disk.

function validateProductionBundleExports(moduleExports) {
  if (typeof moduleExports.default !== 'function') {
    throw new Error('BUNDLE_MISSING_DEFAULT_INIT_WASM');
  }
  if (typeof moduleExports.createAndroidRuntime !== 'function') {
    throw new Error('BUNDLE_MISSING_CREATE_ANDROID_RUNTIME');
  }
  return true;
}

function validateWasmMagicHeader(bytes) {
  if (bytes.length < 4) throw new Error('WASM_BINARY_TOO_SHORT');
  if (bytes[0] !== 0x00 || bytes[1] !== 0x61 || bytes[2] !== 0x73 || bytes[3] !== 0x6d) {
    throw new Error('INVALID_WASM_MAGIC_HEADER');
  }
  return true;
}

describe('Tier 1: F-BLD-01 Runtime Artifact Packaging (production)', () => {
  test('F-BLD-01-01: pkg/android_vm.js carries the plan section 8 contract', () => {
    const src = readFileSync(new URL('../../../pkg/android_vm.js', import.meta.url), 'utf8');
    assertOk(src.includes('createAndroidRuntime'));
    assertOk(src.includes('runValidationLoop'));
    assertOk(src.includes('attachV86'));
  });

  test('F-BLD-01-02: generated module is importable with both entry points', async () => {
    const mod = await import('../../../pkg/android_vm.js');
    assertEqual(validateProductionBundleExports(mod), true);
  });

  test('F-BLD-01-03: real WASM artifact has the standard magic header', () => {
    const bytes = new Uint8Array(readFileSync(new URL('../../../pkg/android_vm_bg.wasm', import.meta.url)));
    assertEqual(validateWasmMagicHeader(bytes), true);
  });

  test('F-BLD-01-04: bundle lives at the contracted pkg/ path', () => {
    assertOk(existsSync(new URL('../../../pkg/android_vm.js', import.meta.url)));
    assertOk(existsSync(new URL('../../../pkg/android_vm_bg.wasm', import.meta.url)));
  });

  test('F-BLD-01-05: build manifest pins the v86 commit and media state', () => {
    const manifest = JSON.parse(readFileSync(new URL('../../../build-manifest.json', import.meta.url), 'utf8'));
    assertOk(typeof manifest.v86.commit === 'string' && manifest.v86.commit.length >= 7);
    assertOk(typeof manifest.android.version === 'string');
  });
});
