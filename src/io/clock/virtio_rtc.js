/**
 * virtio_rtc.js - Entity E10 (part): wall + monotonic clock.
 * RTC distinguishes wall time from monotonic time; visibility changes never
 * warp guest time. Reset closes handles (none held here).
 */

export class GuestClock {
  constructor() {
    this.bootMonoMs = this.monoMs();
    this.suspendedAccumMs = 0;
    this.suspended = false;
    this.suspendStartedAt = 0;
  }

  monoMs() {
    if (typeof performance !== 'undefined' && typeof performance.now === 'function') return performance.now();
    return Date.now();
  }

  wallMs() {
    return Date.now();
  }

  guestMonoMs() {
    if (this.suspended) return this.suspendStartedAt - this.bootMonoMs - this.suspendedAccumMs;
    return this.monoMs() - this.bootMonoMs - this.suspendedAccumMs;
  }

  nowNs() {
    return BigInt(Math.round(this.guestMonoMs() * 1e6));
  }

  onVisibility(hidden) {
    // Pause monotonic accumulation while hidden so guest time does not jump.
    if (hidden && !this.suspended) {
      this.suspended = true;
      this.suspendStartedAt = this.monoMs();
    } else if (!hidden && this.suspended) {
      this.suspendedAccumMs += this.monoMs() - this.suspendStartedAt;
      this.suspended = false;
    }
  }

  reset() {
    this.bootMonoMs = this.monoMs();
    this.suspendedAccumMs = 0;
    this.suspended = false;
  }
}
