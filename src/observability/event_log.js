/**
 * event_log.js - Entity E18: bounded structured event ring + human log.
 * Every entry: {epoch, seq, ts, src, kind, payload}. Bounded; wraps.
 */

export class EventLog {
  constructor(capacity = 4096) {
    this.capacity = capacity;
    this.events = [];
    this.seq = 0;
  }

  emit({ epoch = 0, src = 'host', kind = 'info', payload = {} } = {}) {
    const entry = Object.freeze({
      epoch: Number(epoch) || 0,
      seq: this.seq++,
      ts: Date.now(),
      src: String(src),
      kind: String(kind),
      payload: Object.freeze({ ...payload }),
    });
    this.events.push(entry);
    if (this.events.length > this.capacity) {
      this.events.splice(0, this.events.length - this.capacity);
    }
    return entry;
  }

  snapshot() {
    return this.events.slice();
  }

  exportJson() {
    return JSON.stringify({ exportedAt: new Date().toISOString(), events: this.events });
  }

  clear() {
    this.events = [];
  }
}
