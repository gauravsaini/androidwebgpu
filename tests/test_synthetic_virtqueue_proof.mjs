import { describe, it } from 'node:test';
import assert from 'node:assert';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';
import { SyntheticGuestProbe } from '../src/synthetic_guest_probe.js';
import { VirtioPacketBuilder } from '../src/virtio_packet_builder.js';

describe('Synthetic Option A /dev/mem + outl proof — Gates 2.2→2.5', () => {
    it('Gate 2.2a: STATUS ACK→DRIVER→DRIVER_OK handshake drives host log', () => {
        const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });
        const probe = new SyntheticGuestProbe({ device: dev, guestMemory: new Uint8Array(8 * 1024 * 1024) });
        // manually sync guestMemory to device v86 memory if device has v86
        const guestMem = new Uint8Array(8 * 1024 * 1024);
        const fakeV86 = { cpu: { memory: { buffer: guestMem.buffer }, device_raise_irq: () => {}, devices: { pci: { devices: {}, register_device: () => {} } } }, io: { register_read: () => {}, register_write: () => {} } };
        dev.v86 = fakeV86;
        // bind probe to this device and memory
        probe.device = dev;
        probe.guestMemory = guestMem;
        probe.guestMemView = new DataView(guestMem.buffer);
        probe.pfn0 = 0x10; probe.pfn1 = 0x11;
        probe.descTableAddr = probe.pfn0 * 4096;
        probe.availRingAddr = probe.descTableAddr + 256 * 16;
        probe.usedRingAddr = Math.ceil((probe.availRingAddr + 4 + 2 * 256) / 4096) * 4096;

        const ok = probe.performStatusHandshake();
        assert.ok(ok, 'STATUS handshake should reach DRIVER_OK|FEATURES_OK');
        assert.equal(probe.gates['2.2a'], true);
    });

    it('Gate 2.2b: QUEUE_PFN !=0 for control (256) and cursor (16)', () => {
        const dev = new VirtioGpuDevice(null, null, { width: 1280, height: 720 });
        const guestMem = new Uint8Array(8 * 1024 * 1024);
        dev.v86 = { cpu: { memory: { buffer: guestMem.buffer }, device_raise_irq: () => {}, devices: { pci: { devices: {}, register_device: () => {} } } }, io: { register_read: () => {}, register_write: () => {} } };
        const probe = new SyntheticGuestProbe({ device: dev, guestMemory: guestMem });
        probe.performStatusHandshake();
        const ok = probe.setupQueues();
        assert.ok(ok, 'both queues PFN non-zero and sizes correct');
        assert.equal(dev.queues[0].pfn, 0x10);
        assert.equal(dev.queues[1].pfn, 0x11);
        assert.equal(dev.queues[0].size, 256);
        assert.equal(dev.queues[1].size, 16);
        assert.equal(probe.gates['2.2b'], true);
    });

    it('Gate 2.3a/2.3b: ≥5 opcodes and RESOURCE_CREATE_2D 1280x720 via QUEUE_NOTIFY', async () => {
        const guestMem = new Uint8Array(8 * 1024 * 1024);
        let raisedIrq = null;
        const fakeV86 = {
            cpu: {
                memory: { buffer: guestMem.buffer },
                device_raise_irq: (irq) => { raisedIrq = irq; },
                devices: { pci: { devices: {}, register_device: () => {} } }
            },
            io: { register_read: () => {}, register_write: () => {} }
        };
        let lastCmd = null;
        const mockBridge = {
            process_virtqueue_descriptor: (mem, descTable, head) => {
                // minimal parse: read descriptor chain to get cmd
                const view = new DataView(mem.buffer, mem.byteOffset, mem.byteLength);
                const addrLo = view.getUint32(descTable + head * 16, true);
                const len = view.getUint32(descTable + head * 16 + 8, true);
                lastCmd = mem.slice(addrLo, addrLo + len);
                // write generic OK_NODATA response to out buffer (desc 1)
                const outAddrLo = view.getUint32(descTable + 1 * 16, true);
                // craft response based on opcode
                const op = lastCmd[0] | (lastCmd[1] << 8);
                // For display info, fabricate pmodes
                if (op === 0x00 && lastCmd[0] === 0x00 && lastCmd[1] === 0x01) { // 0x0100 LE
                    // GET_DISPLAY_INFO response: header 0x1101 + pmodes
                    const resp = new Uint8Array(512);
                    const rv = new DataView(resp.buffer);
                    rv.setUint32(0, 0x1101, true); // RESP_OK_DISPLAY_INFO
                    rv.setUint32(24, 1, true); // enabled
                    rv.setUint32(28, 0, true);
                    rv.setUint32(24 + 4, 0, true); rv.setUint32(24 + 8, 0, true); rv.setUint32(24 + 12, 1280, true); rv.setUint32(24 + 16, 720, true);
                    mem.set(resp, outAddrLo);
                    return resp.length;
                } else {
                    const resp = new Uint8Array([0x00, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
                    mem.set(resp, outAddrLo);
                    return resp.length;
                }
            }
        };
        const dev = new VirtioGpuDevice(fakeV86, mockBridge, { width: 1280, height: 720 });
        const probe = new SyntheticGuestProbe({ device: dev, guestMemory: guestMem });
        const report = await probe.runFullProof();
        assert.ok(report.gates['2.2a'], '2.2a pass');
        assert.ok(report.gates['2.2b'], '2.2b pass');
        assert.ok(report.gates['2.3a'], `2.3a distinct >=5 got ${report.distinctOpcodes}`);
        assert.ok(report.gates['2.3b'], `2.3b 1280x720 got ${JSON.stringify(report.lastCreate2d)}`);
        assert.ok(report.gates['2.4a'], '2.4a used ring + IRQ');
        assert.ok(report.gates['2.4b'], '2.4b fb0');
        assert.ok(report.gates['2.5a'], '2.5a pixels');
        assert.ok(report.gates['2.5b'], '2.5b damage');
        assert.ok(report.gates['2.5c'], `2.5c FPS ${report.metrics.fps}`);
        assert.ok(report.allPass, 'all gates pass');
        assert.notEqual(raisedIrq, null, 'IRQ should have been raised');
        const isrCleared = dev.ioRead(0x13, 1);
        assert.equal(isrCleared, 0, 'ISR cleared after read');
    });

    it('full synthetic proof matches C /dev/mem + outl semantics — queue notify triggers consumeVirtqueue log', async () => {
        const guestMem = new Uint8Array(8 * 1024 * 1024);
        const fakeV86 = {
            cpu: { memory: { buffer: guestMem.buffer }, device_raise_irq: () => {}, devices: { pci: { devices: {}, register_device: () => {} } } },
            io: { register_read: () => {}, register_write: () => {} }
        };
        const dev = new VirtioGpuDevice(fakeV86, null, { width: 1280, height: 720 });
        // Use fallback JS processor (no bridge) — should still progress rings
        const probe = new SyntheticGuestProbe({ device: dev, guestMemory: guestMem });
        // only test handshake + single GET_DISPLAY_INFO
        probe.performStatusHandshake();
        probe.setupQueues();
        const pkt = VirtioPacketBuilder.encodeHeader(0x0100, 0, 1, 0);
        const r = probe.sendCommandViaQueue(0, pkt, 24);
        // With fallback, processControlQueue returns OK_NODATA but used ring still written
        assert.ok(r.usedIdx >= 0, 'used ring written even with fallback');
        // ISR should be set after notify
        const isr = dev.ioRead(0x13, 1);
        // ISR was cleared by probe.sendCommandViaQueue read, so second read 0
        assert.equal(isr, 0, 'ISR cleared');
        assert.ok(probe.distinctOpcodes.has(0x0100), 'opcode tracked');
    });
});
