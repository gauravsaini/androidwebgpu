/**
/**
 * m1_stress_challenge_test.mjs - Empirical Challenger Test Suite for Milestone 1
 *
 * Stress-tests and fuzzes:
 * 1. IrqController: High-concurrency interrupt assertion, line-sharing invariants,
 *    rapid INTx masking/unmasking, and multi-line re-entrancy.
 * 2. PciBus: 32-slot saturation, dynamic BAR reallocation under load,
 *    Config Mechanism 1 fuzzing, and unaligned port bounds checks.
 * 3. VmRuntime: Rapid state cycling, watchdog timing precision, pause/resume accuracy,
 *    and error resilience under adversarial conditions.
 */

import assert from 'node:assert/strict';
import {
  IrqController,
  VIRTIO_ISR_QUEUE,
  VIRTIO_ISR_CONFIG,
  PCI_COMMAND_INTX_DIS,
  PCI_STATUS_INTX_STATUS
} from '../../src/virtio/irq_controller.js';
import {
  PciBus,
  PCI_CONFIG_ADDRESS_PORT,
  PCI_CONFIG_DATA_PORT
} from '../../src/virtio/pci_bus.js';
import {
  VirtioPciDevice
} from '../../src/virtio/virtio_pci_device.js';
import {
  VmRuntime,
  VmStateError,
  VmWatchdogTimeoutError
} from '../../src/vm/vm_runtime.js';

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;

function test(name, fn) {
  totalTests++;
  try {
    fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    failedTests++;
    console.error(`  ✗ ${name}`);
    console.error(`    ${err.message}`);
  }
}

async function testAsync(name, fn) {
  totalTests++;
  try {
    await fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    failedTests++;
    console.error(`  ✗ ${name}`);
    console.error(`    ${err.message}`);
  }
}

function createDummyKernel() {
  const kernel = new Uint8Array(1024 * 32);
  const dv = new DataView(kernel.buffer);
  dv.setUint32(0x202, 0x53726448, true); // HdrS magic
  dv.setUint16(0x206, 0x020d, true);     // version 2.13
  dv.setUint8(0x211, 0x01);              // loadflags
  dv.setUint32(0x214, 0x00100000, true); // code32_start
  dv.setUint32(0x260, 1024 * 32, true);  // init_size
  return kernel;
}

console.log('=== RUNNING MILESTONE 1 EMPIRICAL CHALLENGE SUITE ===\n');

// ============================================================================
// Suite 1: IrqController Concurrency & Line Sharing Invariant Stress
// ============================================================================
console.log('--- Challenge Suite 1: IrqController Concurrency & Invariants ---');

test('IrqController: 50,000 interleaved operations across shared INTx lines', () => {
  let physicalLineState = false;
  let lineTransitions = 0;

  const irqCtrl = new IrqController((irq, level) => {
    physicalLineState = level;
    lineTransitions++;
  });

  const bus = new PciBus(irqCtrl);
  const devices = [];
  const numDevices = 8;
  for (let i = 0; i < numDevices; i++) {
    const dev = new VirtioPciDevice({ name: `dev_${i}`, subsystemDeviceId: 1, irqLine: 10 });
    bus.registerDevice(i, dev);
    devices.push(dev);
  }

  const activeAsserted = new Set();
  const isMasked = new Array(numDevices).fill(false);

  for (let op = 0; op < 50000; op++) {
    const devIdx = Math.floor(Math.random() * numDevices);
    const dev = devices[devIdx];
    const action = Math.floor(Math.random() * 4);

    if (action === 0) {
      // Assert device interrupt
      const bit = Math.random() < 0.5 ? VIRTIO_ISR_QUEUE : VIRTIO_ISR_CONFIG;
      irqCtrl.assertDeviceIrq(dev, bit);
      activeAsserted.add(devIdx);
    } else if (action === 1) {
      // Read ISR status (destructive read-to-clear)
      irqCtrl.readIsrStatus(dev);
      activeAsserted.delete(devIdx);
    } else if (action === 2) {
      // Mask INTx
      isMasked[devIdx] = true;
      dev.writePciConfig(0x04, PCI_COMMAND_INTX_DIS, 2);
    } else if (action === 3) {
      // Unmask INTx
      isMasked[devIdx] = false;
      dev.writePciConfig(0x04, 0x0000, 2);
    }

    // Invariant: Physical line must be HIGH iff at least one active asserting device is unmasked
    let expectedHigh = false;
    for (const idx of activeAsserted) {
      if (!isMasked[idx]) {
        expectedHigh = true;
        break;
      }
    }

    assert.equal(
      physicalLineState,
      expectedHigh,
      `Invariant violated at op ${op}! Expected physical line: ${expectedHigh}, Actual: ${physicalLineState}`
    );
  }

  assert.ok(lineTransitions > 500, `Expected active line transitions, got ${lineTransitions}`);
});

