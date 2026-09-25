/**
 * m1_vm_virtio_test.mjs - Comprehensive Unit & Fuzz Test Suite for Milestone 1
 * Covers:
 * 1. GuestMem 64-bit checked arithmetic, bounds checks, wrap protection, views, scatter-gather.
 * 2. PciBus 0xCF8/0xCFC config space, 32-slot router, BAR sizing and allocation.
 * 3. VirtioPciDevice modern capabilities linked-list discovery and legacy transitional BAR0.
 * 4. Virtqueue Split Ring Engine: valid chains, 1:1 used sync, shouldNotify,
 *    and error vectors G2-V01 through G2-V10 (self-loop, circular loop, out-of-RAM,
 *    64-bit overflow, nested indirect, unaligned length, read-after-write).
 * 5. IrqController latching, multi-event accumulation, read-to-clear, masking, line sharing.
 * 6. BootManager Linux direct boot protocol setup, header validation, E820 map, register setup.
 * 7. VmRuntime lifecycle (create, loadBoot, start, pause, resume, reset, destroy) and watchdog.
 */

import assert from 'node:assert/strict';
import {
  GuestMem,
  GuestMemError,
  DmaOutOfBoundsError
} from '../../src/vm/guest_mem.js';
import {
  PciBus,
  PCI_CONFIG_ADDRESS_PORT,
  PCI_CONFIG_DATA_PORT,
  PCI_ADDR_ENABLE_MASK
} from '../../src/virtio/pci_bus.js';
import {
  VirtioPciDevice,
  PCI_VENDOR_ID,
  PCI_DEVICE_ID,
  PCI_COMMAND,
  PCI_STATUS,
  PCI_CAPABILITY_LIST,
  PCI_BAR0,
  PCI_BAR1,
  VIRTIO_STATUS_ACKNOWLEDGE,
  VIRTIO_STATUS_DRIVER,
  VIRTIO_STATUS_FEATURES_OK,
  VIRTIO_STATUS_DRIVER_OK,
  VIRTIO_F_VERSION_1,
  VIRTIO_F_RING_INDIRECT_DESC,
  VIRTIO_F_RING_EVENT_IDX,
  MODERN_COMMON_CFG_OFFSET,
  MODERN_NOTIFY_CFG_OFFSET,
  MODERN_ISR_CFG_OFFSET,
  MODERN_DEVICE_CFG_OFFSET
} from '../../src/virtio/virtio_pci_device.js';
import {
  Virtqueue,
  VirtqueueError,
  VRING_DESC_F_NEXT,
  VRING_DESC_F_WRITE,
  VRING_DESC_F_INDIRECT,
  VRING_AVAIL_F_NO_INTERRUPT,
  VRING_DESC_SIZE
} from '../../src/virtio/virtqueue.js';
import {
  IrqController,
  VIRTIO_ISR_QUEUE,
  VIRTIO_ISR_CONFIG,
  PCI_COMMAND_INTX_DIS,
  PCI_STATUS_INTX_STATUS
} from '../../src/virtio/irq_controller.js';
import {
  BootManager,
  InvalidKernelHeaderError,
  BootError,
  LINUX_HDRS_MAGIC,
  BOOT_PARAMS_ADDR,
  CMDLINE_ADDR,
  CODE32_START_ADDR
} from '../../src/boot/boot_manager.js';
import {
  VmRuntime,
  VmStateError,
  VmWatchdogTimeoutError
} from '../../src/vm/vm_runtime.js';

let totalTests = 0;
let passedTests = 0;

function test(name, fn) {
  totalTests++;
  try {
    fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    console.error(`  ✗ ${name}`);
    console.error(err);
    process.exit(1);
  }
}

async function testAsync(name, fn) {
  totalTests++;
  try {
    await fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    console.error(`  ✗ ${name}`);
    console.error(err);
    process.exit(1);
  }
}

console.log('\n=== RUNNING MILESTONE 1 VM & VIRTIO TEST SUITE ===\n');

// ============================================================================
// SUITE 1: GuestMem Checked DMA & Scalar Access
// ============================================================================
console.log('--- Suite 1: GuestMem ---');

