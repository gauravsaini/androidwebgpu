import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real devices + transport from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { Virtqueue, VRING_DESC_F_NEXT, VRING_DESC_F_WRITE, VRING_DESC_SIZE } from '../../../src/virtio/virtqueue.js';
import { IrqController, VIRTIO_ISR_QUEUE, VIRTIO_ISR_CONFIG } from '../../../src/virtio/irq_controller.js';
import { VirtioPciDevice } from '../../../src/virtio/virtio_pci_device.js';
import { VirtioBlk, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_FLUSH, VIRTIO_BLK_S_OK } from '../../../src/io/block/virtio_blk.js';
import { VirtioInput, EV_ABS } from '../../../src/io/input/virtio_input.js';
import { VirtioNet } from '../../../src/io/net/virtio_net.js';

function queueAt(base) {
  const vq = new Virtqueue(0, 16);
  vq.descTableAddr = BigInt(base);
  vq.availRingAddr = BigInt(base) + 0x1000n;
  vq.usedRingAddr = BigInt(base) + 0x2000n;
  vq.setEnabled(true);
  return vq;
}

function writeDesc(mem, tableBase, idx, addr, len, flags, next) {
  const off = BigInt(tableBase) + BigInt(idx * VRING_DESC_SIZE);
  mem.writeU64(off, BigInt(addr));
  mem.writeU32(off + 8n, len);
  mem.writeU16(off + 12n, flags);
  mem.writeU16(off + 14n, next);
}

function armAvail(mem, availBase, head) {
  mem.writeU16(BigInt(availBase), 0);
  mem.writeU16(BigInt(availBase) + 2n, 1);
  mem.writeU16(BigInt(availBase) + 4n, head);
}

describe('Tier 3: Pairwise Virtio ↔ Devices Combinations (production)', () => {
  test('P-VIO-DEV-01: concurrent Block and Network queues pop independent chains', () => {
    const mem = new GuestMem(2 * 1024 * 1024);
    const blkQ = queueAt(0x10000);
    const netQ = queueAt(0x20000);
    // Block request header: type IN (u32), ioprio (u32), sector (u64)
    const blkReq = new Uint8Array(16);
    new DataView(blkReq.buffer).setUint32(0, VIRTIO_BLK_T_IN, true);
    new DataView(blkReq.buffer).setBigUint64(8, 100n, true);
    mem.writeBytes(0x50000, blkReq);
    writeDesc(mem, 0x10000, 0, 0x50000, 16, 0, 0);
    armAvail(mem, 0x11000, 0);
    // Net TX bytes
    const frame = new Uint8Array(64).fill(0xAB);
    mem.writeBytes(0x60000, frame);
    writeDesc(mem, 0x20000, 0, 0x60000, 64, 0, 0);
    armAvail(mem, 0x21000, 0);
    const blkChain = blkQ.popDescriptorChain(mem);
    const netChain = netQ.popDescriptorChain(mem);
    assertOk(blkChain !== null && netChain !== null);
    assertEqual(mem.readU32(blkChain.readable[0].addr), VIRTIO_BLK_T_IN);
    assertEqual(netChain.totalReadLen, 64);
  });

  test('P-VIO-DEV-02: shared IRQ controller latches both devices and clears atomically', () => {
    const lines = [];
    const ctl = new IrqController((irq, level) => lines.push({ irq, level }));
    const blk = new VirtioBlk({ irqLine: 11 });
    const net = new VirtioNet({ irqLine: 11 });
    ctl.assertDeviceIrq(blk, VIRTIO_ISR_QUEUE);
    ctl.assertDeviceIrq(net, VIRTIO_ISR_CONFIG);
    assertEqual(ctl.readIsrStatus(blk) | ctl.readIsrStatus(net), 0x03);
    assertEqual(ctl.readIsrStatus(blk), 0);
  });

  test('P-VIO-DEV-03: VirtioInput delivers ABS pointer then key events in order', () => {
    const dev = new VirtioInput({ displayWidth: 640, displayHeight: 480 });
    dev.handleDomPointer(350, 100, 640, 480);
    const key = dev.handleDomKey('KeyA', true);
    const types = dev.pendingHost.map((e) => e.type);
    assertOk(types.includes(EV_ABS));
    assertEqual(key.code, 30);
    assertEqual(dev.pendingHost[dev.pendingHost.length - 1].seq > dev.pendingHost[0].seq, true);
  });

  test('P-VIO-DEV-04: VirtioNet TX policy accepts frame then enforces MTU', () => {
    const dev = new VirtioNet();
    assertEqual(dev.transmit(new Uint8Array(1000).fill(0xAA)).accepted, true);
    assertEqual(dev.transmit(new Uint8Array(1600)).accepted, false);
  });

  test('P-VIO-DEV-05: VirtioBlk FLUSH completes the chain with status OK', async () => {
    const mem = new GuestMem(1024 * 1024);
    const blk = new VirtioBlk();
    blk.setGuestMem(mem);
    const vq = blk.queues[0];
    vq.descTableAddr = 0x1000n;
    vq.availRingAddr = 0x2000n;
    vq.usedRingAddr = 0x3000n;
    vq.setEnabled(true);
    // Header: FLUSH + sector 0
    const req = new Uint8Array(16);
    new DataView(req.buffer).setUint32(0, VIRTIO_BLK_T_FLUSH, true);
    mem.writeBytes(0x10000, req);
    const off0 = 0x1000n;
    mem.writeU64(off0, 0x10000n);
    mem.writeU32(off0 + 8n, 16);
    mem.writeU16(off0 + 12n, VRING_DESC_F_NEXT);
    mem.writeU16(off0 + 14n, 1);
    const off1 = 0x1010n;
    mem.writeU64(off1, 0x10100n);
    mem.writeU32(off1 + 8n, 1);
    mem.writeU16(off1 + 12n, VRING_DESC_F_WRITE);
    mem.writeU16(off1 + 14n, 0);
    armAvail(mem, 0x2000, 0);
    await blk.processQueue();
    assertEqual(mem.readU16(0x3000n + 2n), 1);
    assertEqual(mem.readU8(0x10100n), VIRTIO_BLK_S_OK);
  });
});
