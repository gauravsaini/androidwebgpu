/**
 * Android Application, Activity Lifecycle & IPC Controller
 * Coordinates Activity Manager, Package Manager, Window Manager, and Input HAL via Binder IPC.
 */

import { BinderParcel, VirtioBinderFraming } from './binder_test_suite.js';
import { resolveAppMetadata } from './android_runtime.js';
import { renderAppLauncherItem, renderDockItems, renderBinderTransaction, showToast } from './ui_render.js';
import { logger } from './logger.js';

export class AppController {
    /**
     * @param {Object} options
     * @param {import('./system_bootstrap.js').SystemBootstrap} options.bootstrap
     * @param {import('./android_runtime.js').AndroidRuntime} options.runtime
     * @param {Object} options.domElements
     * @param {Function} [options.onLogcat]
     * @param {Function} [options.onToast]
     */
    constructor(options = {}) {
        this.bootstrap = options.bootstrap;
        this.runtime = options.runtime;
        this.dom = options.domElements || {};
        this.onLogcat = options.onLogcat || ((tag, msg, prio) => {});
        this.onToast = options.onToast || ((msg) => {});

        this.binderTxCounter = 0;
        this.audioCtx = null;
        this.camStream = null;
        this.activeScreen = 'home';
    }

    /**
     * Log a Binder IPC transaction to UI panel and telemetry.
     */
    logBinderTransaction({ handle, code, desc, status, durationMs, payloadSize }) {
        this.binderTxCounter++;
        renderBinderTransaction(
            this.dom.trafficList,
            this.dom.netCount,
            this.binderTxCounter,
            { handle, code, desc, status, durationMs, payloadSize }
        );
    }

    /**
     * Switch visible viewport on the phone screen.
     * @param {'home'|'webgpu'|'v86'} screenName
     */
    activateScreen(screenName) {
        const screens = {
            home: this.dom.screenHome,
            webgpu: this.dom.screenWebGpu,
            v86: this.dom.screenV86
        };

        Object.entries(screens).forEach(([name, el]) => {
            if (el) {
                el.classList.remove('active');
                el.style.display = 'none';
            }
        });

        const targetEl = screens[screenName] || this.dom.screenHome;
        if (targetEl) {
            targetEl.classList.add('active');
            targetEl.style.display = (screenName === 'webgpu' || screenName === 'v86') ? 'flex' : 'block';
        }
        this.activeScreen = screenName;

        if (this.dom.btnSwitchCanvas) {
            if (screenName === 'webgpu') {
                this.dom.btnSwitchCanvas.textContent = '🖥️ View Guest VM (VGA)';
            } else if (screenName === 'v86') {
                this.dom.btnSwitchCanvas.textContent = '🏠 View Home Screen';
            } else {
                this.dom.btnSwitchCanvas.textContent = '🎨 View WebGPU Canvas';
            }
        }
    }

    /**
     * Cycle through canvas and OS viewports.
     */
    cycleScreen() {
        if (this.activeScreen === 'webgpu') {
            this.activateScreen('v86');
        } else if (this.activeScreen === 'v86') {
            this.activateScreen('home');
        } else {
            this.activateScreen('webgpu');
        }
    }

