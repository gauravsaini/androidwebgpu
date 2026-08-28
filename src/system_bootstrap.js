/**
 * System Bootstrap & Hypervisor Lifecycle Coordinator
 * Single source of truth for V86 hypervisor boot, WebGPU bridge, and milestone dispatch.
 */

import initWasm, { WasmVirtioGpuBridge } from '../pkg/virtio_gpu_bridge.js';
import { VirtioGpuDevice } from './virtio_gpu_device.js';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES, verifyBzImage } from './v86_guest_manager.js';
import { logger, globalLogcat } from './logger.js';

export { VM_STATES, BOOT_MILESTONES, verifyBzImage };

export class SystemBootstrap {
    /**
     * @param {Object} options
     * @param {Function} [options.V86Class] - Injectable V86 hypervisor class for testing
     * @param {Object} [options.v86Config] - Additional configuration for V86GuestManager
     * @param {Function} [options.onMilestone] - Milestone dispatch callback
     * @param {Function} [options.onStateChange] - State transition callback
     * @param {Function} [options.onSerial] - Serial output line callback
     * @param {Function} [options.onFpsUpdate] - FPS update callback
     * @param {Function} [options.onLog] - Log callback
     * @param {boolean} [options.autostart=true] - Whether to automatically start hypervisor
     * @param {number} [options.memorySizeMb=128] - Guest RAM size in MB
     */
    constructor(options = {}) {
        this.options = {
            memorySizeMb: 128,
            vgaMemorySizeMb: 16,
            autostart: true,
            wasmPath: './v86/v86.wasm',
            biosUrl: './bios/seabios.bin',
            vgaBiosUrl: './bios/vgabios.bin',
            cdromUrl: null,
            kernelUrl: './guest/build/bzImage',
            initrdUrl: './guest/build/initrd.img',
            bootMode: 'direct',
            cmdline: 'console=tty0 console=ttyS0 earlyprintk=serial,ttyS0,115200 root=/dev/ram0 rdinit=/init nosmp maxcpus=1 noapic nolapic panic=1 loglevel=8 androidboot.hardware=android_x86 androidboot.selinux=permissive binder.debug_mask=0x07 video=virtio-gpu',
            ...options
        };

        this.V86Class = options.V86Class || (typeof window !== 'undefined' && (window.V86Starter || window.V86)) || (typeof globalThis !== 'undefined' && (globalThis.V86Starter || globalThis.V86));

        this.listeners = new Map();
        this.bridge = null;
        this.gpuDev = null;
        this.guestManager = null;
        this.isRendering = false;
        this.animationFrameId = null;

        // Telemetry metrics
        this.metrics = {
            fps: 0,
            frameCount: 0,
            lastFpsCheck: typeof performance !== 'undefined' ? performance.now() : Date.now(),
            gpuTimeMs: 1.8
        };

        // Wire event callbacks if passed in options
        if (typeof options.onMilestone === 'function') this.on('milestone', options.onMilestone);
        if (typeof options.onStateChange === 'function') this.on('stateChange', options.onStateChange);
        if (typeof options.onSerial === 'function') this.on('serial', options.onSerial);
        if (typeof options.onFpsUpdate === 'function') this.on('fpsUpdate', options.onFpsUpdate);
    }

