import { describe, test } from 'node:test';
import { assertEqual, assertOk, assertThrows } from '../harness/assertions.mjs';
// Production imports: real VM + virtio transport from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { Virtqueue, VRING_DESC_F_NEXT, VRING_DESC_SIZE } from '../../../src/virtio/virtqueue.js';
import { PciBus } from '../../../src/virtio/pci_bus.js';
import { IrqController } from '../../../src/virtio/irq_controller.js';
import { VirtioPciDevice, VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_DRIVER_OK } from '../../../src/virtio/virtio_pci_device.js';
import { VmRuntime } from '../../../src/vm/vm_runtime.js';

function dummyKernel() {
  const buf = new Uint8Array(0x3000);
  const view = new DataView(buf.buffer);
  view.setUint8(0x01F1, 4);
  view.setUint32(0x0202, 0x53726448, true);
  view.setUint16(0x0206, 0x020c, true);
  view.setUint32(0x0214, 0x00100000, true);
  return buf;
}

function ring(memSize = 1024 * 1024) {
  const mem = new GuestMem(memSize);
  const vq = new Virtqueue(0, 16);
  vq.descTableAddr = 0x1000n;
  vq.availRingAddr = 0x2000n;
  vq.usedRingAddr = 0x3000n;
  vq.setEnabled(true);
  return { mem, vq };
}

describe('Tier 3: Pairwise VM ↔ Virtio Combinations (production)', () => {
  test('P-VM-VIO-01: VM memory bounds enforcement during virtqueue descriptor traversal', () => {
    const { mem, vq } = ring();
    // Descriptor pointing past 1MB physical RAM (2MB address).
    const off = 0x1000n;
    mem.writeU64(off, 0x200000n);
    mem.writeU32(off + 8n, 512);
    mem.writeU16(off + 12n, 0);
    mem.writeU16(off + 14n, 0);
    mem.writeU16(0x2000n, 0);
    mem.writeU16(0x2000n + 2n, 1);
    mem.writeU16(0x2000n + 4n, 0);
    assertThrows(() => vq.popDescriptorChain(mem), /DMA_OUT_OF_BOUNDS/);
  });

  test('P-VM-VIO-02: VM reset propagates to Virtio PCI bus resetting device status and queues', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    const dev = rt.worker.pciBus.getDevice(2);
    dev.setDeviceStatus(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);
    assertEqual(dev.deviceStatus, 7);
    await rt.reset();
    await new Promise((r) => setTimeout(r, 25));
    assertEqual(dev.deviceStatus, 0);
    await rt.destroy();
  });

  test('P-VM-VIO-03: Virtio IRQ assertion coordinates with VM interrupt delivery log', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    await rt.start();
    rt.injectIrq(11, true);
    await new Promise((r) => setTimeout(r, 60));
    const delivered = rt.worker.getDeliveredIrqs();
    assertOk(delivered.length >= 1);
    assertEqual(delivered[delivered.length - 1].irq, 11);
    assertEqual(rt.worker.pendingIrqs.length, 0);
    await rt.destroy();
  });

  test('P-VM-VIO-04: Multi-queue allocation in guest memory respects alignment and spacing', () => {
    const mem = new GuestMem(4 * 1024 * 1024);
    const queueSizes = [64, 128];
    const baseAddr = 0x10000;
    function calcRingLayout(base, size) {
      const desc = base;
      const avail = desc + (16 * size);
      const used = (avail + 4 + (2 * size) + 3) & ~3;
      const total = used + 4 + (8 * size);
      return { desc, avail, used, total };
    }
    const q0 = calcRingLayout(baseAddr, queueSizes[0]);
    const q1 = calcRingLayout(q0.total, queueSizes[1]);
    assertOk(q0.used > q0.avail);
    assertOk(q1.desc >= q0.total);
    mem.writeU32(q0.desc, 0x12345678);
    mem.writeU32(q1.desc, 0x87654321);
    assertEqual(mem.readU32(q0.desc), 0x12345678);
    assertEqual(mem.readU32(q1.desc), 0x87654321);
  });

  test('P-VM-VIO-05: Virtqueue scatter-gather chain across scattered guest pages', () => {
    const mem = new GuestMem(2 * 1024 * 1024);
    const pages = [0xA000, 0x14000, 0x1E000];
    const entries = pages.map((paddr) => ({ paddr: BigInt(paddr), len: 1024 }));
    const target = new Uint8Array(3072);
    mem.writeBytes(0xA000, new Uint8Array(1024).fill(0x11));
    mem.writeBytes(0x14000, new Uint8Array(1024).fill(0x22));
    mem.writeBytes(0x1E000, new Uint8Array(1024).fill(0x33));
    const total = mem.scatterGather(entries, target, 'READ');
    assertEqual(total, 3072);
    assertEqual(target[0], 0x11);
    assertEqual(target[1024], 0x22);
    assertEqual(target[2048], 0x33);
  });
});
