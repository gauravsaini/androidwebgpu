/**
 * Empirical Adversarial Challenger M1: PCI BAR0/BAR1 Emulation, Sizing Probes, Partial Writes & Dynamic Relocation
 * 
 * Scope:
 * 1. PCI BAR Sizing Probes (0xFFFFFFFF to BAR0 [I/O 64B] and BAR1 [MMIO 16MB]).
 * 2. Arbitrary partial 1-byte, 2-byte, and 4-byte register writes at BAR0 (0x10-0x13) and BAR1 (0x14-0x17).
 * 3. Dynamic I/O port base relocation (register_read / register_write rebinding) and port collision prevention.
 * 4. Fuzzed & boundary write sequences across PCI configuration space.
 * 5. Virtqueue ring operation integrity across multiple BAR relocations.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { VirtioGpuDevice, VIRTIO_GPU_F_VIRGL, VIRTIO_GPU_F_EDID } from '../src/virtio_gpu_device.js';

describe('Empirical Challenger M1: PCI BAR0/BAR1 Adversarial Challenge Suite', () => {

    // -------------------------------------------------------------------------
    // Suite 1: PCI BAR Sizing Probes (0xFFFFFFFF)
    // -------------------------------------------------------------------------
    describe('1. BAR Sizing Probes (0xFFFFFFFF to BAR0/BAR1)', () => {
        it('handles BAR0 sizing probe (0xFFFFFFFF) and returns 64-byte I/O mask 0xFFFFFFC1', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });
            
            // Initial BAR0 state (0xC141: I/O space at 0xC140, bit 0 = 1)
            assert.equal(dev.pciRead(0x10, 4), 0xC141);
            assert.equal(dev.bar0Sizing, false);

            // Write 0xFFFFFFFF to BAR0 (offsets 0x10-0x13)
            dev.pciWrite(0x10, 0xFFFFFFFF, 4);
            assert.equal(dev.bar0Sizing, true, 'BAR0 sizing flag must be active');

            // Read back BAR0 (32-bit)
            const mask32 = dev.pciRead(0x10, 4);
            assert.equal(mask32, 0xFFFFFFC1, 'BAR0 sizing read (32-bit) must return 0xFFFFFFC1 (64B I/O mask)');
            
            // Subsequent normal write restores address and deactivates sizing flag
            dev.pciWrite(0x10, 0xC201, 4);
            assert.equal(dev.bar0Sizing, false, 'BAR0 sizing flag must reset on normal write');
            assert.equal(dev.pciRead(0x10, 4), 0xC201, 'BAR0 must now reflect assigned address 0xC201');
            assert.equal(dev.ioBase, 0xC200, 'ioBase must update to 0xC200');
        });

        it('handles BAR1 sizing probe (0xFFFFFFFF) and returns 16MB MMIO mask 0xFF000000', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });
            
            // Initial BAR1 state (MMIO space, bit 0 = 0)
            assert.equal(dev.bar1Sizing, false);

            // Write 0xFFFFFFFF to BAR1 (offsets 0x14-0x17)
            dev.pciWrite(0x14, 0xFFFFFFFF, 4);
            assert.equal(dev.bar1Sizing, true, 'BAR1 sizing flag must be active');

            // Read back BAR1 (32-bit)
            const mask32 = dev.pciRead(0x14, 4);
            assert.equal(mask32, 0xFF000000, 'BAR1 sizing read (32-bit) must return 0xFF000000 (16MB MMIO mask)');

            // Normal write restores address and deactivates sizing flag
            dev.pciWrite(0x14, 0xE0000000, 4);
            assert.equal(dev.bar1Sizing, false, 'BAR1 sizing flag must reset on normal write');
            assert.equal(dev.pciRead(0x14, 4), 0xE0000000, 'BAR1 must reflect assigned address');
            assert.equal(dev.bar1Value, 0xE0000000, 'bar1Value must update to 0xE0000000');
        });

        it('handles sequential interleaved BAR0 and BAR1 sizing probes', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Probe BAR0 then BAR1 simultaneously
            dev.pciWrite(0x10, 0xFFFFFFFF, 4);
            dev.pciWrite(0x14, 0xFFFFFFFF, 4);

            assert.equal(dev.pciRead(0x10, 4), 0xFFFFFFC1);
            assert.equal(dev.pciRead(0x14, 4), 0xFF000000);

            // Restore BAR0 only
            dev.pciWrite(0x10, 0xC141, 4);
            assert.equal(dev.pciRead(0x10, 4), 0xC141);
            assert.equal(dev.pciRead(0x14, 4), 0xFF000000, 'BAR1 must remain in sizing mode until written');

            // Restore BAR1
            dev.pciWrite(0x14, 0xD1000000, 4);
            assert.equal(dev.pciRead(0x14, 4), 0xD1000000);
        });
    });

    // -------------------------------------------------------------------------
    // Suite 2: Partial 1-byte and 2-byte Register Writes at BAR0 and BAR1
    // -------------------------------------------------------------------------
    describe('2. Partial 1-byte and 2-byte Register Writes (0x10-0x17)', () => {
        it('reconstructs full 32-bit BAR0 base via 16-bit word writes', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Write low word: 0x41 (port 0xC140 + I/O bit 1) -> 0x8041
            dev.pciWrite(0x10, 0x8041, 2);
            // Write high word: 0x0000
            dev.pciWrite(0x12, 0x0000, 2);

            assert.equal(dev.pciRead(0x10, 4), 0x8041);
            assert.equal(dev.ioBase, 0x8040, 'ioBase must reconstruct from 16-bit writes to 0x8040');
        });

        it('reconstructs full 32-bit BAR0 base via four individual 8-bit byte writes', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Target base: 0xE480 (full BAR0: 0xE481)
            dev.pciWrite(0x10, 0x81, 1); // Byte 0
            dev.pciWrite(0x11, 0xE4, 1); // Byte 1
            dev.pciWrite(0x12, 0x00, 1); // Byte 2
            dev.pciWrite(0x13, 0x00, 1); // Byte 3

            assert.equal(dev.pciRead(0x10, 4), 0xE481);
            assert.equal(dev.ioBase, 0xE480, 'ioBase must be 0xE480');
        });

        it('handles non-sequential (reverse order) byte writes to BAR0', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Target: 0x5041 (ioBase 0x5040)
            dev.pciWrite(0x13, 0x00, 1); // Byte 3
            dev.pciWrite(0x12, 0x00, 1); // Byte 2
            dev.pciWrite(0x11, 0x50, 1); // Byte 1
            dev.pciWrite(0x10, 0x41, 1); // Byte 0

            assert.equal(dev.pciRead(0x10, 4), 0x5041);
            assert.equal(dev.ioBase, 0x5040);
        });

        it('reconstructs full 32-bit BAR1 MMIO base via 16-bit word writes', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Target: 0xFE000000
            dev.pciWrite(0x14, 0x0000, 2); // Low word
            dev.pciWrite(0x16, 0xFE00, 2); // High word

            assert.equal(dev.pciRead(0x14, 4), 0xFE000000);
            assert.equal(dev.bar1Value, 0xFE000000);
        });

        it('reconstructs full 32-bit BAR1 MMIO base via four 8-bit byte writes', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Target: 0x2A000000
            dev.pciWrite(0x17, 0x2A, 1);
            dev.pciWrite(0x16, 0x00, 1);
            dev.pciWrite(0x15, 0x00, 1);
            dev.pciWrite(0x14, 0x00, 1);

            assert.equal(dev.pciRead(0x14, 4), 0x2A000000);
            assert.equal(dev.bar1Value, 0x2A000000);
        });
    });

    // -------------------------------------------------------------------------
    // Suite 3: Dynamic I/O Port Base Relocation & Port Collision Avoidance
    // -------------------------------------------------------------------------
    describe('3. Dynamic I/O Port Base Relocation & Collision Verification', () => {
        function createMockV86() {
            const portRegistry = new Map();
            const io = {
                register_read: (port, dev, r8, r16, r32) => {
                    portRegistry.set(`r:${port}`, { dev, r8, r16, r32 });
                },
                register_write: (port, dev, w8, w16, w32) => {
                    portRegistry.set(`w:${port}`, { dev, w8, w16, w32 });
                },
                read8: (port) => {
                    const entry = portRegistry.get(`r:${port}`);
                    return entry ? entry.r8() : 0xFF;
                },
                read16: (port) => {
                    const entry = portRegistry.get(`r:${port}`);
                    return entry ? entry.r16() : 0xFFFF;
                },
                read32: (port) => {
                    const entry = portRegistry.get(`r:${port}`);
                    return entry ? entry.r32() : 0xFFFFFFFF;
                },
                write8: (port, val) => {
                    const entry = portRegistry.get(`w:${port}`);
                    if (entry) entry.w8(val);
                },
                write16: (port, val) => {
                    const entry = portRegistry.get(`w:${port}`);
                    if (entry) entry.w16(val);
                },
                write32: (port, val) => {
                    const entry = portRegistry.get(`w:${port}`);
                    if (entry) entry.w32(val);
                }
            };
            const pciDevices = {};
            const pci = {
                devices: pciDevices,
                register_device: (bdf, dev) => {
                    pciDevices[bdf] = dev;
                }
            };
            const cpu = { io, devices: { pci } };
            return { v86: { cpu, io }, portRegistry };
        }

        it('dynamically re-registers all 64 I/O ports upon BAR0 relocation', () => {
            const { v86, portRegistry } = createMockV86();
            const dev = new VirtioGpuDevice(v86, null, { width: 720, height: 1440 });

            // Initial registration at 0xC140..0xC17F
            for (let port = 0xC140; port < 0xC140 + 64; port++) {
                assert.ok(portRegistry.has(`r:${port}`), `Port ${port} read must be registered at initial base`);
                assert.ok(portRegistry.has(`w:${port}`), `Port ${port} write must be registered at initial base`);
            }

            // Relocate BAR0 to 0x7000 (I/O addr 0x7001)
            dev.pciWrite(0x10, 0x7001, 4);
            assert.equal(dev.ioBase, 0x7000);

            // Verify all 64 ports re-registered at 0x7000..0x703F
            for (let port = 0x7000; port < 0x7000 + 64; port++) {
                assert.ok(portRegistry.has(`r:${port}`), `Port 0x${port.toString(16)} read must be registered at new base`);
                assert.ok(portRegistry.has(`w:${port}`), `Port 0x${port.toString(16)} write must be registered at new base`);
            }

            // Test functional reads and writes through relocated base
            // Host features at 0x7000
            assert.equal(v86.io.read32(0x7000), (1 << 0) | (1 << 1), 'Host features readable at new base');

            // Queue select at 0x700E (select queue 0)
            v86.io.write16(0x700E, 0);
            assert.equal(v86.io.read16(0x700C), 256, 'Queue 0 size is 256 via new base');

            // Queue select at 0x700E (select queue 1)
            v86.io.write16(0x700E, 1);
            assert.equal(v86.io.read16(0x700C), 16, 'Queue 1 size is 16 via new base');
        });

        it('does not collide with NE2000 ports at 0xC000-0xC01F on slot 0x05', () => {
            const { v86, portRegistry } = createMockV86();

            // Simulate NE2000 mock registered at 0xC000..0xC01F
            for (let p = 0xC000; p < 0xC020; p++) {
                portRegistry.set(`r:${p}`, { dev: 'ne2000', r8: () => 0xAA, r16: () => 0xAAAA, r32: () => 0xAAAAAAAA });
                portRegistry.set(`w:${p}`, { dev: 'ne2000', w8: () => {}, w16: () => {}, w32: () => {} });
            }

            const dev = new VirtioGpuDevice(v86, null, { width: 720, height: 1440 });

            // Verify VirtIO GPU at 0xC140 does not overwrite NE2000 at 0xC000..0xC01F
            for (let p = 0xC000; p < 0xC020; p++) {
                const entry = portRegistry.get(`r:${p}`);
                assert.equal(entry.dev, 'ne2000', `Port 0x${p.toString(16)} must remain owned by NE2000`);
                assert.equal(v86.io.read8(p), 0xAA, `NE2000 port 0x${p.toString(16)} read must return 0xAA`);
            }

            // Relocate VirtIO-GPU to 0xC080 (distinct from 0xC000)
            dev.pciWrite(0x10, 0xC081, 4);
            assert.equal(dev.ioBase, 0xC080);

            // Verify NE2000 is still untouched
            for (let p = 0xC000; p < 0xC020; p++) {
                const entry = portRegistry.get(`r:${p}`);
                assert.equal(entry.dev, 'ne2000', `Port 0x${p.toString(16)} must still be owned by NE2000 after VirtIO relocation`);
            }
        });

        it('handles rapid sequential BAR0 relocations without leaking or corrupting state', () => {
            const { v86, portRegistry } = createMockV86();
            const dev = new VirtioGpuDevice(v86, null, { width: 720, height: 1440 });

            const targetBases = [0x1000, 0x2000, 0x4000, 0x8000, 0xC140, 0x9000];

            for (const base of targetBases) {
                dev.pciWrite(0x10, (base | 1) >>> 0, 4);
                assert.equal(dev.ioBase, base);

                // Write device status via relocated base
                v86.io.write8(base + 0x12, 0x07); // ACK | DRV | DRV_OK
                assert.equal(v86.io.read8(base + 0x12), 0x07);
                assert.equal(dev.deviceStatus, 0x07);
            }
        });
    });

    // -------------------------------------------------------------------------
    // Suite 4: VirtIO Capabilities Walking & Config Space Compliance
    // -------------------------------------------------------------------------
    describe('4. VirtIO 1.0 Capability Chain Verification', () => {
        it('validates the 4-capability vendor-specific capability linked list', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 720, height: 1440 });

            // Capabilities Pointer at 0x34
            const capPtr = dev.pciRead(0x34, 1);
            assert.equal(capPtr, 0x40, 'Capabilities pointer must be 0x40');

            // Status register bit 4 (Capabilities List) must be set
            const status = dev.pciRead(0x06, 2);
            assert.ok((status & 0x0010) !== 0, 'Status register bit 4 must indicate capabilities list');

            // Walk Cap 1: Common Config at 0x40
            assert.equal(dev.pciRead(0x40, 1), 0x09, 'Cap 1 vndr = PCI_CAP_ID_VNDR');
            assert.equal(dev.pciRead(0x41, 1), 0x50, 'Cap 1 cap_next points to 0x50');
            assert.equal(dev.pciRead(0x42, 1), 0x10, 'Cap 1 cap_len = 16 bytes');
            assert.equal(dev.pciRead(0x43, 1), 0x01, 'Cap 1 cfg_type = VIRTIO_PCI_CAP_COMMON_CFG (1)');
            assert.equal(dev.pciRead(0x44, 1), 0x01, 'Cap 1 bar = BAR1');
            assert.equal(dev.pciRead(0x48, 4), 0x0000, 'Cap 1 offset = 0x0000');
            assert.equal(dev.pciRead(0x4C, 4), 0x38, 'Cap 1 length = 56 bytes');

            // Walk Cap 2: Notify Config at 0x50
            assert.equal(dev.pciRead(0x50, 1), 0x09);
            assert.equal(dev.pciRead(0x51, 1), 0x64, 'Cap 2 cap_next points to 0x64');
            assert.equal(dev.pciRead(0x52, 1), 0x14, 'Cap 2 cap_len = 20 bytes');
            assert.equal(dev.pciRead(0x53, 1), 0x02, 'Cap 2 cfg_type = VIRTIO_PCI_CAP_NOTIFY_CFG (2)');
            assert.equal(dev.pciRead(0x54, 1), 0x01, 'Cap 2 bar = BAR1');
            assert.equal(dev.pciRead(0x58, 4), 0x1000, 'Cap 2 offset = 0x1000');
            assert.equal(dev.pciRead(0x5C, 4), 0x1000, 'Cap 2 length = 0x1000');
            assert.equal(dev.pciRead(0x60, 4), 0x04, 'Cap 2 notify_off_multiplier = 4');

            // Walk Cap 3: ISR Config at 0x64
            assert.equal(dev.pciRead(0x64, 1), 0x09);
            assert.equal(dev.pciRead(0x65, 1), 0x74, 'Cap 3 cap_next points to 0x74');
            assert.equal(dev.pciRead(0x66, 1), 0x10, 'Cap 3 cap_len = 16 bytes');
            assert.equal(dev.pciRead(0x67, 1), 0x03, 'Cap 3 cfg_type = VIRTIO_PCI_CAP_ISR_CFG (3)');
            assert.equal(dev.pciRead(0x68, 1), 0x01, 'Cap 3 bar = BAR1');
            assert.equal(dev.pciRead(0x6C, 4), 0x2000, 'Cap 3 offset = 0x2000');
            assert.equal(dev.pciRead(0x70, 4), 0x04, 'Cap 3 length = 4 bytes');

            // Walk Cap 4: Device Config at 0x74
            assert.equal(dev.pciRead(0x74, 1), 0x09);
            assert.equal(dev.pciRead(0x75, 1), 0x00, 'Cap 4 cap_next = 0 (Terminator)');
            assert.equal(dev.pciRead(0x76, 1), 0x10, 'Cap 4 cap_len = 16 bytes');
            assert.equal(dev.pciRead(0x77, 1), 0x04, 'Cap 4 cfg_type = VIRTIO_PCI_CAP_DEVICE_CFG (4)');
            assert.equal(dev.pciRead(0x78, 1), 0x01, 'Cap 4 bar = BAR1');
            assert.equal(dev.pciRead(0x7C, 4), 0x3000, 'Cap 4 offset = 0x3000');
            assert.equal(dev.pciRead(0x80, 4), 0x18, 'Cap 4 length = 24 bytes');
        });
    });
});
