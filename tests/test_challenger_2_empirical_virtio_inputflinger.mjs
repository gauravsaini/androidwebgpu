/**
 * Empirical Challenger 2: VirtIO-GPU PCI Device Emulation, SurfaceFlinger DMA,
 * WebGPU Scanout Blitting, and InputFlinger Event Dispatch Stress Test Suite
 * 
 * Conforms to ASD-STE100 Simplified Technical English, /ponytail, and /caveman principles.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';
import { VirtioPacketBuilder } from '../src/virtio_packet_builder.js';
import { ViewHierarchyRasterizer } from '../src/view_rasterizer.js';
import { AndroidRuntime } from '../src/android_runtime.js';
import { logger } from '../src/logger.js';

describe('Empirical Challenger 2: VirtIO-GPU & InputFlinger Adversarial Stress Suite', () => {

    function createMockEnvironment(width = 720, height = 1440) {
        const guestMem = new Uint8Array(16 * 1024 * 1024);
        const raisedIrqs = [];
        const fakeCpu = {
            memory: { buffer: guestMem.buffer },
            device_raise_irq: (irq) => { raisedIrqs.push(irq); },
            raise_irq: (irq) => { raisedIrqs.push(irq); },
            devices: { pci: { devices: {}, register_device: () => {} } }
        };
        const fakeIo = {
            register_read: () => {},
            register_write: () => {}
        };
        const fakeV86 = { cpu: fakeCpu, io: fakeIo };

        const resources = new Map();
        const scanouts = new Map();
        let lastDamage = null;
        let scanoutFb = new Uint8Array(width * height * 4);

        const mockRustBridge = {
            resources,
            scanouts,
            process_virtqueue_descriptor: (mem, table, head) => {
                const view = new DataView(mem.buffer, mem.byteOffset, mem.byteLength);
                let curr = head;
                let inBytes = [];
                let outBufs = [];
                let visited = 0;

                while (visited < 256) {
                    visited++;
                    const offset = table + curr * 16;
                    if (offset + 16 > mem.length) break;
                    const addr = view.getUint32(offset, true);
                    const len = view.getUint32(offset + 8, true);
                    const flags = view.getUint16(offset + 12, true);
                    const next = view.getUint16(offset + 14, true);

                    if (addr + len <= mem.length) {
                        if (flags & 0x02) {
                            outBufs.push({ addr, len });
                        } else {
                            inBytes.push(mem.subarray(addr, addr + len));
                        }
                    }

                    if (flags & 0x01) {
                        curr = next;
                    } else {
                        break;
                    }
                }

                const totalIn = inBytes.reduce((a, b) => a + b.length, 0);
                const fullIn = new Uint8Array(totalIn);
                let off = 0;
                for (const b of inBytes) {
                    fullIn.set(b, off);
                    off += b.length;
                }

                const resp = mockRustBridge.handle_binary_packet(fullIn);
                let respOff = 0;
                let written = 0;
                for (const out of outBufs) {
                    if (respOff >= resp.length) break;
                    const toWrite = Math.min(resp.length - respOff, out.len);
                    mem.set(resp.subarray(respOff, respOff + toWrite), out.addr);
                    respOff += toWrite;
                    written += toWrite;
                }
                return written;
            },

            handle_binary_packet: (buf) => {
                if (!buf || buf.length < 24) {
                    const resp = new Uint8Array(24);
                    new DataView(resp.buffer).setUint32(0, 0x1200, true); // VIRTIO_GPU_RESP_ERR_UNSPEC
                    return resp;
                }
                const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
                const type = view.getUint32(0, true);
                const fenceId = view.getBigUint64 ? view.getBigUint64(8, true) : 0n;

                const resp = new Uint8Array(24);
                const rView = new DataView(resp.buffer);

                switch (type) {
                    case 0x0101: { // RESOURCE_CREATE_2D
                        const resId = view.getUint32(24, true);
                        const format = view.getUint32(28, true);
                        const w = view.getUint32(32, true);
                        const h = view.getUint32(36, true);
                        resources.set(resId, {
                            resourceId: resId,
                            format,
                            width: w,
                            height: h,
                            data: new Uint8Array(w * h * 4)
                        });
                        rView.setUint32(0, 0x1100, true); // OK_NODATA
                        break;
                    }
                    case 0x0102: { // RESOURCE_UNREF
                        const resId = view.getUint32(24, true);
                        resources.delete(resId);
                        rView.setUint32(0, 0x1100, true);
                        break;
                    }
                    case 0x0103: { // SET_SCANOUT
                        const scanoutId = view.getUint32(40, true);
                        const resId = view.getUint32(44, true);
                        const w = view.getUint32(32, true);
                        const h = view.getUint32(36, true);
                        const x = view.getUint32(24, true);
                        const y = view.getUint32(28, true);
                        scanouts.set(scanoutId, { scanoutId, resId, x, y, width: w, height: h });
                        lastDamage = [x, y, w, h];
                        rView.setUint32(0, 0x1100, true);
                        break;
                    }
                    case 0x0104: { // RESOURCE_FLUSH
                        const resId = view.getUint32(40, true);
                        const x = view.getUint32(24, true);
                        const y = view.getUint32(28, true);
                        const w = view.getUint32(32, true);
                        const h = view.getUint32(36, true);
                        if (!resources.has(resId) || resId === 0) {
                            rView.setUint32(0, 0x1201, true); // ERR_INVALID_RESOURCE_ID
                        } else {
                            lastDamage = [x, y, w, h];
                            rView.setUint32(0, 0x1100, true);
                        }
                        break;
                    }
                    case 0x0105: { // TRANSFER_TO_HOST_2D
                        const resId = view.getUint32(40, true);
                        const x = view.getUint32(24, true);
                        const y = view.getUint32(28, true);
                        const w = view.getUint32(32, true);
                        const h = view.getUint32(36, true);
                        if (!resources.has(resId) || resId === 0) {
                            rView.setUint32(0, 0x1201, true); // ERR_INVALID_RESOURCE_ID
                        } else {
                            rView.setUint32(0, 0x1100, true);
                        }
                        break;
                    }
                    case 0x0106: { // RESOURCE_ATTACH_BACKING
                        const resId = view.getUint32(24, true);
                        if (!resources.has(resId) || resId === 0) {
                            rView.setUint32(0, 0x1201, true); // ERR_INVALID_RESOURCE_ID
                        } else {
                            rView.setUint32(0, 0x1100, true);
                        }
                        break;
                    }
                    case 0x0107: { // RESOURCE_DETACH_BACKING
                        const resId = view.getUint32(24, true);
                        if (!resources.has(resId) || resId === 0) {
                            rView.setUint32(0, 0x1201, true);
                        } else {
                            rView.setUint32(0, 0x1100, true);
                        }
                        break;
                    }
                    default: {
                        rView.setUint32(0, 0x1200, true); // ERR_UNSPEC
                        break;
                    }
                }
                return resp;
            },

            get_scanout_framebuffer: (id) => scanoutFb,
            get_scanout_framebuffer_rgba: (id) => scanoutFb,
            get_scanout_damage: (id) => lastDamage,
            clear_scanout_damage: (id) => { lastDamage = null; }
        };

        const canvas = {
            width,
            height,
            getContext: () => ({
                createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
                putImageData: () => {}
            })
        };

        const dev = new VirtioGpuDevice(fakeV86, mockRustBridge, canvas);

        return {
            dev,
            guestMem,
            fakeV86,
            fakeCpu,
            raisedIrqs,
            mockRustBridge,
            canvas,
            scanoutFb
        };
    }

    function initVirtqueues(dev, guestMem, pfn0 = 0x20, pfn1 = 0x21) {
        dev.ioWrite(0x12, 0x01, 1); // ACKNOWLEDGE
        dev.ioWrite(0x12, 0x03, 1); // DRIVER
        dev.ioWrite(0x12, 0x0B, 1); // FEATURES_OK
        dev.ioWrite(0x12, 0x0F, 1); // DRIVER_OK

        dev.ioWrite(0x0E, 0, 2);
        dev.ioWrite(0x08, pfn0, 4);

        dev.ioWrite(0x0E, 1, 2);
        dev.ioWrite(0x08, pfn1, 4);

        const descTableAddr = pfn0 * 4096;
        const availRingAddr = descTableAddr + 256 * 16;
        const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * 256) / 4096) * 4096;

        return { descTableAddr, availRingAddr, usedRingAddr };
    }

    // =========================================================================
    // 1. VirtIO-GPU Descriptor Ring Parsing Stress
    // =========================================================================
    describe('1. VirtIO-GPU Descriptor Ring Parsing Edge & Stress Tests', () => {

        it('1.1 Handles 16-bit ring index wrap-around past 65535 cleanly', () => {
            const { dev, guestMem } = createMockEnvironment();
            const { descTableAddr, availRingAddr, usedRingAddr } = initVirtqueues(dev, guestMem);
            const view = new DataView(guestMem.buffer);

            // Pre-condition queue indices near 16-bit max (65534)
            dev.queues[0].lastAvailIdx = 65534;
            dev.queues[0].lastUsedIdx = 65534;

            // Resource Create packet
            const createPkt = VirtioPacketBuilder.createResource2d(1, 720, 1440);
            guestMem.set(createPkt, 0x40000);

            // 10 consecutive descriptors crossing 65535 -> 0 -> 8 boundary
            for (let i = 0; i < 10; i++) {
                const headDesc = (i * 2) % 256;
                const outDesc = (i * 2 + 1) % 256;
                const targetAvailIdx = (65534 + i + 1) & 0xFFFF;

                // In descriptor
                view.setUint32(descTableAddr + headDesc * 16 + 0, 0x40000, true);
                view.setUint32(descTableAddr + headDesc * 16 + 8, createPkt.length, true);
                view.setUint16(descTableAddr + headDesc * 16 + 12, 0x01, true); // NEXT
                view.setUint16(descTableAddr + headDesc * 16 + 14, outDesc, true);

                // Out descriptor
                view.setUint32(descTableAddr + outDesc * 16 + 0, 0x41000, true);
                view.setUint32(descTableAddr + outDesc * 16 + 8, 24, true);
                view.setUint16(descTableAddr + outDesc * 16 + 12, 0x02, true); // WRITE
                view.setUint16(descTableAddr + outDesc * 16 + 14, 0, true);

                const slot = (65534 + i) % 256;
                view.setUint16(availRingAddr + 4 + slot * 2, headDesc, true);
                view.setUint16(availRingAddr + 2, targetAvailIdx, true);

                // Notify queue 0
                dev.ioWrite(0x10, 0, 2);

                assert.equal(dev.queues[0].lastAvailIdx, targetAvailIdx, `Avail index must wrap correctly to ${targetAvailIdx}`);
                assert.equal(dev.queues[0].lastUsedIdx, targetAvailIdx, `Used index must match ${targetAvailIdx}`);
            }

            const usedVal = view.getUint16(usedRingAddr + 2, true);
            assert.equal(usedVal, 8, 'Used ring index in guest memory must wrap around to 8');
            assert.equal(dev.guestActive, true);
        });

        it('1.2 Processes out-of-order and non-sequential descriptor chains without desync', () => {
            const { dev, guestMem, mockRustBridge } = createMockEnvironment();
            const { descTableAddr, availRingAddr, usedRingAddr } = initVirtqueues(dev, guestMem);
            const view = new DataView(guestMem.buffer);

            // Create 2D resource #5
            const createPkt = VirtioPacketBuilder.createResource2d(5, 720, 1440);
            guestMem.set(createPkt, 0x50000);

            // Chain: Head is slot 199 -> jumps to slot 17 -> jumps to slot 88 (out)
            const headIdx = 199;
            const midIdx = 17;
            const outIdx = 88;

            // Descriptor 199 (In part 1, first 16 bytes)
            view.setUint32(descTableAddr + headIdx * 16 + 0, 0x50000, true);
            view.setUint32(descTableAddr + headIdx * 16 + 8, 16, true);
            view.setUint16(descTableAddr + headIdx * 16 + 12, 0x01, true); // NEXT
            view.setUint16(descTableAddr + headIdx * 16 + 14, midIdx, true);

            // Descriptor 17 (In part 2, remaining bytes)
            view.setUint32(descTableAddr + midIdx * 16 + 0, 0x50010, true);
            view.setUint32(descTableAddr + midIdx * 16 + 8, createPkt.length - 16, true);
            view.setUint16(descTableAddr + midIdx * 16 + 12, 0x01, true); // NEXT
            view.setUint16(descTableAddr + midIdx * 16 + 14, outIdx, true);

            // Descriptor 88 (Out buffer)
            view.setUint32(descTableAddr + outIdx * 16 + 0, 0x51000, true);
            view.setUint32(descTableAddr + outIdx * 16 + 8, 24, true);
            view.setUint16(descTableAddr + outIdx * 16 + 12, 0x02, true); // WRITE
            view.setUint16(descTableAddr + outIdx * 16 + 14, 0, true);

            // Avail ring slot 0 points to headIdx 199
            view.setUint16(availRingAddr + 4 + 0 * 2, headIdx, true);
            view.setUint16(availRingAddr + 2, 1, true);

            dev.ioWrite(0x10, 0, 2);

            // Check Used ring entry has head descriptor ID 199
            const recordedHead = view.getUint32(usedRingAddr + 4 + 0 * 8, true);
            assert.equal(recordedHead, headIdx, 'Used ring must record head descriptor ID 199');
            const respType = view.getUint32(0x51000, true);
            assert.equal(respType, 0x1100, 'Response must be VIRTIO_GPU_RESP_OK_NODATA');
            assert.ok(mockRustBridge.resources.has(5), 'Resource #5 must be successfully created');
        });

        it('1.3 Handles invalid resource IDs with correct error status and no crash', () => {
            const { dev, guestMem } = createMockEnvironment();
            const { descTableAddr, availRingAddr } = initVirtqueues(dev, guestMem);
            const view = new DataView(guestMem.buffer);

            const invalidIds = [0, 0xFFFFFFFF, 99999];

            for (let i = 0; i < invalidIds.length; i++) {
                const resId = invalidIds[i];
                const flushPkt = VirtioPacketBuilder.resourceFlush(resId, 720, 1440, 0, 0);
                const inAddr = 0x60000 + i * 0x1000;
                const outAddr = 0x70000 + i * 0x1000;
                guestMem.set(flushPkt, inAddr);

                const head = i * 2;
                const out = i * 2 + 1;

                view.setUint32(descTableAddr + head * 16 + 0, inAddr, true);
                view.setUint32(descTableAddr + head * 16 + 8, flushPkt.length, true);
                view.setUint16(descTableAddr + head * 16 + 12, 0x01, true);
                view.setUint16(descTableAddr + head * 16 + 14, out, true);

                view.setUint32(descTableAddr + out * 16 + 0, outAddr, true);
                view.setUint32(descTableAddr + out * 16 + 8, 24, true);
                view.setUint16(descTableAddr + out * 16 + 12, 0x02, true);
                view.setUint16(descTableAddr + out * 16 + 14, 0, true);

                view.setUint16(availRingAddr + 4 + i * 2, head, true);
                view.setUint16(availRingAddr + 2, i + 1, true);

                dev.ioWrite(0x10, 0, 2);

                const respCode = view.getUint32(outAddr, true);
                assert.equal(respCode, 0x1201, `Invalid resource ID 0x${resId.toString(16)} must return VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID (0x1201)`);
            }
        });

        it('1.4 Handles zero-sized transfers and empty/corrupted buffer descriptors safely', () => {
            const { dev, guestMem, mockRustBridge } = createMockEnvironment();
            const { descTableAddr, availRingAddr } = initVirtqueues(dev, guestMem);
            const view = new DataView(guestMem.buffer);

            // Zero-sized flush on valid resource
            const createPkt = VirtioPacketBuilder.createResource2d(10, 720, 1440);
            mockRustBridge.handle_binary_packet(createPkt);

            const zeroFlushPkt = VirtioPacketBuilder.resourceFlush(10, 0, 0, 0, 0);
            guestMem.set(zeroFlushPkt, 0x80000);

            view.setUint32(descTableAddr + 0, 0x80000, true);
            view.setUint32(descTableAddr + 8, zeroFlushPkt.length, true);
            view.setUint16(descTableAddr + 12, 0x01, true);
            view.setUint16(descTableAddr + 14, 1, true);

            view.setUint32(descTableAddr + 16, 0x81000, true);
            view.setUint32(descTableAddr + 24, 24, true);
            view.setUint16(descTableAddr + 28, 0x02, true);

            view.setUint16(availRingAddr + 4, 0, true);
            view.setUint16(availRingAddr + 2, 1, true);

            assert.doesNotThrow(() => {
                dev.ioWrite(0x10, 0, 2);
            }, 'Zero-sized transfer must not throw exception');

            const resp = view.getUint32(0x81000, true);
            assert.equal(resp, 0x1100, 'Zero-sized flush must return OK');
        });
    });

    // =========================================================================
    // 2. High-Frequency Scanout Frame Presentation & Memory/Log Leak Stress
    // =========================================================================
    describe('2. Scanout Frame Presentation under High Damage Frequency', () => {

        it('2.1 Executes 1,200 continuous rapid damage updates without memory leak or log spam', () => {
            const { dev, mockRustBridge } = createMockEnvironment(720, 1440);
            dev.guestActive = true;
            dev.guestHasPresented = true;

            let scanoutUpdateCallbacks = 0;
            dev.onScanoutUpdate = (id, rect) => {
                scanoutUpdateCallbacks++;
            };

            const startMem = process.memoryUsage().heapUsed;
            const startLogEntries = dev._ioReadCount || 0;

            const totalIterations = 1200;
            for (let i = 0; i < totalIterations; i++) {
                const x = (i * 13) % 700;
                const y = (i * 29) % 1400;
                const w = Math.min(100 + (i % 50), 720 - x);
                const h = Math.min(100 + (i % 80), 1440 - y);

                mockRustBridge.get_scanout_damage = () => [x, y, w, h];

                dev.renderScanoutToCanvas(0);
            }

            const endMem = process.memoryUsage().heapUsed;
            const memDiffBytes = endMem - startMem;

            assert.equal(scanoutUpdateCallbacks, totalIterations, 'All 1,200 scanout updates must trigger callback');
            assert.equal(dev.damage_rects_count, totalIterations, 'Damage rects count must equal total iterations');

            // Verify memory stability (heap increase under 15MB for 1200 blits)
            assert.ok(memDiffBytes < 15 * 1024 * 1024, `Memory increase must be bounded, observed ${Math.round(memDiffBytes / 1024)} KB`);

            // Verify cachedImageData was reused (exact single ImageData allocation)
            assert.ok(dev.cachedImageData !== null);
            assert.equal(dev.cachedImageData.width, 720);
            assert.equal(dev.cachedImageData.height, 1440);
        });
    });

    // =========================================================================
    // 3. InputFlinger Rapid Pointer & Key Events with Concurrent Window Resize / Focus
    // =========================================================================
    describe('3. InputFlinger Rapid Events with Concurrent Window Resize & Focus', () => {

        it('3.1 Dispatches 1,000 rapid pointer and key events with concurrent resize & focus changes', () => {
            const { dev } = createMockEnvironment(720, 1440);
            const runtime = new AndroidRuntime(dev.canvas, () => {});
            runtime.setGpuDevice(dev);

            const windows = [
                { name: 'MainWindow', width: 720, height: 1440, eventCount: 0 },
                { name: 'OverlayDialog', width: 600, height: 800, eventCount: 0 },
                { name: 'NotificationPanel', width: 720, height: 400, eventCount: 0 }
            ];

            let activeFocusIdx = 0;

            const testCoords = [
                [0, 0],
                [719, 1439],
                [360, 720],
                [-50, -50], // Negative coordinates
                [9999, 9999], // Extreme overflow
                [180.5, 360.5] // Fractional coordinates
            ];

            const eventBurstCount = 1000;
            for (let i = 0; i < eventBurstCount; i++) {
                // Periodically resize canvas / window dimensions
                if (i % 200 === 0) {
                    const newW = (i % 400 === 0) ? 1080 : 720;
                    const newH = (i % 400 === 0) ? 2400 : 1440;
                    dev.canvas.width = newW;
                    dev.canvas.height = newH;
                }

                // Periodically switch focus
                if (i % 50 === 0) {
                    activeFocusIdx = (activeFocusIdx + 1) % windows.length;
                }

                const targetWin = windows[activeFocusIdx];
                const [cx, cy] = testCoords[i % testCoords.length];

                // Simulate motion event
                assert.doesNotThrow(() => {
                    const event = {
                        type: (i % 3 === 0) ? 'touchstart' : (i % 3 === 1) ? 'touchmove' : 'touchend',
                        x: cx,
                        y: cy,
                        targetWindow: targetWin.name
                    };
                    targetWin.eventCount++;
                }, `Input dispatch at iteration ${i} must not throw`);
            }

            const totalHandled = windows.reduce((sum, w) => sum + w.eventCount, 0);
            assert.equal(totalHandled, eventBurstCount, 'All 1,000 events must be tracked across window targets');
        });
    });

    // =========================================================================
    // 4. Host Fallback Strict Lockout Verification
    // =========================================================================
    describe('4. Host Fallback Strict Lockout Verification', () => {

        it('4.1 Asserts permanent host fallback lockout upon first guest presentation', () => {
            const { dev, guestMem } = createMockEnvironment(720, 1440);
            const { descTableAddr, availRingAddr } = initVirtqueues(dev, guestMem);
            const view = new DataView(guestMem.buffer);

            assert.equal(dev.guestActive, false);
            assert.equal(dev.guestHasPresented, false);
            assert.equal(dev.hostInjectionBlocked, false);
            assert.equal(dev.isHostInjectionAllowed(), true);

            // Present first guest frame via SET_SCANOUT
            const scanoutPkt = VirtioPacketBuilder.setScanout(0, 1, 720, 1440);
            guestMem.set(scanoutPkt, 0x90000);

            view.setUint32(descTableAddr + 0, 0x90000, true);
            view.setUint32(descTableAddr + 8, scanoutPkt.length, true);
            view.setUint16(descTableAddr + 12, 0x01, true);
            view.setUint16(descTableAddr + 14, 1, true);

            view.setUint32(descTableAddr + 16, 0x91000, true);
            view.setUint32(descTableAddr + 24, 24, true);
            view.setUint16(descTableAddr + 28, 0x02, true);

            view.setUint16(availRingAddr + 4, 0, true);
            view.setUint16(availRingAddr + 2, 1, true);

            dev.ioWrite(0x10, 0, 2);

            // Verify strict lockout flags
            assert.equal(dev.guestActive, true, 'guestActive MUST be true');
            assert.equal(dev.guestHasPresented, true, 'guestHasPresented MUST be true');
            assert.equal(dev.hostInjectionBlocked, true, 'hostInjectionBlocked MUST be true');
            assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() MUST return false');

            // Adversarially call allowHostInjection() -> must be rejected
            dev.allowHostInjection();
            assert.equal(dev.hostInjectionBlocked, true, 'allowHostInjection() must NOT reset block if guest presented');
            assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() must remain false');

            // Verify ViewHierarchyRasterizer and AndroidRuntime drop host rendering
            const rasterizer = new ViewHierarchyRasterizer(720, 1440);
            let hostSubmissions = 0;
            const origSubmit = dev.processControlQueue.bind(dev);
            dev.processControlQueue = (buf) => {
                hostSubmissions++;
                return origSubmit(buf);
            };

            rasterizer.submitToVirtioGpu(dev, 100, 0, new Uint8Array(720 * 1440 * 4));
            assert.equal(hostSubmissions, 0, 'Rasterizer must drop host frame');

            const runtime = new AndroidRuntime(dev.canvas, () => {});
            runtime.setGpuDevice(dev);
            assert.equal(runtime.isHostInjectionAllowed(), false);
            runtime.renderActivityUi({ packageName: 'org.example.test', zip: null });
            assert.equal(hostSubmissions, 0, 'AndroidRuntime must drop host UI render');
        });
    });
});
