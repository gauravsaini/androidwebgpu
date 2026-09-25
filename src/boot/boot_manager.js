/**
 * boot_manager.js - Entity E03 / E04: Linux Direct Boot Protocol Setup
 * Implements the Linux x86 32-bit Boot Protocol (Documentation/x86/boot.rst v2.10+):
 * Parses bzImage setup header, populates boot_params (zero-page), builds E820 memory map,
 * stages initrd, writes kernel cmdline, and configures initial protected mode CPU registers.
 */

export const LINUX_HDRS_MAGIC = 0x53726448; // "HdrS" in Little-Endian

export const BOOT_PARAMS_ADDR   = 0x00090000;
export const CMDLINE_ADDR       = 0x00020000;
export const CODE32_START_ADDR  = 0x00100000; // 1 MB physical

export const E820_RAM      = 1;
export const E820_RESERVED = 2;

export const INITIAL_CPU_REGISTERS = {
  cr0: 0x00000001,    // PE=1 (Protected Mode Enabled)
  cs: 0x0010,        // Flat 4GB Code Segment (Descriptor 2)
  ds: 0x0018,        // Flat 4GB Data Segment (Descriptor 3)
  es: 0x0018,
  fs: 0x0018,
  gs: 0x0018,
  ss: 0x0018,
  eip: CODE32_START_ADDR, // 0x00100000
  esi: BOOT_PARAMS_ADDR,  // 0x00090000 (Pointer to boot_params)
  esp: 0x0008FFF0,        // Stack pointer
  eflags: 0x00000002      // Bit 1 reserved, IF=0 (Interrupts disabled)
};

export class BootError extends Error {
  constructor(message, code = 'BOOT_ERROR') {
    super(message);
    this.name = 'BootError';
    this.code = code;
  }
}

export class InvalidKernelHeaderError extends BootError {
  constructor(message) {
    super(message, 'INVALID_KERNEL_HEADER');
    this.name = 'InvalidKernelHeaderError';
  }
}

export class BootManager {
  /**
   * @param {import('../vm/guest_mem.js').GuestMem} guestMem
   */
  constructor(guestMem) {
    this.guestMem = guestMem;
  }

  /**
   * Parse and validate Linux bzImage setup_header starting at offset 0x01F1.
   * @param {Uint8Array} kernel
   * @returns {{
   *   setupSects: number,
   *   setupSize: number,
   *   headerMagic: number,
   *   version: number,
   *   code32Start: number,
   *   initrdAddrMax: number
   * }}
   */
  parseSetupHeader(kernel) {
    if (kernel.byteLength < 0x0250) {
      throw new InvalidKernelHeaderError(`Kernel binary too small (${kernel.byteLength} bytes)`);
    }

    const view = new DataView(kernel.buffer, kernel.byteOffset, kernel.byteLength);
    const magic = view.getUint32(0x0202, true);

    if (magic !== LINUX_HDRS_MAGIC) {
      const magicHex = magic.toString(16).padStart(8, '0');
      throw new InvalidKernelHeaderError(
        `Invalid bzImage magic: expected 0x53726448 ("HdrS"), found 0x${magicHex}`
      );
    }

    const version = view.getUint16(0x0206, true);
    if (version < 0x0202) {
      throw new InvalidKernelHeaderError(`Boot protocol version 0x${version.toString(16)} < 0x0202 unsupported`);
    }

    let setupSects = view.getUint8(0x01F1);
    if (setupSects === 0) setupSects = 4;
    const setupSize = (setupSects + 1) * 512;

    const code32Start = view.getUint32(0x0214, true);
    let initrdAddrMax = 0x37FFFFFF;
    if (version >= 0x0203) {
      initrdAddrMax = view.getUint32(0x022C, true);
    }

    return {
      setupSects,
      setupSize,
      headerMagic: magic,
      version,
      code32Start: code32Start || CODE32_START_ADDR,
      initrdAddrMax
    };
  }