test('GuestMem: bounds checking and scalar read/write', () => {
  const ramSize = 16 * 1024 * 1024; // 16 MB
  const mem = new GuestMem(ramSize);

  assert.equal(mem.ramSize, ramSize);

  // Scalar writes and reads
  mem.writeU8(0x1000, 0xAB);
  assert.equal(mem.readU8(0x1000), 0xAB);

  mem.writeU16(0x2000, 0x1234);
  assert.equal(mem.readU16(0x2000), 0x1234);

  mem.writeU32(0x3000, 0xDEADBEEF);
  assert.equal(mem.readU32(0x3000), 0xDEADBEEF);

  mem.writeU64(0x4000, 0x0123456789ABCDEFn);
  assert.equal(mem.readU64(0x4000), 0x0123456789ABCDEFn);
});

test('GuestMem: boundary upper limit and out-of-bounds rejection', () => {
  const ramSize = 1024;
  const mem = new GuestMem(ramSize);

  // Valid access at the very last byte
  mem.writeU8(1023, 0xFF);
  assert.equal(mem.readU8(1023), 0xFF);

  // Exact boundary violation (paddr + len > ramSize)
  assert.throws(() => mem.readU16(1023), DmaOutOfBoundsError);
  assert.throws(() => mem.readU32(1021), DmaOutOfBoundsError);
  assert.throws(() => mem.readU64(1020), DmaOutOfBoundsError);
  assert.throws(() => mem.validateRange(1024, 1), DmaOutOfBoundsError);
  assert.throws(() => mem.validateRange(2000, 10), DmaOutOfBoundsError);
});

test('GuestMem: 64-bit integer wrap-around protection', () => {
  const mem = new GuestMem(1024 * 1024);

  // Wrap around BigInt 64-bit boundaries
  assert.throws(
    () => mem.validateRange(0xFFFFFFFFFFFFFFF0n, 0x20n),
    DmaOutOfBoundsError
  );
  assert.throws(
    () => mem.validateRange(0xFFFFFFFFFFFFFFFFn, 1n),
    DmaOutOfBoundsError
  );
  // Negative offsets / lengths
  assert.throws(() => mem.validateRange(-1n, 4n), DmaOutOfBoundsError);
  assert.throws(() => mem.validateRange(100n, -5n), DmaOutOfBoundsError);
});

test('GuestMem: direct views, byte copying, and scatter-gather', () => {
  const mem = new GuestMem(1024 * 1024);

  // Direct view zero-copy mutation
  const direct = mem.getDirectView(0x5000, 4);
  direct[0] = 0x11;
  direct[1] = 0x22;
  direct[2] = 0x33;
  direct[3] = 0x44;
  assert.equal(mem.readU32(0x5000, true), 0x44332211);

  // readBytes copy
  const copied = mem.readBytes(0x5000, 4);
  assert.deepEqual(Array.from(copied), [0x11, 0x22, 0x33, 0x44]);

  // Scatter-gather
  mem.writeBytes(0x1000, new Uint8Array([1, 2, 3]));
  mem.writeBytes(0x2000, new Uint8Array([4, 5]));
  mem.writeBytes(0x3000, new Uint8Array([6, 7, 8, 9]));

  const gathered = new Uint8Array(9);
  const transferred = mem.scatterGather(
    [
      { paddr: 0x1000n, len: 3 },
      { paddr: 0x2000n, len: 2 },
      { paddr: 0x3000n, len: 4 }
    ],
    gathered,
    'READ'
  );
  assert.equal(transferred, 9);
  assert.deepEqual(Array.from(gathered), [1, 2, 3, 4, 5, 6, 7, 8, 9]);

  // Scatter-gather WRITE
  const hostData = new Uint8Array([10, 20, 30, 40]);
  mem.scatterGather(
    [
      { paddr: 0x4000, len: 2 },
      { paddr: 0x4100, len: 2 }
    ],
    hostData,
    'WRITE'
  );
  assert.equal(mem.readU8(0x4000), 10);
  assert.equal(mem.readU8(0x4001), 20);
  assert.equal(mem.readU8(0x4100), 30);
  assert.equal(mem.readU8(0x4101), 40);

  // zero
  mem.zero(0x4000, 2);
  assert.equal(mem.readU16(0x4000), 0);
});

// ============================================================================
// SUITE 2: PciBus & VirtioPciDevice Configuration
// ============================================================================
console.log('\n--- Suite 2: PciBus & VirtioPciDevice ---');

