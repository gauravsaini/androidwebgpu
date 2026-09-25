import { describe, test } from 'node:test';
import { assertEqual, assertDeepEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real GuestMem from src/, not the harness oracle.
import { GuestMem } from '../../../src/vm/guest_mem.js';

describe('Tier 1: F-VM-02 Bounds-Checked GuestMem DMA View (production)', () => {
  const ramSize = 16 * 1024 * 1024; // 16MB

  test('F-VM-02-01: GuestMem allocates requested size and verifies initial configuration', () => {
    const mem = new GuestMem(ramSize);
    assertEqual(mem.ramSize, ramSize);
    assertEqual(mem.u8.byteLength, ramSize);
  });

  test('F-VM-02-02: read and write little-endian scalar primitives (U8, U16, U32)', () => {
    const mem = new GuestMem(ramSize);
    mem.writeU8(0x1000, 0xAB);
    assertEqual(mem.readU8(0x1000), 0xAB);

    mem.writeU16(0x2000, 0x1234);
    assertEqual(mem.readU16(0x2000), 0x1234);

    mem.writeU32(0x3000, 0xDEADBEEF);
    assertEqual(mem.readU32(0x3000), 0xDEADBEEF);
  });

  test('F-VM-02-03: block byte transfer writes and reads contiguous buffers', () => {
    const mem = new GuestMem(ramSize);
    const sourceData = new Uint8Array([10, 20, 30, 40, 50, 60, 70, 80]);
    mem.writeBytes(0x4000, sourceData);

    const readBack = mem.readBytes(0x4000, sourceData.length);
    assertDeepEqual(readBack, sourceData);
  });

  test('F-VM-02-04: zero fills pattern across physical memory segment', () => {
    const mem = new GuestMem(ramSize);
    mem.writeBytes(0x5000, new Uint8Array(256).fill(0x5A));
    for (let i = 0; i < 256; i++) {
      assertEqual(mem.readU8(0x5000 + i), 0x5A);
    }
    mem.zero(0x5000, 256);
    for (let i = 0; i < 256; i++) {
      assertEqual(mem.readU8(0x5000 + i), 0);
    }
  });

  test('F-VM-02-05: bounds enforced at lowest/highest byte; out-of-range rejected', () => {
    const mem = new GuestMem(ramSize);
    mem.writeU8(0, 0x42);
    assertEqual(mem.readU8(0), 0x42);

    mem.writeU8(ramSize - 1, 0x99);
    assertEqual(mem.readU8(ramSize - 1), 0x99);

    let rejected = false;
    try {
      mem.readU8(ramSize);
    } catch (_e) {
      rejected = true;
    }
    assertOk(rejected);
  });
});
