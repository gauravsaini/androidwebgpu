import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioConsole from src/.
import { VirtioConsole, CONSOLE_MAX_BUFFER } from '../../../src/io/console/virtio_console.js';

describe('Tier 2: F-DEV-02 Boundary & Corner Cases (production)', () => {
  test('F-DEV-02-B01: empty write is accepted and contributes no bytes', () => {
    const dev = new VirtioConsole();
    const res = dev.writeSerial('');
    assertEqual(res.accepted, true);
    assertEqual(dev.logBytes, 0);
  });

  test('F-DEV-02-B02: buffer saturation counts overflow instead of losing order', () => {
    const dev = new VirtioConsole();
    dev.logBytes = CONSOLE_MAX_BUFFER;
    const res = dev.writeSerial('x');
    assertEqual(res.accepted, false);
    assertEqual(dev.overflowCount, 1);
  });

  test('F-DEV-02-B03: binary (non-UTF8) bytes are preserved losslessly', () => {
    const dev = new VirtioConsole();
    const bytes = new Uint8Array([0xff, 0x00, 0x80, 0x41]);
    dev.appendLog(bytes);
    assertEqual(dev.log[0].data.length, 4);
    assertEqual(dev.log[0].data[3], 0x41);
  });

  test('F-DEV-02-B04: RX disabled by default; enabling is explicit', () => {
    const dev = new VirtioConsole();
    assertEqual(dev.rxEnabled, false);
    const opt = new VirtioConsole({ rxEnabled: true });
    assertEqual(opt.rxEnabled, true);
    assertOk(opt.injectRx(new Uint8Array([1])).accepted);
  });

  test('F-DEV-02-B05: teardown wakes pending reads by clearing RX', () => {
    const dev = new VirtioConsole({ rxEnabled: true });
    dev.injectRx(new Uint8Array([1, 2, 3]));
    dev.reset();
    assertEqual(dev.rxQueue.length, 0);
  });
});