  /**
   * Set up Linux 32-bit direct boot in guest RAM.
   * @param {Object} options
   * @param {Uint8Array|ArrayBuffer} options.kernel - bzImage binary
   * @param {Uint8Array|ArrayBuffer|null} [options.initrd=null] - Optional initrd ramdisk
   * @param {string} [options.cmdline=''] - Kernel command line string
   * @returns {{
   *   entryIp: number,
   *   bootParamsAddr: number,
   *   registers: typeof INITIAL_CPU_REGISTERS
   * }}
   */
  setupLinuxBoot({ kernel, initrd = null, cmdline = '' }) {
    const kU8 = kernel instanceof Uint8Array ? kernel : new Uint8Array(kernel);
    const header = this.parseSetupHeader(kU8);

    if (this.guestMem.ramSize < 0x00400000) {
      throw new BootError(`Guest RAM (${this.guestMem.ramSize}B) too small for Linux kernel boot`);
    }

    // 1. Zero out Linux boot_params zero-page (4096 bytes at physical 0x00090000)
    this.guestMem.zero(BOOT_PARAMS_ADDR, 4096);

    // 2. Copy setup_header into boot_params
    const setupHeaderBytes = kU8.subarray(0x01F1, header.setupSize);
    this.guestMem.writeBytes(BOOT_PARAMS_ADDR + 0x01F1, setupHeaderBytes);

    // 3. Configure boot_params fields
    const bootView = new DataView(this.guestMem.buffer, BOOT_PARAMS_ADDR, 4096);

    bootView.setUint8(0x0210, 0xFF); // type_of_loader: 0xFF (custom bootloader)
    const loadFlags = bootView.getUint8(0x0211) | 0x21; // LOADED_HIGH (0x01) | CAN_USE_HEAP (0x20)
    bootView.setUint8(0x0211, loadFlags);
    bootView.setUint16(0x0224, 0x9E00, true); // heap_end_ptr
    bootView.setUint32(0x0228, CMDLINE_ADDR, true); // cmd_line_ptr: 0x00020000

    // 4. Write kernel command line string to CMDLINE_ADDR (0x00020000)
    const cmdlineStr = cmdline || 'console=ttyS0 root=/dev/ram0 rw';
    const cmdBytes = new TextEncoder().encode(cmdlineStr + '\0');
    this.guestMem.writeBytes(CMDLINE_ADDR, cmdBytes);

    // 5. Write protected mode kernel payload to CODE32_START_ADDR (0x00100000)
    const kernelPayload = kU8.subarray(header.setupSize);
    this.guestMem.writeBytes(header.code32Start, kernelPayload);

    // 6. Handle optional Initrd placement
    if (initrd) {
      const iU8 = initrd instanceof Uint8Array ? initrd : new Uint8Array(initrd);
      const initrdSize = iU8.byteLength;

      // Restrict initrd below initrdAddrMax and below guest RAM top
      const ramMax = this.guestMem.ramSize;
      const upperLimit = Math.min(header.initrdAddrMax, ramMax);

      if (initrdSize > upperLimit) {
        throw new BootError(`Initrd size (0x${initrdSize.toString(16)}) exceeds memory limit 0x${upperLimit.toString(16)}`);
      }

      // Page align downward
      const initrdStart = (upperLimit - initrdSize) & ~0xFFF;

      if (initrdStart < header.code32Start + kernelPayload.byteLength) {
        throw new BootError('Initrd collides with protected mode kernel code');
      }

      this.guestMem.writeBytes(initrdStart, iU8);
      bootView.setUint32(0x0218, initrdStart, true); // ramdisk_image
      bootView.setUint32(0x021C, initrdSize, true);  // ramdisk_size
    }

    // 7. Construct E820 memory map inside boot_params
    this.buildE820Map(BOOT_PARAMS_ADDR, this.guestMem.ramSize);

    return {
      entryIp: header.code32Start,
      bootParamsAddr: BOOT_PARAMS_ADDR,
      registers: {
        ...INITIAL_CPU_REGISTERS,
        eip: header.code32Start,
        esi: BOOT_PARAMS_ADDR
      }
    };
  }

  /**
   * Populate standard x86 E820 memory map inside boot_params.
   * Entry 0: 0x0000_0000 .. 0x0009_F000 (Usable conventional RAM, 636 KB)
   * Entry 1: 0x0009_F000 .. 0x0010_0000 (Reserved BIOS/VGA, 388 KB)
   * Entry 2: 0x0010_0000 .. ramSize (Usable extended high memory)
   * @param {number} bootParamsAddr
   * @param {number} ramSize
   */
  buildE820Map(bootParamsAddr, ramSize) {
    const view = new DataView(this.guestMem.buffer, bootParamsAddr, 4096);

    // Number of E820 entries at offset 0x01E8
    view.setUint8(0x01E8, 3);

    const E820_TABLE_OFFSET = 0x02D0;
    const ENTRY_SIZE = 20;

    // Entry 0: Low conventional memory (0x0 .. 0x9F000)
    let offset = E820_TABLE_OFFSET;
    view.setBigUint64(offset + 0, 0n, true);
    view.setBigUint64(offset + 8, 0x0009F000n, true);
    view.setUint32(offset + 16, E820_RAM, true);

    // Entry 1: Reserved BIOS & Video area (0x9F000 .. 0x100000)
    offset += ENTRY_SIZE;
    view.setBigUint64(offset + 0, 0x0009F000n, true);
    view.setBigUint64(offset + 8, 0x00061000n, true); // 0x100000 - 0x9F000 = 0x61000
    view.setUint32(offset + 16, E820_RESERVED, true);

    // Entry 2: High memory (0x100000 .. ramSize)
    offset += ENTRY_SIZE;
    view.setBigUint64(offset + 0, 0x00100000n, true);
    view.setBigUint64(offset + 8, BigInt(ramSize - 0x00100000), true);
    view.setUint32(offset + 16, E820_RAM, true);
  }
}
