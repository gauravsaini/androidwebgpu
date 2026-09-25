import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real GuestClock from src/.
import { GuestClock } from '../../../src/io/clock/virtio_rtc.js';

describe('Tier 1: F-DEV-06 CMOS RTC & Monotonic Clock (production)', () => {
  test('F-DEV-06-01: wall time tracks epoch milliseconds', () => {
    const clock = new GuestClock();
    const before = Date.now();
    const wall = clock.wallMs();
    assertOk(wall >= before && wall <= Date.now());
  });

  test('F-DEV-06-02: monotonic clock advances forward only', () => {
    const clock = new GuestClock();
    const a = clock.guestMonoMs();
    const b = clock.guestMonoMs();
    assertOk(b >= a);
  });

  test('F-DEV-06-03: nowNs returns nanosecond-scale bigint', () => {
    const clock = new GuestClock();
    const ns = clock.nowNs();
    assertEqual(typeof ns, 'bigint');
    assertOk(ns > 0n);
  });

  test('F-DEV-06-04: visibility hide/show does not warp guest time', () => {
    const clock = new GuestClock();
    clock.onVisibility(true);
    const frozen = clock.guestMonoMs();
    clock.onVisibility(false);
    const after = clock.guestMonoMs();
    assertOk(after >= 0 && after < frozen + 5000);
  });

  test('F-DEV-06-05: reset restarts the monotonic base', () => {
    const clock = new GuestClock();
    clock.reset();
    assertOk(clock.guestMonoMs() >= 0);
    assertOk(clock.guestMonoMs() < 5000);
  });
});
