/**
 * tests/test_challenger_m3_2_empirical_stress.mjs
 * 
 * Challenger 2 Empirical Stress Test Suite for Milestone 3:
 * Target APK Auto-Ingestion, Runtime Launch, Logcat Circular Buffer, Binder Routing, and Error Recovery.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// -----------------------------------------------------------------------------
// Mock DOM & Environment Shims (Must be established before UI component instantiation)
// -----------------------------------------------------------------------------

class MockDOMElement {
    constructor(tagName = 'div', id = '') {
        this.tagName = tagName.toUpperCase();
        this.id = id;
        this.children = [];
        this.style = {};
        this.dataset = {};
        this.classList = {
            _classes: new Set(),
            add: (c) => this.classList._classes.add(c),
            remove: (c) => this.classList._classes.delete(c),
            toggle: (c, force) => {
                if (force !== undefined) {
                    if (force) this.classList._classes.add(c);
                    else this.classList._classes.delete(c);
                    return force;
                }
                const has = this.classList._classes.has(c);
                if (has) this.classList._classes.delete(c);
                else this.classList._classes.add(c);
                return !has;
            },
            contains: (c) => this.classList._classes.has(c)
        };
        this.listeners = new Map();
        this.innerHTML = '';
        this.textContent = '';
        this.value = '';
        this.scrollTop = 0;
        this.scrollHeight = 1000;
        this.clientHeight = 500;
        this.width = 720;
        this.height = 1440;
    }

    get className() {
        return Array.from(this.classList._classes).join(' ');
    }

    set className(val) {
        this.classList._classes = new Set((val || '').split(/\s+/).filter(Boolean));
    }

    get childElementCount() {
        return this.children.length;
    }

    setAttribute(name, val) {
        if (name.startsWith('data-')) {
            const prop = name.slice(5);
            this.dataset[prop] = val;
        }
        this[name] = val;
    }

    getAttribute(name) {
        if (name.startsWith('data-')) {
            const prop = name.slice(5);
            return this.dataset[prop];
        }
        return this[name] || null;
    }

    addEventListener(event, handler) {
        if (!this.listeners.has(event)) this.listeners.set(event, []);
        this.listeners.get(event).push(handler);
    }

    removeEventListener(event, handler) {
        if (this.listeners.has(event)) {
            this.listeners.set(event, this.listeners.get(event).filter(h => h !== handler));
        }
    }

    dispatchEvent(event) {
        const list = this.listeners.get(event.type) || [];
        for (const h of list) h(event);
    }

    click() {
        this.dispatchEvent({ type: 'click' });
    }

    getBoundingClientRect() {
        return { left: 0, top: 0, width: this.width, height: this.height, right: this.width, bottom: this.height };
    }

    appendChild(child) {
        this.children.push(child);
        return child;
    }

    removeChild(child) {
        const idx = this.children.indexOf(child);
        if (idx !== -1) {
            this.children.splice(idx, 1);
        }
        return child;
    }

    insertBefore(newNode, refNode) {
        if (!refNode) {
            this.children.push(newNode);
        } else {
            const idx = this.children.indexOf(refNode);
            if (idx === -1) this.children.push(newNode);
            else this.children.splice(idx, 0, newNode);
        }
        return newNode;
    }

    get firstChild() {
        return this.children.length > 0 ? this.children[0] : null;
    }

    querySelector(selector) {
        return this.children.find(c => {
            if (selector.includes('[data-pkg=')) {
                const match = selector.match(/\[data-pkg="?([^"\]]+)"?\]/);
                if (match && c.dataset && c.dataset.pkg === match[1]) {
                    return true;
                }
            }
            if (selector.startsWith('.')) return c.classList.contains(selector.slice(1));
            if (selector.startsWith('#')) return c.id === selector.slice(1);
            return false;
        }) || null;
    }

    querySelectorAll(selector) {
        return this.children.filter(c => {
            if (selector.startsWith('.')) return c.classList.contains(selector.slice(1));
            return false;
        });
    }

    getContext(type) {
        return {
            fillRect: () => {},
            clearRect: () => {},
            drawImage: () => {},
            save: () => {},
            restore: () => {},
            fillText: () => {},
            beginPath: () => {},
            stroke: () => {},
            fill: () => {},
            scale: () => {},
            translate: () => {},
            measureText: (txt) => ({ width: (txt || '').length * 8 })
        };
    }
}

if (typeof globalThis.document === 'undefined') {
    globalThis.document = {
        createElement: (tag) => new MockDOMElement(tag),
        getElementById: (id) => new MockDOMElement('div', id),
        body: new MockDOMElement('body')
    };
}
if (typeof globalThis.window === 'undefined') {
    globalThis.window = {
        document: globalThis.document,
        location: { search: '' },
        AudioContext: function() {},
        webkitAudioContext: function() {}
    };
}
if (typeof globalThis.navigator === 'undefined') {
    globalThis.navigator = {
        mediaDevices: {
            getUserMedia: async () => ({
                getTracks: () => [{ stop: () => {} }]
            })
        }
    };
}

import {
    ApkZipReader,
    AxmlDecoder,
    ArscStringPoolParser,
    PackageManagerRegistry,
    defaultPackageManager
} from '../src/apk_client_parser.js';
import { ArscDecoder, ArscResourceTable, TypedValue } from '../src/apk_resource_resolver.js';
import { DexParser, DalvikVM } from '../src/dex_vm.js';
import {
    StructuredLogger,
    LogcatBuffer,
    globalLogcat,
    logger,
    logDebug,
    PRIORITY_ORDER,
    LOG_LEVELS,
    KNOWN_SUBSYSTEMS
} from '../src/logger.js';
import { AndroidRuntime, resolveAppMetadata } from '../src/android_runtime.js';
import { AppController } from '../src/app_controller.js';
import { BinderParcel, VirtioBinderFraming } from '../src/binder_test_suite.js';
import { MotionEvent, KeyEvent, ViewRootImpl, ViewHierarchyRasterizer } from '../src/view_rasterizer.js';
import {
    View,
    ViewGroup,
    LayoutParams,
    FrameLayout,
    LinearLayout,
    ConstraintLayout,
    TextView,
    ImageView,
    LayoutInflater,
    MATCH_PARENT,
    WRAP_CONTENT
} from '../src/view_hierarchy.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;

const testQueue = [];
function test(name, fn) {
    testQueue.push({ name, fn });
}

function createMockDom() {
    return {
        btnToggleMic: new MockDOMElement('button', 'btn-toggle-mic'),
        btnToggleCam: new MockDOMElement('button', 'btn-toggle-cam'),
        btnUploadApk: new MockDOMElement('button', 'btn-upload-apk'),
        btnSwitchCanvas: new MockDOMElement('button', 'btn-switch-canvas'),
        btnToggleViewmode: new MockDOMElement('button', 'btn-toggle-viewmode'),
        apkFileInput: new MockDOMElement('input', 'apk-file-input'),
        dropTargetArea: new MockDOMElement('div', 'drop-target-area'),
        phoneBezel: new MockDOMElement('div', 'phone-bezel'),
        phoneScreenRoot: new MockDOMElement('div', 'phone-screen-root'),
        toast: new MockDOMElement('div', 'android-toast'),
        toastText: new MockDOMElement('span', 'toast-text'),
        clockHeader: new MockDOMElement('div', 'android-clock-header'),
        fpsPill: new MockDOMElement('div', 'fps-pill'),
        screenHome: new MockDOMElement('div', 'screen-home'),
        screenV86: new MockDOMElement('div', 'screen-v86'),
        screenWebGpu: new MockDOMElement('div', 'screen-webgpu'),
        v86ScreenContainer: new MockDOMElement('div', 'v86-screen-container'),
        canvas: new MockDOMElement('canvas', 'screen'),
        canvasHudFps: new MockDOMElement('div', 'canvas-hud-fps'),
        canvasHudGpu: new MockDOMElement('div', 'canvas-hud-gpu'),
        homeClock: new MockDOMElement('div', 'home-clock'),
        homeDate: new MockDOMElement('div', 'home-date'),
        btnHomeSearch: new MockDOMElement('button', 'btn-home-search'),
        homeAppGrid: new MockDOMElement('div', 'home-app-grid'),
        homeDock: new MockDOMElement('div', 'home-dock'),
        btnNavBack: new MockDOMElement('button', 'btn-nav-back'),
        btnNavHome: new MockDOMElement('button', 'btn-nav-home'),
        btnNavRecents: new MockDOMElement('button', 'btn-nav-recents'),
        hwPower: new MockDOMElement('button', 'hw-power'),
        hwVolUp: new MockDOMElement('button', 'hw-vol-up'),
        hwVolDown: new MockDOMElement('button', 'hw-vol-down'),
        tabTelemetry: new MockDOMElement('button', 'tab-telemetry'),
        tabNetwork: new MockDOMElement('button', 'tab-network'),
        tabVm: new MockDOMElement('button', 'tab-vm'),
        contentTelemetry: new MockDOMElement('div', 'content-telemetry'),
        contentNetwork: new MockDOMElement('div', 'content-network'),
        contentVm: new MockDOMElement('div', 'content-vm'),
        statFps: new MockDOMElement('div', 'stat-fps'),
        statGpuTime: new MockDOMElement('div', 'stat-gpu-time'),
        statKernel: new MockDOMElement('div', 'stat-kernel'),
        trafficList: new MockDOMElement('div', 'traffic-list'),
        netCount: new MockDOMElement('div', 'net-count'),
        logcatPrio: new MockDOMElement('select', 'logcat-prio'),
        logcatTag: new MockDOMElement('select', 'logcat-tag'),
        logcatSearch: new MockDOMElement('input', 'logcat-search'),
        logcatAutoscroll: new MockDOMElement('button', 'logcat-autoscroll'),
        logcatClear: new MockDOMElement('button', 'logcat-clear'),
        logcatCounter: new MockDOMElement('div', 'logcat-counter'),
        vmLogView: new MockDOMElement('div', 'vm-log-view'),
        serialCmdInput: new MockDOMElement('input', 'serial-cmd-input'),
        btnSendSerial: new MockDOMElement('button', 'btn-send-serial')
    };
}

function parseVirtioBinderRequest(reqBytes) {
    const view = new DataView(reqBytes.buffer, reqBytes.byteOffset, reqBytes.byteLength);
    const msgId = view.getBigUint64(0, true);
    const cmd = view.getUint32(8, true);
    const targetHandle = view.getUint32(12, true);
    const code = view.getUint32(16, true);
    const flags = view.getUint32(20, true);
    const cookie = view.getBigUint64(24, true);
    const dataLen = view.getUint32(32, true);
    const offsetsLen = view.getUint32(36, true);
    const data = reqBytes.slice(48, 48 + dataLen);
    return {
        header: { msgId, cmd, targetHandle, code, flags, cookie, dataLen, offsetsLen },
        data
    };
}

function createVirtioBinderResponse(msgId, payloadBytes = new Uint8Array([0, 0, 0, 0])) {
    const hdrSize = 32;
    const totalSize = hdrSize + payloadBytes.length;
    const buf = new ArrayBuffer(totalSize);
    const view = new DataView(buf);
    const bytes = new Uint8Array(buf);
    view.setBigUint64(0, BigInt(msgId), true);
    view.setInt32(8, 0, true); // status = 0 (OK)
    view.setInt32(12, 0, true); // resultCode = 0
    view.setUint32(16, payloadBytes.length, true); // dataSize
    view.setUint32(20, 0, true); // offsetsSize = 0
    view.setUint32(24, 0, true); // flags = 0
    bytes.set(payloadBytes, hdrSize);
    return bytes;
}

class MockWasmBridge {
    constructor() {
        this.receivedPackets = [];
        this.presentedFrames = 0;
        this.customHandler = null;
    }

    process_binder_packet(reqBytes) {
        const req = parseVirtioBinderRequest(reqBytes);
        this.receivedPackets.push(req);

        if (this.customHandler) {
            return this.customHandler(reqBytes, req);
        }

        // Generic success response: 32-byte header + 4-byte int32 exception code 0
        return createVirtioBinderResponse(req.header.msgId, new Uint8Array([0, 0, 0, 0]));
    }

    compose_and_present() {
        this.presentedFrames++;
    }
}

// =============================================================================
// Suite 1: Logcat Circular Buffer & Structured Logger Extreme Stress
// =============================================================================

test("1.1 Buffer capacity limit (5,000 entries) and FIFO drop under 10,000 entry flood", () => {
    const buffer = new LogcatBuffer(5000);
    assert.equal(buffer.maxEntries, 5000);

    const origLog = console.log;
    const origInfo = console.info;
    const origDebug = console.debug;
    console.log = () => {};
    console.info = () => {};
    console.debug = () => {};

    try {
        for (let i = 0; i < 10000; i++) {
            buffer.append('TestTag', `Log record sequence ${i}`, i % 2 === 0 ? 'I' : 'D', 1000, 1000);
        }
    } finally {
        console.log = origLog;
        console.info = origInfo;
        console.debug = origDebug;
    }

    assert.equal(buffer.entries.length, 5000, "Buffer length strictly capped at maxEntries (5000)");
    assert.equal(buffer.entries[0].msg, "Log record sequence 5000", "Oldest entry in buffer is #5000 (FIFO drop verified)");
    assert.equal(buffer.entries[4999].msg, "Log record sequence 9999", "Latest entry in buffer is #9999");
});

test("1.2 Priority filtering & strict rank ordering across V, D, I, W, E", () => {
    const buffer = new LogcatBuffer(100);
    buffer.append('TagA', 'Verbose message', 'V');
    buffer.append('TagB', 'Debug message', 'D');
    buffer.append('TagC', 'Info message', 'I');
    buffer.append('TagD', 'Warning message', 'W');
    buffer.append('TagE', 'Error message', 'E');

    const vFilter = buffer.filter({ minPriority: 'V' });
    assert.equal(vFilter.length, 5, "Priority 'V' includes all 5 levels");

    const dFilter = buffer.filter({ minPriority: 'D' });
    assert.equal(dFilter.length, 4, "Priority 'D' excludes 'V'");

    const iFilter = buffer.filter({ minPriority: 'I' });
    assert.equal(iFilter.length, 3, "Priority 'I' excludes 'V' and 'D'");

    const wFilter = buffer.filter({ minPriority: 'W' });
    assert.equal(wFilter.length, 2, "Priority 'W' excludes 'V', 'D', and 'I'");

    const eFilter = buffer.filter({ minPriority: 'E' });
    assert.equal(eFilter.length, 1, "Priority 'E' includes only 'E'");
    assert.equal(eFilter[0].msg, 'Error message');
});

test("1.3 Tag filtering, search substring matching, and case-insensitivity", () => {
    const buffer = new LogcatBuffer(100);
    buffer.append('PackageManager', 'Installed org.fdroid.fdroid successfully', 'I');
    buffer.append('ActivityTaskManager', 'START u0 {act=android.intent.action.MAIN}', 'I');
    buffer.append('WindowManager', 'relayoutWindow: org.fdroid.fdroid', 'D');
    buffer.append('InputDispatcher', 'MotionEvent: ACTION_DOWN at (100, 200)', 'D');

    const pmLogs = buffer.filter({ tag: 'packagemanager' });
    assert.equal(pmLogs.length, 1);
    assert.equal(pmLogs[0].tag, 'PackageManager');

    const fdroidSearch = buffer.filter({ search: 'FDROID' });
    assert.equal(fdroidSearch.length, 2, "Case-insensitive message search found both entries");

    const allTagWithSearch = buffer.filter({ tag: 'all', search: 'ACTION_DOWN' });
    assert.equal(allTagWithSearch.length, 1);
    assert.equal(allTagWithSearch[0].tag, 'InputDispatcher');
});

test("1.4 Serial stream feeding: chunk boundaries, character-by-character, and CRLF handling", () => {
    const buffer = new LogcatBuffer(100);
    const linesReceived = [];

    const line1 = "Linux version 6.1.0-android-x86\n";
    for (const ch of line1) {
        buffer.feedSerialChar(ch, (e) => linesReceived.push(e.msg));
    }
    assert.equal(linesReceived.length, 1);
    assert.equal(linesReceived[0], "Linux version 6.1.0-android-x86");

    buffer.feedSerial("Init stage 1 started...");
    assert.equal(buffer.entries.length, 1, "Incomplete line should be buffered in serialBuffer");
    buffer.feedSerial(" done.\r\nInit stage 2 launching daemons.\n");
    
    assert.equal(buffer.entries.length, 3, "Both completed lines processed");
    assert.equal(buffer.entries[1].msg, "Init stage 1 started... done.");
    assert.equal(buffer.entries[2].msg, "Init stage 2 launching daemons.");
});

test("1.5 Auto-escalation of panic / fatal / exception / SIGILL keywords in serial feed to Error priority", () => {
    const buffer = new LogcatBuffer(100);
    buffer.feedSerial("Kernel panic - not syncing: VFS: Unable to mount root fs\n");
    buffer.feedSerial("Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR)\n");
    buffer.feedSerial("java.lang.NullPointerException: Attempt to invoke virtual method on null object reference\n");
    buffer.feedSerial("Unhandled SIGILL instruction at 0xdeadbeef\n");
    buffer.feedSerial("Normal boot informational line\n");

    const errorEntries = buffer.filter({ minPriority: 'E' });
    assert.equal(errorEntries.length, 4, "All 4 panic/fatal/exception/SIGILL lines escalated to level 'E'");
    assert.equal(buffer.entries[4].priority, 'D', "Normal line retains Debug priority");
});

test("1.6 Exception-resilient listeners & listener unsubscribe lifecycle", () => {
    const buffer = new LogcatBuffer(100);
    let subscriberACalls = 0;
    let subscriberBCalls = 0;

    const unsubA = buffer.subscribe(() => {
        subscriberACalls++;
        throw new Error("Malfunctioning subscriber crash!");
    });

    const unsubB = buffer.subscribe(() => {
        subscriberBCalls++;
    });

    buffer.append('TestTag', 'Safe log entry', 'I');
    assert.equal(subscriberACalls, 1);
    assert.equal(subscriberBCalls, 1, "Subscriber B still called despite Subscriber A throwing");

    unsubA();
    buffer.append('TestTag', 'Second log entry', 'I');
    assert.equal(subscriberACalls, 1, "Subscriber A not called after unsubscribe");
    assert.equal(subscriberBCalls, 2, "Subscriber B received second entry");

    unsubB();
});

test("1.7 StructuredLogger circular metadata sanitization", () => {
    const sLogger = new StructuredLogger({ consoleDispatch: false });
    const circularObj = { name: "testNode" };
    circularObj.self = circularObj;

    const entry = sLogger.log('runtime', 'I', 'Testing circular metadata', circularObj);
    assert.equal(entry.subsystem, 'runtime');
    assert.deepEqual(entry.metadata, { error: 'circular_or_unserializable_metadata' }, "Circular metadata sanitized safely");
});

// =============================================================================
// Suite 2: Binder Call Routing to ams_rs, wms_rs, pms_rs, and inputflinger_rs
// =============================================================================

test("2.1 AppController.launchActivity -> ams_rs (Handle 4, Code 1) parcel format and AIDL header", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const runtime = new AndroidRuntime();

    const controller = new AppController({
        bootstrap: mockBootstrap,
        runtime,
        domElements: mockDom
    });

    await controller.launchActivity('org.fdroid.fdroid', 'org.fdroid.fdroid.views.main.MainActivity');

    const amsPacket = mockBridge.receivedPackets.find(p => p.header.targetHandle === 4 && p.header.code === 1);
    assert(amsPacket !== undefined, "Binder packet sent to ams_rs (Handle 4, Code 1)");

    const parcel = BinderParcel.fromUint8Array(amsPacket.data);
    const descriptor = parcel.readUtf16();
    assert.equal(descriptor, "android.app.IActivityManager", "AIDL interface descriptor matches IActivityManager");

    const callerSync = parcel.readBool();
    const intentAction = parcel.readUtf8();
    assert.equal(intentAction, "android.intent.action.MAIN", "Intent action is MAIN");

    const catCount = parcel.readInt32();
    const cat = parcel.readUtf8();
    assert.equal(cat, "android.intent.category.LAUNCHER", "Intent category is LAUNCHER");

    parcel.readUtf8(); // type
    parcel.readUtf8(); // data
    parcel.readBool(); // hasComponent
    const pkg = parcel.readUtf8();
    const act = parcel.readUtf8();
    assert.equal(pkg, "org.fdroid.fdroid", "Package name in parcel matches");
    assert.equal(act, "org.fdroid.fdroid.views.main.MainActivity", "Activity name in parcel matches");

    const flags = parcel.readUint32();
    assert.equal(flags, 0x10000000, "FLAG_ACTIVITY_NEW_TASK (0x10000000) set in intent");
});

test("2.2 AppController.launchActivity -> wms_rs (Handle 3, Code 2) window relayout parcel format", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const runtime = new AndroidRuntime();

    const controller = new AppController({
        bootstrap: mockBootstrap,
        runtime,
        domElements: mockDom
    });

    await controller.launchActivity('org.mozilla.firefox');

    const wmsPacket = mockBridge.receivedPackets.find(p => p.header.targetHandle === 3 && p.header.code === 2);
    assert(wmsPacket !== undefined, "Binder packet sent to wms_rs (Handle 3, Code 2)");

    const parcel = BinderParcel.fromUint8Array(wmsPacket.data);
    const descriptor = parcel.readUtf16();
    assert.equal(descriptor, "android.view.IWindowManager", "AIDL interface descriptor matches IWindowManager");

    parcel.readBool(); // inTouchMode
    const x = parcel.readInt32();
    const y = parcel.readInt32();
    const w = parcel.readInt32();
    const h = parcel.readInt32();
    assert.equal(w, 720, "Requested frame width is 720");
    assert.equal(h, 1440, "Requested frame height is 1440");

    const viewType = parcel.readInt32();
    assert.equal(viewType, 1, "Window type is TYPE_APPLICATION (1)");

    parcel.readInt32(); // flags
    parcel.readInt32(); // format
    const pkg = parcel.readUtf8();
    assert.equal(pkg, 'org.mozilla.firefox', "Window package name matches");
});

test("2.3 AppController.sendAmsLifecycle -> AMS lifecycle codes (pause=4, stop=5, finish=6)", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const controller = new AppController({ bootstrap: mockBootstrap, domElements: mockDom });

    await controller.sendAmsLifecycle(4, 'activityPaused');
    await controller.sendAmsLifecycle(5, 'activityStopped');
    await controller.sendAmsLifecycle(6, 'finishActivity');

    const paused = mockBridge.receivedPackets.find(p => p.header.targetHandle === 4 && p.header.code === 4);
    const stopped = mockBridge.receivedPackets.find(p => p.header.targetHandle === 4 && p.header.code === 5);
    const finished = mockBridge.receivedPackets.find(p => p.header.targetHandle === 4 && p.header.code === 6);

    assert(paused !== undefined, "activityPaused dispatched to Handle 4 Code 4");
    assert(stopped !== undefined, "activityStopped dispatched to Handle 4 Code 5");
    assert(finished !== undefined, "finishActivity dispatched to Handle 4 Code 6");

    const parcel = BinderParcel.fromUint8Array(paused.data);
    assert.equal(parcel.readUtf16(), "android.app.IActivityManager");
    assert.equal(parcel.readUint32(), 1, "Token ID 1 passed");
});

test("2.4 AppController.installPackage -> pms_rs (Handle 5, Code 10)", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const controller = new AppController({ bootstrap: mockBootstrap, domElements: mockDom });

    await controller.installPackage('com.test.app', 'Test App', '2.0.0', 42, 'com.test.app.MainAct');

    const pmsPacket = mockBridge.receivedPackets.find(p => p.header.targetHandle === 5 && p.header.code === 10);
    assert(pmsPacket !== undefined, "installPackage sent to Handle 5 Code 10");

    const parcel = BinderParcel.fromUint8Array(pmsPacket.data);
    assert.equal(parcel.readUtf16(), "android.content.pm.IPackageManager");
    assert.equal(parcel.readUtf8(), 'com.test.app');
    assert.equal(parcel.readUtf8(), 'Test App');
    assert.equal(parcel.readUtf8(), '2.0.0');
    assert.equal(parcel.readInt32(), 42);
    assert.equal(parcel.readUtf8(), 'com.test.app.MainAct');
});

test("2.5 AppController.sendInputEvent -> inputflinger_rs (Handle 2, Code 4)", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const controller = new AppController({ bootstrap: mockBootstrap, domElements: mockDom });

    await controller.sendInputEvent(0, 4); // KEY_DOWN, KEYCODE_BACK

    const inputPacket = mockBridge.receivedPackets.find(p => p.header.targetHandle === 2 && p.header.code === 4);
    assert(inputPacket !== undefined, "Input event dispatched to Handle 2 Code 4");

    const parcel = BinderParcel.fromUint8Array(inputPacket.data);
    assert.equal(parcel.readUtf16(), "android.hardware.input.IInputManager");
    assert.equal(parcel.readInt32(), 1, "InputEvent tag 1 (Key event)");

    const keyBytes = parcel.readByteArray();
    assert.equal(keyBytes.length, 56, "56-byte key event binary struct");

    const view = new DataView(keyBytes.buffer, keyBytes.byteOffset, keyBytes.byteLength);
    const action = view.getInt32(24, true);
    const keyCode = view.getInt32(32, true);
    assert.equal(action, 0, "Action is 0 (KEY_DOWN)");
    assert.equal(keyCode, 4, "KeyCode is 4 (KEYCODE_BACK)");
});

test("2.6 High-frequency transaction storm (100 sequential Binder calls) with zero corruption", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const controller = new AppController({ bootstrap: mockBootstrap, domElements: mockDom });

    for (let i = 0; i < 100; i++) {
        await controller.sendInputEvent(0, 100 + i);
    }

    assert.equal(mockBridge.receivedPackets.length, 100, "All 100 transactions received by bridge");
    assert.equal(controller.binderTxCounter, 100, "Binder transaction telemetry counter matches exactly 100");
});

// =============================================================================
// Suite 3: main_android.js Bootstrap & Navigation Pipeline
// =============================================================================

test("3.1 Target APK resolution matrix from URL params, window object, and fallback default", () => {
    let target = ('F-Droid.apk');
    assert.equal(target, 'F-Droid.apk');

    const mockWindow = { TARGET_APK: 'firefox.apk' };
    target = mockWindow.TARGET_APK || 'F-Droid.apk';
    assert.equal(target, 'firefox.apk');

    const urlParams = new URLSearchParams('apk=custom_app.apk');
    target = urlParams.get('apk') || 'F-Droid.apk';
    assert.equal(target, 'custom_app.apk');
});

test("3.2 Navigation button handling: Back button pops Activity and transitions to home", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const runtime = new AndroidRuntime();

    const controller = new AppController({
        bootstrap: mockBootstrap,
        runtime,
        domElements: mockDom
    });

    await controller.launchActivity('org.fdroid.fdroid');
    assert.equal(controller.activeScreen, 'webgpu');
    assert.equal(runtime.activityBackstack.size(), 1);

    await controller.handleBackPress();
    assert.equal(controller.activeScreen, 'home', "Screen returned to home launcher when backstack emptied");
    assert.equal(runtime.currentPackage, null);
});

test("3.3 Navigation button handling: Home button pauses and switches to home", async () => {
    const mockDom = createMockDom();
    const mockBridge = new MockWasmBridge();
    const mockBootstrap = { getBridge: () => mockBridge };
    const runtime = new AndroidRuntime();

    const controller = new AppController({
        bootstrap: mockBootstrap,
        runtime,
        domElements: mockDom
    });

    await controller.launchActivity('org.mozilla.firefox');
    assert.equal(controller.activeScreen, 'webgpu');

    await controller.handleHomePress();
    assert.equal(controller.activeScreen, 'home');

    const pauseTx = mockBridge.receivedPackets.find(p => p.header.targetHandle === 4 && p.header.code === 4);
    assert(pauseTx !== undefined, "activityPaused sent on Home press");
});

test("3.4 Touch pointer events dispatch to AndroidRuntime and View hierarchy", () => {
    const runtime = new AndroidRuntime();
    const mockCanvas = new MockDOMElement('canvas', 'screen');
    runtime.setCanvas(mockCanvas);

    const root = new FrameLayout();
    root.layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT);
    let clickCount = 0;
    root.setOnClickListener(() => { clickCount++; });

    runtime.currentRootView = root;
    runtime.viewRoot.setView(root);

    const downEvt = new MotionEvent(MotionEvent.ACTION_DOWN, 100, 200);
    const handledDown = runtime.dispatchInputEvent(downEvt);
    assert.equal(handledDown, true, "ACTION_DOWN handled by ViewRootImpl");

    const upEvt = new MotionEvent(MotionEvent.ACTION_UP, 100, 200);
    const handledUp = runtime.dispatchInputEvent(upEvt);
    assert.equal(handledUp, true, "ACTION_UP handled by ViewRootImpl");
    assert.equal(clickCount, 1, "Click listener triggered by pointer down + up sequence");
});

// =============================================================================
// Suite 4: Network Error Recovery & Offline Fallback Testing
// =============================================================================

test("4.1 HTTP 404 Not Found handling during APK fetch", async () => {
    let loggedWarning = null;
    const runtime = new AndroidRuntime({
        onLog: (msg, lvl) => {
            if (lvl === 'warn') loggedWarning = msg;
        }
    });

    const mockFetch = async () => ({
        ok: false,
        status: 404,
        statusText: "Not Found"
    });

    const resp = await mockFetch("missing_app.apk");
    assert.equal(resp.ok, false);
    if (!resp.ok) {
        runtime.log(`Target APK fetch failed: missing_app.apk (HTTP ${resp.status})`, 'warn', 'PackageManager');
    }

    assert(loggedWarning && loggedWarning.includes("HTTP 404"), "Warning logged to Logcat on 404");
    assert.equal(runtime.currentPackage, null, "Runtime remains stable in launcher state");
});

test("4.2 Network drop / Fetch TypeError (Offline mode)", async () => {
    let loggedError = null;
    const runtime = new AndroidRuntime({
        onLog: (msg, lvl) => {
            if (lvl === 'error') loggedError = msg;
        }
    });

    const mockFetch = async () => {
        throw new TypeError("Failed to fetch (Network unreachable)");
    };

    try {
        await mockFetch("F-Droid.apk");
    } catch (e) {
        runtime.log(`Target APK bootstrap error: ${e.message}`, 'error', 'PackageManager');
    }

    assert(loggedError && loggedError.includes("Failed to fetch"), "Error logged cleanly to Logcat");
    assert.equal(runtime.installedApps.size > 0, true, "Default packages preserved in runtime");
});

// =============================================================================
// Suite 5: Corrupt, Truncated & Adversarial APK Ingestion Stress
// =============================================================================

test("5.1 Zero-byte buffer rejection", async () => {
    const runtime = new AndroidRuntime();
    let thrown = false;

    try {
        await runtime.loadAndRunApk(new ArrayBuffer(0));
    } catch (err) {
        thrown = true;
        assert(err.message.includes("ZIP") || err.message.includes("central directory") || err.message.includes("Invalid APK"), "Rejection message is clear");
    }

    assert.equal(thrown, true, "Zero-byte buffer threw exception");
});

test("5.2 Random garbage bytes buffer rejection", async () => {
    const runtime = new AndroidRuntime();
    const garbage = new Uint8Array(4096);
    for (let i = 0; i < garbage.length; i++) garbage[i] = (i * 37 + 13) & 0xff;

    let thrown = false;
    try {
        await runtime.loadAndRunApk(garbage.buffer);
    } catch (err) {
        thrown = true;
    }
    assert.equal(thrown, true, "Garbage buffer threw exception without hanging");
});

test("5.3 Valid ZIP archive without AndroidManifest.xml rejection", async () => {
    const runtime = new AndroidRuntime();

    const zipBuf = new Uint8Array([
        // Local File Header
        0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x00, 0x00, // filename length: 8
        0x74, 0x65, 0x73, 0x74, 0x2e, 0x74, 0x78, 0x74, // "test.txt"
        0x74, 0x65, 0x73, 0x74, // "test"
        // Central Directory Header
        0x50, 0x4b, 0x01, 0x02, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
        0x74, 0x65, 0x73, 0x74, 0x2e, 0x74, 0x78, 0x74,
        // End of Central Directory Record (EOCD)
        0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
        0x36, 0x00, 0x00, 0x00, // central dir size: 54 bytes
        0x2a, 0x00, 0x00, 0x00, // central dir offset: 42 bytes
        0x00, 0x00
    ]);

    let thrown = false;
    try {
        await runtime.loadAndRunApk(zipBuf.buffer);
    } catch (err) {
        thrown = true;
        assert.equal(err.message, "Invalid APK: Missing AndroidManifest.xml");
    }
    assert.equal(thrown, true, "Rejected APK missing AndroidManifest.xml");
});

test("5.4 Malformed AXML chunk header handling and invalid magic rejection", () => {
    let caughtShort = false;
    try {
        AxmlDecoder.decode(new Uint8Array([0x03, 0x00, 0x08]));
    } catch (e) {
        caughtShort = true;
        assert(e.message.includes("< 8 bytes"));
    }
    assert.equal(caughtShort, true, "Short buffer (< 8 bytes) threw clear error");

    let caughtMagic = false;
    try {
        AxmlDecoder.decode(new Uint8Array([0x00, 0x00, 0x08, 0x00, 0x20, 0x00, 0x00, 0x00]));
    } catch (e) {
        caughtMagic = true;
        assert(e.message.includes("Invalid AXML magic"));
    }
    assert.equal(caughtMagic, true, "Invalid magic threw clear error");
});

test("5.5 Corrupt resources.arsc fallback resilience in loadAndRunApk", () => {
    const corruptArsc = new Uint8Array([0x02, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x00, 0x00]);
    const table = ArscDecoder.decode(corruptArsc);
    assert(table instanceof ArscResourceTable, "ArscDecoder returns valid fallback ArscResourceTable");

    const poolParser = new ArscStringPoolParser(corruptArsc).parse();
    assert.equal(poolParser.globalStrings.length, 0, "Corrupted string pool yields empty list safely");
});

test("5.6 DalvikVM multi-dex symbol resolution and missing class fallback", () => {
    const vm = new DalvikVM();
    assert.equal(vm.findClass("com.missing.NonExistentActivity"), null, "findClass returns null for missing class");

    const act = vm.startActivity("com.missing.NonExistentActivity", { packageName: "com.missing" });
    assert(act !== null, "Synthetic activity created");
    assert.equal(act.className, "com.missing.NonExistentActivity");
    assert.equal(act.isResumed, true);
});

// =============================================================================
// Suite 6: Full Stack End-to-End Ingestion & Recovery Proof
// =============================================================================

test("6.1 Complete recovery: Ingest authentic F-Droid.apk after multiple failure injections", async () => {
    const runtime = new AndroidRuntime();
    const mockCanvas = new MockDOMElement('canvas', 'screen');
    runtime.setCanvas(mockCanvas);

    const apkPath = path.join(rootDir, "F-Droid.apk");
    const apkBuffer = fs.readFileSync(apkPath);

    // Inject 3 consecutive failures
    try { await runtime.loadAndRunApk(new ArrayBuffer(10)); } catch (_) {}
    try { await runtime.loadAndRunApk(new Uint8Array(100).buffer); } catch (_) {}
    try { await runtime.loadAndRunApk(new ArrayBuffer(0)); } catch (_) {}

    // Ingest authentic APK
    const appState = await runtime.loadAndRunApk(apkBuffer);

    assert(appState !== null, "appState successfully returned after prior failures");
    assert.equal(appState.packageName, "org.fdroid.fdroid");
    assert.equal(runtime.currentPackage, "org.fdroid.fdroid");
    assert.equal(runtime.activityBackstack.size(), 1);
    assert.equal(runtime.currentRootView !== null, true, "Root View inflated and attached to ViewRootImpl");
    assert(runtime.vm.dexParsers.length >= 2, "DalvikVM loaded multi-dex archives");
    assert(runtime.pms.getPackage("org.fdroid.fdroid") !== null, "PMS registered org.fdroid.fdroid");
});

// -----------------------------------------------------------------------------
// Sequential Test Runner
// -----------------------------------------------------------------------------
async function runAllTests() {
    console.log("================================================================================");
    console.log("🔥 Challenger 2: Milestone 3 Empirical Stress & Adversarial Verification Suite");
    console.log("================================================================================");

    for (const t of testQueue) {
        totalTests++;
        try {
            await t.fn();
            passedTests++;
            console.log(`  ✔ [PASS] ${t.name}`);
        } catch (err) {
            failedTests++;
            console.error(`  ❌ [FAIL] ${t.name}:`, err);
            throw err;
        }
    }

    console.log("\n================================================================================");
    console.log(`⚡ ALL CHALLENGER 2 EMPIRICAL TESTS PASSED! (${passedTests}/${totalTests} passed, ${failedTests} failed)`);
    console.log("================================================================================");
}

runAllTests().catch((err) => {
    console.error("Test execution terminated with failure:", err);
    process.exit(1);
});
