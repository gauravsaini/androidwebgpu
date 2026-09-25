import { readFileSync } from 'node:fs';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
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

console.log('HTTP_FAIL_CLOSED_OK');
