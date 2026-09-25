/**
 * Event Bus & State Transition Validator
 * Tracks and validates runtime state transitions and required event sequences.
 */

import { validateRuntimeEvent } from './runtime_mock.mjs';

export const VM_STATES = [
  'NEW',
  'ASSET_CHK',
  'VM_RDY',
  'BOOTING',
  'KERN_RDY',
  'AND_RDY',
  'RUN',
  'PAUSE',
  'STOPPED',
  'FAIL'
];

export const REQUIRED_EVENT_KINDS = [
  'asset_check',
  'vm_create',
  'boot_start',
  'kernel_seen',
  'android_ready',
  'frame_presented',
  'input_delivered',
  'disk_commit',
  'net_ready',
  'fatal_error'
];

export class EventBusSpy {
  constructor() {
    this.events = [];
    this.listeners = new Map();
  }

  emit(event) {
    validateRuntimeEvent(event);
    this.events.push(event);
    const list = this.listeners.get(event.kind) || [];
    for (const fn of list) {
      fn(event);
    }
  }

  on(kind, fn) {
    if (!this.listeners.has(kind)) {
      this.listeners.set(kind, []);
    }
    this.listeners.get(kind).push(fn);
  }

  getEventsByKind(kind) {
    return this.events.filter(e => e.kind === kind);
  }

  hasSeen(kind) {
    return this.events.some(e => e.kind === kind);
  }

  clear() {
    this.events = [];
  }
}
