/**
 * Bounds-checked Guest Physical Memory Oracle & DMA View Validator
 * Implements contract specification for F-VM-02 and Virtio DMA access.
 */
export class GuestMemOracle {
  constructor(sizeBytes = 256 * 1024 * 1024) { // 256MB default
    if (sizeBytes <= 0 || sizeBytes > 0xFFFFFFFF) {
      throw new RangeError(`INVALID_RAM_SIZE:${sizeBytes}`);
    }
    this.sizeBytes = sizeBytes;
    this.buffer = new ArrayBuffer(sizeBytes);
    this.view = new DataView(this.buffer);
    this.bytes = new Uint8Array(this.buffer);
  }

  checkBounds(paddr, length) {
    if (paddr < 0 || length < 0) {
      throw new RangeError(`DMA_NEGATIVE_ADDR_OR_LEN:addr=${paddr},len=${length}`);
    }
    // Check 32-bit wrap-around
    const end = paddr + length;
    if (end < paddr || end > 0xFFFFFFFF) {
      throw new RangeError(`DMA_WRAPAROUND_OVERFLOW:addr=${paddr},len=${length}`);
    }
    if (end > this.sizeBytes) {
      throw new RangeError(`DMA_OUT_OF_BOUNDS:addr=${paddr},len=${length},ramSize=${this.sizeBytes}`);
    }
  }

  readU8(paddr) {
    this.checkBounds(paddr, 1);
    return this.view.getUint8(paddr);
  }

  writeU8(paddr, val) {
    this.checkBounds(paddr, 1);
    this.view.setUint8(paddr, val & 0xFF);
  }

  readU16(paddr) {
    this.checkBounds(paddr, 2);
    return this.view.getUint16(paddr, true);
  }

  writeU16(paddr, val) {
    this.checkBounds(paddr, 2);
    this.view.setUint16(paddr, val & 0xFFFF, true);
  }

  readU32(paddr) {
    this.checkBounds(paddr, 4);
    return this.view.getUint32(paddr, true);
  }

  writeU32(paddr, val) {
    this.checkBounds(paddr, 4);
    this.view.setUint32(paddr, val >>> 0, true);
  }

  readBytes(paddr, length) {
    this.checkBounds(paddr, length);
    return this.bytes.subarray(paddr, paddr + length);
  }

  writeBytes(paddr, src) {
    this.checkBounds(paddr, src.byteLength || src.length);
    this.bytes.set(src, paddr);
  }

  fill(paddr, length, value) {
    this.checkBounds(paddr, length);
    this.bytes.fill(value, paddr, paddr + length);
  }
}
