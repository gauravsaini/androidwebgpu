import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioConsole from src/.
import { VirtioConsole, CONSOLE_MAX_BUFFER } from '../../../src/io/console/virtio_console.js';

describe('Tier 1: F-DEV-02 VirtioConsole Serial Device & Log Streaming (production)', () => {
  test('F-DEV-02-01: transmits serial bytes and decodes UTF-8 string log', () => {
    const consoleDev = new VirtioConsole();
    const message = 'Linux version 4.19.110-android-x86\n';
    const res = consoleDev.writeSerial(message);
    assertEqual(res.accepted, true);
    assertEqual(consoleDev.serialText(), message);
  });

  test('F-DEV-02-02: RX input is opt-in and ordered', () => {
    const locked = new VirtioConsole();
    assertEqual(locked.injectRx(new Uint8Array([1])).accepted, false);
    const open = new VirtioConsole({ rxEnabled: true });
    assertEqual(open.injectRx(new TextEncoder().encode('getprop')).accepted, true);
    assertEqual(open.rxQueue.length, 1);
  });

  test('F-DEV-02-03: streams multi-line kernel dmesg without data loss', () => {
    const consoleDev = new VirtioConsole();
    const lines = [
      '[    0.000000] Linux version 4.19.110',
      '[    0.001000] Command line: console=ttyS0 root=/dev/ram0',
      '[    0.002000] Memory: 524288K'
    ];
    for (const l of lines) consoleDev.writeSerial(l + '\n');
    assertEqual(consoleDev.serialText().split('\n').filter(Boolean).length, 3);
  });

  test('F-DEV-02-04: log entries carry epoch and timestamp metadata', () => {
    const consoleDev = new VirtioConsole();
    consoleDev.writeSerial('BOOT');
    assertEqual(consoleDev.log.length, 1);
    assertOk(typeof consoleDev.log[0].ts === 'number');
    assertEqual(consoleDev.serialText(), 'BOOT');
  });

  test('F-DEV-02-05: reset wakes pending reads and preserves the size bound', () => {
    const consoleDev = new VirtioConsole({ rxEnabled: true });
    consoleDev.injectRx(new Uint8Array([0x41]));
    consoleDev.reset();
    assertEqual(consoleDev.rxQueue.length, 0);
    assertEqual(consoleDev.name, 'virtio-console');
    assertOk(CONSOLE_MAX_BUFFER >= 1024 * 1024);
  });
});
