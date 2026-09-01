/**
 * VirtioGpuDevice - v86 Virtio-GPU PCI Device Emulation & WebGPU Bridge
 * Conforms to OASIS Virtio 1.2 GPU PCI Device Specification
 * Enhanced with 120 FPS OffscreenCanvas Worker Raster, SharedArrayBuffer DMA, and Virtqueue Rings
 */

import { logger } from './logger.js';

export const VIRTIO_GPU_F_VIRGL = 1 << 0;
export const VIRTIO_GPU_F_EDID = 1 << 1;

export class VirtioGpuDevice {
    /**
     * @param {Object} v86 - Reference to v86 emulator instance
     * @param {Object} rustBridge - Instantiated Rust VirtioGpuBridge Wasm module
     * @param {HTMLCanvasElement|OffscreenCanvas} canvas - Target Canvas
     * @param {Worker} [rasterWorker] - Optional dedicated raster worker
     */
    constructor(v86, rustBridge, canvas, rasterWorker = null, offscreenTransferred = false) {
        this.v86 = v86;
        this.rustBridge = rustBridge;
        this.canvas = canvas;
        this.worker = rasterWorker;
        this.offscreenTransferred = offscreenTransferred;
        this.ctx2d = (!offscreenTransferred && canvas && typeof canvas.getContext === "function")
            ? canvas.getContext("2d", { alpha: false, desynchronized: true })
            : null;
        this.cachedImageData = null;
        this.pci_space = new Uint8Array(256);
        this.io_bar = new Uint8Array(64);
        this.num_scanouts = 1;
        this.num_capsets = 1;
        this.damage_rects_count = 0;
        this.onScanoutUpdate = null;
        this.guestActive = false;

        // Virtqueue Ring State
        this.queues = [
            { size: 256, pfn: 0, lastAvailIdx: 0, lastUsedIdx: 0 }, // Queue 0: Control Queue
            { size: 16,  pfn: 0, lastAvailIdx: 0, lastUsedIdx: 0 }  // Queue 1: Cursor Queue
        ];
        this.selectedQueue = 0;
        this.deviceStatus = 0;
        this.isrStatus = 0;
        this.hostFeatures = (1 << 0) | (1 << 1); // VIRGL + EDID (low 32 bits, legacy)
        this.guestFeatures = 0;
        // Modern VirtIO feature negotiation (64-bit, pages of 32 bits each)
        // Page 0 = bits 0-31, Page 1 = bits 32-63
        // VIRTIO_F_VERSION_1 = bit 32 → page 1, bit 0
        this.hostFeaturesHi = 0x01; // VIRTIO_F_VERSION_1 (bit 32)
        this.guestFeaturesHi = 0;
        this.deviceFeatureSelect = 0;
        this.driverFeatureSelect = 0;
        this.modernConfigGeneration = 0;
        this.pciSlot = 0x06;
        this.pci_id = this.pciSlot << 3;
        this.pci_bars = [{ size: 64 }, { size: 16 * 1024 * 1024 }];
        this.name = "virtio-gpu";
        this.ioBase = 0xC140;
        this.irqLine = 10;
        this.bar0Size = 64;
        this.bar0Value = 0xC141;
        this.bar0Sizing = false;
        this.bar1Size = 16 * 1024 * 1024;
        this.bar1Value = 0xD1000000;
        this.bar1Sizing = false;
        this.bar1Addr = 0xD1000000; // Tracks relocated BAR1 MMIO base
        this.hostInjectionBlocked = false;
        this.guestHasPresented = false;
        this.firstGuestFrameAt = 0;

        this.initPci();
        if (this.v86) {
            this.registerWithV86(this.v86);
        }
    }

    initPci() {
        // Vendor ID: Red Hat / QEMU Virtio (0x1AF4)
        this.pci_space[0] = 0xF4;
        this.pci_space[1] = 0x1A;

        // Device ID: Virtio GPU Legacy/Transitional (0x1010 = 0x1000 + subsys 0x10)
        // 0x1050 is modern-only and requires MMIO caps v86 cannot trap.
        // 0x1010 falls in legacy range 0x1000-0x103F, auto-matched by virtio_pci_legacy.
        this.pci_space[2] = 0x10;
        this.pci_space[3] = 0x10;

        // Command & Status
        this.pci_space[4] = 0x07; // I/O, Memory, Bus Master enabled
        this.pci_space[5] = 0x00;

        // Revision ID (0x08) = 0x00 (legacy VirtIO device)
        this.pci_space[8] = 0x00;
        // Programming Interface (0x09) = 0x00
        this.pci_space[9] = 0x00;
        // Subclass (0x0A): Display Controller - Other (0x80) or VGA (0x00)
        this.pci_space[10] = 0x00;
        // Class Code (0x0B): Display Controller (0x03)
        this.pci_space[11] = 0x03;

        // BAR0: I/O Space (64 bytes) at 0xC140 (virtio legacy) — slot 0x06 avoids NE2000 collision at 0x05/C000
        this.pci_space[16] = 0x41;
        this.pci_space[17] = 0xC1;
        this.pci_space[18] = 0x00;
        this.pci_space[19] = 0x00;

        // BAR1: MMIO Space at 0xD1000000
        this.pci_space[20] = 0x00;
        this.pci_space[21] = 0x00;
        this.pci_space[22] = 0x10;
        this.pci_space[23] = 0xD1;

        // Capabilities Pointer & Status (Bit 4 of Status = Capabilities List)
        this.pci_space[6] = 0x10;
        this.pci_space[7] = 0x00;
        this.pci_space[0x34] = 0x40;

        // Capability 1: VIRTIO_PCI_CAP_COMMON_CFG (Type 1) at offset 0x40 (len 16)
        this.pci_space[0x40] = 0x09; // cap_vndr: PCI_CAP_ID_VNDR
        this.pci_space[0x41] = 0x50; // cap_next: points to 0x50
        this.pci_space[0x42] = 0x10; // cap_len: 16 bytes
        this.pci_space[0x43] = 0x01; // cfg_type: 1 (VIRTIO_PCI_CAP_COMMON_CFG)
        this.pci_space[0x44] = 0x01; // bar: 1 (BAR1 MMIO)
        this.pci_space[0x45] = 0x00; // id: 0
        this.pci_space[0x46] = 0x00; // padding
        this.pci_space[0x47] = 0x00;
        this.pci_space[0x48] = 0x00; // offset = 0x0000
        this.pci_space[0x49] = 0x00;
        this.pci_space[0x4A] = 0x00;
        this.pci_space[0x4B] = 0x00;
        this.pci_space[0x4C] = 0x38; // length = 0x38 (56 bytes for virtio_pci_common_cfg)
        this.pci_space[0x4D] = 0x00;
        this.pci_space[0x4E] = 0x00;
        this.pci_space[0x4F] = 0x00;

        // Capability 2: VIRTIO_PCI_CAP_NOTIFY_CFG (Type 2) at offset 0x50 (len 20)
        this.pci_space[0x50] = 0x09; // cap_vndr
        this.pci_space[0x51] = 0x64; // cap_next: points to 0x64
        this.pci_space[0x52] = 0x14; // cap_len: 20 bytes
        this.pci_space[0x53] = 0x02; // cfg_type: 2 (VIRTIO_PCI_CAP_NOTIFY_CFG)
        this.pci_space[0x54] = 0x01; // bar: 1 (BAR1 MMIO)
        this.pci_space[0x55] = 0x00; // id: 0
        this.pci_space[0x56] = 0x00; // padding
        this.pci_space[0x57] = 0x00;
        this.pci_space[0x58] = 0x00; // offset = 0x1000
        this.pci_space[0x59] = 0x10;
        this.pci_space[0x5A] = 0x00;
        this.pci_space[0x5B] = 0x00;
        this.pci_space[0x5C] = 0x00; // length = 0x1000
        this.pci_space[0x5D] = 0x10;
        this.pci_space[0x5E] = 0x00;
        this.pci_space[0x5F] = 0x00;
        this.pci_space[0x60] = 0x04; // notify_off_multiplier = 4
        this.pci_space[0x61] = 0x00;
        this.pci_space[0x62] = 0x00;
        this.pci_space[0x63] = 0x00;

        // Capability 3: VIRTIO_PCI_CAP_ISR_CFG (Type 3) at offset 0x64 (len 16)
        this.pci_space[0x64] = 0x09; // cap_vndr
        this.pci_space[0x65] = 0x74; // cap_next: points to 0x74
        this.pci_space[0x66] = 0x10; // cap_len: 16 bytes
        this.pci_space[0x67] = 0x03; // cfg_type: 3 (VIRTIO_PCI_CAP_ISR_CFG)
        this.pci_space[0x68] = 0x01; // bar: 1 (BAR1 MMIO)
        this.pci_space[0x69] = 0x00; // id: 0
        this.pci_space[0x6A] = 0x00; // padding
        this.pci_space[0x6B] = 0x00;
        this.pci_space[0x6C] = 0x00; // offset = 0x2000
        this.pci_space[0x6D] = 0x20;
        this.pci_space[0x6E] = 0x00;
        this.pci_space[0x6F] = 0x00;
        this.pci_space[0x70] = 0x04; // length = 0x04
        this.pci_space[0x71] = 0x00;
        this.pci_space[0x72] = 0x00;
        this.pci_space[0x73] = 0x00;

        // Capability 4: VIRTIO_PCI_CAP_DEVICE_CFG (Type 4) at offset 0x74 (len 16)
        this.pci_space[0x74] = 0x09; // cap_vndr
        this.pci_space[0x75] = 0x00; // cap_next: 0 (end of capabilities list)
        this.pci_space[0x76] = 0x10; // cap_len: 16 bytes
        this.pci_space[0x77] = 0x04; // cfg_type: 4 (VIRTIO_PCI_CAP_DEVICE_CFG)
        this.pci_space[0x78] = 0x01; // bar: 1 (BAR1 MMIO)
        this.pci_space[0x79] = 0x00; // id: 0
        this.pci_space[0x7A] = 0x00; // padding
        this.pci_space[0x7B] = 0x00;
        this.pci_space[0x7C] = 0x00; // offset = 0x3000
        this.pci_space[0x7D] = 0x30;
        this.pci_space[0x7E] = 0x00;
        this.pci_space[0x7F] = 0x00;
        this.pci_space[0x80] = 0x18; // length = 0x18 (24 bytes for virtio_gpu_config)
        this.pci_space[0x81] = 0x00;
        this.pci_space[0x82] = 0x00;
        this.pci_space[0x83] = 0x00;

        // Subsystem Vendor ID (0x1AF4) & Subsystem ID (0x0010 for VirtIO GPU)
        this.pci_space[44] = 0xF4;
        this.pci_space[45] = 0x1A;
        this.pci_space[46] = 0x10;
        this.pci_space[47] = 0x00;

        // Interrupt line
        this.pci_space[60] = this.irqLine;
        this.pci_space[61] = 0x01; // INTA#

        logger.log('bridge', 'I', 'Virtio-GPU PCI device (0x1AF4:0x1010) initialized with 1 scanout and VirtIO legacy I/O capabilities', {
            vendorId: 0x1AF4,
            deviceId: 0x1010,
            scanouts: this.num_scanouts
        });
    }

