/**
 * vm_runtime.js - Entity E04: x86 VM Runtime Controller
 * Main thread orchestrator managing VM lifecycle (create, loadBoot, start, pause, resume, reset, destroy),
 * GuestMem physical address allocation, serial log streaming, IRQ dispatch, and watchdog liveness monitoring.
 */

import { GuestMem } from './guest_mem.js';
import { BootManager } from '../boot/boot_manager.js';
import { VmWorkerCore } from './vm_worker.js';

export class VmError extends Error {
  constructor(message, code = 'VM_ERROR') {
    super(`${message} [${code}]`);
    this.name = 'VmError';
    this.code = code;
  }
}

export class VmStateError extends VmError {
  constructor(message) {
    super(message, 'INVALID_VM_STATE');
    this.name = 'VmStateError';
  }
}

export class VmWatchdogTimeoutError extends VmError {
  constructor(message = 'VM worker watchdog heartbeat timed out') {
    super(message, 'WATCHDOG_TIMEOUT');
    this.name = 'VmWatchdogTimeoutError';
  }
}

export class VmWatchdog {
  /**
   * @param {VmRuntime} runtime
   * @param {number} [intervalMs=1000]
   * @param {number} [timeoutMs=5000]
   */
  constructor(runtime, intervalMs = 1000, timeoutMs = 5000) {
    this.runtime = runtime;
    this.intervalMs = intervalMs;
    this.timeoutMs = timeoutMs;
    this.seq = 0;
    this.ackedSeq = 0;
    this.lastPongTime = Date.now();
    this.timerId = null;
    this.isActive = false;
  }

  start() {
    this.lastPongTime = Date.now();
    this.isActive = true;
    if (this.timerId) clearInterval(this.timerId);
    this.timerId = setInterval(() => this.tick(), this.intervalMs);
    if (this.timerId && typeof this.timerId.unref === 'function') {
      this.timerId.unref();
    }
  }

  tick() {
    if (!this.isActive) return;
    const now = Date.now();
    if (now - this.lastPongTime >= this.timeoutMs) {
      this.stop();
      this.runtime.handleWatchdogTimeout();
      return;
    }
    this.seq++;
    this.runtime.sendWorkerMessage('CMD_PING', { seq: this.seq, hostTime: now });
  }

  handlePong(seq) {
    // Monotonic ack: any pong for an outstanding ping (0 < seq <= issued)
    // proves liveness, including delayed pongs that arrive after a newer
    // ping was issued. Strict seq===issued equality caused false-positive
    // termination under normal worker scheduling jitter.
    const s = Number(seq);
    if (Number.isInteger(s) && s > this.ackedSeq && s <= this.seq) {
      this.ackedSeq = s;
      this.lastPongTime = Date.now();
    }
  }

  pause() {
    this.isActive = false;
  }

  resume() {
    this.lastPongTime = Date.now();
    this.isActive = true;
  }

  stop() {
    this.isActive = false;
    if (this.timerId) {
      clearInterval(this.timerId);
      this.timerId = null;
    }
  }
}

export class VmRuntime {
  /**
   * @param {Object} [options]
   * @param {number} [options.heartbeatIntervalMs=1000]
   * @param {number} [options.watchdogTimeoutMs=5000]
   * @param {number} [options.maxSerialBufferSize=10485760] // 10 MB buffer
   * @param {boolean} [options.allowStub=false] - Explicit opt-in to the
   *   non-executing stub quantum (unit tests only). Production boot requires
   *   an attached v86 backend; start() rejects with V86_NOT_PRESENT otherwise.
   */
  constructor(options = {}) {
    this.options = {
      heartbeatIntervalMs: 1000,
      watchdogTimeoutMs: 5000,
      maxSerialBufferSize: 10 * 1024 * 1024,
      allowStub: false,
      ...options
    };
    this.allowStub = this.options.allowStub === true;
    /** @type {boolean} True once a real v86 backend is attached. */
    this.v86Attached = false;

    /** @type {'UNINITIALIZED'|'READY'|'CONFIGURED'|'RUNNING'|'PAUSED'|'STOPPED'|'ERROR'|'DESTROYED'} */
    this.state = 'UNINITIALIZED';

    /** @type {GuestMem|null} */
    this.guestMem = null;

    /** @type {any} */
    this.worker = null;
    this.isInProcessWorker = false;

    this.watchdog = new VmWatchdog(
      this,
      this.options.heartbeatIntervalMs,
      this.options.watchdogTimeoutMs
    );

    this.serialListeners = new Set();
    this.stateChangeListeners = new Set();
    this.errorListeners = new Set();
    this.pendingResolvers = new Map();
  }

