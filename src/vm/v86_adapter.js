/**
 * v86_adapter.js - W1: v86 x86 VM integration adapter (fail-closed).
 * Pinned backend contract:
 *   source: copy/v86 @ 5f9a90f2be01243dd0ea4fe014cce12686cf3ced
 *   build:  npm v86@0.5.462 (libv86.mjs exports {V86, CPU}, v86.wasm)
 *   workerModel: dedicated-worker; virtioMode: modern (guest drivers)
 * probe() detects built outputs; loadV86() validates the {V86} export shape;
 * start() builds a V86WorkerBackend (real boot assets) and attaches it to a
 * VmWorkerCore. Absent/incompatible bundle -> V86_NOT_PRESENT or
 * V86_API_INCOMPATIBLE; the validation loop reports BLOCKED.
 */
import { V86WorkerBackend } from './v86_backend.js';

export const V86_PIN = Object.freeze({
  fork: 'copy/v86',
  commit: '5f9a90f2be01243dd0ea4fe014cce12686cf3ced',
  tarballSha256: '05723b91af25922cb8c0bd6ab49816fd4f1e690047c598bacebf81936c61b277',
  npmPackage: 'v86',
  npmVersion: '0.5.462',
  npmShasum: '6da4d03be3aeaa37c6cfca94b501990bf92c2f20',
  workerModel: 'dedicated-worker',
  virtioMode: 'modern',
  wasm: 'vendor/v86/v86.wasm',
  js: 'vendor/v86/libv86.mjs',
  bios: 'vendor/v86/bios.bin',
});

export class V86Adapter {
  constructor({ bundleUrl = null, wasmUrl = null } = {}) {
    this.bundleUrl = bundleUrl || V86_PIN.js;
    this.wasmUrl = wasmUrl || V86_PIN.wasm;
    this.present = false;
    this.bundlePath = null;
    this.backend = null;
  }

  /** Probe for built v86 outputs without executing guest code. */
  async probe({ fetchImpl = null } = {}) {
    if (typeof window !== 'undefined' && window.__V86__present === true) {
      this.present = true;
      this.bundlePath = 'window.__V86__';
      return { present: true, path: this.bundlePath, pin: V86_PIN };
    }
    try {
      const fs = await import('node:fs');
      for (const c of [V86_PIN.js, V86_PIN.wasm, V86_PIN.bios]) {
        try {
          fs.accessSync(new URL(`../../${c}`, import.meta.url));
        } catch (_e) {
          this.present = false;
          this.bundlePath = null;
          return { present: false, error: 'V86_NOT_PRESENT', pin: V86_PIN };
        }
      }
      this.present = true;
      this.bundlePath = V86_PIN.js;
      return { present: true, path: this.bundlePath, pin: V86_PIN };
    } catch (_e) {}
    if (fetchImpl && this.bundleUrl) {
      try {
        const res = await fetchImpl(this.bundleUrl, { method: 'HEAD' });
        this.present = !!res.ok;
        if (this.present) this.bundlePath = this.bundleUrl;
        return { present: this.present, path: this.bundlePath, pin: V86_PIN };
      } catch (_e) {}
    }
    this.present = false;
    this.bundlePath = null;
    return { present: false, error: 'V86_NOT_PRESENT', pin: V86_PIN };
  }

  /**
   * Really load the bundle and validate the {V86} export shape.
   * Resolves {V86}; rejects V86_NOT_PRESENT / V86_API_INCOMPATIBLE.
   */
  async loadV86() {
    if (typeof window !== 'undefined' && window.__V86__ && typeof window.__V86__.V86 === 'function') {
      return { V86: window.__V86__.V86 };
    }
    let mod = null;
    try {
      mod = await import(`../../${V86_PIN.js}`);
    } catch (_e) {
      throw new Error('V86_NOT_PRESENT');
    }
    const V86 = mod.V86 || mod.default?.V86 || mod.default;
    if (typeof V86 !== 'function' || typeof V86.prototype.run !== 'function' || typeof V86.prototype.add_listener !== 'function') {
      throw new Error('V86_API_INCOMPATIBLE');
    }
    return { V86 };
  }

  /**
   * Build a live backend (bios + kernel + initrd) and attach it to a
   * VmWorkerCore so quanta execute real x86 instead of the stub counter.
   * @param {any} workerCore - VmWorkerCore with attachV86Backend()
   * @param {Object} boot - {bios, vgaBios, bzimage, initrd, cmdline, memorySize, onSerialByte, startTimeoutMs}
   */
  async start(workerCore = null, boot = {}) {
    if (!workerCore || typeof workerCore.attachV86Backend !== 'function') {
      throw new Error('V86_ATTACH_INVALID');
    }
    const { V86 } = await this.loadV86();
    const backend = await V86WorkerBackend.create({
      loadModule: async () => ({ V86 }),
      wasmUrl: this.wasmUrl,
      bios: boot.bios || { url: V86_PIN.bios },
      vgaBios: boot.vgaBios || null,
      bzimage: boot.bzimage || null,
      initrd: boot.initrd || null,
      cmdline: boot.cmdline || 'console=ttyS0',
      memorySize: boot.memorySize || 512 * 1024 * 1024,
      onSerialByte: boot.onSerialByte || null,
      startTimeoutMs: boot.startTimeoutMs || 30000,
    });
    workerCore.attachV86Backend(backend);
    this.backend = backend;
    return { started: true, pin: V86_PIN, backend: true };
  }
}