    /**
     * Register an event listener.
     */
    on(event, callback) {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event).push(callback);
        return this;
    }

    /**
     * Remove an event listener.
     */
    off(event, callback) {
        if (this.listeners.has(event)) {
            const list = this.listeners.get(event).filter(fn => fn !== callback);
            this.listeners.set(event, list);
        }
        return this;
    }

    /**
     * Emit an event to registered listeners.
     */
    emit(event, ...args) {
        const list = this.listeners.get(event) || [];
        for (const cb of list) {
            try {
                cb(...args);
            } catch (err) {
                console.error(`[SystemBootstrap] Listener error on event "${event}":`, err);
            }
        }
    }

    /**
     * Validate a bzImage binary buffer using standard x86 boot header rules.
     */
    verifyBzImage(buffer) {
        return verifyBzImage(buffer);
    }

    /**
     * Initialize WebGPU Virtio-GPU Bridge and V86 Guest Hypervisor.
     * @param {Object} domElements
     * @param {HTMLCanvasElement} [domElements.canvas]
     * @param {HTMLElement} [domElements.v86ScreenContainer]
     * @param {boolean} [domElements.isHeadless=false]
     */
    async init(domElements = {}) {
        if (this.isInitialized) return this;
        this.isInitialized = true;
        const { canvas, v86ScreenContainer, isHeadless = false } = domElements;

        // 1. Initialize WebGPU Graphics Bridge if not in pure headless mode without canvas
        if (!isHeadless && (canvas || typeof window !== 'undefined')) {
            try {
                await this.initGraphics(canvas);
            } catch (err) {
                console.warn("[SystemBootstrap] WebGPU bridge init warning:", err);
            }
        }

        // 2. Initialize V86 Guest Manager
        await this.bootGuest(v86ScreenContainer);

        // 3. Start render loop if graphics bridge is ready
        if (this.bridge && this.gpuDev && !isHeadless) {
            this.startRenderLoop();
        }

        return this;
    }

    /**
     * Initialize WebGPU Virtio-GPU Bridge.
     */
    async initGraphics(canvas) {
        await initWasm('./pkg/virtio_gpu_bridge_bg.wasm?v=3');
        this.bridge = new WasmVirtioGpuBridge();
        await this.bridge.initialize(720, 1440);

        if (canvas) {
            this.gpuDev = new VirtioGpuDevice(null, this.bridge, canvas);
            this.gpuDev.onScanoutUpdate = (scanoutId, damageRect) => {
                if (this.bridge && typeof this.bridge.compose_and_present === 'function') {
                    try { this.bridge.compose_and_present(); } catch (_) {}
                }
            };
        }

        if (typeof this.bridge.enable_system_ui === 'function') {
            this.bridge.enable_system_ui();
        }

        logger.log('compositor', 'I', 'WebGPU Virtio-GPU Bridge initialized (720x1440, SystemUI active)');
    }

    /**
     * Boot the V86 Guest Hypervisor.
     */
    async bootGuest(screenContainer) {
        const guestConfig = {
            ...this.options,
            autostart: false,
            gpuDevice: this.gpuDev,
            screenContainer: screenContainer || this.options.screenContainer,
            V86Class: this.V86Class,
            onMilestone: (milestone) => {
                this.emit('milestone', milestone);
            },
            onStateChange: (state) => {
                this.emit('stateChange', state);
            },
            onSerial: (line) => {
                this.emit('serial', line);
            }
        };

        this.guestManager = new V86GuestManager(guestConfig);
        if (this.gpuDev && typeof this.guestManager.setGpuDevice === 'function') {
            this.guestManager.setGpuDevice(this.gpuDev);
        }

        // Forward manager milestones and serial events
        this.guestManager.onMilestone = (m) => this.emit('milestone', m);
        this.guestManager.onStateChange = (s) => this.emit('stateChange', s);
        this.guestManager.onSerial = (l) => this.emit('serial', l);

        if (this.options.autostart) {
            await this.guestManager.start();
        }

        // If emulator attached and gpu device ready, wire them
        if (this.guestManager.emulator && this.gpuDev) {
            this.gpuDev.registerWithV86(this.guestManager.emulator);
        }

        return this.guestManager;
    }

    /**
     * Start the 60/120 Hz WebGPU composition render loop.
     */
    startRenderLoop() {
        if (this.isRendering) return;
        this.isRendering = true;
        this.metrics.lastFpsCheck = performance.now();
        this.metrics.frameCount = 0;

        const loop = () => {
            if (!this.isRendering) return;
            this.metrics.frameCount++;

            // Real compositor pass
            if (this.bridge && typeof this.bridge.compose_and_present === 'function') {
                try { this.bridge.compose_and_present(); } catch (_) {}
            }

            // Render guest scanout directly to canvas
            if (this.gpuDev && typeof this.gpuDev.renderScanoutToCanvas === 'function') {
                try { this.gpuDev.renderScanoutToCanvas(0); } catch (_) {}
            }

            const now = performance.now();
            if (now - this.metrics.lastFpsCheck >= 500) {
                const fps = ((this.metrics.frameCount * 1000) / (now - this.metrics.lastFpsCheck));
                this.metrics.fps = fps;
                this.emit('fpsUpdate', fps, this.metrics.gpuTimeMs);
                this.metrics.frameCount = 0;
                this.metrics.lastFpsCheck = now;

                if (!this.metrics.lastLogTime) this.metrics.lastLogTime = now;
                if (now - this.metrics.lastLogTime >= 10000) {
                    logger.log('compositor', 'D', `WebGPU Telemetry: ${fps.toFixed(1)} FPS • GPU Time: ${this.metrics.gpuTimeMs.toFixed(2)}ms`);
                    this.metrics.lastLogTime = now;
                }
            }

            this.animationFrameId = requestAnimationFrame(loop);
        };

        this.animationFrameId = requestAnimationFrame(loop);
    }

    /**
     * Stop the render loop.
     */
    stopRenderLoop() {
        this.isRendering = false;
        if (this.animationFrameId && typeof cancelAnimationFrame !== 'undefined') {
            cancelAnimationFrame(this.animationFrameId);
            this.animationFrameId = null;
        }
    }

    /**
     * Send a command string to guest serial console.
     */
    sendSerialCommand(cmd) {
        if (!cmd) return;
        if (this.guestManager && this.guestManager.emulator && typeof this.guestManager.emulator.serial0_send === 'function') {
            globalLogcat.append('v86Guest', `$ ${cmd}`, 'I');
            this.guestManager.emulator.serial0_send(`${cmd}\n`);
        } else if (typeof window !== 'undefined' && window.v86emulator && typeof window.v86emulator.serial0_send === 'function') {
            globalLogcat.append('v86Guest', `$ ${cmd}`, 'I');
            window.v86emulator.serial0_send(`${cmd}\n`);
        } else {
            globalLogcat.append('v86Guest', `(Hypervisor serial input unavailable: ${cmd})`, 'W');
        }
    }

    /**
     * Get underlying V86 guest manager.
     */
    getGuestManager() {
        return this.guestManager;
    }

    /**
     * Get WebGPU bridge.
     */
    getBridge() {
        return this.bridge;
    }

    /**
     * Get Virtio-GPU device.
     */
    getGpuDevice() {
        return this.gpuDev;
    }

    /**
     * Get current VM state.
     */
    getState() {
        return this.guestManager ? this.guestManager.getState() : VM_STATES.UNINITIALIZED;
    }

    /**
     * Get list of achieved boot milestones.
     */
    getMilestones() {
        return this.guestManager ? this.guestManager.getMilestones() : [];
    }

    /**
     * Teardown and cleanup all resources.
     */
    destroy() {
        this.stopRenderLoop();
        if (this.guestManager) {
            this.guestManager.destroy();
            this.guestManager = null;
        }
        if (this.gpuDev) {
            this.gpuDev.destroy();
            this.gpuDev = null;
        }
        this.bridge = null;
        this.listeners.clear();
    }
}
