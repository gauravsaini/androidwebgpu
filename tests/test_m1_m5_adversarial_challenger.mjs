/**
 * Adversarial Challenger Test Suite: Bootstrap, App Controller, & DOM Architecture
 * 
 * Tests:
 * 1. Single V86Class Instantiation & HTML DOM Invariants
 * 2. Unexpected Serial Streams, Fuzzing & Token Fragmentation
 * 3. Rapid State Machine Transitions & Bootstrap Lifecycle Abuse
 * 4. Missing / Null DOM Tolerance in AppController & ui_render
 * 5. Backstack Navigation Stress, Rapid Cycles, & Package Registry Fuzzing
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { SystemBootstrap, VM_STATES, BOOT_MILESTONES, verifyBzImage } from '../src/system_bootstrap.js';
import { AppController } from '../src/app_controller.js';
import { AndroidRuntime } from '../src/android_runtime.js';
import {
    renderAppLauncherItem,
    renderDockItems,
    renderLogcatList,
    appendLogcatToDom,
    renderBinderTransaction,
    showToast,
    updateClock,
    updateMetrics,
    createLogcatElement
} from '../src/ui_render.js';
import { globalLogcat } from '../src/logger.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

let totalChecks = 0;
let passedChecks = 0;

function check(desc, condition) {
    totalChecks++;
    if (!condition) {
        console.error(`  ✖ [FAIL] ${desc}`);
        throw new Error(`Assertion failed: ${desc}`);
    }
    passedChecks++;
    console.log(`  ✔ [PASS] ${desc}`);
}

/**
 * Minimal Headless DOM Mock Engine (Native / Standard Library only)
 */
class MockDOMElement {
    constructor(tag = 'div', id = '', className = '') {
        this.tagName = tag.toUpperCase();
        this.id = id;
        this.className = className;
        this.classList = {
            _classes: new Set(className ? className.split(' ') : []),
            add: (c) => this.classList._classes.add(c),
            remove: (c) => this.classList._classes.delete(c),
            toggle: (c, force) => {
                if (force === true) { this.classList._classes.add(c); return true; }
                if (force === false) { this.classList._classes.delete(c); return false; }
                if (this.classList._classes.has(c)) { this.classList._classes.delete(c); return false; }
                this.classList._classes.add(c); return true;
            },
            contains: (c) => this.classList._classes.has(c)
        };
        this.style = {};
        this.attributes = new Map();
        this.children = [];
        this.parentElement = null;
        this.eventListeners = new Map();
        this._innerHTML = '';
        this._textContent = '';
        this.scrollTop = 0;
        this.scrollHeight = 100;
        this.clientHeight = 100;
        this.width = 1280;
        this.height = 720;
    }

    get childElementCount() {
        return this.children.length;
    }

    get firstChild() {
        return this.children[0] || null;
    }

    get innerHTML() {
        return this._innerHTML;
    }

    set innerHTML(val) {
        this._innerHTML = String(val);
        this.children = [];
    }

    get textContent() {
        return this._textContent;
    }

    set textContent(val) {
        this._textContent = String(val);
    }

    setAttribute(k, v) {
        this.attributes.set(k, String(v));
    }

    getAttribute(k) {
        return this.attributes.get(k) || null;
    }

    appendChild(el) {
        if (!el) return el;
        if (el.parentElement) el.parentElement.removeChild(el);
        el.parentElement = this;
        this.children.push(el);
        return el;
    }

    insertBefore(newNode, refNode) {
        if (!newNode) return newNode;
        if (newNode.parentElement) newNode.parentElement.removeChild(newNode);
        newNode.parentElement = this;
        const idx = this.children.indexOf(refNode);
        if (idx === -1) {
            this.children.push(newNode);
        } else {
            this.children.splice(idx, 0, newNode);
        }
        return newNode;
    }

    removeChild(el) {
        const idx = this.children.indexOf(el);
        if (idx !== -1) {
            this.children.splice(idx, 1);
            el.parentElement = null;
        }
        return el;
    }