  /**
   * Subscribe to serial log console output.
   * @param {(text: string) => void} listener
   */
  onSerial(listener) {
    this.serialListeners.add(listener);
    return () => this.serialListeners.delete(listener);
  }

  /**
   * Subscribe to state transition events.
   * @param {(transition: {oldState: string, newState: string, reason?: string}) => void} listener
   */
  onStateChange(listener) {
    this.stateChangeListeners.add(listener);
    return () => this.stateChangeListeners.delete(listener);
  }

  /**
   * Subscribe to fatal VM errors.
   * @param {(err: Error) => void} listener
   */
  onError(listener) {
    this.errorListeners.add(listener);
    return () => this.errorListeners.delete(listener);
  }

  transitionState(newState, reason = '') {
    const oldState = this.state;
    this.state = newState;
    for (const listener of this.stateChangeListeners) {
      try {
        listener({ oldState, newState, reason });
      } catch (_e) {}
    }
  }

  isRealWorker() {
    return typeof Worker !== 'undefined' && this.worker instanceof Worker;
  }

  sendWorkerMessage(type, payload = {}, idOrOpts = null) {
    let id = null;
    let transfer = null;
    if (idOrOpts && typeof idOrOpts === 'object') {
      id = idOrOpts.id || null;
      transfer = idOrOpts.transfer || null;
    } else {
      id = idOrOpts;
    }
    const msgId = id || `main_${Date.now()}_${Math.random()}`;
    const env = {
      type,
      id: msgId,
      timestamp: Date.now(),
      payload
    };

    if (this.isRealWorker()) {
      // Real Web Worker: SharedArrayBuffer is shared by reference; a plain
      // ArrayBuffer must ride a transfer list so both sides never fork RAM.
      if (transfer) this.worker.postMessage(env, transfer);
      else this.worker.postMessage(env);
    } else if (this.worker && typeof this.worker.postMessage === 'function') {
      // In-process core (or test double): same postMessage surface so tests
      // can intercept/silence traffic exactly like a real worker.
      this.worker.postMessage(env);
    } else if (this.isInProcessWorker && this.worker) {
      setTimeout(() => {
        if (this.worker) this.worker.handleMessage(env);
      }, 0);
    }

    return msgId;
  }

  handleWorkerMessage(msg) {
    if (!msg || typeof msg !== 'object') return;
    const { type, payload = {}, id } = msg;

    if (id && this.pendingResolvers.has(id)) {
      const resolver = this.pendingResolvers.get(id);
      this.pendingResolvers.delete(id);
      resolver(payload);
    }

    switch (type) {
      case 'EVT_STATE_CHANGE':
        if (this.state !== 'ERROR' && payload.newState && payload.newState !== this.state) {
          this.transitionState(payload.newState, payload.reason);
        }
        break;
      case 'EVT_SERIAL_OUT':
        if (payload.data) {
          const str = new TextDecoder().decode(payload.data);
          for (const listener of this.serialListeners) {
            try { listener(str); } catch (_e) {}
          }
        }
        break;
      case 'EVT_PONG':
        if (this.watchdog) {
          this.watchdog.handlePong(payload.seq);
        }
        break;
      case 'EVT_WARN': {
        const err = new VmError(payload.message || 'Worker warning', payload.code || 'WORKER_WARN');
        for (const listener of this.errorListeners) {
          try { listener(err); } catch (_e) {}
        }
        break;
      }
      case 'EVT_ERROR': {
        const err = new VmError(payload.message || 'Worker CPU fault', payload.code);
        this.watchdog.stop();
        this.transitionState('ERROR', payload.code);
        for (const listener of this.errorListeners) {
          try { listener(err); } catch (_e) {}
        }
        break;
      }
      default:
        break;
    }
  }

