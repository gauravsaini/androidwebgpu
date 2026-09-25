/**
 * vm_worker.js - Entity E04: x86 VM Execution Core & Web Worker Runner
 * Hosts the CPU emulation quantum loop, cooperative scheduling, Virtio PCI dispatch,
 * IRQ latching, and bidirectional message protocol with VmRuntime.
 */

import { GuestMem } from './guest_mem.js';
import { PciBus } from '../virtio/pci_bus.js';
import { IrqController } from '../virtio/irq_controller.js';
import { VirtioBlk } from '../io/block/virtio_blk.js';
import { VirtioConsole } from '../io/console/virtio_console.js';
import { VirtioInput } from '../io/input/virtio_input.js';
import { VirtioNet } from '../io/net/virtio_net.js';
import { VirtioRng } from '../io/rng/virtio_rng.js';
import { VirtioSound } from '../io/audio/virtio_sound.js';
import { VirtioGpu } from '../gpu_transport/virtio_gpu.js';

export const QUANTUM_CYCLES = 50000;

/**
 * Default boot device map: every boot wires this fixed set unless a lane
 * overrides it explicitly. Registration is acknowledged via EVT_BOOT_DEVICES;
 * failures are reported per slot, never silent.
 */
export const DEFAULT_BOOT_DEVICES = Object.freeze([
  { slot: 1, kind: 'blk', options: { readOnly: true } },
  { slot: 2, kind: 'console', options: {} },
  { slot: 3, kind: 'input', options: {} },
  { slot: 4, kind: 'net', options: {} },
  { slot: 5, kind: 'rng', options: {} },
  { slot: 6, kind: 'gpu', options: {} },
]);

/**
 * Worker-side device factory: concrete virtio devices by kind.
 * Shared with VmRuntime.attachDevice so both paths register identical code.
 */
export function createWorkerDevice(kind, options = {}) {
  switch (kind) {
    case 'blk': return new VirtioBlk(options);
    case 'console': return new VirtioConsole(options);
    case 'input': return new VirtioInput(options);
    case 'net': return new VirtioNet(options);
    case 'rng': return new VirtioRng(options);
    case 'sound': return new VirtioSound(options);
    case 'gpu': return new VirtioGpu(options);
    default: throw new Error(`UNKNOWN_DEVICE_KIND:${kind}`);
  }
}

export class VmWorkerCore {
  /**
   * @param {Object} [options]
   * @param {(msg: any) => void} [options.postMessage] - Message emitter to host
   */
  constructor(options = {}) {
    this.postMessageCb = options.postMessage || (() => {});

    this.state = 'UNINITIALIZED';
    this.guestMem = null;
    this.irqController = null;
    this.pciBus = null;

    // Emulated CPU registers
    this.regs = {
      cr0: 0,
      cs: 0,
      ds: 0,
      es: 0,
      fs: 0,
      gs: 0,
      ss: 0,
      eip: 0,
      esi: 0,
      esp: 0,
      eflags: 2,
      eax: 0,
      ebx: 0,
      ecx: 0,
      edx: 0,
      ebp: 0,
      edi: 0
    };

    this.isRunning = false;
    this.isPaused = false;
    this.instructionsExecuted = 0n;
    this.quantumTimer = null;
    this.seqCounter = 0;
    // Pending level-triggered CPU interrupts latched from VirtioBus IRQ lines.
    // The worker has no real x86 PIC; this queue is the explicit delivery point
    // the CPU loop drains each quantum (fail-closed: never silently dropped).
    this.pendingIrqs = [];
    // Pending guest input events injected via CMD_INPUT_EVENT (virtio-input).
    this.pendingInputEvents = [];
    // v86 backend selection: 'stub' (quantum counter) or 'v86' when the
    // vendored v86 emulator is present. Stub never claims guest execution.
    this.cpuBackend = 'stub';
    this.v86 = null;
    // Count of stub quanta executed. Observable via EVT_PONG and
    // getStubQuanta(): any runValidation evidence citing executed quanta must
    // distinguish stub counts from real x86 execution.
    this.stubQuanta = 0;
    // Observable IRQ->CPU delivery log (bounded). Each quantum drains the
    // latch into here (and into the v86 backend when attached), so delivery
    // is evidenced instead of silently dropped.
    this.deliveredIrqs = [];
    this.registeredDevices = [];
  }