test('PciBus: 0xCF8 / 0xCFC configuration mechanism 1', () => {
  const irqCtrl = new IrqController();
  const pciBus = new PciBus(irqCtrl);

  const dev = new VirtioPciDevice({
    name: 'test-blk',
    subsystemDeviceId: 2, // Block device
    pciClass: 0x010000,
    irqLine: 11
  });
  pciBus.registerDevice(3, dev); // Slot 3

  assert.equal(pciBus.getDevice(3), dev);

  // Configure address: Bus 0, Slot 3, Func 0, Reg 0, Enable bit 31 set
  const addr = (PCI_ADDR_ENABLE_MASK | (3 << 11) | 0x00) >>> 0;
  pciBus.writeConfigAddress(addr);
  assert.equal(pciBus.readConfigAddress(), addr);

  // Read Vendor ID at 0xCFC (size 2) -> 0x1AF4 (Red Hat / Virtio)
  const vendorId = pciBus.readConfigData(PCI_CONFIG_DATA_PORT, 2);
  assert.equal(vendorId, 0x1AF4);

  // Read Device ID at 0xCFE (size 2) -> 0x1040 + 2 = 0x1042
  const deviceId = pciBus.readConfigData(PCI_CONFIG_DATA_PORT + 2, 2);
  assert.equal(deviceId, 0x1042);
});

test('VirtioPciDevice: BAR sizing probes and active base routing', () => {
  const irqCtrl = new IrqController();
  const pciBus = new PciBus(irqCtrl);
  const dev = new VirtioPciDevice({
    name: 'test-net',
    subsystemDeviceId: 1,
    pciClass: 0x020000,
    irqLine: 10
  });
  pciBus.registerDevice(1, dev);

  // Select Slot 1
  pciBus.writeConfigAddress(PCI_ADDR_ENABLE_MASK | (1 << 11) | PCI_BAR0);

  // 1. Probe BAR0 (I/O space, 64 bytes)
  pciBus.writeConfigData(PCI_CONFIG_DATA_PORT, 0xFFFFFFFF, 4);
  const bar0SizeMask = pciBus.readConfigData(PCI_CONFIG_DATA_PORT, 4);
  // Inverted mask for 64B I/O: ~(64-1) | 1 = 0xFFFFFFC1
  assert.equal(bar0SizeMask >>> 0, 0xFFFFFFC1 >>> 0);

  // 2. Program BAR0 address 0xC000
  pciBus.writeConfigData(PCI_CONFIG_DATA_PORT, 0xC000, 4);

  // 3. Probe BAR1 (MMIO space, 4096 bytes)
  pciBus.writeConfigAddress(PCI_ADDR_ENABLE_MASK | (1 << 11) | PCI_BAR1);
  pciBus.writeConfigData(PCI_CONFIG_DATA_PORT, 0xFFFFFFFF, 4);
  const bar1SizeMask = pciBus.readConfigData(PCI_CONFIG_DATA_PORT, 4);
  // Inverted mask for 4096B MMIO: ~(4096-1) = 0xFFFFF000
  assert.equal(bar1SizeMask >>> 0, 0xFFFFF000 >>> 0);

  // 4. Program BAR1 address 0xFEB00000
  pciBus.writeConfigData(PCI_CONFIG_DATA_PORT, 0xFEB00000, 4);

  // 5. Test routing through bus readIo / readMmio
  // Legacy BAR0 offset 0x00 is DeviceFeatures low 32 bits
  const devFeatIo = pciBus.readIo(0xC000, 4);
  assert.notEqual(devFeatIo, 0);

  // Modern BAR1 offset 0x00 is deviceFeatureSelect (0)
  const commonCfg0 = pciBus.readMmio(0xFEB00000, 4);
  assert.equal(commonCfg0, 0);
});

