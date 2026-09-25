/**
 * virtio_pci_device.js - Entity E05: Base Virtio PCI Device
 * Implements dual-mode Virtio 1.2 PCI transport:
 * 1. Modern PCI Capabilities: COMMON_CFG, NOTIFY_CFG, ISR_CFG, DEVICE_CFG in BAR1 (MMIO)
 * 2. Transitional Legacy BAR0 Registers (0x00..0x13) for early bootloaders and older kernels.
 */

import {
  PCI_BAR_TYPE_IO,
  PCI_BAR_TYPE_MEMORY
} from './pci_bus.js';

// PCI Standard Configuration Space Offsets
export const PCI_VENDOR_ID           = 0x00;
export const PCI_DEVICE_ID           = 0x02;
export const PCI_COMMAND             = 0x04;
export const PCI_STATUS              = 0x06;
export const PCI_REVISION_ID         = 0x08;
export const PCI_CLASS_PROG          = 0x09;
export const PCI_CLASS_DEVICE        = 0x0A;
export const PCI_CACHE_LINE_SIZE     = 0x0C;
export const PCI_LATENCY_TIMER       = 0x0D;
export const PCI_HEADER_TYPE         = 0x0E;
export const PCI_BIST                = 0x0F;
export const PCI_BAR0                = 0x10;
export const PCI_BAR1                = 0x14;
export const PCI_BAR2                = 0x18;
export const PCI_BAR3                = 0x1C;
export const PCI_BAR4                = 0x20;
export const PCI_BAR5                = 0x24;
export const PCI_SUBSYSTEM_VENDOR_ID = 0x2C;
export const PCI_SUBSYSTEM_ID        = 0x2E;
export const PCI_CAPABILITY_LIST     = 0x34;
export const PCI_INTERRUPT_LINE      = 0x3C;
export const PCI_INTERRUPT_PIN       = 0x3D;

// PCI Command & Status Bitmasks
export const PCI_COMMAND_IO_EN       = 0x0001;
export const PCI_COMMAND_MEM_EN      = 0x0002;
export const PCI_COMMAND_BUS_MASTER  = 0x0004;
export const PCI_COMMAND_INTX_DIS    = 0x0400;

export const PCI_STATUS_CAP_LIST     = 0x0010;
export const PCI_STATUS_INTX_STATUS  = 0x0008;

// OASIS Virtio 1.2 Vendor-Specific Capability Types
export const VIRTIO_PCI_CAP_COMMON_CFG = 1;
export const VIRTIO_PCI_CAP_NOTIFY_CFG = 2;
export const VIRTIO_PCI_CAP_ISR_CFG    = 3;
export const VIRTIO_PCI_CAP_DEVICE_CFG = 4;
export const VIRTIO_PCI_CAP_PCI_CFG    = 5;

// Virtio Device Status Flags (OASIS §2.1)
export const VIRTIO_STATUS_ACKNOWLEDGE        = 1;
export const VIRTIO_STATUS_DRIVER             = 2;
export const VIRTIO_STATUS_DRIVER_OK          = 4;
export const VIRTIO_STATUS_FEATURES_OK        = 8;
export const VIRTIO_STATUS_DEVICE_NEEDS_RESET = 64;
export const VIRTIO_STATUS_FAILED             = 128;

// Virtio Standard Feature Bits (OASIS §6)
export const VIRTIO_F_RING_INDIRECT_DESC      = 1n << 28n;
export const VIRTIO_F_RING_EVENT_IDX          = 1n << 29n;
export const VIRTIO_F_VERSION_1               = 1n << 32n;

// Modern BAR1 Memory Map
export const MODERN_COMMON_CFG_OFFSET = 0x0000;
export const MODERN_COMMON_CFG_SIZE   = 56;
export const MODERN_ISR_CFG_OFFSET    = 0x0100;
export const MODERN_ISR_CFG_SIZE      = 1;
export const MODERN_DEVICE_CFG_OFFSET = 0x0200;
export const MODERN_NOTIFY_CFG_OFFSET = 0x0400;
export const MODERN_NOTIFY_CFG_SIZE   = 0x0400;
export const MODERN_BAR_TOTAL_SIZE    = 4096;

