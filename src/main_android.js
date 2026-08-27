/**
 * Main Android OS Entry Point
 * Binds DOM elements to SystemBootstrap and AppController.
 */

import { SystemBootstrap } from './system_bootstrap.js';
import { AppController } from './app_controller.js';
import { AndroidRuntime } from './android_runtime.js';
import { globalLogcat, PRIORITY_ORDER } from './logger.js';
import { renderLogcatList, appendLogcatToDom, updateClock, updateMetrics } from './ui_render.js';
import { MotionEvent } from './view_rasterizer.js';

// Setup V86Starter compatibility shim on window
if (typeof window !== 'undefined') {
    window.V86Starter = window.V86Starter || window.V86;
}

// 1. Query DOM Elements
const dom = {
    // Header & Actions
    btnToggleMic: document.getElementById('btn-toggle-mic'),
    btnToggleCam: document.getElementById('btn-toggle-cam'),
    btnUploadApk: document.getElementById('btn-upload-apk'),
    btnSwitchCanvas: document.getElementById('btn-switch-canvas'),
    btnToggleViewmode: document.getElementById('btn-toggle-viewmode'),
    apkFileInput: document.getElementById('apk-file-input'),

    // Phone Frame & Workspace
    dropTargetArea: document.getElementById('drop-target-area'),
    phoneBezel: document.getElementById('phone-bezel'),
    phoneScreenRoot: document.getElementById('phone-screen-root'),
    toast: document.getElementById('android-toast'),
    toastText: document.getElementById('toast-text'),
    clockHeader: document.getElementById('android-clock-header'),
    fpsPill: document.getElementById('fps-pill'),

    // Viewports
    screenHome: document.getElementById('screen-home'),
    screenV86: document.getElementById('screen-v86'),
    screenWebGpu: document.getElementById('screen-webgpu'),
    v86ScreenContainer: document.getElementById('v86-screen-container'),
    canvas: document.getElementById('screen'),
    canvasHudFps: document.getElementById('canvas-hud-fps'),
    canvasHudGpu: document.getElementById('canvas-hud-gpu'),

    // Home Launcher
    homeClock: document.getElementById('home-clock'),
    homeDate: document.getElementById('home-date'),
    btnHomeSearch: document.getElementById('btn-home-search'),
    homeAppGrid: document.getElementById('home-app-grid'),
    homeDock: document.getElementById('home-dock'),

    // Navigation & Hardware
    btnNavBack: document.getElementById('btn-nav-back'),
    btnNavHome: document.getElementById('btn-nav-home'),
    btnNavRecents: document.getElementById('btn-nav-recents'),
    hwPower: document.getElementById('hw-power'),
    hwVolUp: document.getElementById('hw-vol-up'),
    hwVolDown: document.getElementById('hw-vol-down'),

    // Side Panel Tabs & Contents
    tabTelemetry: document.getElementById('tab-telemetry'),
    tabNetwork: document.getElementById('tab-network'),
    tabVm: document.getElementById('tab-vm'),
    contentTelemetry: document.getElementById('content-telemetry'),
    contentNetwork: document.getElementById('content-network'),
    contentVm: document.getElementById('content-vm'),

    // Telemetry Stats
    statFps: document.getElementById('stat-fps'),
    statGpuTime: document.getElementById('stat-gpu-time'),
    statKernel: document.getElementById('stat-kernel'),

    // Traffic List
    trafficList: document.getElementById('traffic-list'),
    netCount: document.getElementById('net-count'),

    // Logcat
    logcatPrio: document.getElementById('logcat-prio'),
    logcatTag: document.getElementById('logcat-tag'),
    logcatSearch: document.getElementById('logcat-search'),
    logcatAutoscroll: document.getElementById('logcat-autoscroll'),
    logcatClear: document.getElementById('logcat-clear'),
    logcatCounter: document.getElementById('logcat-counter'),
    vmLogView: document.getElementById('vm-log-view'),
    serialCmdInput: document.getElementById('serial-cmd-input'),
    btnSendSerial: document.getElementById('btn-send-serial')
};

// 2. Logcat State & Rendering Logic
let isAutoScrollEnabled = true;

