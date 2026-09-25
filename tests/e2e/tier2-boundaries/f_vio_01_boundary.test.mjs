import { describe, test } from 'node:test';
import { assertEqual } from '../harness/assertions.mjs';
// Production imports: real PciBus + device from src/.
import { PciBus } from '../../../src/virtio/pci_bus.js';
import { VirtioPciDevice } from '../../../src/virtio/virtio_pci_device.js';
import { IrqController } from '../../../src/virtio/irq_controller.js';

function oneDevice() {
  const bus = new PciBus(new IrqController());
  bus.registerDevice(1, new VirtioPciDevice({ name: 'blk', subsystemDeviceId: 2, pciClass: 0x018000, irqLine: 10 }));
  return bus;
}

describe('Tier 2: F-VIO-01 Boundary & Corner Cases (production)', () => {
  test('F-VIO-01-B01: probing unpopulated BDF returns master abort 0xFFFFFFFF', () => {
    const bus = oneDevice();
    bus.writeConfigAddress(0x80000000 | (31 << 11) | (7 << 8)); // slot 31 func 7
    assertEqual(bus.readConfigData(0xCFC, 4), 0xFFFFFFFF);
  });

  test('F-VIO-01-B02: sub-dword port reads return the addressed bytes', () => {
    const bus = oneDevice();
    bus.writeConfigAddress(0x80000000 | (1 << 11) | 0x00);
    // Vendor ID via 2-byte read at 0xCFC and device ID via 0xCFE
    assertEqual(bus.readConfigData(0xCFC, 2), 0x1AF4);
    assertEqual(bus.readConfigData(0xCFE, 2), 0x1040 + 2);
  });

  test('F-VIO-01-B03: writes with enable bit cleared have no effect', () => {
    const bus = oneDevice();
    bus.writeConfigAddress(0x00000000); // enable cleared
    bus.writeConfigData(0xCFC, 0x9999, 4);
    bus.writeConfigAddress(0x80000000 | (1 << 11) | 0x00);
    assertEqual(bus.readConfigData(0xCFC, 2), 0x1AF4); // unchanged
  });

  test('F-VIO-01-B04: out-of-range tail reads return all-ones, never throw', () => {
    const bus = oneDevice();
    bus.writeConfigAddress(0x80000000 | (1 << 11) | 0xFC);
    assertEqual(bus.readConfigData(0xCFD, 4), 0xFFFFFFFF);
  });

  test('F-VIO-01-B05: bus scanning across 32 slots terminates safely', () => {
    const bus = oneDevice();
    let found = 0;
    for (let slot = 0; slot < 32; slot++) {
      bus.writeConfigAddress(0x80000000 | (slot << 11));
      if (bus.readConfigData(0xCFC, 4) !== 0xFFFFFFFF) found += 1;
    }
    assertEqual(found, 1);
  });
});
