import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production SUT: the real COOP/COEP static server + built bundle on disk.
import { startServer } from '../../../scripts/serve.mjs';
import { VmRuntime } from '../../../src/vm/vm_runtime.js';
import { V86Adapter } from '../../../src/vm/v86_adapter.js';

async function getJSON(port, path) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`);
  return { status: res.status, headers: res.headers, text: await res.text() };
}

describe('Tier 4: HTTP SUT — served bundle under test (production)', () => {
  test('SUT-HTTP-01: entry serves 200 with COOP/COEP isolation headers', async () => {
    const { server, port } = await startServer({});
    try {
      const res = await fetch(`http://127.0.0.1:${port}/index.html`);
      assertEqual(res.status, 200);
      assertEqual(res.headers.get('cross-origin-opener-policy'), 'same-origin');
      assertEqual(res.headers.get('cross-origin-embedder-policy'), 'require-corp');
      await res.text();
    } finally {
      server.close();
    }
  });

  test('SUT-HTTP-02: production runtime module + WASM served with correct MIME', async () => {
    const { server, port } = await startServer({});
    try {
      const js = await fetch(`http://127.0.0.1:${port}/pkg/android_vm.js`);
      assertEqual(js.status, 200);
      assertOk((js.headers.get('content-type') || '').includes('javascript'));
      const wasm = await fetch(`http://127.0.0.1:${port}/pkg/android_vm_bg.wasm`);
      assertEqual(wasm.status, 200);
      assertEqual(wasm.headers.get('content-type'), 'application/wasm');
      const magic = new Uint8Array(await wasm.arrayBuffer());
      assertEqual(magic[0], 0x00);
      assertEqual(magic[1], 0x61);
    } finally {
      server.close();
    }
  });

  test('SUT-HTTP-03: boot manifest served; kernel/initrd pinned hashes match bytes', async () => {
    const { server, port } = await startServer({});
    try {
      const { status, text } = await getJSON(port, '/images/manifest.json');
      assertEqual(status, 200);
      const manifest = JSON.parse(text);
      assertOk(typeof manifest.androidVersion === 'string');
      const { createHash } = await import('node:crypto');
      for (const role of ['kernel', 'initrd.img']) {
        const res = await fetch(`http://127.0.0.1:${port}/images/${role === 'kernel' ? 'kernel' : role}`);
        assertEqual(res.status, 200);
        const bytes = new Uint8Array(await res.arrayBuffer());
        const actual = createHash('sha256').update(bytes).digest('hex');
        assertEqual(actual, String(manifest.sha256[role]).toLowerCase());
      }
    } finally {
      server.close();
    }
  });

  test('SUT-HTTP-04: served entry is fail-closed (BOOT WAIT, all PENDING, no green)', async () => {
    const { server, port } = await startServer({});
    try {
      const { text } = await getJSON(port, '/index.html');
      assertOk(text.includes('BOOT WAIT'));
      assertEqual((text.match(/class="badge badge-pending"/g) || []).length, 10);
      assertEqual(text.includes('>PASSED<'), false);
    } finally {
      server.close();
    }
  });

  test('SUT-HTTP-05: pkg entry exports the production runtime contract', async () => {
    const { server, port } = await startServer({});
    try {
      const { text } = await getJSON(port, '/pkg/android_vm.js');
      assertOk(text.includes('createAndroidRuntime'));
      assertOk(text.includes('runValidationLoop'));
      assertOk(text.includes('V86_NOT_PRESENT'));
    } finally {
      server.close();
    }
  });

  test('SUT-HTTP-06: strict VM start against served media fails closed without attach (no stub boot)', async () => {
    // Starts the real VM path (production VmRuntime, no allowStub) while no
    // backend is attached: start() must reject V86_NOT_PRESENT and the worker
    // must record zero stub quanta — even though the bundle is vendored.
    const { server } = await startServer({});
    try {
      const adapter = new V86Adapter();
      const probed = await adapter.probe();
      assertEqual(probed.present, true);
      const { V86 } = await adapter.loadV86();
      assertEqual(typeof V86, 'function');
      const rt = new VmRuntime();
      await rt.create({ ramSize: 16 * 1024 * 1024 });
      const kernel = new Uint8Array((await import('node:fs')).readFileSync(new URL('../../../images/kernel', import.meta.url)));
      await rt.loadBoot({ kernel, cmdline: 'console=ttyS0' });
      let rejected = false;
      try {
        await rt.start();
      } catch (e) {
        rejected = /V86_NOT_PRESENT/.test(e.message);
      }
      assertEqual(rejected, true);
      assertEqual(rt.state, 'CONFIGURED');
      assertEqual(rt.worker.getStubQuanta(), 0);
      await rt.destroy();
    } finally {
      server.close();
    }
  });
});