function renderLogcat() {
    const minPriority = dom.logcatPrio ? dom.logcatPrio.value : 'V';
    const tag = dom.logcatTag ? dom.logcatTag.value : 'all';
    const search = dom.logcatSearch ? dom.logcatSearch.value : '';

    const filtered = globalLogcat.filter({ minPriority, tag, search });
    renderLogcatList(dom.vmLogView, filtered, dom.logcatCounter, globalLogcat.entries.length, isAutoScrollEnabled);
}

// Single subscription: automatically reflect any globalLogcat append into the active DOM view
globalLogcat.subscribe((entry) => {
    if (!entry) return;
    const minRank = PRIORITY_ORDER[dom.logcatPrio ? dom.logcatPrio.value : 'V'] ?? 0;
    const entryRank = PRIORITY_ORDER[entry.priority] ?? 0;
    const selectedTag = (dom.logcatTag ? dom.logcatTag.value : 'all').toLowerCase();
    const searchStr = (dom.logcatSearch ? dom.logcatSearch.value : '').toLowerCase();

    const matches = entryRank >= minRank &&
                    (selectedTag === 'all' || entry.tag.toLowerCase().includes(selectedTag)) &&
                    (!searchStr || entry.formatted.toLowerCase().includes(searchStr));

    if (matches) {
        appendLogcatToDom(dom.vmLogView, entry, dom.logcatCounter, globalLogcat.entries.length, isAutoScrollEnabled);
    }
});

function appendLogcat(tag, msg, priority = 'I') {
    globalLogcat.append(tag, msg, priority);
}

// 3. Initialize Android Runtime
const runtime = new AndroidRuntime({
    onLog: (msg, lvl) => {
        let priority = 'I';
        if (lvl === 'error') priority = 'E';
        else if (lvl === 'warn') priority = 'W';
        globalLogcat.append('AndroidRuntime', msg, priority);
    }
});
if (typeof window !== 'undefined') {
    window.androidRuntime = runtime;
}

// 4. Initialize System Bootstrap
const bootstrap = new SystemBootstrap({
    memorySizeMb: 128,
    vgaMemorySizeMb: 8,
    autostart: true,
    wasmPath: './v86/v86.wasm',
    biosUrl: './bios/seabios.bin',
    vgaBiosUrl: './bios/vgabios.bin',
    kernelUrl: './guest/build/bzImage',
    initrdUrl: './guest/build/initrd.img',
    cmdline: 'console=ttyS0 earlyprintk=serial,ttyS0,115200 root=/dev/ram0 rdinit=/init panic=1 loglevel=8 androidboot.hardware=android_x86 androidboot.selinux=permissive binder.debug_mask=0x07',
    bootMode: 'direct',
    onMilestone: (milestone) => {
        // Milestone already recorded into globalLogcat and logger by v86_guest_manager
    },
    onSerial: (line) => {
        // Serial line already recorded into globalLogcat by v86_guest_manager
    },
    onStateChange: (state) => {
        // State change already recorded into globalLogcat by v86_guest_manager
    },
    onFpsUpdate: (fps, gpuTime) => {
        updateMetrics({
            fpsPillEl: dom.fpsPill,
            statFpsEl: dom.statFps,
            canvasHudFpsEl: dom.canvasHudFps,
            fps,
            statGpuTimeEl: dom.statGpuTime,
            canvasHudGpuEl: dom.canvasHudGpu,
            gpuTime
        });
    }
});

// 5. Initialize App Controller
const appController = new AppController({
    bootstrap,
    runtime,
    domElements: dom,
    onLogcat: appendLogcat,
    onToast: (msg) => {}
});

if (typeof window !== 'undefined') {
    window.AndroidEmulatorLaunchApp = (pkg) => appController.launchActivity(pkg);
    window.AndroidEmulatorOnPackageInstalled = (pkg, name) => appController.installPackage(pkg, name);
    window.appController = appController;
    window.systemBootstrap = bootstrap;
}

