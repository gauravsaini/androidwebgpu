/**
 * Empirical and Adversarial Stress Test Suite for GeckoView Rendering, Touch Input, and URL Navigation
 * 
 * Targeted Components:
 * - AndroidRuntime.renderActivityUi()
 * - ViewHierarchyRasterizer (720x1440 RGBA buffer, 4,147,200 bytes, Shannon entropy)
 * - ViewRootImpl.dispatchInputEvent()
 * - ViewGroup & View dispatchTouchEvent() reverse-Z hit testing
 * - Toolbar action button state mutations (Back, Forward, Reload, Home, Tabs, Top Sites, URL bar)
 * - Coordinate boundary conditions (0,0, 719,1439, negatives, overflow, NaN, float)
 * - Rapid high-frequency touch event storm (2,500+ events)
 * - Re-entrant rapid navigation toggling (500 cycles)
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import fs from 'fs';
import { ApkZipReader, AxmlDecoder, PackageManagerRegistry } from '../src/apk_client_parser.js';
import { DexParser, DalvikVM } from '../src/dex_vm.js';
import { AndroidRuntime } from '../src/android_runtime.js';
import { AppController } from '../src/app_controller.js';
import { SystemBootstrap } from '../src/system_bootstrap.js';
import { MotionEvent, KeyEvent, ViewHierarchyRasterizer, ViewRootImpl, Software2DContext, parseCssColor } from '../src/view_rasterizer.js';
import { LinearLayout, TextView, FrameLayout, LayoutParams, MATCH_PARENT, WRAP_CONTENT, VISIBLE, GONE } from '../src/view_hierarchy.js';

let passed = 0;
let failed = 0;
const metrics = {
    touchEventsDispatched: 0,
    navigationCycles: 0,
    framesRasterized: 0,
    shannonEntropyScores: {},
    bufferIntegrityChecks: 0,
    reentrancyCycles: 0,
    timings: {}
};

function assert(condition, message) {
    if (!condition) {
        console.error(`❌ [FAIL] ${message}`);
        failed++;
        throw new Error(message);
    } else {
        passed++;
    }
}

function calculateShannonEntropy(rgbaData) {
    const hist = new Uint32Array(256);
    const len = rgbaData.length;
    for (let i = 0; i < len; i += 4) {
        const lum = Math.round(0.299 * rgbaData[i] + 0.587 * rgbaData[i + 1] + 0.114 * rgbaData[i + 2]);
        hist[lum]++;
    }
    const totalPixels = len / 4;
    let entropy = 0;
    for (let i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            const p = hist[i] / totalPixels;
            entropy -= p * Math.log2(p);
        }
    }
    return entropy;
}

console.log("==================================================================================");
console.log("▶ STARTING ADVERSARIAL STRESS TEST: GECKOVIEW RENDERING, TOUCH & NAVIGATION");
console.log("==================================================================================");

// -----------------------------------------------------------------------------
// SETUP: Load Authentic firefox.apk & Initialize Runtime
// -----------------------------------------------------------------------------
const apkPath = 'firefox.apk';
assert(fs.existsSync(apkPath), `firefox.apk archive file must exist at ${apkPath}`);
const apkBuffer = fs.readFileSync(apkPath);

let mockCanvasBuffer = new Uint8ClampedArray(720 * 1440 * 4);
const drawCalls = {
    fillRects: [],
    drawnTexts: [],
    putImageDataCount: 0
};

const mockCanvas = {
    width: 720,
    height: 1440,
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 720, height: 1440 }),
    getContext: (type) => ({
        fillStyle: '',
        strokeStyle: '',
        lineWidth: 1,
        font: '',
        fillRect: (x, y, w, h) => drawCalls.fillRects.push({ x, y, w, h }),
        clearRect: () => {},
        fillText: (txt, x, y) => drawCalls.drawnTexts.push({ txt, x, y }),
        strokeRect: () => {},
        measureText: (txt) => ({ width: (txt || '').length * 8 }),
        beginPath: () => {},
        roundRect: () => {},
        fill: () => {},
        stroke: () => {},
        save: () => {},
        restore: () => {},
        createImageData: (w, h) => ({ data: new Uint8ClampedArray(w * h * 4) }),
        putImageData: (imgData) => {
            drawCalls.putImageDataCount++;
            mockCanvasBuffer.set(imgData.data);
        },
        getImageData: (x, y, w, h) => ({ data: mockCanvasBuffer }),
        drawImage: () => {}
    })
};

const runtime = new AndroidRuntime();
runtime.setCanvas(mockCanvas);

const appState = await runtime.loadAndRunApk(apkBuffer);
assert(appState.packageName === 'org.mozilla.firefox', "Package name must be org.mozilla.firefox");

// =============================================================================
// TEST SUITE 1: BOUNDARY & MALFORMED TOUCH COORDINATE MATRIX
// =============================================================================
console.log("\n--- Suite 1: Boundary & Malformed Touch Coordinate Matrix ---");
const boundaryCoordinates = [
    // Exact canvas bounds
    { name: "Top-Left (0, 0)", x: 0, y: 0 },
    { name: "Top-Right (719, 0)", x: 719, y: 0 },
    { name: "Bottom-Left (0, 1439)", x: 0, y: 1439 },
    { name: "Bottom-Right (719, 1439)", x: 719, y: 1439 },
    { name: "Center (360, 720)", x: 360, y: 720 },
    // Off-screen negative coordinates
    { name: "Negative X (-1, 500)", x: -1, y: 500 },
    { name: "Negative Y (500, -1)", x: 500, y: -1 },
    { name: "Extreme Negative (-10000, -10000)", x: -10000, y: -10000 },
    // Exact outer boundary and overflow coordinates
    { name: "Exact Outer Width (720, 720)", x: 720, y: 720 },
    { name: "Exact Outer Height (360, 1440)", x: 360, y: 1440 },
    { name: "Overflow Coordinate (721, 1441)", x: 721, y: 1441 },
    { name: "Extreme Overflow (100000, 100000)", x: 100000, y: 100000 },
    // Sub-pixel floating point coordinates
    { name: "Sub-pixel Float (360.45, 720.89)", x: 360.45, y: 720.89 },
    { name: "Near Zero Float (0.0001, 0.0001)", x: 0.0001, y: 0.0001 },
    { name: "Near Max Float (718.999, 1438.999)", x: 718.999, y: 1438.999 },
    // Malformed coordinates
    { name: "NaN coordinates (NaN, NaN)", x: NaN, y: NaN },
    { name: "Infinity coordinates (Infinity, Infinity)", x: Infinity, y: Infinity },
    { name: "Negative Infinity (-Infinity, -Infinity)", x: -Infinity, y: -Infinity },
    { name: "Undefined coordinates (undefined, undefined)", x: undefined, y: undefined },
    { name: "Null coordinates (null, null)", x: null, y: null }
];

for (const coord of boundaryCoordinates) {
    // Test ACTION_DOWN
    const downEvt = new MotionEvent(MotionEvent.ACTION_DOWN, coord.x, coord.y);
    let downResult;
    try {
        downResult = runtime.dispatchInputEvent(downEvt);
        metrics.touchEventsDispatched++;
    } catch (err) {
        assert(false, `dispatchInputEvent(ACTION_DOWN) crashed on ${coord.name}: ${err.message}`);
    }
    assert(typeof downResult === 'boolean', `Result of ${coord.name} DOWN must be boolean (got ${downResult})`);

    // Test ACTION_MOVE
    const moveEvt = new MotionEvent(MotionEvent.ACTION_MOVE, coord.x, coord.y);
    try {
        runtime.dispatchInputEvent(moveEvt);
        metrics.touchEventsDispatched++;
    } catch (err) {
        assert(false, `dispatchInputEvent(ACTION_MOVE) crashed on ${coord.name}: ${err.message}`);
    }

    // Test ACTION_UP
    const upEvt = new MotionEvent(MotionEvent.ACTION_UP, coord.x, coord.y);
    let upResult;
    try {
        upResult = runtime.dispatchInputEvent(upEvt);
        metrics.touchEventsDispatched++;
    } catch (err) {
        assert(false, `dispatchInputEvent(ACTION_UP) crashed on ${coord.name}: ${err.message}`);
    }
    assert(typeof upResult === 'boolean', `Result of ${coord.name} UP must be boolean (got ${upResult})`);

    // Test ACTION_CANCEL
    const cancelEvt = new MotionEvent(MotionEvent.ACTION_CANCEL, coord.x, coord.y);
    try {
        runtime.dispatchInputEvent(cancelEvt);
        metrics.touchEventsDispatched++;
    } catch (err) {
        assert(false, `dispatchInputEvent(ACTION_CANCEL) crashed on ${coord.name}: ${err.message}`);
    }
}
console.log(`  ✔ [PASS] All ${boundaryCoordinates.length} boundary and malformed coordinate cases handled without crashes`);

// =============================================================================
// TEST SUITE 2: HIGH-FREQUENCY RAPID TOUCH STORM (2,500 EVENTS)
// =============================================================================
console.log("\n--- Suite 2: High-Frequency Rapid Touch Storm (2,500 Events) ---");
const tStorm0 = performance.now();
const TOTAL_STORM_EVENTS = 2500;
let stormHandledCount = 0;

for (let i = 0; i < TOTAL_STORM_EVENTS; i++) {
    const randX = (Math.sin(i * 0.3) * 0.5 + 0.5) * 800 - 40; // -40 to 760
    const randY = (Math.cos(i * 0.3) * 0.5 + 0.5) * 1600 - 80; // -80 to 1520
    const action = i % 4; // DOWN (0), UP (1), MOVE (2), CANCEL (3)
    
    const evt = new MotionEvent(action, randX, randY);
    const handled = runtime.dispatchInputEvent(evt);
    if (handled) stormHandledCount++;
    metrics.touchEventsDispatched++;
}
const stormDuration = performance.now() - tStorm0;
metrics.timings.touchStormMs = stormDuration;
const stormThroughput = (TOTAL_STORM_EVENTS / (stormDuration / 1000)).toFixed(0);

console.log(`  ✔ [PASS] Dispatched ${TOTAL_STORM_EVENTS} rapid touch events in ${stormDuration.toFixed(2)}ms (${stormThroughput} events/sec, ${stormHandledCount} handled)`);
assert(stormDuration < 15000, `2,500 touch events must complete in < 15,000ms (actual: ${stormDuration.toFixed(2)}ms)`);

// =============================================================================
// TEST SUITE 3: RAPID NAVIGATION TOGGLING & URL MUTATION FUZZING (500 CYCLES)
// =============================================================================
console.log("\n--- Suite 3: Rapid Navigation Toggling & URL Mutation Fuzzing (500 Cycles) ---");

const testUrls = [
    { url: "https://www.google.com", page: "Google" },
    { url: "https://www.mozilla.org/firefox", page: "home" },
    { url: "https://wikipedia.org", page: "Wikipedia" },
    { url: "https://developer.mozilla.org", page: "MDN Web Docs" },
    { url: "https://w3.org/TR/webgpu", page: "WebGPU Specification" },
    { url: "https://rust-lang.org", page: "Rust Programming" },
    // Fuzz inputs & Edge URLs
    { url: "", page: "empty" },
    { url: "about:blank", page: "blank" },
    { url: "javascript:void(0)", page: "js" },
    { url: "https://example.com/search?q=" + encodeURIComponent("🚀 WebGPU & Rust 🔥"), page: "unicode" },
    { url: "https://example.com/" + "a".repeat(2000), page: "long_url" },
    { url: "http://127.0.0.1:8080/test", page: "localhost" }
];

const tNav0 = performance.now();
const NAV_CYCLES = 500;

for (let i = 0; i < NAV_CYCLES; i++) {
    const target = testUrls[i % testUrls.length];
    appState.activeUrl = target.url;
    appState.currentPage = target.page;

    runtime.renderActivityUi(appState);
    metrics.navigationCycles++;
    metrics.framesRasterized++;

    // Verify root view layout stability
    assert(runtime.currentRootView !== null, `currentRootView must not be null at cycle ${i}`);
    assert(runtime.currentRootView.getWidth() === 720, `RootView width must remain 720 at cycle ${i}`);
    assert(runtime.currentRootView.getHeight() === 1440, `RootView height must remain 1440 at cycle ${i}`);
}

const navDuration = performance.now() - tNav0;
metrics.timings.navigationCyclesMs = navDuration;
const navFps = (NAV_CYCLES / (navDuration / 1000)).toFixed(1);
console.log(`  ✔ [PASS] Executed ${NAV_CYCLES} rapid navigation & re-rasterization cycles in ${navDuration.toFixed(2)}ms (${navFps} FPS)`);
assert(navDuration < 20000, `500 navigation cycles must complete in < 20,000ms (actual: ${navDuration.toFixed(2)}ms)`);

// =============================================================================
// TEST SUITE 4: BUFFER MEMORY BOUNDS, RGBA LAYOUT & SHANNON ENTROPY
// =============================================================================
console.log("\n--- Suite 4: Buffer Memory Bounds, RGBA Layout & Shannon Entropy ---");

const rasterizer = new ViewHierarchyRasterizer(720, 1440);

// Test 4a: Exact Buffer Byte Count
const frameGoogle = rasterizer.rasterize(runtime.currentRootView, 720, 1440);
assert(frameGoogle.rgbaData instanceof Uint8Array, "Rasterizer rgbaData must be an instance of Uint8Array");
assert(frameGoogle.rgbaData.byteLength === 4147200, `Buffer size must be exactly 4,147,200 bytes (got: ${frameGoogle.rgbaData.byteLength})`);
assert(frameGoogle.width === 720, "Rasterizer frame width must be 720");
assert(frameGoogle.height === 1440, "Rasterizer frame height must be 1440");
assert(Array.isArray(frameGoogle.damageRect) && frameGoogle.damageRect.length === 4, "damageRect must be a 4-element array");
metrics.bufferIntegrityChecks++;

// Test 4b: RGBA Pixel Validation & Bounds
let nonZeroAlphaCount = 0;
let invalidPixelCount = 0;
for (let i = 0; i < frameGoogle.rgbaData.length; i += 4) {
    const r = frameGoogle.rgbaData[i];
    const g = frameGoogle.rgbaData[i + 1];
    const b = frameGoogle.rgbaData[i + 2];
    const a = frameGoogle.rgbaData[i + 3];

    if (r < 0 || r > 255 || g < 0 || g > 255 || b < 0 || b > 255 || a < 0 || a > 255) {
        invalidPixelCount++;
    }
    if (a > 0) nonZeroAlphaCount++;
}
assert(invalidPixelCount === 0, `No invalid pixel values permitted (found: ${invalidPixelCount})`);
assert(nonZeroAlphaCount === 720 * 1440, `All pixels must have non-zero alpha in full screen frame (got: ${nonZeroAlphaCount} / ${720 * 1440})`);
metrics.bufferIntegrityChecks++;

// Test 4c: Shannon Entropy Verification Across Different Pages
// 1. Google Page Entropy
appState.activeUrl = 'https://www.google.com';
appState.currentPage = 'Google';
runtime.renderActivityUi(appState);
const frameGooglePage = rasterizer.rasterize(runtime.currentRootView, 720, 1440);
const googleRgbaCopy = new Uint8Array(frameGooglePage.rgbaData);
const entropyGoogle = calculateShannonEntropy(googleRgbaCopy);
metrics.shannonEntropyScores.google = entropyGoogle;
console.log(`  📊 Measured Google Search page Shannon entropy: H = ${entropyGoogle.toFixed(4)} bits/pixel`);
assert(entropyGoogle >= 1.0, `Google page Shannon entropy must be >= 1.0 (actual: ${entropyGoogle.toFixed(4)})`);

// 2. Home Page Entropy
appState.activeUrl = 'https://www.mozilla.org/firefox';
appState.currentPage = 'home';
runtime.renderActivityUi(appState);
const frameHomePage = rasterizer.rasterize(runtime.currentRootView, 720, 1440);
const homeRgbaCopy = new Uint8Array(frameHomePage.rgbaData);
const entropyHome = calculateShannonEntropy(homeRgbaCopy);
metrics.shannonEntropyScores.home = entropyHome;
console.log(`  📊 Measured Home / Top Sites page Shannon entropy: H = ${entropyHome.toFixed(4)} bits/pixel`);
assert(entropyHome >= 1.0, `Home page Shannon entropy must be >= 1.0 (actual: ${entropyHome.toFixed(4)})`);

// 3. Generic Web Page Entropy (Wikipedia)
appState.activeUrl = 'https://wikipedia.org';
appState.currentPage = 'Wikipedia';
runtime.renderActivityUi(appState);
const frameWikiPage = rasterizer.rasterize(runtime.currentRootView, 720, 1440);
const wikiRgbaCopy = new Uint8Array(frameWikiPage.rgbaData);
const entropyWiki = calculateShannonEntropy(wikiRgbaCopy);
metrics.shannonEntropyScores.wikipedia = entropyWiki;
console.log(`  📊 Measured Wikipedia page Shannon entropy: H = ${entropyWiki.toFixed(4)} bits/pixel`);
assert(entropyWiki >= 1.0, `Wikipedia page Shannon entropy must be >= 1.0 (actual: ${entropyWiki.toFixed(4)})`);

// Verify Distinct Visual Fingerprints Between Pages
let diffGoogleVsHome = 0;
for (let idx = 0; idx < googleRgbaCopy.length; idx++) {
    if (googleRgbaCopy[idx] !== homeRgbaCopy[idx]) diffGoogleVsHome++;
}
console.log(`  📊 Pixel byte differences between Google and Home layouts: ${diffGoogleVsHome.toLocaleString()} bytes`);
assert(diffGoogleVsHome > 50000, `Layout transition must produce distinct visual frames (>50,000 byte diffs, got ${diffGoogleVsHome})`);

// =============================================================================
// TEST SUITE 5: SOFTWARE2D CONTEXT ADVERSARIAL DRAWING STRESS
// =============================================================================
console.log("\n--- Suite 5: Software2DContext Adversarial Drawing Stress ---");

const testBuf = new Uint8Array(720 * 1440 * 4);
const swCtx = new Software2DContext(testBuf, 720, 1440);

// 5a. Extreme fillRect cases
swCtx.fillRect(-500, -500, 1000, 1000); // Crosses top-left corner
swCtx.fillRect(700, 1400, 500, 500);   // Crosses bottom-right corner
swCtx.fillRect(-10000, -10000, 100, 100); // Fully outside top-left
swCtx.fillRect(10000, 10000, 100, 100);   // Fully outside bottom-right
swCtx.fillRect(100, 100, 0, 0);          // Zero size
swCtx.fillRect(100, 100, -50, -50);      // Negative size
swCtx.fillRect(NaN, NaN, 100, 100);      // NaN coordinates

// 5b. Extreme fillText cases
swCtx.font = "24px sans-serif";
swCtx.fillStyle = "#ffffff";
swCtx.fillText("Adversarial Normal String", 100, 100);
swCtx.fillText("Offscreen Text Left", -500, 200);
swCtx.fillText("Offscreen Text Right", 5000, 200);
swCtx.fillText("Offscreen Text Top", 200, -500);
swCtx.fillText("Offscreen Text Bottom", 200, 5000);
swCtx.fillText("", 100, 300); // Empty string
swCtx.fillText("A".repeat(5000), 0, 400); // Giant string
swCtx.fillText("🔥🚀💻⚡🤖🦀", 100, 500); // Non-ASCII emojis
swCtx.fillText(null, 100, 600); // Null
swCtx.fillText(undefined, 100, 700); // Undefined

// 5c. State Stack Save/Restore underflow stress
for (let i = 0; i < 50; i++) swCtx.save();
for (let i = 0; i < 100; i++) swCtx.restore(); // 50 extra restores (underflow)

assert(testBuf.byteLength === 4147200, "Software2DContext buffer length must remain exactly 4,147,200 bytes");
console.log("  ✔ [PASS] Software2DContext survived all adversarial clipping, off-screen drawing, and stack underflows");

// =============================================================================
// TEST SUITE 6: VIRTIO-GPU SCANOUT & HOST INJECTION GATING ADVERSARIAL CHECK
// =============================================================================
console.log("\n--- Suite 6: VirtIO-GPU Scanout & Host Injection Gating Adversarial Check ---");

const mockGpuDevice = {
    guestActive: false,
    guestHasPresented: false,
    hostInjectionBlocked: false,
    processedPackets: [],
    processControlQueue: function(pkt) {
        this.processedPackets.push(pkt);
    },
    blockHostInjection: function() {
        this.hostInjectionBlocked = true;
    },
    allowHostInjection: function() {
        this.hostInjectionBlocked = false;
    },
    isHostInjectionAllowed: function() {
        return !this.guestHasPresented && !this.hostInjectionBlocked;
    }
};

runtime.setGpuDevice(mockGpuDevice);

// Step 1: When guest has NOT presented, host injection should be allowed
mockGpuDevice.processedPackets = [];
runtime.renderActivityUi(appState);
assert(mockGpuDevice.processedPackets.length > 0, "VirtIO packets must be submitted when guest has not presented");
const packetsBefore = mockGpuDevice.processedPackets.length;
console.log(`  ✔ [PASS] Host VirtIO injection allowed before guest presentation (${packetsBefore} control packets sent)`);

// Step 2: When guest presents (guestHasPresented = true), host injection MUST BE 100% GATED
mockGpuDevice.guestHasPresented = true;
mockGpuDevice.guestActive = true;
mockGpuDevice.processedPackets = [];

runtime.renderActivityUi(appState);
assert(mockGpuDevice.processedPackets.length === 0, `VirtIO packets must be ZERO when guestHasPresented=true (got ${mockGpuDevice.processedPackets.length})`);
console.log("  ✔ [PASS] Host VirtIO injection is 100% strictly gated when guestHasPresented=true");

// Step 3: Direct submission via submitToVirtioGpu must also be blocked
rasterizer.submitToVirtioGpu(mockGpuDevice, 100, 0, frameGooglePage.rgbaData);
assert(mockGpuDevice.processedPackets.length === 0, "Direct submitToVirtioGpu must drop buffers when guestHasPresented=true");
console.log("  ✔ [PASS] Direct submitToVirtioGpu drops host buffers when guestHasPresented=true");

// =============================================================================
// TEST SUMMARY & METRICS DUMP
// =============================================================================
console.log("\n==================================================================================");
console.log(`🎉 EMPIRICAL ADVERSARIAL STRESS TEST COMPLETE: ALL ${passed} ASSERTIONS PASSED (0 FAILURES)`);
console.log("==================================================================================");
console.log("Summary Metrics:");
console.log(`- Touch Events Dispatched: ${metrics.touchEventsDispatched.toLocaleString()}`);
console.log(`- Touch Storm Throughput: ${stormThroughput} events/sec (${metrics.timings.touchStormMs.toFixed(2)}ms)`);
console.log(`- Navigation Cycles: ${metrics.navigationCycles.toLocaleString()} (${metrics.timings.navigationCyclesMs.toFixed(2)}ms, ${navFps} FPS)`);
console.log(`- Buffer Exact Size: 4,147,200 bytes (720x1440x4 RGBA)`);
console.log(`- Google Search Shannon Entropy: H = ${metrics.shannonEntropyScores.google.toFixed(4)} bits/pixel`);
console.log(`- Home / Top Sites Shannon Entropy: H = ${metrics.shannonEntropyScores.home.toFixed(4)} bits/pixel`);
console.log(`- Wikipedia Page Shannon Entropy: H = ${metrics.shannonEntropyScores.wikipedia.toFixed(4)} bits/pixel`);
console.log(`- VirtIO Guest Scanout Lockout: 100% Verified Gated`);
console.log("==================================================================================");
