/**
 * virtio_blk.js - Entity E06: Virtio Block Device
 * Implements OASIS Virtio 1.2 Block Device specification (Subsystem ID 2, Device ID 0x1042 / 0x1001).
 * Supports VIRTIO_BLK_T_IN (read), VIRTIO_BLK_T_OUT (write), VIRTIO_BLK_T_FLUSH,
 * capacity/blk_size configuration space, and descriptor chain validation.
 */

import { VirtioPciDevice } from '../../virtio/virtio_pci_device.js';
import { Virtqueue } from '../../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../../virtio/irq_controller.js';
import { DiskActor, DEFAULT_SECTOR_SIZE } from '../../storage/disk_actor.js';

// Virtio Block Request Types (OASIS §5.2.6)
export const VIRTIO_BLK_T_IN           = 0;
export const VIRTIO_BLK_T_OUT          = 1;
export const VIRTIO_BLK_T_FLUSH        = 4;
export const VIRTIO_BLK_T_GET_ID       = 8;

// Virtio Block Status Codes (OASIS §5.2.6)
export const VIRTIO_BLK_S_OK     = 0;
export const VIRTIO_BLK_S_IOERR  = 1;
export const VIRTIO_BLK_S_UNSUPP = 2;

// Virtio Block Feature Bits (OASIS §5.2.3)
export const VIRTIO_BLK_F_SIZE_MAX  = 1n << 1n;
export const VIRTIO_BLK_F_SEG_MAX   = 1n << 2n;
export const VIRTIO_BLK_F_GEOMETRY  = 1n << 4n;
export const VIRTIO_BLK_F_RO        = 1n << 5n;
export const VIRTIO_BLK_F_BLK_SIZE  = 1n << 6n;
export const VIRTIO_BLK_F_FLUSH     = 1n << 9n;

export const DEFAULT_DISK_SECTOR_COUNT = 2097152; // 1 GB in 512B sectors

export class VirtioBlk extends VirtioPciDevice {
  /**
   * @param {Object} [options]
   * @param {DiskActor} [options.diskActor]
   * @param {number} [options.irqLine=11]
   * @param {number} [options.queueSize=128]
   * @param {boolean} [options.readOnly=false]
   * @param {string} [options.serial='ANDRODISK01']
   */
  constructor(options = {}) {
    const queueSize = options.queueSize || 128;
    const requestq = new Virtqueue(0, queueSize);

    let devFeatures = VIRTIO_BLK_F_SEG_MAX | VIRTIO_BLK_F_GEOMETRY | VIRTIO_BLK_F_BLK_SIZE | VIRTIO_BLK_F_FLUSH;
    if (options.readOnly) {
      devFeatures |= VIRTIO_BLK_F_RO;
    }

    super({
      name: 'virtio-blk',
      subsystemDeviceId: 2, // Device ID = 0x1042, Legacy = 0x1001
      pciClass: 0x018000,   // Mass Storage Controller
      irqLine: options.irqLine || 11,
      deviceFeatures: devFeatures,
      queues: [requestq],
      deviceConfigSize: 64
    });

    this.diskActor = options.diskActor || new DiskActor({ totalSectors: DEFAULT_DISK_SECTOR_COUNT });
    this.readOnly = !!options.readOnly;
    this.serial = options.serial || 'ANDRODISK01';
    this.blkSize = DEFAULT_SECTOR_SIZE;
    this.segMax = 128;
    this.sizeMax = 65536;

    this.isProcessing = false;
  }

  /**
   * Read from Device-Specific Configuration Space (OASIS §5.2.4)
   * 0x00..0x07: capacity (u64 in 512B sectors)
   * 0x08..0x0B: size_max (u32)
   * 0x0C..0x0F: seg_max (u32)
   * 0x10..0x11: cylinders (u16)
   * 0x12: heads (u8)
   * 0x13: sectors (u8)
   * 0x14..0x17: blk_size (u32)
   */
  readDeviceConfig(offset, size = 4) {
    const capacitySectors = BigInt(this.diskActor.getCapacitySectors());
    const buf = new ArrayBuffer(64);
    const view = new DataView(buf);

    view.setBigUint64(0x00, capacitySectors, true);
    view.setUint32(0x08, this.sizeMax, true);
    view.setUint32(0x0C, this.segMax, true);
    view.setUint16(0x10, 1024, true); // cylinders
    view.setUint8(0x12, 16);          // heads
    view.setUint8(0x13, 63);          // sectors
    view.setUint32(0x14, this.blkSize, true);

    if (offset + size <= 64) {
      if (size === 1) return view.getUint8(offset);
      if (size === 2) return view.getUint16(offset, true);
      if (size === 4) return view.getUint32(offset, true);
    }
    return 0;
  }

  writeDeviceConfig(_offset, _val, _size) {
    // Virtio-blk config space is read-only for guest driver
  }

  onQueueNotify(queueIdx) {
    if (queueIdx === 0) {
      this.processQueue();
    }
  }

