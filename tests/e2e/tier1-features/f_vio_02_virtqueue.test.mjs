import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real Virtqueue + GuestMem from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import {
  Virtqueue,
  VRING_DESC_F_NEXT,
  VRING_DESC_F_WRITE,
  VRING_DESC_SIZE,
} from '../../../src/virtio/virtqueue.js';

function setupQueue(size = 16) {
  const mem = new GuestMem(4 * 1024 * 1024);
  const vq = new Virtqueue(0, size);
  const descAddr = 0x10000n;
  const availAddr = 0x20000n;
  const usedAddr = 0x30000n;
  vq.descTableAddr = descAddr;
  vq.availRingAddr = availAddr;
  vq.usedRingAddr = usedAddr;
  vq.setEnabled(true);
  return { mem, vq, descAddr, availAddr, usedAddr };
}

function writeDesc(mem, base, idx, addr, len, flags, next) {
  const off = base + BigInt(idx * VRING_DESC_SIZE);
  mem.writeU64(off, BigInt(addr));
  mem.writeU32(off + 8n, len);
  mem.writeU16(off + 12n, flags);
  mem.writeU16(off + 14n, next);
}

describe('Tier 1: F-VIO-02 Virtqueue Split Ring Parser & Descriptor Engine (production)', () => {
  test('F-VIO-02-01: virtqueue initializes with valid power-of-2 queue size; non-integer rejected', () => {
    const { vq } = setupQueue(16);
    assertEqual(vq.size, 16);
    let rejected = false;
    try {
      vq.setSize(1.5);
    } catch (_e) {
      rejected = true;
    }
    assertOk(rejected);
  });

  test('F-VIO-02-02: pops single descriptor chain without NEXT flag', () => {
    const { mem, vq, availAddr } = setupQueue();
    writeDesc(mem, vq.descTableAddr, 0, 0x5000, 64, 0, 0);
    mem.writeU16(availAddr, 0);
    mem.writeU16(availAddr + 2n, 1);
    mem.writeU16(availAddr + 4n, 0);
    const chain = vq.popDescriptorChain(mem);
    assertOk(chain !== null);
    assertEqual(chain.headIndex, 0);
    assertEqual(chain.readable.length, 1);
  });

  test('F-VIO-02-03: traverses multi-element chain linked by VRING_DESC_F_NEXT', () => {
    const { mem, vq, availAddr } = setupQueue();
    writeDesc(mem, vq.descTableAddr, 0, 0x6000, 16, VRING_DESC_F_NEXT, 1);
    writeDesc(mem, vq.descTableAddr, 1, 0x6020, 32, VRING_DESC_F_WRITE, 0);
    mem.writeU16(availAddr, 0);
    mem.writeU16(availAddr + 2n, 1);
    mem.writeU16(availAddr + 4n, 0);
    const chain = vq.popDescriptorChain(mem);
    assertOk(chain !== null);
    assertEqual(chain.readable.length, 1);
    assertEqual(chain.writable.length, 1);
  });

  test('F-VIO-02-04: self-loop descriptor chain is rejected, not hung', () => {
    const { mem, vq, availAddr } = setupQueue();
    writeDesc(mem, vq.descTableAddr, 0, 0x7000, 16, VRING_DESC_F_NEXT, 0);
    mem.writeU16(availAddr, 0);
    mem.writeU16(availAddr + 2n, 1);
    mem.writeU16(availAddr + 4n, 0);
    let rejected = false;
    try {
      vq.popDescriptorChain(mem);
    } catch (e) {
      rejected = e.code === 'DESCRIPTOR_LOOP_DETECTED';
    }
    assertOk(rejected);
  });

  test('F-VIO-02-05: pushUsed records one used entry per consumed chain', () => {
    const { mem, vq, availAddr, usedAddr } = setupQueue();
    writeDesc(mem, vq.descTableAddr, 0, 0x8000, 48, VRING_DESC_F_WRITE, 0);
    mem.writeU16(availAddr, 0);
    mem.writeU16(availAddr + 2n, 1);
    mem.writeU16(availAddr + 4n, 0);
    const chain = vq.popDescriptorChain(mem);
    assertOk(chain !== null);
    vq.pushUsed(chain.headIndex, 48, mem);
    assertEqual(mem.readU16(usedAddr + 2n), 1);
    assertEqual(mem.readU32(usedAddr + 4n), 0);
    assertEqual(mem.readU32(usedAddr + 8n), 48);
  });
});