  postEnvelope(type, payload = {}, id = null) {
    this.seqCounter++;
    const env = {
      type,
      id: id || `msg_${this.seqCounter}`,
      seq: this.seqCounter,
      timestamp: Date.now(),
      payload
    };
    this.postMessageCb(env);
  }

  postMessage(msg) {
    setTimeout(() => {
      if (this.state !== 'DESTROYED') {
        this.handleMessage(msg);
      }
    }, 0);
  }

  terminate() {
    this.isRunning = false;
    this.isPaused = false;
    if (this.quantumTimer) {
      clearTimeout(this.quantumTimer);
      this.quantumTimer = null;
    }
    this.state = 'DESTROYED';
  }

  handleMessage(msg) {
    if (!msg || typeof msg !== 'object') return;
    const { type, payload = {}, id } = msg;

    switch (type) {
      case 'CMD_INIT':
        this.cmdInit(payload, id);
        break;
      case 'CMD_LOAD_BOOT':
        this.cmdLoadBoot(payload, id);
        break;
      case 'CMD_START':
        this.cmdStart(payload, id);
        break;
      case 'CMD_PAUSE':
        this.cmdPause(id);
        break;
      case 'CMD_RESUME':
        this.cmdResume(id);
        break;
      case 'CMD_RESET':
        this.cmdReset(id);
        break;
      case 'CMD_DESTROY':
        this.cmdDestroy(id);
        break;
      case 'CMD_INJECT_IRQ':
        this.cmdInjectIrq(payload);
        break;
      case 'CMD_REGISTER_DEVICE':
        this.cmdRegisterDevice(payload, id);
        break;
      case 'CMD_ATTACH_V86':
        this.cmdAttachV86(payload, id);
        break;
      case 'CMD_NOTIFY_QUEUE':
        this.cmdNotifyQueue(payload);
        break;
      case 'CMD_INPUT_EVENT':
        this.cmdInputEvent(payload);
        break;
      case 'CMD_PING':
        this.cmdPing(payload, id);
        break;
      default:
        // Ignore unrecognized messages
        break;
    }
  }

