/**
 * virtio_rng.js - Entity E10 (part): VirtioRng via crypto.getRandomValues.
 * Hard error when no entropy source exists; never silently predictable.
 */

import { VirtioPciDevice } from '../../virtio/virtio_pci_device.js';
import { Virtqueue } from '../../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../../virtio/irq_controller.js';

export function fillRandom(target) {
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    // getRandomValues rejects buffers over 65536 bytes: chunk large fills.
    const CHUNK = 65536;
    for (let off = 0; off < target.byteLength; off += CHUNK) {
      crypto.getRandomValues(target.subarray(off, Math.min(off + CHUNK, target.byteLength)));
    }
    return { source: 'crypto.getRandomValues' };
  }
  throw new Error('RNG_UNAVAILABLE');
}

export class VirtioRng extends VirtioPciDevice {
  constructor(options = {}) {
    super({
      name: 'virtio-rng',
      subsystemDeviceId: 4,
      pciClass: 0x0c0000,
      irqLine: options.irqLine || 10,
      deviceFeatures: 0n,
      queues: [new Virtqueue(0, options.queueSize || 64)],
      deviceConfigSize: 8,
    });
    this.name = 'virtio-rng';
    this.bytesServed = 0;
  }

  onQueueNotify(queueIdx) {
    if (queueIdx !== 0 || !this.guestMem) return;
    const vq = this.queues[0];
    if (!vq || !vq.enabled) return;
    try {
      while (vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        let served = 0;
        for (const seg of chain.writable) {
          const buf = new Uint8Array(seg.len);
          fillRandom(buf);
          this.guestMem.writeBytes(seg.addr, buf);
          served += seg.len;
        }
        this.bytesServed += served;
        vq.pushUsed(chain.headIndex, served, this.guestMem);
        if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus?.irqController) {
          this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
        }
      }
    } catch (_e) {}
  }

  reset() {
    super.reset();
    this.name = 'virtio-rng';
  }
}