export class VirtioPciDevice {
  /**
   * @param {Object} options
   * @param {string} options.name - Device name (e.g. 'virtio-blk', 'virtio-gpu')
   * @param {number} options.subsystemDeviceId - Virtio Subsystem Device ID (1..25)
   * @param {number} options.pciClass - 24-bit PCI class code
   * @param {number} options.irqLine - Hardware IRQ line (e.g. 10 or 11)
   * @param {bigint} [options.deviceFeatures=0n] - Device-specific supported feature bits
   * @param {Array<import('./virtqueue.js').Virtqueue>} [options.queues=[]] - Owned virtqueues
   * @param {number} [options.deviceConfigSize=64] - Size of device-specific config space
   */
  constructor(options) {
    this.name = options.name || 'virtio-device';
    this.subsystemDeviceId = options.subsystemDeviceId || 1;
    this.pciClass = options.pciClass || 0;
    this.irqLine = options.irqLine || 10;
    this.deviceConfigSize = options.deviceConfigSize || 64;

    // Feature negotiation state (64-bit)
    // Only advertise transport features whose behavior is implemented and
    // tested: VERSION_1 always; INDIRECT_DESC (descriptor parser + indirect
    // tables); EVENT_IDX (vring_need_event in Virtqueue.shouldNotify).
    // Callers may mask a feature off via options to match a reduced build.
    const baseDevFeatures = options.deviceFeatures !== undefined ? BigInt(options.deviceFeatures) : 0n;
    let advertised = baseDevFeatures | VIRTIO_F_VERSION_1;
    if (options.disableIndirect !== true) advertised |= VIRTIO_F_RING_INDIRECT_DESC;
    if (options.disableEventIdx !== true) advertised |= VIRTIO_F_RING_EVENT_IDX;
    this.deviceFeatures = advertised;
    this.driverFeatures = 0n;
    this.deviceFeatureSelect = 0;
    this.driverFeatureSelect = 0;

    // Status and lifecycle
    this.deviceStatus = 0;
    this.configGeneration = 0;
    this.queues = options.queues || [];
    this.selectedQueueIdx = 0;

    // References to bus and guest memory
    /** @type {import('./pci_bus.js').PciBus|null} */
    this.bus = null;
    this.slot = -1;
    /** @type {import('../vm/guest_mem.js').GuestMem|null} */
    this.guestMem = null;

    // PCI Configuration Space (256 bytes)
    this.pciSpace = new Uint8Array(256);
    this.initPciHeader();

    // BAR setup: BAR0 = Legacy I/O (64B), BAR1 = Modern MMIO (4KB)
    this.bars = [
      { index: 0, type: 'io', size: 64, base: 0, sizing: false },
      { index: 1, type: 'mmio', size: MODERN_BAR_TOTAL_SIZE, base: 0, sizing: false },
      { index: 2, type: 'mmio', size: 0, base: 0, sizing: false },
      { index: 3, type: 'mmio', size: 0, base: 0, sizing: false },
      { index: 4, type: 'mmio', size: 0, base: 0, sizing: false },
      { index: 5, type: 'mmio', size: 0, base: 0, sizing: false },
    ];
  }

  attachToBus(bus, slot) {
    this.bus = bus;
    this.slot = slot;
  }

  setGuestMem(guestMem) {
    this.guestMem = guestMem;
  }

  initPciHeader() {
    const view = new DataView(this.pciSpace.buffer);
    view.setUint16(PCI_VENDOR_ID, 0x1AF4, true);
    // Modern: 0x1040 + subsystemDeviceId
    view.setUint16(PCI_DEVICE_ID, 0x1040 + this.subsystemDeviceId, true);
    view.setUint16(PCI_COMMAND, 0x0000, true);
    view.setUint16(PCI_STATUS, PCI_STATUS_CAP_LIST, true);
    view.setUint8(PCI_REVISION_ID, 0x01);

    this.pciSpace[PCI_CLASS_PROG] = (this.pciClass) & 0xFF;
    this.pciSpace[PCI_CLASS_DEVICE] = (this.pciClass >>> 8) & 0xFF;
    this.pciSpace[PCI_CLASS_DEVICE + 1] = (this.pciClass >>> 16) & 0xFF;

    view.setUint8(PCI_HEADER_TYPE, 0x00);
    view.setUint16(PCI_SUBSYSTEM_VENDOR_ID, 0x1AF4, true);
    view.setUint16(PCI_SUBSYSTEM_ID, this.subsystemDeviceId, true);
    view.setUint8(PCI_CAPABILITY_LIST, 0x40);
    view.setUint8(PCI_INTERRUPT_LINE, this.irqLine);
    view.setUint8(PCI_INTERRUPT_PIN, 0x01); // INTA#

    this.initCapabilities();
  }