test('IrqController: Multi-line distribution, re-entrancy, and clearDevice', () => {
  const lineStates = new Map();
  let reentrantFired = false;

  const irqCtrl = new IrqController((irq, level) => {
    lineStates.set(irq, level);
  });

  const bus = new PciBus(irqCtrl);
  const devices = [];
  const lines = [9, 10, 11, 14, 15];

  for (let i = 0; i < 15; i++) {
    const irqLine = lines[i % lines.length];
    const dev = new VirtioPciDevice({ name: `dev_${i}`, subsystemDeviceId: 1, irqLine });
    bus.registerDevice(i, dev);
    devices.push({ dev, irqLine });
  }

  // Hook up re-entrant callback on line 10
  irqCtrl.setVmIrqCallback((irq, level) => {
    lineStates.set(irq, level);
    if (irq === 10 && level && !reentrantFired) {
      reentrantFired = true;
      // Re-entrant assertion of device on line 11
      irqCtrl.assertDeviceIrq(devices[2].dev, VIRTIO_ISR_QUEUE);
    }
  });

  irqCtrl.assertDeviceIrq(devices[1].dev, VIRTIO_ISR_QUEUE); // Line 10
  assert.equal(reentrantFired, true);
  assert.equal(lineStates.get(10), true);
  assert.equal(lineStates.get(11), true);

  // Clear device 1
  irqCtrl.clearDevice(devices[1].dev);
  assert.equal(lineStates.get(10), false);
  assert.equal(lineStates.get(11), true); // Line 11 unaffected

  // Clear device 2
  irqCtrl.clearDevice(devices[2].dev);
  assert.equal(lineStates.get(11), false);
});

// ============================================================================
// Suite 2: PciBus Stress, Saturation, and Boundary Fuzzing
// ============================================================================
console.log('\n--- Challenge Suite 2: PciBus Saturation & Fuzzing ---');

test('PciBus: Full 32-slot saturation and dynamic BAR routing stress', () => {
  const bus = new PciBus(new IrqController());
  const devices = [];

  for (let slot = 0; slot < 32; slot++) {
    const dev = new VirtioPciDevice({ name: `dev_${slot}`, subsystemDeviceId: (slot % 10) + 1 });
    bus.registerDevice(slot, dev);
    devices.push(dev);

    // Set up unique non-overlapping BARs
    const ioBase = 0x1000 + slot * 0x40;
    const mmioBase = 0xD0000000 + slot * 0x1000;
    dev.writePciConfig(0x10, ioBase, 4);
    dev.writePciConfig(0x14, mmioBase, 4);
  }

  assert.equal(bus.ioBars.length, 32);
  assert.equal(bus.mmioBars.length, 32);

  // 10,000 randomized accesses across all 32 devices
  for (let i = 0; i < 10000; i++) {
    const slot = Math.floor(Math.random() * 32);
    const ioVal = bus.readIo(0x1000 + slot * 0x40, 4);
    assert.notEqual(ioVal, 0xFFFFFFFF, `Failed IO read on slot ${slot}`);

    const mmioVal = bus.readMmio(0xD0000000 + slot * 0x1000, 4);
    assert.notEqual(mmioVal, 0xFFFFFFFF, `Failed MMIO read on slot ${slot}`);
  }
});

test('PciBus: Configuration Mechanism 1 address space fuzzing (10,000 cycles)', () => {
  const bus = new PciBus(new IrqController());
  const dev = new VirtioPciDevice({ name: 'dev0', subsystemDeviceId: 1 });
  bus.registerDevice(0, dev);

  for (let i = 0; i < 10000; i++) {
    // Generate arbitrary 32-bit CONFIG_ADDRESS values
    const addr = (Math.random() * 0xFFFFFFFF) >>> 0;
    bus.writeConfigAddress(addr);

    const busNum = (addr & 0x00FF0000) >>> 16;
    const slotNum = (addr & 0x0000F800) >>> 11;
    const funcNum = (addr & 0x00000700) >>> 8;
    const enabled = (addr & 0x80000000) !== 0;

    // Aligned 4-byte read from 0xCFC
    const val = bus.readConfigData(0xCFC, 4);

    if (!enabled || busNum !== 0 || funcNum !== 0 || slotNum !== 0) {
      assert.equal(val, 0xFFFFFFFF, `Expected 0xFFFFFFFF for unmapped config address 0x${addr.toString(16)}`);
    } else {
      // Must return valid data from registered device 0
      assert.notEqual(val, undefined);
    }
  }
});

