/**
 * Challenger M4: Empirical Pipeline & In-Guest DRM Scanout Stress Test Suite
 * 
 * Rules: ASD-STE100 Simplified Technical English, /ponytail, /caveman
 */

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';
import { VirtioPacketBuilder } from '../src/virtio_packet_builder.js';
import { ViewHierarchyRasterizer } from '../src/view_rasterizer.js';
import { AndroidRuntime } from '../src/android_runtime.js';
import { calculateShannonEntropy } from '../validate_browser.mjs';

describe('Challenger M4: VirtIO GPU Pipeline, guestActive Gating & DRM Scanout Empirical Suite', () => {

    function createTestRig(width = 720, height = 1440) {
        const guestMem = new Uint8Array(16 * 1024 * 1024);
        let raisedIrqs = [];
        const fakeV86 = {
            cpu: {
                memory: { buffer: guestMem.buffer },
                device_raise_irq: (irq) => { raisedIrqs.push(irq); },
                raise_irq: (irq) => { raisedIrqs.push(irq); },
                devices: { pci: { devices: {}, register_device: () => {} } }
            },
            io: {
                register_read: () => {},
                register_write: () => {}
            }
        };

        let scanoutFb = new Uint8Array(width * height * 4);
        let damageRect = [0, 0, width, height];
        let processedDescriptors = 0;

        const mockBridge = {
            process_virtqueue_descriptor: (mem, descTable, head) => {
                processedDescriptors++;
                const view = new DataView(mem.buffer, mem.byteOffset, mem.byteLength);
                const outAddrLo = view.getUint32(descTable + 1 * 16, true);
                const resp = new Uint8Array(24);
                const respView = new DataView(resp.buffer);
                respView.setUint32(0, 0x1100, true); // VIRTIO_GPU_RESP_OK_NODATA
                mem.set(resp, outAddrLo);
                return resp.length;
            },
            get_scanout_framebuffer: () => scanoutFb,
            get_scanout_framebuffer_rgba: (scanoutId) => {
                // Swizzle BGRX to RGBA
                const rgba = new Uint8Array(scanoutFb.length);
                for (let i = 0; i < scanoutFb.length; i += 4) {
                    rgba[i] = scanoutFb[i + 2];     // R
                    rgba[i + 1] = scanoutFb[i + 1]; // G
                    rgba[i + 2] = scanoutFb[i];     // B
                    rgba[i + 3] = 255;             // A
                }
                return rgba;
            },
            get_scanout_damage: () => damageRect,
            clear_scanout_damage: () => { damageRect = null; }
        };

        let lastPutImageData = null;
        let lastPutRect = null;
        const fakeCanvas = {
            width,
            height,
            getContext: () => ({
                createImageData: (w, h) => ({ data: new Uint8Array(w * h * 4), width: w, height: h }),
                putImageData: (imgData, x, y, dx, dy, dw, dh) => {
                    lastPutImageData = imgData;
                    if (dx !== undefined) {
                        lastPutRect = [dx, dy, dw, dh];
                    } else {
                        lastPutRect = [0, 0, imgData.width, imgData.height];
                    }
                }
            })
        };

        const dev = new VirtioGpuDevice(fakeV86, mockBridge, fakeCanvas);

        return {
            dev,
            guestMem,
            fakeV86,
            mockBridge,
            fakeCanvas,
            raisedIrqs,
            getProcessedCount: () => processedDescriptors,
            setScanoutFb: (buf) => { scanoutFb = buf; },
            setDamageRect: (r) => { damageRect = r; },
            getLastPutImageData: () => lastPutImageData,
            getLastPutRect: () => lastPutRect
        };
    }

    function setupQueue(dev, guestMem, queueIdx = 0, pfn = 0x10, queueSize = 256) {
        dev.ioWrite(0x12, 0x0F, 1); // STATUS = DRIVER_OK
        dev.ioWrite(0x0E, queueIdx, 2); // Select queue
        dev.ioWrite(0x08, pfn, 4); // Set PFN

        const descTableAddr = pfn * 4096;
        const availRingAddr = descTableAddr + queueSize * 16;
        const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * queueSize) / 4096) * 4096;

        return { descTableAddr, availRingAddr, usedRingAddr };
    }

    // =========================================================================
    // Suite 1: VirtioGpuDevice.guestActive Gating & State Machine Invariants
    // =========================================================================
    describe('1. VirtioGpuDevice.guestActive Gating & State Machine Invariants', () => {

        it('1.1 Initial state invariant: guestActive false, host injection permitted', () => {
            const { dev } = createTestRig();
            assert.equal(dev.guestActive, false, 'dev.guestActive must initially be false');
            assert.equal(dev.guestHasPresented, false, 'dev.guestHasPresented must initially be false');
            assert.equal(dev.hostInjectionBlocked, false, 'dev.hostInjectionBlocked must initially be false');
            assert.equal(dev.isHostInjectionAllowed(), true, 'dev.isHostInjectionAllowed() must initially return true');
        });

        it('1.2 Spurious kicks with empty avail ring do NOT set guestActive', () => {
            const { dev, guestMem } = createTestRig();
            setupQueue(dev, guestMem, 0, 0x10, 256);

            // Trigger empty kick (lastAvailIdx == availIdx == 0)
            dev.consumeVirtqueue(0);
            assert.equal(dev.guestActive, false, 'Spurious kick with empty ring must NOT set guestActive');
            assert.equal(dev.isHostInjectionAllowed(), true, 'Host injection must remain allowed');

            // Trigger kick with PFN=0
            dev.queues[0].pfn = 0;
            dev.consumeVirtqueue(0);
            assert.equal(dev.guestActive, false, 'Kick with PFN=0 must NOT set guestActive');
            assert.equal(dev.isHostInjectionAllowed(), true);
        });

        it('1.3 Valid virtqueue descriptor kick locks guestActive and hostInjectionBlocked permanently', () => {
            const { dev, guestMem, raisedIrqs } = createTestRig(720, 1440);
            const { descTableAddr, availRingAddr, usedRingAddr } = setupQueue(dev, guestMem, 0, 0x10, 256);

            const view = new DataView(guestMem.buffer);

            // Put a valid command packet in guest memory
            const cmd = VirtioPacketBuilder.setScanout(0, 1, 720, 1440);
            const cmdAddr = 0x30000;
            const respAddr = 0x31000;
            guestMem.set(cmd, cmdAddr);

            // Descriptor 0 (In: command, NEXT)
            view.setUint32(descTableAddr + 0 * 16 + 0, cmdAddr, true);
            view.setUint32(descTableAddr + 0 * 16 + 8, cmd.length, true);
            view.setUint16(descTableAddr + 0 * 16 + 12, 0x01, true); // NEXT
            view.setUint16(descTableAddr + 0 * 16 + 14, 1, true);

            // Descriptor 1 (Out: response, WRITE)
            view.setUint32(descTableAddr + 1 * 16 + 0, respAddr, true);
            view.setUint32(descTableAddr + 1 * 16 + 8, 24, true);
            view.setUint16(descTableAddr + 1 * 16 + 12, 0x02, true); // WRITE
            view.setUint16(descTableAddr + 1 * 16 + 14, 0, true);

            // Set Avail ring
            view.setUint16(availRingAddr + 4 + 0 * 2, 0, true); // slot 0 -> desc 0
            view.setUint16(availRingAddr + 2, 1, true); // avail.idx = 1

            // Kick via I/O Port 0xC110
            dev.ioWrite(0x10, 0, 2);

            assert.equal(dev.guestActive, true, 'guestActive MUST be true');
            assert.equal(dev.guestHasPresented, true, 'guestHasPresented MUST be true');
            assert.equal(dev.hostInjectionBlocked, true, 'hostInjectionBlocked MUST be true');
            assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() MUST return false');
            assert.ok(raisedIrqs.includes(10), 'IRQ 10 raised for guest driver');
        });

        it('1.4 Adversarial attempt to call allowHostInjection() after guest kicks is rejected', () => {
            const { dev, guestMem } = createTestRig();
            const { descTableAddr, availRingAddr } = setupQueue(dev, guestMem, 0, 0x10, 256);
            const view = new DataView(guestMem.buffer);

            // Activate guest
            view.setUint16(availRingAddr + 4, 0, true);
            view.setUint16(availRingAddr + 2, 1, true);
            dev.consumeVirtqueue(0);

            assert.equal(dev.guestActive, true);
            assert.equal(dev.isHostInjectionAllowed(), false);

            // Attempt adversarial bypass
            dev.allowHostInjection();
            assert.equal(dev.hostInjectionBlocked, true, 'allowHostInjection() MUST NOT reset hostInjectionBlocked');
            assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() MUST remain false');

            // Attempt property tampering
            dev.guestActive = false;
            assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() MUST remain false via guestHasPresented');
        });

        it('1.5 16-bit Virtqueue ring index wrap-around past 65535 preserves sync and lockout', () => {
            const { dev, guestMem } = createTestRig();
            const { descTableAddr, availRingAddr, usedRingAddr } = setupQueue(dev, guestMem, 0, 0x10, 256);
            const view = new DataView(guestMem.buffer);

            // Descriptor 0
            view.setUint32(descTableAddr + 0, 0x20000, true);
            view.setUint32(descTableAddr + 8, 24, true);

            // Set lastAvailIdx to 65535 (0xFFFF)
            dev.queues[0].lastAvailIdx = 0xFFFF;
            dev.queues[0].lastUsedIdx = 0xFFFF;

            // Guest sets avail.idx to 0x0001 (wrapped past 0xFFFF)
            const slot = 0xFFFF % 256;
            view.setUint16(availRingAddr + 4 + slot * 2, 0, true);
            view.setUint16(availRingAddr + 2, 0x0000, true);

            dev.consumeVirtqueue(0);

            assert.equal(dev.queues[0].lastAvailIdx, 0x0000, 'Queue lastAvailIdx wrapped cleanly to 0');
            assert.equal(dev.queues[0].lastUsedIdx, 0x0000, 'Queue lastUsedIdx wrapped cleanly to 0');
            assert.equal(dev.guestActive, true);
            assert.equal(dev.isHostInjectionAllowed(), false);
        });
    });

    // =========================================================================
    // Suite 2: Synthetic Frame Injection Gating Stress
    // =========================================================================
    describe('2. Synthetic Frame Injection Gating Stress', () => {

        it('2.1 AndroidRuntime.renderActivityUi drops synthetic frame submissions when guestActive is true', () => {
            const { dev, guestMem } = createTestRig(720, 1440);
            const { descTableAddr, availRingAddr } = setupQueue(dev, guestMem, 0, 0x10, 256);
            const view = new DataView(guestMem.buffer);

            // Activate guest
            view.setUint16(availRingAddr + 4, 0, true);
            view.setUint16(availRingAddr + 2, 1, true);
            dev.consumeVirtqueue(0);
            assert.equal(dev.guestActive, true);

            let controlQueuePackets = 0;
            const origProcessControlQueue = dev.processControlQueue.bind(dev);
            dev.processControlQueue = (cmdBuf) => {
                controlQueuePackets++;
                return origProcessControlQueue(cmdBuf);
            };

            const runtime = new AndroidRuntime(dev.canvas, () => {});
            runtime.setGpuDevice(dev);

            assert.equal(runtime.isHostInjectionAllowed(), false, 'runtime.isHostInjectionAllowed() MUST be false');

            // Attempt synthetic UI rendering
            runtime.renderActivityUi({ packageName: 'org.fdroid.fdroid', zip: null });

            assert.equal(controlQueuePackets, 0, 'Zero synthetic control queue packets MUST be dispatched');
        });

        it('2.2 ViewHierarchyRasterizer.submitToVirtioGpu skips submission when host injection is disallowed', () => {
            const { dev } = createTestRig(720, 1440);
            dev.blockHostInjection();

            let submittedPackets = 0;
            dev.processControlQueue = () => { submittedPackets++; return new Uint8Array(24); };

            const rasterizer = new ViewHierarchyRasterizer(720, 1440);
            const dummyPixels = new Uint8Array(720 * 1440 * 4);

            rasterizer.submitToVirtioGpu(dev, 100, 0, dummyPixels);

            assert.equal(submittedPackets, 0, 'ViewHierarchyRasterizer MUST NOT submit when host injection blocked');
        });

        it('2.3 High-concurrency storm (100 synthetic render invocations) generates zero leakage', () => {
            const { dev, guestMem } = createTestRig(720, 1440);
            const { descTableAddr, availRingAddr } = setupQueue(dev, guestMem, 0, 0x10, 256);
            const view = new DataView(guestMem.buffer);

            // Activate guest
            view.setUint16(availRingAddr + 4, 0, true);
            view.setUint16(availRingAddr + 2, 1, true);
            dev.consumeVirtqueue(0);

            let leakedCalls = 0;
            dev.processControlQueue = () => { leakedCalls++; return new Uint8Array(24); };

            const runtime = new AndroidRuntime(dev.canvas, () => {});
            runtime.setGpuDevice(dev);

            for (let i = 0; i < 100; i++) {
                runtime.renderActivityUi({ packageName: `app.stress.${i}`, zip: null });
            }

            assert.equal(leakedCalls, 0, 'Zero leaked synthetic packets across 100 invocations');
        });
    });

    // =========================================================================
    // Suite 3: Scanout Frame Processing, BGRX/RGBA Swizzling & DRM Damage Rects
    // =========================================================================
    describe('3. Scanout Frame Processing, BGRX/RGBA Swizzling & DRM Damage Rects', () => {

        it('3.1 Accurate BGRX to RGBA swizzle and forced alpha 255', () => {
            const { dev, setScanoutFb, getLastPutImageData } = createTestRig(2, 1);
            // BGRX format: [Blue=0xAA, Green=0xBB, Red=0xCC, X=0x00, Blue=0x11, Green=0x22, Red=0x33, X=0x00]
            const bgrx = new Uint8Array([0xAA, 0xBB, 0xCC, 0x00, 0x11, 0x22, 0x33, 0x00]);
            setScanoutFb(bgrx);

            dev.renderScanoutToCanvas(0);

            const img = getLastPutImageData();
            assert.ok(img !== null, 'Canvas received ImageData');
            assert.equal(img.data[0], 0xCC, 'Pixel 0 Red = 0xCC');
            assert.equal(img.data[1], 0xBB, 'Pixel 0 Green = 0xBB');
            assert.equal(img.data[2], 0xAA, 'Pixel 0 Blue = 0xAA');
            assert.equal(img.data[3], 255, 'Pixel 0 Alpha = 255');

            assert.equal(img.data[4], 0x33, 'Pixel 1 Red = 0x33');
            assert.equal(img.data[5], 0x22, 'Pixel 1 Green = 0x22');
            assert.equal(img.data[6], 0x11, 'Pixel 1 Blue = 0x11');
            assert.equal(img.data[7], 255, 'Pixel 1 Alpha = 255');
        });

        it('3.2 DRM damage subrect clipping and out-of-bounds boundary protection', () => {
            const { dev, setScanoutFb, setDamageRect, getLastPutRect } = createTestRig(720, 1440);
            setScanoutFb(new Uint8Array(720 * 1440 * 4).fill(128));

            // Case A: Valid internal subrect [100, 200, 300, 400]
            setDamageRect([100, 200, 300, 400]);
            dev.renderScanoutToCanvas(0);
            assert.deepEqual(getLastPutRect(), [100, 200, 300, 400], 'Valid subrect rendered');

            // Case B: Oversized subrect [600, 1300, 500, 500] -> clamped to [600, 1300, 120, 140]
            setDamageRect([600, 1300, 500, 500]);
            dev.renderScanoutToCanvas(0);
            assert.deepEqual(getLastPutRect(), [600, 1300, 120, 140], 'Oversized subrect clamped accurately');

            // Case C: Out of bounds rect [1000, 2000, 100, 100] -> falls back to full blit [0, 0, 720, 1440]
            setDamageRect([1000, 2000, 100, 100]);
            dev.renderScanoutToCanvas(0);
            assert.deepEqual(getLastPutRect(), [0, 0, 720, 1440], 'Out of bounds damage falls back safely');
        });

        it('3.3 Framebuffer size mismatch robustness', () => {
            const { dev, setScanoutFb } = createTestRig(720, 1440);

            // Smaller framebuffer than canvas (e.g. 100 bytes)
            setScanoutFb(new Uint8Array(100).fill(255));
            assert.doesNotThrow(() => {
                dev.renderScanoutToCanvas(0);
            }, 'Smaller framebuffer handled without crash');

            // Larger framebuffer than canvas (e.g. 10 MB)
            setScanoutFb(new Uint8Array(10 * 1024 * 1024).fill(255));
            assert.doesNotThrow(() => {
                dev.renderScanoutToCanvas(0);
            }, 'Larger framebuffer clamped without crash');
        });
    });

    // =========================================================================
    // Suite 4: Shannon Entropy Robustness & PNG Verification
    // =========================================================================
    describe('4. Shannon Entropy Robustness & PNG Verification', () => {

        it('4.1 Shannon entropy calculation handles boundary distributions', () => {
            // All zeros
            const zeros = new Uint8Array(1000 * 4);
            const r0 = calculateShannonEntropy(zeros);
            assert.equal(r0.entropy, 0.0);
            assert.equal(r0.uniqueColors, 1);
            assert.equal(r0.nonZeroPixels, 0);

            // 50/50 binary split
            const binary = new Uint8Array(1000 * 4);
            for (let i = 0; i < 500; i++) {
                binary[i * 4] = 255; binary[i * 4 + 3] = 255; // Red
            }
            for (let i = 500; i < 1000; i++) {
                binary[i * 4 + 2] = 255; binary[i * 4 + 3] = 255; // Blue
            }
            const rBin = calculateShannonEntropy(binary);
            assert.equal(rBin.entropy.toFixed(3), '1.000');
            assert.equal(rBin.uniqueColors, 2);
            assert.equal(rBin.nonZeroRatio, 1.0);
        });

        it('4.2 Empirical verify_screenshot.py executes via uv and passes all checks', () => {
            const res = spawnSync('uv', ['run', 'python', 'tests/verify_screenshot.py'], {
                encoding: 'utf-8'
            });
            console.log(res.stdout);
            if (res.stderr) console.error(res.stderr);
            assert.equal(res.status, 0, 'verify_screenshot.py must exit with 0');
            assert.ok(res.stdout.includes('ALL PNG DECODE AND ENTROPY CHECKS PASSED EMPIRICALLY!'));
        });
    });
});
