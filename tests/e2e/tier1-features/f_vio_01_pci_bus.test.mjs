import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real PciBus + VirtioPciDevice from src/.
import { PciBus } from '../../../src/virtio/pci_bus.js';
import { VirtioPciDevice } from '../../../src/virtio/virtio_pci_device.js';
import { IrqController } from '../../../src/virtio/irq_controller.js';

function twoDevices() {
  const bus = new PciBus(new IrqController());
  const blk = new VirtioPciDevice({ name: 'virtio-blk', subsystemDeviceId: 2, pciClass: 0x018000, irqLine: 10 });
  const gpu = new VirtioPciDevice({ name: 'virtio-gpu', subsystemDeviceId: 16, pciClass: 0x038000, irqLine: 11 });
  bus.registerDevice(1, blk);
  bus.registerDevice(2, gpu);
  return { bus, blk, gpu };
}

describe('Tier 1: F-VIO-01 Virtio PCI Bus & Configuration Router (production)', () => {
  test('F-VIO-01-01: configuration space enable bit controls access to PCI bus', () => {
    const { bus } = twoDevices();
    bus.writeConfigAddress(0x00000800); // Enable bit 31 cleared
    assertEqual(bus.readConfigData(0xCFC, 4), 0xFFFFFFFF);

    bus.writeConfigAddress(0x80000800); // Enable bit 31 set
    assertOk(bus.readConfigAddress() & 0x80000000);
  });

  test('F-VIO-01-02: reading offset 0 returns Virtio Vendor ID 0x1AF4 and Device ID', () => {
    const { bus } = twoDevices();
    // Bus 0, slot 1, func 0, offset 0
    bus.writeConfigAddress(0x80000000 | (1 << 11) | 0x00);
    const dword0 = bus.readConfigData(0xCFC, 4);
    assertEqual(dword0 & 0xFFFF, 0x1AF4);
    assertEqual((dword0 >> 16) & 0xFFFF, 0x1040 + 2);
  });

  test('F-VIO-01-03: unmapped slot and out-of-range reads return all-ones, never throw', () => {
    const { bus } = twoDevices();
    bus.writeConfigAddress(0x80000000 | (9 << 11) | 0x00); // empty slot 9
    assertEqual(bus.readConfigData(0xCFC, 4), 0xFFFFFFFF);
    bus.writeConfigAddress(0x80000000 | (1 << 11) | 0xFC); // tail offset
    assertEqual(bus.readConfigData(0xCFD, 4), 0xFFFFFFFF);
  });

  test('F-VIO-01-04: BAR sizing probe reports size mask; address assignment routes I/O', () => {
    const { bus, blk } = twoDevices();
    bus.writeConfigAddress(0x80000000 | (1 << 11) | 0x10); // BAR0
    bus.writeConfigData(0xCFC, 0xFFFFFFFF, 4);
    const mask = bus.readConfigData(0xCFC, 4);
    assertOk(mask !== 0 && mask !== 0xFFFFFFFF);
    bus.writeConfigData(0xCFC, 0xD000, 4);
    assertOk(blk.getActiveBars().some((b) => b.base === 0xD000));
  });

  test('F-VIO-01-05: duplicate slot registration is rejected', () => {
    const { bus } = twoDevices();
    let rejected = false;
    try {
      bus.registerDevice(1, new VirtioPciDevice({ name: 'dup', subsystemDeviceId: 1 }));
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
  });
});
