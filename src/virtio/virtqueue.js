/**
 * virtqueue.js - Entity E05: Virtqueue Split Ring Engine
 * Implements OASIS Virtio 1.2 split virtqueues: descriptor chain parsing,
 * cycle and loop detection, indirect descriptor tables, checked DMA memory access,
 * and 1:1 used ring synchronization.
 */

export const VRING_DESC_F_NEXT     = 0x0001; // Buffer continues in next field
export const VRING_DESC_F_WRITE    = 0x0002; // Device write-only (guest read-only)
export const VRING_DESC_F_INDIRECT = 0x0004; // Buffer is an indirect descriptor table

export const VRING_AVAIL_F_NO_INTERRUPT = 0x0001;
export const VRING_USED_F_NO_NOTIFY     = 0x0001;

export const VRING_DESC_SIZE = 16;

export class VirtqueueError extends Error {
  constructor(message, code) {
    super(`${message} [${code}]`);
    this.name = 'VirtqueueError';
    this.code = code;
  }
}

export class Virtqueue {
  /**
   * @param {number} index - Queue index (0, 1, 2, ...)
   * @param {number} [maxSize=256] - Maximum allowed queue capacity (must be power of two)
   */
  constructor(index, maxSize = 256) {
    this.index = index;
    this.maxSize = maxSize;
    this.size = maxSize;
    this.enabled = false;

    // 64-bit Guest Physical Addresses
    this.descTableAddr = 0n;
    this.availRingAddr = 0n;
    this.usedRingAddr = 0n;

    // Monotonic 16-bit ring indices
    this.lastAvailIdx = 0;
    this.lastUsedIdx = 0;
    // Last used index at which the guest was notified (EVENT_IDX baseline).
    // Implements vring_need_event(new, event, old): interrupt iff
    // (new - event - 1) < (new - old) in u16 arithmetic.
    this.notifiedUsedIdx = 0;
  }

  /**
   * Configure queue size. Must be a power of two between 1 and maxSize.
   * @param {number} size
   */
  setSize(size) {
    const s = Number(size);
    if (!Number.isInteger(s) || (s & (s - 1)) !== 0 || s < 1 || s > this.maxSize) {
      throw new VirtqueueError(`Invalid queue size: ${size}`, 'INVALID_QUEUE_SIZE');
    }
    this.size = s;
  }

  /**
   * Enable or disable queue processing.
   * @param {boolean} enabled
   */
  setEnabled(enabled) {
    this.enabled = !!enabled;
  }

  // --- 64-bit Address Setters (Low/High 32-bit registers) ---

  setDescTableLow(low) {
    this.descTableAddr = (this.descTableAddr & 0xFFFFFFFF00000000n) | BigInt(low >>> 0);
  }

  setDescTableHigh(high) {
    this.descTableAddr = (this.descTableAddr & 0x00000000FFFFFFFFn) | (BigInt(high >>> 0) << 32n);
  }

  setAvailRingLow(low) {
    this.availRingAddr = (this.availRingAddr & 0xFFFFFFFF00000000n) | BigInt(low >>> 0);
  }

  setAvailRingHigh(high) {
    this.availRingAddr = (this.availRingAddr & 0x00000000FFFFFFFFn) | (BigInt(high >>> 0) << 32n);
  }

  setUsedRingLow(low) {
    this.usedRingAddr = (this.usedRingAddr & 0xFFFFFFFF00000000n) | BigInt(low >>> 0);
  }

  setUsedRingHigh(high) {
    this.usedRingAddr = (this.usedRingAddr & 0x00000000FFFFFFFFn) | (BigInt(high >>> 0) << 32n);
  }

  /**
   * Configure legacy contiguous ring layout from Page Frame Number (PFN).
   * Used Ring is aligned up to a 4096-byte page boundary per OASIS legacy spec.
   * @param {number|bigint} basePaddr - Physical address (PFN << 12)
   */
  setupLegacyAddresses(basePaddr) {
    const base = typeof basePaddr === 'bigint' ? basePaddr : BigInt(basePaddr);
    this.descTableAddr = base;
    this.availRingAddr = base + BigInt(16 * this.size);
    const unalignedUsed = this.availRingAddr + BigInt(4 + 2 * this.size);
    this.usedRingAddr = (unalignedUsed + 4095n) & ~4095n;
  }

  /**
   * Reset ring state, addresses, and indices to uninitialized state.
   */
  reset() {
    this.enabled = false;
    this.descTableAddr = 0n;
    this.availRingAddr = 0n;
    this.usedRingAddr = 0n;
    this.lastAvailIdx = 0;
    this.lastUsedIdx = 0;
    this.notifiedUsedIdx = 0;
  }

  // --- Descriptor Chain Parser & Validator ---

  /**
   * Check if available ring contains unconsumed descriptor chains.
   * @param {import('../vm/guest_mem.js').GuestMem} guestMem
   * @returns {boolean}
   */
  hasAvailable(guestMem) {
    if (!this.enabled || this.availRingAddr === 0n) return false;
    const availIdx = guestMem.readU16(this.availRingAddr + 2n);
    return ((this.lastAvailIdx & 0xFFFF) !== (availIdx & 0xFFFF));
  }

