import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioNet from src/.
import { VirtioNet, NET_MTU } from '../../../src/io/net/virtio_net.js';

describe('Tier 2: F-DEV-04 Boundary & Corner Cases (production)', () => {
  test('F-DEV-04-B01: exact-MTU frame is accepted', () => {
    const dev = new VirtioNet();
    assertEqual(dev.transmit(new Uint8Array(NET_MTU)).accepted, true);
  });

  test('F-DEV-04-B02: frame beyond MTU+ethernet header is rejected and counted', () => {
    const dev = new VirtioNet();
    const before = dev.txDropped;
    assertEqual(dev.transmit(new Uint8Array(NET_MTU + 15)).accepted, false);
    assertEqual(dev.txDropped, before + 1);
  });

  test('F-DEV-04-B03: zero-byte frame is accepted as a no-op probe', () => {
    const dev = new VirtioNet();
    const res = dev.transmit(new Uint8Array(0));
    assertEqual(res.accepted, true);
    assertEqual(res.bytes, 0);
  });

  test('F-DEV-04-B04: device config exposes MAC, status, and MTU', () => {
    const dev = new VirtioNet();
    assertEqual(dev.readDeviceConfig(10, 2), NET_MTU);
    assertOk(dev.readDeviceConfig(0, 1) >= 0);
  });

  test('F-DEV-04-B05: reset clears RX without changing declared mode', () => {
    const dev = new VirtioNet();
    dev.receive(new Uint8Array(16));
    dev.reset();
    assertEqual(dev.rxQueue.length, 0);
    assertEqual(dev.mode, 'websocket-tunnel');
    assertEqual(dev.name, 'virtio-net');
  });
});