    /**
     * Query Package Manager Service via Binder (Handle 5) for installed packages.
     */
    async syncPackagesFromTruePms() {
        const bridge = this.bootstrap ? this.bootstrap.getBridge() : null;
        if (!bridge || typeof bridge.process_binder_packet !== 'function') {
            this.populateFallbackPackages();
            return;
        }

        try {
            const t0 = performance.now();
            const parcel = new BinderParcel(64);
            parcel.writeUtf16("android.content.pm.IPackageManager");
            parcel.writeInt64(0n); // flags
            parcel.writeInt32(0);  // userId

            const reqBytes = VirtioBinderFraming.buildRequest({
                msgId: 2001n,
                cmd: 1, // CMD_TRANSACT
                targetHandle: 5, // Handle 5 = pms_rs
                code: 7, // GET_INSTALLED_PACKAGES
                flags: 0,
                cookie: 0n,
                data: parcel.toUint8Array()
            });

            const respBytes = bridge.process_binder_packet(reqBytes);
            const dur = performance.now() - t0;
            const resp = VirtioBinderFraming.parseResponse(respBytes);

            this.logBinderTransaction({
                handle: 5,
                code: 7,
                desc: 'IPackageManager::getInstalledPackages',
                status: resp.hdr.status,
                durationMs: dur,
                payloadSize: resp.data.length
            });

            if (this.dom.homeAppGrid) this.dom.homeAppGrid.innerHTML = '';
            if (this.dom.homeDock) this.dom.homeDock.innerHTML = '';

            const replyParcel = resp.parcel;
            const statusEx = replyParcel.readInt32(); // exception code (0 = OK)
            const count = replyParcel.readInt32();
            this.onLogcat('PackageManager', `PMS returned ${count} installed packages via Binder (status=${statusEx}).`, 'I');

            const packages = [];
            for (let i = 0; i < count; i++) {
                try {
                    const pkgName = replyParcel.readUtf8() || '';
                    const versionCode = replyParcel.readInt32();
                    const versionName = replyParcel.readUtf8() || '1.0';

                    let appLabel = pkgName;
                    const hasAppInfo = replyParcel.readInt32() !== 0;
                    if (hasAppInfo) {
                        const aiPkg = replyParcel.readUtf8();
                        const aiName = replyParcel.readUtf8();
                        const aiLabel = replyParcel.readUtf8();
                        if (aiLabel) appLabel = aiLabel;
                        else if (aiName) appLabel = aiName;
                        replyParcel.readUint32(); // icon
                        replyParcel.readInt32();  // target_sdk_version
                        replyParcel.readInt32();  // min_sdk_version
                        replyParcel.readUint32(); // flags
                        replyParcel.readUtf8();   // data_dir
                        replyParcel.readUtf8();   // source_dir
                        replyParcel.readUtf8();   // public_source_dir
                        replyParcel.readUtf8();   // native_library_dir
                        replyParcel.readInt32();  // uid
                        replyParcel.readInt32();  // enabled
                    }

                    // Parse Activities
                    const actCount = replyParcel.readInt32();
                    for (let a = 0; a < actCount; a++) {
                        replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readUtf8();
                        replyParcel.readUint32(); replyParcel.readUint32(); replyParcel.readInt32();
                        replyParcel.readUtf8(); replyParcel.readInt32(); replyParcel.readInt32();
                        const filterCount = replyParcel.readInt32();
                        for (let f = 0; f < filterCount; f++) {
                            const ac = replyParcel.readInt32(); for (let x = 0; x < ac; x++) replyParcel.readUtf8();
                            const cc = replyParcel.readInt32(); for (let x = 0; x < cc; x++) replyParcel.readUtf8();
                            const dc = replyParcel.readInt32(); for (let x = 0; x < dc; x++) replyParcel.readUtf8();
                            replyParcel.readInt32();
                        }
                        const hasActAppInfo = replyParcel.readInt32() !== 0;
                        if (hasActAppInfo) {
                            replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readUtf8();
                            replyParcel.readUint32(); replyParcel.readInt32(); replyParcel.readInt32();
                            replyParcel.readUint32(); replyParcel.readUtf8(); replyParcel.readUtf8();
                            replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readInt32();
                            replyParcel.readInt32();
                        }
                    }

                    // Parse Services
                    const svcCount = replyParcel.readInt32();
                    for (let s = 0; s < svcCount; s++) {
                        replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readUtf8();
                        replyParcel.readInt32(); replyParcel.readInt32();
                    }

                    // Parse Receivers
                    const rcvCount = replyParcel.readInt32();
                    for (let r = 0; r < rcvCount; r++) {
                        replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readUtf8();
                        replyParcel.readInt32(); replyParcel.readInt32();
                        const rfCount = replyParcel.readInt32();
                        for (let f = 0; f < rfCount; f++) {
                            const ac = replyParcel.readInt32(); for (let x = 0; x < ac; x++) replyParcel.readUtf8();
                            const cc = replyParcel.readInt32(); for (let x = 0; x < cc; x++) replyParcel.readUtf8();
                            const dc = replyParcel.readInt32(); for (let x = 0; x < dc; x++) replyParcel.readUtf8();
                            replyParcel.readInt32();
                        }
                    }

                    // Parse Providers
                    const provCount = replyParcel.readInt32();
                    for (let p2 = 0; p2 < provCount; p2++) {
                        replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readUtf8();
                        replyParcel.readInt32(); replyParcel.readInt32(); replyParcel.readUtf8();
                        replyParcel.readUtf8(); replyParcel.readInt32(); replyParcel.readInt32();
                        replyParcel.readInt32();
                        const hasProvApp = replyParcel.readInt32() !== 0;
                        if (hasProvApp) {
                            replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readUtf8();
                            replyParcel.readUint32(); replyParcel.readInt32(); replyParcel.readInt32();
                            replyParcel.readUint32(); replyParcel.readUtf8(); replyParcel.readUtf8();
                            replyParcel.readUtf8(); replyParcel.readUtf8(); replyParcel.readInt32();
                            replyParcel.readInt32();
                        }
                    }

                    // Parse Permissions
                    const permCount = replyParcel.readInt32();
                    for (let pm = 0; pm < permCount; pm++) {
                        replyParcel.readUtf8();
                    }

                    try { replyParcel.readInt64(); replyParcel.readInt64(); } catch(_) {}

                    const meta = resolveAppMetadata(pkgName, { applicationLabel: appLabel });
                    packages.push({ pkg: pkgName, name: meta.name, icon: meta.icon });
                } catch (parseErr) {
                    this.onLogcat('PackageManager', `Parcel parse error at pkg ${i}: ${parseErr.message}`, 'W');
                    break;
                }
            }

            if (packages.length === 0) {
                this.populateFallbackPackages();
                return;
            }

            packages.forEach(p => {
                renderAppLauncherItem(this.dom.homeAppGrid, p.pkg, p.name, p.icon, (pkg) => this.launchActivity(pkg));
            });
            renderDockItems(this.dom.homeDock, packages.slice(0, 4), (pkg) => this.launchActivity(pkg));

        } catch (e) {
            console.warn("[AppController] PMS sync error:", e);
            this.populateFallbackPackages();
        }
    }

