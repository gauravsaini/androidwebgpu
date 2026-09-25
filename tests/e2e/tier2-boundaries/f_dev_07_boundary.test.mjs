import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioSound from src/.
import { VirtioSound } from '../../../src/io/audio/virtio_sound.js';

describe('Tier 2: F-DEV-07 Boundary & Corner Cases (production)', () => {
  test('F-DEV-07-B01: sample-rate extremes do not exist as silent config; writes gate on unlock', () => {
    const dev = new VirtioSound();
    assertEqual(dev.unlocked, false);
    assertEqual(dev.audioWrite(new Uint8Array(1024)).error, 'AUDIO_BLOCKED');
  });

  test('F-DEV-07-B02: repeated underruns accumulate a visible counter', () => {
    const dev = new VirtioSound();
    dev.unlock();
    dev.audioWrite(new Uint8Array(0));
    dev.audioWrite(new Uint8Array(0));
    dev.audioWrite(new Uint8Array(0));
    assertEqual(dev.underruns, 3);
  });

  test('F-DEV-07-B03: large PCM bursts are accepted frame-counted', () => {
    const dev = new VirtioSound();
    dev.unlock();
    assertEqual(dev.audioWrite(new Uint8Array(65536)).accepted, true);
    assertEqual(dev.framesAccepted, 1);
  });

  test('F-DEV-07-B04: null PCM is an underrun, not a crash', () => {
    const dev = new VirtioSound();
    dev.unlock();
    const res = dev.audioWrite(null);
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'AUDIO_UNDERRUN');
  });

  test('F-DEV-07-B05: unlock is idempotent and sticky until reset', () => {
    const dev = new VirtioSound();
    dev.unlock();
    dev.unlock();
    assertOk(dev.audioWrite(new Uint8Array(8)).accepted);
    dev.reset();
    assertEqual(dev.unlocked, false);
  });
});
