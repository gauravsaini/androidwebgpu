import { describe, test } from 'node:test';
import { assertEqual, assertThrows, assertOk } from '../harness/assertions.mjs';
// Production import: live HTTP assertions run against the real server.
import { startServer } from '../../../scripts/serve.mjs';

function validateHttpBootstrapHtml(html) {
  const head = html.slice(0, html.indexOf('<script type="module">'));

  if (!html.includes("const RUNTIME_MODULE_URL = './pkg/android_vm.js';")) {
    throw new Error('HTTP_RUNTIME_URL_MISSING');
  }
  if (!html.includes('async function bootstrap()')) throw new Error('HTTP_BOOTSTRAP_MISSING');
  if (!html.includes('async function prepareRuntime()')) throw new Error('HTTP_PREFLIGHT_MISSING');
  if (!html.includes('function blockedRun(run, reason)')) throw new Error('HTTP_BLOCKED_PATH_MISSING');
  if (!head.includes('BOOT WAIT')) throw new Error('HTTP_INITIAL_READY_STATE');
  if ((head.match(/class="badge badge-pending"/g) || []).length !== 10) {
    throw new Error('HTTP_INITIAL_GATE_STATE');
  }
  if (head.includes('ONLINE')) {
    throw new Error('HTTP_FALSE_GREEN_STATE');
  }

  return true;
}

describe('Tier 2: F-VAL-03 Boundary & Corner Cases (production)', () => {
  const validHtmlSnippet = `
    <html>
      <head>
        <div id="status">BOOT WAIT</div>
        ${Array.from({ length: 10 }, (_, i) => `<span id="badge-g${i}" class="badge badge-pending">G${i}</span>`).join('\n')}
      </head>
      <body>
        <script type="module">
          const RUNTIME_MODULE_URL = './pkg/android_vm.js';
          async function bootstrap() {}
          async function prepareRuntime() {}
          function blockedRun(run, reason) {}
        </script>
      </body>
    </html>
  `;

  test('F-VAL-03-B01: premature ONLINE badge in initial DOM throws HTTP_FALSE_GREEN_STATE', () => {
    const falseGreen = validHtmlSnippet.replace('BOOT WAIT', 'BOOT WAIT ONLINE');
    assertThrows(() => validateHttpBootstrapHtml(falseGreen), /HTTP_FALSE_GREEN_STATE/);
  });

  test('F-VAL-03-B02: missing blockedRun fail-closed path throws HTTP_BLOCKED_PATH_MISSING', () => {
    const missingBlocked = validHtmlSnippet.replace('function blockedRun(run, reason) {}', '');
    assertThrows(() => validateHttpBootstrapHtml(missingBlocked), /HTTP_BLOCKED_PATH_MISSING/);
  });

  test('F-VAL-03-B03: missing prepareRuntime preflight check throws HTTP_PREFLIGHT_MISSING', () => {
    const missingPreflight = validHtmlSnippet.replace('async function prepareRuntime() {}', '');
    assertThrows(() => validateHttpBootstrapHtml(missingPreflight), /HTTP_PREFLIGHT_MISSING/);
  });

  test('F-VAL-03-B04: missing initial BOOT WAIT status throws HTTP_INITIAL_READY_STATE', () => {
    const missingBootWait = validHtmlSnippet.replace('BOOT WAIT', 'IDLE');
    assertThrows(() => validateHttpBootstrapHtml(missingBootWait), /HTTP_INITIAL_READY_STATE/);
  });

  test('F-VAL-03-B05: served entry ships exactly 10 pending badges, never preset green', async () => {
    const only5Badges = `
      <html>
        <head>
          <div id="status">BOOT WAIT</div>
          ${Array.from({ length: 5 }, (_, i) => `<span id="badge-g${i}" class="badge badge-pending">G${i}</span>`).join('\n')}
        </head>
        <body>
          <script type="module">
            const RUNTIME_MODULE_URL = './pkg/android_vm.js';
            async function bootstrap() {}
            async function prepareRuntime() {}
            function blockedRun(run, reason) {}
          </script>
        </body>
      </html>
    `;
    assertThrows(() => validateHttpBootstrapHtml(only5Badges), /HTTP_INITIAL_GATE_STATE/);

    // Black-box: the served production entry upholds the same contract live.
    const { server, port } = await startServer({});
    try {
      const html = await (await fetch(`http://127.0.0.1:${port}/index.html`)).text();
      assertEqual(validateHttpBootstrapHtml(html), true);
    } finally {
      server.close();
    }
  });
});