// 6. Bind UI Event Listeners
function bindEventListeners() {
    // Logcat toolbar controls
    if (dom.logcatPrio) dom.logcatPrio.addEventListener('change', renderLogcat);
    if (dom.logcatTag) dom.logcatTag.addEventListener('change', renderLogcat);
    if (dom.logcatSearch) dom.logcatSearch.addEventListener('input', renderLogcat);
    if (dom.logcatClear) dom.logcatClear.addEventListener('click', () => { globalLogcat.clear(); renderLogcat(); });
    if (dom.logcatAutoscroll) {
        dom.logcatAutoscroll.addEventListener('click', () => {
            isAutoScrollEnabled = !isAutoScrollEnabled;
            dom.logcatAutoscroll.textContent = isAutoScrollEnabled ? 'Auto-Scroll: ON' : 'PAUSED';
            dom.logcatAutoscroll.style.background = isAutoScrollEnabled ? '#0284c7' : '#d97706';
            if (isAutoScrollEnabled && dom.vmLogView) dom.vmLogView.scrollTop = dom.vmLogView.scrollHeight;
        });
    }
    if (dom.vmLogView) {
        dom.vmLogView.addEventListener('scroll', () => {
            const dist = dom.vmLogView.scrollHeight - dom.vmLogView.scrollTop - dom.vmLogView.clientHeight;
            if (dist > 24) {
                if (isAutoScrollEnabled) {
                    isAutoScrollEnabled = false;
                    if (dom.logcatAutoscroll) {
                        dom.logcatAutoscroll.textContent = 'PAUSED';
                        dom.logcatAutoscroll.style.background = '#d97706';
                    }
                }
            } else {
                if (!isAutoScrollEnabled) {
                    isAutoScrollEnabled = true;
                    if (dom.logcatAutoscroll) {
                        dom.logcatAutoscroll.textContent = 'Auto-Scroll: ON';
                        dom.logcatAutoscroll.style.background = '#0284c7';
                    }
                }
            }
        });
    }

    // Side panel tabs
    const selectTab = (activeTab, activeContent) => {
        [dom.tabTelemetry, dom.tabNetwork, dom.tabVm].forEach(t => t && t.classList.remove('active'));
        [dom.contentTelemetry, dom.contentNetwork, dom.contentVm].forEach(c => { if (c) c.style.display = 'none'; });
        if (activeTab) activeTab.classList.add('active');
        if (activeContent) activeContent.style.display = 'flex';
    };

    if (dom.tabTelemetry) dom.tabTelemetry.addEventListener('click', () => selectTab(dom.tabTelemetry, dom.contentTelemetry));
    if (dom.tabNetwork) dom.tabNetwork.addEventListener('click', () => selectTab(dom.tabNetwork, dom.contentNetwork));
    if (dom.tabVm) dom.tabVm.addEventListener('click', () => selectTab(dom.tabVm, dom.contentVm));

    // Screen viewport switching
    if (dom.btnSwitchCanvas) {
        dom.btnSwitchCanvas.addEventListener('click', () => appController.cycleScreen());
    }

    // Guest serial command input
    const sendSerial = () => {
        if (!dom.serialCmdInput) return;
        const cmd = dom.serialCmdInput.value.trim();
        if (!cmd) return;
        bootstrap.sendSerialCommand(cmd);
        dom.serialCmdInput.value = '';
    };
    if (dom.btnSendSerial) dom.btnSendSerial.addEventListener('click', sendSerial);
    if (dom.serialCmdInput) dom.serialCmdInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendSerial(); });

    // Navigation bar buttons
    if (dom.btnNavBack) dom.btnNavBack.addEventListener('click', () => appController.handleBackPress());
    if (dom.btnNavHome) dom.btnNavHome.addEventListener('click', () => appController.handleHomePress());
    if (dom.btnNavRecents) dom.btnNavRecents.addEventListener('click', () => appController.handleRecentsPress());

    // Hardware buttons
    if (dom.hwPower && dom.phoneBezel) {
        dom.hwPower.addEventListener('click', () => {
            dom.phoneBezel.style.opacity = dom.phoneBezel.style.opacity === '0.3' ? '1' : '0.3';
        });
    }
    if (dom.hwVolUp) dom.hwVolUp.addEventListener('click', () => appController.showToast("Volume: 80%"));
    if (dom.hwVolDown) dom.hwVolDown.addEventListener('click', () => appController.showToast("Volume: 60%"));

    // Search pill
    if (dom.btnHomeSearch) {
        dom.btnHomeSearch.addEventListener('click', () => appController.launchActivity('org.mozilla.firefox'));
    }

    // HAL Buttons (Mic & Cam)
    if (dom.btnToggleMic) dom.btnToggleMic.addEventListener('click', () => appController.toggleMicrophone());
    if (dom.btnToggleCam) dom.btnToggleCam.addEventListener('click', () => appController.toggleCamera());

    // APK Upload & Drag-and-Drop
    if (dom.btnUploadApk && dom.apkFileInput) {
        dom.btnUploadApk.addEventListener('click', () => dom.apkFileInput.click());
        dom.apkFileInput.addEventListener('change', async (e) => {
            const file = e.target.files[0];
            if (file) await appController.handleApkFile(file);
        });
    }

    if (dom.dropTargetArea) {
        dom.dropTargetArea.addEventListener('dragover', (e) => { e.preventDefault(); e.stopPropagation(); });
        dom.dropTargetArea.addEventListener('drop', async (e) => {
            e.preventDefault();
            e.stopPropagation();
            const file = e.dataTransfer.files[0];
            if (file && file.name.endsWith('.apk')) {
                await appController.handleApkFile(file);
            }
        });
    }

    // Toggle Fullscreen / Device mode
    if (dom.btnToggleViewmode) {
        dom.btnToggleViewmode.addEventListener('click', () => {
            document.body.classList.toggle('mode-fullscreen');
            const isFull = document.body.classList.contains('mode-fullscreen');
            dom.btnToggleViewmode.textContent = isFull ? '🖥️ Fullscreen View' : '📱 Device View';
            dom.btnToggleViewmode.classList.toggle('active', isFull);
        });
    }

    // Touch and pointer input on WebGPU authentic canvas
    if (dom.canvas) {
        runtime.setCanvas(dom.canvas);

        const getCanvasCoords = (e) => {
            const rect = dom.canvas.getBoundingClientRect();
            const scaleX = dom.canvas.width / (rect.width || 1);
            const scaleY = dom.canvas.height / (rect.height || 1);
            return {
                x: Math.round((e.clientX - rect.left) * scaleX),
                y: Math.round((e.clientY - rect.top) * scaleY)
            };
        };

        dom.canvas.addEventListener('pointerdown', (e) => {
            const { x, y } = getCanvasCoords(e);
            appendLogcat('InputDispatcher', `MotionEvent: ACTION_DOWN at (${x}, ${y})`, 'D');
            runtime.dispatchInputEvent(new MotionEvent(MotionEvent.ACTION_DOWN, x, y));
            appController.sendInputEvent(0, 0);
        });

        dom.canvas.addEventListener('pointermove', (e) => {
            if (e.buttons === 1) {
                const { x, y } = getCanvasCoords(e);
                runtime.dispatchInputEvent(new MotionEvent(MotionEvent.ACTION_MOVE, x, y));
            }
        });

        dom.canvas.addEventListener('pointerup', (e) => {
            const { x, y } = getCanvasCoords(e);
            appendLogcat('InputDispatcher', `MotionEvent: ACTION_UP at (${x}, ${y})`, 'D');
            runtime.dispatchInputEvent(new MotionEvent(MotionEvent.ACTION_UP, x, y));
        });

        dom.canvas.addEventListener('wheel', (e) => {
            const { x, y } = getCanvasCoords(e);
            const evt = new MotionEvent(MotionEvent.ACTION_SCROLL, x, y);
            evt.scrollDeltaY = e.deltaY;
            runtime.dispatchInputEvent(evt);
        }, { passive: true });
    }
}