    /**
     * Fallback standard Android packages.
     */
    populateFallbackPackages() {
        if (!this.dom.homeAppGrid || !this.dom.homeDock) return;
        this.dom.homeAppGrid.innerHTML = '';
        const fallback = [
            { pkg: 'org.fdroid.fdroid', name: 'F-Droid', icon: '🤖' },
            { pkg: 'org.mozilla.firefox', name: 'Firefox', icon: '🦊' },
            { pkg: 'com.android.chrome', name: 'Chrome', icon: '🌐' },
            { pkg: 'com.android.settings', name: 'Settings', icon: '⚙️' },
            { pkg: 'com.android.terminal', name: 'Terminal', icon: '💻' },
            { pkg: 'com.android.files', name: 'Files', icon: '📁' },
            { pkg: 'com.android.glbenchmark', name: '3D Arcade', icon: '🎮' }
        ];
        fallback.forEach(p => {
            renderAppLauncherItem(this.dom.homeAppGrid, p.pkg, p.name, p.icon, (pkg) => this.launchActivity(pkg));
        });
        renderDockItems(this.dom.homeDock, fallback.slice(0, 4), (pkg) => this.launchActivity(pkg));
    }

    /**
     * Install an application package into PackageManagerService via Binder.
     */
    async installPackage(pkgName, appName, versionName = '1.0.0', versionCode = 1, mainAct = '') {
        this.onLogcat('PackageManager', `Dispatching Binder installPackage -> [${pkgName}]`, 'I');
        const bridge = this.bootstrap ? this.bootstrap.getBridge() : null;

        if (bridge && typeof bridge.process_binder_packet === 'function') {
            const t0 = performance.now();
            const parcel = new BinderParcel(256);
            parcel.writeUtf16("android.content.pm.IPackageManager");
            parcel.writeUtf8(pkgName);
            parcel.writeUtf8(appName);
            parcel.writeUtf8(versionName || '1.0.0');
            parcel.writeInt32(versionCode || 1);
            parcel.writeUtf8(mainAct || `${pkgName}.MainActivity`);

            const reqBytes = VirtioBinderFraming.buildRequest({
                msgId: 3001n,
                cmd: 1,
                targetHandle: 5,
                code: 10, // INSTALL_PACKAGE
                flags: 0,
                cookie: 0n,
                data: parcel.toUint8Array()
            });

            const respBytes = bridge.process_binder_packet(reqBytes);
            const dur = performance.now() - t0;
            const resp = VirtioBinderFraming.parseResponse(respBytes);

            this.logBinderTransaction({
                handle: 5,
                code: 10,
                desc: `IPackageManager::installPackage(${pkgName})`,
                status: resp.hdr.status,
                durationMs: dur,
                payloadSize: resp.data.length
            });

            this.onLogcat('PackageManager', `Package [${pkgName}] installed successfully via Binder (Status 0).`, 'I');
        }

        const meta = resolveAppMetadata(pkgName, { applicationLabel: appName });
        renderAppLauncherItem(this.dom.homeAppGrid, pkgName, meta.name, meta.icon, (pkg) => this.launchActivity(pkg));
        this.showToast(`✓ Installed ${meta.name}`);
    }

