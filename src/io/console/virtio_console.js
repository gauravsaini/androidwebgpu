/**
 * virtio_console.js - Entity E07: VirtioConsole + boot log.
 * TX lossless up to a bounded buffer; overflow counted and signalled.
 */

import { VirtioPciDevice } from '../../virtio/virtio_pci_device.js';
import { Virtqueue } from '../../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../../virtio/irq_controller.js';

export const CONSOLE_MAX_BUFFER = 10 * 1024 * 1024; // 10 MB

export class VirtioConsole extends VirtioPciDevice {
  constructor(options = {}) {
    super({
      name: 'virtio-console',
      subsystemDeviceId: 3,
      pciClass: 0x078000,
      irqLine: options.irqLine || 10,
      deviceFeatures: 0n,
      queues: [new Virtqueue(0, options.queueSize || 128), new Virtqueue(1, options.queueSize || 128)],
      deviceConfigSize: 16,
    });
    this.name = 'virtio-console';
    this.log = [];
    this.logBytes = 0;
    this.overflowCount = 0;
    this.rxEnabled = options.rxEnabled === true;
    this.rxQueue = [];
    this.epoch = 0;
  }

  writeSerial(text) {
    const bytes = typeof text === 'string' ? new TextEncoder().encode(text) : text;
    return this.appendLog(bytes);
  }

  appendLog(bytes) {
    if (this.logBytes + bytes.byteLength > CONSOLE_MAX_BUFFER) {
      this.overflowCount += 1;
      return { accepted: false, overflow: this.overflowCount };
    }
    this.log.push({ epoch: this.epoch, ts: Date.now(), data: bytes.slice() });
    this.logBytes += bytes.byteLength;
    return { accepted: true, overflow: this.overflowCount };
  }

  serialText() {
    const dec = new TextDecoder();
    return this.log.map((e) => { try { return dec.decode(e.data); } catch (_e) { return ''; } }).join('');
  }

  injectRx(bytes) {
    if (!this.rxEnabled) return { accepted: false, reason: 'RX_DISABLED' };
    this.rxQueue.push(bytes.slice());
    return { accepted: true };
  }

  onQueueNotify(queueIdx) {
    // TX queue 0: guest -> host serial bytes. Drain readable segments.
    if (queueIdx !== 0 || !this.guestMem) return;
    const vq = this.queues[0];
    if (!vq || !vq.enabled) return;
    try {
      while (vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        let n = 0;
        for (const seg of chain.readable) {
          const bytes = this.guestMem.readBytes(seg.addr, seg.len);
          this.appendLog(bytes);
          n += seg.len;
        }
        vq.pushUsed(chain.headIndex, n, this.guestMem);
        if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus?.irqController) {
          this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
        }
      }
    } catch (_e) {}
  }

  reset() {
    super.reset();
    this.name = 'virtio-console';
    // Wake pending reads: drop RX waiters by clearing the queue.
    this.rxQueue = [];
  }
}
