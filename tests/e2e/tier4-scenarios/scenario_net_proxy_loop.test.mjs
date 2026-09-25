import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real VirtioNet + validation loop from src/.
import { VirtioNet } from '../../../src/io/net/virtio_net.js';
import { runValidationGates, GATE_IDS } from '../../../src/validation/validation_loop.js';

describe('Tier 4: Scenario 4 — Network Proxy Loop (production)', () => {
  test('SCN-NET-01: guest frame transmits through the declared proxy mode', () => {
    const dev = new VirtioNet();
    const dnsQuery = new Uint8Array([
      0x08, 0x00, 0x45, 0x00, 0x00, 0x3c,
      0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
      0x07, 0x61, 0x6e, 0x64, 0x72, 0x6f, 0x69, 0x64, 0x00,
    ]);
    const res = dev.transmit(dnsQuery);
    assertEqual(res.accepted, true);
    assertEqual(res.mode, 'websocket-tunnel');
    assertEqual(res.bytes, dnsQuery.byteLength);
  });

  test('SCN-NET-02: link flap drops then restores TX (no fake link-up)', () => {
    const dev = new VirtioNet();
    assertEqual(dev.transmit(new Uint8Array(64)).accepted, true);
    dev.setLink(false);
    const down = dev.transmit(new Uint8Array(64));
    assertEqual(down.accepted, false);
    assertEqual(down.error, 'NET_LINK_DOWN');
    dev.setLink(true);
    assertEqual(dev.transmit(new Uint8Array(64)).accepted, true);
  });

  test('SCN-NET-03: RX loopback queues host bytes for the guest', () => {
    const dev = new VirtioNet();
    const answer = new Uint8Array([0x12, 0x34, 0x81, 0x80, 0x0a, 0x00, 0x02, 0x0f]);
    assertEqual(dev.receive(answer).accepted, true);
    assertEqual(dev.rxQueue.length, 1);
    assertEqual(dev.rxQueue[0][4], 0x0a);
    assertEqual(dev.rxQueue[0][7], 0x0f);
  });

  test('SCN-NET-04: link-down keeps the device healthy and countable', () => {
    const dev = new VirtioNet();
    dev.setLink(false);
    dev.transmit(new Uint8Array(32));
    assertEqual(dev.txDropped, 1);
    dev.setLink(true);
    assertEqual(dev.transmit(new Uint8Array(32)).accepted, true);
  });

  test('SCN-NET-05: production loop marks G5 PASSED on proxy-policy proof', async () => {
    const dev = new VirtioNet();
    const lease = dev.transmit(new Uint8Array(64)).accepted && dev.mode === 'websocket-tunnel';
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, async () => ({ status: 'PASSED', evidence: ['ok'], error: null })]));
    probes.g5 = async () => lease
      ? { status: 'PASSED', evidence: ['proxy-mode:' + dev.mode, 'tx-ok'], error: null }
      : { status: 'BLOCKED', evidence: [], error: 'NET_LINK_DOWN' };
    const result = await runValidationGates({ runId: 'scn-net-run', epoch: 1, probes });
    assertEqual(result.gates.g5.status, 'PASSED');
  });
});