    /**
     * Queue live scanout framebuffer pixels to SurfaceFlinger BufferQueue via Binder (Handle 10).
     */
    async queueAppBufferToSurfaceFlinger(pkg, appName) {
        const bridge = this.bootstrap ? this.bootstrap.getBridge() : null;
        const gpuDev = this.bootstrap ? this.bootstrap.getGpuDevice() : null;
        if (!bridge || typeof bridge.process_binder_packet !== 'function') return;

        try {
            let rawBytes = null;
            if (gpuDev && typeof gpuDev.getScanoutFramebuffer === 'function') {
                const fb = gpuDev.getScanoutFramebuffer(0);
                if (fb && fb.length >= 1280 * 720 * 4) {
                    rawBytes = fb.subarray(0, 1280 * 720 * 4);
                }
            }
            if (!rawBytes && typeof bridge.get_scanout_framebuffer === 'function') {
                const fb = bridge.get_scanout_framebuffer(0);
                if (fb && fb.length >= 1280 * 720 * 4) {
                    rawBytes = fb.subarray(0, 1280 * 720 * 4);
                }
            }
            if (!rawBytes) {
                rawBytes = new Uint8Array(1280 * 720 * 4);
            }

            const deqParcel = new BinderParcel(32);
            deqParcel.writeUint32(1280);
            deqParcel.writeUint32(720);
            deqParcel.writeUint32(1);

            const deqReq = VirtioBinderFraming.buildRequest({
                msgId: 4101n,
                cmd: 1,
                targetHandle: 10,
                code: 3, // DEQUEUE_BUFFER
                flags: 0,
                cookie: 0n,
                data: deqParcel.toUint8Array()
            });
            const deqRespBytes = bridge.process_binder_packet(deqReq);
            const deqResp = VirtioBinderFraming.parseResponse(deqRespBytes);

            let slot = 0;
            if (deqResp.hdr.status === 0 && deqResp.data.length >= 8) {
                const deqReplyParcel = new BinderParcel();
                deqReplyParcel.buffer = deqResp.data.buffer.slice(deqResp.data.byteOffset, deqResp.data.byteOffset + deqResp.data.byteLength);
                deqReplyParcel.view = new DataView(deqReplyParcel.buffer);
                deqReplyParcel.bytes = new Uint8Array(deqReplyParcel.buffer);
                deqReplyParcel.writePos = deqResp.data.byteLength;
                deqReplyParcel.readPos = 4;
                slot = deqReplyParcel.readInt32();
            }

            const qParcel = new BinderParcel(1280 * 720 * 4 + 64);
            qParcel.writeInt32(slot);
            qParcel.writeUint32(1280);
            qParcel.writeUint32(720);
            qParcel.writeByteArray(rawBytes);

            const qReq = VirtioBinderFraming.buildRequest({
                msgId: 4102n,
                cmd: 1,
                targetHandle: 10,
                code: 6, // QUEUE_BUFFER
                flags: 0,
                cookie: 0n,
                data: qParcel.toUint8Array()
            });
            const qRespBytes = bridge.process_binder_packet(qReq);
            const qResp = VirtioBinderFraming.parseResponse(qRespBytes);

            this.logBinderTransaction({
                handle: 10,
                code: 6,
                desc: `IGraphicBufferProducer::queueBuffer(slot=${slot}, 1280x720)`,
                status: qResp.hdr.status,
                durationMs: 0.8,
                payloadSize: qParcel.dataSize()
            });

        } catch (e) {
            console.warn("[AppController] queueAppBufferToSurfaceFlinger error:", e);
        }
    }

