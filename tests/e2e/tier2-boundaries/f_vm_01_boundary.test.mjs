import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VmRuntime lifecycle from src/.
import { VmRuntime } from '../../../src/vm/vm_runtime.js';

function dummyKernel() {
  const buf = new Uint8Array(0x3000);
  const view = new DataView(buf.buffer);
  view.setUint8(0x01F1, 4);
  view.setUint32(0x0202, 0x53726448, true);
  view.setUint16(0x0206, 0x020c, true);
  view.setUint32(0x0214, 0x00100000, true);
  return buf;
}

describe('Tier 2: F-VM-01 Boundary & Corner Cases (production)', () => {
  test('F-VM-01-B01: start from READY without loadBoot is rejected', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    let rejected = false;
    try {
      await rt.start();
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
    await rt.destroy();
  });

  test('F-VM-01-B02: strict start without a v86 backend is rejected', async () => {
    const rt = new VmRuntime();
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    let rejected = false;
    try {
      await rt.start();
    } catch (e) {
      rejected = /V86_NOT_PRESENT/.test(e.message);
    }
    assertEqual(rejected, true);
    await rt.destroy();
  });

  test('F-VM-01-B03: rapid pause-resume oscillation maintains consistent state', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    await rt.start();
    for (let i = 0; i < 20; i++) {
      await rt.pause();
      await rt.resume();
    }
    assertEqual(rt.state, 'RUNNING');
    await rt.destroy();
  });

  test('F-VM-01-B04: reset during RUN returns to READY with devices intact', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    await rt.start();
    await rt.reset();
    assertEqual(rt.state, 'READY');
    assertOk(rt.worker.pciBus.getDevice(2)?.name === 'virtio-console');
    await rt.destroy();
  });

  test('F-VM-01-B05: worker fault transitions the runtime to ERROR and stops the watchdog', async () => {
    const rt = new VmRuntime({ allowStub: true });
    await rt.create({ ramSize: 16 * 1024 * 1024 });
    await rt.loadBoot({ kernel: dummyKernel() });
    await rt.start();
    const errors = [];
    rt.onError((e) => errors.push(e.code));
    rt.handleWorkerMessage({ type: 'EVT_ERROR', payload: { code: 'CPU_FAULT', message: 'test fault' } });
    assertEqual(rt.state, 'ERROR');
    assertOk(errors.includes('CPU_FAULT'));
    assertEqual(rt.watchdog.isActive, false);
    await rt.destroy();
  });
});
