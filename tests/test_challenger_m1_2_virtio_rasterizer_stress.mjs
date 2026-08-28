/**
 * Empirical Stress Test Suite: VirtIO GPU DRM Virtqueues & Rasterizer Buffer Presentation
 * Challenger 2 for Milestone 1
 * 
 * Tests:
 * 1. VirtIO-GPU Virtqueue Ring Descriptors & Memory Safety (chained, looped, out-of-bounds, split in/out).
 * 2. VirtIO-GPU DRM Wire Commands (ResourceCreate2D, SetScanout, TransferToHost2D, ResourceFlush, Unref).
 * 3. Dirty Rectangle Clipping & Boundary Stress (subrects, negative coords, boundary overflows, zero dims).
 * 4. Software2DContext Rendering & Buffer Bounds (fillRect, clearRect, fillText, nested clip, state stack).
 * 5. End-to-End ViewHierarchyRasterizer -> VirtIO-GPU Presentation Pipeline.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';
import { Software2DContext, ViewHierarchyRasterizer, parseCssColor, FONT_5X7 } from '../src/view_rasterizer.js';
import { VirtioPacketBuilder, VIRTIO_GPU_CMD, VIRTIO_GPU_FORMAT } from '../src/virtio_packet_builder.js';
import { FrameLayout, LinearLayout, TextView, ImageView, MeasureSpec } from '../src/view_hierarchy.js';

let passed = 0;
let total = 0;

function check(desc, fn) {
    total++;
    try {
        fn();
        passed++;
        console.log(`  ✔ [PASS] ${desc}`);
    } catch (err) {
        console.error(`  ✖ [FAIL] ${desc}: ${err.message}`);
        throw err;
    }
}

console.log("================================================================================");
console.log("⚡ STARTING EMPIRICAL CHALLENGER M1: VIRTIO-GPU & RASTERIZER STRESS SUITE");
console.log("================================================================================\n");

// =============================================================================
// Section 1: VirtIO-GPU Virtqueue Ring Descriptors & Memory Safety
// =============================================================================
console.log("▶ Section 1: VirtIO-GPU Virtqueue Ring Descriptors & Memory Safety");

check("Virtqueue 0 handles valid multi-descriptor command and response chain", () => {
    // Mock 1MB guest memory buffer
    const guestMemBuffer = new ArrayBuffer(1024 * 1024);
    const guestMem = new Uint8Array(guestMemBuffer);
    const view = new DataView(guestMemBuffer);

    // Mock v86 instance
    const mockV86 = {
        cpu: {
            memory: { buffer: guestMemBuffer },
            devices: { pci: { register_device: () => true } },
            device_raise_irq: () => {}
        }
    };

    const device = new VirtioGpuDevice(mockV86, null, null);
    device.ioWrite(0x0E, 0, 2); // Select Queue 0
    device.ioWrite(0x08, 1, 4);  // Set PFN = 1 (Descriptor table at 4096 = 0x1000)

    const descTableAddr = 0x1000;
    const qSize = 256;
    const availRingAddr = descTableAddr + qSize * 16; // 0x2000
    const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * qSize) / 4096) * 4096; // 0x3000

    // Prepare ResourceCreate2D command packet at 0x5000
    const createPkt = VirtioPacketBuilder.createResource2d(10, 128, 128, VIRTIO_GPU_FORMAT.R8G8B8A8_UNORM, 100);
    guestMem.set(createPkt, 0x5000);

    // Desc 0: IN buffer at 0x5000 (len = createPkt.length), flags: NEXT, next = 1
    view.setUint32(descTableAddr + 0, 0x5000, true);
    view.setUint32(descTableAddr + 4, 0, true);
    view.setUint32(descTableAddr + 8, createPkt.length, true);
    view.setUint16(descTableAddr + 12, 0x01, true); // VRING_DESC_F_NEXT
    view.setUint16(descTableAddr + 14, 1, true);

    // Desc 1: OUT buffer at 0x6000 (len = 24), flags: WRITE, next = 0
    view.setUint32(descTableAddr + 16, 0x6000, true);
    view.setUint32(descTableAddr + 20, 0, true);
    view.setUint32(descTableAddr + 24, 24, true);
    view.setUint16(descTableAddr + 28, 0x02, true); // VRING_DESC_F_WRITE
    view.setUint16(descTableAddr + 30, 0, true);

    // Put head descriptor 0 into Avail ring slot 0
    view.setUint16(availRingAddr + 2, 1, true); // idx = 1
    view.setUint16(availRingAddr + 4, 0, true); // ring[0] = desc 0

    // Trigger queue processing
    device.ioWrite(0x10, 0, 2); // Notify Queue 0

    // Verify response written to 0x6000
    const respType = view.getUint32(0x6000, true);
    assert.equal(respType, 0x1100, "Response must be VIRTIO_GPU_RESP_OK_NODATA (0x1100)");
    const usedIdx = view.getUint16(usedRingAddr + 2, true);
    assert.equal(usedIdx, 1, "Used ring index must increment to 1");
    const usedHead = view.getUint32(usedRingAddr + 4, true);
    assert.equal(usedHead, 0, "Used ring must record head descriptor index 0");
    assert.equal(device.isrStatus & 0x01, 1, "ISR queue bit must be set");
});

check("Virtqueue descriptor parser detects and halts cyclic descriptor loops safely", () => {
    const guestMemBuffer = new ArrayBuffer(64 * 1024);
    const guestMem = new Uint8Array(guestMemBuffer);
    const view = new DataView(guestMemBuffer);

    const device = new VirtioGpuDevice(null, null, null);
    const descTableAddr = 0x1000;

    // Create a loop: Desc 0 -> Desc 1 -> Desc 0
    // Desc 0:
    view.setUint32(descTableAddr + 0, 0x2000, true);
    view.setUint32(descTableAddr + 4, 0, true);
    view.setUint32(descTableAddr + 8, 24, true);
    view.setUint16(descTableAddr + 12, 0x01, true); // NEXT
    view.setUint16(descTableAddr + 14, 1, true);

    // Desc 1:
    view.setUint32(descTableAddr + 16, 0x3000, true);
    view.setUint32(descTableAddr + 20, 0, true);
    view.setUint32(descTableAddr + 24, 24, true);
    view.setUint16(descTableAddr + 28, 0x01, true); // NEXT
    view.setUint16(descTableAddr + 30, 0, true); // loops back to 0

    // Should complete within 256 iterations without infinite hang
    const startTime = Date.now();
    const written = device.consumeDescriptorChainJs(guestMem, descTableAddr, 0);
    const elapsed = Date.now() - startTime;
    assert.ok(elapsed < 200, "Descriptor loop execution completed in bounded time");
});

check("Virtqueue descriptor parser rejects out-of-bounds guest memory descriptors", () => {
    const guestMemBuffer = new ArrayBuffer(16 * 1024);
    const guestMem = new Uint8Array(guestMemBuffer);
    const view = new DataView(guestMemBuffer);

    const device = new VirtioGpuDevice(null, null, null);
    const descTableAddr = 0x1000;

    // Desc 0 points to address 0x50000 with length 0x1000 (exceeds 16KB guest memory)
    view.setUint32(descTableAddr + 0, 0x50000, true);
    view.setUint32(descTableAddr + 4, 0, true);
    view.setUint32(descTableAddr + 8, 0x1000, true);
    view.setUint16(descTableAddr + 12, 0x00, true);
    view.setUint16(descTableAddr + 14, 0, true);

    // Should not throw or crash
    const written = device.consumeDescriptorChainJs(guestMem, descTableAddr, 0);
    assert.equal(written, 0, "No bytes written for out-of-bounds buffer");
});

check("Virtqueue descriptor parser handles split IN command buffers and split OUT response buffers", () => {
    const guestMemBuffer = new ArrayBuffer(64 * 1024);
    const guestMem = new Uint8Array(guestMemBuffer);
    const view = new DataView(guestMemBuffer);

    const device = new VirtioGpuDevice(null, null, null);
    const descTableAddr = 0x1000;

    const fullCreatePkt = VirtioPacketBuilder.createResource2d(55, 64, 64);
    const part1 = fullCreatePkt.subarray(0, 24); // header
    const part2 = fullCreatePkt.subarray(24);     // body

    guestMem.set(part1, 0x2000);
    guestMem.set(part2, 0x2100);

    // Desc 0: IN Part 1 (24 bytes at 0x2000)
    view.setUint32(descTableAddr + 0, 0x2000, true);
    view.setUint32(descTableAddr + 8, part1.length, true);
    view.setUint16(descTableAddr + 12, 0x01, true); // NEXT
    view.setUint16(descTableAddr + 14, 1, true);

    // Desc 1: IN Part 2 (16 bytes at 0x2100)
    view.setUint32(descTableAddr + 16, 0x2100, true);
    view.setUint32(descTableAddr + 24, part2.length, true);
    view.setUint16(descTableAddr + 28, 0x01, true); // NEXT
    view.setUint16(descTableAddr + 30, 2, true);

    // Desc 2: OUT Part 1 (12 bytes at 0x3000)
    view.setUint32(descTableAddr + 32, 0x3000, true);
    view.setUint32(descTableAddr + 40, 12, true);
    view.setUint16(descTableAddr + 44, 0x03, true); // NEXT | WRITE
    view.setUint16(descTableAddr + 46, 3, true);

    // Desc 3: OUT Part 2 (12 bytes at 0x3100)
    view.setUint32(descTableAddr + 48, 0x3100, true);
    view.setUint32(descTableAddr + 56, 12, true);
    view.setUint16(descTableAddr + 60, 0x02, true); // WRITE
    view.setUint16(descTableAddr + 62, 0, true);

    const written = device.consumeDescriptorChainJs(guestMem, descTableAddr, 0);
    assert.equal(written, 4, "Wrote response header across split output descriptors");
    assert.equal(guestMem[0x3000], 0x00);
    assert.equal(guestMem[0x3001], 0x11); // 0x1100
});

// =============================================================================
// Section 2: VirtIO-GPU Wire Protocol & DRM Command Execution
// =============================================================================
console.log("\n▶ Section 2: VirtIO-GPU Wire Protocol & DRM Command Execution");

check("VirtioPacketBuilder creates standard OASIS binary wire packets", () => {
    const createPkt = VirtioPacketBuilder.createResource2d(1, 800, 600, VIRTIO_GPU_FORMAT.R8G8B8A8_UNORM, 42);
    assert.equal(createPkt.length, 40);
    const view = new DataView(createPkt.buffer, createPkt.byteOffset, createPkt.byteLength);
    assert.equal(view.getUint32(0, true), VIRTIO_GPU_CMD.RESOURCE_CREATE_2D);
    assert.equal(view.getBigUint64(8, true), 42n);
    assert.equal(view.getUint32(24, true), 1);
    assert.equal(view.getUint32(28, true), VIRTIO_GPU_FORMAT.R8G8B8A8_UNORM);
    assert.equal(view.getUint32(32, true), 800);
    assert.equal(view.getUint32(36, true), 600);

    const scanoutPkt = VirtioPacketBuilder.setScanout(0, 1, 800, 600, 0, 0, 43);
    assert.equal(scanoutPkt.length, 48);

    const flushPkt = VirtioPacketBuilder.resourceFlush(1, 800, 600, 0, 0, 44);
    assert.equal(flushPkt.length, 48);

    const xferPkt = VirtioPacketBuilder.transferToHost2d(1, 10, 10, 0, 0, new Uint8Array(400), 45);
    assert.equal(xferPkt.length, 56 + 400);
});

check("VirtioGpuDevice handles control queue command processing without Rust bridge", () => {
    const device = new VirtioGpuDevice(null, null, null);
    const createPkt = VirtioPacketBuilder.createResource2d(10, 640, 480);
    const resp = device.processControlQueue(createPkt);
    assert.equal(resp.length, 4);
    assert.equal(resp[0], 0x00);
    assert.equal(resp[1], 0x11); // VIRTIO_GPU_RESP_OK_NODATA
});

// =============================================================================
// Section 3: Dirty Rectangle Updates & Boundary Clipping Stress
// =============================================================================
console.log("\n▶ Section 3: Dirty Rectangle Updates & Boundary Clipping Stress");

check("VirtioGpuDevice.renderScanoutToCanvas handles subrectangle damage updates", () => {
    let putImageDataCalls = [];
    const mockCtx = {
        createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
        putImageData: (imgData, dx, dy, dirtyX, dirtyY, dirtyW, dirtyH) => {
            putImageDataCalls.push({ dx, dy, dirtyX, dirtyY, dirtyW, dirtyH });
        }
    };

    const mockCanvas = { width: 800, height: 600, getContext: () => mockCtx };

    const mockRustBridge = {
        get_scanout_framebuffer: () => new Uint8Array(800 * 600 * 4),
        get_scanout_damage: () => [100, 150, 200, 100],
        clear_scanout_damage: () => {}
    };

    const device = new VirtioGpuDevice(null, mockRustBridge, mockCanvas);
    let scanoutUpdateNotified = null;
    device.onScanoutUpdate = (id, rect) => {
        scanoutUpdateNotified = { id, rect };
    };

    device.renderScanoutToCanvas(0);

    assert.equal(putImageDataCalls.length, 1);
    const call = putImageDataCalls[0];
    assert.equal(call.dirtyX, 100);
    assert.equal(call.dirtyY, 150);
    assert.equal(call.dirtyW, 200);
    assert.equal(call.dirtyH, 100);
    assert.deepEqual(scanoutUpdateNotified, { id: 0, rect: [100, 150, 200, 100] });
    assert.equal(device.damage_rects_count, 1);
});

check("VirtioGpuDevice.renderScanoutToCanvas clips overflowing damage rectangles safely", () => {
    let putImageDataCalls = [];
    const mockCtx = {
        createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
        putImageData: (imgData, dx, dy, dirtyX, dirtyY, dirtyW, dirtyH) => {
            putImageDataCalls.push({ dx, dy, dirtyX, dirtyY, dirtyW, dirtyH });
        }
    };

    const mockCanvas = { width: 400, height: 300, getContext: () => mockCtx };

    // Damage rect extends beyond canvas boundaries (dx=350, dw=100 -> reaches 450 > 400; dy=250, dh=100 -> reaches 350 > 300)
    const mockRustBridge = {
        get_scanout_framebuffer: () => new Uint8Array(400 * 300 * 4),
        get_scanout_damage: () => [350, 250, 100, 100],
        clear_scanout_damage: () => {}
    };

    const device = new VirtioGpuDevice(null, mockRustBridge, mockCanvas);
    device.renderScanoutToCanvas(0);

    assert.equal(putImageDataCalls.length, 1);
    const call = putImageDataCalls[0];
    assert.equal(call.dirtyX, 350);
    assert.equal(call.dirtyY, 250);
    assert.equal(call.dirtyW, 50, "Clipped subW must be 400 - 350 = 50");
    assert.equal(call.dirtyH, 50, "Clipped subH must be 300 - 250 = 50");
});

check("VirtioGpuDevice.renderScanoutToCanvas handles out-of-bounds damage rectangles with full fallback", () => {
    let putImageDataCalls = [];
    const mockCtx = {
        createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
        putImageData: (imgData, dx, dy, dirtyX, dirtyY, dirtyW, dirtyH) => {
            putImageDataCalls.push({ dx, dy, dirtyX, dirtyY, dirtyW, dirtyH });
        }
    };

    const mockCanvas = { width: 400, height: 300, getContext: () => mockCtx };

    // Damage rect completely outside canvas (dx=500, dy=400)
    const mockRustBridge = {
        get_scanout_framebuffer: () => new Uint8Array(400 * 300 * 4),
        get_scanout_damage: () => [500, 400, 100, 100],
        clear_scanout_damage: () => {}
    };

    const device = new VirtioGpuDevice(null, mockRustBridge, mockCanvas);
    device.renderScanoutToCanvas(0);

    assert.equal(putImageDataCalls.length, 1);
    const call = putImageDataCalls[0];
    assert.equal(call.dirtyX, undefined, "Full blit has no subrect parameters");
    assert.equal(call.dx, 0);
    assert.equal(call.dy, 0);
});

// =============================================================================
// Section 4: Software2DContext Rendering & Memory Bounds
// =============================================================================
console.log("\n▶ Section 4: Software2DContext Rendering & Memory Bounds");

check("parseCssColor parses all hex, rgb, rgba, and named colors accurately", () => {
    assert.deepEqual(parseCssColor('#FFF'), [255, 255, 255, 255]);
    assert.deepEqual(parseCssColor('#0000'), [0, 0, 0, 0]);
    assert.deepEqual(parseCssColor('#123456'), [0x12, 0x34, 0x56, 255]);
    assert.deepEqual(parseCssColor('#12345678'), [0x12, 0x34, 0x56, 0x78]);
    assert.deepEqual(parseCssColor('rgb(10, 20, 30)'), [10, 20, 30, 255]);
    assert.deepEqual(parseCssColor('rgba(10, 20, 30, 0.5)'), [10, 20, 30, 128]);
    assert.deepEqual(parseCssColor('transparent'), [0, 0, 0, 0]);
    assert.deepEqual(parseCssColor('white'), [255, 255, 255, 255]);
    assert.deepEqual(parseCssColor('black'), [0, 0, 0, 255]);
    assert.deepEqual(parseCssColor(null), [0, 0, 0, 255]);
    assert.deepEqual(parseCssColor('invalid_garbage'), [0, 0, 0, 255]);
});

check("Software2DContext.fillRect handles negative, partial, and overflowing coordinates safely", () => {
    const width = 50;
    const height = 50;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);

    ctx.fillStyle = '#ff0000';

    // 1. Fully outside (negative)
    ctx.fillRect(-100, -100, 50, 50);

    // 2. Fully outside (positive)
    ctx.fillRect(100, 100, 50, 50);

    // 3. Overlapping left-top edge: (-10, -10, 20, 20) -> should draw in [0..10, 0..10]
    ctx.fillRect(-10, -10, 20, 20);

    // 4. Overlapping right-bottom edge: (40, 40, 30, 30) -> should draw in [40..50, 40..50]
    ctx.fillStyle = '#00ff00';
    ctx.fillRect(40, 40, 30, 30);

    // Verify inside bounds
    assert.equal(buf[0], 255, "Top-left red pixel at (0,0)");
    assert.equal(buf[1], 0);
    assert.equal(buf[2], 0);
    assert.equal(buf[3], 255);

    const botRightIdx = ((49 * width) + 49) * 4;
    assert.equal(buf[botRightIdx], 0, "Bottom-right green pixel at (49,49)");
    assert.equal(buf[botRightIdx + 1], 255);
    assert.equal(buf[botRightIdx + 2], 0);
    assert.equal(buf[botRightIdx + 3], 255);

    // Verify unaffected center pixel remains 0
    const centerIdx = ((25 * width) + 25) * 4;
    assert.equal(buf[centerIdx + 3], 0, "Center pixel unaffected");
});

check("Software2DContext nested clipping stacks and translation bounds", () => {
    const width = 100;
    const height = 100;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);

    ctx.save();
    ctx.translate(10, 10);
    ctx.beginPath();
    ctx.rect(0, 0, 30, 30); // Clip [10, 10, 40, 40]
    ctx.clip();

    ctx.save();
    ctx.translate(5, 5);
    ctx.beginPath();
    ctx.rect(0, 0, 20, 20); // Nested Clip [15, 15, 35, 35]
    ctx.clip();

    ctx.fillStyle = '#0000ff';
    ctx.fillRect(-50, -50, 200, 200); // Attempt to fill whole canvas

    ctx.restore(); // Restores to clip [10, 10, 40, 40]
    ctx.restore(); // Restores to unclipped

    // Verify only [15..35, 15..35] received blue pixels
    const insideIdx = ((20 * width) + 20) * 4;
    assert.equal(buf[insideIdx + 2], 255, "Inside nested clip is blue");

    const outsideIdx = ((12 * width) + 12) * 4;
    assert.equal(buf[outsideIdx + 3], 0, "Outside nested clip is untouched");
});

check("Software2DContext state stack handles deep recursion (100 levels) without corruption", () => {
    const width = 20;
    const height = 20;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);

    for (let i = 0; i < 100; i++) {
        ctx.save();
        ctx.translate(1, 1);
        ctx.globalAlpha *= 0.99;
    }

    assert.equal(ctx.transX, 100);
    assert.equal(ctx.transY, 100);

    for (let i = 0; i < 100; i++) {
        ctx.restore();
    }

    assert.equal(ctx.transX, 0);
    assert.equal(ctx.transY, 0);
    assert.equal(ctx.globalAlpha, 1.0);
});

check("Software2DContext.fillText handles text boundaries, alignments, and maxW truncation", () => {
    const width = 100;
    const height = 40;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);

    ctx.font = "14px monospace";
    ctx.fillStyle = "#ffffff";

    // 1. Text align center
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText("OK", 50, 20);

    // Verify pixels plotted around center (x=44..56, y=14..26)
    let centerPixels = 0;
    for (let y = 14; y < 26; y++) {
        for (let x = 44; x < 56; x++) {
            if (buf[(y * width + x) * 4 + 3] === 255) centerPixels++;
        }
    }
    assert.ok(centerPixels > 10, "Centered text plotted active pixels");

    // 2. Truncation via maxW
    const bufTrunc = new Uint8Array(width * height * 4);
    const ctxTrunc = new Software2DContext(bufTrunc, width, height);
    ctxTrunc.font = "14px monospace";
    ctxTrunc.fillStyle = "#ffffff";
    ctxTrunc.fillText("ABCDEFGHIJKLMN", 0, 10, 24); // Only allow 24px width (~2 chars)

    let maxX = 0;
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            if (bufTrunc[(y * width + x) * 4 + 3] === 255) {
                maxX = Math.max(maxX, x);
            }
        }
    }
    assert.ok(maxX <= 25, `Max X coordinate was ${maxX}, within 24px limit`);
});

// =============================================================================
// Section 5: End-to-End ViewHierarchyRasterizer to VirtIO-GPU Pipeline
// =============================================================================
console.log("\n▶ Section 5: End-to-End ViewHierarchyRasterizer to VirtIO-GPU Pipeline");

check("ViewHierarchyRasterizer rasterizes dynamic View tree and submits to VirtIO-GPU", () => {
    const rasterizer = new ViewHierarchyRasterizer(320, 240);

    // Construct authentic Android View hierarchy
    const root = new FrameLayout();
    root.backgroundColor = "#1e293b";

    const layout = new LinearLayout(LinearLayout.VERTICAL);

    const title = new TextView();
    title.setText("F-Droid Store");
    title.textSize = 16;
    title.textColor = "#f8fafc";

    const subtitle = new TextView();
    subtitle.setText("Open Source Catalog");
    subtitle.textSize = 12;
    subtitle.textColor = "#94a3b8";

    layout.addView(title);
    layout.addView(subtitle);
    root.addView(layout);

    // Rasterize
    const result = rasterizer.rasterize(root, 320, 240);
    assert.equal(result.width, 320);
    assert.equal(result.height, 240);
    assert.equal(result.rgbaData.length, 320 * 240 * 4);

    // Check background color (#1e293b -> R:30, G:41, B:59)
    assert.equal(result.rgbaData[0], 30);
    assert.equal(result.rgbaData[1], 41);
    assert.equal(result.rgbaData[2], 59);

    // Verify non-zero entropy and character glyph pixels
    let nonBgCount = 0;
    for (let i = 0; i < result.rgbaData.length; i += 4) {
        if (result.rgbaData[i] !== 30 || result.rgbaData[i+1] !== 41 || result.rgbaData[i+2] !== 59) {
            nonBgCount++;
        }
    }
    assert.ok(nonBgCount > 50, `Rendered ${nonBgCount} glyph pixels for text views`);

    // Submit to mock VirtIO-GPU device
    let processedPackets = [];
    const mockGpuDevice = {
        processControlQueue: (pkt) => {
            processedPackets.push(pkt);
            return new Uint8Array([0x00, 0x11, 0x00, 0x00]);
        }
    };

    rasterizer.submitToVirtioGpu(mockGpuDevice, 101, 0, result.rgbaData);
    assert.equal(processedPackets.length, 2, "Submitted TransferToHost2D and ResourceFlush packets");

    const viewXfer = new DataView(processedPackets[0].buffer, processedPackets[0].byteOffset, processedPackets[0].byteLength);
    assert.equal(viewXfer.getUint32(0, true), VIRTIO_GPU_CMD.TRANSFER_TO_HOST_2D);
    assert.equal(viewXfer.getUint32(48, true), 101, "Resource ID 101");

    const viewFlush = new DataView(processedPackets[1].buffer, processedPackets[1].byteOffset, processedPackets[1].byteLength);
    assert.equal(viewFlush.getUint32(0, true), VIRTIO_GPU_CMD.RESOURCE_FLUSH);
    assert.equal(viewFlush.getUint32(40, true), 101, "Resource ID 101");
});

console.log("\n================================================================================");
console.log(`📊 SUMMARY: ${passed}/${total} Empirical Stress Tests Passed (0 Failed)`);
console.log("================================================================================");
