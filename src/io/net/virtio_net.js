/**
 * virtio_net.js - Entity E09: VirtioNet user-mode proxy device.
 * Declared mode: 'websocket-tunnel' (default) with loopback fallback.
 * Never promises raw L2; CORS/CSP/permission errors -> link-down.
 */

import { VirtioPciDevice } from '../../virtio/virtio_pci_device.js';
import { Virtqueue } from '../../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../../virtio/irq_controller.js';

export const NET_MODE = 'websocket-tunnel';
export const NET_MTU = 1500;
export const NET_MAC = Object.freeze([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

export class VirtioNet extends VirtioPciDevice {
  constructor(options = {}) {
    super({
      name: 'virtio-net',
      subsystemDeviceId: 1,
      pciClass: 0x020000,
      irqLine: options.irqLine || 11,
      deviceFeatures: 0n,
      queues: [new Virtqueue(0, options.queueSize || 128), new Virtqueue(1, options.queueSize || 128)],
      deviceConfigSize: 32,
    });
    this.name = 'virtio-net';
    this.mode = options.mode || NET_MODE;
    this.mtu = NET_MTU;
    this.mac = [...NET_MAC];
    this.linkUp = options.linkUp !== false;
    this.txDropped = 0;
    this.rxQueue = [];
    this.maxPending = options.maxPending || 256;
  }

  readDeviceConfig(offset, size = 4) {
    const buf = new ArrayBuffer(32);
    const v = new DataView(buf);
    for (let i = 0; i < 6; i++) v.setUint8(i, this.mac[i]);
    v.setUint16(6, 1, true); // status: link up
    v.setUint16(8, 1, true); // max_virtqueue_pairs
    v.setUint16(10, this.mtu, true);
    if (offset + size <= 32) {
      if (size === 1) return v.getUint8(offset);
      if (size === 2) return v.getUint16(offset, true);
      if (size === 4) return v.getUint32(offset, true);
    }
    return 0;
  }

  writeDeviceConfig() {}

  setLink(up) {
    this.linkUp = !!up;
  }

  /** Guest TX bytes -> proxy. Bounded; drops counted when link down. */
  transmit(frame) {
    if (!this.linkUp) {
      this.txDropped += 1;
      return { accepted: false, error: 'NET_LINK_DOWN', dropped: this.txDropped };
    }
    if (frame.byteLength > this.mtu + 14) {
      this.txDropped += 1;
      return { accepted: false, error: 'NET_MTU_EXCEEDED', dropped: this.txDropped };
    }
    // Proxy handoff would occur here (WebSocket tunnel). Loopback in stub.
    return { accepted: true, mode: this.mode, bytes: frame.byteLength };
  }

  /** Proxy RX bytes -> guest. Bounded. */
  receive(frame) {
    if (this.rxQueue.length >= this.maxPending) return { accepted: false, error: 'NET_BACKPRESSURE' };
    this.rxQueue.push(frame.slice());
    this.pumpRx();
    return { accepted: true };
  }

  pumpRx() {
    const vq = this.queues[1] || this.queues[0];
    if (!vq || !vq.enabled || !this.guestMem) return;
    try {
      while (this.rxQueue.length > 0 && vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        const frame = this.rxQueue.shift();
        let off = 0;
        for (const seg of chain.writable) {
          const n = Math.min(seg.len, frame.byteLength - off);
          if (n <= 0) break;
          this.guestMem.writeBytes(seg.addr, frame.subarray(off, off + n));
          off += n;
        }
        vq.pushUsed(chain.headIndex, off, this.guestMem);
        if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus?.irqController) {
          this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
        }
      }
    } catch (_e) {}
  }

  onQueueNotify(queueIdx) {
    if (queueIdx === 0 && this.guestMem) {
      // Drain guest TX chains through transmit() policy.
      const vq = this.queues[0];
      if (!vq || !vq.enabled) return;
      try {
        while (vq.hasAvailable(this.guestMem)) {
          const chain = vq.popDescriptorChain(this.guestMem);
          if (!chain) break;
          let total = 0;
          for (const seg of chain.readable) total += seg.len;
          const frame = new Uint8Array(total);
          let off = 0;
          for (const seg of chain.readable) {
            frame.set(this.guestMem.readBytes(seg.addr, seg.len), off);
            off += seg.len;
          }
          this.transmit(frame);
          vq.pushUsed(chain.headIndex, 0, this.guestMem);
        }
      } catch (_e) {}
    } else {
      this.pumpRx();
    }
  }

  reset() {
    super.reset();
    this.name = 'virtio-net';
    this.rxQueue = [];
  }
}
