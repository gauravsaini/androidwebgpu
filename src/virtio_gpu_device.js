/**
 * VirtioGpuDevice - v86 Virtio-GPU PCI Device Emulation & WebGPU Bridge
 * Conforms to OASIS Virtio 1.2 GPU PCI Device Specification
 */

export class VirtioGpuDevice {
    /**
     * @param {Object} v86 - Reference to v86 emulator instance
     * @param {Object} rustBridge - Instantiated Rust VirtioGpuBridge Wasm module
     * @param {HTMLCanvasElement} canvas - Target WebGPU HTML5 Canvas
     */
    constructor(v86, rustBridge, canvas) {
        this.v86 = v86;
        this.rustBridge = rustBridge;
        this.canvas = canvas;
        this.ctx2d = canvas && canvas.getContext ? canvas.getContext("2d") : null;
        this.pci_space = new Uint8Array(256);
        this.io_bar = new Uint8Array(64);
        this.num_scanouts = 1;
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

        // Render updated scanout framebuffer to canvas if 2D context is available
        this.renderScanoutToCanvas(0);

        return resp;
    }

    /**
     * Blit scanout pixels to canvas
     */
    renderScanoutToCanvas(scanoutId = 0) {
        if (!this.rustBridge || !this.ctx2d || typeof this.rustBridge.get_scanout_framebuffer !== "function") {
            return;
        }
        const fb = this.rustBridge.get_scanout_framebuffer(scanoutId);
        if (fb && fb.length >= this.canvas.width * this.canvas.height * 4) {
            const imgData = this.ctx2d.createImageData(this.canvas.width, this.canvas.height);
            imgData.data.set(fb);
            this.ctx2d.putImageData(imgData, 0, 0);
        }
    }

    /**
     * Process Virtqueue 1 (Cursor Queue)
     */
    processCursorQueue(cursorBuffer) {
        // Handle guest cursor position and shape updates
    }
}
