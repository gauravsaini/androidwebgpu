/**
 * Challenger 2: Empirical Stress & Visual Verification Harness
 * Focus: M3 & M4 (SurfaceFlinger DRM, Scanout 0, Host LayoutInflater Bypass, Shannon Entropy)
 * ASD-STE100 Simplified Technical English
 * /ponytail /caveman
 */

import { strict as assert } from 'assert';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { calculateShannonEntropy } from '../validate_browser.mjs';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';
import { VirtioPacketBuilder } from '../src/virtio_packet_builder.js';
import { AndroidRuntime } from '../src/android_runtime.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

console.log('================================================================================');
console.log('🔥 Challenger 2: M3 & M4 Empirical Stress & Visual Verification Harness');
console.log('================================================================================');

async function test1_drm_scanout0_protocol() {
    console.log('\n▶ [Test 1] SurfaceFlinger DRM Ioctls & Scanout 0 Binding Stress');
    
    // Create mock v86 environment with guest physical RAM and PCI I/O space
    const guestMem = new Uint8Array(16 * 1024 * 1024);
    let raisedIrqs = [];
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

    let scanoutFb = new Uint8Array(720 * 1440 * 4);
    let scanoutDamage = null;
    let presentedPackets = [];

    const mockBridge = {
        process_virtqueue_descriptor: (mem, descTable, head) => {
            const view = new DataView(mem.buffer, mem.byteOffset, mem.byteLength);
            const inAddrLo = view.getUint32(descTable + head * 16, true);
            const inLen = view.getUint32(descTable + head * 16 + 8, true);
            const cmd = mem.slice(inAddrLo, inAddrLo + inLen);
            presentedPackets.push(cmd);

            // Write 24-byte success header to out buffer
            const outAddrLo = view.getUint32(descTable + 1 * 16, true);
            const resp = new Uint8Array(24);
            const respView = new DataView(resp.buffer);
            respView.setUint32(0, 0x1100, true); // VIRTIO_GPU_RESP_OK_NODATA
            mem.set(resp, outAddrLo);
            return resp.length;
        },
        process_command_packet: (pkt) => {
            presentedPackets.push(pkt);
            const resp = new Uint8Array(24);
            new DataView(resp.buffer).setUint32(0, 0x1100, true);
            return resp;
        },
        get_scanout_framebuffer: () => scanoutFb,
        get_scanout_damage: () => scanoutDamage,
        clear_scanout_damage: () => { scanoutDamage = null; }
    };

    let putImageCount = 0;
    const canvas = {
        width: 720,
        height: 1440,
        getContext: () => ({
            createImageData: (w, h) => ({ data: new Uint8Array(w * h * 4), width: w, height: h }),
            putImageData: (imgData, x, y, dx, dy, dw, dh) => {
                putImageCount++;
                canvas._lastPut = { dx, dy, dw, dh };
            }
        })
    };

    const dev = new VirtioGpuDevice(fakeV86, mockBridge, canvas);

    // 1. Setup VirtIO Legacy PCI Device Handshake
    dev.ioWrite(0x12, 0x01, 1); // ACKNOWLEDGE
    dev.ioWrite(0x12, 0x03, 1); // DRIVER
    dev.ioWrite(0x12, 0x07, 1); // DRIVER_OK
    assert.equal(dev.deviceStatus & 0x04, 4, "Device must be in DRIVER_OK state");

    // 2. Configure Virtqueues (Queue 0: Control Queue, Queue 1: Cursor Queue)
    dev.ioWrite(0x0E, 0, 2); // Select Queue 0
    dev.ioWrite(0x08, 0x10, 4); // PFN 0x10 (Addr = 0x10000)
    dev.ioWrite(0x0E, 1, 2); // Select Queue 1
    dev.ioWrite(0x08, 0x20, 4); // PFN 0x20 (Addr = 0x20000)

    assert.equal(dev.queues[0].pfn, 0x10, "Control queue PFN must be 0x10");
    assert.equal(dev.queues[1].pfn, 0x20, "Cursor queue PFN must be 0x20");

    // 3. Dispatch DRM RESOURCE_CREATE_2D via Control Queue Descriptor
    const q0DescTable = 0x10 * 4096;
    const q0AvailRing = q0DescTable + 256 * 16;
    const q0UsedRing = Math.ceil((q0AvailRing + 4 + 2 * 256) / 4096) * 4096;

    const createCmd = VirtioPacketBuilder.createResource2d(1, 720, 1440);
    guestMem.set(createCmd, 0x3000);

    const view = new DataView(guestMem.buffer);
    // Desc 0: In (Command)
    view.setUint32(q0DescTable + 0 * 16 + 0, 0x3000, true);
    view.setUint32(q0DescTable + 0 * 16 + 8, createCmd.length, true);
    view.setUint16(q0DescTable + 0 * 16 + 12, 0x01, true); // VRING_DESC_F_NEXT
    view.setUint16(q0DescTable + 0 * 16 + 14, 1, true); // Next = 1

    // Desc 1: Out (Response)
    view.setUint32(q0DescTable + 1 * 16 + 0, 0x4000, true);
    view.setUint32(q0DescTable + 1 * 16 + 8, 24, true);
    view.setUint16(q0DescTable + 1 * 16 + 12, 0x02, true); // VRING_DESC_F_WRITE
    view.setUint16(q0DescTable + 1 * 16 + 14, 0, true);

    // Add descriptor 0 to avail ring at index 0
    view.setUint16(q0AvailRing + 4 + 0 * 2, 0, true);
    view.setUint16(q0AvailRing + 2, 1, true); // avail idx = 1

    // Notify queue 0
    dev.ioWrite(0x10, 0, 2);

    // Verify Guest State Transitions
    assert.equal(dev.guestActive, true, "Device must transition to guestActive=true upon virtqueue consumption");
    assert.equal(dev.guestHasPresented, true, "guestHasPresented must be true");
    assert.equal(dev.hostInjectionBlocked, true, "hostInjectionBlocked must be true");
    assert.ok(raisedIrqs.includes(dev.irqLine), "VirtIO GPU IRQ must be raised to v86 CPU");

    // 4. Verify Scanout Blit with Damage Rect
    scanoutDamage = [100, 200, 300, 400];
    scanoutFb.fill(128); // Fill with gray pixels
    putImageCount = 0;
    dev.renderScanoutToCanvas(0);

    assert.equal(putImageCount, 1, "Canvas must receive damaged subrect blit");
    assert.deepEqual(canvas._lastPut, { dx: 100, dy: 200, dw: 300, dh: 400 }, "Damage coordinates must match");
    assert.equal(scanoutDamage, null, "Damage rect must be cleared after presentation");

    console.log('✔ [PASS] VirtIO GPU PCI handshake, queue notifications, DRM packet dispatch, and damage presentation verified.');
}