  /**
   * Consume next available descriptor chain with complete validation:
   * bounds check, loop detection via visited bitset, indirect tables, and read/write ordering.
   * @param {import('../vm/guest_mem.js').GuestMem} guestMem
   * @returns {{
   *   headIndex: number,
   *   readable: Array<{addr: bigint, len: number, isWrite: boolean}>,
   *   writable: Array<{addr: bigint, len: number, isWrite: boolean}>,
   *   totalReadLen: number,
   *   totalWriteLen: number
   * }|null}
   */
  popDescriptorChain(guestMem) {
    if (!this.hasAvailable(guestMem)) return null;

    // Read head index from avail.ring[lastAvailIdx % size]
    const ringSlot = this.lastAvailIdx % this.size;
    const headIndex = guestMem.readU16(this.availRingAddr + 4n + BigInt(ringSlot * 2));

    if (headIndex >= this.size) {
      throw new VirtqueueError(`Descriptor head index ${headIndex} >= size ${this.size}`, 'HEAD_INDEX_OUT_OF_BOUNDS');
    }

    const readable = [];
    const writable = [];
    let totalReadLen = 0;
    let totalWriteLen = 0;
    let seenWritable = false;

    const visited = new Set();
    let currIdx = headIndex;

    while (true) {
      if (visited.has(currIdx)) {
        throw new VirtqueueError(`Descriptor loop detected at index ${currIdx}`, 'DESCRIPTOR_LOOP_DETECTED');
      }
      if (visited.size >= this.size) {
        throw new VirtqueueError(`Descriptor chain length exceeded queue size ${this.size}`, 'DESCRIPTOR_CHAIN_TOO_LONG');
      }
      visited.add(currIdx);

      // Read descriptor: addr (u64), len (u32), flags (u16), next (u16)
      const descOffset = this.descTableAddr + BigInt(currIdx * VRING_DESC_SIZE);
      const addr = guestMem.readU64(descOffset);
      const len = guestMem.readU32(descOffset + 8n);
      const flags = guestMem.readU16(descOffset + 12n);
      const next = guestMem.readU16(descOffset + 14n);

      // Checked 64-bit DMA bounds validation
      guestMem.validateRange(addr, len);

      if (flags & VRING_DESC_F_INDIRECT) {
        // OASIS §2.6.5.3: Indirect descriptors cannot have VRING_DESC_F_WRITE or VRING_DESC_F_NEXT
        if (flags & (VRING_DESC_F_WRITE | VRING_DESC_F_NEXT)) {
          throw new VirtqueueError('Indirect descriptor must not set WRITE or NEXT flags', 'MALFORMED_INDIRECT_DESCRIPTOR');
        }
        if (len === 0 || (len % VRING_DESC_SIZE) !== 0) {
          throw new VirtqueueError(`Indirect table len ${len} is not a non-zero multiple of 16`, 'MALFORMED_INDIRECT_DESCRIPTOR');
        }
        const numIndirect = len / VRING_DESC_SIZE;
        if (numIndirect > this.size) {
          throw new VirtqueueError(`Indirect table size ${numIndirect} > queue size ${this.size}`, 'INVALID_INDIRECT_TABLE_SIZE');
        }

        // Parse indirect descriptor chain starting at table index 0
        const indirectRes = this.parseIndirectTable(guestMem, addr, numIndirect);
        readable.push(...indirectRes.readable);
        writable.push(...indirectRes.writable);
        totalReadLen += indirectRes.totalReadLen;
        totalWriteLen += indirectRes.totalWriteLen;
        break; // Indirect descriptor cannot continue via NEXT
      }

      const isWrite = (flags & VRING_DESC_F_WRITE) !== 0;
      if (isWrite) {
        seenWritable = true;
        writable.push({ addr, len, isWrite: true });
        totalWriteLen += len;
      } else {
        if (seenWritable) {
          throw new VirtqueueError('Malformed chain: read-only buffer follows write-only buffer', 'MALFORMED_DESCRIPTOR_CHAIN');
        }
        readable.push({ addr, len, isWrite: false });
        totalReadLen += len;
      }

      if (flags & VRING_DESC_F_NEXT) {
        if (next >= this.size) {
          throw new VirtqueueError(`Next descriptor index ${next} >= size ${this.size}`, 'OUT_OF_BOUNDS_DESCRIPTOR_NEXT');
        }
        currIdx = next;
      } else {
        break;
      }
    }

    // Monotonically advance available index
    this.lastAvailIdx = (this.lastAvailIdx + 1) & 0xFFFF;

    return {
      headIndex,
      readable,
      writable,
      totalReadLen,
      totalWriteLen
    };
  }

