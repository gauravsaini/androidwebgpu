import { describe, test } from 'node:test';
import { assertEqual } from '../harness/assertions.mjs';
// Production imports: real IrqController from src/.
import { IrqController, VIRTIO_ISR_QUEUE, VIRTIO_ISR_CONFIG } from '../../../src/virtio/irq_controller.js';
import { VirtioPciDevice } from '../../../src/virtio/virtio_pci_device.js';

function wired() {
  const lines = [];
  const ctl = new IrqController((irq, level) => lines.push({ irq, level }));
  const dev = new VirtioPciDevice({ name: 'b-dev', subsystemDeviceId: 1, irqLine: 10 });
  return { ctl, dev, lines };
}

describe('Tier 2: F-VIO-03 Boundary & Corner Cases (production)', () => {
  test('F-VIO-03-B01: spurious read of ISR when no interrupt asserted returns 0 and line remains low', () => {
    const { ctl, dev, lines } = wired();
    assertEqual(ctl.readIsrStatus(dev), 0);
    assertEqual(lines.length, 0);
  });

  test('F-VIO-03-B02: repeated assertion of same interrupt does not corrupt status bits', () => {
    const { ctl, dev } = wired();
    for (let i = 0; i < 10; i++) {
      ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    }
    assertEqual(ctl.readIsrStatus(dev), VIRTIO_ISR_QUEUE);
    assertEqual(ctl.readIsrStatus(dev), 0);
  });

  test('F-VIO-03-B03: rapid back-to-back read cycles consistently clear status', () => {
    const { ctl, dev } = wired();
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    assertEqual(ctl.readIsrStatus(dev), 0x01);
    assertEqual(ctl.readIsrStatus(dev), 0x00);
    assertEqual(ctl.readIsrStatus(dev), 0x00);
  });

  test('F-VIO-03-B04: simultaneous queue and config assertions latch both bit 0 and bit 1', () => {
    const { ctl, dev } = wired();
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_CONFIG);
    assertEqual(ctl.readIsrStatus(dev), 0x03);
  });

  test('F-VIO-03-B05: new assertion arriving immediately after read re-asserts line cleanly', () => {
    const { ctl, dev, lines } = wired();
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    ctl.readIsrStatus(dev); // Cleared
    assertEqual(lines[lines.length - 1].level, false);

    ctl.assertDeviceIrq(dev, VIRTIO_ISR_CONFIG); // New arrival
    assertEqual(lines[lines.length - 1].level, true);
    assertEqual(ctl.readIsrStatus(dev), 0x02);
  });
});