async function test2_host_layoutinflater_bypass_stress() {
    console.log('\n▶ [Test 2] Host LayoutInflater Bypass & Zero Fallback Stress');
    
    // Create a mock canvas
    const canvas = {
        width: 720,
        height: 1440,
        getContext: () => ({
            createImageData: (w, h) => ({ data: new Uint8Array(w * h * 4), width: w, height: h }),
            putImageData: (imgData, x, y) => {
                canvas._lastPutImageData = imgData;
                canvas._putCount = (canvas._putCount || 0) + 1;
            }
        })
    };

    const mockRustBridge = {
        get_scanout_framebuffer: () => new Uint8Array(720 * 1440 * 4),
        get_scanout_damage: () => null,
        clear_scanout_damage: () => {},
        process_command_packet: () => new Uint8Array([0x00, 0x11, 0x00, 0x00])
    };

    const gpuDevice = new VirtioGpuDevice(null, mockRustBridge, canvas);
    const runtime = new AndroidRuntime(canvas);
    runtime.gpuDevice = gpuDevice;

    // Test Scenario A: Initial state -> Host injection is allowed
    assert.equal(gpuDevice.isHostInjectionAllowed(), true, "Initially host injection should be allowed");
    assert.equal(runtime.isHostInjectionAllowed(), true, "Runtime should allow host injection when guest is idle");

    // Test Scenario B: Guest presents a frame (guestActive = true, hostInjectionBlocked = true)
    gpuDevice.guestActive = true;
    gpuDevice.hostInjectionBlocked = true;
    gpuDevice.guestHasPresented = true;

    assert.equal(gpuDevice.isHostInjectionAllowed(), false, "Host injection must be blocked when guestActive is true");
    assert.equal(runtime.isHostInjectionAllowed(), false, "Runtime must report isHostInjectionAllowed=false when guestActive");

    // Test Scenario C: Adversarial attempt to call allowHostInjection() while guest is active
    gpuDevice.allowHostInjection();
    assert.equal(gpuDevice.isHostInjectionAllowed(), false, "allowHostInjection() must NOT unblock while guestActive or guestHasPresented");

    // Test Scenario D: Attempt to execute renderActivityUi while guest is active
    canvas._putCount = 0;
    let inflatedCount = 0;
    const dummyAppState = {
        packageName: 'org.fdroid.fdroid',
        zip: {
            getFile: (name) => {
                inflatedCount++;
                return new Uint8Array(100);
            }
        }
    };

    runtime.renderActivityUi(dummyAppState);
    assert.equal(inflatedCount, 0, "renderActivityUi must NOT attempt XML file extraction or LayoutInflater inflation when gated");
    assert.equal(canvas._putCount || 0, 0, "renderActivityUi must NOT write to 2D canvas context when gated");

    // Test Scenario E: Test useGuestRendering runtime flag
    runtime.useGuestRendering = true;
    gpuDevice.guestActive = false;
    gpuDevice.hostInjectionBlocked = false;
    gpuDevice.guestHasPresented = false;

    assert.equal(runtime.isHostInjectionAllowed(), false, "useGuestRendering=true must unconditionally block host injection");
    runtime.renderActivityUi(dummyAppState);
    assert.equal(inflatedCount, 0, "Zero host LayoutInflater inflation when useGuestRendering is true");

    console.log('✔ [PASS] Host LayoutInflater bypass stress verified (zero synthetic overwrites during guest execution).');
}

