/**
 * v86_backend_test.mjs - Contract tests for the worker-side v86 adapter.
 * Uses an API-shape double implementing the REAL starter surface observed in
 * vendor/v86 (constructor + run/stop/add_listener/destroy, serial0-output-byte
 * and emulator-started events). The double proves wiring only; it executes no
 * guest code and claims no VM boot.
 */
import assert from 'node:assert/strict';
import { V86WorkerBackend } from '../../src/vm/v86_backend.js';

let totalTests = 0;
let passedTests = 0;

function test(name, fn) {
  totalTests++;
  try {
    fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    console.error(`  ✗ ${name}`);
    console.error(err);
    process.exit(1);
  }
}

async function testAsync(name, fn) {
  totalTests++;
  try {
    await fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    console.error(`  ✗ ${name}`);
    console.error(err);
    process.exit(1);
  }
}

console.log('\n=== V86 BACKEND ADAPTER CONTRACT ===\n');

// Faithful API-shape double of the real V86 starter (see vendor/v86/v86.d.ts).
function makeFakeV86Module({ serial = 'Linux version 5.10.0', failStart = false } = {}) {
  const listeners = new Map();
  class FakeV86 {
    constructor(options) {
      this.options = options;
      this.runCalled = false;
      this.destroyCalled = false;
      if (options.autostart !== true) {
        throw new Error('FakeV86: adapter must pass autostart:true like the real starter flow');
      }
      // Mirror the real starter: async init -> emulator-loaded, then CPU start.
      queueMicrotask(() => {
        if (failStart) return;
        for (const fn of listeners.get('emulator-loaded') || []) fn();
        for (const fn of listeners.get('emulator-started') || []) fn();
        for (const ch of serial) {
          for (const fn of listeners.get('serial0-output-byte') || []) fn(ch.charCodeAt(0));
        }
      });
    }
    add_listener(event, fn) {
      if (!listeners.has(event)) listeners.set(event, []);
      listeners.get(event).push(fn);
    }
    async run() {
      this.runCalled = true;
    }
    async stop() {}
    async destroy() {
      this.destroyCalled = true;
    }
  }
  return { V86: FakeV86, __listeners: listeners };
}

await testAsync('V86WorkerBackend: boots via the real starter shape and captures serial', async () => {
  const mod = makeFakeV86Module();
  const seen = [];
  const backend = await V86WorkerBackend.create({
    loadModule: async () => mod,
    bios: { buffer: new Uint8Array([1, 2, 3]) },
    bzimage: { buffer: new Uint8Array([4, 5, 6]) },
    initrd: { buffer: new Uint8Array([7]) },
    cmdline: 'console=ttyS0',
    memorySize: 64 * 1024 * 1024,
    onSerialByte: (b) => seen.push(b),
    startTimeoutMs: 2000,
  });
  assert.equal(backend.emulatorRunning, true);
  assert.ok(backend.serialBytes.length > 0);
  assert.ok(seen.length > 0);
  assert.equal(backend.instructionsExecuted, null);
  const q = backend.runQuantum(50000);
  assert.equal(q.running, true);
  assert.equal(backend.quanta, 1);
  await backend.destroy();
  assert.equal(backend.stopped, true);
});

test('V86WorkerBackend: rejects modules without the {V86} starter shape', async () => {
  await assert.rejects(
    V86WorkerBackend.create({ loadModule: async () => ({}) }),
    (err) => err.message.includes('V86_API_INCOMPATIBLE')
  );
  await assert.rejects(
    V86WorkerBackend.create({ loadModule: async () => { throw new Error('nope'); } }),
    (err) => err.message === 'V86_NOT_PRESENT'
  );
});

test('V86WorkerBackend: deliverIrq is recorded unforwarded (v86 owns its PIC)', async () => {
  const mod = makeFakeV86Module();
  const backend = await V86WorkerBackend.create({
    loadModule: async () => mod,
    startTimeoutMs: 2000,
  });
  const res = backend.deliverIrq(11, true);
  assert.equal(res.forwarded, false);
  assert.equal(res.reason, 'V86_OWNS_PIC');
  await backend.destroy();
});

console.log(`\n=== RESULTS: ${passedTests} passed, ${totalTests - passedTests} failed, ${totalTests} total ===`);