test('VirtioPciDevice: Modern Capabilities linked-list discovery', () => {
  const dev = new VirtioPciDevice({
    name: 'test-gpu',
    subsystemDeviceId: 16,
    pciClass: 0x030000,
    irqLine: 11
  });

  // Cap pointer at 0x34
  const capPtr = dev.readPciConfig(PCI_CAPABILITY_LIST, 1);
  assert.equal(capPtr, 0x40);

  // Cap 1: COMMON_CFG at 0x40
  assert.equal(dev.readPciConfig(0x40, 1), 0x09); // PCI_CAP_ID_VNDR
  assert.equal(dev.readPciConfig(0x41, 1), 0x50); // next -> 0x50
  assert.equal(dev.readPciConfig(0x43, 1), 1);    // cfgType: COMMON_CFG
  assert.equal(dev.readPciConfig(0x44, 1), 1);    // bar: 1
  assert.equal(dev.readPciConfig(0x48, 4), MODERN_COMMON_CFG_OFFSET);

  // Cap 2: NOTIFY_CFG at 0x50
  assert.equal(dev.readPciConfig(0x50, 1), 0x09);
  assert.equal(dev.readPciConfig(0x51, 1), 0x64); // next -> 0x64
  assert.equal(dev.readPciConfig(0x53, 1), 2);    // cfgType: NOTIFY_CFG
  assert.equal(dev.readPciConfig(0x58, 4), MODERN_NOTIFY_CFG_OFFSET);
  assert.equal(dev.readPciConfig(0x60, 4), 4);    // notify_off_multiplier: 4

  // Cap 3: ISR_CFG at 0x64
  assert.equal(dev.readPciConfig(0x64, 1), 0x09);
  assert.equal(dev.readPciConfig(0x65, 1), 0x74); // next -> 0x74
  assert.equal(dev.readPciConfig(0x67, 1), 3);    // cfgType: ISR_CFG
  assert.equal(dev.readPciConfig(0x6C, 4), MODERN_ISR_CFG_OFFSET);

  // Cap 4: DEVICE_CFG at 0x74
  assert.equal(dev.readPciConfig(0x74, 1), 0x09);
  assert.equal(dev.readPciConfig(0x75, 1), 0x00); // end of list
  assert.equal(dev.readPciConfig(0x77, 1), 4);    // cfgType: DEVICE_CFG
  assert.equal(dev.readPciConfig(0x7C, 4), MODERN_DEVICE_CFG_OFFSET);
});

test('VirtioPciDevice: Feature handshake strictness (Vector G2-V10)', () => {
  const dev = new VirtioPciDevice({
    name: 'test-dev',
    subsystemDeviceId: 2,
    pciClass: 0x010000,
    irqLine: 10,
    deviceFeatures: 1n << 0n // Feature bit 0
  });

  // Verify device offers VIRTIO_F_VERSION_1 (bit 32)
  assert.notEqual(dev.deviceFeatures & VIRTIO_F_VERSION_1, 0n);

  // Case 1: Driver accepts only feature 0, omitting VIRTIO_F_VERSION_1
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x08, 0, 4); // driverFeatureSelect = 0
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x0C, 1, 4); // driverFeatures = 1
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x14, VIRTIO_STATUS_FEATURES_OK, 1);

  // Assert FEATURES_OK was cleared by device
  assert.equal(dev.deviceStatus & VIRTIO_STATUS_FEATURES_OK, 0);

  // Case 2: Driver offers unoffered feature bit 15
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x08, 0, 4);
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x0C, (1 << 15), 4);
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x08, 1, 4);
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x0C, 1, 4); // Bit 32 (VIRTIO_F_VERSION_1)
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x14, VIRTIO_STATUS_FEATURES_OK, 1);

  assert.equal(dev.deviceStatus & VIRTIO_STATUS_FEATURES_OK, 0);

  // Case 3: Driver offers valid features including VIRTIO_F_VERSION_1
  dev.driverFeatures = 0n;
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x08, 0, 4);
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x0C, 1, 4); // Bit 0
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x08, 1, 4);
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x0C, 1, 4); // Bit 32
  dev.writeModernBar1(MODERN_COMMON_CFG_OFFSET + 0x14, VIRTIO_STATUS_FEATURES_OK, 1);

  assert.notEqual(dev.deviceStatus & VIRTIO_STATUS_FEATURES_OK, 0);
});

// ============================================================================
// SUITE 3: Virtqueue Split Ring Engine & Fuzz Vectors
// ============================================================================
console.log('\n--- Suite 3: Virtqueue Split Ring Engine ---');

function setupTestQueue(size = 16) {
  const ramSize = 1024 * 1024;
  const guestMem = new GuestMem(ramSize);
  const vq = new Virtqueue(0, size);

  const descAddr = 0x1000n;
  const availAddr = 0x2000n;
  const usedAddr = 0x3000n;

  vq.setDescTableLow(Number(descAddr));
  vq.setAvailRingLow(Number(availAddr));
  vq.setUsedRingLow(Number(usedAddr));
  vq.setEnabled(true);

  return { guestMem, vq, descAddr, availAddr, usedAddr };
}

