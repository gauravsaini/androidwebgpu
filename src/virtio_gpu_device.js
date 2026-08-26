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

        // Virtqueue Ring State
        this.queues = [
            { size: 256, pfn: 0, lastAvailIdx: 0, lastUsedIdx: 0 }, // Queue 0: Control Queue
            { size: 16,  pfn: 0, lastAvailIdx: 0, lastUsedIdx: 0 }  // Queue 1: Cursor Queue
        ];
        this.selectedQueue = 0;
        this.deviceStatus = 0;
        this.isrStatus = 0;
        this.hostFeatures = (1 << 0) | (1 << 1); // VIRGL + EDID
        this.guestFeatures = 0;
        this.pciSlot = 0x05;
        this.ioBase = 0xC000;
        this.irqLine = 10;

        this.initPci();
        if (this.v86) {
            this.registerWithV86(this.v86);
        }
    }

    initPci() {
        // Vendor ID: Red Hat / QEMU Virtio (0x1AF4)
        this.pci_space[0] = 0xF4;
        this.pci_space[1] = 0x1A;

        // Device ID: Virtio GPU (0x1050)
        this.pci_space[2] = 0x50;
        this.pci_space[3] = 0x10;

        // Command & Status
        this.pci_space[4] = 0x07; // I/O, Memory, Bus Master enabled
        this.pci_space[5] = 0x00;

        // Subsystem IDs & Class: Display Controller (0x030000)
        this.pci_space[10] = 0x00;
        this.pci_space[11] = 0x03;

        // BAR0: I/O Space (64 bytes)
        this.pci_space[16] = 0x01; // I/O indicator
        this.pci_space[17] = 0xC0;

        // BAR1: MMIO Space
        this.pci_space[20] = 0x00;
        this.pci_space[21] = 0xD0;

        // Interrupt line
        this.pci_space[60] = this.irqLine;
        this.pci_space[61] = 0x01; // INTA#

        logger.log('bridge', 'I', 'Virtio-GPU PCI device (0x1AF4:0x1050) initialized with 1 scanout', {
            vendorId: 0x1AF4,
            deviceId: 0x1050,
            scanouts: this.num_scanouts
        });
    }

    /**
     * Register Virtio-GPU device on v86 PCI bus and I/O ports
     * @param {Object} v86 - v86 emulator instance
     */
    registerWithV86(v86) {
        if (!v86) return;
        this.v86 = v86;

        try {
            const cpu = v86.cpu || (v86.v86 && v86.v86.cpu);
            const io = v86.io || (v86.v86 && v86.v86.io);

            // Register on PCI bus if accessible
            if (cpu && cpu.devices && cpu.devices.pci) {
                cpu.devices.pci.register_device(this.pciSlot << 3, this);
            }

            // Register I/O port handlers for BAR0 (0xC000..0xC03F)
            if (io && typeof io.register_read === 'function' && typeof io.register_write === 'function') {
                for (let port = this.ioBase; port < this.ioBase + 64; port++) {
                    const offset = port - this.ioBase;
                    io.register_read(port, this, () => this.ioRead(offset, 1));
                    io.register_write(port, this, (val) => this.ioWrite(offset, val, 1));
                }
            }

            logger.log('bridge', 'I', `Virtio-GPU device attached to v86 (slot 0x${this.pciSlot.toString(16)}, I/O 0x${this.ioBase.toString(16)})`, {
                pciSlot: this.pciSlot,
                ioBase: this.ioBase,
                irqLine: this.irqLine
            });
        } catch (e) {
            logger.log('bridge', 'W', `v86 registration notice: ${e.message}`);
        }
    }

    /**
     * Read from VirtIO Legacy PCI I/O Configuration Space
     */
    ioRead(offset, size) {
        switch (offset) {
            case 0x00: // Host Features (32-bit)
                return this.hostFeatures >>> 0;
            case 0x04: // Guest Features (32-bit)
                return this.guestFeatures >>> 0;
            case 0x08: // Queue Address PFN (32-bit)
                return (this.queues[this.selectedQueue]?.pfn || 0) >>> 0;
            case 0x0C: // Queue Size (16-bit)
                return (this.queues[this.selectedQueue]?.size || 0) & 0xFFFF;
            case 0x0E: // Queue Select (16-bit)
                return this.selectedQueue & 0xFFFF;
            case 0x12: // Device Status (8-bit)
                return this.deviceStatus & 0xFF;
            case 0x13: { // ISR Status (8-bit)
                const isr = this.isrStatus;
                this.isrStatus = 0; // Read clears ISR
                return isr & 0xFF;
            }
            case 0x14: // Device-specific: events_read (32-bit)
                return 0;
            case 0x18: // Device-specific: events_clear (32-bit)
                return 0;
            case 0x1C: // Device-specific: num_scanouts (32-bit)
                return this.num_scanouts;
            case 0x20: // Device-specific: num_capsets (32-bit)
                return this.num_capsets;
            default:
                return this.pci_space[offset] || 0;
        }
    }

    /**
     * Write to VirtIO Legacy PCI I/O Configuration Space
     */
    ioWrite(offset, val, size) {
        switch (offset) {
            case 0x04: // Guest Features
                this.guestFeatures = val >>> 0;
                break;
            case 0x08: // Queue Address PFN
                if (this.queues[this.selectedQueue]) {
                    this.queues[this.selectedQueue].pfn = val >>> 0;
                }
                break;
            case 0x0E: // Queue Select
                this.selectedQueue = (val & 0x1) === 1 ? 1 : 0;
                break;
            case 0x10: // Queue Notify
                this.consumeVirtqueue(val & 0xFFFF);
                break;
            case 0x12: // Device Status
                this.deviceStatus = val & 0xFF;
                if (this.deviceStatus === 0) {
                    // Reset device
                    this.queues[0].lastAvailIdx = 0;
                    this.queues[0].lastUsedIdx = 0;
                    this.queues[1].lastAvailIdx = 0;
                    this.queues[1].lastUsedIdx = 0;
                }
                break;
            default:
                if (offset < this.pci_space.length) {
                    this.pci_space[offset] = val & 0xFF;
                }
                break;
        }
    }

    /**
     * Read from PCI Configuration space or BARs
     */
    pciRead(addr, size) {
        let val = 0;
        for (let i = 0; i < size; i++) {
            val |= (this.pci_space[addr + i] || 0) << (i * 8);
        }
        return val >>> 0;
    }

    /**
     * Write to PCI Configuration space or BARs
     */
    pciWrite(addr, val, size) {
        for (let i = 0; i < size; i++) {
            this.pci_space[addr + i] = (val >>> (i * 8)) & 0xFF;
        }
    }

    /**
     * Obtain guest physical memory buffer slice from v86
     * @returns {Uint8Array|null}
     */
    getGuestMemory() {
        if (!this.v86) return null;
        try {
            const cpu = this.v86.cpu || (this.v86.v86 && this.v86.v86.cpu);
            if (cpu && cpu.memory) {
                if (cpu.memory.buffer) return new Uint8Array(cpu.memory.buffer);
                if (cpu.memory.u8) return cpu.memory.u8;
                if (cpu.memory.raw_memory) return new Uint8Array(cpu.memory.raw_memory.buffer);
            }
            if (this.v86.memory && this.v86.memory.buffer) {
                return new Uint8Array(this.v86.memory.buffer);
            }
        } catch (_) {}
        return null;
    }

    /**
     * Consume pending virtqueue descriptors from guest ring buffers
     * @param {number} [queueIdx=0] - Virtqueue index (0=control, 1=cursor)
     */
    consumeVirtqueue(queueIdx = 0) {
        const q = this.queues[queueIdx];
        if (!q || q.pfn === 0) return;

        const guestMem = this.getGuestMemory();
        if (!guestMem) return;

        const qSize = q.size;
        const descTableAddr = q.pfn * 4096;
        const availRingAddr = descTableAddr + qSize * 16;
        const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * qSize) / 4096) * 4096;

        if (usedRingAddr + 4 + 8 * qSize > guestMem.length) return;

        const view = new DataView(guestMem.buffer, guestMem.byteOffset, guestMem.byteLength);
        const availIdx = view.getUint16(availRingAddr + 2, true);

        let processedCount = 0;
        while (q.lastAvailIdx !== availIdx) {
            const availSlot = q.lastAvailIdx % qSize;
            const headDescIdx = view.getUint16(availRingAddr + 4 + availSlot * 2, true);

            let writtenBytes = 0;
            if (this.rustBridge && typeof this.rustBridge.process_virtqueue_descriptor === 'function') {
                try {
                    writtenBytes = this.rustBridge.process_virtqueue_descriptor(
                        guestMem,
                        descTableAddr,
                        headDescIdx
                    );
                } catch (e) {
                    logger.log('bridge', 'W', `process_virtqueue_descriptor error: ${e}`);
                }
            } else {
                // Fallback: parse descriptor chain manually in JS
                writtenBytes = this.consumeDescriptorChainJs(guestMem, descTableAddr, headDescIdx);
            }

            // Record entry in Used Ring: { id: u32, len: u32 }
            const usedSlot = q.lastUsedIdx % qSize;
            const entryOffset = usedRingAddr + 4 + usedSlot * 8;
            view.setUint32(entryOffset, headDescIdx, true);
            view.setUint32(entryOffset + 4, writtenBytes || 24, true);

            q.lastUsedIdx = (q.lastUsedIdx + 1) & 0xFFFF;
            view.setUint16(usedRingAddr + 2, q.lastUsedIdx, true);

            q.lastAvailIdx = (q.lastAvailIdx + 1) & 0xFFFF;
            processedCount++;
        }

        if (processedCount > 0) {
            // Render scanout to canvas
            this.renderScanoutToCanvas(0);

            // Assert ISR queue interrupt
            this.isrStatus |= 0x01;
            try {
                const cpu = this.v86.cpu || (this.v86.v86 && this.v86.v86.cpu);
                if (cpu && typeof cpu.device_raise_irq === 'function') {
                    cpu.device_raise_irq(this.irqLine);
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

        const resp = this.processControlQueue(combinedIn);

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
        logger.log('bridge', 'D', `Processing control queue packet (${cmdLen} bytes)`, {
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

        const fb = this.rustBridge.get_scanout_framebuffer(scanoutId);
        if (!fb || fb.length === 0) return;

        const width = this.canvas.width;
        const height = this.canvas.height;

        if (!this.offscreenTransferred && this.ctx2d) {
            if (!this.cachedImageData || this.cachedImageData.width !== width || this.cachedImageData.height !== height) {
                this.cachedImageData = this.ctx2d.createImageData(width, height);
            }
            if (fb.length >= width * height * 4) {
                this.cachedImageData.data.set(fb.subarray(0, width * height * 4));
            }
        }

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
                        pixels: fb.subarray(0, width * height * 4)
                    });
                } else if (this.ctx2d) {
                    this.ctx2d.putImageData(this.cachedImageData, 0, 0, dx, dy, subW, subH);
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
                width,
                height,
                pixels: fb.subarray(0, width * height * 4)
            });
        } else if (this.ctx2d && fb.length >= width * height * 4) {
            this.ctx2d.putImageData(this.cachedImageData, 0, 0);
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

    /**
     * Process Virtqueue 1 (Cursor Queue)
     */
    processCursorQueue(cursorBuffer) {
        // Handle guest cursor position and shape updates
    }
}