  initCapabilities() {
    const view = new DataView(this.pciSpace.buffer);

    // Capability 1: COMMON_CFG at offset 0x40 -> points to BAR1 offset 0x0000
    this.writeCapHeader(0x40, 0x50, 16, VIRTIO_PCI_CAP_COMMON_CFG, 1, MODERN_COMMON_CFG_OFFSET, MODERN_COMMON_CFG_SIZE);

    // Capability 2: NOTIFY_CFG at offset 0x50 -> points to BAR1 offset 0x0400
    this.writeCapHeader(0x50, 0x64, 20, VIRTIO_PCI_CAP_NOTIFY_CFG, 1, MODERN_NOTIFY_CFG_OFFSET, MODERN_NOTIFY_CFG_SIZE);
    view.setUint32(0x50 + 16, 4, true); // notify_off_multiplier = 4 bytes per queue

    // Capability 3: ISR_CFG at offset 0x64 -> points to BAR1 offset 0x0100
    this.writeCapHeader(0x64, 0x74, 16, VIRTIO_PCI_CAP_ISR_CFG, 1, MODERN_ISR_CFG_OFFSET, MODERN_ISR_CFG_SIZE);

    // Capability 4: DEVICE_CFG at offset 0x74 -> points to BAR1 offset 0x0200
    this.writeCapHeader(0x74, 0x00, 16, VIRTIO_PCI_CAP_DEVICE_CFG, 1, MODERN_DEVICE_CFG_OFFSET, this.deviceConfigSize);
  }

  writeCapHeader(offset, next, capLen, cfgType, bar, barOffset, length) {
    const view = new DataView(this.pciSpace.buffer);
    this.pciSpace[offset + 0] = 0x09; // PCI_CAP_ID_VNDR
    this.pciSpace[offset + 1] = next;
    this.pciSpace[offset + 2] = capLen;
    this.pciSpace[offset + 3] = cfgType;
    this.pciSpace[offset + 4] = bar;
    this.pciSpace[offset + 5] = 0; // padding
    this.pciSpace[offset + 6] = 0;
    this.pciSpace[offset + 7] = 0;
    view.setUint32(offset + 8, barOffset, true);
    view.setUint32(offset + 12, length, true);
  }

  getActiveBars() {
    return this.bars.filter((b) => b.size > 0 && b.base !== 0);
  }

  // --- PCI Configuration Space Access ---

  readPciConfig(offset, size = 4) {
    // Standard PCI behavior: out-of-range reads return all-ones, never throw.
    // (Unaligned tail reads must not crash the host via DataView RangeError.)
    if (!Number.isInteger(offset) || !Number.isInteger(size) || offset < 0 || size < 1 || size > 4 || offset + size > 256) {
      return size === 4 ? 0xFFFFFFFF : (size === 2 ? 0xFFFF : 0xFF);
    }
    if (offset >= PCI_BAR0 && offset <= PCI_BAR5 && (offset & 3) === 0) {
      const barIdx = (offset - PCI_BAR0) >> 2;
      const bar = this.bars[barIdx];
      if (bar.sizing) {
        if (bar.type === 'io') {
          return ((~(bar.size - 1) & ~0x3) | PCI_BAR_TYPE_IO) >>> 0;
        } else {
          return ((~(bar.size - 1) & ~0xF) | PCI_BAR_TYPE_MEMORY) >>> 0;
        }
      }
    }

    const view = new DataView(this.pciSpace.buffer);
    if (size === 1) return view.getUint8(offset);
    if (size === 2) return view.getUint16(offset, true);
    if (size === 4) return view.getUint32(offset, true);
    return 0xFFFFFFFF;
  }