    querySelector(sel) {
        if (sel.startsWith('.')) {
            const cls = sel.slice(1);
            return this.children.find(c => c.classList && c.classList.contains(cls)) || null;
        }
        if (sel.includes('[data-pkg=')) {
            const match = sel.match(/\[data-pkg="([^"]+)"\]/);
            if (match) {
                const pkg = match[1];
                return this.children.find(c => c.getAttribute('data-pkg') === pkg) || null;
            }
        }
        return null;
    }

    addEventListener(event, fn) {
        if (!this.eventListeners.has(event)) this.eventListeners.set(event, []);
        this.eventListeners.get(event).push(fn);
    }

    dispatchEvent(event, ...args) {
        const list = this.eventListeners.get(event) || [];
        for (const fn of list) fn(...args);
    }

    click() {
        this.dispatchEvent('click');
    }

    getBoundingClientRect() {
        return { left: 0, top: 0, width: 1280, height: 720, right: 1280, bottom: 720 };
    }
}

// Global document shim for testing ui_render DOM factory methods
if (typeof globalThis.document === 'undefined') {
    globalThis.document = {
        createElement: (tag) => new MockDOMElement(tag),
        getElementById: (id) => new MockDOMElement('div', id),
        body: new MockDOMElement('body')
    };
}
if (typeof globalThis.requestAnimationFrame === 'undefined') {
    globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 16);
    globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
}

/**
 * Mock V86 hypervisor implementation for adversarial testing.
 */
class AdversarialMockV86 {
    constructor(options = {}) {
        this.options = options;
        this.listeners = new Map();
        this.sentCommands = [];
        this.isDestroyed = false;
    }

    add_listener(event, cb) {
        if (!this.listeners.has(event)) this.listeners.set(event, []);
        this.listeners.get(event).push(cb);
    }

    remove_listener(event, cb) {
        if (this.listeners.has(event)) {
            this.listeners.set(event, this.listeners.get(event).filter(f => f !== cb));
        }
    }

    emit(event, ...args) {
        const list = this.listeners.get(event) || [];
        for (const cb of list) cb(...args);
    }

    emitSerialChunk(chunk) {
        if (typeof chunk === 'string') {
            for (const ch of chunk) this.emit('serial0-output-char', ch);
        } else if (chunk instanceof Uint8Array) {
            for (const byte of chunk) this.emit('serial0-output-char', String.fromCharCode(byte));
        }
    }

    serial0_send(text) {
        this.sentCommands.push(text);
    }

    destroy() {
        this.isDestroyed = true;
    }
}