test('Virtqueue Vector G2-V01: Normal Request Completion', () => {
  const { guestMem, vq, descAddr, availAddr, usedAddr } = setupTestQueue(16);

  // Desc 0: Readable header (32 bytes at 0x10000)
  guestMem.writeU64(descAddr + 0n, 0x10000n);
  guestMem.writeU32(descAddr + 8n, 32);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_NEXT);
  guestMem.writeU16(descAddr + 14n, 1); // next -> Desc 1

  // Desc 1: Writable response (16 bytes at 0x10020)
  guestMem.writeU64(descAddr + 16n, 0x10020n);
  guestMem.writeU32(descAddr + 24n, 16);
  guestMem.writeU16(descAddr + 28n, VRING_DESC_F_WRITE);
  guestMem.writeU16(descAddr + 30n, 0);

  // Available Ring: head 0 at slot 0, avail.idx = 1
  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  assert.equal(vq.hasAvailable(guestMem), true);

  const chain = vq.popDescriptorChain(guestMem);
  assert.notEqual(chain, null);
  assert.equal(chain.headIndex, 0);
  assert.equal(chain.readable.length, 1);
  assert.equal(chain.readable[0].len, 32);
  assert.equal(chain.writable.length, 1);
  assert.equal(chain.writable[0].len, 16);

  // Commit used
  vq.pushUsed(chain.headIndex, 16, guestMem);

  // Assert used.idx is 1
  assert.equal(guestMem.readU16(usedAddr + 2n), 1);
  // Assert used.ring[0]: id=0, len=16
  assert.equal(guestMem.readU32(usedAddr + 4n), 0);
  assert.equal(guestMem.readU32(usedAddr + 8n), 16);

  assert.equal(vq.shouldNotify(guestMem), true);
});

test('Virtqueue Vector G2-V02: Indirect Descriptor Chain', () => {
  const { guestMem, vq, descAddr, availAddr, usedAddr } = setupTestQueue(16);

  // Outer Desc 0: Indirect table at 0x20000, length 32 (2 entries)
  const indirectTableAddr = 0x20000n;
  guestMem.writeU64(descAddr + 0n, indirectTableAddr);
  guestMem.writeU32(descAddr + 8n, 32);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_INDIRECT);
  guestMem.writeU16(descAddr + 14n, 0);

  // Indirect Desc 0: Read buffer (64 bytes)
  guestMem.writeU64(indirectTableAddr + 0n, 0x40000n);
  guestMem.writeU32(indirectTableAddr + 8n, 64);
  guestMem.writeU16(indirectTableAddr + 12n, VRING_DESC_F_NEXT);
  guestMem.writeU16(indirectTableAddr + 14n, 1);

  // Indirect Desc 1: Write buffer (128 bytes)
  guestMem.writeU64(indirectTableAddr + 16n, 0x40100n);
  guestMem.writeU32(indirectTableAddr + 24n, 128);
  guestMem.writeU16(indirectTableAddr + 28n, VRING_DESC_F_WRITE);
  guestMem.writeU16(indirectTableAddr + 30n, 0);

  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  const chain = vq.popDescriptorChain(guestMem);
  assert.equal(chain.headIndex, 0); // Head of outer descriptor
  assert.equal(chain.readable.length, 1);
  assert.equal(chain.readable[0].len, 64);
  assert.equal(chain.writable.length, 1);
  assert.equal(chain.writable[0].len, 128);

  vq.pushUsed(chain.headIndex, 128, guestMem);
  assert.equal(guestMem.readU32(usedAddr + 4n), 0); // Outer head committed
});

test('Virtqueue Vector G2-V03: Self-referencing cycle detection', () => {
  const { guestMem, vq, descAddr, availAddr } = setupTestQueue(16);

  // Desc 0 -> next = 0
  guestMem.writeU64(descAddr + 0n, 0x10000n);
  guestMem.writeU32(descAddr + 8n, 16);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_NEXT);
  guestMem.writeU16(descAddr + 14n, 0);

  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  assert.throws(() => vq.popDescriptorChain(guestMem), (err) => {
    return err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED';
  });
});

test('Virtqueue Vector G2-V04: Multi-hop cycle detection', () => {
  const { guestMem, vq, descAddr, availAddr } = setupTestQueue(16);

  // Desc 0 -> Desc 1 -> Desc 2 -> Desc 1
  guestMem.writeU64(descAddr + 0n, 0x10000n);
  guestMem.writeU32(descAddr + 8n, 16);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_NEXT);
  guestMem.writeU16(descAddr + 14n, 1);

  guestMem.writeU64(descAddr + 16n, 0x10020n);
  guestMem.writeU32(descAddr + 24n, 16);
  guestMem.writeU16(descAddr + 28n, VRING_DESC_F_NEXT);
  guestMem.writeU16(descAddr + 30n, 2);

  guestMem.writeU64(descAddr + 32n, 0x10040n);
  guestMem.writeU32(descAddr + 40n, 16);
  guestMem.writeU16(descAddr + 44n, VRING_DESC_F_NEXT);
  guestMem.writeU16(descAddr + 46n, 1); // Loops back to 1!

  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  assert.throws(() => vq.popDescriptorChain(guestMem), (err) => {
    return err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED';
  });
});

