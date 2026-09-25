import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real VirtioNet from src/.
import { VirtioNet, NET_MODE, NET_MTU } from '../../../src/io/net/virtio_net.js';

describe('Tier 1: F-DEV-04 VirtioNet User-Mode Proxy Network (production)', () => {
  test('F-DEV-04-01: declared proxy mode is websocket-tunnel, MTU 1500', () => {
    const dev = new VirtioNet();
    assertEqual(dev.mode, NET_MODE);
    assertEqual(dev.mode, 'websocket-tunnel');
    assertEqual(dev.mtu, NET_MTU);
    assertEqual(dev.mac.length, 6);
  });

  test('F-DEV-04-02: link-down drops TX with counted backpressure, never fake-up', () => {
    const dev = new VirtioNet({ linkUp: false });
    const res = dev.transmit(new Uint8Array(64));
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'NET_LINK_DOWN');
    assertEqual(dev.txDropped, 1);
  });

  test('F-DEV-04-03: oversize frames exceeding MTU are rejected', () => {
    const dev = new VirtioNet();
    const res = dev.transmit(new Uint8Array(NET_MTU + 100));
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'NET_MTU_EXCEEDED');
  });

  test('F-DEV-04-04: link flap toggles acceptance', () => {
    const dev = new VirtioNet();
    assertEqual(dev.transmit(new Uint8Array(64)).accepted, true);
    dev.setLink(false);
    assertEqual(dev.transmit(new Uint8Array(64)).accepted, false);
    dev.setLink(true);
    assertEqual(dev.transmit(new Uint8Array(64)).accepted, true);
  });

  test('F-DEV-04-05: RX queue is bounded under backpressure', () => {
    const dev = new VirtioNet({ maxPending: 4 });
    for (let i = 0; i < 4; i++) assertEqual(dev.receive(new Uint8Array(32)).accepted, true);
    const res = dev.receive(new Uint8Array(32));
    assertEqual(res.accepted, false);
    assertEqual(res.error, 'NET_BACKPRESSURE');
  });
});
