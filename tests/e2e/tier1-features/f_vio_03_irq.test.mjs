import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real IrqController + device from src/.
import { IrqController, VIRTIO_ISR_QUEUE, VIRTIO_ISR_CONFIG } from '../../../src/virtio/irq_controller.js';
import { VirtioPciDevice } from '../../../src/virtio/virtio_pci_device.js';

function wiredDevice(irqLine = 11) {
  const lines = [];
  const ctl = new IrqController((irq, level) => lines.push({ irq, level }));
  const dev = new VirtioPciDevice({ name: 'test-dev', subsystemDeviceId: 1, irqLine });
  return { ctl, dev, lines };
}

describe('Tier 1: F-VIO-03 Virtio IRQ Assertion & Status Latches (production)', () => {
  test('F-VIO-03-01: initial IRQ controller state has zero ISR and line low', () => {
    const { ctl, dev, lines } = wiredDevice(11);
    assertEqual(ctl.readIsrStatus(dev), 0);
    assertEqual(lines.length, 0);
  });

  test('F-VIO-03-02: assertQueueInterrupt latches bit 0 and drives line high', () => {
    const { ctl, dev, lines } = wiredDevice(11);
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    assertOk(lines.length === 1 && lines[0].level === true);
    assertEqual(lines[0].irq, 11);
  });

  test('F-VIO-03-03: assertConfigInterrupt latches bit 1 and drives line high', () => {
    const { ctl, dev, lines } = wiredDevice(11);
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_CONFIG);
    assertOk(lines.length === 1 && lines[0].level === true);
  });

  test('F-VIO-03-04: reading ISR register clears latched status and de-asserts line', () => {
    const { ctl, dev, lines } = wiredDevice(11);
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    const readStatus = ctl.readIsrStatus(dev);
    assertEqual(readStatus, VIRTIO_ISR_QUEUE);
    assertEqual(lines.length, 2);
    assertEqual(lines[1].level, false);
    assertEqual(ctl.readIsrStatus(dev), 0);
  });

  test('F-VIO-03-05: multiple assertions latch combined bits until acknowledged', () => {
    const { ctl, dev } = wiredDevice(11);
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
    ctl.assertDeviceIrq(dev, VIRTIO_ISR_CONFIG);
    const latched = ctl.readIsrStatus(dev);
    assertEqual(latched, 0x03);
    assertEqual(ctl.readIsrStatus(dev), 0);
  });
});