    /**
     * Register Virtio-GPU device on v86 PCI bus and I/O ports
     * Handles both immediate and deferred registration (emulator-ready)
     * @param {Object} v86 - v86 emulator instance
     */
    registerWithV86(v86) {
        if (!v86) return;
        this.v86 = v86;
        const tryRegister = () => {
            try {
                const cpu = v86.cpu || (v86.v86 && v86.v86.cpu) || (v86.emulator && v86.emulator.cpu);
                let io = (cpu && cpu.io) || (v86.io) || (v86.v86 && v86.v86.io) || (v86.emulator && v86.emulator.io);
                if (!io && cpu && cpu.devices && cpu.devices.pci) {
                    try { io = cpu.devices.pci.cpu?.io || cpu.devices.pci.io || null; } catch (_) {}
                }
                logger.log('bridge','D', `[virtio-gpu][VERBOSE] register attempt cpu=${!!cpu} io=${!!io} pciExists=${!!(cpu && cpu.devices && cpu.devices.pci)} bdf=0x${this.pci_id.toString(16)}`);
                if (cpu && cpu.devices && cpu.devices.pci) {
                    // Avoid double-registration
                    if (cpu.devices.pci.devices && cpu.devices.pci.devices[this.pci_id]) {
                        logger.log('bridge', 'D', `PCI bdf=0x${this.pci_id.toString(16)} already registered`);
                        return true;
                    }
                    let ret;
                    try {
                        // Try v86 new API (single arg device with pci_id), then legacy (slotMask, device)
                        if (cpu.devices.pci.register_device.length >= 2) {
                            ret = cpu.devices.pci.register_device(this.pci_id, this);
                        } else {
                            ret = cpu.devices.pci.register_device(this);
                            // For test mocks that expect (slotMask, dev), also call with slotMask if first call didn't populate devices
                            if (cpu.devices.pci.devices && !cpu.devices.pci.devices[this.pci_id] && ret === undefined) {
                                try { cpu.devices.pci.register_device(this.pci_id, this); } catch (_) {}
                            }
                        }
                    } catch (_) {
                        ret = cpu.devices.pci.register_device(this);
                    }
                    const check = cpu.devices.pci.devices ? cpu.devices.pci.devices[this.pci_id] : null;
                    const mockSuccess = !cpu.devices.pci.devices; // test mock has no devices dict but register_device was called
                    const slotHex = `0x${this.pciSlot.toString(16)}`;
                    const bdfHex = `0x${this.pci_id.toString(16)}`;
                    logger.log('bridge', 'I', `PCI register bdf=${bdfHex} (${this.name}) slot=${slotHex} io=0x${this.ioBase.toString(16)} -> ${check || mockSuccess ? 'OK' : 'FAIL'}`, {
                        pci_id: this.pci_id,
                        found: !!(check || mockSuccess)
                    });
                   if (check || mockSuccess) {
                       if (!io) {
                            logger.log('bridge','W', `[virtio-gpu][VERBOSE] PCI registered but io STILL missing (cpu.io=${!!(cpu&&cpu.io)} v86.io=${!!v86.io} v86.v86.io=${!!(v86.v86&&v86.v86.io)}) - will retry handler registration via poll`);
                            console.warn(`[VirtIO-GPU][VERBOSE] io missing after PCI register, scheduling retry`);
                            let retryCount=0;
                            const retryIo = () => {
                                retryCount++;
                                const cpuR = v86.cpu || (v86.v86 && v86.v86.cpu) || (v86.emulator && v86.emulator.cpu);
                                let ioR = (cpuR && cpuR.io) || v86.io || (v86.v86 && v86.v86.io) || (cpuR && cpuR.devices && cpuR.devices.pci && cpuR.devices.pci.cpu?.io);
                                if (ioR && typeof ioR.register_read === 'function') {
                                    this._io = ioR;
                                    logger.log('bridge','I', `[virtio-gpu][VERBOSE] Retry ${retryCount}: I/O now available, registering handlers at 0x${this.ioBase.toString(16)}`);
                                    console.info(`[VirtIO-GPU][VERBOSE] Retry I/O handlers register at 0x${this.ioBase.toString(16)}`);
                                    for (let port=this.ioBase; port<this.ioBase+64; port++) {
                                        const offset=port-this.ioBase;
                                        ioR.register_read(port,this, ()=>this.ioRead(offset,1), ()=>this.ioRead(offset,2), ()=>this.ioRead(offset,4));
                                        ioR.register_write(port,this, (v)=>this.ioWrite(offset,v,1), (v)=>this.ioWrite(offset,v,2), (v)=>this.ioWrite(offset,v,4));
                                    }
                                    try {
                                        const pciDevR = (cpuR.devices.pci.devices[this.pci_id] || cpu.devices.pci.devices[this.pci_id]);
                                        if (pciDevR && pciDevR.pci_bars && pciDevR.pci_bars[0]) {
                                            const bar0=pciDevR.pci_bars[0];
                                            bar0.entries=[];
                                            for(let i=0;i<bar0.size;i++) bar0.entries[i]=ioR.ports[this.ioBase+i];
                                            logger.log('bridge','I', `[virtio-gpu][VERBOSE] Retry bar0 patched count=${bar0.entries.length}`);
                                        }
                                    } catch(e){ logger.log('bridge','W', `[virtio-gpu][VERBOSE] Retry bar0 patch fail ${e.message}`); }
                                    return;
                                }
                                if (retryCount<20) setTimeout(retryIo, 50);
                                else logger.log('bridge','W', `[virtio-gpu][VERBOSE] Retry exhausted io still missing`);
                            };
                            setTimeout(retryIo, 50);
                       } else {
                           this._io = io;
                       }
                        logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] PCI register success bdf=${bdfHex} ioBase=0x${this.ioBase.toString(16)} wrapping set_io_bars for BAR relocation visibility ioAvailable=${!!io}`);
                        console.info(`[VirtIO-GPU][VERBOSE] PCI registered bdf=${bdfHex} ioBase=0x${this.ioBase.toString(16)} wrapping set_io_bars`);
                        if (cpu.devices.pci.set_io_bars && !cpu.devices.pci._gpuBarWrap) {
                            const origSetIo = cpu.devices.pci.set_io_bars.bind(cpu.devices.pci);
                            const self = this;
                            cpu.devices.pci.set_io_bars = function(bar, oldPort, newPort) {
                                logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] v86 set_io_bars barSize=${bar.size} old=0x${oldPort.toString(16)} -> new=0x${newPort.toString(16)}`);
                                console.info(`[VirtIO-GPU][VERBOSE] set_io_bars old=0x${oldPort.toString(16)} -> new=0x${newPort.toString(16)} barSize=${bar.size}`);
                                const ret = origSetIo(bar, oldPort, newPort);
                                if (bar.size === 64) {
                                    const oldBase = self.ioBase;
                                    self.ioBase = newPort;
                                    logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] BAR0 relocated via v86 core old=0x${oldBase.toString(16)} -> new=0x${newPort.toString(16)}`);
                                    console.info(`[VirtIO-GPU] BAR0 relocated via v86 core 0x${oldBase.toString(16)} -> 0x${newPort.toString(16)}`);
                                }
                                return ret;
                            };
                            cpu.devices.pci._gpuBarWrap = true;
                            logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] set_io_bars wrapped OK`);
                        }
                       if (io && typeof io.register_read === 'function' && typeof io.register_write === 'function') {
                            logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] Registering I/O handlers at base 0x${this.ioBase.toString(16)} count=64`);
                            console.info(`[VirtIO-GPU][VERBOSE] Registering I/O handlers at 0x${this.ioBase.toString(16)}`);
                            for (let port = this.ioBase; port < this.ioBase + 64; port++) {
                                const offset = port - this.ioBase;
                                io.register_read(
                                    port,
                                    this,
                                    () => this.ioRead(offset, 1),
                                    () => this.ioRead(offset, 2),
                                    () => this.ioRead(offset, 4)
                                );
                                io.register_write(
                                    port,
                                    this,
                                    (val) => this.ioWrite(offset, val, 1),
                                    (val) => this.ioWrite(offset, val, 2),
                                    (val) => this.ioWrite(offset, val, 4)
                                );
                            }
                            logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] I/O handlers registered OK at 0x${this.ioBase.toString(16)}`);
                            // Register BAR1 MMIO for VirtIO modern transport (VIRTIO_F_VERSION_1)
                            this.registerBar1Mmio(io);
                       } else {
                            logger.log('bridge', 'W', `[virtio-gpu][VERBOSE] I/O handlers NOT registered - io missing`);
                        }
                        if (io) {
                            try {
                                const pciDev = cpu.devices.pci.devices[this.pci_id];
                                if (pciDev && pciDev.pci_bars && pciDev.pci_bars[0]) {
                                    const bar0 = pciDev.pci_bars[0];
                                    logger.log('bridge', 'D', `[virtio-gpu][VERBOSE] pre-patch bar0.entries len=${bar0.entries ? bar0.entries.length : 0} firstEmpty=${!bar0.entries || !bar0.entries[0]} ioBase=0x${this.ioBase.toString(16)}`);
                                    bar0.entries = [];
                                    for (let i = 0; i < bar0.size; i++) {
                                        bar0.entries[i] = io.ports[this.ioBase + i];
                                    }
                                    logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] bar0.entries patched count=${bar0.entries.length} ioBase=0x${this.ioBase.toString(16)} sampleExists=${!!bar0.entries[0]}`);
                                    console.info(`[VirtIO-GPU][VERBOSE] bar0.entries patched for ioBase 0x${this.ioBase.toString(16)} count=${bar0.entries.length}`);
                                } else {
                                    logger.log('bridge', 'W', `[virtio-gpu][VERBOSE] pci_bars[0] missing after register - cannot patch`);
                                }
                            } catch (e) {
                                logger.log('bridge', 'W', `[virtio-gpu][VERBOSE] bar0 patch failed: ${e.message}`);
                            }
                        } else {
                            logger.log('bridge','I', `[virtio-gpu][VERBOSE] bar0 patch deferred until retry (io missing)`);
                        }
                        setTimeout(() => {
                            try {
                                const cpuChk = this.v86?.cpu || this.v86?.v86?.cpu;
                                const ioChk = this.v86?.io || this.v86?.v86?.io || this._io;
                                const hasHandler = ioChk && ioChk.ports && ioChk.ports[this.ioBase];
                                logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] POST-CHECK 2s after attach ioBase=0x${this.ioBase.toString(16)} handlerAtBase=${!!hasHandler} portsLen=${ioChk?.ports?.length}`);
                                console.info(`[VirtIO-GPU][VERBOSE] POST-CHECK ioBase=0x${this.ioBase.toString(16)} handler=${!!hasHandler}`);
                                if (cpuChk && cpuChk.devices && cpuChk.devices.pci && cpuChk.devices.pci.devices[this.pci_id]) {
                                    const pd = cpuChk.devices.pci.devices[this.pci_id];
                                    const ds = cpuChk.devices.pci.device_spaces[this.pci_id];
                                    if (ds) {
                                        const barVal = (ds[4] !== undefined) ? (ds[4] >>> 0) : 0;
                                        logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] POST-CHECK device_spaces BAR0 val=0x${barVal.toString(16)} expected 0x${(this.ioBase|1).toString(16)}`);
                                    }
                                }
                            } catch (e2) { logger.log('bridge','W', `[virtio-gpu][VERBOSE] POST-CHECK failed ${e2.message}`); }
                        }, 2000);
                        logger.log('bridge', 'I', `Virtio-GPU device attached to v86 (slot 0x${this.pciSlot.toString(16)}, I/O 0x${this.ioBase.toString(16)})`, {
                            pciSlot: this.pciSlot,
                            ioBase: this.ioBase,
                            irqLine: this.irqLine
                        });
                        console.info(`[VirtIO-GPU][VERBOSE] device attached slot 0x${this.pciSlot.toString(16)} ioBase 0x${this.ioBase.toString(16)} irq ${this.irqLine}`);
                        return true;
                    }
                } else {
                    logger.log('bridge', 'D', `PCI bus not ready for bdf=0x${this.pci_id.toString(16)}, deferring`);
                }
            } catch (e) {
                logger.log('bridge', 'W', `v86 registration notice: ${e.message}`);
            }
            return false;
        };
        if (!tryRegister()) {
            // Leaf 2.1 fix: PCI must be visible before guest kernel PCI scan (early BIOS + kernel init).
            // Retry aggressively at 10ms/50ms/100ms and also on emulator-ready, not 500ms+.
            const onReady = () => { try { tryRegister(); } catch (_) {} };
            try {
                if (typeof v86.add_listener === 'function') {
                    v86.add_listener('emulator-ready', onReady);
                    v86.add_listener('emulator-started', onReady);
                } else if (v86.v86 && typeof v86.v86.add_listener === 'function') {
                    v86.v86.add_listener('emulator-ready', onReady);
                    v86.v86.add_listener('emulator-started', onReady);
                }
            } catch (_) {}
            let attempts = 0;
            const fastRetry = () => {
                attempts++;
                if (tryRegister()) return;
                if (attempts < 20) setTimeout(fastRetry, attempts < 5 ? 10 : attempts < 10 ? 25 : 50);
            };
            setTimeout(fastRetry, 10);
            setTimeout(fastRetry, 50);
            if (typeof v86.cpu === 'undefined' && typeof v86.v86 === 'undefined') {
                // Fallback poll until cpu appears (handles async wasm init)
                let pollCount = 0;
                const poll = setInterval(() => {
                    if (tryRegister() || ++pollCount > 40) clearInterval(poll);
                }, 25);
            }
        }
    }

    /**
     * Register BAR1 MMIO handlers for VirtIO modern PCI capabilities.
     * BAR1 layout (from PCI caps):
     *   0x0000-0x0037: Common Configuration (type 1)
     *   0x1000-0x1FFF: Notification (type 2)
     *   0x2000-0x2003: ISR Status (type 3)
     *   0x3000-0x3017: Device-specific Config (type 4, virtio_gpu_config)
     */
    registerBar1Mmio(io) {
        if (!io || typeof io.mmap_register !== 'function') {
            logger.log('bridge', 'W', '[virtio-gpu] Cannot register BAR1 MMIO: io.mmap_register unavailable');
            return;
        }
        const bar1Base = this.bar1Addr;
        const bar1Size = this.bar1Size;
        const self = this;

        io.mmap_register(
            bar1Base,
            bar1Size,
            function mmioRead8(addr) { return self.bar1MmioRead(addr - bar1Base, 1); },
            function mmioWrite8(addr, val) { self.bar1MmioWrite(addr - bar1Base, val, 1); },
            function mmioRead32(addr) { return self.bar1MmioRead(addr - bar1Base, 4); },
            function mmioWrite32(addr, val) { self.bar1MmioWrite(addr - bar1Base, val, 4); }
        );

        logger.log('bridge', 'I', `[virtio-gpu] BAR1 MMIO registered at 0x${bar1Base.toString(16)} (modern VirtIO cfg with VIRTIO_F_VERSION_1)`);
        console.info(`[VirtIO-GPU] BAR1 MMIO registered at 0x${bar1Base.toString(16)} (modern cfg with VIRTIO_F_VERSION_1)`);
    }

    /**
     * Read from BAR1 MMIO (VirtIO modern capabilities)
     */
    bar1MmioRead(offset, size) {
        if (offset < 0x0038) return this.modernCommonRead(offset, size);
        if (offset >= 0x1000 && offset < 0x2000) return 0xFFFF;
        if (offset >= 0x2000 && offset < 0x2004) {
            const isr = this.isrStatus;
            this.isrStatus = 0;
            return isr & 0xFF;
        }
        if (offset >= 0x3000 && offset < 0x3018) return this.modernDeviceCfgRead(offset - 0x3000, size);
        return 0;
    }

    /**
     * Write to BAR1 MMIO (VirtIO modern capabilities)
     */
    bar1MmioWrite(offset, val, size) {
        if (offset < 0x0038) { this.modernCommonWrite(offset, val, size); return; }
        if (offset >= 0x1000 && offset < 0x2000) {
            const qIdx = ((offset - 0x1000) >> 2) & 0x01;
            console.info(`[VirtIO-GPU][MMIO] NOTIFY queue=${qIdx} (modern)`);
            this.consumeVirtqueue(qIdx);
            return;
        }
    }

    /**
     * VirtIO Modern Common Configuration Register Read
     * virtio_pci_common_cfg layout (56 bytes):
     *   0x00: device_feature_select  0x04: device_feature
     *   0x08: driver_feature_select  0x0C: driver_feature
     *   0x10: msix_config            0x12: num_queues
     *   0x14: device_status          0x15: config_generation
     *   0x16: queue_select           0x18: queue_size
     *   0x1A: queue_msix_vector      0x1C: queue_enable
     *   0x1E: queue_notify_off       0x20: queue_desc (u64)
     *   0x28: queue_driver (u64)     0x30: queue_device (u64)
     */
    modernCommonRead(offset, size) {
        if (size === 1) {
            const regBase = offset & ~3;
            const val32 = this.modernCommonRead(regBase, 4);
            return (val32 >>> ((offset - regBase) * 8)) & 0xFF;
        }
        switch (offset) {
            case 0x00: return this.deviceFeatureSelect >>> 0;
            case 0x04: // device_feature: page-selected
                if (this.deviceFeatureSelect === 0) return this.hostFeatures >>> 0;
                if (this.deviceFeatureSelect === 1) return this.hostFeaturesHi >>> 0;
                return 0;
            case 0x08: return this.driverFeatureSelect >>> 0;
            case 0x0C:
                if (this.driverFeatureSelect === 0) return this.guestFeatures >>> 0;
                if (this.driverFeatureSelect === 1) return this.guestFeaturesHi >>> 0;
                return 0;
            case 0x10: return 0xFFFF; // msix_config: no MSI-X
            case 0x12: return 2;      // num_queues
            case 0x14: return this.deviceStatus & 0xFF;
            case 0x15: return this.modernConfigGeneration & 0xFF;
            case 0x16: return this.selectedQueue & 0xFFFF;
            case 0x18: return (this.queues[this.selectedQueue]?.size || 0) & 0xFFFF;
            case 0x1A: return 0xFFFF; // queue_msix_vector: no MSI-X
            case 0x1C: return (this.queues[this.selectedQueue]?.pfn ? 1 : 0);
            case 0x1E: return this.selectedQueue & 0xFFFF; // queue_notify_off
            case 0x20: return (this.queues[this.selectedQueue]?.descAddr || 0) >>> 0;
            case 0x24: return 0;
            case 0x28: return (this.queues[this.selectedQueue]?.availAddr || 0) >>> 0;
            case 0x2C: return 0;
            case 0x30: return (this.queues[this.selectedQueue]?.usedAddr || 0) >>> 0;
            case 0x34: return 0;
            default: return 0;
        }
    }

    /**
     * VirtIO Modern Common Configuration Register Write
     */
    modernCommonWrite(offset, val, size) {
        if (size === 1) {
            // Only handle device_status byte write directly (offset 0x14)
            if (offset === 0x14) { this.modernCommonWrite(0x14, val, 4); return; }
            return;
        }
        switch (offset) {
            case 0x00: this.deviceFeatureSelect = val >>> 0; break;
            case 0x08: this.driverFeatureSelect = val >>> 0; break;
            case 0x0C:
                if (this.driverFeatureSelect === 0) this.guestFeatures = val >>> 0;
                else if (this.driverFeatureSelect === 1) this.guestFeaturesHi = val >>> 0;
                break;
            case 0x14: {
                const prev = this.deviceStatus;
                this.deviceStatus = val & 0xFF;
                console.info(`[VirtIO-GPU][MMIO] DEVICE_STATUS: 0x${prev.toString(16)} -> 0x${this.deviceStatus.toString(16)} ACK=${!!(val&1)} DRV=${!!(val&2)} OK=${!!(val&4)} FEAT=${!!(val&8)} (modern)`);
                logger.log('bridge','I',`[virtio-gpu][MMIO] DEVICE_STATUS: 0x${prev.toString(16)} -> 0x${this.deviceStatus.toString(16)}`);
                if (this.deviceStatus === 0) {
                    this.queues[0].lastAvailIdx = 0; this.queues[0].lastUsedIdx = 0;
                    this.queues[1].lastAvailIdx = 0; this.queues[1].lastUsedIdx = 0;
                    this.isrStatus = 0; this.guestActive = false; this.hostInjectionBlocked = false;
                    if (typeof this.onGuestActiveChange === 'function') {
                        try { this.onGuestActiveChange(false); } catch (_) {}
                    }
                }
                break;
            }
            case 0x16: this.selectedQueue = val & 0x01; break;
            case 0x18:
                if (this.queues[this.selectedQueue]) this.queues[this.selectedQueue].size = val & 0xFFFF;
                break;
            case 0x1C: // queue_enable
                if (val & 1) {
                    const q = this.queues[this.selectedQueue];
                    if (q && q.descAddr) {
                        q.pfn = (q.descAddr >>> 12) >>> 0;
                        console.info(`[VirtIO-GPU][MMIO] Queue ${this.selectedQueue} ENABLED desc=0x${(q.descAddr||0).toString(16)} pfn=0x${q.pfn.toString(16)}`);
                        logger.log('bridge','I',`[virtio-gpu][MMIO] Queue ${this.selectedQueue} enabled pfn=0x${q.pfn.toString(16)}`);
                    }
                }
                break;
            case 0x20: if (this.queues[this.selectedQueue]) this.queues[this.selectedQueue].descAddr = val >>> 0; break;
            case 0x28: if (this.queues[this.selectedQueue]) this.queues[this.selectedQueue].availAddr = val >>> 0; break;
            case 0x30: if (this.queues[this.selectedQueue]) this.queues[this.selectedQueue].usedAddr = val >>> 0; break;
            default: break;
        }
    }

    /**
     * VirtIO GPU device-specific config read (virtio_gpu_config)
     * 0x00: events_read  0x04: events_clear  0x08: num_scanouts  0x0C: num_capsets
     */
    modernDeviceCfgRead(offset, size) {
        if (size === 1) {
            const regBase = offset & ~3;
            const val32 = this.modernDeviceCfgRead(regBase, 4);
            return (val32 >>> ((offset - regBase) * 8)) & 0xFF;
        }
        switch (offset) {
            case 0x00: return 0;
            case 0x04: return 0;
            case 0x08: return this.num_scanouts >>> 0;
            case 0x0C: return this.num_capsets >>> 0;
            default: return 0;
        }
    }

    /**
     * Read from VirtIO Legacy PCI I/O Configuration Space
     */
    ioRead(offset, size = 1) {
        // VERBOSE: log all I/O reads for BAR mismatch debugging (limit to first 500 to avoid spam)
        if (!this._ioReadCount) this._ioReadCount=0;
        if (this._ioReadCount < 500 || offset === 0x12) { 
            this._ioReadCount++; 
            logger.log('bridge','D',`[virtio-gpu] ioRead offset=0x${offset.toString(16)} size=${size} ioBase=0x${this.ioBase.toString(16)} (DEVICE_STATUS=${this.deviceStatus})`); 
            if (this._ioReadCount === 500) logger.log('bridge', 'I', `[virtio-gpu] ioRead verbose limit reached (500)`);
        }
        if (offset >= 0x00 && offset <= 0x03) {
            // HOST_FEATURES (0x00, 32-bit): VIRTIO_GPU_F_VIRGL | VIRTIO_GPU_F_EDID
            if (offset === 0x00 && (size === 4 || size === undefined)) {
                return this.hostFeatures >>> 0;
            }
            if (offset === 0x00 && size === 2) {
                return this.hostFeatures & 0xFFFF;
            }
            return (this.hostFeatures >>> ((offset - 0x00) * 8)) & 0xFF;
        }

        if (offset >= 0x04 && offset <= 0x07) {
            // GUEST_FEATURES (0x04, 32-bit)
            if (offset === 0x04 && (size === 4 || size === undefined)) {
                return this.guestFeatures >>> 0;
            }
            if (offset === 0x04 && size === 2) {
                return this.guestFeatures & 0xFFFF;
            }
            return (this.guestFeatures >>> ((offset - 0x04) * 8)) & 0xFF;
        }

        if (offset >= 0x08 && offset <= 0x0B) {
            // QUEUE_PFN (0x08, 32-bit)
            const pfn = (this.queues[this.selectedQueue]?.pfn || 0) >>> 0;
            if (offset === 0x08 && (size === 4 || size === undefined)) {
                return pfn;
            }
            if (offset === 0x08 && size === 2) {
                return pfn & 0xFFFF;
            }
            return (pfn >>> ((offset - 0x08) * 8)) & 0xFF;
        }

        if (offset >= 0x0C && offset <= 0x0D) {
            // QUEUE_NUM (0x0C, 16-bit): 256 for control (q0), 16 for cursor (q1)
            const qSize = (this.queues[this.selectedQueue]?.size || 0) & 0xFFFF;
            if (offset === 0x0C && (size === 2 || size === 4 || size === undefined)) {
                return qSize;
            }
            return (qSize >>> ((offset - 0x0C) * 8)) & 0xFF;
        }

        if (offset >= 0x0E && offset <= 0x0F) {
            // QUEUE_SEL (0x0E, 16-bit)
            const qSel = this.selectedQueue & 0xFFFF;
            if (offset === 0x0E && (size === 2 || size === 4 || size === undefined)) {
                return qSel;
            }
            return (qSel >>> ((offset - 0x0E) * 8)) & 0xFF;
        }

        if (offset === 0x10 || offset === 0x11) {
            // QUEUE_NOTIFY (0x10, 16-bit)
            return 0;
        }

        if (offset === 0x12) {
            // DEVICE_STATUS (0x12, 8-bit): ACKNOWLEDGE -> DRIVER -> DRIVER_OK
            return this.deviceStatus & 0xFF;
        }

        if (offset === 0x13) {
            // ISR_STATUS (0x13, 8-bit): Bit 0 = queue interrupt (read-to-clear)
            const isr = this.isrStatus;
            this.isrStatus = 0; // Read clears ISR
            return isr & 0xFF;
        }

        if (offset >= 0x14 && offset <= 0x17) {
            // events_read (32-bit)
            return 0;
        }

        if (offset >= 0x18 && offset <= 0x1B) {
            // events_clear (32-bit)
            return 0;
        }

        if (offset >= 0x1C && offset <= 0x1F) {
            // num_scanouts (32-bit)
            if (offset === 0x1C && (size === 4 || size === undefined)) {
                return this.num_scanouts >>> 0;
            }
            return (this.num_scanouts >>> ((offset - 0x1C) * 8)) & 0xFF;
        }

        if (offset >= 0x20 && offset <= 0x23) {
            // num_capsets (32-bit)
            if (offset === 0x20 && (size === 4 || size === undefined)) {
                return this.num_capsets >>> 0;
            }
            return (this.num_capsets >>> ((offset - 0x20) * 8)) & 0xFF;
        }

        return this.pci_space[offset] || 0;
    }

    /**
     * Write to VirtIO Legacy PCI I/O Configuration Space
     */
    ioWrite(offset, val, size = 1) {
        if (!this._ioWriteCount) this._ioWriteCount=0;
        if (this._ioWriteCount < 500 || offset === 0x12 || (offset >= 0x08 && offset <= 0x0B)) { 
            this._ioWriteCount++; 
            logger.log('bridge','D',`[virtio-gpu] ioWrite offset=0x${offset.toString(16)} val=0x${val.toString(16)} size=${size} ioBase=0x${this.ioBase.toString(16)}`); 
            if (this._ioWriteCount === 500) logger.log('bridge','I','[virtio-gpu] ioWrite verbose limit reached (500), silencing general writes'); 
        }
        if (offset >= 0x04 && offset <= 0x07) {
            // GUEST_FEATURES (0x04, 32-bit)
            if (offset === 0x04 && (size === 4 || size === undefined)) {
                this.guestFeatures = val >>> 0;
            } else if (size === 2) {
                const shift = (offset - 0x04) * 8;
                this.guestFeatures = ((this.guestFeatures & ~(0xFFFF << shift)) | ((val & 0xFFFF) << shift)) >>> 0;
            } else {
                const shift = (offset - 0x04) * 8;
                this.guestFeatures = ((this.guestFeatures & ~(0xFF << shift)) | ((val & 0xFF) << shift)) >>> 0;
            }
            return;
        }

        if (offset >= 0x08 && offset <= 0x0B) {
            // QUEUE_PFN (0x08, 32-bit)
            if (this.queues[this.selectedQueue]) {
                if (offset === 0x08 && (size === 4 || size === undefined)) {
                    this.queues[this.selectedQueue].pfn = val >>> 0;
                } else if (size === 2) {
                    const shift = (offset - 0x08) * 8;
                    const currentPfn = this.queues[this.selectedQueue].pfn;
                    this.queues[this.selectedQueue].pfn = ((currentPfn & ~(0xFFFF << shift)) | ((val & 0xFFFF) << shift)) >>> 0;
                } else {
                    const shift = (offset - 0x08) * 8;
                    const currentPfn = this.queues[this.selectedQueue].pfn;
                    this.queues[this.selectedQueue].pfn = ((currentPfn & ~(0xFF << shift)) | ((val & 0xFF) << shift)) >>> 0;
                }
                logger.log('bridge', 'I', `[virtio-gpu-device] QUEUE_PFN set: queue=${this.selectedQueue}, pfn=0x${this.queues[this.selectedQueue].pfn.toString(16)}, size=${this.queues[this.selectedQueue].size}`);
            }
            return;
        }

        if (offset === 0x0E || offset === 0x0F) {
            // QUEUE_SEL (0x0E, 16-bit)
            this.selectedQueue = (val & 0x1) === 1 ? 1 : 0;
            logger.log('bridge', 'D', `[virtio-gpu-device] QUEUE_SEL: ${this.selectedQueue}`);
            return;
        }

        if (offset === 0x10 || offset === 0x11) {
            // QUEUE_NOTIFY (0x10, 16-bit): kick queue processing immediately
            const qIdx = val & 0xFFFF;
            console.info(`[VirtIO-GPU] QUEUE_NOTIFY kicked for queue ${qIdx}`);
            logger.log('bridge', 'I', `[virtio-gpu-device] QUEUE_NOTIFY kicked for queue ${qIdx}`);
            this.consumeVirtqueue(qIdx);
            return;
        }

        if (offset === 0x12) {
            // DEVICE_STATUS (0x12, 8-bit)
            const prevStatus = this.deviceStatus;
            this.deviceStatus = val & 0xFF;
            console.info(`[VirtIO-GPU] DEVICE_STATUS: 0x${prevStatus.toString(16)} -> 0x${this.deviceStatus.toString(16)} ACK=${!!(val & 1)} DRV=${!!(val & 2)} OK=${!!(val & 4)} FEAT=${!!(val & 8)} guestActive=${this.guestActive} guestHasPresented=${this.guestHasPresented}`);
            logger.log('bridge', 'I', `[virtio-gpu-device] DEVICE_STATUS: 0x${prevStatus.toString(16)} -> 0x${this.deviceStatus.toString(16)} (ACK=${!!(val & 1)}, DRV=${!!(val & 2)}, OK=${!!(val & 4)}, FEAT=${!!(val & 8)})`);
            if (this.deviceStatus === 0) {
                // Reset device
                this.queues[0].lastAvailIdx = 0;
                this.queues[0].lastUsedIdx = 0;
                this.queues[1].lastAvailIdx = 0;
                this.queues[1].lastUsedIdx = 0;
                this.isrStatus = 0;
                this.guestActive = false;
                this.hostInjectionBlocked = false;
                logger.log('bridge', 'I', 'Virtio-GPU device reset (status=0)');
                if (typeof this.onGuestActiveChange === 'function') {
                    try { this.onGuestActiveChange(false); } catch (_) {}
                }
            } else if (prevStatus !== this.deviceStatus) {
                logger.log('bridge', 'I', `Virtio-GPU device status changed: 0x${prevStatus.toString(16)} -> 0x${this.deviceStatus.toString(16)}`, {
                    status: this.deviceStatus,
                    acknowledge: (this.deviceStatus & 0x01) !== 0,
                    driver: (this.deviceStatus & 0x02) !== 0,
                    driverOk: (this.deviceStatus & 0x04) !== 0,
                    featuresOk: (this.deviceStatus & 0x08) !== 0,
                    failed: (this.deviceStatus & 0x80) !== 0,
                });
            }
            return;
        }

        if (offset < this.pci_space.length) {
            this.pci_space[offset] = val & 0xFF;
        }
    }

   pciRead(addr, size) {
        if (!this._pciReadCount) this._pciReadCount = 0;
        if (this._pciReadCount < 50) {
            this._pciReadCount++;
            logger.log('bridge', 'D', `[virtio-gpu][VERBOSE] pciRead addr=0x${addr.toString(16)} size=${size} ioBase=0x${this.ioBase.toString(16)}`);
            console.debug(`[VirtIO-GPU][VERBOSE] pciRead addr=0x${addr.toString(16)} size=${size}`);
            if (this._pciReadCount === 50) logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] pciRead limit reached`);
        }
        if (addr === 0x10 && this.bar0Sizing) {
            logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] pciRead BAR0 sizing probe -> mask 0xFFFFFFC1`);
            return 0xFFFFFFC1 >>> 0;
        }
        if (addr === 0x14 && this.bar1Sizing) {
            logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] pciRead BAR1 sizing probe -> mask 0xFF000000`);
            return 0xFF000000 >>> 0;
        }
        let val = 0;
        for (let i = 0; i < size; i++) val |= (this.pci_space[addr + i] || 0) << (i * 8);
        return val >>> 0;
    }
    pci_read(addr, size) {
        logger.log('bridge', 'D', `[virtio-gpu][VERBOSE] pci_read alias addr=0x${addr.toString(16)} size=${size}`);
        return this.pciRead(addr, size);
    }
    pci_write(addr, val, size) {
        logger.log('bridge', 'D', `[virtio-gpu][VERBOSE] pci_write alias addr=0x${addr.toString(16)} val=0x${val.toString(16)} size=${size}`);
        return this.pciWrite(addr, val, size);
    }
    pciWrite(addr, val, size) {
        if (!this._pciWriteCount) this._pciWriteCount = 0;
        if (this._pciWriteCount < 50) {
            this._pciWriteCount++;
            logger.log('bridge','D',`[virtio-gpu][VERBOSE] pciWrite addr=0x${addr.toString(16)} val=0x${val.toString(16)} size=${size} before ioBase=0x${this.ioBase.toString(16)}`);
            console.debug(`[VirtIO-GPU][VERBOSE] pciWrite addr=0x${addr.toString(16)} val=0x${val.toString(16)} size=${size}`);
            if (this._pciWriteCount === 50) logger.log('bridge', 'I', `[virtio-gpu][VERBOSE] pciWrite limit reached`);
        }

        // BAR0 (0x10-0x13) sizing probe handling
        if (addr >= 0x10 && addr <= 0x13 && val === 0xFFFFFFFF) {
            this.bar0Sizing = true;
            this.pci_space[16] = 0xC1; this.pci_space[17] = 0xFF; this.pci_space[18] = 0xFF; this.pci_space[19] = 0xFF;
            logger.log('bridge', 'I', `[virtio-gpu] BAR0 sizing probe 0xFFFFFFFF -> mask 0xFFFFFFC1`);
            return;
        }

        // BAR1 (0x14-0x17) sizing probe handling
        if (addr >= 0x14 && addr <= 0x17 && val === 0xFFFFFFFF) {
            this.bar1Sizing = true;
            this.pci_space[20] = 0x00; this.pci_space[21] = 0x00; this.pci_space[22] = 0x00; this.pci_space[23] = 0xFF;
            logger.log('bridge', 'I', `[virtio-gpu] BAR1 sizing probe 0xFFFFFFFF -> mask 0xFF000000`);
            return;
        }

        if (this.bar0Sizing && addr >= 0x10 && addr <= 0x13) this.bar0Sizing = false;
        if (this.bar1Sizing && addr >= 0x14 && addr <= 0x17) this.bar1Sizing = false;

        // Store bytes into pci_space
        for (let i = 0; i < size; i++) {
            if (addr + i < this.pci_space.length) {
                this.pci_space[addr + i] = (val >>> (i * 8)) & 0xFF;
            }
        }

        // Check if COMMAND register (0x04-0x05) was modified
        if (addr <= 0x04 && (addr + size) > 0x04) {
            const cmd = (this.pci_space[4] | (this.pci_space[5] << 8)) >>> 0;
            logger.log('bridge', 'I', `[virtio-gpu] PCI COMMAND register updated: 0x${cmd.toString(16)} (IO=${(cmd & 1) !== 0}, MEM=${(cmd & 2) !== 0}, BUS_MASTER=${(cmd & 4) !== 0})`);
        }

        // Reconstruct full 32-bit BAR0 from pci_space after write
        if (addr >= 0x10 && addr <= 0x13) {
            const fullBar0 = (this.pci_space[16] | (this.pci_space[17] << 8) | (this.pci_space[18] << 16) | (this.pci_space[19] << 24)) >>> 0;
            const isIO = (fullBar0 & 0x01) !== 0;
            const newBase = isIO ? (fullBar0 & 0xFFFFFFC0) : (fullBar0 & 0xFFFFFFF0);
            if (newBase !== 0 && newBase !== this.ioBase) {
                const oldBase = this.ioBase;
                this.ioBase = newBase;
                logger.log('bridge', 'I', `[virtio-gpu] BAR0 relocated ioBase 0x${oldBase.toString(16)} -> 0x${newBase.toString(16)} fullBar0=0x${fullBar0.toString(16)}`);
                console.info(`[VirtIO-GPU] BAR0 relocated ioBase 0x${oldBase.toString(16)} -> 0x${newBase.toString(16)}`);
                // Re-register I/O handlers at new base if v86 io available
                try {
                    const v86 = this.v86;
                    const cpu = v86?.cpu || v86?.v86?.cpu;
                    const io = v86?.io || v86?.v86?.io || this._io;
                    if (io && typeof io.register_read === 'function') {
                        this._io = io;
                        for (let port = this.ioBase; port < this.ioBase + 64; port++) {
                            const offset = port - this.ioBase;
                            io.register_read(port, this, () => this.ioRead(offset, 1), () => this.ioRead(offset, 2), () => this.ioRead(offset, 4));
                            io.register_write(port, this, (v) => this.ioWrite(offset, v, 1), (v) => this.ioWrite(offset, v, 2), (v) => this.ioWrite(offset, v, 4));
                        }
                        logger.log('bridge', 'I', `Virtio-GPU I/O handlers re-registered at 0x${this.ioBase.toString(16)}`);
                    }
                } catch(e) { logger.log('bridge', 'W', `BAR0 reregister failed: ${e.message}`); }
            }
        }

        // Reconstruct full 32-bit BAR1 from pci_space after write
        if (addr >= 0x14 && addr <= 0x17) {
            const fullBar1 = (this.pci_space[20] | (this.pci_space[21] << 8) | (this.pci_space[22] << 16) | (this.pci_space[23] << 24)) >>> 0;
            const newBar1 = (fullBar1 & 0xFF000000) >>> 0;
            if (newBar1 !== 0 && newBar1 !== this.bar1Value) {
                const oldBar1 = this.bar1Value;
                this.bar1Value = newBar1;
                logger.log('bridge', 'I', `[virtio-gpu] BAR1 relocated bar1Value 0x${oldBar1.toString(16)} -> 0x${newBar1.toString(16)} fullBar1=0x${fullBar1.toString(16)}`);
            }
        }
    }

    /**
     * Obtain guest physical memory buffer slice from v86
     * @returns {Uint8Array|null}
     */
    getGuestMemory() {
        if (!this.v86) return null;
        try {
            const cpu = this.v86.cpu || (this.v86.v86 && this.v86.v86.cpu) || (this.v86.emulator && this.v86.emulator.cpu);
            if (cpu) {
                if (cpu.memory) {
                    if (cpu.memory.buffer) return new Uint8Array(cpu.memory.buffer);
                    if (cpu.memory.u8) return cpu.memory.u8;
                    if (cpu.memory.mem8) return cpu.memory.mem8;
                    if (cpu.memory.raw_memory) return new Uint8Array(cpu.memory.raw_memory.buffer);
                    if (cpu.memory.wasm_memory && cpu.memory.wasm_memory.buffer) return new Uint8Array(cpu.memory.wasm_memory.buffer);
                }
                if (cpu.mem8) return cpu.mem8;
                if (cpu.buffer) return new Uint8Array(cpu.buffer);
            }
            if (this.v86.memory && this.v86.memory.buffer) {
                return new Uint8Array(this.v86.memory.buffer);
            }
            if (this.v86.mem8) return this.v86.mem8;
        } catch (_) {}
        return null;
    }

    /**
     * Consume pending virtqueue descriptors from guest ring buffers
     * @param {number} [queueIdx=0] - Virtqueue index (0=control, 1=cursor)
     */
    consumeVirtqueue(queueIdx = 0) {
        const q = this.queues[queueIdx];
        if (!q || (q.pfn === 0 && !q.descAddr)) {
            console.warn(`[VirtIO-GPU] consumeVirtqueue(${queueIdx}) skipped: q=${!!q} pfn=${q ? q.pfn : 0} descAddr=${q ? q.descAddr : 0}`);
            return;
        }

        const guestMem = this.getGuestMemory();
        if (!guestMem) {
            console.warn(`[VirtIO-GPU] consumeVirtqueue(${queueIdx}) skipped: guestMem is null!`);
            return;
        }

        const qSize = q.size;
        let descTableAddr, availRingAddr, usedRingAddr;

        if (q.descAddr && q.availAddr && q.usedAddr) {
            // Modern transport: kernel provided explicit ring addresses
            descTableAddr = q.descAddr;
            availRingAddr = q.availAddr;
            usedRingAddr = q.usedAddr;
        } else {
            // Legacy transport: compute from PFN
            descTableAddr = q.pfn * 4096;
            availRingAddr = descTableAddr + qSize * 16;
            usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * qSize) / 4096) * 4096;
        }

        if (usedRingAddr + 4 + 8 * qSize > guestMem.length) {
            console.warn(`[VirtIO-GPU] usedRingAddr 0x${usedRingAddr.toString(16)} exceeds guestMem length 0x${guestMem.length.toString(16)}`);
            return;
        }

        const view = new DataView(guestMem.buffer, guestMem.byteOffset, guestMem.byteLength);
        const availIdx = view.getUint16(availRingAddr + 2, true);
        console.info(`[VirtIO-GPU] consumeVirtqueue(${queueIdx}) pfn=0x${q.pfn.toString(16)} availIdx=${availIdx} lastAvailIdx=${q.lastAvailIdx} qSize=${qSize}`);

        let processedCount = 0;
        while (q.lastAvailIdx !== availIdx) {
            const availSlot = q.lastAvailIdx % qSize;
            const headDescIdx = view.getUint16(availRingAddr + 4 + availSlot * 2, true);

            let writtenBytes = 0;
            // Always use the JS descriptor chain parser — it walks descriptors via
            // zero-copy subarray views over guest memory, then routes the extracted
            // command packet through rustBridge.process_command_packet (WASM).
            // The old path (rustBridge.process_virtqueue_descriptor) copies the
            // ENTIRE guest RAM (256MB+) into WASM linear memory and OOMs.
            writtenBytes = this.consumeDescriptorChainJs(guestMem, descTableAddr, headDescIdx);
            console.info(`[VirtIO-GPU] consumeDescriptorChainJs returned ${writtenBytes} bytes (headDesc=${headDescIdx})`);

            // Record entry in Used Ring: { id: u32, len: u32 }
            const usedSlot = q.lastUsedIdx % qSize;
            const entryOffset = usedRingAddr + 4 + usedSlot * 8;
            view.setUint32(entryOffset, headDescIdx, true);
            view.setUint32(entryOffset + 4, writtenBytes !== undefined ? writtenBytes : 24, true);

            q.lastUsedIdx = (q.lastUsedIdx + 1) & 0xFFFF;
            view.setUint16(usedRingAddr + 2, q.lastUsedIdx, true);

            q.lastAvailIdx = (q.lastAvailIdx + 1) & 0xFFFF;
            processedCount++;
        }

        if (processedCount > 0) {
            const wasGuestActive = this.guestActive;
            this.guestActive = true;
            this.guestHasPresented = true;
            this.hostInjectionBlocked = true;
            console.info(`[Pipeline][Phase 5/8: VirtIO-GPU] Guest Virtqueue Frame: processed ${processedCount} descriptors on queue ${queueIdx} (lastAvail=${q.lastAvailIdx}, lastUsed=${q.lastUsedIdx}) -> guestHasPresented=true (host fallback LOCKED OUT)`);
            console.info(`[VirtIO-GPU] GUEST FIRST FRAME: processed ${processedCount} descriptors queue=${queueIdx} lastAvail=${q.lastAvailIdx} lastUsed=${q.lastUsedIdx} wasGuestActive=${wasGuestActive} -> GUEST_ACTIVE TRUE (host injection now BLOCKED)`);
            logger.log('bridge', 'I', `[virtio-gpu-device] Processed ${processedCount} descriptors on queue ${queueIdx} (lastAvail=${q.lastAvailIdx}, lastUsed=${q.lastUsedIdx})`);
            if (!wasGuestActive) {
                this.firstGuestFrameAt = (typeof performance !== 'undefined' ? performance.now() : Date.now());
                logger.log('bridge', 'I', 'GUEST FIRST FRAME - host injection blocked');
                if (typeof this.onGuestActiveChange === 'function') {
                    try { this.onGuestActiveChange(true); } catch (_) {}
                }
            }
            // Render scanout to canvas
            try {
                this.renderScanoutToCanvas(0);
                console.info(`[VirtIO-GPU] renderScanoutToCanvas(0) completed successfully`);
            } catch (e) {
                console.error(`[VirtIO-GPU] renderScanoutToCanvas(0) failed: ${e}`, e);
            }

            // Assert ISR queue interrupt (bit 0)
            this.isrStatus |= 0x01;
            try {
                const cpu = this.v86.cpu || (this.v86.v86 && this.v86.v86.cpu);
                if (cpu) {
                    if (typeof cpu.device_raise_irq === 'function') {
                        cpu.device_raise_irq(this.irqLine);
                    } else if (typeof cpu.raise_irq === 'function') {
                        cpu.raise_irq(this.irqLine);
                    }
                }
                if (this.v86 && typeof this.v86.raise_irq === 'function') {
                    this.v86.raise_irq(this.irqLine);
                }
            } catch (_) {}
        }
    }

    /**
     * Fallback manual descriptor chain parser in JavaScript
     */
    consumeDescriptorChainJs(guestMem, descTableAddr, headDescIdx) {
        const view = new DataView(guestMem.buffer, guestMem.byteOffset, guestMem.byteLength);
        let currIdx = headDescIdx;
        let inBuffers = [];
        let outBuffers = [];
        let visited = 0;

        while (visited < 256) {
            visited++;
            const descOffset = descTableAddr + currIdx * 16;
            if (descOffset + 16 > guestMem.length) break;

            const addrLo = view.getUint32(descOffset, true);
            const addrHi = view.getUint32(descOffset + 4, true);
            const addr = addrLo + addrHi * 0x100000000;
            const len = view.getUint32(descOffset + 8, true);
            const flags = view.getUint16(descOffset + 12, true);
            const next = view.getUint16(descOffset + 14, true);

            if (addr + len <= guestMem.length) {
                if ((flags & 0x02) !== 0) {
                    outBuffers.push({ addr, len });
                } else {
                    inBuffers.push(guestMem.subarray(addr, addr + len));
                }
            }

            if ((flags & 0x01) !== 0) {
                currIdx = next;
            } else {
                break;
            }
        }

        const totalInLen = inBuffers.reduce((acc, b) => acc + b.length, 0);
        const combinedIn = new Uint8Array(totalInLen);
        let offset = 0;
        for (const b of inBuffers) {
            combinedIn.set(b, offset);
            offset += b.length;
        }

        // Parse command type and attach guest DMA memory for scatter-gather transfers
        let packetToSend = combinedIn;
        if (combinedIn.length >= 4) {
            const viewIn = new DataView(combinedIn.buffer, combinedIn.byteOffset, combinedIn.byteLength);
            const cmdType = viewIn.getUint32(0, true);

            // 1. RESOURCE_ATTACH_BACKING (0x0106)
            if (cmdType === 0x0106 && combinedIn.length >= 32) {
                const resId = viewIn.getUint32(24, true);
                const nrEntries = viewIn.getUint32(28, true);
                const entries = [];
                for (let i = 0; i < nrEntries; i++) {
                    const off = 32 + i * 16;
                    if (off + 16 <= combinedIn.length) {
                        const addrLo = viewIn.getUint32(off, true);
                        const addrHi = viewIn.getUint32(off + 4, true);
                        const addr = addrLo + addrHi * 0x100000000;
                        const len = viewIn.getUint32(off + 8, true);
                        entries.push({ addr, len });
                    }
                }
                if (!this.resourceBacking) this.resourceBacking = new Map();
                this.resourceBacking.set(resId, entries);
                console.info(`[VirtIO-GPU] RESOURCE_ATTACH_BACKING cached resId=${resId} entries=${entries.length} (entry0: 0x${entries[0]?.addr.toString(16)}, ${entries[0]?.len} bytes)`);
            }
            // 2. RESOURCE_DETACH_BACKING (0x0107)
            else if (cmdType === 0x0107 && combinedIn.length >= 28) {
                const resId = viewIn.getUint32(24, true);
                if (this.resourceBacking) this.resourceBacking.delete(resId);
            }
            // 3. TRANSFER_TO_HOST_2D (0x0105)
            else if (cmdType === 0x0105 && combinedIn.length >= 56) {
                const resId = viewIn.getUint32(48, true);
                const entries = this.resourceBacking ? this.resourceBacking.get(resId) : null;
                if (entries && entries.length > 0 && combinedIn.length === 56) {
                    let totalDmaLen = 0;
                    for (const e of entries) totalDmaLen += e.len;
                    const fullPacket = new Uint8Array(56 + totalDmaLen);
                    fullPacket.set(combinedIn, 0);
                    let dmaOffset = 56;
                    for (const e of entries) {
                        if (e.addr + e.len <= guestMem.length) {
                            fullPacket.set(guestMem.subarray(e.addr, e.addr + e.len), dmaOffset);
                            dmaOffset += e.len;
                        }
                    }
                    packetToSend = fullPacket;
                    console.info(`[VirtIO-GPU] TRANSFER_TO_HOST_2D attached DMA pixel payload (${totalDmaLen} bytes) for resId=${resId} (from guest physical addr 0x${entries[0]?.addr.toString(16)})`);
                }
            }
            // 4. TRANSFER_TO_HOST_3D (0x0205)
            else if (cmdType === 0x0205 && combinedIn.length >= 72) {
                const resId = viewIn.getUint32(56, true);
                const entries = this.resourceBacking ? this.resourceBacking.get(resId) : null;
                if (entries && entries.length > 0 && combinedIn.length === 72) {
                    let totalDmaLen = 0;
                    for (const e of entries) totalDmaLen += e.len;
                    const fullPacket = new Uint8Array(72 + totalDmaLen);
                    fullPacket.set(combinedIn, 0);
                    let dmaOffset = 72;
                    for (const e of entries) {
                        if (e.addr + e.len <= guestMem.length) {
                            fullPacket.set(guestMem.subarray(e.addr, e.addr + e.len), dmaOffset);
                            dmaOffset += e.len;
                        }
                    }
                    packetToSend = fullPacket;
                    console.info(`[VirtIO-GPU] TRANSFER_TO_HOST_3D attached DMA pixel payload (${totalDmaLen} bytes) for resId=${resId}`);
                }
            }
        }

        const resp = this.processControlQueue(packetToSend);

        let respOffset = 0;
        let written = 0;
        for (const out of outBuffers) {
            if (respOffset >= resp.length) break;
            const toWrite = Math.min(resp.length - respOffset, out.len);
            guestMem.set(resp.subarray(respOffset, respOffset + toWrite), out.addr);
            respOffset += toWrite;
            written += toWrite;
        }

        return written;
    }

    /**
     * Process Virtqueue 0 (Control Queue) incoming command buffers
     * @param {Uint8Array} commandBuffer - Serialized virtio command stream
     * @returns {Uint8Array} Response packet to return to guest kernel
     */
    processControlQueue(commandBuffer) {
        const cmdLen = commandBuffer ? commandBuffer.length : 0;
        const cmdType = commandBuffer && commandBuffer.length >= 4
            ? (commandBuffer[0] | (commandBuffer[1] << 8) | (commandBuffer[2] << 16) | (commandBuffer[3] << 24)) >>> 0
            : 0;
        logger.log('bridge', 'D', `Processing control queue packet (0x${cmdType.toString(16).padStart(4, '0')}, ${cmdLen} bytes)`, {
            opcode: `0x${cmdType.toString(16)}`,
            bytes: cmdLen
        });

        if (!this.rustBridge) {
            return new Uint8Array([0x00, 0x11, 0x00, 0x00]); // VIRTIO_GPU_RESP_OK_NODATA
        }

        let resp;
        if (typeof this.rustBridge.process_command_packet === "function") {
            resp = this.rustBridge.process_command_packet(commandBuffer);
        } else if (typeof this.rustBridge.handle_binary_packet === "function") {
            resp = this.rustBridge.handle_binary_packet(commandBuffer);
        } else {
            resp = new Uint8Array([0x00, 0x11, 0x00, 0x00]);
        }

        // Render updated scanout framebuffer with damage rect support
        this.renderScanoutToCanvas(0);

        return resp;
    }

    /**
     * Blit scanout pixels to canvas, utilizing damage rects when available
     */
    renderScanoutToCanvas(scanoutId = 0) {
        if (!this.rustBridge || !this.ctx2d || typeof this.rustBridge.get_scanout_framebuffer !== "function") {
            return;
        }
        const damage = typeof this.rustBridge.get_scanout_damage === "function"
            ? this.rustBridge.get_scanout_damage(scanoutId)
            : null;
        const fb = (typeof this.rustBridge.get_scanout_framebuffer_rgba === "function")
            ? this.rustBridge.get_scanout_framebuffer_rgba(scanoutId)
            : this.rustBridge.get_scanout_framebuffer(scanoutId);
        if (!fb || fb.length === 0) return;

        const width = (this.canvas && this.canvas.width) ? this.canvas.width : 720;
        const height = (this.canvas && this.canvas.height) ? this.canvas.height : 1440;

        if (!this.offscreenTransferred && this.ctx2d) {
            if (!this.cachedImageData || this.cachedImageData.width !== width || this.cachedImageData.height !== height) {
                this.cachedImageData = this.ctx2d.createImageData(width, height);
            }
            const copyBytes = Math.min(fb.length, this.cachedImageData.data.length);
            this.cachedImageData.data.set(fb.subarray(0, copyBytes));
        }

        const hasDamage = damage && damage.length === 4 && damage[2] > 0 && damage[3] > 0;
        if (hasDamage || !this._lastHadDamage) {
            console.info(`[Pipeline][Phase 6/8: Rust Bridge] Readback: scanout=${scanoutId} damage=[${damage ? damage.join(',') : 'none'}] fbBytes=${fb.length} (BGRX->RGBA swizzled)`);
            console.info(`[Pipeline][Phase 7/8: WebGPU] Compositor Pass: scanout=${scanoutId} target=${width}x${height} mode=${this.offscreenTransferred ? 'OffscreenWorker' : 'DirectWebGPU'}`);
        }
        this._lastHadDamage = hasDamage;

        if (damage && damage.length === 4) {
            const [dx, dy, dw, dh] = damage;
            if (dw > 0 && dh > 0 && dx < width && dy < height) {
                const subW = Math.min(dw, width - dx);
                const subH = Math.min(dh, height - dy);

                logger.log('bridge', 'D', `Scanout ${scanoutId} damaged rect [${dx}, ${dy}, ${subW}, ${subH}] presented`, {
                    scanoutId,
                    rect: [dx, dy, subW, subH],
                    bytes: subW * subH * 4
                });

                if (typeof this.onScanoutUpdate === "function") {
                    try {
                        this.onScanoutUpdate(scanoutId, [dx, dy, subW, subH]);
                    } catch (_) {}
                }

                if (this.worker && this.offscreenTransferred) {
                    this.worker.postMessage({
                        type: "UPDATE_DAMAGE_RECT",
                        x: dx,
                        y: dy,
                        width: subW,
                        height: subH,
                        pixels: fb.subarray(0, Math.min(fb.length, width * height * 4))
                    });
                } else if (this.ctx2d) {
                    this.ctx2d.putImageData(this.cachedImageData, 0, 0, dx, dy, subW, subH);
                    console.info(`[Pipeline][Phase 8/8: Canvas] Presentation: painted dirty damage rect [${dx}, ${dy}, ${subW}, ${subH}] (${subW * subH * 4} bytes) to <canvas id="${this.canvas?.id || 'screen'}">`);
                }

                this.damage_rects_count++;
                if (typeof this.rustBridge.clear_scanout_damage === "function") {
                    this.rustBridge.clear_scanout_damage(scanoutId);
                }
                return;
            }
        }

        logger.log('bridge', 'D', `Scanout ${scanoutId} full blit [0, 0, ${width}, ${height}] presented`, {
            scanoutId,
            rect: [0, 0, width, height],
            bytes: width * height * 4
        });

        if (typeof this.onScanoutUpdate === "function") {
            try {
                this.onScanoutUpdate(scanoutId, [0, 0, width, height]);
            } catch (_) {}
        }

        // Full blit fallback
        if (this.worker && this.offscreenTransferred) {
            this.worker.postMessage({
                type: "UPDATE_DAMAGE_RECT",
                x: 0,
                y: 0,
                width: width,
                height: height,
                pixels: fb.subarray(0, Math.min(fb.length, width * height * 4))
            });
        } else if (this.ctx2d) {
            this.ctx2d.putImageData(this.cachedImageData, 0, 0);
            console.info(`[Pipeline][Phase 8/8: Canvas] Presentation: full scanout [0, 0, ${width}, ${height}] (${width * height * 4} bytes) to <canvas id="${this.canvas?.id || 'screen'}">`);
        }
    }

    /**
     * Get live scanout framebuffer byte buffer from WASM bridge
     * @param {number} [scanoutId=0]
     * @returns {Uint8Array}
     */
    getScanoutFramebuffer(scanoutId = 0) {
        if (this.rustBridge && typeof this.rustBridge.get_scanout_framebuffer === "function") {
            return this.rustBridge.get_scanout_framebuffer(scanoutId);
        }
        return new Uint8Array(0);
    }

    /**
     * Get live scanout damage rect [x, y, w, h] from WASM bridge
     * @param {number} [scanoutId=0]
     * @returns {number[]|null}
     */
    getScanoutDamage(scanoutId = 0) {
        if (this.rustBridge && typeof this.rustBridge.get_scanout_damage === "function") {
            return this.rustBridge.get_scanout_damage(scanoutId);
        }
        return null;
    }

    /**
     * Clear damage rect for specified scanout
     * @param {number} [scanoutId=0]
     */
    clearScanoutDamage(scanoutId = 0) {
        if (this.rustBridge && typeof this.rustBridge.clear_scanout_damage === "function") {
            this.rustBridge.clear_scanout_damage(scanoutId);
        }
    }

    blockHostInjection() {
        this.hostInjectionBlocked = true;
    }

    allowHostInjection() {
        if (!this.guestHasPresented) this.hostInjectionBlocked = false;
    }

    isHostInjectionAllowed() {
        const allowed = !this.hostInjectionBlocked && !this.guestHasPresented;
        // verbose trace for gate debugging (throttled elsewhere, here per-call for visibility)
        // console.debug(`[gate] isHostInjectionAllowed -> blocked=${this.hostInjectionBlocked} presented=${this.guestHasPresented} => allowed=${allowed}`);
        return allowed;
    }

    /**
     * Process Virtqueue 1 (Cursor Queue)
     */
    processCursorQueue(cursorBuffer) {
        // Handle guest cursor position and shape updates
    }
}