  handleWatchdogTimeout() {
    this.watchdog.stop();
    const err = new VmWatchdogTimeoutError();
    this.transitionState('ERROR', 'WATCHDOG_TIMEOUT');
    if (this.worker && typeof this.worker.terminate === 'function') {
      this.worker.terminate();
    }
    for (const listener of this.errorListeners) {
      try { listener(err); } catch (_e) {}
    }
  }

  /**
   * Instantiate guest RAM and initialize execution worker.
   * @param {Object} config
   * @param {number} config.ramSize - RAM size in bytes
   * @param {SharedArrayBuffer|ArrayBuffer} [config.sharedBuffer]
   * @param {string} [config.workerUrl]
   * @param {any} [config.workerInstance] - Custom or test worker instance
   */
  async create(config) {
    if (this.state !== 'UNINITIALIZED') {
      throw new VmStateError(`Cannot create VM in state ${this.state}`);
    }

    const { ramSize = 128 * 1024 * 1024, sharedBuffer = null, workerInstance = null, allowStub = null } = config;
    if (allowStub !== null) this.allowStub = allowStub === true;
    this.guestMem = new GuestMem(ramSize, sharedBuffer);
    this.ramShared = this.guestMem.isShared;

    if (workerInstance) {
      this.worker = workerInstance;
      this.isInProcessWorker = false;
      this.worker.onmessage = (e) => this.handleWorkerMessage(e.data);
    } else if (typeof Worker !== 'undefined') {
      try {
        const url = config.workerUrl || new URL('./vm_worker.js', import.meta.url);
        this.worker = new Worker(url, { type: 'module' });
        this.isInProcessWorker = false;
        this.worker.onmessage = (e) => this.handleWorkerMessage(e.data);
      } catch (_e) {
        // Fall back to in-process worker core if web worker instantiation fails
        this.initInProcessWorker();
      }
    } else {
      this.initInProcessWorker();
    }

    // Fail-closed shared-RAM gate: a real Web Worker without
    // SharedArrayBuffer would fork RAM (structured clone) and hide staged
    // boot data. Refuse to go READY in that mode instead of forking.
    if (this.isRealWorker() && !this.guestMem.isShared) {
      if (this.worker && typeof this.worker.terminate === 'function') {
        try { this.worker.terminate(); } catch (_e) {}
      }
      this.worker = null;
      throw new VmError(
        'Guest RAM is not SharedArrayBuffer; real-worker boot would fork memory. Serve with COOP/COEP or use the in-process fallback.',
        'GUEST_RAM_NOT_SHARED'
      );
    }

    await new Promise((resolve) => {
      const initPayload = {
        ramSize: this.guestMem.ramSize,
        ramShared: this.guestMem.isShared,
      };
      let transfer = null;
      if (this.isRealWorker()) {
        // Shared: passed by reference, zero-copy, stays visible both sides.
        // Non-shared is rejected by the gate above; transfer if ever used.
        initPayload.sharedBuffer = this.guestMem.buffer;
        if (!this.guestMem.isShared) transfer = [this.guestMem.buffer];
      } else {
        initPayload.sharedBuffer = null;
      }
      const msgId = this.sendWorkerMessage('CMD_INIT', initPayload, transfer ? { transfer } : null);
      this.pendingResolvers.set(msgId, resolve);
      // Fallback timer if event loop ticks
      setTimeout(resolve, 50);
    });

    // In-process fallback shares the SAME GuestMem instance so boot staging
    // is visible without any copy. Real workers adopt the transferred buffer.
    if (this.isInProcessWorker && this.worker && this.worker.guestMem !== this.guestMem) {
      this.worker.guestMem = this.guestMem;
      this.worker.u8 = this.guestMem.u8;
    }
    if (!this.guestMem.isShared) {
      for (const listener of this.errorListeners) {
        try { listener(new VmError('Guest RAM is not SharedArrayBuffer; worker runs in forked-memory mode', 'GUEST_RAM_NOT_SHARED')); } catch (_e) {}
      }
    }

    this.transitionState('READY');
  }

