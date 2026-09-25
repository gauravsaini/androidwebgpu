import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioSound policy from src/.
import { VirtioSound } from '../../../src/io/audio/virtio_sound.js';

describe('Tier 1: F-DEV-07 VirtioSound Audio Output Device (production)', () => {
  test('F-DEV-07-01: writes blocked until user activation unlock', () => {
    const dev = new VirtioSound();
    const res = dev.audioWrite(new Uint8Array(256));
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'AUDIO_BLOCKED');
  });

  test('F-DEV-07-02: unlock admits non-empty PCM without underrun', () => {
    const dev = new VirtioSound();
    dev.unlock();
    const res = dev.audioWrite(new Uint8Array(256));
    assertEqual(res.accepted, true);
    assertEqual(res.underrun, false);
    assertEqual(dev.framesAccepted, 1);
  });

  test('F-DEV-07-03: zero-length write counts an underrun', () => {
    const dev = new VirtioSound();
    dev.unlock();
    const res = dev.audioWrite(new Uint8Array(0));
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'AUDIO_UNDERRUN');
    assertEqual(dev.underruns, 1);
  });

  test('F-DEV-07-04: muted device reports mute instead of accepting', () => {
    const dev = new VirtioSound({ muted: true });
    dev.unlock();
    const res = dev.audioWrite(new Uint8Array(64));
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'AUDIO_MUTED');
  });

  test('F-DEV-07-05: reset re-locks the device', () => {
    const dev = new VirtioSound();
    dev.unlock();
    dev.reset();
    assertEqual(dev.audioWrite(new Uint8Array(64)).error, 'AUDIO_BLOCKED');
    assertEqual(dev.name, 'virtio-snd');
  });
});
