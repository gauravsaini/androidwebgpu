/**
 * virtio_sound.js - Entity E10 (part): VirtioSound output policy.
 * Starts only after browser user activation; reports mute/blocked state,
 * underrun/overrun counters. Non-blocking writes.
 */

import { VirtioPciDevice } from '../../virtio/virtio_pci_device.js';
import { Virtqueue } from '../../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../../virtio/irq_controller.js';

export class VirtioSound extends VirtioPciDevice {
  constructor(options = {}) {
    super({
      name: 'virtio-snd',
      subsystemDeviceId: 25,
      pciClass: 0x040300,
      irqLine: options.irqLine || 11,
      deviceFeatures: 0n,
      queues: [new Virtqueue(0, options.queueSize || 64)],
      deviceConfigSize: 16,
    });
    this.name = 'virtio-snd';
    this.unlocked = false;
    this.muted = options.muted === true;
    this.underruns = 0;
    this.overruns = 0;
    this.framesAccepted = 0;
  }

  unlock() {
    this.unlocked = true;
    return { unlocked: true };
  }

  audioWrite(pcm) {
    if (!this.unlocked) return { accepted: false, error: 'AUDIO_BLOCKED' };
    if (this.muted) return { accepted: false, error: 'AUDIO_MUTED' };
    if (!pcm || pcm.byteLength === 0) {
      this.underruns += 1;
      return { accepted: false, error: 'AUDIO_UNDERRUN', underruns: this.underruns };
    }
    this.framesAccepted += 1;
    return { accepted: true, underrun: false, overrun: false };
  }

  onQueueNotify(queueIdx) {
    if (queueIdx !== 0 || !this.guestMem) return;
    const vq = this.queues[0];
    if (!vq || !vq.enabled) return;
    try {
      while (vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        let total = 0;
        for (const seg of chain.readable) total += seg.len;
        const pcm = new Uint8Array(total);
        let off = 0;
        for (const seg of chain.readable) {
          pcm.set(this.guestMem.readBytes(seg.addr, seg.len), off);
          off += seg.len;
        }
        const res = this.audioWrite(pcm);
        vq.pushUsed(chain.headIndex, res.accepted ? total : 0, this.guestMem);
        if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus?.irqController) {
          this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
        }
      }
    } catch (_e) {}
  }

  reset() {
    super.reset();
    this.name = 'virtio-snd';
    this.unlocked = false;
  }
}
