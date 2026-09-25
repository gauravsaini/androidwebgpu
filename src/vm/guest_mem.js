/**
 * guest_mem.js - Entity E12: Guest Physical Memory Subsystem
 * Provides bounds-checked 64-bit BigInt DMA access, primitive scalar operations,
 * direct views, and scatter-gather operations for x86 VM and Virtio devices.
 */

export class GuestMemError extends Error {
  constructor(message, code) {
    super(`${message} [${code}]`);
    this.name = 'GuestMemError';
    this.code = code;
  }
}

export class DmaOutOfBoundsError extends GuestMemError {
  constructor(paddr, length, ramSize) {
    const paddrHex = typeof paddr === 'bigint' ? paddr.toString(16) : BigInt(paddr >>> 0).toString(16);
    const lenHex = typeof length === 'bigint' ? length.toString(16) : Number(length).toString(16);
    const ramHex = typeof ramSize === 'bigint' ? ramSize.toString(16) : Number(ramSize).toString(16);
    super(
      `DMA out of bounds: paddr=0x${paddrHex}, len=0x${lenHex}, ramSize=0x${ramHex}`,
      'DMA_OUT_OF_BOUNDS'
    );
    this.paddr = paddr;
    this.length = length;
    this.ramSize = ramSize;
  }
}

export class DmaAlignmentError extends GuestMemError {
  constructor(paddr, alignment) {
    const paddrHex = typeof paddr === 'bigint' ? paddr.toString(16) : BigInt(paddr >>> 0).toString(16);
    super(
      `DMA unaligned access: paddr=0x${paddrHex}, required alignment=${alignment}`,
      'DMA_UNALIGNED'
    );
    this.paddr = paddr;
    this.alignment = alignment;
  }
}

export class GuestMem {
  /**
   * @param {number|bigint} ramSize - Total RAM size in bytes.
   * @param {SharedArrayBuffer|ArrayBuffer} [existingBuffer] - Optional shared or preallocated buffer.
   */
  constructor(ramSize, existingBuffer = null) {
    this.ramSize = typeof ramSize === 'bigint' ? Number(ramSize) : Number(ramSize);
    this.ramSizeBigInt = BigInt(this.ramSize);

    if (existingBuffer) {
      if (existingBuffer.byteLength < this.ramSize) {
        throw new GuestMemError(
          `Buffer byteLength (${existingBuffer.byteLength}) is smaller than ramSize (${this.ramSize})`,
          'BUFFER_TOO_SMALL'
        );
      }
      this.buffer = existingBuffer;
    } else {
      // Allocate SharedArrayBuffer if supported, else ArrayBuffer fallback
      try {
        if (typeof SharedArrayBuffer !== 'undefined') {
          this.buffer = new SharedArrayBuffer(this.ramSize);
        } else {
          this.buffer = new ArrayBuffer(this.ramSize);
        }
      } catch (_err) {
        this.buffer = new ArrayBuffer(this.ramSize);
      }
    }

    this.isShared = typeof SharedArrayBuffer !== 'undefined' && this.buffer instanceof SharedArrayBuffer;
    this.u8 = new Uint8Array(this.buffer, 0, this.ramSize);
    this.dataView = new DataView(this.buffer, 0, this.ramSize);
  }

  /**
   * Validate that [paddr, paddr + length) is entirely within RAM bounds using 64-bit BigInt arithmetic.
   * Rejects negative offsets, negative lengths, range exceeding ramSize, and 64-bit integer wrap-around.
   * @param {number|bigint} paddr
   * @param {number|bigint} length
   * @returns {number} Offset as safe number within buffer
   */
  validateRange(paddr, length) {
    let p;
    let len;
    try {
      p = typeof paddr === 'bigint' ? paddr : BigInt(Math.trunc(Number(paddr)));
      len = typeof length === 'bigint' ? length : BigInt(Math.trunc(Number(length)));
    } catch (_e) {
      throw new DmaOutOfBoundsError(paddr, length, this.ramSize);
    }

    if (p < 0n || len < 0n) {
      throw new DmaOutOfBoundsError(paddr, length, this.ramSize);
    }

    const end = p + len;
    // Checked arithmetic: wrap-around check and upper bound check
    if (end < p || end > this.ramSizeBigInt) {
      throw new DmaOutOfBoundsError(paddr, length, this.ramSize);
    }

    return Number(p);
  }

  // --- Primitive Scalar Reads (Little-Endian default) ---

  readU8(paddr) {
    const offset = this.validateRange(paddr, 1);
    return this.dataView.getUint8(offset);
  }

