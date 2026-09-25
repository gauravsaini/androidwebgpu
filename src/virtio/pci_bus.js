/**
 * pci_bus.js - Entity E05: PCI Host Bridge & Configuration Router
 * Routes standard x86 PCI Configuration Mechanism 1 (I/O ports 0xCF8 / 0xCFC..0xCFF),
 * manages 32 device slots on Bus 0, and dispatches dynamic I/O and MMIO BAR accesses.
 */

export const PCI_CONFIG_ADDRESS_PORT = 0x0CF8;
export const PCI_CONFIG_DATA_PORT    = 0x0CFC;

export const PCI_ADDR_ENABLE_MASK = 0x80000000;
export const PCI_ADDR_BUS_MASK    = 0x00FF0000;
export const PCI_ADDR_SLOT_MASK   = 0x0000F800;
export const PCI_ADDR_FUNC_MASK   = 0x00000700;
export const PCI_ADDR_REG_MASK    = 0x000000FC;

export const PCI_BAR_TYPE_MEMORY  = 0x00;
export const PCI_BAR_TYPE_IO      = 0x01;
export const PCI_BAR_MEM_64BIT    = 0x04;
export const PCI_BAR_PREFETCHABLE = 0x08;

export class PciBus {
  /**
   * @param {import('./irq_controller.js').IrqController} irqController
   */
  constructor(irqController) {
    this.irqController = irqController;

    /** @type {Array<any>} 32 slots on PCI Bus 0 */
    this.slots = new Array(32).fill(null);

    /** @type {number} Latched 32-bit CONFIG_ADDRESS register */
    this.configAddress = 0;

    /** @type {Array<{slot: number, barIndex: number, type: 'io', base: number, size: number, device: any}>} */
    this.ioBars = [];

    /** @type {Array<{slot: number, barIndex: number, type: 'mmio', base: number, size: number, device: any}>} */
    this.mmioBars = [];
  }

  /**
   * Register a Virtio PCI device on Bus 0.
   * @param {number} slot - Slot index (0..31)
   * @param {any} device - VirtioPciDevice instance
   */
  registerDevice(slot, device) {
    if (slot < 0 || slot >= 32) {
      throw new RangeError(`Invalid PCI slot: ${slot}`);
    }
    if (this.slots[slot]) {
      throw new Error(`PCI slot ${slot} already occupied`);
    }
    this.slots[slot] = device;
    if (typeof device.attachToBus === 'function') {
      device.attachToBus(this, slot);
    }
    this.rebuildBarMappings();
  }

  /**
   * Get device registered at slot.
   * @param {number} slot
   * @returns {any}
   */
  getDevice(slot) {
    return this.slots[slot] || null;
  }

  /**
   * Rebuild internal fast-lookup BAR routing tables after BAR reallocation.
   */
  rebuildBarMappings() {
    this.ioBars = [];
    this.mmioBars = [];
    for (let slot = 0; slot < 32; slot++) {
      const dev = this.slots[slot];
      if (!dev || typeof dev.getActiveBars !== 'function') continue;
      for (const bar of dev.getActiveBars()) {
        if (bar.type === 'io') {
          this.ioBars.push({ slot, barIndex: bar.index, type: 'io', base: bar.base, size: bar.size, device: dev });
        } else {
          this.mmioBars.push({ slot, barIndex: bar.index, type: 'mmio', base: bar.base, size: bar.size, device: dev });
        }
      }
    }
  }

  // --- I/O Port 0xCF8 / 0xCFC PCI Configuration Access ---

  readConfigAddress() {
    return this.configAddress >>> 0;
  }

  writeConfigAddress(val) {
    this.configAddress = val >>> 0;
  }

