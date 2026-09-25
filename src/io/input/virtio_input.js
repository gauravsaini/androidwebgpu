/**
 * virtio_input.js - Entity E08: VirtioInput (keyboard/mouse/touch).
 * DOM key mapping explicit + versioned; unknown keys reported, not guessed.
 * Focus loss sends key-up for all pressed keys.
 */

import { VirtioPciDevice } from '../../virtio/virtio_pci_device.js';
import { Virtqueue } from '../../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../../virtio/irq_controller.js';

export const INPUT_KEYMAP_VERSION = 'evdev-1.0';
export const EV_SYN = 0x00;
export const EV_KEY = 0x01;
export const EV_REL = 0x02;
export const EV_ABS = 0x03;

// Minimal explicit DOM code -> evdev mapping (extend only by version bump).
export const DOM_TO_EVDEV = Object.freeze({
  KeyA: 30, KeyB: 48, KeyC: 46, KeyD: 32, KeyE: 18, KeyF: 33, KeyG: 34,
  KeyH: 35, KeyI: 23, KeyJ: 36, KeyK: 37, KeyL: 38, KeyM: 50, KeyN: 49,
  KeyO: 24, KeyP: 25, KeyQ: 16, KeyR: 19, KeyS: 31, KeyT: 20, KeyU: 22,
  KeyV: 47, KeyW: 17, KeyX: 45, KeyY: 21, KeyZ: 44,
  Enter: 28, Escape: 1, Space: 57, Tab: 15, Backspace: 14,
  ArrowUp: 103, ArrowDown: 108, ArrowLeft: 105, ArrowRight: 106,
});

export class VirtioInput extends VirtioPciDevice {
  constructor(options = {}) {
    super({
      name: 'virtio-input',
      subsystemDeviceId: 18,
      pciClass: 0x090000,
      irqLine: options.irqLine || 10,
      deviceFeatures: 0n,
      queues: [new Virtqueue(0, options.queueSize || 128), new Virtqueue(1, options.queueSize || 128)],
      deviceConfigSize: 32,
    });
    this.name = 'virtio-input';
    this.seq = 0;
    this.pressed = new Set();
    this.pendingHost = [];
    this.displayWidth = options.displayWidth || 640;
    this.displayHeight = options.displayHeight || 480;
    this.unknownKeys = [];
  }

  mapDomKey(code) {
    if (code in DOM_TO_EVDEV) return DOM_TO_EVDEV[code];
    this.unknownKeys.push(String(code));
    return null;
  }

  enqueueHostEvent({ type, code, value, seq = null }) {
    this.seq += 1;
    const ev = { type: Number(type), code: Number(code), value: Number(value), seq: seq ?? this.seq };
    this.pendingHost.push(ev);
    if (this.pendingHost.length > 4096) this.pendingHost.shift();
    this.pumpQueue();
    return ev;
  }

  handleDomKey(domCode, down) {
    const evdev = this.mapDomKey(domCode);
    if (evdev === null) return { reported: domCode, guessed: false };
    if (down) this.pressed.add(evdev);
    else this.pressed.delete(evdev);
    return this.enqueueHostEvent({ type: EV_KEY, code: evdev, value: down ? 1 : 0 });
  }

  handleDomPointer(cssX, cssY, cssWidth, cssHeight) {
    const gx = Math.round((cssX / Math.max(1, cssWidth)) * this.displayWidth);
    const gy = Math.round((cssY / Math.max(1, cssHeight)) * this.displayHeight);
    this.enqueueHostEvent({ type: EV_ABS, code: 0x00, value: gx });
    return this.enqueueHostEvent({ type: EV_ABS, code: 0x01, value: gy });
  }

  handleFocusLoss() {
    // Key-up for every pressed key so the guest never sticks.
    const ups = [...this.pressed].map((code) => this.enqueueHostEvent({ type: EV_KEY, code, value: 0 }));
    this.pressed.clear();
    return ups;
  }

  pumpQueue() {
    const vq = this.queues[0];
    if (!vq || !vq.enabled || !this.guestMem) return;
    try {
      while (this.pendingHost.length > 0 && vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        const ev = this.pendingHost.shift();
        // virtio_input_event: type u16, code u16, value s32
        if (chain.writable.length > 0 && chain.writable[0].len >= 8) {
          const dst = chain.writable[0].addr;
          this.guestMem.writeU16(dst, ev.type);
          this.guestMem.writeU16(dst + 2n, ev.code);
          this.guestMem.writeU32(dst + 4n, ev.value >>> 0);
          vq.pushUsed(chain.headIndex, 8, this.guestMem);
        } else {
          vq.pushUsed(chain.headIndex, 0, this.guestMem);
        }
        if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus?.irqController) {
          this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
        }
      }
    } catch (_e) {}
  }

  onQueueNotify(queueIdx) {
    if (queueIdx === 0) this.pumpQueue();
  }

  reset() {
    super.reset();
    this.name = 'virtio-input';
    this.pendingHost = [];
    this.pressed.clear();
  }
}