  writePciConfig(offset, val, size = 4) {
    // Out-of-range writes are ignored (standard PCI), never throw.
    if (!Number.isInteger(offset) || !Number.isInteger(size) || offset < 0 || size < 1 || size > 4 || offset + size > 256) {
      return;
    }
    const view = new DataView(this.pciSpace.buffer);

    // Command register updates
    if (offset === PCI_COMMAND && size >= 2) {
      const newCmd = (val & 0xFFFF);
      view.setUint16(PCI_COMMAND, newCmd, true);
      if (this.bus && this.bus.irqController) {
        this.bus.irqController.setCommandRegister(this, newCmd);
      }
      return;
    }

    // Intercept BAR writes for sizing probes and address assignment
    if (offset >= PCI_BAR0 && offset <= PCI_BAR5 && (offset & 3) === 0 && size === 4) {
      const barIdx = (offset - PCI_BAR0) >> 2;
      const bar = this.bars[barIdx];
      if ((val >>> 0) === 0xFFFFFFFF) {
        bar.sizing = true;
      } else {
        bar.sizing = false;
        bar.base = ((bar.type === 'io' ? (val & ~0x3) : (val & ~0xF)) >>> 0);
        view.setUint32(offset, (bar.base | (bar.type === 'io' ? PCI_BAR_TYPE_IO : PCI_BAR_TYPE_MEMORY)) >>> 0, true);
        if (this.bus) this.bus.rebuildBarMappings();
      }
      return;
    }

    // Capabilities area (0x40+) is read-only to guest
    if (offset >= 0x40) return;

    if (size === 1) view.setUint8(offset, val);
    else if (size === 2) view.setUint16(offset, val, true);
    else if (size === 4) view.setUint32(offset, val, true);
  }

  // --- BAR Dispatch ---

  readBar(barIdx, offset, size = 4) {
    if (barIdx === 0) return this.readLegacyBar0(offset, size);
    if (barIdx === 1) return this.readModernBar1(offset, size);
    return size === 4 ? 0xFFFFFFFF : 0xFF;
  }

  writeBar(barIdx, offset, val, size = 4) {
    if (barIdx === 0) this.writeLegacyBar0(offset, val, size);
    else if (barIdx === 1) this.writeModernBar1(offset, val, size);
  }

  // --- Modern BAR1 Configuration Structures ---

  readModernBar1(offset, size = 4) {
    // 1. Common Configuration (0x0000..0x0037)
    if (offset >= MODERN_COMMON_CFG_OFFSET && offset < MODERN_COMMON_CFG_OFFSET + MODERN_COMMON_CFG_SIZE) {
      return this.readCommonCfg(offset - MODERN_COMMON_CFG_OFFSET, size);
    }
    // 2. ISR Status (0x0100)
    if (offset === MODERN_ISR_CFG_OFFSET) {
      if (this.bus && this.bus.irqController) {
        return this.bus.irqController.readIsrStatus(this);
      }
      return 0;
    }
    // 3. Device-Specific Configuration (0x0200..)
    if (offset >= MODERN_DEVICE_CFG_OFFSET && offset < MODERN_DEVICE_CFG_OFFSET + this.deviceConfigSize) {
      return this.readDeviceConfig(offset - MODERN_DEVICE_CFG_OFFSET, size);
    }
    return size === 4 ? 0xFFFFFFFF : 0xFF;
  }

  writeModernBar1(offset, val, size = 4) {
    // 1. Common Configuration
    if (offset >= MODERN_COMMON_CFG_OFFSET && offset < MODERN_COMMON_CFG_OFFSET + MODERN_COMMON_CFG_SIZE) {
      this.writeCommonCfg(offset - MODERN_COMMON_CFG_OFFSET, val, size);
      return;
    }
    // 2. Queue Notify Doorbell (0x0400..)
    if (offset >= MODERN_NOTIFY_CFG_OFFSET && offset < MODERN_NOTIFY_CFG_OFFSET + MODERN_NOTIFY_CFG_SIZE) {
      const qIndex = (offset - MODERN_NOTIFY_CFG_OFFSET) >> 2;
      this.notifyQueue(qIndex);
      return;
    }
    // 3. Device-Specific Configuration
    if (offset >= MODERN_DEVICE_CFG_OFFSET && offset < MODERN_DEVICE_CFG_OFFSET + this.deviceConfigSize) {
      this.writeDeviceConfig(offset - MODERN_DEVICE_CFG_OFFSET, val, size);
      return;
    }
  }

