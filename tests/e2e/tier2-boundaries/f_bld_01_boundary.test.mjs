import { describe, test } from 'node:test';
import { readFileSync } from 'node:fs';
import { assertEqual, assertThrows, assertRejects, assertOk } from '../harness/assertions.mjs';
// Production import: the real generated runtime bundle.
import * as prodBundle from '../../../pkg/android_vm.js';

function validateProductionExports(mod) {
  if (!mod || typeof mod !== 'object') {
    throw new Error('MODULE_NOT_AN_OBJECT');
  }
  if (typeof mod.default !== 'function') {
    throw new Error('BUNDLE_MISSING_DEFAULT_INIT_WASM');
  }
  if (typeof mod.createAndroidRuntime !== 'function') {
    throw new Error('BUNDLE_MISSING_CREATE_ANDROID_RUNTIME');
  }
  return true;
}

function validateWasmBinary(buffer) {
  if (!buffer || !(buffer instanceof Uint8Array || buffer instanceof ArrayBuffer)) {
    throw new TypeError('WASM_BUFFER_INVALID_TYPE');
  }
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  if (bytes.length < 8) {
    throw new RangeError(`WASM_BINARY_TRUNCATED:length=${bytes.length}`);
  }
  // Magic: \0asm -> 0x00, 0x61, 0x73, 0x6d
  if (bytes[0] !== 0x00 || bytes[1] !== 0x61 || bytes[2] !== 0x73 || bytes[3] !== 0x6d) {
    throw new Error(`INVALID_WASM_MAGIC:[${bytes[0]},${bytes[1]},${bytes[2]},${bytes[3]}]`);
  }
  // Version: 1 -> 0x01, 0x00, 0x00, 0x00
  if (bytes[4] !== 0x01 || bytes[5] !== 0x00 || bytes[6] !== 0x00 || bytes[7] !== 0x00) {
    throw new Error(`UNSUPPORTED_WASM_VERSION:[${bytes[4]},${bytes[5]},${bytes[6]},${bytes[7]}]`);
  }
  return true;
}

async function mockCreateAndroidRuntime(options) {
  return prodBundle.createAndroidRuntime(options);
}

describe('Tier 2: F-BLD-01 Boundary & Corner Cases (production)', () => {
  test('F-BLD-01-B01: bundle missing default initWasm throws BUNDLE_MISSING_DEFAULT_INIT_WASM', () => {
    const invalidMod = { createAndroidRuntime: async () => ({}) };
    assertThrows(() => validateProductionExports(invalidMod), /BUNDLE_MISSING_DEFAULT_INIT_WASM/);
  });

  test('F-BLD-01-B02: bundle missing createAndroidRuntime throws BUNDLE_MISSING_CREATE_ANDROID_RUNTIME', () => {
    const invalidMod = { default: async () => {} };
    assertThrows(() => validateProductionExports(invalidMod), /BUNDLE_MISSING_CREATE_ANDROID_RUNTIME/);
  });

  test('F-BLD-01-B03: truncated WASM binary (<8 bytes) throws WASM_BINARY_TRUNCATED', () => {
    const truncated = new Uint8Array([0x00, 0x61, 0x73]);
    assertThrows(() => validateWasmBinary(truncated), /WASM_BINARY_TRUNCATED/);
    const realWasm = new Uint8Array(readFileSync(new URL('../../../pkg/android_vm_bg.wasm', import.meta.url)));
    assertEqual(validateWasmBinary(realWasm), true);
  });

  test('F-BLD-01-B04: corrupt WASM magic or unsupported version throws validation error', () => {
    const corruptMagic = new Uint8Array([0x7f, 0x45, 0x4c, 0x46, 0x01, 0x00, 0x00, 0x00]); // ELF header
    assertThrows(() => validateWasmBinary(corruptMagic), /INVALID_WASM_MAGIC/);

    const corruptVersion = new Uint8Array([0x00, 0x61, 0x73, 0x6d, 0x02, 0x00, 0x00, 0x00]); // Version 2
    assertThrows(() => validateWasmBinary(corruptVersion), /UNSUPPORTED_WASM_VERSION/);
  });

  test('F-BLD-01-B05: production createAndroidRuntime rejects missing canvas', async () => {
    await assertRejects(mockCreateAndroidRuntime(null), /RUNTIME_FACTORY_MISSING/);
    await assertRejects(mockCreateAndroidRuntime({ canvas: null }), /RUNTIME_FACTORY_MISSING/);
    await assertRejects(mockCreateAndroidRuntime({}), /RUNTIME_FACTORY_MISSING/);
  });
});