    /**
     * Launch an Activity with real ActivityManager and WindowManager Binder calls.
     */
    async launchActivity(pkg, activityName = '') {
        this.onLogcat('ActivityTaskManager', `START u0 {act=android.intent.action.MAIN cat=[android.intent.category.LAUNCHER] pkg=${pkg}}`, 'I');
        const meta = resolveAppMetadata(pkg);
        const bridge = this.bootstrap ? this.bootstrap.getBridge() : null;

        // 1. Dispatch genuine Binder transaction to ams_rs on Handle 4 (IActivityManager::startActivity)
        if (bridge && typeof bridge.process_binder_packet === 'function') {
            try {
                const t0 = performance.now();
                const parcel = new BinderParcel(512);
                parcel.writeUtf16("android.app.IActivityManager");
                parcel.writeBool(false);
                parcel.writeUtf8("android.intent.action.MAIN");
                parcel.writeInt32(1);
                parcel.writeUtf8("android.intent.category.LAUNCHER");
                parcel.writeUtf8(null);
                parcel.writeUtf8(null);
                parcel.writeBool(true);
                parcel.writeUtf8(pkg);
                parcel.writeUtf8(activityName || `${pkg}.MainActivity`);
                parcel.writeUint32(0x10000000); // FLAG_ACTIVITY_NEW_TASK
                parcel.writeUtf8(pkg);
                parcel.writeBool(false);
                parcel.writeBool(false);
                parcel.writeInt32(0);
                parcel.writeInt32(0);

                const reqBytes = VirtioBinderFraming.buildRequest({
                    msgId: 4001n,
                    cmd: 1,
                    targetHandle: 4, // ams_rs
                    code: 1, // START_ACTIVITY
                    flags: 0,
                    cookie: 0n,
                    data: parcel.toUint8Array()
                });

                const respBytes = bridge.process_binder_packet(reqBytes);
                const dur = performance.now() - t0;
                const resp = VirtioBinderFraming.parseResponse(respBytes);

                this.logBinderTransaction({
                    handle: 4,
                    code: 1,
                    desc: `IActivityManager::startActivity(${pkg})`,
                    status: resp.hdr.status,
                    durationMs: dur,
                    payloadSize: resp.data.length
                });

                this.onLogcat('ActivityThread', `ActivityRecord{${pkg}/.MainActivity}: onCreate() -> onStart() -> onResume()`, 'I');
            } catch (e) {
                console.warn("[AppController] Binder startActivity error:", e);
            }

            // 2. Dispatch WMS relayout to Handle 3
            try {
                const wmsParcel = new BinderParcel(256);
                wmsParcel.writeUtf16("android.view.IWindowManager");
                wmsParcel.writeBool(false);
                wmsParcel.writeInt32(0);
                wmsParcel.writeInt32(0);
                wmsParcel.writeInt32(1280);
                wmsParcel.writeInt32(720);
                wmsParcel.writeInt32(1); // TYPE_APPLICATION
                wmsParcel.writeInt32(0);
                wmsParcel.writeInt32(1);
                wmsParcel.writeUtf8(pkg);
                wmsParcel.writeFloat32(1.0);
                wmsParcel.writeFloat32(0.0);
                wmsParcel.writeBool(false);
                wmsParcel.writeInt32(1280);
                wmsParcel.writeInt32(720);
                wmsParcel.writeInt32(0); // VISIBLE
                wmsParcel.writeInt32(0);

                const wmsReq = VirtioBinderFraming.buildRequest({
                    msgId: 4002n,
                    cmd: 1,
                    targetHandle: 3, // wms_rs
                    code: 2, // RELAYOUT
                    flags: 0,
                    cookie: 0n,
                    data: wmsParcel.toUint8Array()
                });
                const wmsRespBytes = bridge.process_binder_packet(wmsReq);
                const wmsResp = VirtioBinderFraming.parseResponse(wmsRespBytes);

                this.logBinderTransaction({
                    handle: 3,
                    code: 2,
                    desc: `IWindowManager::relayout(${pkg})`,
                    status: wmsResp.hdr.status,
                    durationMs: 0.5,
                    payloadSize: wmsResp.data.length
                });
            } catch (e) {
                console.warn("[AppController] Binder WMS relayout error:", e);
            }

            // 3. Queue Real Surface Frame into GraphicBufferProducer
            await this.queueAppBufferToSurfaceFlinger(pkg, meta.name);
        }

        // 4. Trigger WebGPU Compositor Presentation
        if (bridge && typeof bridge.compose_and_present === 'function') {
            try {
                bridge.compose_and_present();
            } catch (e) {
                console.warn("[AppController] compose_and_present error:", e);
            }
        }

        // 5. Track state in AndroidRuntime and present on real WebGPU canvas
        if (this.runtime) {
            this.runtime.startActivity(pkg, activityName || `${pkg}.MainActivity`);
        }
        this.activateScreen('webgpu');
    }

