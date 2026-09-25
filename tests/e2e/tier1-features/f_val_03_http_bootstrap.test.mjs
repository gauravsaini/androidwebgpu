import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real COOP/COEP server; assertions run over live HTTP.
import { startServer } from '../../../scripts/serve.mjs';

async function fetchEntry(port) {
  const res = await fetch(`http://127.0.0.1:${port}/index.html`);
  assertEqual(res.status, 200);
  assertEqual(res.headers.get('cross-origin-opener-policy'), 'same-origin');
  assertEqual(res.headers.get('cross-origin-embedder-policy'), 'require-corp');
  return res.text();
}

describe('Tier 1: F-VAL-03 Fail-Closed HTTP Bootstrap & UI State (production)', () => {
  test('F-VAL-03-01: served entry displays initial BOOT WAIT state', async () => {
    const { server, port } = await startServer({});
    try {
      assertOk((await fetchEntry(port)).includes('BOOT WAIT'));
    } finally {
      server.close();
    }
  });

  test('F-VAL-03-02: served entry ships all ten gate badges as pending', async () => {
    const { server, port } = await startServer({});
    try {
      const html = await fetchEntry(port);
      assertEqual((html.match(/class="badge badge-pending"/g) || []).length, 10);
    } finally {
      server.close();
    }
  });

  test('F-VAL-03-03: served entry imports production runtime dynamically', async () => {
    const { server, port } = await startServer({});
    try {
      const html = await fetchEntry(port);
      assertOk(html.includes("const RUNTIME_MODULE_URL = './pkg/android_vm.js';"));
      assertOk(html.includes('await import(RUNTIME_MODULE_URL)'));
    } finally {
      server.close();
    }
  });

  test('F-VAL-03-04: served entry carries the BLOCKED fail-closed path', async () => {
    const { server, port } = await startServer({});
    try {
      assertOk((await fetchEntry(port)).includes("setRuntimeState('BLOCKED'"));
    } finally {
      server.close();
    }
  });

  test('F-VAL-03-05: served entry exposes the validation loop record', async () => {
    const { server, port } = await startServer({});
    try {
      const html = await fetchEntry(port);
      assertOk(html.includes('window.runValidationLoop = runValidationLoop;'));
      assertOk(html.includes('window.__VALIDATION_RESULTS__ = null;'));
    } finally {
      server.close();
    }
  });
});