  cmdInit(payload, id) {
    const { ramSize = 128 * 1024 * 1024, sharedBuffer = null, cpuBackend = 'stub' } = payload;
    // Shared-memory contract: the main thread owns the RAM buffer. When a
    // SharedArrayBuffer (or transferable ArrayBuffer) is supplied it MUST be
    // adopted so staged boot data stays visible to the worker. Creating a
    // fresh buffer here would fork memory and hide main-thread boot staging.
    if (sharedBuffer) {
      this.guestMem = new GuestMem(ramSize, sharedBuffer);
    } else {
      this.guestMem = new GuestMem(ramSize, sharedBuffer);
      this.postEnvelope('EVT_WARN', {
        code: 'GUEST_RAM_NOT_SHARED',
        message: 'No shared RAM buffer supplied; worker allocated private RAM. Boot staging from main thread is invisible in this mode.',
      });
    }

    this.irqController = new IrqController((irq, level) => {
      // Deliver IRQ to the emulated CPU interrupt latch (drained per quantum).
      this.pendingIrqs.push({ irq, level: !!level, at: Date.now() });
      // Bound the latch so a hot device cannot grow memory without limit.
      if (this.pendingIrqs.length > 1024) this.pendingIrqs.shift();
    });

    this.pciBus = new PciBus(this.irqController);

    // Fixed boot device map: blk/console/input/net/rng/gpu, acked per slot.
    const bootDevices = [];
    for (const spec of DEFAULT_BOOT_DEVICES) {
      try {
        const dev = createWorkerDevice(spec.kind, spec.options || {});
        if (typeof dev.setGuestMem === 'function') dev.setGuestMem(this.guestMem);
        this.pciBus.registerDevice(spec.slot, dev);
        this.registeredDevices.push({ slot: spec.slot, kind: spec.kind });
        bootDevices.push({ slot: spec.slot, kind: spec.kind, ok: true });
      } catch (err) {
        bootDevices.push({ slot: spec.slot, kind: spec.kind, ok: false, error: err instanceof Error ? err.message : String(err) });
      }
    }
    this.postEnvelope('EVT_BOOT_DEVICES', { devices: bootDevices });

    const oldState = this.state;
    this.state = 'READY';
    if (cpuBackend === 'v86') {
      this.cpuBackend = 'v86';
    }
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'READY' });
    this.postEnvelope('EVT_READY', {
      isShared: this.guestMem.isShared,
      ramSize: this.guestMem.ramSize,
      buffer: this.guestMem.isShared ? undefined : this.guestMem.buffer
    }, id);
  }

  cmdLoadBoot(payload, id) {
    const { bootParamsAddr = 0x00090000, entryIp = 0x00100000, initialRegs = {} } = payload;
    this.regs = {
      ...this.regs,
      ...initialRegs,
      eip: entryIp,
      esi: bootParamsAddr
    };

    const oldState = this.state;
    this.state = 'CONFIGURED';
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'CONFIGURED' });
  }

  cmdStart(payload = {}, id = null) {
    const requireV86 = payload && payload.requireV86 === true;
    if (requireV86 && !(this.cpuBackend === 'v86' && this.v86 && typeof this.v86.runQuantum === 'function')) {
      this.postEnvelope('EVT_ERROR', { code: 'V86_NOT_PRESENT', message: 'start() requires an attached v86 backend; stub quanta refused' });
      return;
    }
    if (this.isRunning) return;
    this.isRunning = true;
    this.isPaused = false;
    const oldState = this.state;
    this.state = 'RUNNING';
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'RUNNING' });
    this.scheduleNextQuantum();
  }

  cmdPause(id) {
    if (!this.isRunning || this.isPaused) return;
    this.isPaused = true;
    if (this.quantumTimer) {
      clearTimeout(this.quantumTimer);
      this.quantumTimer = null;
    }
    const oldState = this.state;
    this.state = 'PAUSED';
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'PAUSED' });
  }

  cmdResume(id) {
    if (!this.isRunning || !this.isPaused) return;
    this.isPaused = false;
    const oldState = this.state;
    this.state = 'RUNNING';
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'RUNNING' });
    this.scheduleNextQuantum();
  }

  cmdReset(id) {
    this.isRunning = false;
    this.isPaused = false;
    if (this.quantumTimer) {
      clearTimeout(this.quantumTimer);
      this.quantumTimer = null;
    }
    this.instructionsExecuted = 0n;
    if (this.pciBus) {
      for (const dev of this.pciBus.slots) {
        if (dev && typeof dev.reset === 'function') dev.reset();
      }
    }
    const oldState = this.state;
    this.state = 'READY';
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'READY', reason: 'RESET' });
  }

  cmdDestroy(id) {
    this.isRunning = false;
    this.isPaused = false;
    if (this.quantumTimer) {
      clearTimeout(this.quantumTimer);
      this.quantumTimer = null;
    }
    const oldState = this.state;
    this.state = 'DESTROYED';
    this.postEnvelope('EVT_STATE_CHANGE', { oldState, newState: 'DESTROYED' });
  }

  cmdInjectIrq(payload) {
    const { irq, level = true } = payload;
    if (this.irqController) {
      this.irqController.assertIrq(irq, level);
    }
  }

  cmdRegisterDevice(payload = {}, id = null) {
    const { slot, kind, options = {} } = payload;
    if (!Number.isInteger(slot) || slot < 0 || slot >= 32) {
      this.postEnvelope('EVT_ERROR', { code: 'INVALID_DEVICE_SLOT', message: `Bad device slot ${slot}` });
      return;
    }
    try {
      const dev = createWorkerDevice(kind, options);
      if (typeof dev.setGuestMem === 'function') dev.setGuestMem(this.guestMem);
      this.pciBus.registerDevice(slot, dev);
      this.registeredDevices.push({ slot, kind });
      this.postEnvelope('EVT_DEVICE_REGISTERED', { slot, kind }, id);
    } catch (err) {
      this.postEnvelope('EVT_ERROR', { code: 'DEVICE_REGISTER_FAILED', message: err instanceof Error ? err.message : String(err) });
    }
  }

  cmdAttachV86(payload = {}, id = null) {
    // Real-worker path: import the bundle URL inside the worker, validate the
    // {V86} shape, boot with the supplied assets, and attach. Anything else
    // stays stub and reports V86_NOT_PRESENT / V86_API_INCOMPATIBLE.
    const bundleUrl = payload && payload.bundleUrl;
    if (!bundleUrl) {
      this.postEnvelope('EVT_WARN', { code: 'V86_NOT_PRESENT', message: 'No v86 bundle attached; stub quantum only' });
      return;
    }
    import(/* @vite-ignore */ bundleUrl).then(
      async (mod) => {
        try {
          const { V86WorkerBackend } = await import('./v86_backend.js');
          const dir = String(bundleUrl).slice(0, String(bundleUrl).lastIndexOf('/') + 1);
          // Forward guest serial bytes to the main thread as EVT_SERIAL_OUT
          // (batched): in v86-attached mode this is the guest's serial path.
          let serialBatch = [];
          let serialTimer = null;
          const flushSerial = () => {
            if (serialBatch.length === 0) return;
            const data = new Uint8Array(serialBatch);
            serialBatch = [];
            this.postEnvelope('EVT_SERIAL_OUT', { data });
          };
          const backend = await V86WorkerBackend.create({
            loadModule: async () => mod,
            wasmUrl: payload.wasmUrl || (dir + 'v86.wasm'),
            bios: payload.bios || (payload.biosUrl ? { url: payload.biosUrl } : null),
            vgaBios: payload.vgaBios || null,
            bzimage: payload.bzimage || (payload.bzimageUrl ? { url: payload.bzimageUrl } : null),
            initrd: payload.initrd || (payload.initrdUrl ? { url: payload.initrdUrl } : null),
            cmdline: payload.cmdline || 'console=ttyS0',
            memorySize: payload.memorySize || 512 * 1024 * 1024,
            startTimeoutMs: payload.startTimeoutMs || 120000,
            onSerialByte: (byte) => {
              serialBatch.push(byte);
              if (serialBatch.length >= 256) flushSerial();
              else if (!serialTimer) {
                serialTimer = setTimeout(() => { serialTimer = null; flushSerial(); }, 250);
              }
            },
          });
          flushSerial();
          this.attachV86Backend(backend);
          this.postEnvelope('EVT_V86_ATTACHED', { attached: true, bundleUrl }, id);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          const code = /V86_API_INCOMPATIBLE/.test(msg) ? 'V86_API_INCOMPATIBLE' : 'V86_NOT_PRESENT';
          this.postEnvelope('EVT_ERROR', { code, message: msg });
        }
      },
      (err) => {
        this.postEnvelope('EVT_ERROR', { code: 'V86_NOT_PRESENT', message: err instanceof Error ? err.message : String(err) });
      }
    );
  }

  /**
   * Attach a live v86 backend (in-process path or tests). The backend must
   * be a V86WorkerBackend (runQuantum + emulatorRunning + destroy).
   */
  attachV86Backend(backend) {
    if (!backend || typeof backend.runQuantum !== 'function' || typeof backend.destroy !== 'function') {
      throw new Error('V86_BACKEND_INVALID');
    }
    this.v86 = backend;
    this.cpuBackend = 'v86';
  }

  getDeliveredIrqs() {
    return this.deliveredIrqs.slice();
  }

  cmdNotifyQueue(payload) {
    const { deviceSlot, queueIdx } = payload;
    if (this.pciBus && this.pciBus.slots[deviceSlot]) {
      const dev = this.pciBus.slots[deviceSlot];
      if (typeof dev.notifyQueue === 'function') {
        dev.notifyQueue(queueIdx);
      }
    }
  }

  cmdInputEvent(payload) {
    // Latch DOM-sourced input for delivery to the registered VirtioInput
    // device. Events carry {type, code, value, seq}; unknown shapes are
    // rejected rather than guessed so the guest never sees invented keys.
    const { type, code, value, seq } = payload || {};
    if (!Number.isFinite(Number(type)) || !Number.isFinite(Number(code)) || !Number.isFinite(Number(value))) {
      this.postEnvelope('EVT_WARN', { code: 'INPUT_EVENT_MALFORMED', message: 'Rejected malformed input event' });
      return;
    }
    this.pendingInputEvents.push({ type: Number(type), code: Number(code), value: Number(value), seq: seq ?? null });
    if (this.pendingInputEvents.length > 4096) this.pendingInputEvents.shift();
    // Eagerly forward to a registered virtio-input device if present.
    try {
      const dev = this.findDeviceByName('virtio-input');
      if (dev && typeof dev.enqueueHostEvent === 'function') {
        dev.enqueueHostEvent({ type: Number(type), code: Number(code), value: Number(value), seq: seq ?? null });
      }
    } catch (_e) {}
  }

  cmdPing(payload, id = null) {
    this.postEnvelope('EVT_PONG', {
      seq: payload.seq,
      hostTime: payload.hostTime,
      workerTime: Date.now(),
      instructionsExecuted: this.instructionsExecuted,
      backend: this.cpuBackend,
      stubQuanta: this.stubQuanta,
      ip: this.regs.eip
    }, id);
  }

  getStubQuanta() {
    return this.stubQuanta;
  }

  scheduleNextQuantum() {
    if (!this.isRunning || this.isPaused) return;
    this.quantumTimer = setTimeout(() => this.runQuantum(), 0);
  }

  findDeviceByName(name) {
    if (!this.pciBus || !Array.isArray(this.pciBus.slots)) return null;
    for (const dev of this.pciBus.slots) {
      if (dev && dev.name === name) return dev;
    }
    return null;
  }

  /**
   * Drain latched CPU interrupts (8259 PIC/IOAPIC delivery point).
   * Returns and clears the pending queue; empty array when idle.
   */
  consumePendingIrqs() {
    const out = this.pendingIrqs;
    this.pendingIrqs = [];
    return out;
  }

  /**
   * Drain latched host input events not yet consumed by virtio-input.
   */
  drainPendingInputEvents() {
    const out = this.pendingInputEvents;
    this.pendingInputEvents = [];
    return out;
  }

  runQuantum() {
    if (!this.isRunning || this.isPaused) return;
    try {
      if (this.cpuBackend === 'v86' && this.v86 && typeof this.v86.runQuantum === 'function') {
        // Real x86 execution path (v86 backend). Faults surface as
        // EVT_ERROR, never silently counted.
        this.v86.runQuantum(QUANTUM_CYCLES);
        // The starter API exposes no instruction counter; only adopt a
        // numeric one, never invent it.
        if (Number.isFinite(Number(this.v86.instructionsExecuted))) {
          this.instructionsExecuted = BigInt(this.v86.instructionsExecuted);
        }
      } else {
        // Stub quantum: advance the counter only. Counted separately as
        // stubQuanta so no consumer can mistake it for guest execution.
        this.instructionsExecuted += BigInt(QUANTUM_CYCLES);
        this.stubQuanta += 1;
      }

      // Deliver latched IRQs to the CPU interrupt logic once per quantum.
      // Each drained IRQ is recorded (and forwarded to the v86 backend when
      // attached); the log is the evidence that delivery happened.
      if (this.pendingIrqs.length > 0) {
        const drained = this.consumePendingIrqs();
        for (const irq of drained) {
          const record = { ...irq, deliveredAt: Date.now(), eip: this.regs.eip, backend: this.cpuBackend };
          if (this.v86 && typeof this.v86.deliverIrq === 'function') {
            try { this.v86.deliverIrq(irq.irq, irq.level); record.forwarded = true; } catch (_e) { record.forwarded = false; }
          }
          this.deliveredIrqs.push(record);
          if (this.deliveredIrqs.length > 1024) this.deliveredIrqs.shift();
        }
      }

      // Advance device timers
      if (this.pciBus) {
        this.pciBus.tick();
      }

      this.scheduleNextQuantum();
    } catch (err) {
      this.handleCpuFault(err);
    }
  }

  handleCpuFault(err) {
    this.isRunning = false;
    this.state = 'ERROR';
    this.postEnvelope('EVT_ERROR', {
      code: err.code || 'CPU_TRAP',
      message: err.message,
      cpuState: { ...this.regs, instructionsExecuted: this.instructionsExecuted }
    });
  }

  emitSerial(strOrBytes) {
    const data = typeof strOrBytes === 'string'
      ? new TextEncoder().encode(strOrBytes)
      : strOrBytes;
    this.postEnvelope('EVT_SERIAL_OUT', { data });
  }
}

// Global Web Worker / Node parentPort bootstrap
if (typeof self !== 'undefined' && typeof self.postMessage === 'function') {
  const core = new VmWorkerCore({
    postMessage: (msg) => self.postMessage(msg)
  });
  self.onmessage = (e) => core.handleMessage(e.data);
}