test('PciBus FIX: Unaligned config read returns all-ones, never RangeError', () => {
  const bus = new PciBus(new IrqController());
  const dev = new VirtioPciDevice({ name: 'dev0', subsystemDeviceId: 1 });
  bus.registerDevice(0, dev);

  // Address last register 0xFC (252) of device 0
  bus.writeConfigAddress(0x80000000 | (0 << 11) | 0xFC);

  // Issue 4-byte read at port 0xCFD (subOffset = 1, total offset = 253)
  // 253 + 4 = 257 > 256. Standard PCI returns 0xFFFFFFFF; must not throw.
  let val;
  let threw = null;
  try {
    val = bus.readConfigData(0xCFD, 4);
  } catch (err) {
    threw = err;
  }

  assert.equal(threw, null, 'Unaligned tail read must not throw RangeError');
  assert.equal(val, 0xFFFFFFFF, 'Expected 0xFFFFFFFF for out-of-range tail read');
});

// ============================================================================
// Suite 3: VmRuntime Lifecycle & Rapid State Cycling Stress
// ============================================================================
console.log('\n--- Challenge Suite 3: VmRuntime Lifecycle & State Cycling ---');

await testAsync('VmRuntime: 50 rapid sequential lifecycle cycles (load/start/pause/resume/reset)', async () => {
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 50, watchdogTimeoutMs: 300 });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });

  const kernel = createDummyKernel();

  for (let i = 0; i < 50; i++) {
    await runtime.loadBoot({ kernel, cmdline: `cycle=${i}` });
    assert.equal(runtime.state, 'CONFIGURED');

    await runtime.start();
    assert.equal(runtime.state, 'RUNNING');

    await runtime.pause();
    assert.equal(runtime.state, 'PAUSED');

    await runtime.resume();
    assert.equal(runtime.state, 'RUNNING');

    await runtime.pause();
    assert.equal(runtime.state, 'PAUSED');

    await runtime.reset();
    assert.equal(runtime.state, 'READY');
  }

  await runtime.destroy();
  assert.equal(runtime.state, 'DESTROYED');
});

await testAsync('VmRuntime: Rapid un-awaited pause/resume toggling under execution', async () => {
  const runtime = new VmRuntime({ allowStub: true });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });
  await runtime.loadBoot({ kernel: createDummyKernel() });
  await runtime.start();

  // Burst 100 rapid pause/resume calls synchronously
  for (let i = 0; i < 100; i++) {
    runtime.pause();
    runtime.resume();
  }

  assert.equal(runtime.state, 'RUNNING');
  await runtime.destroy();
});

await testAsync('VmRuntime: Rejection of illegal state transitions', async () => {
  const runtime = new VmRuntime({ allowStub: true });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });

  // Cannot start directly from READY (must loadBoot first)
  await assert.rejects(
    async () => runtime.start(),
    (err) => err instanceof VmStateError && err.code === 'INVALID_VM_STATE'
  );

  await runtime.loadBoot({ kernel: createDummyKernel() });
  await runtime.start();

  // Cannot start while already RUNNING
  await assert.rejects(
    async () => runtime.start(),
    (err) => err instanceof VmStateError && err.code === 'INVALID_VM_STATE'
  );

  // Cannot loadBoot while RUNNING
  await assert.rejects(
    async () => runtime.loadBoot({ kernel: createDummyKernel() }),
    (err) => err instanceof VmStateError && err.code === 'INVALID_VM_STATE'
  );

  await runtime.destroy();
});

// ============================================================================
// Suite 4: VmWatchdog Precision, Accuracy, and Bug Mining
// ============================================================================
console.log('\n--- Challenge Suite 4: VmWatchdog Accuracy & Vulnerabilities ---');