  initInProcessWorker() {
    this.isInProcessWorker = true;
    this.worker = new VmWorkerCore({
      postMessage: (msg) => setTimeout(() => this.handleWorkerMessage(msg), 0)
    });
  }

  /**
   * Stage boot assets (kernel, initrd, command line) in guest memory via BootManager.
   * @param {Object} bootConfig
   * @param {Uint8Array|ArrayBuffer} bootConfig.kernel
   * @param {Uint8Array|ArrayBuffer|null} [bootConfig.initrd]
   * @param {string} [bootConfig.cmdline]
   */
  async loadBoot(bootConfig) {
    if (this.state !== 'READY' && this.state !== 'CONFIGURED') {
      throw new VmStateError(`Cannot loadBoot in state ${this.state}`);
    }

    const bootManager = new BootManager(this.guestMem);
    const bootResult = bootManager.setupLinuxBoot(bootConfig);

    this.sendWorkerMessage('CMD_LOAD_BOOT', {
      bootParamsAddr: bootResult.bootParamsAddr,
      entryIp: bootResult.entryIp,
      initialRegs: bootResult.registers
    });

    this.transitionState('CONFIGURED');
    return bootResult;
  }

  /**
   * Attach a real v86 backend. In-process path attaches the object directly;
   * real-worker path asks the worker to load the bundle URL itself and awaits
   * its ack (fail-closed: V86_NOT_PRESENT when nothing is vendored).
   * @param {any} backendOrDescriptor - backend object ({runQuantum}) or {bundleUrl}
   * @param {Object} [opts] - {attachTimeoutMs} ack budget for real workers
   *   (bundle fetch + wasm compile + BIOS can take ~2 min; default 2000).
   */
  async attachV86(backendOrDescriptor = null, opts = {}) {
    if (this.state === 'DESTROYED') {
      throw new VmStateError('VM is destroyed; attach rejected');
    }
    if (backendOrDescriptor && typeof backendOrDescriptor.runQuantum === 'function') {
      if (!this.isRealWorker() && this.worker && typeof this.worker.attachV86Backend === 'function') {
        this.worker.attachV86Backend(backendOrDescriptor);
        this.v86Attached = true;
        return { attached: true, path: 'in-process' };
      }
      throw new VmError('Backend objects cannot cross to a real worker; pass {bundleUrl}', 'V86_ATTACH_INVALID');
    }
    const bundleUrl = backendOrDescriptor?.bundleUrl || null;
    if (!bundleUrl) throw new VmError('No v86 bundle available', 'V86_NOT_PRESENT');
    // Forward the full boot descriptor (bundle + bios/bzimage/initrd/urls);
    // the worker validates the {V86} shape and boots real assets.
    const { runQuantum: _ignored, ...descriptor } = backendOrDescriptor;
    const msgId = this.sendWorkerMessage('CMD_ATTACH_V86', { bundleUrl, ...descriptor });
    const attachTimeoutMs = Number(opts.attachTimeoutMs) > 0 ? Number(opts.attachTimeoutMs) : 2000;
    const ack = await new Promise((resolve) => {
      this.pendingResolvers.set(msgId, resolve);
      setTimeout(() => resolve({ attached: false, timeout: true }), attachTimeoutMs);
    });
    if (!ack || ack.attached !== true) {
      throw new VmError('v86 bundle not present in worker', 'V86_NOT_PRESENT');
    }
    this.v86Attached = true;
    return { attached: true, path: 'worker-bundle' };
  }

