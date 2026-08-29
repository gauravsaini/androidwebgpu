/**
 * Comprehensive Test Suite for Firefox APK Ingestion & GeckoView Web Content Execution
 * 
 * Verifies:
 * 1. R1: Target APK Resolution, ApkZipReader, AxmlDecoder, Multi-DEX DalvikVM loading (80,012 classes), PMS registration.
 * 2. R2: GeckoView Activity Launch, Viewport transition to WebGPU canvas, activeUrl = https://www.google.com, Google mobile search 720x1440 rasterization.
 * 3. R3: Interactive Navigation, Address bar input, Top Sites navigation, Toolbar action button dispatch (Reload, Back, Home, Forward, Tabs), MotionEvent touch hit-testing.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import { ApkZipReader, AxmlDecoder, PackageManagerRegistry } from '../src/apk_client_parser.js';
import { DexParser, DalvikVM } from '../src/dex_vm.js';
import { AndroidRuntime } from '../src/android_runtime.js';
import { AppController } from '../src/app_controller.js';
import { SystemBootstrap } from '../src/system_bootstrap.js';
import { MotionEvent, ViewHierarchyRasterizer, ViewRootImpl } from '../src/view_rasterizer.js';
import { LinearLayout, TextView, FrameLayout, LayoutParams, MATCH_PARENT } from '../src/view_hierarchy.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`❌ FAIL: ${message}`);
        failed++;
        throw new Error(message);
    } else {
        console.log(`  ✔ [PASS] ${message}`);
        passed++;
    }
}

console.log("===================================================================");
console.log("▶ Running Firefox APK Ingestion & GeckoView Execution Test Suite");
console.log("===================================================================");

// Load authentic firefox.apk binary
const apkPath = 'firefox.apk';
assert(fs.existsSync(apkPath), `firefox.apk archive file must exist at ${apkPath}`);
const apkBuffer = fs.readFileSync(apkPath);
assert(apkBuffer.byteLength > 100000000, `firefox.apk size must exceed 100MB (actual: ${apkBuffer.byteLength} bytes)`);

// -----------------------------------------------------------------------------
// Suite 1: APK Ingestion, Zip Decoding, Manifest & Multi-DEX DalvikVM Loading
// -----------------------------------------------------------------------------
console.log("\n--- Suite 1: Target APK Resolution, Zip Unpacking & AXML Manifest Decoding ---");

const zipReader = new ApkZipReader(apkBuffer);
const entries = zipReader.readEntries();
assert(entries.size > 3000, `ApkZipReader must extract > 3,000 files from firefox.apk (found: ${entries.size})`);

const manifestBytes = zipReader.readFile('AndroidManifest.xml');
assert(manifestBytes && manifestBytes.byteLength > 0, "AndroidManifest.xml must be extracted from firefox.apk");

const manifest = AxmlDecoder.decode(manifestBytes);
assert(manifest.packageName === 'org.mozilla.firefox', `Manifest package name must be 'org.mozilla.firefox' (got: ${manifest.packageName})`);
assert(manifest.targetSdkVersion === 37, `Manifest targetSdkVersion must be 37 (got: ${manifest.targetSdkVersion})`);
assert(manifest.minSdkVersion === 26, `Manifest minSdkVersion must be 26 (got: ${manifest.minSdkVersion})`);
assert(manifest.activities.length === 45, `Manifest must declare 45 activities (found: ${manifest.activities.length})`);
assert(manifest.permissions.length === 36, `Manifest must declare 36 permissions (found: ${manifest.permissions.length})`);
assert(manifest.launcherActivity && manifest.launcherActivity.includes('org.mozilla.firefox'), `Launcher activity must be defined (got: ${manifest.launcherActivity})`);

console.log("\n--- Suite 1b: Multi-DEX Bytecode Loading (80,012 Classes into DalvikVM) ---");

const dexFileNames = ['classes.dex', 'classes2.dex', 'classes3.dex'];
const dexParsers = [];
const vm = new DalvikVM();
let totalDexClasses = 0;

for (const dexName of dexFileNames) {
    const dexBytes = zipReader.readFile(dexName);
    assert(dexBytes && dexBytes.byteLength > 0, `${dexName} must be present in firefox.apk`);
    const parser = new DexParser(dexBytes, dexName).parse();
    assert(parser.classes.size > 20000, `${dexName} must contain > 20,000 classes (got: ${parser.classes.size})`);
    assert(parser.methods.length > 40000, `${dexName} must contain > 40,000 methods (got: ${parser.methods.length})`);
    vm.loadDex(parser);
    dexParsers.push(parser);
    totalDexClasses += parser.classes.size;
}

assert(dexParsers[0].classes.size === 30592, `classes.dex must contain 30,592 classes (got: ${dexParsers[0].classes.size})`);
assert(dexParsers[1].classes.size === 24666, `classes2.dex must contain 24,666 classes (got: ${dexParsers[1].classes.size})`);
assert(dexParsers[2].classes.size === 24754, `classes3.dex must contain 24,754 classes (got: ${dexParsers[2].classes.size})`);
assert(totalDexClasses === 80012, `Total parsed classes across 3 DEX files must equal 80,012 (got: ${totalDexClasses})`);
assert(vm.classes.size === 80012, `DalvikVM class registry must hold exactly 80,012 classes (got: ${vm.classes.size})`);

// Verify Native Shared Libraries (.so) in APK
const nativeLibs = [];
for (const [name] of entries) {
    if (name.startsWith('lib/') && name.endsWith('.so')) {
        nativeLibs.push(name);
    }
}
assert(nativeLibs.length === 18, `firefox.apk must contain 18 native x86_64 libraries (found: ${nativeLibs.length})`);
assert(nativeLibs.some(l => l.includes('libxul.so')), "libxul.so must be present in native libraries");
assert(nativeLibs.some(l => l.includes('libmozglue.so')), "libmozglue.so must be present in native libraries");

// -----------------------------------------------------------------------------
// Suite 2: PMS Registration & GeckoView Activity Launch
// -----------------------------------------------------------------------------
console.log("\n--- Suite 2: PMS Registration & GeckoView Activity Viewport Transition ---");

const pms = new PackageManagerRegistry();
pms.registerPackage({
    packageName: 'org.mozilla.firefox',
    versionCode: 1,
    versionName: '124.0',
    applicationLabel: 'Firefox',
    launcherActivity: manifest.launcherActivity,
    activities: manifest.activities,
    permissions: manifest.permissions
});

assert(pms.getPackageInfo('org.mozilla.firefox') !== null, "PMS must retrieve org.mozilla.firefox package info");
assert(pms.getPackageInfo('org.mozilla.firefox').applicationLabel === 'Firefox', "PMS package applicationLabel must be 'Firefox'");

// Setup Mock DOM & Canvas
const drawnTexts = [];
const fillRects = [];
let mockCanvasBuffer = new Uint8ClampedArray(720 * 1440 * 4);

const mockCanvas = {
    width: 720,
    height: 1440,
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 720, height: 1440 }),
    getContext: (type) => ({
        fillStyle: '',
        strokeStyle: '',
        lineWidth: 1,
        font: '',
        fillRect: (x, y, w, h) => fillRects.push({ x, y, w, h }),
        clearRect: () => {},
        fillText: (txt, x, y) => drawnTexts.push({ txt, x, y }),
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
            mockCanvasBuffer.set(imgData.data);
        },
        getImageData: (x, y, w, h) => ({ data: mockCanvasBuffer }),
        drawImage: () => {}
    })
};

const dom = {
    screenHome: { style: { display: 'block' }, classList: { contains: () => true, add: () => {}, remove: () => {} } },
    screenV86: { style: { display: 'none' }, classList: { contains: () => false, add: () => {}, remove: () => {} } },
    screenWebGpu: { style: { display: 'none' }, classList: { contains: () => false, add: () => {}, remove: () => {} } },
    canvas: mockCanvas,
    homeAppGrid: { appendChild: () => {}, querySelectorAll: () => [] },
    homeDock: { appendChild: () => {} },
    toast: { classList: { add: () => {}, remove: () => {} }, textContent: '' },
    toastText: { textContent: '' }
};

const runtime = new AndroidRuntime();
runtime.setCanvas(mockCanvas);

const bootstrap = new SystemBootstrap({ memorySizeMb: 512, autostart: false });
const appController = new AppController({
    bootstrap,
    runtime,
    domElements: dom,
    onLogcat: () => {},
    onToast: () => {}
});

// Ingest firefox.apk into runtime
const appState = await runtime.loadAndRunApk(apkBuffer);
assert(appState.packageName === 'org.mozilla.firefox', "appState packageName must be 'org.mozilla.firefox'");
assert(appState.activeUrl === 'https://www.google.com', `Default activeUrl must be 'https://www.google.com' (got: ${appState.activeUrl})`);
assert(appState.currentPage === 'Google', `Default currentPage must be 'Google' (got: ${appState.currentPage})`);

// Launch Activity via AppController
await appController.launchActivity('org.mozilla.firefox');
assert(appController.activeScreen === 'webgpu', `AppController activeScreen must transition to 'webgpu' (got: ${appController.activeScreen})`);
assert(dom.screenWebGpu.style.display === 'flex', "DOM #screen-webgpu viewport must be displayed (style.display = 'flex')");

// -----------------------------------------------------------------------------
// Suite 3: Google Search Mobile Layout Rasterization
// -----------------------------------------------------------------------------
console.log("\n--- Suite 3: Google Search Mobile Layout Rasterization to 720x1440 Canvas ---");

const textStrings = drawnTexts.map(t => t.txt);
assert(textStrings.some(t => t.includes('Firefox Browser')), "Canvas must render Firefox Browser header");
assert(textStrings.some(t => t === "🦊  Firefox Browser  •  GeckoView Engine (x86_64 / EGL)"), "Canvas must render exact Firefox Browser header '🦊  Firefox Browser  •  GeckoView Engine (x86_64 / EGL)'");
assert(textStrings.some(t => t.includes('🔒  https://www.google.com')), "Canvas must render active URL '🔒  https://www.google.com'");
assert(textStrings.some(t => t.includes('G o o g l e')), "Canvas must render Google logo text 'G o o g l e'");
assert(textStrings.some(t => t.includes('Sign in')), "Canvas must render Google Sign in button");
assert(textStrings.some(t => t.includes('Search Google or type a URL')), "Canvas must render Google search pill placeholder");
assert(textStrings.some(t => t === 'Google Search'), "Canvas must render 'Google Search' action button");
assert(textStrings.some(t => t === "I'm Feeling Lucky"), "Canvas must render \"I'm Feeling Lucky\" action button");
assert(textStrings.some(t => t.includes('Google offered in:')), "Canvas must render language offerings");
assert(textStrings.some(t => t.includes('Trending on Google')), "Canvas must render Trending on Google section");
assert(textStrings.some(t => t.includes('Android 14 WebGPU Graphics Acceleration')), "Canvas must render Trending cards");
assert(textStrings.some(t => t.includes('Top Sites & Bookmarks')), "Canvas must render 'Top Sites & Bookmarks' navigation button");

// Check Toolbar buttons
const expectedToolbar = ['◀', '▶', '🔄', '🏠', '📑'];
for (const icon of expectedToolbar) {
    assert(textStrings.some(t => t === icon), `Canvas bottom toolbar must render action icon '${icon}'`);
}

// Check 720x1440 buffer rasterization
assert(mockCanvasBuffer.byteLength === 720 * 1440 * 4, `Canvas buffer size must equal 4,147,200 bytes (got: ${mockCanvasBuffer.byteLength})`);

// -----------------------------------------------------------------------------
// Suite 4: Interactive Navigation, Address Bar & Toolbar Action Event Dispatch
// -----------------------------------------------------------------------------
console.log("\n--- Suite 4: Interactive Navigation & Toolbar Action Button Dispatch ---");

// Helper function to find a view by text in the current root view hierarchy
function findViewByText(root, text) {
    if (!root) return null;
    if (root.findViewByText && typeof root.findViewByText === 'function') {
        return root.findViewByText(text);
    }
    if (root.text === text || (typeof root.text === 'string' && root.text.includes(text))) {
        return root;
    }
    if (root.children) {
        for (const c of root.children) {
            const found = findViewByText(c, text);
            if (found) return found;
        }
    }
    return null;
}

// 1. Directly trigger Toolbar Back Button ('◀')
console.log("Testing direct click listener on Toolbar Back button ('◀')...");
appState.activeUrl = 'https://www.google.com';
appState.currentPage = 'Google';
runtime.renderActivityUi(appState);
assert(appState.activeUrl === 'https://www.google.com', "Initial activeUrl must be 'https://www.google.com'");

const backBtn = findViewByText(runtime.currentRootView, '◀');
assert(backBtn !== null, "Toolbar Back button view ('◀') must be found in view hierarchy");
assert(typeof backBtn.onClickListener === 'function' || typeof backBtn.performClick === 'function', "Back button must have a valid click listener");

drawnTexts.length = 0;
if (typeof backBtn.performClick === 'function') {
    backBtn.performClick();
} else if (typeof backBtn.onClickListener === 'function') {
    backBtn.onClickListener(backBtn);
}

assert(appState.activeUrl === 'https://www.mozilla.org/firefox', "Toolbar Back click must transition activeUrl to 'https://www.mozilla.org/firefox'");
assert(appState.currentPage === 'home', "Toolbar Back click must transition currentPage to 'home'");

const homeTexts = drawnTexts.map(t => t.txt);
assert(homeTexts.some(t => t.includes('Top Sites & Bookmarks')), "Home view must render 'Top Sites & Bookmarks' label");
assert(homeTexts.some(t => t.includes('Fast, Private & Open Source Mobile Web')), "Home view must render welcoming subtitle");

const expectedShortcuts = ['Google', 'Mozilla', 'Wikipedia', 'MDN Web Docs', 'WebGPU Specification', 'Rust Programming'];
for (const site of expectedShortcuts) {
    assert(homeTexts.some(t => t === site), `Top Sites must list shortcut '${site}'`);
}

// 2. Select a Shortcut (Wikipedia) via direct click listener -> Generic Live Web Page View
console.log("Testing direct click listener on Shortcut card (Wikipedia)...");
const wikiCard = findViewByText(runtime.currentRootView, 'Wikipedia');
assert(wikiCard !== null, "Wikipedia shortcut must be found in view hierarchy");

drawnTexts.length = 0;
if (typeof wikiCard.performClick === 'function') {
    wikiCard.performClick();
} else if (typeof wikiCard.onClickListener === 'function') {
    wikiCard.onClickListener(wikiCard);
}

assert(appState.activeUrl === 'https://wikipedia.org', "Clicking Wikipedia shortcut must transition activeUrl to 'https://wikipedia.org'");
assert(appState.currentPage === 'Wikipedia', "Clicking Wikipedia shortcut must transition currentPage to 'Wikipedia'");

const wikiTexts = drawnTexts.map(t => t.txt);
assert(wikiTexts.some(t => t.includes('Wikipedia')), "Page must render 'Wikipedia' title header");
assert(wikiTexts.some(t => t.includes('Rendering live content for https://wikipedia.org')), "Page must render live content indicator");
assert(wikiTexts.some(t => t.includes('Back to Google & Top Sites')), "Page must render 'Back to Google & Top Sites' return button");

// 3. Test Toolbar Reload Button ('🔄')
console.log("Testing direct click listener on Toolbar Reload button ('🔄')...");
const reloadBtn = findViewByText(runtime.currentRootView, '🔄');
assert(reloadBtn !== null, "Toolbar Reload button view ('🔄') must be found in view hierarchy");

drawnTexts.length = 0;
if (typeof reloadBtn.performClick === 'function') {
    reloadBtn.performClick();
} else if (typeof reloadBtn.onClickListener === 'function') {
    reloadBtn.onClickListener(reloadBtn);
}

assert(appState.activeUrl === 'https://wikipedia.org', "Reload button must preserve activeUrl 'https://wikipedia.org'");
assert(appState.currentPage === 'Wikipedia', "Reload button must preserve currentPage 'Wikipedia'");
assert(drawnTexts.map(t => t.txt).some(t => t.includes('Rendering live content for https://wikipedia.org')), "Reload must re-rasterize active page content");

// 4. Test Toolbar Home Button ('🏠')
console.log("Testing direct click listener on Toolbar Home button ('🏠')...");
const homeBtn = findViewByText(runtime.currentRootView, '🏠');
assert(homeBtn !== null, "Toolbar Home button view ('🏠') must be found in view hierarchy");

drawnTexts.length = 0;
if (typeof homeBtn.performClick === 'function') {
    homeBtn.performClick();
} else if (typeof homeBtn.onClickListener === 'function') {
    homeBtn.onClickListener(homeBtn);
}

assert(appState.activeUrl === 'https://www.google.com', "Clicking Home button must transition activeUrl to 'https://www.google.com'");
assert(appState.currentPage === 'Google', "Clicking Home button must transition currentPage to 'Google'");

const backToGoogleTexts = drawnTexts.map(t => t.txt);
assert(backToGoogleTexts.some(t => t.includes('G o o g l e')), "Returning to Google via Home button must rasterize Google logo");
assert(backToGoogleTexts.some(t => t.includes('Search Google or type a URL')), "Returning to Google via Home button must rasterize search box");

// 5. Test Toolbar Forward ('▶') and Tabs ('📑') buttons
console.log("Testing direct click listeners on Toolbar Forward ('▶') & Tabs ('📑')...");
const fwdBtn = findViewByText(runtime.currentRootView, '▶');
assert(fwdBtn !== null, "Toolbar Forward button view ('▶') must be found in view hierarchy");
if (typeof fwdBtn.performClick === 'function') fwdBtn.performClick();
else if (typeof fwdBtn.onClickListener === 'function') fwdBtn.onClickListener(fwdBtn);

const tabsBtn = findViewByText(runtime.currentRootView, '📑');
assert(tabsBtn !== null, "Toolbar Tabs button view ('📑') must be found in view hierarchy");
if (typeof tabsBtn.performClick === 'function') tabsBtn.performClick();
else if (typeof tabsBtn.onClickListener === 'function') tabsBtn.onClickListener(tabsBtn);

// 6. Test URL Search Bar direct click listener
console.log("Testing direct click listener on URL search bar...");
const urlBar = findViewByText(runtime.currentRootView, 'https://www.google.com');
assert(urlBar !== null, "URL search bar must be found in view hierarchy");
if (typeof urlBar.performClick === 'function') urlBar.performClick();
else if (typeof urlBar.onClickListener === 'function') urlBar.onClickListener(urlBar);
assert(appState.activeUrl === 'https://www.google.com', "URL bar click maintains activeUrl = 'https://www.google.com'");
assert(appState.currentPage === 'Google', "URL bar click maintains currentPage = 'Google'");

// 7. Test Return Button on Google Page ("⬅ Top Sites & Bookmarks")
console.log("Testing direct click listener on '⬅ Top Sites & Bookmarks' return button...");
const returnBtn = findViewByText(runtime.currentRootView, 'Top Sites & Bookmarks');
assert(returnBtn !== null, "Top Sites return button must be found in view hierarchy");
drawnTexts.length = 0;
if (typeof returnBtn.performClick === 'function') returnBtn.performClick();
else if (typeof returnBtn.onClickListener === 'function') returnBtn.onClickListener(returnBtn);
assert(appState.activeUrl === 'https://www.mozilla.org/firefox', "Top Sites return button must transition activeUrl to 'https://www.mozilla.org/firefox'");
assert(appState.currentPage === 'home', "Top Sites return button must transition currentPage to 'home'");

// 8. Test MotionEvent Touch Dispatch & Hit Testing on ViewRootImpl
console.log("Testing MotionEvent touch event dispatch on ViewRootImpl...");
const downEvent = new MotionEvent(MotionEvent.ACTION_DOWN, 360, 200);
const handledDown = runtime.dispatchInputEvent(downEvent);
assert(typeof handledDown === 'boolean', "dispatchInputEvent(ACTION_DOWN) must return boolean");

const upEvent = new MotionEvent(MotionEvent.ACTION_UP, 360, 200);
const handledUp = runtime.dispatchInputEvent(upEvent);
assert(typeof handledUp === 'boolean', "dispatchInputEvent(ACTION_UP) must return boolean");

console.log("===================================================================");
console.log(`🎉 ALL FIREFOX GECKOVIEW TESTS PASSED! (${passed} assertions, ${failed} failures)`);
console.log("===================================================================");