test('Virtqueue Vector G2-V05 & V06: DMA Out-of-RAM and 64-bit wrap check', () => {
  const { guestMem, vq, descAddr, availAddr } = setupTestQueue(16);

  // G2-V05: Address beyond ramSize
  guestMem.writeU64(descAddr + 0n, BigInt(guestMem.ramSize + 0x1000));
  guestMem.writeU32(descAddr + 8n, 64);
  guestMem.writeU16(descAddr + 12n, 0);
  guestMem.writeU16(descAddr + 14n, 0);

  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  assert.throws(() => vq.popDescriptorChain(guestMem), DmaOutOfBoundsError);

  // G2-V06: 64-bit overflow wrap
  guestMem.writeU64(descAddr + 0n, 0xFFFFFFFFFFFFFFFFn);
  guestMem.writeU32(descAddr + 8n, 128);
  assert.throws(() => vq.popDescriptorChain(guestMem), DmaOutOfBoundsError);
});

test('Virtqueue Vector G2-V07: Nested Indirect Table Rejection', () => {
  const { guestMem, vq, descAddr, availAddr } = setupTestQueue(16);

  const indirectTableAddr = 0x20000n;
  // Outer descriptor
  guestMem.writeU64(descAddr + 0n, indirectTableAddr);
  guestMem.writeU32(descAddr + 8n, 16);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_INDIRECT);
  guestMem.writeU16(descAddr + 14n, 0);

  // Inner descriptor sets VRING_DESC_F_INDIRECT (forbidden!)
  guestMem.writeU64(indirectTableAddr + 0n, 0x30000n);
  guestMem.writeU32(indirectTableAddr + 8n, 16);
  guestMem.writeU16(indirectTableAddr + 12n, VRING_DESC_F_INDIRECT);
  guestMem.writeU16(indirectTableAddr + 14n, 0);

  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  assert.throws(() => vq.popDescriptorChain(guestMem), (err) => {
    return err instanceof VirtqueueError && err.code === 'NESTED_INDIRECT_TABLE_REJECTED';
  });
});

test('Virtqueue Vector G2-V08 & G2-V09: Unaligned indirect length & read-after-write', () => {
  const { guestMem, vq, descAddr, availAddr } = setupTestQueue(16);

  // G2-V08: len = 25 (not multiple of 16)
  guestMem.writeU64(descAddr + 0n, 0x20000n);
  guestMem.writeU32(descAddr + 8n, 25);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_INDIRECT);
  guestMem.writeU16(descAddr + 14n, 0);

  guestMem.writeU16(availAddr + 4n, 0);
  guestMem.writeU16(availAddr + 2n, 1);

  assert.throws(() => vq.popDescriptorChain(guestMem), (err) => {
    return err instanceof VirtqueueError && err.code === 'MALFORMED_INDIRECT_DESCRIPTOR';
  });

  // G2-V09: Read buffer following write buffer
  // Desc 0: Writeable
  guestMem.writeU64(descAddr + 0n, 0x10000n);
  guestMem.writeU32(descAddr + 8n, 16);
  guestMem.writeU16(descAddr + 12n, VRING_DESC_F_WRITE | VRING_DESC_F_NEXT);
  guestMem.writeU16(descAddr + 14n, 1);

  // Desc 1: Readable (illegal after writeable)
  guestMem.writeU64(descAddr + 16n, 0x10020n);
  guestMem.writeU32(descAddr + 24n, 16);
  guestMem.writeU16(descAddr + 28n, 0);
  guestMem.writeU16(descAddr + 30n, 0);

  assert.throws(() => vq.popDescriptorChain(guestMem), (err) => {
    return err instanceof VirtqueueError && err.code === 'MALFORMED_DESCRIPTOR_CHAIN';
  });
});

// ============================================================================
// SUITE 4: IrqController Latching & INTx Line Sharing
// ============================================================================
console.log('\n--- Suite 4: IrqController ---');

