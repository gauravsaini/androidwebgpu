/**
 * Empirical Challenger M3.1: Adversarial Stress Test Suite
 * VirtIO GPU Bridge, Command Opcodes, Scanouts, and Virtqueue Ring Transitions
 * 
 * Conforms to ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { VirtioGpuDevice, VIRTIO_GPU_F_VIRGL, VIRTIO_GPU_F_EDID } from '../src/virtio_gpu_device.js';
import { calculateShannonEntropy } from '../validate_browser.mjs';

describe('Challenger M3.1: VirtIO GPU Bridge & Virtqueue Adversarial Stress Test', () => {

    // -------------------------------------------------------------------------
    // 1. PCI Space & I/O Port Boundary Stress
    // -------------------------------------------------------------------------
    describe('1. PCI Space & Legacy I/O Boundaries', () => {
        it('verifies PCI configuration space layout and read widths', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });
            
            // 8-bit, 16-bit, and 32-bit reads on PCI header
            assert.equal(dev.pciRead(0, 2), 0x1AF4, 'Vendor ID must be 0x1AF4 (Red Hat / QEMU VirtIO)');
            assert.equal(dev.pciRead(2, 2), 0x1050, 'Device ID must be 0x1050 (VirtIO GPU)');
            assert.equal(dev.pciRead(10, 2), 0x0300, 'Class code must be Display Controller (0x0300)');
            assert.equal(dev.pciRead(60, 1), 10, 'IRQ Line must be 10');
            
            // Out of bounds PCI space access returns 0 without crashing
            assert.equal(dev.pciRead(250, 4), 0, 'High PCI read returns 0');
            dev.pciWrite(250, 0xFF, 1);
            assert.equal(dev.pciRead(250, 1), 0xFF, 'PCI write/read roundtrip');
        });

        it('stress-tests legacy I/O space multi-byte registers and queue switches', () => {
            const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });

            // 1. Host features (VIRGL | EDID)
            assert.equal(dev.ioRead(0x00, 4), 0x03);
            assert.equal(dev.ioRead(0x00, 2), 0x03);
            assert.equal(dev.ioRead(0x00, 1), 0x03);
            assert.equal(dev.ioRead(0x01, 1), 0x00);

            // 2. Queue selection & size validation
            dev.ioWrite(0x0E, 0, 2); // Select Queue 0
            assert.equal(dev.ioRead(0x0E, 2), 0);
            assert.equal(dev.ioRead(0x0C, 2), 256, 'Queue 0 size must be 256');

            dev.ioWrite(0x0E, 1, 2); // Select Queue 1
            assert.equal(dev.ioRead(0x0E, 2), 1);
            assert.equal(dev.ioRead(0x0C, 2), 16, 'Queue 1 size must be 16');

            // Select invalid queue index -> defaults safely
            dev.ioWrite(0x0E, 5, 2);
            assert.equal(dev.ioRead(0x0E, 2), 1); // 5 & 1 == 1

            // 3. PFN writes and 32-bit readbacks
            dev.ioWrite(0x0E, 0, 2);
            dev.ioWrite(0x08, 0x12345678, 4);
            assert.equal(dev.ioRead(0x08, 4), 0x12345678);
            assert.equal(dev.ioRead(0x08, 2), 0x5678);

            // 4. Device reset cycle (status = 0)
            dev.queues[0].lastAvailIdx = 42;
            dev.queues[0].lastUsedIdx = 42;
            dev.isrStatus = 0x01;
            dev.ioWrite(0x12, 0, 1); // Reset
            assert.equal(dev.ioRead(0x12, 1), 0);
            assert.equal(dev.queues[0].lastAvailIdx, 0, 'Avail index reset to 0');
            assert.equal(dev.queues[0].lastUsedIdx, 0, 'Used index reset to 0');
            assert.equal(dev.isrStatus, 0, 'ISR reset to 0');
        });
    });

    // -------------------------------------------------------------------------
    // 2. Virtqueue Ring Transitions, Descriptor Chaining & Wrap-Arounds
    // -------------------------------------------------------------------------
    describe('2. Virtqueue Ring Transitions & Ring Wrap-Arounds', () => {
        it('handles 16-bit descriptor ring index wrap-around past 65535 cleanly', () => {
            const guestMem = new Uint8Array(2 * 1024 * 1024);
            const view = new DataView(guestMem.buffer);

            let lastCommand = null;
            const mockBridge = {
                process_virtqueue_descriptor: (mem, table, head) => {
                    const descOffset = table + head * 16;
                    const addr = view.getUint32(descOffset, true);
                    const len = view.getUint32(descOffset + 8, true);
                    lastCommand = mem.slice(addr, addr + len);

                    const outDescOffset = table + 1 * 16;
                    const outAddr = view.getUint32(outDescOffset, true);
                    // Write VIRTIO_GPU_RESP_OK_NODATA (24 bytes)
                    view.setUint32(outAddr, 0x1100, true);
                    return 24;
                }
            };

            const dev = new VirtioGpuDevice(null, mockBridge, { width: 1280, height: 720 });
            dev.getGuestMemory = () => guestMem;

            // Configure Queue 0 at PFN 0x10 (0x10000)
            dev.ioWrite(0x0E, 0, 2);
            dev.ioWrite(0x08, 0x10, 4);

            const descTableAddr = 0x10000;
            const availRingAddr = descTableAddr + 256 * 16;
            const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * 256) / 4096) * 4096;

            // Setup descriptor 0 (In: command) and 1 (Out: response)
            view.setUint32(descTableAddr + 0, 0x40000, true);
            view.setUint32(descTableAddr + 8, 24, true);
            view.setUint16(descTableAddr + 12, 0x01, true); // NEXT
            view.setUint16(descTableAddr + 14, 1, true);

            view.setUint32(descTableAddr + 16, 0x50000, true);
            view.setUint32(descTableAddr + 24, 24, true);
            view.setUint16(descTableAddr + 28, 0x02, true); // WRITE
            view.setUint16(descTableAddr + 30, 0, true);

            // Put command at 0x40000
            view.setUint32(0x40000, 0x0100, true); // GET_DISPLAY_INFO

            // Start queue at index 65535 (0xFFFF)
            dev.queues[0].lastAvailIdx = 0xFFFF;
            dev.queues[0].lastUsedIdx = 0xFFFF;

            // Guest increments avail.idx from 0xFFFF -> 0x0000 (16-bit wrap)
            view.setUint16(availRingAddr + 2, 0x0000, true); // avail.idx = 0
            const slot = 0xFFFF % 256;
            view.setUint16(availRingAddr + 4 + slot * 2, 0, true); // desc 0

            // Kick queue
            dev.consumeVirtqueue(0);

            assert.equal(dev.queues[0].lastAvailIdx, 0, 'Avail index wrapped to 0');
            assert.equal(dev.queues[0].lastUsedIdx, 0, 'Used index wrapped to 0');
            assert.equal(view.getUint16(usedRingAddr + 2, true), 0, 'Used ring index updated in guest memory');
            assert.equal(lastCommand !== null, true, 'Command processed on wrap');
            assert.equal(dev.guestActive, true, 'guestActive set to true');
            assert.equal(dev.isHostInjectionAllowed(), false, 'Host injection locked out');
        });

        it('processes burst of 100 consecutive descriptor kicks without loss', () => {
            const guestMem = new Uint8Array(2 * 1024 * 1024);
            const view = new DataView(guestMem.buffer);

            let processedCount = 0;
            const mockBridge = {
                process_virtqueue_descriptor: () => {
                    processedCount++;
                    return 24;
                }
            };

            const dev = new VirtioGpuDevice(null, mockBridge, { width: 1280, height: 720 });
            dev.getGuestMemory = () => guestMem;

            dev.ioWrite(0x0E, 0, 2);
            dev.ioWrite(0x08, 0x10, 4);

            const descTableAddr = 0x10000;
            const availRingAddr = descTableAddr + 256 * 16;

            const burstCount = 100;
            for (let i = 0; i < burstCount; i++) {
                const descIdx = (i * 2) % 256;
                view.setUint32(descTableAddr + descIdx * 16, 0x30000, true);
                view.setUint32(descTableAddr + descIdx * 16 + 8, 24, true);

                const availSlot = i % 256;
                view.setUint16(availRingAddr + 4 + availSlot * 2, descIdx, true);
            }

            view.setUint16(availRingAddr + 2, burstCount, true); // avail.idx = 100
            dev.consumeVirtqueue(0);

            assert.equal(processedCount, burstCount, 'All 100 burst descriptors processed');
            assert.equal(dev.queues[0].lastAvailIdx, burstCount);
            assert.equal(dev.queues[0].lastUsedIdx, burstCount);
        });

        it('defends against circular / loop descriptor chains in JS fallback', () => {
            const guestMem = new Uint8Array(1024 * 1024);
            const view = new DataView(guestMem.buffer);

            const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });

            // Create circular loop: desc 0 -> desc 1 -> desc 0
            const descTable = 0x1000;
            view.setUint32(descTable + 0, 0x5000, true);
            view.setUint32(descTable + 8, 24, true);
            view.setUint16(descTable + 12, 0x01, true); // NEXT
            view.setUint16(descTable + 14, 1, true);    // Next = 1

            view.setUint32(descTable + 16, 0x6000, true);
            view.setUint32(descTable + 24, 24, true);
            view.setUint16(descTable + 28, 0x01, true); // NEXT
            view.setUint16(descTable + 30, 0, true);    // Next = 0 (Loop!)

            // Must terminate within 256 iterations without infinite freeze
            const written = dev.consumeDescriptorChainJs(guestMem, descTable, 0);
            assert.equal(typeof written, 'number', 'Circular chain terminated safely');
        });
    });

    // -------------------------------------------------------------------------
    // 3. Command Opcode Validation & Error Boundary Defenses
    // -------------------------------------------------------------------------
    describe('3. Command Opcodes & Error Invalidation', () => {
        it('handles malformed, truncated, and empty control queue packets gracefully', () => {
            const mockRustBridge = {
                process_command_packet: (pkt) => {
                    if (!pkt || pkt.length < 24) {
                        return new Uint8Array([0x00, 0x12, 0x00, 0x00]); // ERR_INVALID_PARAMETER
                    }
                    return new Uint8Array([0x00, 0x11, 0x00, 0x00]);
                }
            };

            const dev = new VirtioGpuDevice(null, mockRustBridge, { width: 1280, height: 720 });

            // Null packet
            const respNull = dev.processControlQueue(null);
            assert.equal(respNull.length >= 4, true);

            // 0-byte packet
            const resp0 = dev.processControlQueue(new Uint8Array(0));
            assert.equal(resp0.length >= 4, true);

            // 10-byte truncated packet
            const resp10 = dev.processControlQueue(new Uint8Array(10));
            assert.equal(resp10[1], 0x12, 'Returns ERR_INVALID_PARAMETER');
        });

        it('maintains scanout damage rect lifecycle through repeated dirty blits', () => {
            let currentDamage = [10, 20, 100, 50];
            let damageCleared = false;

            const mockRustBridge = {
                get_scanout_damage: () => currentDamage,
                get_scanout_framebuffer: () => new Uint8Array(1280 * 720 * 4).fill(255),
                get_scanout_framebuffer_rgba: () => new Uint8Array(1280 * 720 * 4).fill(255),
                clear_scanout_damage: () => { damageCleared = true; }
            };

            let renderedRect = null;
            const mockContext = {
                createImageData: (w, h) => ({ data: new Uint8Array(w * h * 4), width: w, height: h }),
                putImageData: (img, x, y, dx, dy, dw, dh) => {
                    renderedRect = [dx, dy, dw, dh];
                }
            };
            const mockCanvas = {
                width: 1280,
                height: 720,
                getContext: () => mockContext
            };

            const dev = new VirtioGpuDevice(null, mockRustBridge, mockCanvas);
            dev.renderScanoutToCanvas(0);

            assert.deepEqual(renderedRect, [10, 20, 100, 50], 'Damage subrect accurately passed to canvas');
            assert.equal(damageCleared, true, 'Damage rect cleared after presentation');
            assert.equal(dev.damage_rects_count, 1, 'Damage rect counter incremented');
        });
    });

    // -------------------------------------------------------------------------
    // 4. Shannon Entropy Robustness & Pixel Distributions
    // -------------------------------------------------------------------------
    describe('4. Shannon Entropy Robustness & Distribution Matrices', () => {
        it('calculates exact theoretical Shannon entropy across distinct distributions', () => {
            const totalPixels = 1000;

            // 1. Uniform Black (All 0x000000FF) -> Entropy must be 0.000
            const flatBuffer = new Uint8Array(totalPixels * 4);
            for (let i = 0; i < flatBuffer.length; i += 4) {
                flatBuffer[i + 3] = 255;
            }
            const resFlat = calculateShannonEntropy(flatBuffer);
            assert.equal(resFlat.entropy, 0.0, 'Flat color entropy is exactly 0.0');
            assert.equal(resFlat.uniqueColors, 1);

            // 2. 50/50 Binary Split (Red / Blue) -> Theoretical H = - (0.5*log2(0.5) + 0.5*log2(0.5)) = 1.000
            const binaryBuffer = new Uint8Array(totalPixels * 4);
            for (let i = 0; i < totalPixels; i++) {
                const off = i * 4;
                if (i < 500) {
                    binaryBuffer[off] = 255; // Red
                    binaryBuffer[off + 3] = 255;
                } else {
                    binaryBuffer[off + 2] = 255; // Blue
                    binaryBuffer[off + 3] = 255;
                }
            }
            const resBinary = calculateShannonEntropy(binaryBuffer);
            assert.equal(resBinary.entropy.toFixed(3), '1.000', '50/50 binary split entropy is exactly 1.000');
            assert.equal(resBinary.uniqueColors, 2);

            // 3. 4-Way Equal Quad Split -> Theoretical H = - 4 * (0.25 * log2(0.25)) = 2.000
            const quadBuffer = new Uint8Array(totalPixels * 4);
            for (let i = 0; i < totalPixels; i++) {
                const off = i * 4;
                quadBuffer[off + 3] = 255;
                if (i < 250) quadBuffer[off] = 255;          // Red
                else if (i < 500) quadBuffer[off + 1] = 255;  // Green
                else if (i < 750) quadBuffer[off + 2] = 255;  // Blue
                else { quadBuffer[off] = 255; quadBuffer[off + 1] = 255; } // Yellow
            }
            const resQuad = calculateShannonEntropy(quadBuffer);
            assert.equal(resQuad.entropy.toFixed(3), '2.000', '4-way equal split entropy is exactly 2.000');
            assert.equal(resQuad.uniqueColors, 4);

            // 4. Pure Uniform Noise (256 distinct equally distributed colors) -> Theoretical H = 8.000
            const noiseBuffer = new Uint8Array(256 * 4 * 10);
            for (let i = 0; i < noiseBuffer.length / 4; i++) {
                const off = i * 4;
                noiseBuffer[off] = i % 256;
                noiseBuffer[off + 1] = (i * 3) % 256;
                noiseBuffer[off + 2] = (i * 7) % 256;
                noiseBuffer[off + 3] = 255;
            }
            const resNoise = calculateShannonEntropy(noiseBuffer);
            assert.equal(resNoise.entropy >= 7.99, true, 'Uniform noise entropy approaches 8.0 bits/pixel');

            // 5. Empty / null buffers
            const resNull = calculateShannonEntropy(null);
            assert.equal(resNull.entropy, 0);
            const resEmpty = calculateShannonEntropy(new Uint8Array(0));
            assert.equal(resEmpty.entropy, 0);
        });
    });
});