  readCommonCfg(offset, size = 4) {
    const q = this.queues[this.selectedQueueIdx] || null;
    switch (offset) {
      case 0x00: return this.deviceFeatureSelect;
      case 0x04: {
        const shift = BigInt(this.deviceFeatureSelect * 32);
        return Number((this.deviceFeatures >> shift) & 0xFFFFFFFFn);
      }
      case 0x08: return this.driverFeatureSelect;
      case 0x0C: {
        const shift = BigInt(this.driverFeatureSelect * 32);
        return Number((this.driverFeatures >> shift) & 0xFFFFFFFFn);
      }
      case 0x10: return 0xFFFF; // config_msix_vector
      case 0x12: return this.queues.length; // num_queues
      case 0x14: return this.deviceStatus;
      case 0x15: return this.configGeneration;
      case 0x16: return this.selectedQueueIdx;
      case 0x18: return q ? q.size : 0;
      case 0x1A: return 0xFFFF; // queue_msix_vector
      case 0x1C: return q && q.enabled ? 1 : 0;
      case 0x1E: return this.selectedQueueIdx; // queue_notify_off
      case 0x20: return q ? Number(q.descTableAddr & 0xFFFFFFFFn) : 0;
      case 0x24: return q ? Number((q.descTableAddr >> 32n) & 0xFFFFFFFFn) : 0;
      case 0x28: return q ? Number(q.availRingAddr & 0xFFFFFFFFn) : 0;
      case 0x2C: return q ? Number((q.availRingAddr >> 32n) & 0xFFFFFFFFn) : 0;
      case 0x30: return q ? Number(q.usedRingAddr & 0xFFFFFFFFn) : 0;
      case 0x34: return q ? Number((q.usedRingAddr >> 32n) & 0xFFFFFFFFn) : 0;
      default: return 0;
    }
  }

  writeCommonCfg(offset, val, size = 4) {
    const q = this.queues[this.selectedQueueIdx] || null;
    switch (offset) {
      case 0x00: this.deviceFeatureSelect = val & 1; break;
      case 0x08: this.driverFeatureSelect = val & 1; break;
      case 0x0C: {
        const mask = 0xFFFFFFFFn;
        const shift = BigInt(this.driverFeatureSelect * 32);
        this.driverFeatures = (this.driverFeatures & ~(mask << shift)) | (BigInt(val >>> 0) << shift);
        break;
      }
      case 0x14: this.setDeviceStatus(val & 0xFF); break;
      case 0x16: this.selectedQueueIdx = Math.min(val, Math.max(0, this.queues.length - 1)); break;
      case 0x18: if (q) q.setSize(val); break;
      case 0x1C: if (q) q.setEnabled((val & 1) !== 0); break;
      case 0x20: if (q) q.setDescTableLow(val >>> 0); break;
      case 0x24: if (q) q.setDescTableHigh(val >>> 0); break;
      case 0x28: if (q) q.setAvailRingLow(val >>> 0); break;
      case 0x2C: if (q) q.setAvailRingHigh(val >>> 0); break;
      case 0x30: if (q) q.setUsedRingLow(val >>> 0); break;
      case 0x34: if (q) q.setUsedRingHigh(val >>> 0); break;
    }
  }

  setDeviceStatus(status) {
    if (status === 0) {
      this.reset();
      return;
    }

    const previousStatus = this.deviceStatus;
    this.deviceStatus = status;

    // OASIS §2.2: Feature Handshake Verification
    if ((status & VIRTIO_STATUS_FEATURES_OK) && !(previousStatus & VIRTIO_STATUS_FEATURES_OK)) {
      // Must reject if unoffered feature requested, or if VIRTIO_F_VERSION_1 is missing
      const unoffered = (this.driverFeatures & ~this.deviceFeatures) !== 0n;
      const missingVersion1 = (this.driverFeatures & VIRTIO_F_VERSION_1) === 0n;

      if (unoffered || missingVersion1) {
        this.deviceStatus &= ~VIRTIO_STATUS_FEATURES_OK;
      }
    }

    if (status & VIRTIO_STATUS_DRIVER_OK) {
      this.onDriverOk();
    }
  }

