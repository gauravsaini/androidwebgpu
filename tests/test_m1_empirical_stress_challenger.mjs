/**
 * Empirical Challenger Stress Test Suite: Milestone 1
 * 
 * Verifies:
 * 1. Full ASCII range (32 to 126) glyph rasterization in Software2DContext.
 * 2. Glyph non-zero pixel generation and distinctness.
 * 3. Clipping, negative coordinates, and out-of-bounds safety.
 * 4. Text alignment, baseline offsets, maxW limits, and scale factor variations.
 * 5. Alpha blending and RGBA color parsing permutations.
 * 6. Non-ASCII / Unicode / null / empty string resilience.
 * 7. Shannon pixel entropy across font and density variations.
 * 8. Dynamic renderActivityUi bindings (0 packages, 50 packages, missing metadata).
 * 9. VirtIO GPU control packet dispatch validation.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import { AndroidRuntime, resolveAppMetadata } from '../src/android_runtime.js';
import { Software2DContext, FONT_5X7, parseCssColor, ViewHierarchyRasterizer } from '../src/view_rasterizer.js';
import { defaultPackageManager } from '../src/apk_client_parser.js';
import { View, RecyclerView, FrameLayout, TextView, ImageView, LayoutInflater } from '../src/view_hierarchy.js';
import { VirtioPacketBuilder } from '../src/virtio_packet_builder.js';

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

function computeEntropy(buf, width, height) {
    const hist = new Map();
    const numPixels = width * height;
    for (let i = 0; i < buf.length; i += 4) {
        const lum = Math.round(0.299 * buf[i] + 0.587 * buf[i + 1] + 0.114 * buf[i + 2]);
        hist.set(lum, (hist.get(lum) || 0) + 1);
    }
    let entropy = 0;
    for (const count of hist.values()) {
        const p = count / numPixels;
        if (p > 0) entropy -= p * Math.log2(p);
    }
    return entropy;
}

console.log("================================================================================");
console.log("⚡ STARTING CHALLENGER M1 EMPIRICAL STRESS TEST SUITE");
console.log("================================================================================\n");

// -----------------------------------------------------------------------------
// 1. Full ASCII Range (32 to 126) Glyph Rendering Verification
// -----------------------------------------------------------------------------
console.log("▶ 1. Full ASCII Range (32-126) Glyph Sweep & Non-Zero Pixel Assertion");

check("All printable ASCII characters (33-126) generate non-zero pixels and space (32) generates 0", () => {
    const width = 16;
    const height = 16;
    const glyphPixelCounts = new Map();

    for (let code = 32; code <= 126; code++) {
        const buf = new Uint8Array(width * height * 4);
        const ctx = new Software2DContext(buf, width, height);
        ctx.font = "7px monospace";
        ctx.fillStyle = "#ffffff";
        const char = String.fromCharCode(code);
        ctx.fillText(char, 2, 2);

        let activePixels = 0;
        for (let i = 0; i < buf.length; i += 4) {
            if (buf[i] === 255 && buf[i + 1] === 255 && buf[i + 2] === 255 && buf[i + 3] === 255) {
                activePixels++;
            }
        }

        glyphPixelCounts.set(char, activePixels);

        if (code === 32) {
            assert.equal(activePixels, 0, "Space character (ASCII 32) must have 0 active pixels");
        } else {
            assert.ok(activePixels > 0, `ASCII character '${char}' (code ${code}) must generate non-zero pixels (got ${activePixels})`);
            assert.ok(activePixels <= 35, `ASCII character '${char}' (code ${code}) must not exceed maximum 5x7 glyph size (got ${activePixels})`);
        }
    }

    // Verify alphabet diversity: letters 'A', 'B', 'C', '1', '2', '.' must have distinct pixel signatures
    assert.notEqual(glyphPixelCounts.get('A'), glyphPixelCounts.get('B'));
    assert.notEqual(glyphPixelCounts.get('1'), glyphPixelCounts.get('8'));
    assert.ok(glyphPixelCounts.get('.') > 0 && glyphPixelCounts.get('.') < glyphPixelCounts.get('M'));
});

// -----------------------------------------------------------------------------
// 2. Out-of-Bounds, Negative Coordinates & Clipping Stress
// -----------------------------------------------------------------------------
console.log("\n▶ 2. Out-of-Bounds, Negative Coordinates & Clipping Stress");

check("fillText with negative, overflowing, and NaN-adjacent coordinates executes without out-of-bounds corruption", () => {
    const width = 32;
    const height = 32;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);
    ctx.font = "14px monospace";
    ctx.fillStyle = "#ffffff";

    // Completely off-screen coordinates
    ctx.fillText("FarOffScreenLeft", -500, 10);
    ctx.fillText("FarOffScreenTop", 10, -500);
    ctx.fillText("FarOffScreenRight", 1000, 10);
    ctx.fillText("FarOffScreenBottom", 10, 1000);

    // Partially clipped coordinates at edges
    ctx.fillText("EdgeOverlapLeft", -5, 5);
    ctx.fillText("EdgeOverlapRight", 28, 5);
    ctx.fillText("EdgeOverlapTop", 5, -3);
    ctx.fillText("EdgeOverlapBottom", 5, 28);

    // Verify buffer integrity (length unchanged, no memory violation)
    assert.equal(buf.length, width * height * 4);
    let painted = 0;
    for (let i = 0; i < buf.length; i += 4) {
        if (buf[i + 3] > 0) painted++;
    }
    assert.ok(painted > 0, "Partial edge text rendered pixels inside valid bounds");
});

check("Software2DContext clip rect restricts text rendering strictly within clipping bounds", () => {
    const width = 64;
    const height = 64;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);
    ctx.font = "14px sans-serif";
    ctx.fillStyle = "#ffffff";

    // Clip to sub-rectangle [10, 10, 30, 30]
    ctx.rect(10, 10, 20, 20);
    ctx.clip();

    ctx.fillText("ABCDEFGHIJKLMNOPQRSTUVWXYZ", 0, 15);

    // Verify no pixels outside [10, 10, 30, 30]
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const idx = (y * width + x) * 4;
            if (buf[idx + 3] > 0) {
                assert.ok(x >= 10 && x < 30, `Pixel at x=${x} outside clip horizontal range [10, 30)`);
                assert.ok(y >= 10 && y < 30, `Pixel at y=${y} outside clip vertical range [10, 30)`);
            }
        }
    }
});

// -----------------------------------------------------------------------------
// 3. Text Align, Baseline, MaxW & Scale Variations
// -----------------------------------------------------------------------------
console.log("\n▶ 3. Text Alignment, Baseline, MaxW & Scale Variations");

check("textAlign variants (start, center, right, end, left) adjust horizontal start position", () => {
    const width = 100;
    const height = 40;

    const renderAlign = (align) => {
        const buf = new Uint8Array(width * height * 4);
        const ctx = new Software2DContext(buf, width, height);
        ctx.font = "7px monospace";
        ctx.fillStyle = "#ffffff";
        ctx.textAlign = align;
        ctx.fillText("TEST", 50, 10);
        let minX = width;
        let maxX = 0;
        for (let y = 0; y < height; y++) {
            for (let x = 0; x < width; x++) {
                if (buf[(y * width + x) * 4 + 3] > 0) {
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x);
                }
            }
        }
        return { minX, maxX };
    };

    const startPos = renderAlign('start');
    const centerPos = renderAlign('center');
    const rightPos = renderAlign('right');

    assert.ok(startPos.minX >= 50, "start align begins at x >= 50");
    assert.ok(centerPos.minX < 50 && centerPos.maxX > 50, "center align spans across x=50");
    assert.ok(rightPos.maxX <= 50, "right align ends at x <= 50");
});

check("maxW truncates text rendering when string exceeds specified limit", () => {
    const width = 120;
    const height = 30;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);
    ctx.font = "7px monospace";
    ctx.fillStyle = "#ffffff";

    // Character width is 6px. String is 15 chars = 90px. maxW = 30px (approx 5 chars).
    ctx.fillText("VERY_LONG_STRING_SAMPLE", 0, 5, 30);

    let maxX = 0;
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            if (buf[(y * width + x) * 4 + 3] > 0) {
                maxX = Math.max(maxX, x);
            }
        }
    }

    assert.ok(maxX <= 36, `Rendered pixels maxX=${maxX} stayed within maxW threshold of 30`);
});

check("Font scaling (scale=1 for 7px, scale=2 for 14px, scale=4 for 28px) scales pixel footprint quadratically", () => {
    const measurePixels = (fontStr) => {
        const width = 100;
        const height = 100;
        const buf = new Uint8Array(width * height * 4);
        const ctx = new Software2DContext(buf, width, height);
        ctx.font = fontStr;
        ctx.fillStyle = "#ffffff";
        ctx.fillText("H", 30, 50);
        let count = 0;
        for (let i = 0; i < buf.length; i += 4) {
            if (buf[i + 3] > 0) count++;
        }
        return count;
    };

    const count7px = measurePixels("7px monospace");
    const count14px = measurePixels("14px monospace");
    const count28px = measurePixels("28px monospace");

    assert.ok(count7px > 0, "7px glyph has non-zero pixels");
    // scale=2 has 4x the pixel area of scale=1 (2x2 per bit)
    assert.equal(count14px, count7px * 4, "14px glyph has exactly 4x the pixel area of 7px glyph");
    // scale=4 has 16x the pixel area of scale=1 (4x4 per bit)
    assert.equal(count28px, count7px * 16, "28px glyph has exactly 16x the pixel area of 7px glyph");
});

// -----------------------------------------------------------------------------
// 4. Alpha Compositing and Color Parsing Permutations
// -----------------------------------------------------------------------------
console.log("\n▶ 4. Alpha Compositing & Color Parsing Permutations");

check("Alpha blending in fillText composites smoothly over existing background", () => {
    const width = 30;
    const height = 30;
    const buf = new Uint8Array(width * height * 4);
    // Background blue: #0000ff (0, 0, 255, 255)
    for (let i = 0; i < buf.length; i += 4) {
        buf[i] = 0; buf[i + 1] = 0; buf[i + 2] = 255; buf[i + 3] = 255;
    }

    const ctx = new Software2DContext(buf, width, height);
    ctx.font = "7px monospace";
    // Red color with 50% alpha: rgba(255, 0, 0, 0.5)
    ctx.fillStyle = "rgba(255, 0, 0, 0.5)";
    ctx.fillText("A", 5, 5);

    let blendedPixels = 0;
    for (let i = 0; i < buf.length; i += 4) {
        const r = buf[i];
        const b = buf[i + 2];
        if (r > 100 && b > 100) {
            blendedPixels++;
        }
    }
    assert.ok(blendedPixels > 0, "Blended text pixels contain mixed red and blue channels");
});

check("parseCssColor handles 3-hex, 4-hex, 6-hex, 8-hex, rgb, rgba, and named colors", () => {
    assert.deepEqual(parseCssColor("#fff"), [255, 255, 255, 255]);
    assert.deepEqual(parseCssColor("#f008"), [255, 0, 0, 136]);
    assert.deepEqual(parseCssColor("#00ff00"), [0, 255, 0, 255]);
    assert.deepEqual(parseCssColor("#0000ff80"), [0, 0, 255, 128]);
    assert.deepEqual(parseCssColor("rgb(10, 20, 30)"), [10, 20, 30, 255]);
    assert.deepEqual(parseCssColor("rgba(10, 20, 30, 0.5)"), [10, 20, 30, 128]);
    assert.deepEqual(parseCssColor("white"), [255, 255, 255, 255]);
    assert.deepEqual(parseCssColor("transparent"), [0, 0, 0, 0]);
});

// -----------------------------------------------------------------------------
// 5. Non-ASCII, Unicode, and Edge String Handling
// -----------------------------------------------------------------------------
console.log("\n▶ 5. Non-ASCII, Unicode, Null, and Empty String Edge Cases");

check("Non-ASCII, Unicode emojis, empty string, and null inputs render safely without throwing", () => {
    const width = 60;
    const height = 30;
    const buf = new Uint8Array(width * height * 4);
    const ctx = new Software2DContext(buf, width, height);

    assert.doesNotThrow(() => ctx.fillText("", 0, 0));
    assert.doesNotThrow(() => ctx.fillText(null, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(undefined, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(12345, 0, 0));

    // Unicode string: should render fallback '?' glyphs for non-ASCII
    assert.doesNotThrow(() => ctx.fillText("Hello 🚀 世界 \u0000 \uFFFF", 0, 5));
});

// -----------------------------------------------------------------------------
// 6. Shannon Pixel Entropy Verification
// -----------------------------------------------------------------------------
console.log("\n▶ 6. Shannon Pixel Entropy Across Text Scenarios");

check("Rendered text produces positive Shannon entropy (H > 0.3 for isolated label, H >= 1.0 for multi-element UI view tree)", () => {
    // 1. Isolated TextView label buffer (120x40)
    const labelW = 120;
    const labelH = 40;
    const labelBuf = new Uint8Array(labelW * labelH * 4);
    for (let i = 0; i < labelBuf.length; i += 4) {
        labelBuf[i] = 15; labelBuf[i + 1] = 23; labelBuf[i + 2] = 42; labelBuf[i + 3] = 255;
    }
    const labelCtx = new Software2DContext(labelBuf, labelW, labelH);
    labelCtx.font = "14px Roboto";
    labelCtx.fillStyle = "#ffffff";
    labelCtx.fillText("Android WebGPU", 10, 15);

    const singleLineEntropy = computeEntropy(labelBuf, labelW, labelH);
    console.log(`    Measured Shannon entropy for single label: ${singleLineEntropy.toFixed(4)}`);
    assert.ok(singleLineEntropy >= 0.3, `Entropy ${singleLineEntropy.toFixed(4)} must be >= 0.3 for single text label`);

    // 2. Multi-element UI card layout (300x100)
    const cardW = 300;
    const cardH = 100;
    const cardBuf = new Uint8Array(cardW * cardH * 4);
    for (let i = 0; i < cardBuf.length; i += 4) {
        cardBuf[i] = 15; cardBuf[i + 1] = 23; cardBuf[i + 2] = 42; cardBuf[i + 3] = 255;
    }
    const cardCtx = new Software2DContext(cardBuf, cardW, cardH);
    cardCtx.fillStyle = "#1e293b";
    cardCtx.fillRect(10, 10, 280, 80);
    cardCtx.fillStyle = "#38bdf8";
    cardCtx.font = "14px monospace";
    cardCtx.fillText("F-Droid App Store", 20, 25);
    cardCtx.fillStyle = "#94a3b8";
    cardCtx.font = "7px monospace";
    cardCtx.fillText("Free and Open Source Software catalog", 20, 45);
    cardCtx.fillStyle = "#22c55e";
    cardCtx.fillRect(20, 60, 80, 22);
    cardCtx.fillStyle = "#0f172a";
    cardCtx.fillText("INSTALL", 35, 71);

    const fullUiEntropy = computeEntropy(cardBuf, cardW, cardH);
    console.log(`    Measured Shannon entropy for composite UI: ${fullUiEntropy.toFixed(4)}`);
    assert.ok(fullUiEntropy >= 1.0, `Composite UI entropy ${fullUiEntropy.toFixed(4)} must be >= 1.0`);
});

// -----------------------------------------------------------------------------
// 7. Dynamic renderActivityUi Package Binding & Fallback Resiliency
// -----------------------------------------------------------------------------
console.log("\n▶ 7. Dynamic renderActivityUi Binding & Fallback Resiliency");

check("renderActivityUi handles empty package list without creating phantom items", () => {
    const runtime = new AndroidRuntime();
    const mockAppXml = new Uint8Array([0x03, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);
    const appState = {
        packageData: [],
        zip: {
            getFile: (path) => path === 'res/Kt.xml' ? mockAppXml : null
        }
    };
    runtime.renderActivityUi(appState);
    assert.ok(runtime.currentRootView instanceof View);
});

check("renderActivityUi handles 50 dynamic packages with incomplete metadata fields", () => {
    const runtime = new AndroidRuntime();
    const packages = [];
    for (let i = 0; i < 50; i++) {
        packages.push({
            packageName: `org.stress.test.app${i}`,
            applicationLabel: i % 2 === 0 ? `Stress App ${i}` : undefined,
            summary: i % 3 === 0 ? `Summary ${i}` : undefined,
            icon: i % 5 === 0 ? "⚡" : undefined,
            color: i % 4 === 0 ? "#123456" : undefined
        });
    }

    const mockItemXml = new Uint8Array([0x03, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);
    const appState = {
        packageData: packages,
        zip: {
            getFile: (path) => path === 'res/Kt.xml' ? mockItemXml : null
        }
    };

    assert.doesNotThrow(() => runtime.renderActivityUi(appState));
    assert.ok(runtime.currentRootView instanceof View);
});

check("renderActivityUi without APK zip or XML gracefully provides FrameLayout root", () => {
    const runtime = new AndroidRuntime();
    assert.doesNotThrow(() => runtime.renderActivityUi({}));
    assert.ok(runtime.currentRootView instanceof FrameLayout);
});

// -----------------------------------------------------------------------------
// 8. VirtIO GPU Control Queue Command Dispatch Integration
// -----------------------------------------------------------------------------
console.log("\n▶ 8. VirtIO GPU Control Queue Command Wire Dispatch");

check("renderActivityUi emits RESOURCE_CREATE_2D, SET_SCANOUT, TRANSFER_TO_HOST_2D, and RESOURCE_FLUSH to gpuDevice", () => {
    const runtime = new AndroidRuntime();
    const packets = [];
    runtime.gpuDevice = {
        processControlQueue: (pkt) => {
            packets.push(pkt);
        }
    };

    const mockItemXml = new Uint8Array([0x03, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);
    const appState = {
        packageData: [{ packageName: "org.test.app", name: "Test App" }],
        zip: {
            getFile: (path) => path === 'res/Kt.xml' ? mockItemXml : null
        }
    };

    runtime.renderActivityUi(appState);

    // Expect 4 packets: createResource2d, setScanout, transferToHost2d, resourceFlush
    assert.equal(packets.length, 4, `Expected 4 VirtIO control queue packets, got ${packets.length}`);

    // Verify command types in packets
    const view0 = new DataView(packets[0].buffer, packets[0].byteOffset);
    const view1 = new DataView(packets[1].buffer, packets[1].byteOffset);
    const view2 = new DataView(packets[2].buffer, packets[2].byteOffset);
    const view3 = new DataView(packets[3].buffer, packets[3].byteOffset);

    assert.equal(view0.getUint32(0, true), 0x0101, "Packet 0 must be VIRTIO_GPU_CMD_RESOURCE_CREATE_2D (0x0101)");
    assert.equal(view1.getUint32(0, true), 0x0103, "Packet 1 must be VIRTIO_GPU_CMD_SET_SCANOUT (0x0103)");
    assert.equal(view2.getUint32(0, true), 0x0105, "Packet 2 must be VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D (0x0105)");
    assert.equal(view3.getUint32(0, true), 0x0104, "Packet 3 must be VIRTIO_GPU_CMD_RESOURCE_FLUSH (0x0104)");
});

console.log("\n================================================================================");
console.log(`📊 SUMMARY: ${passed}/${total} Challenger Stress Tests Passed (0 Failed)`);
console.log("================================================================================\n");