await testAsync('VmWatchdog: Precision calibration on silent worker', async () => {
  const targetTimeout = 200;
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 40, watchdogTimeoutMs: targetTimeout });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });
  await runtime.loadBoot({ kernel: createDummyKernel() });
  await runtime.start();

  const silenceStart = Date.now();
  runtime.worker.postMessage = () => {}; // drop all pings

  const err = await new Promise((resolve) => {
    runtime.onError((err) => resolve(err));
  });

  const duration = Date.now() - silenceStart;
  assert.equal(err.code, 'WATCHDOG_TIMEOUT');
  assert.ok(
    duration >= targetTimeout - 10 && duration <= targetTimeout + 80,
    `Watchdog fired in ${duration}ms, expected window [${targetTimeout - 10}ms, ${targetTimeout + 80}ms]`
  );

  await runtime.destroy();
});

await testAsync('VmWatchdog: Immunity to timeouts during PAUSED state', async () => {
  const timeoutMs = 150;
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 30, watchdogTimeoutMs: timeoutMs });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });
  await runtime.loadBoot({ kernel: createDummyKernel() });
  await runtime.start();

  let errorFired = false;
  runtime.onError(() => { errorFired = true; });

  await runtime.pause();
  // Sleep for 250ms (longer than timeoutMs 150ms)
  await new Promise((r) => setTimeout(r, 250));

  assert.equal(errorFired, false, 'Watchdog must not fire while VM is PAUSED');
  assert.equal(runtime.state, 'PAUSED');

  await runtime.resume();
  await new Promise((r) => setTimeout(r, 80));

  assert.equal(errorFired, false, 'Watchdog must not falsely fire upon RESUME');
  assert.equal(runtime.state, 'RUNNING');

  await runtime.destroy();
});

await testAsync('VmWatchdog FIX: Delayed PONGs within timeout keep worker alive', async () => {
  // Interval 40ms, timeout 300ms.
  // Worker responds to every ping within 60ms (well within 300ms timeout).
  // Monotonic ack accepts delayed pongs; no false-positive termination.
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 40, watchdogTimeoutMs: 300 });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });
  await runtime.loadBoot({ kernel: createDummyKernel() });
  await runtime.start();

  runtime.worker.handleMessage = (msg) => {
    if (msg.type === 'CMD_PING') {
      setTimeout(() => {
        runtime.handleWorkerMessage({
          type: 'EVT_PONG',
          payload: { seq: msg.payload.seq }
        });
      }, 60);
    }
  };

  let timedOut = false;
  runtime.onError((err) => {
    if (err.code === 'WATCHDOG_TIMEOUT') {
      timedOut = true;
    }
  });

  // Monitor for 450ms
  await new Promise((r) => setTimeout(r, 450));

  assert.equal(
    timedOut,
    false,
    'Delayed-but-timely PONGs must not trigger WATCHDOG_TIMEOUT'
  );

  await runtime.destroy();
});

await testAsync('VmRuntime FIX: Watchdog stops after worker EVT_ERROR (no spurious timeout)', async () => {
  const runtime = new VmRuntime({ allowStub: true,  heartbeatIntervalMs: 30, watchdogTimeoutMs: 150 });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });
  await runtime.loadBoot({ kernel: createDummyKernel() });
  await runtime.start();

  const errors = [];
  runtime.onError((err) => {
    errors.push(err.code);
  });

  // Simulate CPU fault
  runtime.handleWorkerMessage({
    type: 'EVT_ERROR',
    payload: { code: 'CPU_FAULT', message: 'Triple fault at 0x1000' }
  });

  // Silence worker
  runtime.worker.postMessage = () => {};

  // Wait 250ms: watchdog must stay silent on the already-errored VM
  await new Promise((r) => setTimeout(r, 250));

  assert.ok(
    errors.includes('CPU_FAULT') && !errors.includes('WATCHDOG_TIMEOUT'),
    'Watchdog must stop after EVT_ERROR; no spurious WATCHDOG_TIMEOUT expected'
  );

  await runtime.destroy();
});

await testAsync('VmRuntime FIX: reset() on DESTROYED VM is rejected (no zombie)', async () => {
  const runtime = new VmRuntime({ allowStub: true });
  await runtime.create({ ramSize: 16 * 1024 * 1024 });
  await runtime.destroy();
  assert.equal(runtime.state, 'DESTROYED');

  // Attempt reset on DESTROYED runtime: must reject, state stays DESTROYED
  await assert.rejects(
    async () => runtime.reset(),
    (err) => err instanceof VmStateError && err.code === 'INVALID_VM_STATE'
  );
  assert.equal(runtime.state, 'DESTROYED', 'State must remain DESTROYED after rejected reset');
  assert.equal(runtime.worker, null, 'Worker remains null after rejected reset');
});

console.log(`\n=== RESULTS: ${passedTests} passed, ${failedTests} failed, ${totalTests} total ===`);
