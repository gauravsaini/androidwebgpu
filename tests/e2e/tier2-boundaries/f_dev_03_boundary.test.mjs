import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioInput from src/.
import { VirtioInput, EV_KEY } from '../../../src/io/input/virtio_input.js';

describe('Tier 2: F-DEV-03 Boundary & Corner Cases (production)', () => {
  test('F-DEV-03-B01: negative coordinates clamp into guest space without crash', () => {
    const dev = new VirtioInput({ displayWidth: 640, displayHeight: 480 });
    dev.handleDomPointer(-50, -20, 640, 480);
    assertOk(dev.pendingHost.length > 0);
  });

  test('F-DEV-03-B02: unknown key codes are reported, never guessed', () => {
    const dev = new VirtioInput();
    const res = dev.handleDomKey('Frobnicator', true);
    assertEqual(res.guessed, false);
    assertEqual(res.reported, 'Frobnicator');
    assertEqual(dev.pendingHost.length, 0);
  });

  test('F-DEV-03-B03: duplicate key-down does not corrupt pressed set', () => {
    const dev = new VirtioInput();
    dev.handleDomKey('KeyA', true);
    dev.handleDomKey('KeyA', true);
    assertEqual(dev.pressed.size, 1);
    const ups = dev.handleFocusLoss();
    assertEqual(ups.length, 1);
    assertEqual(ups[0].value, 0);
  });

  test('F-DEV-03-B04: key-up without prior key-down is still delivered', () => {
    const dev = new VirtioInput();
    const ev = dev.handleDomKey('KeyZ', false);
    assertEqual(ev.type, EV_KEY);
    assertEqual(ev.value, 0);
  });

  test('F-DEV-03-B05: zero-size viewport mapping cannot divide by zero', () => {
    const dev = new VirtioInput({ displayWidth: 640, displayHeight: 480 });
    dev.handleDomPointer(10, 10, 0, 0);
    assertOk(dev.pendingHost.length > 0);
  });
});
