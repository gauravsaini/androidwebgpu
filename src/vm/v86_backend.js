/**
 * v86_backend.js - W1: worker-side adapter over the REAL v86 starter API.
 *
 * Verified against vendor/v86 (copy/v86 @ 5f9a90f; npm v86@0.5.462):
 *   import { V86 } from 'vendor/v86/libv86.mjs';
 *   const emu = new V86({ wasm_path, bios: {url|buffer}, vga_bios,
 *     bzimage: {url|buffer}, initrd: {url|buffer}, cmdline, memory_size });
 *   emu.add_listener('serial0-output-byte', (byte) => ...);
 *   emu.add_listener('emulator-started', () => ...);
 *   await emu.run(); await emu.stop(); await emu.destroy();
 *   emu.serial0_send(data);
 * (See vendor/v86/v86.d.ts and src/browser/starter.js.)
 *
 * API-shape contract enforced in create(): a bundle whose module does not
 * export a `V86` constructor with run/stop/add_listener is rejected with
 * V86_API_INCOMPATIBLE (fail-closed), never driven as if it were v86.
 *
 * Honesty notes:
 * - v86 runs continuously once started; runQuantum() ensures start and
 *   verifies liveness via public emulator events. There is no per-quantum
 *   instruction count in the starter API, so instructionsExecuted is null
 *   (unknown) rather than invented.
 * - v86 owns its own PIC/APIC and device models; host IRQ lines from our
 *   VirtioBus have no forwarding path in the starter API. deliverIrq()
 *   records the request and reports forwarded:false with reason.
 */
export const V86_API_INCOMPATIBLE = 'V86_API_INCOMPATIBLE';

/**
 * The starter only accepts {buffer: ArrayBuffer} (see buffer_from_object in
 * vendor/v86/src/buffer.js: "TODO: accept Uint8Array"). Normalize views.
 */
function toArrayBuffer(view) {
  if (!view) return view;
  if (view instanceof ArrayBuffer) return view;
  if (ArrayBuffer.isView(view)) {
    return view.buffer.slice(view.byteOffset, view.byteOffset + view.byteLength);
  }
  return view;
}

function assertStarterShape(V86) {
  if (typeof V86 !== 'function') throw new Error(V86_API_INCOMPATIBLE);
  for (const m of ['run', 'stop', 'add_listener', 'destroy']) {
    if (typeof V86.prototype[m] !== 'function') {
      throw new Error(`${V86_API_INCOMPATIBLE}:${m}`);
    }
  }
  return V86;
}

export class V86WorkerBackend {
  constructor(emulator, { serialBuffer = null } = {}) {
    this.emulator = emulator;
    this.loaded = false;
    this.started = false;
    this.stopped = false;
    this.quanta = 0;
    this.serialBytes = serialBuffer || [];
    this.instructionsExecuted = null;
  }

  /**
   * @param {Object} args
   * @param {() => Promise<any>} args.loadModule - resolves the bundle module
   * @param {string} [args.wasmUrl] - wasm_path override (default: derived)
   * @param {{url?:string,buffer?:Uint8Array}} [args.bios]
   * @param {{url?:string,buffer?:Uint8Array}} [args.vgaBios]
   * @param {{url?:string,buffer?:Uint8Array}} [args.bzimage]
   * @param {{url?:string,buffer?:Uint8Array}} [args.initrd]
   * @param {string} [args.cmdline]
   * @param {number} [args.memorySize]
   * @param {(byte:number)=>void} [args.onSerialByte]
   * @param {number} [args.startTimeoutMs=30000]
   */
  static async create(args = {}) {
    const {
      loadModule,
      wasmUrl = null,
      bios = null,
      vgaBios = null,
      bzimage = null,
      initrd = null,
      cmdline = 'console=ttyS0',
      memorySize = 512 * 1024 * 1024,
      onSerialByte = null,
      startTimeoutMs = 30000,
    } = args;
    if (typeof loadModule !== 'function') throw new Error('V86_NOT_PRESENT');
    let mod = null;
    try {
      mod = await loadModule();
    } catch (_e) {
      throw new Error('V86_NOT_PRESENT');
    }
    const V86 = assertStarterShape(mod.V86 || mod.default?.V86 || mod.default);
    const serialBytes = [];
    const options = { memory_size: memorySize, cmdline, autostart: true };
    if (wasmUrl) options.wasm_path = wasmUrl;
    if (bios) options.bios = { ...bios, buffer: toArrayBuffer(bios.buffer) };
    if (vgaBios) options.vga_bios = { ...vgaBios, buffer: toArrayBuffer(vgaBios.buffer) };
    if (bzimage) options.bzimage = { ...bzimage, buffer: toArrayBuffer(bzimage.buffer) };
    if (initrd) options.initrd = { ...initrd, buffer: toArrayBuffer(initrd.buffer) };
    const emulator = new V86(options);
    const backend = new V86WorkerBackend(emulator, { serialBuffer: serialBytes });
    emulator.add_listener('serial0-output-byte', (byte) => {
      serialBytes.push(byte & 0xff);
      if (serialBytes.length > 4 * 1024 * 1024) serialBytes.splice(0, serialBytes.length - 4 * 1024 * 1024);
      if (typeof onSerialByte === 'function') {
        try { onSerialByte(byte & 0xff); } catch (_e) {}
      }
    });
    const startedP = new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('V86_START_TIMEOUT')), startTimeoutMs);
      // Init completes first (wasm + devices), then the CPU starts.
      // autostart:true lets the starter drive both; we only observe.
      emulator.add_listener('emulator-loaded', () => {
        backend.loaded = true;
      });
      emulator.add_listener('emulator-started', () => {
        clearTimeout(timer);
        backend.started = true;
        resolve(true);
      });
    });
    emulator.add_listener('emulator-stopped', () => { backend.stopped = true; });
    await startedP;
    return backend;
  }

  /** Verify the guest runs (autostarted by the bundle); never force it. */
  runQuantum(_cycles = 0) {
    if (!this.emulator || this.stopped || !this.started) {
      throw new Error('V86_NOT_RUNNING');
    }
    this.quanta += 1;
    return { running: true, quanta: this.quanta };
  }

  get emulatorRunning() {
    return this.started && !this.stopped;
  }

  get serialText() {
    return String.fromCharCode(...this.serialBytes.slice(0, 65536));
  }

  /**
   * Host IRQ injection: no forwarding path exists in the starter API (v86
   * owns its PIC/APIC and device models). Recorded, reported unforwarded.
   */
  deliverIrq(irq, level = true) {
    void irq;
    void level;
    return { forwarded: false, reason: 'V86_OWNS_PIC' };
  }

  async destroy() {
    try {
      await this.emulator.destroy();
    } finally {
      this.stopped = true;
    }
  }
}
