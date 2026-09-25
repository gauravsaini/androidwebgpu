/**
 * irq_controller.js - Entity E05: Virtio & PCI Interrupt Controller Subsystem
 * Manages latched ISRStatus registers, shared PCI INTx lines with reference counting,
 * and level-triggered VM interrupt line dispatches.
 */

export const VIRTIO_ISR_QUEUE  = 0x01; // Buffer used interrupt (bit 0)
export const VIRTIO_ISR_CONFIG = 0x02; // Configuration change / error interrupt (bit 1)

export const PCI_COMMAND            = 0x04;
export const PCI_STATUS             = 0x06;
export const PCI_COMMAND_INTX_DIS   = 0x0400; // Bit 10: Interrupt Disable
export const PCI_STATUS_INTX_STATUS = 0x0008; // Bit 3: Interrupt Status

export class IrqController {
  /**
   * @param {((irqNumber: number, level: boolean) => void)|null} [vmIrqCallback=null]
   */
  constructor(vmIrqCallback = null) {
    this.vmIrqCallback = vmIrqCallback;

    /** @type {Map<any, number>} Map of device instance to latched ISR status byte */
    this.deviceIsr = new Map();

    /** @type {Map<number, Set<any>>} Map of IRQ line number to active asserting devices */
    this.activeIrqLines = new Map();
  }

  /**
   * Bind or replace the VM interrupt callback handler.
   * @param {(irqNumber: number, level: boolean) => void} cb
   */
  setVmIrqCallback(cb) {
    this.vmIrqCallback = cb;
  }

  /**
   * Direct IRQ assertion helper (e.g., for system platform IRQs).
   * @param {number} irq
   * @param {boolean} [level=true]
   */
  assertIrq(irq, level = true) {
    if (this.vmIrqCallback) {
      this.vmIrqCallback(irq, level);
    }
  }

  /**
   * Assert a virtio interrupt for a PCI device.
   * Latches ISRStatus bitmask, sets PCI Status INTX_STATUS bit,
   * and drives the physical PCI INTx line HIGH if not masked by PCI Command INTX_DIS.
   * @param {any} device - VirtioPciDevice instance
   * @param {number} isrBit - VIRTIO_ISR_QUEUE (0x01) or VIRTIO_ISR_CONFIG (0x02)
   */
  assertDeviceIrq(device, isrBit) {
    const current = this.deviceIsr.get(device) || 0;
    const updated = (current | isrBit) & 0xFF;
    this.deviceIsr.set(device, updated);

    // Update PCI Status register: INTX_STATUS bit 3
    if (typeof device.readPciConfig === 'function' && typeof device.writePciConfig === 'function') {
      const pciStatus = device.readPciConfig(PCI_STATUS, 2);
      device.writePciConfig(PCI_STATUS, pciStatus | PCI_STATUS_INTX_STATUS, 2);

      // Check PCI Command register: INTX_DIS bit 10
      const pciCommand = device.readPciConfig(PCI_COMMAND, 2);
      if ((pciCommand & PCI_COMMAND_INTX_DIS) !== 0) {
        // Interrupt is masked in PCI configuration space
        return;
      }
    }

    const irqLine = Number(device.irqLine ?? 10);
    let set = this.activeIrqLines.get(irqLine);
    if (!set) {
      set = new Set();
      this.activeIrqLines.set(irqLine, set);
    }

    const wasAsserted = set.size > 0;
    set.add(device);

    if (!wasAsserted && this.vmIrqCallback) {
      this.vmIrqCallback(irqLine, true);
    }
  }

  /**
   * Destructive read-to-clear: returns the latched ISR status byte for a device,
   * resets the latch to 0, clears PCI Status INTX_STATUS bit,
   * and de-asserts the PCI INTx line if no other devices share and hold it HIGH.
   * @param {any} device
   * @returns {number} The latched status byte before clearing
   */
  readIsrStatus(device) {
    const status = this.deviceIsr.get(device) || 0;
    this.deviceIsr.set(device, 0);

    // Clear PCI Status INTX_STATUS bit
    if (typeof device.readPciConfig === 'function' && typeof device.writePciConfig === 'function') {
      const pciStatus = device.readPciConfig(PCI_STATUS, 2);
      device.writePciConfig(PCI_STATUS, pciStatus & ~PCI_STATUS_INTX_STATUS, 2);
    }

    const irqLine = Number(device.irqLine ?? 10);
    const set = this.activeIrqLines.get(irqLine);
    if (set && set.has(device)) {
      set.delete(device);
      if (set.size === 0 && this.vmIrqCallback) {
        this.vmIrqCallback(irqLine, false);
      }
    }

    return status;
  }

  /**
   * Handle updates to the PCI Command register (such as INTX_DIS toggle).
   * @param {any} device
   * @param {number} newCommand
   */
  setCommandRegister(device, newCommand) {
    const irqLine = Number(device.irqLine ?? 10);
    let set = this.activeIrqLines.get(irqLine);
    if (!set) {
      set = new Set();
      this.activeIrqLines.set(irqLine, set);
    }

    const isMasked = (newCommand & PCI_COMMAND_INTX_DIS) !== 0;
    const hasPendingIsr = (this.deviceIsr.get(device) || 0) !== 0;

    if (isMasked) {
      if (set.has(device)) {
        set.delete(device);
        if (set.size === 0 && this.vmIrqCallback) {
          this.vmIrqCallback(irqLine, false);
        }
      }
    } else if (hasPendingIsr) {
      if (!set.has(device)) {
        const wasEmpty = set.size === 0;
        set.add(device);
        if (wasEmpty && this.vmIrqCallback) {
          this.vmIrqCallback(irqLine, true);
        }
      }
    }
  }

  /**
   * Clear all pending ISR status and remove device from active IRQ line sharing sets.
   * @param {any} device
   */
  clearDevice(device) {
    this.readIsrStatus(device);
  }
}
