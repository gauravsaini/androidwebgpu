import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';
import { VirtioPacketBuilder } from '../src/virtio_packet_builder.js';
import { ViewHierarchyRasterizer } from '../src/view_rasterizer.js';
import { AndroidRuntime } from '../src/android_runtime.js';

describe('Challenger M1: VirtIO Scanout Lockout & Host Fallback Suppression Empirical Stress Suite', () => {

    function createTestEnvironment(width = 720, height = 1440) {
        const guestMem = new Uint8Array(16 * 1024 * 1024);
        let raisedIrqs = [];
        const fakeCpu = {
            memory: { buffer: guestMem.buffer },
            device_raise_irq: (irq) => { raisedIrqs.push(irq); },
            raise_irq: (irq) => { raisedIrqs.push(irq); },
            devices: { pci: { devices: {}, register_device: () => {} } }
        };
        const fakeIo = {
            register_read: () => {},
            register_write: () => {}
        };
        const fakeV86 = { cpu: fakeCpu, io: fakeIo };

        const mockBridge = {
            process_virtqueue_descriptor: (mem, descTable, head) => {
                const view = new DataView(mem.buffer, mem.byteOffset, mem.byteLength);
                const inAddrLo = view.getUint32(descTable + head * 16, true);
                const inLen = view.getUint32(descTable + head * 16 + 8, true);
                const cmd = mem.slice(inAddrLo, inAddrLo + inLen);

                // Get out buffer
                const outAddrLo = view.getUint32(descTable + 1 * 16, true);
                const resp = new Uint8Array(24);
                const respView = new DataView(resp.buffer);
                respView.setUint32(0, 0x1100, true); // VIRTIO_GPU_RESP_OK_NODATA
                mem.set(resp, outAddrLo);
                return resp.length;
            },
            get_scanout_framebuffer: () => new Uint8Array(width * height * 4),
            get_scanout_damage: () => [0, 0, width, height],
            clear_scanout_damage: () => {}
        };

        const canvas = { width, height, getContext: () => null };
        const dev = new VirtioGpuDevice(fakeV86, mockBridge, canvas);

        return { dev, guestMem, fakeV86, fakeCpu, raisedIrqs, mockBridge };
    }

    function setupVirtqueues(dev, guestMem, pfn0 = 0x10, pfn1 = 0x11) {
        // Status handshake: ACK (1) -> DRIVER (2) -> DRIVER_OK (4) | FEATURES_OK (8)
        dev.ioWrite(0x12, 0x01, 1);
        dev.ioWrite(0x12, 0x03, 1);
        dev.ioWrite(0x12, 0x0B, 1);
        dev.ioWrite(0x12, 0x0F, 1);

        // Queue 0 (Control)
        dev.ioWrite(0x0E, 0, 2); // Select Queue 0
        dev.ioWrite(0x08, pfn0, 4); // Set PFN 0x10

        // Queue 1 (Cursor)
        dev.ioWrite(0x0E, 1, 2); // Select Queue 1
        dev.ioWrite(0x08, pfn1, 4); // Set PFN 0x11

        const descTableAddr = pfn0 * 4096;
        const availRingAddr = descTableAddr + 256 * 16;
        const usedRingAddr = Math.ceil((availRingAddr + 4 + 2 * 256) / 4096) * 4096;

        return { descTableAddr, availRingAddr, usedRingAddr };
    }

    it('1. Initial State: Host injection allowed, guestActive false', () => {
        const { dev } = createTestEnvironment();
        assert.equal(dev.guestActive, false, 'guestActive must initially be false');
        assert.equal(dev.guestHasPresented, false, 'guestHasPresented must initially be false');
        assert.equal(dev.hostInjectionBlocked, false, 'hostInjectionBlocked must initially be false');
        assert.equal(dev.isHostInjectionAllowed(), true, 'isHostInjectionAllowed() must initially return true');
    });

    it('2. VirtIO PCI I/O Handshake & PFN Configuration at Port 0xC100', () => {
        const { dev, guestMem } = createTestEnvironment();
        
        // Host features read at offset 0x00
        const hostFeatures = dev.ioRead(0x00, 4);
        assert.equal(hostFeatures, 0x03, 'HOST_FEATURES must support VIRGL(0x1) + EDID(0x2)');

        // Guest features write at offset 0x04
        dev.ioWrite(0x04, 0x03, 4);
        assert.equal(dev.guestFeatures, 0x03, 'GUEST_FEATURES must match negotiated value');

        // Queue sizing
        dev.ioWrite(0x0E, 0, 2);
        assert.equal(dev.ioRead(0x0C, 2), 256, 'Queue 0 size must be 256');
        dev.ioWrite(0x0E, 1, 2);
        assert.equal(dev.ioRead(0x0C, 2), 16, 'Queue 1 size must be 16');

        // Set PFNs
        const rings = setupVirtqueues(dev, guestMem);
        assert.equal(dev.queues[0].pfn, 0x10);
        assert.equal(dev.queues[1].pfn, 0x11);
        assert.equal(dev.deviceStatus, 0x0F, 'Device status must be DRIVER_OK');
    });

    it('3. SET_SCANOUT via Queue Notify (0xC110) locks guestActive to true and disables host injection', () => {
        const { dev, guestMem, raisedIrqs } = createTestEnvironment(720, 1440);
        const { descTableAddr, availRingAddr, usedRingAddr } = setupVirtqueues(dev, guestMem);

        const view = new DataView(guestMem.buffer);

        // Build SET_SCANOUT command packet (opcode 0x0103)
        const cmdPacket = VirtioPacketBuilder.setScanout(0, 1, 720, 1440);
        const cmdAddr = 0x20000;
        guestMem.set(cmdPacket, cmdAddr);

        // Response buffer
        const respAddr = 0x21000;

        // Descriptor 0: Input command buffer (flags: NEXT = 1)
        view.setUint32(descTableAddr + 0 * 16 + 0, cmdAddr, true);
        view.setUint32(descTableAddr + 0 * 16 + 4, 0, true);
        view.setUint32(descTableAddr + 0 * 16 + 8, cmdPacket.length, true);
        view.setUint16(descTableAddr + 0 * 16 + 12, 0x01, true); // VRING_DESC_F_NEXT
        view.setUint16(descTableAddr + 0 * 16 + 14, 1, true);    // Next = 1

        // Descriptor 1: Output response buffer (flags: WRITE = 2)
        view.setUint32(descTableAddr + 1 * 16 + 0, respAddr, true);
        view.setUint32(descTableAddr + 1 * 16 + 4, 0, true);
        view.setUint32(descTableAddr + 1 * 16 + 8, 24, true);
        view.setUint16(descTableAddr + 1 * 16 + 12, 0x02, true); // VRING_DESC_F_WRITE
        view.setUint16(descTableAddr + 1 * 16 + 14, 0, true);

        // Put head descriptor 0 into Avail ring slot 0
        view.setUint16(availRingAddr + 4 + 0 * 2, 0, true);
        view.setUint16(availRingAddr + 2, 1, true); // Avail index = 1

        // Trigger Queue 0 kick via I/O Port 0xC110 (offset 0x10)
        dev.ioWrite(0x10, 0, 2);

        // Verify Lockout
        assert.equal(dev.guestActive, true, 'VirtioGpuDevice.guestActive MUST be true');
        assert.equal(dev.guestHasPresented, true, 'VirtioGpuDevice.guestHasPresented MUST be true');
        assert.equal(dev.hostInjectionBlocked, true, 'VirtioGpuDevice.hostInjectionBlocked MUST be true');
        assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() MUST return false');

        // Verify Used ring progress
        const usedIdx = view.getUint16(usedRingAddr + 2, true);
        assert.equal(usedIdx, 1, 'Used ring index must advance to 1');
        assert.equal(view.getUint32(usedRingAddr + 4 + 0 * 8, true), 0, 'Used element id must match head descriptor (0)');

        // Verify IRQ & ISR
        assert.ok(raisedIrqs.includes(10), 'IRQ 10 must be raised upon processing');
        assert.equal(dev.isrStatus & 0x01, 1, 'ISR status bit 0 must be set');
        assert.equal(dev.ioRead(0x13, 1), 1, 'Reading ISR_STATUS (0xC113) must return 1');
        assert.equal(dev.ioRead(0x13, 1), 0, 'Second read to ISR_STATUS must return 0 (read-to-clear)');
    });

    it('4. RESOURCE_FLUSH via Queue Notify reinforces scanout lockout and blocks fallback', () => {
        const { dev, guestMem } = createTestEnvironment(720, 1440);
        const { descTableAddr, availRingAddr, usedRingAddr } = setupVirtqueues(dev, guestMem);
        const view = new DataView(guestMem.buffer);

        // Build RESOURCE_FLUSH command packet (opcode 0x0104)
        const flushPacket = VirtioPacketBuilder.resourceFlush(1, 720, 1440, 0, 0);
        const cmdAddr = 0x22000;
        const respAddr = 0x23000;
        guestMem.set(flushPacket, cmdAddr);

        // Descriptor 0
        view.setUint32(descTableAddr + 0 * 16 + 0, cmdAddr, true);
        view.setUint32(descTableAddr + 0 * 16 + 4, 0, true);
        view.setUint32(descTableAddr + 0 * 16 + 8, flushPacket.length, true);
        view.setUint16(descTableAddr + 0 * 16 + 12, 0x01, true);
        view.setUint16(descTableAddr + 0 * 16 + 14, 1, true);

        // Descriptor 1
        view.setUint32(descTableAddr + 1 * 16 + 0, respAddr, true);
        view.setUint32(descTableAddr + 1 * 16 + 4, 0, true);
        view.setUint32(descTableAddr + 1 * 16 + 8, 24, true);
        view.setUint16(descTableAddr + 1 * 16 + 12, 0x02, true);
        view.setUint16(descTableAddr + 1 * 16 + 14, 0, true);

        view.setUint16(availRingAddr + 4 + 0 * 2, 0, true);
        view.setUint16(availRingAddr + 2, 1, true);

        dev.ioWrite(0x10, 0, 2);

        assert.equal(dev.guestActive, true, 'guestActive must be true after RESOURCE_FLUSH');
        assert.equal(dev.isHostInjectionAllowed(), false, 'Host injection must remain blocked');

        // Verify permanent lockout: allowHostInjection() cannot unblock when guestActive/guestHasPresented
        dev.allowHostInjection();
        assert.equal(dev.hostInjectionBlocked, true, 'allowHostInjection() must NOT unblock when guestActive is true');
        assert.equal(dev.isHostInjectionAllowed(), false, 'isHostInjectionAllowed() must remain false');
    });

    it('5. ViewHierarchyRasterizer and AndroidRuntime completely drop host frames when guestActive is true', () => {
        const { dev, guestMem } = createTestEnvironment(720, 1440);
        setupVirtqueues(dev, guestMem);

        // Manually trigger guest activation
        dev.consumeVirtqueue(0); // empty kick sets nothing yet
        assert.equal(dev.guestActive, false);

        // Now activate via actual packet
        const { descTableAddr, availRingAddr } = setupVirtqueues(dev, guestMem);
        const view = new DataView(guestMem.buffer);
        const pkt = VirtioPacketBuilder.setScanout(0, 1, 720, 1440);
        guestMem.set(pkt, 0x20000);
        view.setUint32(descTableAddr + 0, 0x20000, true);
        view.setUint32(descTableAddr + 8, pkt.length, true);
        view.setUint16(descTableAddr + 12, 0x01, true);
        view.setUint16(descTableAddr + 14, 1, true);
        view.setUint32(descTableAddr + 16, 0x21000, true);
        view.setUint32(descTableAddr + 24, 24, true);
        view.setUint16(descTableAddr + 28, 0x02, true);
        view.setUint16(availRingAddr + 4, 0, true);
        view.setUint16(availRingAddr + 2, 1, true);

        dev.ioWrite(0x10, 0, 2);
        assert.equal(dev.guestActive, true);

        // Test ViewHierarchyRasterizer
        const rasterizer = new ViewHierarchyRasterizer(720, 1440);
        let controlQueuePackets = 0;
        const origProcessControlQueue = dev.processControlQueue.bind(dev);
        dev.processControlQueue = (cmdBuf) => {
            controlQueuePackets++;
            return origProcessControlQueue(cmdBuf);
        };

        rasterizer.submitToVirtioGpu(dev, 100, 0, new Uint8Array(720 * 1440 * 4));
        assert.equal(controlQueuePackets, 0, 'ViewHierarchyRasterizer MUST NOT submit host packets when guestActive is true');

        // Test AndroidRuntime gating
        const runtime = new AndroidRuntime(dev.canvas, () => {});
        runtime.setGpuDevice(dev);
        assert.equal(runtime.isHostInjectionAllowed(), false, 'runtime.isHostInjectionAllowed() MUST return false');

        runtime.renderActivityUi({ packageName: 'test.app', zip: null });
        assert.equal(controlQueuePackets, 0, 'runtime.renderActivityUi MUST NOT submit host packets when guestActive is true');
    });

    it('6. High-Frequency VirtIO Kick Storm (50 frames) maintains scanout lockout & descriptor synchronization', () => {
        const { dev, guestMem } = createTestEnvironment(720, 1440);
        const { descTableAddr, availRingAddr, usedRingAddr } = setupVirtqueues(dev, guestMem);
        const view = new DataView(guestMem.buffer);

        const flushPacket = VirtioPacketBuilder.resourceFlush(1, 720, 1440, 0, 0);
        const cmdAddrBase = 0x30000;
        guestMem.set(flushPacket, cmdAddrBase);

        for (let frame = 0; frame < 50; frame++) {
            const descIdx0 = (frame * 2) % 256;
            const descIdx1 = (frame * 2 + 1) % 256;

            // In Descriptor
            view.setUint32(descTableAddr + descIdx0 * 16 + 0, cmdAddrBase, true);
            view.setUint32(descTableAddr + descIdx0 * 16 + 4, 0, true);
            view.setUint32(descTableAddr + descIdx0 * 16 + 8, flushPacket.length, true);
            view.setUint16(descTableAddr + descIdx0 * 16 + 12, 0x01, true);
            view.setUint16(descTableAddr + descIdx0 * 16 + 14, descIdx1, true);

            // Out Descriptor
            view.setUint32(descTableAddr + descIdx1 * 16 + 0, cmdAddrBase + 1024, true);
            view.setUint32(descTableAddr + descIdx1 * 16 + 4, 0, true);
            view.setUint32(descTableAddr + descIdx1 * 16 + 8, 24, true);
            view.setUint16(descTableAddr + descIdx1 * 16 + 12, 0x02, true);
            view.setUint16(descTableAddr + descIdx1 * 16 + 14, 0, true);

            // Avail ring
            const availSlot = frame % 256;
            view.setUint16(availRingAddr + 4 + availSlot * 2, descIdx0, true);
            view.setUint16(availRingAddr + 2, (frame + 1) & 0xFFFF, true);

            // Kick
            dev.ioWrite(0x10, 0, 2);

            assert.equal(dev.guestActive, true, `guestActive must remain true at frame ${frame}`);
            assert.equal(dev.isHostInjectionAllowed(), false);
        }

        const finalUsedIdx = view.getUint16(usedRingAddr + 2, true);
        assert.equal(finalUsedIdx, 50, 'All 50 frames must be processed into used ring');
    });
});
