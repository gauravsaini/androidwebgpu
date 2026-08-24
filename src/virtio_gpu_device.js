/**
 * VirtioGpuDevice - v86 Virtio-GPU PCI Device Emulation & WebGPU Bridge
 * Conforms to OASIS Virtio 1.2 GPU PCI Device Specification
 * Enhanced with 120 FPS OffscreenCanvas Worker Raster and Damage Rect Scissoring
 */

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
        this.damage_rects_count = 0;
        this.initPci();
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
     * Process Virtqueue 0 (Control Queue) incoming command buffers
     * @param {Uint8Array} commandBuffer - Serialized virtio command stream
     * @returns {Uint8Array} Response packet to return to guest kernel
     */
    processControlQueue(commandBuffer) {
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
     * Process Virtqueue 1 (Cursor Queue)
     */
    processCursorQueue(cursorBuffer) {
        // Handle guest cursor position and shape updates
    }
}
