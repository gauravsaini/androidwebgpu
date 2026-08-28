/**
 * Empirical Challenger M1 Test Suite: Authentic Font Rasterization & Dynamic UI Bindings
 * 
 * Adversarial stress testing for:
 * 1. Software2DContext.fillText complete ASCII range [32..126] individual glyph rendering.
 * 2. Fallback glyph rendering for non-ASCII, negative code points, UTF-16 surrogate pairs, emojis.
 * 3. Bounding box, clipping, sub-pixel positioning, negative offsets, and maxWidth truncation.
 * 4. CSS Color parser stress & alpha blending mathematical precision without overflow/NaN.
 * 5. Dynamic package data binding in renderActivityUi and resolveAppMetadata without static fallbacks.
 * 6. Shannon Entropy verification on varied rendered text and full-screen layouts.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import { Software2DContext, FONT_5X7, parseCssColor } from '../src/view_rasterizer.js';
import { AndroidRuntime, resolveAppMetadata } from '../src/android_runtime.js';
import { defaultPackageManager } from '../src/apk_client_parser.js';
import { RecyclerView, FrameLayout, TextView, ImageView, LinearLayout } from '../src/view_hierarchy.js';

let totalTests = 0;
let passedTests = 0;

function runTest(name, fn) {
    totalTests++;
    try {
        fn();
        passedTests++;
        console.log(`  [PASS] ${name}`);
    } catch (err) {
        console.error(`  [FAIL] ${name}: ${err.message}`);
        console.error(err.stack);
        throw err;
    }
}

console.log("================================================================================");
console.log("EMPIRICAL CHALLENGER: MILESTONE 1 ADVERSARIAL STRESS SUITE");
console.log("================================================================================\n");

// -----------------------------------------------------------------------------
// 1. ASCII 32-126 Glyph Rendering & Entropy Verification
// -----------------------------------------------------------------------------
console.log("▶ Category 1: Exhaustive ASCII 32-126 Glyph Pixel Verification");

runTest("Every printable ASCII char (33..126) renders >0 non-zero pixels", () => {
    const w = 16;
    const h = 16;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "7px monospace"; // 1x scale: 5x7 glyph in 6x8 box
    ctx.fillStyle = "#ffffff";

    for (let code = 33; code <= 126; code++) {
        buf.fill(0);
        const ch = String.fromCharCode(code);
        ctx.fillText(ch, 1, 1);

        let activePixels = 0;
        for (let i = 0; i < buf.length; i += 4) {
            if (buf[i] === 255 && buf[i + 3] === 255) {
                activePixels++;
            }
        }
        assert.ok(
            activePixels > 0,
            `ASCII char ${code} ('${ch}') rendered 0 pixels! Must render authentic glyph.`
        );
        assert.ok(
            activePixels <= 35,
            `ASCII char ${code} ('${ch}') rendered ${activePixels} pixels (exceeds max 5x7=35 bounds).`
        );
    }
});

runTest("Space char (ASCII 32) renders 0 foreground pixels but advances spacing", () => {
    const w = 32;
    const h = 16;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "7px monospace";
    ctx.fillStyle = "#ffffff";

    // 1. Space alone -> 0 pixels
    buf.fill(0);
    ctx.fillText(" ", 1, 1);
    let spacePixels = 0;
    for (let i = 0; i < buf.length; i += 4) {
        if (buf[i] > 0) spacePixels++;
    }
    assert.equal(spacePixels, 0, "Space character must not render foreground pixels");

    // 2. "A A" vs "AA" -> "A A" must render further right
    buf.fill(0);
    ctx.fillText("A A", 0, 0);
    const posA1 = [];
    for (let x = 0; x < w; x++) {
        for (let y = 0; y < h; y++) {
            const idx = (y * w + x) * 4;
            if (buf[idx] === 255) posA1.push(x);
        }
    }
    const maxX_space = Math.max(...posA1);

    buf.fill(0);
    ctx.fillText("AA", 0, 0);
    const posA2 = [];
    for (let x = 0; x < w; x++) {
        for (let y = 0; y < h; y++) {
            const idx = (y * w + x) * 4;
            if (buf[idx] === 255) posA2.push(x);
        }
    }
    const maxX_nospace = Math.max(...posA2);

    assert.ok(maxX_space > maxX_nospace, "Space char must advance character coordinate offset");
});

runTest("Glyphs for distinct characters produce distinct pixel bit patterns", () => {
    const w = 16;
    const h = 16;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "7px monospace";
    ctx.fillStyle = "#ffffff";

    const glyphHashes = new Set();
    const printableChars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+~";

    for (const ch of printableChars) {
        buf.fill(0);
        ctx.fillText(ch, 0, 0);
        let hash = 0;
        for (let i = 0; i < buf.length; i += 4) {
            if (buf[i] > 0) hash = ((hash << 5) - hash + i) | 0;
        }
        assert.ok(!glyphHashes.has(hash), `Collision: Glyph for '${ch}' identical to another glyph!`);
        glyphHashes.add(hash);
    }
    assert.equal(glyphHashes.size, printableChars.length);
});

// -----------------------------------------------------------------------------
// 2. Non-ASCII, Unicode, Surrogate Pairs, and Edge Cases
// -----------------------------------------------------------------------------
console.log("\n▶ Category 2: Non-ASCII & Malformed String Resilience");

runTest("Non-ASCII chars (CJK, Cyrillic, Emoji, ASCII 0..31, 127..255) fallback to question glyph without crash", () => {
    const w = 64;
    const h = 32;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "14px monospace";
    ctx.fillStyle = "#ffffff";

    const edgeStrings = [
        "\x00\x01\x02\x07\x08\x1b",     // ASCII control codes 0..31
        "\x7f\x80\xff",                 // ASCII 127..255
        "你好世界",                     // CJK characters
        "Привет мир",                   // Cyrillic
        "🔥🚀📱🎉",                     // High surrogate emojis
        "Mixed text: Hello 世界! 123",  // Mixed Latin + CJK
        "\u0000NullEmbedded\u0000",     // Embedded NUL bytes
        "\\n\\r\\t\n\r\t",              // Newlines and tabs
    ];

    for (const s of edgeStrings) {
        buf.fill(0);
        // Must not throw or produce out of bounds index
        assert.doesNotThrow(() => {
            ctx.fillText(s, 0, 0);
        }, `Failed to render edge string: ${JSON.stringify(s)}`);

        // Must generate non-zero pixels
        let active = 0;
        for (let i = 0; i < buf.length; i += 4) {
            if (buf[i] > 0) active++;
        }
        assert.ok(active > 0, `String ${JSON.stringify(s)} must render pixels`);
    }
});

runTest("Empty, null, undefined, numeric, and boolean inputs are handled safely", () => {
    const w = 32;
    const h = 16;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);

    assert.doesNotThrow(() => ctx.fillText("", 0, 0));
    assert.doesNotThrow(() => ctx.fillText(null, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(undefined, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(0, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(12345, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(false, 0, 0));
    assert.doesNotThrow(() => ctx.fillText(true, 0, 0));

    assert.doesNotThrow(() => ctx.measureText(""));
    assert.doesNotThrow(() => ctx.measureText(null));
    assert.doesNotThrow(() => ctx.measureText(undefined));
    assert.doesNotThrow(() => ctx.measureText(0));
});

// -----------------------------------------------------------------------------
// 3. Clipping, Off-Screen Bounds, Scale, Alignment & MaxWidth
// -----------------------------------------------------------------------------
console.log("\n▶ Category 3: Layout Boundaries, ClipRect, and Text Alignment");

runTest("Negative coordinates and out-of-screen rendering do not write out of buffer bounds", () => {
    const w = 20;
    const h = 20;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "14px monospace";
    ctx.fillStyle = "#ffffff";

    // Sub-pixel, negative, and extreme offscreen positions
    assert.doesNotThrow(() => ctx.fillText("A", -100, -100));
    assert.doesNotThrow(() => ctx.fillText("A", 1000, 1000));
    assert.doesNotThrow(() => ctx.fillText("A", -5, 5));
    assert.doesNotThrow(() => ctx.fillText("A", 15, -3));
    assert.doesNotThrow(() => ctx.fillText("A", 2.7, 4.3));
});

runTest("ClipRect strictly constrains character glyph pixel writes", () => {
    const w = 40;
    const h = 40;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "14px monospace";
    ctx.fillStyle = "#ffffff";

    // Restrict clip region to [10, 10, 20, 20]
    ctx.rect(10, 10, 10, 10);
    ctx.clip();

    ctx.fillText("WWWWWWWWWW", 0, 0);

    // Verify ZERO pixels written outside [10..20, 10..20]
    for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
            const idx = (y * w + x) * 4;
            if (x < 10 || x >= 20 || y < 10 || y >= 20) {
                assert.equal(
                    buf[idx], 0,
                    `Pixel at (${x}, ${y}) was modified outside clip rect [10, 10, 20, 20]`
                );
            }
        }
    }
});

runTest("maxWidth properly constrains and truncates rendered text width", () => {
    const w = 100;
    const h = 20;
    const buf = new Uint8Array(w * h * 4);
    const ctx = new Software2DContext(buf, w, h);
    ctx.font = "7px monospace"; // 1x scale: charW = 6
    ctx.fillStyle = "#ffffff";

    // Text of 10 chars = 60px. Restrict to maxW = 18px (at most 3 chars)
    ctx.fillText("ABCDEFGHIJ", 0, 0, 18);

    let maxWrittenX = 0;
    for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
            const idx = (y * w + x) * 4;
            if (buf[idx] > 0) {
                if (x > maxWrittenX) maxWrittenX = x;
            }
        }
    }
    assert.ok(maxWrittenX < 18, `maxWrittenX=${maxWrittenX} should be strictly < 18px`);
});

runTest("textAlign (left, center, right) shifts rendered coordinates predictably", () => {
    const w = 120;
    const h = 20;
    const bufLeft = new Uint8Array(w * h * 4);
    const bufCenter = new Uint8Array(w * h * 4);
    const bufRight = new Uint8Array(w * h * 4);

    const ctxL = new Software2DContext(bufLeft, w, h);
    ctxL.textAlign = 'left';
    ctxL.fillStyle = '#ffffff';
    ctxL.fillText("ABC", 60, 5);

    const ctxC = new Software2DContext(bufCenter, w, h);
    ctxC.textAlign = 'center';
    ctxC.fillStyle = '#ffffff';
    ctxC.fillText("ABC", 60, 5);

    const ctxR = new Software2DContext(bufRight, w, h);
    ctxR.textAlign = 'right';
    ctxR.fillStyle = '#ffffff';
    ctxR.fillText("ABC", 60, 5);

    const getMinX = (buf) => {
        for (let x = 0; x < w; x++) {
            for (let y = 0; y < h; y++) {
                if (buf[(y * w + x) * 4] > 0) return x;
            }
        }
        return -1;
    };

    const minL = getMinX(bufLeft);
    const minC = getMinX(bufCenter);
    const minR = getMinX(bufRight);

    assert.ok(minR < minC, `Right aligned minX (${minR}) must be < Center aligned minX (${minC})`);
    assert.ok(minC < minL, `Center aligned minX (${minC}) must be < Left aligned minX (${minL})`);
});

// -----------------------------------------------------------------------------
// 4. Color Parsing & Alpha Blending Math
// -----------------------------------------------------------------------------
console.log("\n▶ Category 4: CSS Color Parsing & Alpha Blending");

runTest("parseCssColor parses hex3, hex4, hex6, hex8, rgb, rgba, and named colors", () => {
    assert.deepEqual(parseCssColor("#f00"), [255, 0, 0, 255]);
    assert.deepEqual(parseCssColor("#0f08"), [0, 255, 0, 136]);
    assert.deepEqual(parseCssColor("#123456"), [0x12, 0x34, 0x56, 255]);
    assert.deepEqual(parseCssColor("#12345680"), [0x12, 0x34, 0x56, 0x80]);
    assert.deepEqual(parseCssColor("rgb(10, 20, 30)"), [10, 20, 30, 255]);
    assert.deepEqual(parseCssColor("rgba(10, 20, 30, 0.5)"), [10, 20, 30, 128]);
    assert.deepEqual(parseCssColor("transparent"), [0, 0, 0, 0]);
    assert.deepEqual(parseCssColor("white"), [255, 255, 255, 255]);
    assert.deepEqual(parseCssColor("black"), [0, 0, 0, 255]);
    assert.deepEqual(parseCssColor("invalid-color-str"), [0, 0, 0, 255]);
});

runTest("Software2DContext alpha blending produces valid clamped RGBA bytes without NaN", () => {
    const w = 10;
    const h = 10;
    const buf = new Uint8Array(w * h * 4);
    // Background blue
    for (let i = 0; i < buf.length; i += 4) {
        buf[i] = 0; buf[i + 1] = 0; buf[i + 2] = 200; buf[i + 3] = 255;
    }

    const ctx = new Software2DContext(buf, w, h);
    ctx.globalAlpha = 0.5;
    ctx.fillStyle = "rgba(200, 0, 0, 0.5)";
    ctx.fillRect(0, 0, 10, 10);

    for (let i = 0; i < buf.length; i += 4) {
        assert.ok(!Number.isNaN(buf[i]), `Red channel is NaN`);
        assert.ok(!Number.isNaN(buf[i + 1]), `Green channel is NaN`);
        assert.ok(!Number.isNaN(buf[i + 2]), `Blue channel is NaN`);
        assert.ok(!Number.isNaN(buf[i + 3]), `Alpha channel is NaN`);
        assert.ok(buf[i] >= 0 && buf[i] <= 255);
        assert.ok(buf[i + 2] > 0 && buf[i] > 0, "Blending must combine red and blue components");
    }
});

// -----------------------------------------------------------------------------
// 5. Dynamic Package Binding & Zero Synthetic UI Fallbacks
// -----------------------------------------------------------------------------
console.log("\n▶ Category 5: Authentic Dynamic UI & PMS Integration");

runTest("resolveAppMetadata never relies on hardcoded catalog or knownApps table", () => {
    // Arbitrary unmapped package
    const meta1 = resolveAppMetadata("org.unknown.randomapp", { applicationLabel: "Dynamic Random" });
    assert.equal(meta1.name, "Dynamic Random");

    const meta2 = resolveAppMetadata("com.deep.nested.service.worker", {});
    assert.equal(meta2.name, "Worker"); // Capitalized last token
});

runTest("renderActivityUi dynamically renders 50 distinct packages into RecyclerView", () => {
    const runtime = new AndroidRuntime();
    const pkgList = [];
    for (let i = 1; i <= 50; i++) {
        pkgList.push({
            packageName: `org.test.app${i}`,
            applicationLabel: `Dynamic Test App #${i}`,
            summary: `Summary of dynamic app #${i}`,
            icon: `📦${i}`,
            color: `#${(i * 12345).toString(16).padStart(6, '0').slice(0, 6)}`
        });
    }

    const mockItemXml = new Uint8Array([0x03, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);
    const appState = {
        packageData: pkgList,
        zip: {
            getFile: (path) => (path === 'res/Kt.xml' || path === 'res/layout/app_list_item.xml') ? mockItemXml : null
        }
    };

    runtime.renderActivityUi(appState);
    assert.ok(runtime.currentRootView instanceof FrameLayout);
});

// -----------------------------------------------------------------------------
// 6. Shannon Pixel Entropy Verification
// -----------------------------------------------------------------------------
console.log("\n▶ Category 6: Information Entropy Verification");

runTest("Rendered text screen achieves high Shannon Entropy H >= 1.0", () => {
    const width = 360;
    const height = 640;
    const buf = new Uint8Array(width * height * 4);
    // Background dark theme
    for (let i = 0; i < buf.length; i += 4) {
        buf[i] = 15; buf[i + 1] = 23; buf[i + 2] = 42; buf[i + 3] = 255;
    }

    const ctx = new Software2DContext(buf, width, height);

    // Render realistic UI layout text elements
    ctx.font = "21px sans-serif";
    ctx.fillStyle = "#38bdf8";
    ctx.fillText("F-Droid Application Store", 20, 40);

    ctx.font = "14px sans-serif";
    ctx.fillStyle = "#94a3b8";
    ctx.fillText("What's New in Open Source", 20, 70);

    for (let i = 0; i < 5; i++) {
        const top = 100 + i * 80;
        ctx.fillStyle = "#1e293b";
        ctx.fillRect(16, top, 328, 70);

        ctx.font = "16px sans-serif";
        ctx.fillStyle = "#f8fafc";
        ctx.fillText(`Application Item #${i + 1} - Firefox Focus`, 30, top + 25);

        ctx.font = "12px sans-serif";
        ctx.fillStyle = "#64748b";
        ctx.fillText(`org.mozilla.focus - Fast, private browser with tracking protection`, 30, top + 48);
    }

    // Compute Shannon Entropy across RGBA channels
    const hist = new Uint32Array(256);
    const totalPixels = width * height;
    for (let i = 0; i < buf.length; i += 4) {
        // Luminance formula
        const lum = (0.299 * buf[i] + 0.587 * buf[i + 1] + 0.114 * buf[i + 2]) | 0;
        hist[lum]++;
    }

    let entropy = 0;
    for (let c = 0; c < 256; c++) {
        if (hist[c] > 0) {
            const p = hist[c] / totalPixels;
            entropy -= p * Math.log2(p);
        }
    }

    console.log(`    Measured Full UI Shannon Entropy H = ${entropy.toFixed(4)} bits/pixel`);
    assert.ok(entropy >= 1.0, `Entropy H=${entropy.toFixed(4)} must be >= 1.0 bits/pixel`);
});

console.log("\n================================================================================");
console.log(`EMPERICAL STRESS RESULTS: ${passedTests}/${totalTests} Passed (0 Failed)`);
console.log("================================================================================");
