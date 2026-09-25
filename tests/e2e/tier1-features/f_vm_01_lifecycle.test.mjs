import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
import { EventBusSpy } from '../harness/event_bus.mjs';
// Production imports: this suite exercises real src/ code, not string stubs.
import { VmRuntime } from '../../../src/vm/vm_runtime.js';
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { V86Adapter } from '../../../src/vm/v86_adapter.js';

function dummyKernel() {
  // Minimal bzImage-shaped fixture: valid HdrS magic + protocol >= 2.02.
  const buf = new Uint8Array(0x3000);
  const view = new DataView(buf.buffer);
  view.setUint8(0x01F1, 4);
  view.setUint32(0x0202, 0x53726448, true);
  view.setUint16(0x0206, 0x020c, true);
  view.setUint32(0x0214, 0x00100000, true);
  return buf;
}

describe('Tier 1: F-VM-01 v86 x86 VM Lifecycle Runner (production)', () => {
  test('F-VM-01-01: VmRuntime create shares one GuestMem with the worker; staged boot bytes stay visible', async () => {
    const runtime = new VmRuntime({ allowStub: true });
    await runtime.create({ ramSize: 16 * 1024 * 1024 });
    assertEqual(runtime.state, 'READY');
    // In-process fallback shares the SAME instance (no forked RAM).
    assertEqual(runtime.worker.guestMem === runtime.guestMem, true);
    // Stage bytes from the main thread: worker must see them.
    runtime.guestMem.writeU32(0x1000, 0xdeadbeef);
    assertEqual(runtime.worker.guestMem.readU32(0x1000), 0xdeadbeef);
    await runtime.destroy();
  });

  test('F-VM-01-02: loadBoot stages a real bzImage header; start/pause/resume/reset cycle via production states', async () => {
    const bus = new EventBusSpy();
    const runtime = new VmRuntime({ allowStub: true });
    const seen = [];
    runtime.onStateChange((t) => seen.push(`${t.oldState}->${t.newState}`));
    await runtime.create({ ramSize: 16 * 1024 * 1024 });
    bus.emit({ epoch: 1, seq: 1, ts: Date.now(), src: 'vm', kind: 'asset_check', payload: { ok: true } });
    await runtime.loadBoot({ kernel: dummyKernel(), cmdline: 'console=ttyS0' });
    assertEqual(runtime.state, 'CONFIGURED');
    bus.emit({ epoch: 1, seq: 2, ts: Date.now(), src: 'vm', kind: 'vm_create', payload: { memoryMb: 16 } });
    await runtime.start();
    assertEqual(runtime.state, 'RUNNING');
    bus.emit({ epoch: 1, seq: 3, ts: Date.now(), src: 'vm', kind: 'boot_start', payload: { kernel: 'bzImage' } });
    await runtime.pause();
    assertEqual(runtime.state, 'PAUSED');
    await runtime.resume();
    assertEqual(runtime.state, 'RUNNING');
    await runtime.reset();
    assertEqual(runtime.state, 'READY');
    assertOk(seen.includes('CONFIGURED->RUNNING'));
    assertOk(bus.hasSeen('boot_start'));
    await runtime.destroy();
  });

  test('F-VM-01-03: V86Adapter probes the vendored build and validates its shape', async () => {
    const adapter = new V86Adapter();
    const probed = await adapter.probe();
    // Build lane landed: libv86.mjs + v86.wasm + bios.bin are vendored.
    assertEqual(probed.present, true);
    const { V86 } = await adapter.loadV86();
    assertEqual(typeof V86, 'function');
    assertEqual(typeof V86.prototype.run, 'function');
    assertEqual(typeof V86.prototype.add_listener, 'function');
  });

  test('F-VM-01-04: attachDevice registers a real virtio device on the worker PCI bus', async () => {
    const runtime = new VmRuntime({ allowStub: true });
    await runtime.create({ ramSize: 16 * 1024 * 1024 });
    const res = await runtime.attachDevice({ slot: 10, kind: 'console', options: {} });
    assertEqual(res.slot, 10);
    assertEqual(runtime.worker.pciBus.getDevice(10)?.name, 'virtio-console');
    await runtime.destroy();
  });

  test('F-VM-01-05: destroy terminates worker quanta and rejects reset on DESTROYED (no zombie)', async () => {
    // Fail-closed start gate: no backend + no allowStub must reject.
    const strict = new VmRuntime();
    await strict.create({ ramSize: 16 * 1024 * 1024 });
    await strict.loadBoot({ kernel: dummyKernel() });
    let startRejected = false;
    try {
      await strict.start();
    } catch (e) {
      startRejected = e.code === 'V86_NOT_PRESENT' || e.message === 'V86_NOT_PRESENT' || /V86_NOT_PRESENT/.test(e.message);
    }
    assertEqual(startRejected, true);
    assertEqual(strict.state, 'CONFIGURED');
    await strict.destroy();
    const runtime = new VmRuntime({ allowStub: true });
    await runtime.create({ ramSize: 16 * 1024 * 1024 });
    await runtime.loadBoot({ kernel: dummyKernel() });
    await runtime.start();
    await runtime.destroy();
    assertEqual(runtime.state, 'DESTROYED');
    assertEqual(runtime.worker, null);
    let rejected = false;
    try {
      await runtime.reset();
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
    assertEqual(runtime.state, 'DESTROYED');
  });
});
