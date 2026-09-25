import { describe, test } from 'node:test';
import { assertThrows, assertEqual } from '../harness/assertions.mjs';
// Production imports: real Virtqueue + GuestMem from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { Virtqueue, VRING_DESC_F_NEXT, VRING_DESC_SIZE } from '../../../src/virtio/virtqueue.js';

function setupQueue() {
  const mem = new GuestMem(4 * 1024 * 1024);
  const vq = new Virtqueue(0, 16);
  vq.descTableAddr = 0x10000n;
  vq.availRingAddr = 0x20000n;
  vq.usedRingAddr = 0x30000n;
  vq.setEnabled(true);
  return { mem, vq };
}

function writeDesc(mem, idx, addr, len, flags, next) {
  const off = 0x10000n + BigInt(idx * VRING_DESC_SIZE);
  mem.writeU64(off, BigInt(addr));
  mem.writeU32(off + 8n, len);
  mem.writeU16(off + 12n, flags);
  mem.writeU16(off + 14n, next);
}

function armAvail(mem, head) {
  mem.writeU16(0x20000n, 0);
  mem.writeU16(0x20000n + 2n, 1);
  mem.writeU16(0x20000n + 4n, head);
}

describe('Tier 2: F-VIO-02 Boundary & Corner Cases (production)', () => {
  test('F-VIO-02-B01: cyclic loop in descriptor chain throws DESCRIPTOR_LOOP_DETECTED', () => {
    const { mem, vq } = setupQueue();
    writeDesc(mem, 0, 0x10000, 16, VRING_DESC_F_NEXT, 1);
    writeDesc(mem, 1, 0x10020, 16, VRING_DESC_F_NEXT, 0); // loop!
    armAvail(mem, 0);
    assertThrows(() => vq.popDescriptorChain(mem), /DESCRIPTOR_LOOP_DETECTED/);
  });

  test('F-VIO-02-B02: head index beyond queue size throws HEAD_INDEX_OUT_OF_BOUNDS', () => {
    const { mem, vq } = setupQueue();
    armAvail(mem, 16);
    assertThrows(() => vq.popDescriptorChain(mem), /HEAD_INDEX_OUT_OF_BOUNDS/);
    armAvail(mem, 100);
    vq.lastAvailIdx = 0;
    assertThrows(() => vq.popDescriptorChain(mem), /HEAD_INDEX_OUT_OF_BOUNDS/);
  });

  test('F-VIO-02-B03: non-power-of-2 queue size (e.g. 15, 33) throws INVALID_QUEUE_SIZE', () => {
    const vq = new Virtqueue(0, 64);
    assertThrows(() => vq.setSize(15), /INVALID_QUEUE_SIZE/);
    assertThrows(() => vq.setSize(33), /INVALID_QUEUE_SIZE/);
    assertThrows(() => vq.setSize(1.5), /INVALID_QUEUE_SIZE/);
  });

  test('F-VIO-02-B04: queue size 0 throws INVALID_QUEUE_SIZE', () => {
    const vq = new Virtqueue(0, 64);
    assertThrows(() => vq.setSize(0), /INVALID_QUEUE_SIZE/);
  });

  test('F-VIO-02-B05: available ring index wraps from 65535 to 0 correctly', () => {
    const { mem, vq } = setupQueue();
    writeDesc(mem, 0, 0x5000, 64, 0, 0);
    vq.lastAvailIdx = 65535;
    mem.writeU16(0x20000n, 0);
    mem.writeU16(0x20000n + 2n, 0); // wrapped idx
    mem.writeU16(0x20000n + 4n + BigInt((65535 % 16) * 2), 0);
    const chain = vq.popDescriptorChain(mem);
    assertEqual(chain.headIndex, 0);
    assertEqual(vq.lastAvailIdx, 0);
  });
});