  // --- Legacy BAR0 (Transitional I/O Ports) ---

  readLegacyBar0(offset, size = 4) {
    const q = this.queues[this.selectedQueueIdx] || null;
    switch (offset) {
      case 0x00: return Number(this.deviceFeatures & 0xFFFFFFFFn);
      case 0x04: return Number(this.driverFeatures & 0xFFFFFFFFn);
      case 0x08: return q ? Number(q.descTableAddr >> 12n) : 0; // PFN
      case 0x0C: return q ? q.size : 0;
      case 0x0E: return this.selectedQueueIdx;
      case 0x12: return this.deviceStatus;
      case 0x13: return this.bus && this.bus.irqController ? this.bus.irqController.readIsrStatus(this) : 0;
      default:
        if (offset >= 0x14 && offset < 0x14 + this.deviceConfigSize) {
          return this.readDeviceConfig(offset - 0x14, size);
        }
        return size === 4 ? 0xFFFFFFFF : 0xFF;
    }
  }

  writeLegacyBar0(offset, val, size = 4) {
    const q = this.queues[this.selectedQueueIdx] || null;
    switch (offset) {
      case 0x04:
        this.driverFeatures = (this.driverFeatures & ~0xFFFFFFFFn) | BigInt(val >>> 0);
        break;
      case 0x08:
        if (q) {
          const pfn = BigInt(val >>> 0);
          if (pfn === 0n) {
            q.setEnabled(false);
          } else {
            // Legacy layout: desc at PFN, avail at PFN + 16*size, used at page aligned
            const base = pfn << 12n;
            q.setupLegacyAddresses(base);
            q.setEnabled(true);
          }
        }
        break;
      case 0x0E:
        this.selectedQueueIdx = Math.min(val, Math.max(0, this.queues.length - 1));
        break;
      case 0x10:
        this.notifyQueue(val);
        break;
      case 0x12:
        this.setDeviceStatus(val & 0xFF);
        break;
      default:
        if (offset >= 0x14 && offset < 0x14 + this.deviceConfigSize) {
          this.writeDeviceConfig(offset - 0x14, val, size);
        }
    }
  }

  notifyQueue(qIndex) {
    const q = this.queues[qIndex];
    if (q && q.enabled) {
      this.onQueueNotify(qIndex);
    }
  }

  reset() {
    this.deviceStatus = 0;
    this.driverFeatures = 0n;
    this.selectedQueueIdx = 0;
    for (const q of this.queues) {
      q.reset();
    }
    if (this.bus && this.bus.irqController) {
      this.bus.irqController.clearDevice(this);
    }
    this.onReset();
  }

  // --- Abstract hooks for child devices ---

  onQueueNotify(_queueIdx) {}
  onDriverOk() {}
  onReset() {}
  readDeviceConfig(_offset, _size) { return 0; }
  writeDeviceConfig(_offset, _val, _size) {}

  /**
   * True when the driver negotiated VIRTIO_F_RING_EVENT_IDX (both offered
   * and accepted). Devices must pass this to Virtqueue.shouldNotify().
   * @returns {boolean}
   */
  eventIdxNegotiated() {
    return (this.deviceFeatures & VIRTIO_F_RING_EVENT_IDX) !== 0n
      && (this.driverFeatures & VIRTIO_F_RING_EVENT_IDX) !== 0n;
  }

  /**
   * True when the driver negotiated VIRTIO_F_RING_INDIRECT_DESC.
   * @returns {boolean}
   */
  indirectNegotiated() {
    return (this.deviceFeatures & VIRTIO_F_RING_INDIRECT_DESC) !== 0n
      && (this.driverFeatures & VIRTIO_F_RING_INDIRECT_DESC) !== 0n;
  }
}