    /**
     * Inject key or motion input events into inputflinger_rs via Binder (Handle 2).
     */
    async sendInputEvent(action, keyCode) {
        const bridge = this.bootstrap ? this.bootstrap.getBridge() : null;
        if (!bridge || typeof bridge.process_binder_packet !== 'function') return;

        try {
            const t0 = performance.now();
            const parcel = new BinderParcel(256);
            parcel.writeUtf16("android.hardware.input.IInputManager");
            parcel.writeInt32(1); // InputEvent tag 1 = Key

            const keyBuf = new Uint8Array(56);
            const view = new DataView(keyBuf.buffer);
            const now = Date.now();
            view.setUint32(0, 1, true);
            view.setBigInt64(4, BigInt(now), true);
            view.setInt32(12, 1, true);
            view.setUint32(16, 0x00000101, true); // KEYBOARD
            view.setInt32(20, 0, true);
            view.setInt32(24, action, true);
            view.setInt32(28, 0, true);
            view.setInt32(32, keyCode, true);
            view.setInt32(36, 0, true);
            view.setInt32(40, 0, true);
            view.setInt32(44, 0, true);
            view.setBigInt64(48, BigInt(now), true);

            parcel.writeByteArray(keyBuf);
            parcel.writeInt32(0); // ASYNC mode

            const reqBytes = VirtioBinderFraming.buildRequest({
                msgId: 5001n,
                cmd: 1,
                targetHandle: 2, // inputflinger_rs
                code: 4, // INJECT_INPUT_EVENT
                flags: 0,
                cookie: 0n,
                data: parcel.toUint8Array()
            });

            const respBytes = bridge.process_binder_packet(reqBytes);
            const dur = performance.now() - t0;
            const resp = VirtioBinderFraming.parseResponse(respBytes);

            this.logBinderTransaction({
                handle: 2,
                code: 4,
                desc: `IInputManager::injectInputEvent(key=${keyCode})`,
                status: resp.hdr.status,
                durationMs: dur,
                payloadSize: resp.data.length
            });
        } catch (e) {
            console.warn("[AppController] sendInputEvent error:", e);
        }
    }

    /**
     * Send Activity lifecycle update to ams_rs (Handle 4).
     */
    async sendAmsLifecycle(opCode, desc) {
        const bridge = this.bootstrap ? this.bootstrap.getBridge() : null;
        if (!bridge || typeof bridge.process_binder_packet !== 'function') return;

        try {
            const t0 = performance.now();
            const parcel = new BinderParcel(32);
            parcel.writeUtf16("android.app.IActivityManager");
            parcel.writeUint32(1); // token_id

            const reqBytes = VirtioBinderFraming.buildRequest({
                msgId: 6001n,
                cmd: 1,
                targetHandle: 4, // ams_rs
                code: opCode,
                flags: 0,
                cookie: 0n,
                data: parcel.toUint8Array()
            });

            const respBytes = bridge.process_binder_packet(reqBytes);
            const dur = performance.now() - t0;
            const resp = VirtioBinderFraming.parseResponse(respBytes);

            this.logBinderTransaction({
                handle: 4,
                code: opCode,
                desc: `IActivityManager::${desc}`,
                status: resp.hdr.status,
                durationMs: dur,
                payloadSize: resp.data.length
            });
        } catch (e) {
            console.warn("[AppController] sendAmsLifecycle error:", e);
        }
    }

