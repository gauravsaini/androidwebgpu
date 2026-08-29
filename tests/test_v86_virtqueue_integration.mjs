import { describe, it } from 'node:test';
import assert from 'node:assert';
import { VirtioGpuDevice, VIRTIO_GPU_F_VIRGL, VIRTIO_GPU_F_EDID } from '../src/virtio_gpu_device.js';

describe('VirtioGpuDevice & Virtqueue Phase 2 Integration Tests', () => {
    it('initializes PCI configuration space according to OASIS VirtIO 1.2 spec', () => {
        const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });
        
        // Vendor ID: 0x1AF4
        assert.strictEqual(dev.pciRead(0, 2), 0x1AF4);
        // Device ID: 0x1050 (Virtio GPU)
        assert.strictEqual(dev.pciRead(2, 2), 0x1050);
        // Class: Display Controller (0x030000)
        assert.strictEqual(dev.pciRead(10, 2), 0x0300);
        // BAR0: I/O Space (indicator bit 0 set)
        assert.strictEqual(dev.pciRead(16, 1) & 0x01, 1);
        // BAR1: MMIO Space
        assert.strictEqual(dev.pciRead(20, 1) & 0x01, 0);
        // IRQ Line
        assert.strictEqual(dev.pciRead(60, 1), 10);
    });

    it('handles legacy PCI I/O configuration reads and writes', () => {
        const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });

        // Host Features
        assert.strictEqual(dev.ioRead(0x00, 4), (1 << 0) | (1 << 1));
        
        // Select Queue 0 (Control Queue)
        dev.ioWrite(0x0E, 0, 2);
        assert.strictEqual(dev.ioRead(0x0C, 2), 256); // Queue 0 size: 256

        // Set Queue 0 Address PFN
        dev.ioWrite(0x08, 0x100, 4);
        assert.strictEqual(dev.ioRead(0x08, 4), 0x100);

        // Select Queue 1 (Cursor Queue)
        dev.ioWrite(0x0E, 1, 2);
        assert.strictEqual(dev.ioRead(0x0C, 2), 16); // Queue 1 size: 16

        // Set Device Status (DRIVER_OK = 4)
        dev.ioWrite(0x12, 4, 1);
        assert.strictEqual(dev.ioRead(0x12, 1), 4);
        
        // Device-specific scanouts and capsets
        assert.strictEqual(dev.ioRead(0x1C, 4), 1); // num_scanouts
        assert.strictEqual(dev.ioRead(0x20, 4), 1); // num_capsets
    });

    it('attaches to v86 emulator instance and registers PCI device & I/O ports', () => {
        let registeredPciDevice = null;
        const registeredPorts = new Map();

        const fakeV86 = {
            cpu: {
                devices: {
                    pci: {
                        register_device: (slotMask, dev) => {
                            registeredPciDevice = { slotMask, dev };
                        }
                    }
                }
            },
            io: {
                register_read: (port, dev, cb) => {
                    registeredPorts.set(`r:${port}`, cb);
                },
                register_write: (port, dev, cb) => {
                    registeredPorts.set(`w:${port}`, cb);
                }
            }
        };

        const dev = new VirtioGpuDevice(fakeV86, null, { width: 1280, height: 720 });
        assert.ok(registeredPciDevice !== null);
        assert.strictEqual(registeredPciDevice.slotMask, 0x06 << 3); // slot 0x06 avoids NE2000 collision at 0x05 (verbose logs: pciSlot=0x06 io=0xC140)
        assert.strictEqual(registeredPorts.size, 128); // 64 read + 64 write handlers
    });

    it('consumes Virtqueue 0 descriptor ring buffer from guest memory', () => {
        const guestMemory = new Uint8Array(2 * 1024 * 1024);
        let raisedIrq = null;

        const fakeV86 = {
            cpu: {
                memory: { buffer: guestMemory.buffer },
                device_raise_irq: (irq) => {
                    raisedIrq = irq;
                }
            }
        };

        let lastCommandReceived = null;
        const fakeRustBridge = {
            process_command_packet: (cmd) => {
                lastCommandReceived = cmd;
                // Return VIRTIO_GPU_RESP_OK_NODATA (4 bytes header)
                return new Uint8Array([0x00, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
            },
            get_scanout_framebuffer: () => new Uint8Array(1280 * 720 * 4)
        };

        const dev = new VirtioGpuDevice(fakeV86, fakeRustBridge, { width: 1280, height: 720 });

        // Configure Queue 0 at PFN 0x10 (Physical Address 0x10000 = 65536)
        dev.ioWrite(0x0E, 0, 2);
        dev.ioWrite(0x08, 0x10, 4);

        const descTableAddr = 0x10 * 4096; // 65536
        const availRingAddr = descTableAddr + 256 * 16; // 69632
        const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * 256) / 4096) * 4096; // 73728

        const view = new DataView(guestMemory.buffer);

        // Put a fake command packet at address 0x20000 (131072)
        const cmdBytes = new Uint8Array([0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        guestMemory.set(cmdBytes, 0x20000);

        // Descriptor 0: In (Command) -> points to 0x20000, len 16, flags NEXT, next 1
        view.setUint32(descTableAddr + 0, 0x20000, true); // addr_lo
        view.setUint32(descTableAddr + 4, 0, true);       // addr_hi
        view.setUint32(descTableAddr + 8, 16, true);      // len
        view.setUint16(descTableAddr + 12, 0x01, true);   // flags: NEXT
        view.setUint16(descTableAddr + 14, 1, true);      // next: 1

        // Descriptor 1: Out (Response) -> points to 0x30000, len 24, flags WRITE, next 0
        view.setUint32(descTableAddr + 16 + 0, 0x30000, true);
        view.setUint32(descTableAddr + 16 + 4, 0, true);
        view.setUint32(descTableAddr + 16 + 8, 24, true);
        view.setUint16(descTableAddr + 16 + 12, 0x02, true); // flags: WRITE
        view.setUint16(descTableAddr + 16 + 14, 0, true);

        // Set Available Ring: flags=0, idx=1, ring[0]=0
        view.setUint16(availRingAddr + 0, 0, true);
        view.setUint16(availRingAddr + 2, 1, true); // avail.idx = 1
        view.setUint16(availRingAddr + 4, 0, true); // ring[0] = desc 0

        // Notify Queue 0 via I/O Port write
        dev.ioWrite(0x10, 0, 2);

        // Assertions
        assert.ok(lastCommandReceived !== null);
        assert.strictEqual(lastCommandReceived.length, 16);
        assert.strictEqual(lastCommandReceived[0], 0x01);

        // Verify response written to guest memory at 0x30000
        assert.strictEqual(guestMemory[0x30000], 0x00);
        assert.strictEqual(guestMemory[0x30001], 0x11);

        // Verify used ring updated
        const usedIdx = view.getUint16(usedRingAddr + 2, true);
        assert.strictEqual(usedIdx, 1);
        assert.strictEqual(view.getUint32(usedRingAddr + 4, true), 0); // head desc index 0

        // Verify IRQ raised
        assert.strictEqual(raisedIrq, 10);
        assert.strictEqual(dev.ioRead(0x13, 1), 0x01); // ISR bit 0 set, read clears it
        assert.strictEqual(dev.ioRead(0x13, 1), 0x00); // Cleared
    });
});
