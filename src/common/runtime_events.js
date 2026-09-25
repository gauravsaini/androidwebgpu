/**
 * runtime_events.js - W0 frozen contract: RuntimeEvent schema (E18).
 * Every event has epoch, seq, ts, src, kind, and a typed payload.
 */

export const RUNTIME_EVENT_KINDS = Object.freeze([
  'asset_check',
  'vm_create',
  'boot_start',
  'kernel_seen',
  'android_ready',
  'frame_presented',
  'input_delivered',
  'disk_commit',
  'net_ready',
  'fatal_error',
]);

export function makeRuntimeEvent({ epoch, seq, src, kind, payload = {}, ts = Date.now() }) {
  if (!Number.isInteger(epoch) || epoch < 0) throw new Error('INVALID_EVENT_EPOCH');
  if (!Number.isInteger(seq) || seq < 0) throw new Error('INVALID_EVENT_SEQ');
  if (typeof src !== 'string' || !src) throw new Error('INVALID_EVENT_SRC');
  if (typeof kind !== 'string' || !kind) throw new Error('INVALID_EVENT_KIND');
  return Object.freeze({ epoch, seq, ts: Number(ts), src, kind, payload: Object.freeze({ ...payload }) });
}

export function validateRuntimeEvent(event) {
  if (!event || typeof event !== 'object') throw new Error('INVALID_RUNTIME_EVENT');
  makeRuntimeEvent(event);
  return true;
}