test('IrqController: Latching, read-to-clear, and deassertion', () => {
  const irqEvents = [];
  const irqCtrl = new IrqController((irq, level) => {
    irqEvents.push({ irq, level });
  });

  const dev = new VirtioPciDevice({ name: 'dev-1', irqLine: 11 });

  // 1. Assert queue interrupt
  irqCtrl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
  assert.equal(irqEvents.length, 1);
  assert.deepEqual(irqEvents[0], { irq: 11, level: true });

  // 2. Accumulate configuration interrupt before read
  irqCtrl.assertDeviceIrq(dev, VIRTIO_ISR_CONFIG);
  assert.equal(irqEvents.length, 1); // No new line edge (already HIGH)

  // 3. Destructive read-to-clear
  const isrVal = irqCtrl.readIsrStatus(dev);
  assert.equal(isrVal, 0x01 | 0x02); // 0x03
  assert.equal(irqEvents.length, 2);
  assert.deepEqual(irqEvents[1], { irq: 11, level: false });

  // 4. Second read returns 0, no edge triggered
  assert.equal(irqCtrl.readIsrStatus(dev), 0);
  assert.equal(irqEvents.length, 2);
});

test('IrqController: Shared INTx line reference counting', () => {
  const irqEvents = [];
  const irqCtrl = new IrqController((irq, level) => {
    irqEvents.push({ irq, level });
  });

  const devA = new VirtioPciDevice({ name: 'dev-a', irqLine: 11 });
  const devB = new VirtioPciDevice({ name: 'dev-b', irqLine: 11 });

  // Assert devA -> line HIGH
  irqCtrl.assertDeviceIrq(devA, VIRTIO_ISR_QUEUE);
  assert.equal(irqEvents.length, 1);
  assert.deepEqual(irqEvents[0], { irq: 11, level: true });

  // Assert devB -> line stays HIGH (no duplicate event)
  irqCtrl.assertDeviceIrq(devB, VIRTIO_ISR_QUEUE);
  assert.equal(irqEvents.length, 1);

  // Clear devA -> devB still active, line stays HIGH!
  assert.equal(irqCtrl.readIsrStatus(devA), 0x01);
  assert.equal(irqEvents.length, 1);

  // Clear devB -> line goes LOW!
  assert.equal(irqCtrl.readIsrStatus(devB), 0x01);
  assert.equal(irqEvents.length, 2);
  assert.deepEqual(irqEvents[1], { irq: 11, level: false });
});

test('IrqController: Masking via PCI Command INTX_DIS', () => {
  const irqEvents = [];
  const irqCtrl = new IrqController((irq, level) => {
    irqEvents.push({ irq, level });
  });

  const dev = new VirtioPciDevice({ name: 'dev-masked', irqLine: 10 });

  // Mask interrupts via PCI Command
  dev.writePciConfig(PCI_COMMAND, PCI_COMMAND_INTX_DIS, 2);
  irqCtrl.assertDeviceIrq(dev, VIRTIO_ISR_QUEUE);
  // Callback must NOT have been called
  assert.equal(irqEvents.length, 0);

  // Unmask interrupts
  dev.writePciConfig(PCI_COMMAND, 0, 2);
  irqCtrl.setCommandRegister(dev, 0);
  // Now line goes HIGH
  assert.equal(irqEvents.length, 1);
  assert.deepEqual(irqEvents[0], { irq: 10, level: true });

  irqCtrl.readIsrStatus(dev);
  assert.equal(irqEvents.length, 2);
  assert.deepEqual(irqEvents[1], { irq: 10, level: false });
});

// ============================================================================
// SUITE 5: BootManager Linux Direct Boot Protocol
// ============================================================================
console.log('\n--- Suite 5: BootManager ---');

function createMockKernel() {
  const size = 64 * 1024;
  const k = new Uint8Array(size);
  const view = new DataView(k.buffer);

  // Setup header magic "HdrS" at 0x0202
  view.setUint32(0x0202, LINUX_HDRS_MAGIC, true);
  // Version 2.15
  view.setUint16(0x0206, 0x020F, true);
  // setup_sects: 4
  view.setUint8(0x01F1, 4);
  // code32_start: 0x00100000
  view.setUint32(0x0214, CODE32_START_ADDR, true);
  // initrd_addr_max: 0x37FFFFFF
  view.setUint32(0x022C, 0x37FFFFFF, true);

  return k;
}

test('BootManager: Setup header parsing and corrupt rejection', () => {
  const mem = new GuestMem(16 * 1024 * 1024);
  const bm = new BootManager(mem);

  const k = createMockKernel();
  const hdr = bm.parseSetupHeader(k);
  assert.equal(hdr.headerMagic, LINUX_HDRS_MAGIC);
  assert.equal(hdr.setupSize, 5 * 512);
  assert.equal(hdr.code32Start, CODE32_START_ADDR);

  // Corrupt magic
  k[0x0202] = 0xFF;
  assert.throws(() => bm.parseSetupHeader(k), InvalidKernelHeaderError);
});

