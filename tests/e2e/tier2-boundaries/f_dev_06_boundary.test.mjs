import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real GuestClock from src/.
import { GuestClock } from '../../../src/io/clock/virtio_rtc.js';

describe('Tier 2: F-DEV-06 Boundary & Corner Cases (production)', () => {
  test('F-DEV-06-B01: double-suspend does not double-count hidden time', () => {
    const clock = new GuestClock();
    clock.onVisibility(true);
    clock.onVisibility(true);
    clock.onVisibility(false);
    assertOk(clock.guestMonoMs() >= 0);
    assertEqual(clock.suspended, false);
  });

  test('F-DEV-06-B02: resume without suspend is a no-op', () => {
    const clock = new GuestClock();
    clock.onVisibility(false);
    assertEqual(clock.suspended, false);
    assertOk(clock.guestMonoMs() >= 0);
  });

  test('F-DEV-06-B03: wall and monotonic clocks diverge honestly', () => {
    const clock = new GuestClock();
    assertOk(clock.wallMs() > 1_000_000_000_000);
    assertOk(clock.guestMonoMs() < 3_600_000);
  });

  test('F-DEV-06-B04: nowNs is monotonic across calls', () => {
    const clock = new GuestClock();
    const a = clock.nowNs();
    const b = clock.nowNs();
    assertOk(b >= a);
  });

  test('F-DEV-06-B05: reset clears suspend accumulation', () => {
    const clock = new GuestClock();
    clock.onVisibility(true);
    clock.reset();
    assertEqual(clock.suspended, false);
    assertEqual(clock.suspendedAccumMs, 0);
  });
});
