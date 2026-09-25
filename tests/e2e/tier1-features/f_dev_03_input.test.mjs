import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioInput from src/.
import { VirtioInput, INPUT_KEYMAP_VERSION, EV_KEY } from '../../../src/io/input/virtio_input.js';

describe('Tier 1: F-DEV-03 VirtioInput DOM to evdev Translation (production)', () => {
  test('F-DEV-03-01: explicit versioned DOM->evdev mapping; unknown keys reported not guessed', () => {
    assertEqual(INPUT_KEYMAP_VERSION, 'evdev-1.0');
    const dev = new VirtioInput();
    assertEqual(dev.mapDomKey('KeyA'), 30);
    assertEqual(dev.mapDomKey('NoSuchKey'), null);
    assertOk(dev.unknownKeys.includes('NoSuchKey'));
  });

  test('F-DEV-03-02: key down/up events carry type/code/value with sequence', () => {
    const dev = new VirtioInput();
    const down = dev.handleDomKey('Enter', true);
    assertEqual(down.type, EV_KEY);
    assertEqual(down.value, 1);
    const up = dev.handleDomKey('Enter', false);
    assertEqual(up.value, 0);
    assertOk(up.seq > down.seq);
  });

  test('F-DEV-03-03: focus loss sends key-up for all pressed keys', () => {
    const dev = new VirtioInput();
    dev.handleDomKey('KeyA', true);
    dev.handleDomKey('KeyB', true);
    assertEqual(dev.pressed.size, 2);
    const ups = dev.handleFocusLoss();
    assertEqual(ups.length, 2);
    assertOk(ups.every((e) => e.value === 0));
    assertEqual(dev.pressed.size, 0);
  });

  test('F-DEV-03-04: pointer maps CSS pixels to guest display pixels', () => {
    const dev = new VirtioInput({ displayWidth: 640, displayHeight: 480 });
    dev.handleDomPointer(320, 240, 640, 480);
    const last = dev.pendingHost[dev.pendingHost.length - 1];
    assertEqual(last.value, 240);
  });

  test('F-DEV-03-05: host event queue is bounded', () => {
    const dev = new VirtioInput();
    for (let i = 0; i < 5000; i++) dev.enqueueHostEvent({ type: 1, code: 30, value: 1 });
    assertOk(dev.pendingHost.length <= 4096);
  });
});
