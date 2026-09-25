import { describe, test } from 'node:test';
import { assertEqual, assertOk, assertThrows } from '../harness/assertions.mjs';
// Production imports: real GuestMem, events, runtime from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { VmRuntime } from '../../../src/vm/vm_runtime.js';
import { EventBusSpy } from '../harness/event_bus.mjs';
import { makeRuntimeEvent, validateRuntimeEvent } from '../../../src/common/runtime_events.js';

function dummyKernel() {
  const buf = new Uint8Array(0x3000);
  const view = new DataView(buf.buffer);
  view.setUint8(0x01F1, 4);
  view.setUint32(0x0202, 0x53726448, true);
  view.setUint16(0x0206, 0x020c, true);
  view.setUint32(0x0214, 0x00100000, true);
  return buf;
}

describe('Tier 3: Pairwise VM ↔ Runtime Combinations (production)', () => {
  test('P-VM-RUN-01: production lifecycle emits ordered state transitions', async () => {
    const rt = new VmRuntime({ allowStub: true });
    const seen = [];
    rt.onStateChange((t) => seen.push(`${t.oldState}->${t.newState}`));
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    await rt.start();
    await rt.pause();
    await rt.resume();
    await rt.destroy();
    for (const leg of ['UNINITIALIZED->READY', 'READY->CONFIGURED', 'CONFIGURED->RUNNING', 'RUNNING->PAUSED', 'PAUSED->RUNNING']) {
      assertOk(seen.includes(leg), `missing ${leg}`);
    }
  });

  test('P-VM-RUN-02: worker fault transitions the runtime to ERROR', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    await rt.start();
    rt.handleWorkerMessage({ type: 'EVT_ERROR', payload: { code: 'PAGE_FAULT', message: 'cr2=0xdeadbeef' } });
    assertEqual(rt.state, 'ERROR');
    await rt.destroy();
  });

  test('P-VM-RUN-03: pause and resume preserve production guest memory', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    rt.guestMem.writeU32(0x50000, 0xCAFEBABE);
    await rt.start();
    await rt.pause();
    assertEqual(rt.guestMem.readU32(0x50000), 0xCAFEBABE);
    await rt.resume();
    assertEqual(rt.guestMem.readU32(0x50000), 0xCAFEBABE);
    await rt.destroy();
  });

  test('P-VM-RUN-04: fresh runtime starts from zeroed RAM, not stale bytes', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    assertEqual(rt.guestMem.readU32(0x1000), 0);
    rt.guestMem.writeU32(0x1000, 0xAAAAAAAA);
    await rt.destroy();
    const rt2 = new VmRuntime({ allowStub: true });
    await rt2.create({ ramSize: 16 * 1024 * 1024 });
    assertEqual(rt2.guestMem.readU32(0x1000), 0);
    await rt2.destroy();
  });

  test('P-VM-RUN-05: production event validator rejects corrupt worker events', () => {
    assertThrows(() => validateRuntimeEvent(null), /INVALID_RUNTIME_EVENT/);
    assertThrows(() => validateRuntimeEvent({ epoch: '1', seq: 1, ts: 1, src: 'vm', kind: 'step' }), /INVALID_EVENT_EPOCH/);
    assertThrows(() => validateRuntimeEvent({ epoch: 1, seq: '1', ts: 1, src: 'vm', kind: 'step' }), /INVALID_EVENT_SEQ/);
    assertThrows(() => validateRuntimeEvent({ epoch: 1, seq: 1, ts: 1, src: '', kind: 'step' }), /INVALID_EVENT_SRC/);
    assertThrows(() => validateRuntimeEvent({ epoch: 1, seq: 1, ts: 1, src: 'vm', kind: '' }), /INVALID_EVENT_KIND/);
    assertOk(validateRuntimeEvent(makeRuntimeEvent({ epoch: 1, seq: 1, src: 'vm', kind: 'boot_start' })));
    const bus = new EventBusSpy();
    bus.emit(makeRuntimeEvent({ epoch: 1, seq: 1, src: 'vm', kind: 'boot_start' }));
    assertOk(bus.hasSeen('boot_start'));
  });
});
