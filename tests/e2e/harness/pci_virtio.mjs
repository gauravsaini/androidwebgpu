/**
 * Virtio PCI Bus & Virtqueue Split Ring Spec Oracle
 * Implements contracts for F-VIO-01, F-VIO-02, F-VIO-03.
 */

export const VIRTIO_VENDOR_ID = 0x1AF4;
export const VIRTIO_DEV_NET = 0x1000;
export const VIRTIO_DEV_BLK = 0x1001;
export const VIRTIO_DEV_CONSOLE = 0x1003;
export const VIRTIO_DEV_RNG = 0x1004;
export const VIRTIO_DEV_BALLOON = 0x1005;
export const VIRTIO_DEV_GPU = 0x1010;
export const VIRTIO_DEV_INPUT = 0x1012;
export const VIRTIO_DEV_SOUND = 0x1019;

export const VIRTIO_STATUS_ACKNOWLEDGE = 1;
export const VIRTIO_STATUS_DRIVER = 2;
export const VIRTIO_STATUS_DRIVER_OK = 4;
export const VIRTIO_STATUS_FEATURES_OK = 8;
export const VIRTIO_STATUS_DEVICE_NEEDS_RESET = 64;
export const VIRTIO_STATUS_FAILED = 128;

export const VRING_DESC_F_NEXT = 1;
export const VRING_DESC_F_WRITE = 2;
export const VRING_DESC_F_INDIRECT = 4;

export const ISR_QUEUE_INTERRUPT = 0x01;
export const ISR_CONFIG_INTERRUPT = 0x02;

export class PciConfigSpaceOracle {
  constructor(devices = []) {
    this.devices = devices; // Array of { bdf, vendorId, deviceId, classCode, status, bar0, irqLine }
    this.addressRegister = 0; // Port 0xCF8
  }

  writeConfigAddress(val) {
    this.addressRegister = val >>> 0;
  }

  readConfigAddress() {
    return this.addressRegister >>> 0;
  }

  isConfigEnabled() {
    return (this.addressRegister & 0x80000000) !== 0;
  }

  getBus() {
    return (this.addressRegister >> 16) & 0xFF;
  }

  getDevice() {
    return (this.addressRegister >> 11) & 0x1F;
  }

  getFunction() {
    return (this.addressRegister >> 8) & 0x07;
  }

  getOffset() {
    return this.addressRegister & 0xFC;
  }

  readConfigData() {
    if (!this.isConfigEnabled()) {
      return 0xFFFFFFFF;
    }
    const bdf = (this.getBus() << 8) | (this.getDevice() << 3) | this.getFunction();
    const dev = this.devices.find(d => d.bdf === bdf);
    if (!dev) {
      return 0xFFFFFFFF; // Master abort
    }
    const offset = this.getOffset();
    if (offset === 0x00) {
      return (dev.deviceId << 16) | dev.vendorId;
    }
    if (offset === 0x04) {
      return 0x00100007; // Status + Command (IO, Mem, Master)
    }
    if (offset === 0x08) {
      return (dev.classCode << 8) | 0x00; // Revision
    }
    if (offset === 0x10) { // BAR0
      return dev.bar0 || 0;
    }
    if (offset === 0x3C) { // IRQ
      return (dev.irqPin << 8) | dev.irqLine;
    }
    return 0;
  }

  writeConfigData(val) {
    if (!this.isConfigEnabled()) return;
    const bdf = (this.getBus() << 8) | (this.getDevice() << 3) | this.getFunction();
    const dev = this.devices.find(d => d.bdf === bdf);
    if (!dev) return;
    const offset = this.getOffset();
    if (offset === 0x10) {
      dev.bar0 = val;
    }
  }
}

export class VirtqueueSplitRingParser {
  constructor(guestMem, descTableAddr, availRingAddr, usedRingAddr, queueSize) {
    this.guestMem = guestMem;
    this.descTableAddr = descTableAddr;
    this.availRingAddr = availRingAddr;
    this.usedRingAddr = usedRingAddr;
    this.queueSize = queueSize;

    if (!queueSize || (queueSize & (queueSize - 1)) !== 0 || queueSize > 1024) {
      throw new Error(`INVALID_VIRTQUEUE_SIZE:${queueSize}`);
    }
    this.lastAvailIdx = 0;
  }

  readDescriptor(index) {
    if (index >= this.queueSize) {
      throw new RangeError(`DESC_INDEX_OUT_OF_BOUNDS:${index}>=${this.queueSize}`);
    }
    const paddr = this.descTableAddr + (index * 16);
    const addrLo = this.guestMem.readU32(paddr);
    const addrHi = this.guestMem.readU32(paddr + 4);
    const addr = addrLo + (addrHi * 0x100000000);
    const len = this.guestMem.readU32(paddr + 8);
    const flags = this.guestMem.readU16(paddr + 12);
    const next = this.guestMem.readU16(paddr + 14);

    return { index, addr, len, flags, next };
  }

  parseDescriptorChain(headIndex) {
    const chain = [];
    const visited = new Set();
    let currentIdx = headIndex;

    while (true) {
      if (visited.has(currentIdx)) {
        throw new Error(`VIRTQUEUE_DESCRIPTOR_LOOP_DETECTED:index=${currentIdx}`);
      }
      visited.add(currentIdx);

      const desc = this.readDescriptor(currentIdx);
      chain.push(desc);

      if ((desc.flags & VRING_DESC_F_NEXT) !== 0) {
        currentIdx = desc.next;
      } else {
        break;
      }
    }
    return chain;
  }

  consumeAvail() {
    const availIdx = this.guestMem.readU16(this.availRingAddr + 2);
    const chains = [];

    while (this.lastAvailIdx !== availIdx) {
      const ringOffset = this.availRingAddr + 4 + ((this.lastAvailIdx % this.queueSize) * 2);
      const headDesc = this.guestMem.readU16(ringOffset);
      const chain = this.parseDescriptorChain(headDesc);
      chains.push({ headIndex: headDesc, chain });
      this.lastAvailIdx = (this.lastAvailIdx + 1) & 0xFFFF;
    }
    return chains;
  }

  pushUsed(headIndex, writtenLength) {
    const usedIdx = this.guestMem.readU16(this.usedRingAddr + 2);
    const elemOffset = this.usedRingAddr + 4 + ((usedIdx % this.queueSize) * 8);
    this.guestMem.writeU32(elemOffset, headIndex);
    this.guestMem.writeU32(elemOffset + 4, writtenLength);
    this.guestMem.writeU16(this.usedRingAddr + 2, (usedIdx + 1) & 0xFFFF);
  }
}