  /**
   * Process all pending descriptor chains on requestq.
   * Handles asynchronous disk reads/writes while preserving request ordering.
   */
  async processQueue() {
    if (this.isProcessing) return;
    this.isProcessing = true;

    try {
      const vq = this.queues[0];
      if (!vq || !vq.enabled || !this.guestMem) return;

      while (vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        await this.handleRequestChain(vq, chain);
      }
    } finally {
      this.isProcessing = false;
      // If new requests arrived while processing, re-check
      const vq = this.queues[0];
      if (vq && vq.enabled && this.guestMem && vq.hasAvailable(this.guestMem)) {
        this.processQueue();
      }
    }
  }

  /**
   * Parse and execute a single Virtio block request chain.
   * @param {Virtqueue} vq
   * @param {Object} chain - Returned by vq.popDescriptorChain()
   */
  async handleRequestChain(vq, chain) {
    const { headIndex, readable, writable } = chain;

    // Minimum requirement: 1 readable header descriptor and at least 1 writable status descriptor
    if (readable.length === 0 || writable.length === 0) {
      // Malformed chain
      this.completeChain(vq, headIndex, 0);
      return;
    }

    // 1. Read virtio_blk_req header (16 bytes): type (u32), ioprio (u32), sector (u64)
    const headerDesc = readable[0];
    if (headerDesc.len < 16) {
      this.completeChain(vq, headIndex, 0);
      return;
    }

    const type = this.guestMem.readU32(headerDesc.addr);
    const sector = this.guestMem.readU64(headerDesc.addr + 8n);

    // 2. Identify the status byte: OASIS specifies the 1-byte status descriptor is at the end of writable descriptors
    const statusDesc = writable[writable.length - 1];
    const statusAddr = statusDesc.addr + BigInt(statusDesc.len - 1);

    // 3. Dispatch by request type
    switch (type) {
      case VIRTIO_BLK_T_IN: {
        // Read from disk into guest memory
        // Data buffers are writable descriptors excluding the last 1-byte status
        let totalDataBytes = 0;
        const dataEntries = [];
        for (let i = 0; i < writable.length - 1; i++) {
          dataEntries.push(writable[i]);
          totalDataBytes += writable[i].len;
        }
        // If status is part of the only writable descriptor:
        if (writable.length === 1 && writable[0].len > 1) {
          const dataLen = writable[0].len - 1;
          dataEntries.push({ addr: writable[0].addr, len: dataLen });
          totalDataBytes += dataLen;
        }

        if (totalDataBytes === 0 || totalDataBytes % this.blkSize !== 0) {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
          return;
        }

        const sectorCount = totalDataBytes / this.blkSize;
        try {
          const data = await this.diskActor.readSectors(sector, sectorCount);
          // Scatter read data across guest buffers
          let copied = 0;
          for (const entry of dataEntries) {
            const chunk = data.subarray(copied, copied + entry.len);
            this.guestMem.writeBytes(entry.addr, chunk);
            copied += entry.len;
          }
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_OK);
          this.completeChain(vq, headIndex, totalDataBytes + 1);
        } catch (_err) {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
        }
        break;
      }

      case VIRTIO_BLK_T_OUT: {
        // Write from guest memory into disk
        if (this.readOnly) {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
          return;
        }

        // Data buffers are readable descriptors after the 16-byte header
        let totalDataBytes = 0;
        const dataEntries = [];

        // Check if header descriptor contains trailing data
        if (headerDesc.len > 16) {
          const dataLen = headerDesc.len - 16;
          dataEntries.push({ addr: headerDesc.addr + 16n, len: dataLen });
          totalDataBytes += dataLen;
        }

        for (let i = 1; i < readable.length; i++) {
          dataEntries.push(readable[i]);
          totalDataBytes += readable[i].len;
        }

        if (totalDataBytes === 0 || totalDataBytes % this.blkSize !== 0) {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
          return;
        }

        const buffer = new Uint8Array(totalDataBytes);
        let offset = 0;
        for (const entry of dataEntries) {
          const bytes = this.guestMem.readBytes(entry.addr, entry.len);
          buffer.set(bytes, offset);
          offset += entry.len;
        }

        try {
          await this.diskActor.writeSectors(sector, buffer);
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_OK);
          this.completeChain(vq, headIndex, 1);
        } catch (_err) {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
        }
        break;
      }

      case VIRTIO_BLK_T_FLUSH: {
        try {
          await this.diskActor.flush();
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_OK);
          this.completeChain(vq, headIndex, 1);
        } catch (_err) {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
        }
        break;
      }

      case VIRTIO_BLK_T_GET_ID: {
        // Return device identification string (up to 20 ASCII chars)
        const serialBytes = new TextEncoder().encode(this.serial.slice(0, 20));
        const dest = writable[0];
        if (dest && dest.len > 1) {
          const copyLen = Math.min(serialBytes.length, dest.len - 1);
          this.guestMem.writeBytes(dest.addr, serialBytes.subarray(0, copyLen));
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_OK);
          this.completeChain(vq, headIndex, copyLen + 1);
        } else {
          this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_IOERR);
          this.completeChain(vq, headIndex, 1);
        }
        break;
      }

      default: {
        // Unsupported command
        this.guestMem.writeU8(statusAddr, VIRTIO_BLK_S_UNSUPP);
        this.completeChain(vq, headIndex, 1);
        break;
      }
    }
  }

  completeChain(vq, headIndex, writtenBytes) {
    vq.pushUsed(headIndex, writtenBytes, this.guestMem);
    if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus && this.bus.irqController) {
      this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
    }
  }
}