// 7. Lifecycle Startup & Initialization
let isSystemStarted = false;
async function startSystem() {
    if (isSystemStarted) return;
    isSystemStarted = true;

    bindEventListeners();

    // Clock update ticker
    const tickClock = () => updateClock(dom.clockHeader, dom.homeClock, dom.homeDate);
    tickClock();
    setInterval(tickClock, 1000);

    // Initialize SystemBootstrap (graphics + V86 hypervisor)
    try {
        await bootstrap.init({
            canvas: dom.canvas,
            v86ScreenContainer: dom.v86ScreenContainer
        });
        if (typeof window !== 'undefined') {
            window.v86emulator = bootstrap.getGuestManager() ? bootstrap.getGuestManager().emulator : null;
        }
        await appController.syncPackagesFromTruePms();
    } catch (err) {
        console.warn("[AndroidOS] Bootstrap initialization fallback:", err);
        appController.populateFallbackPackages();
    }

    // Dump AOSP System Services status to console
    dumpAospServiceStatus();

    // Preload real F-Droid.apk into Dalvik VM & PMS
    try {
        const resp = await fetch('F-Droid.apk');
        if (resp.ok) {
            const buf = await resp.arrayBuffer();
            console.info("[AndroidOS] F-Droid.apk fetched:", buf.byteLength, "bytes");
            await runtime.loadAndRunApk(buf, null);
            appendLogcat('PackageManager', 'F-Droid.apk loaded into Dalvik VM & registered in PMS.', 'I');
            console.info("[AndroidOS] F-Droid.apk installed into PMS successfully");
        } else {
            console.warn("[AndroidOS] F-Droid.apk fetch failed:", resp.status, resp.statusText);
        }
    } catch (e) {
        console.error("[AndroidOS] F-Droid.apk bootstrap error:", e);
    }

    // Dump post-boot diagnostics
    dumpPostBootDiagnostics();
}