  /**
   * Begin or resume CPU quantum execution.
   * Fail-closed: without an attached v86 backend the stub counter would run
   * and masquerade as boot. start() rejects with V86_NOT_PRESENT unless the
   * caller explicitly opted into {allowStub:true} (unit tests only) or
   * attached a backend via attachV86(). The worker enforces the same rule.
   * Idempotent only before STOPPED; second start while RUNNING is rejected.
   */
  async start() {
    if (this.state === 'RUNNING') {
      throw new VmStateError('VM is already running');
    }
    if (this.state === 'STOPPED') {
      throw new VmStateError('VM has stopped; reset required before start');
    }
    if (this.state !== 'CONFIGURED' && this.state !== 'PAUSED') {
      throw new VmStateError(`Cannot start VM in state ${this.state}`);
    }
    if (!this.v86Attached && !this.allowStub) {
      throw new VmError(
        'No v86 backend attached; stub quanta cannot boot a guest. Call attachV86() or opt into {allowStub:true} for unit tests.',
        'V86_NOT_PRESENT'
      );
    }

    this.sendWorkerMessage('CMD_START', { requireV86: !this.allowStub });
    this.watchdog.start();
    this.transitionState('RUNNING');
  }

  /**
   * Pause execution and quiesce all pending worker quanta.
   */
  async pause() {
    if (this.state !== 'RUNNING') return;

    this.sendWorkerMessage('CMD_PAUSE');
    this.watchdog.pause();
    this.transitionState('PAUSED');
  }

  /**
   * Resume execution from paused state.
   */
  async resume() {
    if (this.state !== 'PAUSED') return;

    this.sendWorkerMessage('CMD_RESUME');
    this.watchdog.resume();
    this.transitionState('RUNNING');
  }

  /**
   * Reset VM registers, devices, and memory without resource leaks.
   */
  async reset() {
    if (this.state === 'DESTROYED') {
      throw new VmStateError('VM is destroyed; reset rejected (create a new runtime)');
    }
    this.watchdog.stop();
    this.sendWorkerMessage('CMD_RESET');
    this.transitionState('READY', 'RESET');
  }

  /**
   * Completely shut down VM, terminate worker, and clean up timers.
   */
  async destroy() {
    this.watchdog.stop();
    this.sendWorkerMessage('CMD_DESTROY');
    if (this.worker && typeof this.worker.terminate === 'function') {
      this.worker.terminate();
    }
    this.worker = null;
    this.transitionState('DESTROYED');
  }

  /**
   * Inject hardware interrupt into guest.
   * @param {number} irq
   * @param {boolean} [level=true]
   */
  injectIrq(irq, level = true) {
    this.sendWorkerMessage('CMD_INJECT_IRQ', { irq, level });
  }

  /**
   * Register a virtio device with the worker-side PCI bus.
   * In-process path registers directly; real-worker path sends
   * CMD_REGISTER_DEVICE so the identical factory runs inside the worker.
   * @param {{slot:number, kind:string, options?:Object}} desc
   */
  async attachDevice({ slot, kind, options = {} } = {}) {
    if (this.state === 'DESTROYED' || this.state === 'UNINITIALIZED') {
      throw new VmStateError(`Cannot attach device in state ${this.state}`);
    }
    if (this.isInProcessWorker && this.worker && this.worker.pciBus) {
      const { createWorkerDevice } = await import('./vm_worker.js');
      const dev = createWorkerDevice(kind, options);
      if (typeof dev.setGuestMem === 'function') dev.setGuestMem(this.guestMem);
      this.worker.pciBus.registerDevice(slot, dev);
      this.worker.registeredDevices.push({ slot, kind });
      return { slot, kind, path: 'in-process' };
    }
    const msgId = this.sendWorkerMessage('CMD_REGISTER_DEVICE', { slot, kind, options });
    return new Promise((resolve) => {
      this.pendingResolvers.set(msgId, (payload) => resolve(payload || { slot, kind }));
      setTimeout(() => resolve({ slot, kind, timeout: true }), 500);
    });
  }

  /**
   * Send input event to guest.
   * @param {number} type
   * @param {number} code
   * @param {number} value
   */
  sendInput(type, code, value) {
    this.sendWorkerMessage('CMD_INPUT_EVENT', { type, code, value });
  }
}
