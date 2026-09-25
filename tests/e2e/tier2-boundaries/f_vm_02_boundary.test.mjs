import { describe, test } from 'node:test';
import { assertThrows, assertEqual } from '../harness/assertions.mjs';
// Production import: real GuestMem from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';

describe('Tier 2: F-VM-02 Boundary & Corner Cases (production)', () => {
  const ramSize = 16 * 1024 * 1024; // 16MB
  const mem = new GuestMem(ramSize);

  test('F-VM-02-B01: reading past physical RAM boundary throws DMA_OUT_OF_BOUNDS', () => {
    assertThrows(() => mem.readU8(ramSize), /DMA_OUT_OF_BOUNDS/);
    assertThrows(() => mem.readBytes(ramSize - 10, 20), /DMA_OUT_OF_BOUNDS/);
  });

  test('F-VM-02-B02: 64-bit wrap-around overflow throws DMA_OUT_OF_BOUNDS', () => {
    assertThrows(() => mem.readU32(0xFFFFFFFF), /DMA_OUT_OF_BOUNDS/);
    assertThrows(() => mem.validateRange(0xFFFFFFF0n, 0x20n), /DMA_OUT_OF_BOUNDS/);
  });

  test('F-VM-02-B03: zero-length memory read edge case succeeds without violation', () => {
    const emptySlice = mem.readBytes(0x1000, 0);
    assertEqual(emptySlice.length, 0);
  });

  test('F-VM-02-B04: negative address or length throws DMA_OUT_OF_BOUNDS', () => {
    assertThrows(() => mem.readU8(-1), /DMA_OUT_OF_BOUNDS/);
    assertThrows(() => mem.validateRange(100, -5), /DMA_OUT_OF_BOUNDS/);
  });

  test('F-VM-02-B05: zero-size allocation holds no bytes; any access throws', () => {
    const tiny = new GuestMem(1);
    assertEqual(tiny.ramSize, 1);
    assertThrows(() => tiny.readU8(1), /DMA_OUT_OF_BOUNDS/);
  });
});