  readU16(paddr, littleEndian = true) {
    const offset = this.validateRange(paddr, 2);
    return this.dataView.getUint16(offset, littleEndian);
  }

  readU32(paddr, littleEndian = true) {
    const offset = this.validateRange(paddr, 4);
    return this.dataView.getUint32(offset, littleEndian);
  }

  readU64(paddr, littleEndian = true) {
    const offset = this.validateRange(paddr, 8);
    return this.dataView.getBigUint64(offset, littleEndian);
  }

  // --- Primitive Scalar Writes (Little-Endian default) ---

  writeU8(paddr, val) {
    const offset = this.validateRange(paddr, 1);
    this.dataView.setUint8(offset, Number(val) & 0xFF);
  }

  writeU16(paddr, val, littleEndian = true) {
    const offset = this.validateRange(paddr, 2);
    this.dataView.setUint16(offset, Number(val) & 0xFFFF, littleEndian);
  }

  writeU32(paddr, val, littleEndian = true) {
    const offset = this.validateRange(paddr, 4);
    this.dataView.setUint32(offset, (Number(val) >>> 0), littleEndian);
  }

  writeU64(paddr, val, littleEndian = true) {
    const offset = this.validateRange(paddr, 8);
    const bVal = typeof val === 'bigint' ? val : BigInt(val);
    this.dataView.setBigUint64(offset, bVal, littleEndian);
  }

  // --- Block & View Operations ---

  /**
   * Returns a Uint8Array view directly referencing the guest RAM slice.
   * Zero-copy: mutating this view mutates guest RAM directly.
   * @param {number|bigint} paddr
   * @param {number} length
   * @returns {Uint8Array}
   */
  getDirectView(paddr, length) {
    const offset = this.validateRange(paddr, length);
    return new Uint8Array(this.buffer, offset, Number(length));
  }

  /**
   * Returns a copied slice of guest memory.
   * @param {number|bigint} paddr
   * @param {number} length
   * @returns {Uint8Array}
   */
  readBytes(paddr, length) {
    const offset = this.validateRange(paddr, length);
    return new Uint8Array(this.u8.subarray(offset, offset + Number(length)));
  }

  /**
   * Copies bytes from an external Uint8Array into guest RAM.
   * @param {number|bigint} paddr
   * @param {Uint8Array} src
   * @param {number} [srcOffset=0]
   * @param {number|null} [length=null]
   */
  writeBytes(paddr, src, srcOffset = 0, length = null) {
    const len = length !== null ? Number(length) : (src.byteLength - srcOffset);
    const offset = this.validateRange(paddr, len);
    this.u8.set(src.subarray(srcOffset, srcOffset + len), offset);
  }

  /**
   * Scatter-gather copy across multiple guest physical memory segments.
   * @param {Array<{paddr: bigint|number, len: number}>} entries
   * @param {Uint8Array} targetBuffer
   * @param {'READ'|'WRITE'} direction - READ: Guest RAM -> targetBuffer, WRITE: targetBuffer -> Guest RAM
   * @returns {number} Total bytes transferred
   */
  scatterGather(entries, targetBuffer, direction) {
    let targetOffset = 0;
    let totalBytes = 0;

    for (const entry of entries) {
      const entryLen = Number(entry.len);
      if (entryLen <= 0) continue;
      if (targetOffset + entryLen > targetBuffer.byteLength) {
        throw new GuestMemError(
          `ScatterGather target buffer overflow: need ${targetOffset + entryLen}, have ${targetBuffer.byteLength}`,
          'TARGET_BUFFER_OVERFLOW'
        );
      }

      const paddrOffset = this.validateRange(entry.paddr, entryLen);

      if (direction === 'READ') {
        targetBuffer.set(this.u8.subarray(paddrOffset, paddrOffset + entryLen), targetOffset);
      } else if (direction === 'WRITE') {
        this.u8.set(targetBuffer.subarray(targetOffset, targetOffset + entryLen), paddrOffset);
      } else {
        throw new GuestMemError(`Invalid scatter-gather direction: ${direction}`, 'INVALID_DIRECTION');
      }

      targetOffset += entryLen;
      totalBytes += entryLen;
    }

    return totalBytes;
  }

  /**
   * Zero-fill a memory range.
   * @param {number|bigint} paddr
   * @param {number} length
   */
  zero(paddr, length) {
    const offset = this.validateRange(paddr, length);
    this.u8.fill(0, offset, offset + Number(length));
  }
}