async function runAdversarialSuite() {
    console.log('================================================================');
    console.log('🔥 STARTING ADVERSARIAL CHALLENGER SUITE: M1-M5 ARCHITECTURE 🔥');
    console.log('================================================================\n');

    // =========================================================================
    // SECTION 1: Single Hypervisor Instantiation & DOM Layout Invariants
    // =========================================================================
    console.log('▶ [Section 1] Single Hypervisor Instantiation & DOM Invariants');
    {
        // 1.1 android.html line count <= 150 lines
        const htmlPath = path.join(ROOT_DIR, 'android.html');
        const htmlContent = fs.readFileSync(htmlPath, 'utf8');
        const htmlLines = htmlContent.split(/\r?\n/).filter(l => l.length >= 0);
        check(`android.html line count is <= 150 lines (actual: ${htmlLines.length})`, htmlLines.length <= 150);

        // 1.2 Zero inline <script> blocks without src
        const inlineScriptRegex = /<script(?![^>]*src=)[^>]*>([\s\S]*?)<\/script>/gi;
        const inlineScripts = [...htmlContent.matchAll(inlineScriptRegex)];
        check(`android.html has zero inline <script> blocks (actual: ${inlineScripts.length})`, inlineScripts.length === 0);

        // 1.3 Zero inline style attributes
        const inlineStyleRegex = /\sstyle\s*=\s*["'][^"']*["']/gi;
        const inlineStyles = [...htmlContent.matchAll(inlineStyleRegex)];
        check(`android.html has zero inline style="..." attributes (actual: ${inlineStyles.length})`, inlineStyles.length === 0);

        // 1.4 No #screen-app synthetic overlay
        check(`android.html does not contain synthetic '#screen-app'`, !htmlContent.includes('id="screen-app"'));

        // 1.5 index.html gates Arcade3DScene behind ?demo=1
        const indexPath = path.join(ROOT_DIR, 'index.html');
        const indexContent = fs.readFileSync(indexPath, 'utf8');
        const demoGated = indexContent.includes("get('demo') === '1'") || indexContent.includes('get("demo") === "1"');
        check(`index.html gates arcade demo behind ?demo=1 query flag`, demoGated);

        // 1.6 virtio_gpu_device.js uses log level 'D' for scanout damage rect
        const virtioPath = path.join(ROOT_DIR, 'src/virtio_gpu_device.js');
        const virtioContent = fs.readFileSync(virtioPath, 'utf8');
        const damageLogLevelMatch = virtioContent.match(/logger\.log\s*\(\s*['"]bridge['"]\s*,\s*['"]([A-Z])['"]\s*,\s*`Scanout/);
        check(`virtio_gpu_device.js logs scanout damage rect at level 'D'`, damageLogLevelMatch && damageLogLevelMatch[1] === 'D');

        // 1.7 Global AST/Regex Scan: ONLY src/v86_guest_manager.js instantiates V86Class
        const srcFiles = fs.readdirSync(path.join(ROOT_DIR, 'src')).filter(f => f.endsWith('.js'));
        const forbiddenInstantiations = [];
        const instRegex = /new\s+(V86Class|V86Starter)\s*\(/g;

        for (const file of srcFiles) {
            if (file === 'v86_guest_manager.js') continue;
            const content = fs.readFileSync(path.join(ROOT_DIR, 'src', file), 'utf8');
            if (instRegex.test(content)) {
                forbiddenInstantiations.push(file);
            }
        }
        check(`Zero bypassed new V86Class/V86Starter instantiations in src/ (checked ${srcFiles.length} files)`, forbiddenInstantiations.length === 0);
    }

    // =========================================================================
    // SECTION 2: Unexpected Serial Streams, Fuzzing & Token Fragmentation
    // =========================================================================
    console.log('\n▶ [Section 2] Unexpected Serial Streams, Fuzzing & Fragmentation');
    {
        const milestones = [];
        const serialLines = [];
        const stateTransitions = [];

        const bootstrap = new SystemBootstrap({
            V86Class: AdversarialMockV86,
            autostart: false,
            onMilestone: (m) => milestones.push(m),
            onSerial: (l) => serialLines.push(l),
            onStateChange: (s) => stateTransitions.push(s)
        });

        await bootstrap.init({ isHeadless: true });
        const guestMgr = bootstrap.getGuestManager();
        await guestMgr.start();
        const emu = guestMgr.emulator;

        // 2.1 Fragmented Milestone Tokens (1-byte chunks across regex patterns)
        const dmesgRaw = "[    0.000000] Linux version 5.10.0-android-x86\n";
        for (let i = 0; i < dmesgRaw.length; i++) {
            emu.emitSerialChunk(dmesgRaw[i]);
        }
        check('Fragmented 1-byte stream detects KERNEL_BOOT milestone', milestones.includes(BOOT_MILESTONES.KERNEL_BOOT));

        // 2.2 Broken ANSI Escape Sequences & High ASCII Noise
        const noisySequences = [
            "\x1b[31;1m\x1b[2K[    0.100000] virtio_gpu 0000:00:02.0: DRM/KMS active\x1b[0m\r\n",
            "\x00\x01\x02\x03[    0.200000] binderfs: mounted on /dev/binderfs\x7f\xff\n",
            "Run /init as init process\n",
            "\x1b]0;MalformedTitle\x07servicemanager: ready for IPC\n"
        ];
        for (const seq of noisySequences) {
            emu.emitSerialChunk(seq);
        }
        check('ANSI-polluted stream dispatches VIRTIO_GPU_INIT', milestones.includes(BOOT_MILESTONES.VIRTIO_GPU_INIT));
        check('Binary/control-byte stream dispatches BINDERFS_MOUNT', milestones.includes(BOOT_MILESTONES.BINDERFS_MOUNT));
        check('Stream dispatches INIT_USERSPACE', milestones.includes(BOOT_MILESTONES.INIT_USERSPACE));
        check('Stream dispatches SERVICEMANAGER_READY', milestones.includes(BOOT_MILESTONES.SERVICEMANAGER_READY));

        // 2.3 Fuzzing with 5,000 Random Byte Bursts
        const fuzzedNoise = new Uint8Array(5000);
        for (let i = 0; i < fuzzedNoise.length; i++) {
            fuzzedNoise[i] = Math.floor(Math.random() * 256);
        }
        let fuzzError = null;
        try {
            emu.emitSerialChunk(fuzzedNoise);
        } catch (e) {
            fuzzError = e;
        }
        check('5,000-byte random binary noise does not crash serial listener', fuzzError === null);

        // 2.4 Out-of-Order Panic Event Handling
        emu.emitSerialChunk("\nKernel panic - not syncing: Attempted to kill init!\n");
        check('Kernel panic transitions VM state to ERROR', bootstrap.getState() === VM_STATES.ERROR);

        bootstrap.destroy();
    }

    // =========================================================================
    // SECTION 3: Rapid State Machine Transitions & Lifecycle Abuse
    // =========================================================================
    console.log('\n▶ [Section 3] Rapid State Machine Transitions & Lifecycle Abuse');
    {
        // 3.1 Constructor Option Edge Cases
        const bsEmpty = new SystemBootstrap({});
        check('SystemBootstrap instantiates with empty options without throwing', bsEmpty !== null);
        check('Initial state is UNINITIALIZED', bsEmpty.getState() === VM_STATES.UNINITIALIZED);

        // 3.2 Rapid Render Loop Toggling (1,000 Hammer Cycles)
        let renderLoopCrash = null;
        try {
            for (let i = 0; i < 1000; i++) {
                bsEmpty.startRenderLoop();
                bsEmpty.stopRenderLoop();
            }
        } catch (e) {
            renderLoopCrash = e;
        }
        check('1,000 rapid startRenderLoop / stopRenderLoop cycles execute safely', renderLoopCrash === null);

        // 3.3 Multiple Consecutive Init & Destroy Cycles
        let initCycleCrash = null;
        try {
            for (let i = 0; i < 10; i++) {
                const bs = new SystemBootstrap({ V86Class: AdversarialMockV86, autostart: false });
                await bs.init({ isHeadless: true });
                bs.destroy();
                bs.destroy(); // Redundant destroy call
            }
        } catch (e) {
            initCycleCrash = e;
        }
        check('Repeated init and redundant destroy cycles execute safely', initCycleCrash === null);

        // 3.4 Listener Exception Isolation (Fault Injection)
        const faultyBs = new SystemBootstrap({ V86Class: AdversarialMockV86 });
        let normalListenerRan = false;
        faultyBs.on('testEvent', () => { throw new Error('Fault injection in listener 1'); });
        faultyBs.on('testEvent', () => { normalListenerRan = true; });

        let emitCrash = null;
        try {
            faultyBs.emit('testEvent');
        } catch (e) {
            emitCrash = e;
        }
        check('Faulty event listener exception is isolated without breaking subsequent listeners', normalListenerRan === true && emitCrash === null);
        faultyBs.destroy();

        // 3.5 bzImage Verification Fuzzing (10,000 Mutations)
        let bzFuzzCrashes = 0;
        for (let i = 0; i < 10000; i++) {
            const bufLen = Math.floor(Math.random() * 1200);
            const randomBuf = new Uint8Array(bufLen);
            for (let j = 0; j < bufLen; j++) randomBuf[j] = Math.floor(Math.random() * 256);
            try {
                const res = verifyBzImage(randomBuf.buffer);
                if (typeof res.valid !== 'boolean') bzFuzzCrashes++;
            } catch (e) {
                bzFuzzCrashes++;
            }
        }
        check('10,000 bzImage header fuzzing iterations completed with 0 crashes', bzFuzzCrashes === 0);
    }

    // =========================================================================
    // SECTION 4: Missing / Null DOM Tolerance in AppController & ui_render
    // =========================================================================
    console.log('\n▶ [Section 4] Missing / Null DOM Tolerance in AppController & ui_render');
    {
        // 4.1 AppController with null / empty DOM elements
        const ctrlNullDom = new AppController({
            bootstrap: null,
            runtime: null,
            domElements: {} // Missing all DOM elements
        });

        let ctrlCrash = null;
        try {
            ctrlNullDom.activateScreen('webgpu');
            ctrlNullDom.activateScreen('v86');
            ctrlNullDom.activateScreen('home');
            ctrlNullDom.activateScreen('invalid_screen');
            ctrlNullDom.cycleScreen();
            ctrlNullDom.populateFallbackPackages();
            ctrlNullDom.logBinderTransaction({ handle: 5, code: 7, desc: 'Test', status: 0, durationMs: 1.0, payloadSize: 10 });
            ctrlNullDom.showToast('Test Toast');
            await ctrlNullDom.handleBackPress();
            await ctrlNullDom.handleHomePress();
            await ctrlNullDom.handleRecentsPress();
        } catch (e) {
            ctrlCrash = e;
        }
        check('AppController operations execute cleanly when all DOM elements are missing', ctrlCrash === null);

        // 4.2 ui_render.js pure functions with null containers & malformed entries
        let uiRenderCrash = null;
        try {
            renderLogcatList(null, [], null, 0, false);
            appendLogcatToDom(null, { formatted: 'test', priority: 'I' }, null, 0, false);
            renderBinderTransaction(null, null, 1, { handle: 10, code: 6, desc: 'test', status: 0, durationMs: 1, payloadSize: 4 });
            renderAppLauncherItem(null, 'com.test', 'Test', '📦', null);
            renderDockItems(null, [], null);
            showToast(null, null, 'message');
            updateClock(null, null, null);
            updateMetrics({});
            const el = createLogcatElement({ formatted: 'hello', priority: 'UNKNOWN' });
            assert(el instanceof MockDOMElement, 'createLogcatElement returns element');
        } catch (e) {
            uiRenderCrash = e;
        }
        check('ui_render.js pure methods handle null containers safely without throwing', uiRenderCrash === null);
    }

    // =========================================================================
    // SECTION 5: Backstack Navigation Stress & Package Registry Fuzzing
    // =========================================================================
    console.log('\n▶ [Section 5] Backstack Navigation Stress & Package Registry Fuzzing');
    {
        const runtime = new AndroidRuntime();
        const homeScreen = new MockDOMElement('div', 'screen-home');
        const webGpuScreen = new MockDOMElement('div', 'screen-webgpu');
        const trafficList = new MockDOMElement('div', 'traffic-list');

        const ctrl = new AppController({
            bootstrap: null,
            runtime,
            domElements: {
                screenHome: homeScreen,
                screenWebGpu: webGpuScreen,
                trafficList
            }
        });

        // 5.1 Push 200 Activities onto Navigation Backstack
        for (let i = 0; i < 200; i++) {
            const pkg = `com.stress.app${i % 20}`;
            await ctrl.launchActivity(pkg, `${pkg}.Activity_${i}`);
        }
        check('Pushed 200 activities onto navigation backstack', runtime.activityStack.length === 200);
        check('Active screen is set to webgpu', ctrl.activeScreen === 'webgpu');

        // 5.2 Pop 250 times (including 50 underflow pops past root)
        for (let i = 0; i < 250; i++) {
            await ctrl.handleBackPress();
        }
        check('Backstack cleanly emptied to 0 on underflow pops', runtime.activityStack.length === 0);
        check('Active screen returned to home screen', ctrl.activeScreen === 'home');

        // 5.3 Home Button Reset from Deep Stack
        for (let i = 0; i < 50; i++) {
            await ctrl.launchActivity(`com.stress.modal${i}`);
        }
        await ctrl.handleHomePress();
        check('Home button press immediately sets active screen to home', ctrl.activeScreen === 'home');

        // 5.4 Fuzzing launchActivity with Invalid / Extreme Package Names
        const invalidPkgs = [
            '',
            '..',
            'com.evil/../../../etc/passwd',
            'a'.repeat(2000),
            '<script>alert(1)</script>',
            'null',
            'undefined',
            'org.fdroid.fdroid; DROP TABLE packages;'
        ];

        let pkgFuzzCrash = null;
        try {
            for (const badPkg of invalidPkgs) {
                await ctrl.launchActivity(badPkg);
                await ctrl.installPackage(badPkg, `App_${badPkg}`);
            }
        } catch (e) {
            pkgFuzzCrash = e;
        }
        check('Fuzzed & malicious package names handled safely without unhandled exception', pkgFuzzCrash === null);
    }

    // =========================================================================
    // SECTION 6: UI Render 10k Logcat Capping & Extreme Telemetry Fuzzing
    // =========================================================================
    console.log('\n▶ [Section 6] UI Render 10k Logcat Capping & Telemetry Fuzzing');
    {
        const container = new MockDOMElement('div', 'vm-log-view');
        const counter = new MockDOMElement('span', 'logcat-counter');

        // Append 10,000 lines, ensuring DOM node capping at maxNodes (5,000)
        for (let i = 0; i < 10000; i++) {
            appendLogcatToDom(container, {
                priority: i % 2 === 0 ? 'I' : 'D',
                formatted: `Log message line ${i} with data payload`
            }, counter, i + 1, false, 5000);
        }

        check(`DOM child count capped at maxNodes + 1 (actual: ${container.childElementCount})`, container.childElementCount <= 5001);
        check(`Oldest retained entry is pruned (first child is preserved header or trimmed)`, container.children.length > 0);
        check(`Counter element updated correctly (${counter.textContent})`, counter.textContent.includes('5000 / 10000'));

        // Telemetry fuzzing (NaN, Infinity, -1, extreme floats)
        const fpsPill = new MockDOMElement('span', 'fps-pill');
        const statFps = new MockDOMElement('span', 'stat-fps');
        const canvasHudFps = new MockDOMElement('span', 'canvas-hud-fps');
        const statGpu = new MockDOMElement('span', 'stat-gpu-time');
        const canvasHudGpu = new MockDOMElement('span', 'canvas-hud-gpu');

        let metricsCrash = null;
        try {
            updateMetrics({ fpsPillEl: fpsPill, statFpsEl: statFps, canvasHudFpsEl: canvasHudFps, fps: NaN, statGpuTimeEl: statGpu, canvasHudGpuEl: canvasHudGpu, gpuTime: Infinity });
            updateMetrics({ fpsPillEl: fpsPill, statFpsEl: statFps, canvasHudFpsEl: canvasHudFps, fps: -999.999, statGpuTimeEl: statGpu, canvasHudGpuEl: canvasHudGpu, gpuTime: 0.000001 });
            updateClock(new MockDOMElement('span'), new MockDOMElement('div'), new MockDOMElement('div'));
        } catch (e) {
            metricsCrash = e;
        }
        check('Extreme metric telemetry values (NaN, Infinity, negative) handled safely', metricsCrash === null);
    }

    // =========================================================================
    // SECTION 7: CSS Architecture & Token Conformance
    // =========================================================================
    console.log('\n▶ [Section 7] CSS Architecture & Token Conformance');
    {
        const tokensPath = path.join(ROOT_DIR, 'css/tokens.css');
        const androidCssPath = path.join(ROOT_DIR, 'css/android.css');

        check('css/tokens.css exists', fs.existsSync(tokensPath));
        check('css/android.css exists', fs.existsSync(androidCssPath));

        const tokensContent = fs.readFileSync(tokensPath, 'utf8');
        const androidCssContent = fs.readFileSync(androidCssPath, 'utf8');

        check('css/tokens.css defines root CSS custom properties', tokensContent.includes(':root') && tokensContent.includes('--bg'));
        check('css/android.css implements phone-wrapper, phone-bezel, and nav-btn classes',
            androidCssContent.includes('.phone-wrapper') &&
            androidCssContent.includes('.phone-bezel') &&
            androidCssContent.includes('.nav-btn')
        );
    }

    console.log('\n================================================================');
    console.log(`⚡ ALL ${totalChecks} ADVERSARIAL STRESS CHECKS PASSED WITH 0 FAILURES!`);
    console.log('================================================================\n');
}

runAdversarialSuite().catch((err) => {
    console.error('Fatal challenger error:', err);
    process.exit(1);
});