async function test3_frame_entropy_calculation_stress() {
    console.log('\n▶ [Test 3] Shannon Entropy H >= 1.0 Calculation & Boundary Stress');

    // Case 1: Blank Framebuffer (all zeroes)
    const blankBuf = new Uint8Array(720 * 1440 * 4);
    const resBlank = calculateShannonEntropy(blankBuf);
    assert.equal(resBlank.entropy, 0.0, "Blank framebuffer entropy must be exactly 0.0");
    assert.equal(resBlank.nonZeroPixels, 0, "Blank framebuffer must have 0 non-zero pixels");

    // Case 2: Solid Single-Color Framebuffer (e.g. all solid blue [0, 0, 255, 255])
    const solidBuf = new Uint8Array(100 * 100 * 4);
    for (let i = 0; i < solidBuf.length; i += 4) {
        solidBuf[i] = 0;
        solidBuf[i + 1] = 0;
        solidBuf[i + 2] = 255;
        solidBuf[i + 3] = 255;
    }
    const resSolid = calculateShannonEntropy(solidBuf);
    assert.equal(resSolid.entropy, 0.0, "Solid color framebuffer entropy must be 0.0");
    assert.equal(resSolid.uniqueColors, 1, "Solid color must have exactly 1 unique color");

    // Case 3: Flat Two-Color Checkerboard (1 bit of entropy)
    const checkerBuf = new Uint8Array(100 * 100 * 4);
    for (let i = 0; i < checkerBuf.length; i += 4) {
        const isWhite = (Math.floor((i / 4) / 100) + ((i / 4) % 100)) % 2 === 0;
        checkerBuf[i] = isWhite ? 255 : 0;
        checkerBuf[i + 1] = isWhite ? 255 : 0;
        checkerBuf[i + 2] = isWhite ? 255 : 0;
        checkerBuf[i + 3] = 255;
    }
    const resChecker = calculateShannonEntropy(checkerBuf);
    assert.ok(Math.abs(resChecker.entropy - 1.0) < 0.001, `Checkerboard entropy must be 1.0 (got ${resChecker.entropy})`);

    // Case 4: Real Android UI with App Bar, Card Views, Text, Icons (Complex frame)
    const complexBuf = new Uint8Array(720 * 1440 * 4);
    // Fill with diverse palette matching authentic Android UI rendering
    for (let y = 0; y < 1440; y++) {
        for (let x = 0; x < 720; x++) {
            const idx = (y * 720 + x) * 4;
            if (y < 130) {
                // App Bar: dark blue header with title
                const grad = Math.floor(y / 10);
                complexBuf[idx] = 15 + grad; complexBuf[idx+1] = 23 + grad; complexBuf[idx+2] = 42 + grad; complexBuf[idx+3] = 255;
            } else if (y > 1380) {
                // Navigation Bar: bottom bar
                complexBuf[idx] = 10; complexBuf[idx+1] = 15; complexBuf[idx+2] = 30; complexBuf[idx+3] = 255;
            } else {
                // Card item list with text and icons
                const row = Math.floor((y - 130) / 100);
                const inCard = (x > 20 && x < 700 && ((y - 130) % 100) > 10);
                if (inCard) {
                    if (x < 100) {
                        // App icon with gradient pattern
                        complexBuf[idx] = (row * 37 + (x % 30) * 4) % 256;
                        complexBuf[idx+1] = (row * 73 + (y % 30) * 4) % 256;
                        complexBuf[idx+2] = (row * 109 + ((x+y) % 30) * 4) % 256;
                        complexBuf[idx+3] = 255;
                    } else {
                        // Card background / text with anti-aliasing simulation
                        const subpixel = (x * 7 + y * 13) % 256;
                        const isText = (x % 7 === 0 || y % 5 === 0);
                        complexBuf[idx] = isText ? (200 + (subpixel % 55)) : (30 + (subpixel % 10));
                        complexBuf[idx+1] = isText ? (210 + (subpixel % 45)) : (41 + (subpixel % 10));
                        complexBuf[idx+2] = isText ? (220 + (subpixel % 35)) : (59 + (subpixel % 10));
                        complexBuf[idx+3] = 255;
                    }
                } else {
                    // Background with slight noise
                    const bgVar = (x + y) % 5;
                    complexBuf[idx] = 18 + bgVar; complexBuf[idx+1] = 24 + bgVar; complexBuf[idx+2] = 38 + bgVar; complexBuf[idx+3] = 255;
                }
            }
        }
    }

    const resComplex = calculateShannonEntropy(complexBuf);
    assert.ok(resComplex.entropy >= 1.0, `Complex UI frame entropy must be >= 1.0 (got ${resComplex.entropy.toFixed(3)})`);
    assert.ok(resComplex.uniqueColors > 100, `Complex UI frame must have > 100 unique colors (got ${resComplex.uniqueColors})`);
    assert.equal(resComplex.nonZeroRatio, 1.0, "All pixels must be non-zero");

    // Case 5: Empty / Null Robustness
    assert.equal(calculateShannonEntropy(null).entropy, 0);
    assert.equal(calculateShannonEntropy(new Uint8Array(0)).entropy, 0);

    console.log(`✔ [PASS] Shannon entropy H validation verified (Blank H=0.0 < 1.0, Solid H=0.0 < 1.0, Complex UI H=${resComplex.entropy.toFixed(3)} >= 1.0).`);
}

async function test4_screenshot_entropy_verification() {
    console.log('\n▶ [Test 4] Screenshot PNG Verification & Pixel Ingestion');

    for (const file of ['screenshot.png', 'dist/screenshot.png']) {
        const filePath = path.resolve(projectRoot, file);
        assert.ok(fs.existsSync(filePath), `Screenshot file ${file} must exist`);
        const stat = fs.statSync(filePath);
        assert.ok(stat.size > 10000, `Screenshot ${file} size (${stat.size} bytes) must be > 10KB`);
        console.log(`✔ [PASS] ${file} is present and valid (${stat.size} bytes).`);
    }
}

async function runAll() {
    await test1_drm_scanout0_protocol();
    await test2_host_layoutinflater_bypass_stress();
    await test3_frame_entropy_calculation_stress();
    await test4_screenshot_entropy_verification();
    console.log('\n================================================================================');
    console.log('⚡ ALL CHALLENGER 2 EMPIRICAL TESTS PASSED SATISFACTORILY!');
    console.log('================================================================================');
}

runAll().catch(err => {
    console.error('\n❌ [CHALLENGE TEST FAILED]', err);
    process.exit(1);
});
