/**
 * Challenger M1-2 Empirical Adversarial Test Harness:
 * VirtIO GPU Wire Protocol & Layout Submission Pipeline
 * 
 * Verifies:
 * 1. ViewHierarchyRasterizer.submitToVirtioGpu builds valid TRANSFER_TO_HOST_2D and RESOURCE_FLUSH binary packets.
 * 2. Field-level conformance to OASIS VirtIO 1.2 GPU Specification.
 * 3. Correct dispatch to VirtioGpuDevice control queue and Rust WASM bridge.
 * 4. Error isolation, boundary values, and zero-synthetic invariants.
 * 
 * Conforms to ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import { ViewHierarchyRasterizer, Software2DContext, parseCssColor } from '../src/view_rasterizer.js';
import { VirtioGpuDevice, VIRTIO_GPU_F_VIRGL, VIRTIO_GPU_F_EDID } from '../src/virtio_gpu_device.js';
import { VirtioPacketBuilder, VIRTIO_GPU_CMD, VIRTIO_GPU_FORMAT } from '../src/virtio_packet_builder.js';
import { FrameLayout, TextView, Button, LayoutParams, MATCH_PARENT, WRAP_CONTENT } from '../src/view_hierarchy.js';

let passedCount = 0;
let failedCount = 0;
const testResults = [];

function test(name, fn) {
    try {
        fn();
        passedCount++;
        testResults.push({ name, passed: true });
        console.log(`  ✔ [PASS] ${name}`);
    } catch (err) {
        failedCount++;
        testResults.push({ name, passed: false, error: err.message });
        console.error(`  ✖ [FAIL] ${name}: ${err.message}`);
    }
}

async function asyncTest(name, fn) {
    try {
        await fn();
        passedCount++;
        testResults.push({ name, passed: true });
        console.log(`  ✔ [PASS] ${name}`);
    } catch (err) {
        failedCount++;
        testResults.push({ name, passed: false, error: err.message });
        console.error(`  ✖ [FAIL] ${name}: ${err.message}`);
    }
}

console.log("================================================================================");
console.log("🔥 CHALLENGER M1-2 EMPIRICAL TEST: VIRTIO-GPU & LAYOUT SUBMISSION PIPELINE 🔥");
console.log("================================================================================\n");

// -----------------------------------------------------------------------------
// Suite 1: submitToVirtioGpu Packet Structure & Wire Protocol Encoding
// -----------------------------------------------------------------------------
console.log("▶ [Suite 1] Binary Wire Protocol Encoding for TRANSFER_TO_HOST_2D & RESOURCE_FLUSH");

test("1.1: submitToVirtioGpu dispatches exactly 2 packets in sequence (TRANSFER then FLUSH)", () => {
    const rasterizer = new ViewHierarchyRasterizer(640, 480);
    const capturedPackets = [];
    const mockDevice = {
        processControlQueue(pkt) {
            capturedPackets.push(new Uint8Array(pkt));
            return new Uint8Array([0x00, 0x11, 0x00, 0x00]);
        }
    };

    rasterizer.submitToVirtioGpu(mockDevice, 101, 0);

    assert.equal(capturedPackets.length, 2, "Must dispatch exactly 2 packets");

    const transferPkt = capturedPackets[0];
    const flushPkt = capturedPackets[1];

    const viewTransfer = new DataView(transferPkt.buffer, transferPkt.byteOffset, transferPkt.byteLength);
    const transferType = viewTransfer.getUint32(0, true);
    assert.equal(transferType, VIRTIO_GPU_CMD.TRANSFER_TO_HOST_2D, "First packet must be TRANSFER_TO_HOST_2D (0x0105)");

    const viewFlush = new DataView(flushPkt.buffer, flushPkt.byteOffset, flushPkt.byteLength);
    const flushType = viewFlush.getUint32(0, true);
    assert.equal(flushType, VIRTIO_GPU_CMD.RESOURCE_FLUSH, "Second packet must be RESOURCE_FLUSH (0x0104)");
});

test("1.2: TRANSFER_TO_HOST_2D packet conforms strictly to OASIS VirtIO 1.2 layout", () => {
    const w = 320;
    const h = 240;
    const resId = 55;
    const rasterizer = new ViewHierarchyRasterizer(w, h);
    
    // Fill test pixel data
    for (let i = 0; i < rasterizer.rgbaData.length; i++) {
        rasterizer.rgbaData[i] = (i * 7) & 0xFF;
    }

    const captured = [];
    const mockDevice = {
        processControlQueue: (pkt) => captured.push(pkt)
    };

    rasterizer.submitToVirtioGpu(mockDevice, resId, 0);
    const pkt = captured[0];
    const view = new DataView(pkt.buffer, pkt.byteOffset, pkt.byteLength);

    // Virtio Header (24 bytes)
    assert.equal(view.getUint32(0, true), 0x0105, "type == TRANSFER_TO_HOST_2D");
    assert.equal(view.getUint32(4, true), 0, "flags == 0");
    assert.equal(view.getBigUint64(8, true), 3n, "fenceId == 3");
    assert.equal(view.getUint32(16, true), 0, "ctxId == 0");
    assert.equal(view.getUint32(20, true), 0, "padding == 0");

    // Command Body (32 bytes)
    assert.equal(view.getUint32(24, true), 0, "rect.x == 0");
    assert.equal(view.getUint32(28, true), 0, "rect.y == 0");
    assert.equal(view.getUint32(32, true), w, `rect.width == ${w}`);
    assert.equal(view.getUint32(36, true), h, `rect.height == ${h}`);
    assert.equal(view.getBigUint64(40, true), 0n, "offset == 0");
    assert.equal(view.getUint32(48, true), resId, `resourceId == ${resId}`);
    assert.equal(view.getUint32(52, true), 0, "padding2 == 0");

    // Payload (pixel bytes)
    const expectedPayloadSize = w * h * 4;
    assert.equal(pkt.byteLength, 56 + expectedPayloadSize, `Total size == 56 + ${expectedPayloadSize}`);
    const payload = pkt.subarray(56);
    assert.deepEqual(payload, rasterizer.rgbaData, "Payload bytes must match rasterizer RGBA buffer");
});

test("1.3: RESOURCE_FLUSH packet conforms strictly to OASIS VirtIO 1.2 layout", () => {
    const w = 800;
    const h = 600;
    const resId = 77;
    const rasterizer = new ViewHierarchyRasterizer(w, h);

    const captured = [];
    const mockDevice = {
        processControlQueue: (pkt) => captured.push(pkt)
    };

    rasterizer.submitToVirtioGpu(mockDevice, resId, 0);
    const pkt = captured[1];
    const view = new DataView(pkt.buffer, pkt.byteOffset, pkt.byteLength);

    // Virtio Header (24 bytes)
    assert.equal(view.getUint32(0, true), 0x0104, "type == RESOURCE_FLUSH");
    assert.equal(view.getUint32(4, true), 0, "flags == 0");
    assert.equal(view.getBigUint64(8, true), 4n, "fenceId == 4");
    assert.equal(view.getUint32(16, true), 0, "ctxId == 0");
    assert.equal(view.getUint32(20, true), 0, "padding == 0");

    // Command Body (24 bytes)
    assert.equal(view.getUint32(24, true), 0, "rect.x == 0");
    assert.equal(view.getUint32(28, true), 0, "rect.y == 0");
    assert.equal(view.getUint32(32, true), w, `rect.width == ${w}`);
    assert.equal(view.getUint32(36, true), h, `rect.height == ${h}`);
    assert.equal(view.getUint32(40, true), resId, `resourceId == ${resId}`);
    assert.equal(view.getUint32(44, true), 0, "padding2 == 0");
    assert.equal(pkt.byteLength, 48, "Flush packet size must be exactly 48 bytes");
});

test("1.4: Custom buffer override in submitToVirtioGpu takes precedence over this.rgbaData", () => {
    const rasterizer = new ViewHierarchyRasterizer(100, 100);
    const customBuffer = new Uint8Array(100 * 100 * 4).fill(0xAA);

    const captured = [];
    const mockDevice = {
        processControlQueue: (pkt) => captured.push(pkt)
    };

    rasterizer.submitToVirtioGpu(mockDevice, 200, 0, customBuffer);
    const transferPkt = captured[0];
    const payload = transferPkt.subarray(56);
    assert.equal(payload[0], 0xAA, "Custom buffer bytes must be serialized");
    assert.equal(payload[payload.length - 1], 0xAA, "Custom buffer bytes must be serialized");
});

test("1.5: Null/undefined device in submitToVirtioGpu fails gracefully without throwing", () => {
    const rasterizer = new ViewHierarchyRasterizer(100, 100);
    assert.doesNotThrow(() => {
        rasterizer.submitToVirtioGpu(null, 100, 0);
        rasterizer.submitToVirtioGpu(undefined, 100, 0);
    });
});

// -----------------------------------------------------------------------------
// Suite 2: Integration with VirtioGpuDevice and Rust WASM Bridge
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 2] Integration with VirtioGpuDevice and Rust WASM Bridge");

test("2.1: VirtioGpuDevice processControlQueue routes packets and triggers scanout presentation", () => {
    let mockRustBridgeProcessed = [];
    let scanoutRenderCalled = false;

    const mockRustBridge = {
        process_command_packet(buf) {
            mockRustBridgeProcessed.push(buf);
            return new Uint8Array([0x00, 0x11, 0x00, 0x00]); // RESP_OK_NODATA
        },
        get_scanout_framebuffer(scanoutId) {
            return new Uint8Array(640 * 480 * 4).fill(128);
        },
        get_scanout_damage(scanoutId) {
            return [0, 0, 640, 480];
        },
        clear_scanout_damage(scanoutId) {}
    };

    const mockCanvas = {
        width: 640,
        height: 480,
        getContext(type) {
            return {
                createImageData: (w, h) => ({ data: new Uint8Array(w * h * 4), width: w, height: h }),
                putImageData: () => { scanoutRenderCalled = true; }
            };
        }
    };

    const gpuDev = new VirtioGpuDevice(null, mockRustBridge, mockCanvas);
    const rasterizer = new ViewHierarchyRasterizer(640, 480);

    rasterizer.submitToVirtioGpu(gpuDev, 100, 0);

    assert.equal(mockRustBridgeProcessed.length, 2, "Rust bridge must receive both packets");
    assert.equal(scanoutRenderCalled, true, "Scanout presentation must be triggered on canvas");
});

test("2.2: End-to-End View hierarchy rasterization into VirtioGpuDevice", () => {
    // Create authentic View hierarchy
    const root = new FrameLayout(new LayoutParams(MATCH_PARENT, MATCH_PARENT));
    root.id = 1;
    root.backgroundColor = '#1e293b';

    const tv = new TextView("Authentic View Title", new LayoutParams(WRAP_CONTENT, WRAP_CONTENT));
    tv.id = 2;
    tv.textColor = '#ffffff';
    root.addView(tv);

    const btn = new Button(new LayoutParams(200, 50));
    btn.id = 3;
    btn.setText("Submit");
    root.addView(btn);

    const rasterizer = new ViewHierarchyRasterizer(1280, 720);
    const result = rasterizer.rasterize(root);

    assert.equal(result.width, 1280);
    assert.equal(result.height, 720);
    assert.equal(result.rgbaData.length, 1280 * 720 * 4);

    // Verify button drew on top at (0, 0) with MD3 primary color #6750A4 (103, 80, 164, 255)
    assert.equal(result.rgbaData[0], 103, "Button Red channel match #6750A4");
    assert.equal(result.rgbaData[1], 80, "Button Green channel match #6750A4");
    assert.equal(result.rgbaData[2], 164, "Button Blue channel match #6750A4");

    // Verify background pixel outside button at (500, 500) has FrameLayout background #1e293b (30, 41, 59, 255)
    const bgIdx = (500 * 1280 + 500) * 4;
    assert.equal(result.rgbaData[bgIdx], 30, "Background Red channel match #1e293b");
    assert.equal(result.rgbaData[bgIdx + 1], 41, "Background Green channel match #1e293b");
    assert.equal(result.rgbaData[bgIdx + 2], 59, "Background Blue channel match #1e293b");
    assert.equal(result.rgbaData[bgIdx + 3], 255, "Background Alpha channel match 255");

    let totalTransferBytes = 0;
    const mockDevice = {
        processControlQueue(pkt) {
            totalTransferBytes += pkt.length;
            return new Uint8Array([0x00, 0x11, 0x00, 0x00]);
        }
    };

    rasterizer.submitToVirtioGpu(mockDevice, 100, 0, result.rgbaData);
    assert.equal(totalTransferBytes, (56 + 1280 * 720 * 4) + 48, "Exact byte count match for 1280x720 2D transfer + flush");
});

// -----------------------------------------------------------------------------
// Suite 3: Adversarial Boundary & Stress Cases
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 3] Adversarial Boundary & Stress Cases");

test("3.1: Zero-size dimensions handling in rasterizer and submission", () => {
    const rasterizer = new ViewHierarchyRasterizer(0, 0);
    const result = rasterizer.rasterize(null, 0, 0);
    assert.equal(result.rgbaData.length, 0);

    const captured = [];
    const mockDevice = { processControlQueue: (pkt) => captured.push(pkt) };

    rasterizer.submitToVirtioGpu(mockDevice, 1, 0, result.rgbaData);
    assert.equal(captured.length, 2);
    assert.equal(captured[0].length, 56, "Zero-pixel transfer packet is 56 bytes header");
    assert.equal(captured[1].length, 48, "Flush packet is 48 bytes header");
});

test("3.2: 4K UHD Resolution submission (3840x2160)", () => {
    const w = 3840;
    const h = 2160;
    const rasterizer = new ViewHierarchyRasterizer(w, h);
    assert.equal(rasterizer.rgbaData.length, w * h * 4); // ~33.17 MB

    let receivedTransferLen = 0;
    let receivedFlushLen = 0;
    const mockDevice = {
        processControlQueue(pkt) {
            if (pkt.length > 48) receivedTransferLen = pkt.length;
            else receivedFlushLen = pkt.length;
        }
    };

    rasterizer.submitToVirtioGpu(mockDevice, 500, 0);
    assert.equal(receivedTransferLen, 56 + w * h * 4, "4K UHD Transfer length matches");
    assert.equal(receivedFlushLen, 48, "4K UHD Flush length matches");
});

test("3.3: High-frequency rapid layout submissions (1,000 passes)", () => {
    const rasterizer = new ViewHierarchyRasterizer(320, 240);
    const mockDevice = {
        count: 0,
        processControlQueue(pkt) {
            this.count++;
        }
    };

    for (let i = 0; i < 1000; i++) {
        rasterizer.submitToVirtioGpu(mockDevice, 100 + (i % 10), 0);
    }

    assert.equal(mockDevice.count, 2000, "2,000 virtio packets processed across 1,000 submission passes");
});

test("3.4: Software2DContext Porter-Duff alpha blending accuracy", () => {
    const buf = new Uint8Array(4 * 4 * 4); // 4x4 RGBA
    // Fill base with opaque blue (0, 0, 255, 255)
    for (let i = 0; i < buf.length; i += 4) {
        buf[i] = 0;
        buf[i + 1] = 0;
        buf[i + 2] = 255;
        buf[i + 3] = 255;
    }

    const ctx = new Software2DContext(buf, 4, 4);
    // Draw 50% transparent red (255, 0, 0, 0.5)
    ctx.fillStyle = 'rgba(255, 0, 0, 0.5)';
    ctx.fillRect(0, 0, 4, 4);

    // Expected: sr=255, sa=0.5; dr=0, db=255, da=1.0
    // alpha = (128 / 255) ~ 0.50196; invA ~ 0.49804
    // outR = Math.round(255 * (128/255)) = 128
    // outG = 0
    // outB = Math.round(255 * (127/255)) = 127
    assert.equal(buf[0], 128, "Blended Red channel == 128");
    assert.equal(buf[1], 0, "Blended Green channel == 0");
    assert.equal(buf[2], 127, "Blended Blue channel == 127");
    assert.equal(buf[3], 255, "Blended Alpha channel == 255");
});

// -----------------------------------------------------------------------------
// Summary
// -----------------------------------------------------------------------------
console.log("\n================================================================================");
console.log(`📊 EXECUTION SUMMARY: ${passedCount}/${passedCount + failedCount} Tests Passed (${failedCount} Failed)`);
console.log("================================================================================");

if (failedCount > 0) {
    process.exit(1);
}