  /**
   * Access PCI Configuration Space Data (Port 0xCFC..0xCFF).
   * @param {number} port - 0xCFC, 0xCFD, 0xCFE, or 0xCFF
   * @param {number} size - 1, 2, or 4 bytes
   * @returns {number}
   */
  readConfigData(port, size = 4) {
    if ((this.configAddress & PCI_ADDR_ENABLE_MASK) === 0) {
      return size === 4 ? 0xFFFFFFFF : (size === 2 ? 0xFFFF : 0xFF);
    }
    const bus = (this.configAddress & PCI_ADDR_BUS_MASK) >>> 16;
    const slot = (this.configAddress & PCI_ADDR_SLOT_MASK) >>> 11;
    const func = (this.configAddress & PCI_ADDR_FUNC_MASK) >>> 8;
    const reg = (this.configAddress & PCI_ADDR_REG_MASK);
    const subOffset = port - PCI_CONFIG_DATA_PORT;
    const offset = reg + subOffset;

    if (bus !== 0 || func !== 0 || !this.slots[slot]) {
      return size === 4 ? 0xFFFFFFFF : (size === 2 ? 0xFFFF : 0xFF);
    }
    return this.slots[slot].readPciConfig(offset, size);
  }

  /**
   * Write PCI Configuration Space Data (Port 0xCFC..0xCFF).
   * @param {number} port - 0xCFC, 0xCFD, 0xCFE, or 0xCFF
   * @param {number} val
   * @param {number} size - 1, 2, or 4 bytes
   */
  writeConfigData(port, val, size = 4) {
    if ((this.configAddress & PCI_ADDR_ENABLE_MASK) === 0) return;
    const bus = (this.configAddress & PCI_ADDR_BUS_MASK) >>> 16;
    const slot = (this.configAddress & PCI_ADDR_SLOT_MASK) >>> 11;
    const func = (this.configAddress & PCI_ADDR_FUNC_MASK) >>> 8;
    const reg = (this.configAddress & PCI_ADDR_REG_MASK);
    const subOffset = port - PCI_CONFIG_DATA_PORT;
    const offset = reg + subOffset;

    if (bus !== 0 || func !== 0 || !this.slots[slot]) return;
    this.slots[slot].writePciConfig(offset, val, size);
  }

  // --- Dynamic BAR Space Dispatch ---

  /**
   * Route I/O port read to matching active I/O BAR.
   * @param {number} port
   * @param {number} size
   * @returns {number}
   */
  readIo(port, size = 4) {
    for (const mapping of this.ioBars) {
      if (port >= mapping.base && port < mapping.base + mapping.size) {
        return mapping.device.readBar(mapping.barIndex, port - mapping.base, size);
      }
    }
    return size === 4 ? 0xFFFFFFFF : (size === 2 ? 0xFFFF : 0xFF);
  }

  /**
   * Route I/O port write to matching active I/O BAR.
   * @param {number} port
   * @param {number} val
   * @param {number} size
   */
  writeIo(port, val, size = 4) {
    for (const mapping of this.ioBars) {
      if (port >= mapping.base && port < mapping.base + mapping.size) {
        mapping.device.writeBar(mapping.barIndex, port - mapping.base, val, size);
        return;
      }
    }
  }

  /**
   * Route MMIO read to matching active MMIO BAR.
   * @param {number|bigint} paddr
   * @param {number} size
   * @returns {number}
   */
  readMmio(paddr, size = 4) {
    const addr = typeof paddr === 'bigint' ? Number(paddr) : Number(paddr);
    for (const mapping of this.mmioBars) {
      if (addr >= mapping.base && addr < mapping.base + mapping.size) {
        return mapping.device.readBar(mapping.barIndex, addr - mapping.base, size);
      }
    }
    return size === 4 ? 0xFFFFFFFF : (size === 2 ? 0xFFFF : 0xFF);
  }

  /**
   * Route MMIO write to matching active MMIO BAR.
   * @param {number|bigint} paddr
   * @param {number} val
   * @param {number} size
   */
  writeMmio(paddr, val, size = 4) {
    const addr = typeof paddr === 'bigint' ? Number(paddr) : Number(paddr);
    for (const mapping of this.mmioBars) {
      if (addr >= mapping.base && addr < mapping.base + mapping.size) {
        mapping.device.writeBar(mapping.barIndex, addr - mapping.base, val, size);
        return;
      }
    }
  }

  /**
   * Periodic tick callback for device timer servicing.
   */
  tick() {
    for (let i = 0; i < 32; i++) {
      if (this.slots[i] && typeof this.slots[i].tick === 'function') {
        this.slots[i].tick();
      }
    }
  }
}