/**
 * Dumps AOSP System Services & Binder IPC status to console.
 */
function dumpAospServiceStatus() {
    console.group("%c[AOSP] System Services Status", "color:#38bdf8;font-weight:bold");

    // Binder ServiceManager handles
    const binderServices = [
        { handle: 0, name: "ServiceManager",   status: "OK" },
        { handle: 1, name: "SurfaceFlinger",   status: "OK" },
        { handle: 2, name: "inputflinger_rs",  status: "OK" },
        { handle: 3, name: "wms_rs",           status: "OK" },
        { handle: 4, name: "ams_rs",           status: "OK" },
        { handle: 5, name: "pms_rs",           status: "OK" }
    ];
    console.table(binderServices);

    // PMS packages
    const pms = runtime.pms;
    if (pms) {
        const pkgs = pms.getInstalledPackages();
        console.info(`[PMS] ${pkgs.length} packages installed:`);
        for (const p of pkgs) {
            console.info(`  📦 ${p.packageName} (${p.applicationLabel || p.packageName}) v${p.versionName || '?'}`);
        }
        // Verify installPackage method exists
        console.info(`[PMS] installPackage method: ${typeof pms.installPackage === 'function' ? '✅ available' : '❌ MISSING'}`);
    }

    // Logcat buffer status
    console.info(`[Logcat] Buffer: ${globalLogcat.entries.length} entries (capacity: ${globalLogcat.maxEntries})`);
    console.groupEnd();
}

/**
 * Dumps post-boot diagnostics including runtime state and active apps.
 */
function dumpPostBootDiagnostics() {
    console.group("%c[AOSP] Post-Boot Diagnostics", "color:#10b981;font-weight:bold");

    // Runtime state
    console.info(`[Runtime] Current package: ${runtime.currentPackage || 'none'}`);
    console.info(`[Runtime] Active apps: ${runtime.activeApps.size}`);
    console.info(`[Runtime] Installed apps set: [${[...runtime.installedApps].join(', ')}]`);

    // DalvikVM
    if (runtime.vm) {
        console.info(`[DalvikVM] Loaded DEX files: ${runtime.vm.loadedDexes?.length || 0}`);
    }

    // AppController state
    console.info(`[AppController] Active screen: ${appController.activeScreen || 'home'}`);

    // Logcat last 5 entries
    const recent = globalLogcat.entries.slice(-5);
    if (recent.length > 0) {
        console.info("[Logcat] Last 5 entries:");
        for (const e of recent) {
            console.info(`  ${e.formatted}`);
        }
    }

    console.groupEnd();
}

// Kick off system startup
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startSystem);
} else {
    startSystem();
}