test('BootManager: Full Linux boot configuration and E820 map', () => {
  const ramSize = 32 * 1024 * 1024;
  const mem = new GuestMem(ramSize);
  const bm = new BootManager(mem);

  const k = createMockKernel();
  const initrd = new Uint8Array([0x1F, 0x8B, 0x08, 0x00]); // gzip mock
  const cmdline = 'console=ttyS0 quiet androidboot.hardware=android_x86';

  const res = bm.setupLinuxBoot({ kernel: k, initrd, cmdline });

  assert.equal(res.entryIp, CODE32_START_ADDR);
  assert.equal(res.bootParamsAddr, BOOT_PARAMS_ADDR);
  assert.equal(res.registers.cr0, 1);
  assert.equal(res.registers.cs, 0x0010);
  assert.equal(res.registers.eip, CODE32_START_ADDR);
  assert.equal(res.registers.esi, BOOT_PARAMS_ADDR);

  // Verify cmdline in guest memory at 0x00020000
  const cmdRead = new TextDecoder().decode(mem.readBytes(CMDLINE_ADDR, cmdline.length));
  assert.equal(cmdRead, cmdline);

  // Verify E820 map: 3 entries
  assert.equal(mem.readU8(BOOT_PARAMS_ADDR + 0x01E8), 3);
  // Entry 0: size 0x9F000
  assert.equal(mem.readU64(BOOT_PARAMS_ADDR + 0x02D0 + 8), 0x0009F000n);
  // Entry 2: size = ramSize - 1MB
  assert.equal(mem.readU64(BOOT_PARAMS_ADDR + 0x02D0 + 40 + 8), BigInt(ramSize - 0x100000));
});

// ============================================================================
// SUITE 6: VmRuntime Lifecycle & Watchdog
// ============================================================================
console.log('\n--- Suite 6: VmRuntime Lifecycle & Watchdog ---');

await testAsync('VmRuntime: Complete lifecycle transitions', async () => {
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 50, watchdogTimeoutMs: 500 });
  const stateTransitions = [];
  runtime.onStateChange((tr) => stateTransitions.push(tr.newState));

  // 1. Create
  await runtime.create({ ramSize: 8 * 1024 * 1024 });
  assert.equal(runtime.state, 'READY');

  // 2. Load boot
  const k = createMockKernel();
  await runtime.loadBoot({ kernel: k });
  assert.equal(runtime.state, 'CONFIGURED');

  // 3. Start
  await runtime.start();
  assert.equal(runtime.state, 'RUNNING');

  // 4. Second start rejected
  await assert.rejects(async () => await runtime.start(), VmStateError);

  // 5. Pause & Resume
  await runtime.pause();
  assert.equal(runtime.state, 'PAUSED');

  await runtime.resume();
  assert.equal(runtime.state, 'RUNNING');

  // 6. Reset
  await runtime.reset();
  assert.equal(runtime.state, 'READY');

  // 7. Destroy
  await runtime.destroy();
  assert.equal(runtime.state, 'DESTROYED');

  assert.ok(stateTransitions.includes('READY'));
  assert.ok(stateTransitions.includes('CONFIGURED'));
  assert.ok(stateTransitions.includes('RUNNING'));
  assert.ok(stateTransitions.includes('PAUSED'));
  assert.ok(stateTransitions.includes('DESTROYED'));
});

await testAsync('VmRuntime: Watchdog timeout trigger and worker termination', async () => {
  // Set short timeout
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 20, watchdogTimeoutMs: 60 });
  let errorCaught = null;
  runtime.onError((err) => {
    errorCaught = err;
  });

  await runtime.create({ ramSize: 4 * 1024 * 1024 });
  const k = createMockKernel();
  await runtime.loadBoot({ kernel: k });

  // Start with running worker
  await runtime.start();

  // Simulate unresponsive worker by clearing handleWorkerMessage
  runtime.worker.postMessage = () => {}; // Drop all pings

  await new Promise((resolve) => setTimeout(resolve, 150));

  assert.equal(runtime.state, 'ERROR');
  assert.notEqual(errorCaught, null);
  assert.equal(errorCaught.code, 'WATCHDOG_TIMEOUT');

  await runtime.destroy();
});

console.log(`\nAll ${passedTests} / ${totalTests} tests passed cleanly!\n`);