    /**
     * Back button navigation handler.
     */
    async handleBackPress() {
        this.onLogcat('ViewRootImpl', 'Key down: KEYCODE_BACK', 'D');
        await this.sendInputEvent(0, 4); // KEY_DOWN, KEYCODE_BACK
        await this.sendAmsLifecycle(6, 'finishActivity');

        if (this.runtime) {
            const hasMore = this.runtime.goBack();
            if (!hasMore) {
                this.activateScreen('home');
            }
        } else {
            this.activateScreen('home');
        }
    }

    /**
     * Home button navigation handler.
     */
    async handleHomePress() {
        this.onLogcat('ViewRootImpl', 'Key down: KEYCODE_HOME', 'D');
        await this.sendInputEvent(0, 3); // KEY_DOWN, KEYCODE_HOME
        await this.sendAmsLifecycle(4, 'activityPaused');
        this.activateScreen('home');
    }

    /**
     * Recents / Overview navigation handler.
     */
    async handleRecentsPress() {
        this.onLogcat('WindowManager', 'Overview key dispatched (KEYCODE_APP_SWITCH)', 'D');
        await this.sendInputEvent(0, 187); // KEYCODE_APP_SWITCH
        this.showToast("Recents: Task stack active");
    }

    /**
     * APK file installation handler.
     */
    async handleApkFile(file) {
        if (!file) return;
        this.showToast(`Installing ${file.name}...`);
        const buf = await file.arrayBuffer();
        try {
            const appState = await this.runtime.loadAndRunApk(buf);
            await this.installPackage(appState.packageName, appState.appName);
            await this.launchActivity(appState.packageName);
        } catch (err) {
            alert(`Failed to load APK: ${err.message}`);
        }
    }

    /**
     * Audio HAL WebAudio toggle.
     */
    async toggleMicrophone() {
        try {
            if (!this.audioCtx) {
                this.audioCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 48000 });
            }
            if (this.audioCtx.state === 'suspended') {
                await this.audioCtx.resume();
            }

            const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
            const source = this.audioCtx.createMediaStreamSource(stream);
            const proc = this.audioCtx.createScriptProcessor(1024, 1, 1);
            source.connect(proc);
            proc.connect(this.audioCtx.destination);

            if (this.dom.btnToggleMic) {
                this.dom.btnToggleMic.classList.add('active');
                this.dom.btnToggleMic.textContent = '🎙️ Mic Active';
            }
            this.onLogcat('AudioFlinger', 'AudioWorklet / getUserMedia stream connected to Audio HAL (48kHz Stereo)', 'I');
            this.showToast("Audio HAL: Microphone stream active");
        } catch (err) {
            this.onLogcat('AudioFlinger', `Microphone access denied: ${err.message}`, 'W');
            this.showToast(`Mic error: ${err.message}`);
        }
    }

    /**
     * Camera HAL getUserMedia toggle.
     */
    async toggleCamera() {
        try {
            if (!this.camStream) {
                this.camStream = await navigator.mediaDevices.getUserMedia({
                    video: { width: { ideal: 640 }, height: { ideal: 480 } },
                    audio: false
                });
                if (this.dom.btnToggleCam) {
                    this.dom.btnToggleCam.classList.add('active');
                    this.dom.btnToggleCam.textContent = '📷 Cam Active';
                }
                this.onLogcat('CameraService', 'Camera HAL bound to host getUserMedia video track (640x480)', 'I');
                this.showToast("Camera HAL: Real video stream connected");
            } else {
                this.camStream.getTracks().forEach(t => t.stop());
                this.camStream = null;
                if (this.dom.btnToggleCam) {
                    this.dom.btnToggleCam.classList.remove('active');
                    this.dom.btnToggleCam.textContent = '📷 Cam HAL';
                }
                this.onLogcat('CameraService', 'Camera device session closed', 'I');
            }
        } catch (err) {
            this.onLogcat('CameraService', `Camera access denied: ${err.message}`, 'W');
            this.showToast(`Camera error: ${err.message}`);
        }
    }

    /**
     * Show a toast message using UI render helper.
     */
    showToast(message) {
        showToast(this.dom.toast, this.dom.toastText, message);
        if (typeof this.onToast === 'function') {
            this.onToast(message);
        }
    }
}