  /**
   * Parse and validate an indirect descriptor table.
   * @param {import('../vm/guest_mem.js').GuestMem} guestMem
   * @param {bigint} baseAddr
   * @param {number} count
   */
  parseIndirectTable(guestMem, baseAddr, count) {
    const visited = new Set();
    let curr = 0;
    let seenWritable = false;
    const readable = [];
    const writable = [];
    let totalReadLen = 0;
    let totalWriteLen = 0;

    while (true) {
      if (visited.has(curr)) {
        throw new VirtqueueError(`Indirect loop detected at index ${curr}`, 'DESCRIPTOR_LOOP_DETECTED');
      }
      if (visited.size >= count) {
        throw new VirtqueueError(`Indirect chain length exceeded table count ${count}`, 'DESCRIPTOR_CHAIN_TOO_LONG');
      }
      visited.add(curr);

      const offset = baseAddr + BigInt(curr * VRING_DESC_SIZE);
      const addr = guestMem.readU64(offset);
      const len = guestMem.readU32(offset + 8n);
      const flags = guestMem.readU16(offset + 12n);
      const next = guestMem.readU16(offset + 14n);

      // OASIS §2.6.5.3: A descriptor in an indirect table must not set VRING_DESC_F_INDIRECT
      if (flags & VRING_DESC_F_INDIRECT) {
        throw new VirtqueueError('Nested indirect descriptor rejected', 'NESTED_INDIRECT_TABLE_REJECTED');
      }

      guestMem.validateRange(addr, len);

      const isWrite = (flags & VRING_DESC_F_WRITE) !== 0;
      if (isWrite) {
        seenWritable = true;
        writable.push({ addr, len, isWrite: true });
        totalWriteLen += len;
      } else {
        if (seenWritable) {
          throw new VirtqueueError('Malformed indirect chain: read buffer follows write buffer', 'MALFORMED_DESCRIPTOR_CHAIN');
        }
        readable.push({ addr, len, isWrite: false });
        totalReadLen += len;
      }

      if (flags & VRING_DESC_F_NEXT) {
        if (next >= count) {
          throw new VirtqueueError(`Indirect next index ${next} >= count ${count}`, 'OUT_OF_BOUNDS_DESCRIPTOR_NEXT');
        }
        curr = next;
      } else {
        break;
      }
    }

    return { readable, writable, totalReadLen, totalWriteLen };
  }

  // --- Used Ring Synchronization ---

  /**
   * Commit completed descriptor chain to Used Ring.
   * INVARIANT: Exactly one used entry per consumed chain!
   * @param {number} headIndex - Head descriptor index from popDescriptorChain()
   * @param {number} writtenBytes - Total bytes written to device-writeable buffers
   * @param {import('../vm/guest_mem.js').GuestMem} guestMem
   */
  pushUsed(headIndex, writtenBytes, guestMem) {
    if (this.usedRingAddr === 0n) return;

    const usedSlot = this.lastUsedIdx % this.size;
    const elemOffset = this.usedRingAddr + 4n + BigInt(usedSlot * 8);

    // struct vring_used_elem: id (u32), len (u32)
    guestMem.writeU32(elemOffset, headIndex >>> 0);
    guestMem.writeU32(elemOffset + 4n, Number(writtenBytes) >>> 0);

    // Monotonically advance used.idx
    this.lastUsedIdx = (this.lastUsedIdx + 1) & 0xFFFF;
    guestMem.writeU16(this.usedRingAddr + 2n, this.lastUsedIdx);
  }

  /**
   * Determine whether device should trigger an interrupt to guest driver.
   * Evaluates VRING_AVAIL_F_NO_INTERRUPT and VIRTIO_F_RING_EVENT_IDX via
   * vring_need_event(new_used, event_idx, old_notified_used).
   * @param {import('../vm/guest_mem.js').GuestMem} guestMem
   * @param {boolean} [eventIdxNegotiated=false]
   * @returns {boolean}
   */
  shouldNotify(guestMem, eventIdxNegotiated = false) {
    if (this.availRingAddr === 0n) return false;

    if (!eventIdxNegotiated) {
      const availFlags = guestMem.readU16(this.availRingAddr);
      return (availFlags & VRING_AVAIL_F_NO_INTERRUPT) === 0;
    }
    // Event index check: avail_event is at usedRingAddr + 4 + 8*size
    if (this.usedRingAddr === 0n) return false;
    const availEvent = guestMem.readU16(this.usedRingAddr + 4n + BigInt(this.size * 8));
    const ne = (this.lastUsedIdx & 0xFFFF);
    const ev = (availEvent & 0xFFFF);
    const old = (this.notifiedUsedIdx & 0xFFFF);
    let need;
    if (ne === old) {
      // No progress since last notify: exact-match check so a driver
      // awaiting the already-reached index still gets its interrupt.
      need = (((ne - ev - 1) & 0xFFFF) < 1);
    } else {
      // vring_need_event(new, event, old): catches batched advances where
      // the exact-equality check would miss the interrupt.
      need = (((ne - ev - 1) & 0xFFFF) < ((ne - old) & 0xFFFF));
    }
    if (need) this.notifiedUsedIdx = ne;
    return need;
  }
}
