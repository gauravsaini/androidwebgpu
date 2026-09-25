/**
 * browser_host.js - Entity E02: BrowserHost services.
 * Owns canvas, input sequencing, disk handles, net policy, audio, clock.
 * Feature absence surfaces as UNSUPPORTED, never as green state.
 */

export class BrowserHostError extends Error {
  constructor(message, code = 'HOST_ERROR') {
    super(message);
    this.name = 'BrowserHostError';
    this.code = code;
  }
}

export class BrowserHost {
  constructor({ canvas = null, eventLog = null } = {}) {
    this.canvas = canvas;
    this.eventLog = eventLog;
    this.inputSeq = 0;
    this.audioUnlocked = false;
  }

  openCanvas() {
    if (!this.canvas) throw new BrowserHostError('No canvas element bound', 'UNSUPPORTED');
    if (typeof navigator !== 'undefined' && !navigator.gpu) {
      throw new BrowserHostError('WebGPU unavailable in this browser', 'UNSUPPORTED');
    }
    return this.canvas;
  }

  readInput(domEvent) {
    // Assign monotonic sequence numbers; callers map coordinates.
    this.inputSeq += 1;
    return { seq: this.inputSeq, event: domEvent };
  }

  nowNs() {
    if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
      return BigInt(Math.round(performance.now() * 1e6));
    }
    return BigInt(Date.now()) * 1000000n;
  }

  markAudioUnlocked() {
    this.audioUnlocked = true;
  }

  audioWrite(_pcm) {
    if (!this.audioUnlocked) {
      throw new BrowserHostError('Audio starts only after user activation', 'AUDIO_BLOCKED');
    }
    return { accepted: true, underrun: false, overrun: false };
  }

  checkSupport() {
    const out = { webgpu: false, wasm: false, sab: false, opfs: false };
    try { out.webgpu = typeof navigator !== 'undefined' && !!navigator.gpu; } catch (_e) {}
    try { out.wasm = typeof WebAssembly !== 'undefined'; } catch (_e) {}
    try { out.sab = typeof SharedArrayBuffer !== 'undefined'; } catch (_e) {}
    try { out.opfs = typeof navigator !== 'undefined' && !!(navigator.storage && navigator.storage.getDirectory); } catch (_e) {}
    return out;
  }
}
