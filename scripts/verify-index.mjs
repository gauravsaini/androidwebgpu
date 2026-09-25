import { readFileSync } from 'node:fs';

const index = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const forbidden = [
  'DEMO_ONLY',
  'badge-demo',
  'Arcade3DScene',
  'VisualTestSuite',
  'VirtioGpuDevice',
  'putImageData',
  './pkg/virtio_gpu_bridge.js',
  './src/'
];

for (const token of forbidden) {
  if (index.includes(token)) throw new Error(`FORBIDDEN_INDEX_TOKEN:${token}`);
}

const required = [
  "const RUNTIME_MODULE_URL = './pkg/android_vm.js';",
  'await import(RUNTIME_MODULE_URL)',
  'window.runValidationLoop = runValidationLoop;',
  'window.__VALIDATION_RESULTS__ = null;',
  "setRuntimeState('BLOCKED'",
];
for (const token of required) {
  if (!index.includes(token)) throw new Error(`INDEX_TOKEN_MISSING:${token}`);
}

const gateCount = (index.match(/id="badge-g[0-9]"/g) || []).length;
if (gateCount !== 10) throw new Error(`INDEX_GATE_COUNT:${gateCount}`);
const pendingCount = (index.match(/class="badge badge-pending"/g) || []).length;
if (pendingCount !== 10) throw new Error(`INDEX_INITIAL_PENDING_COUNT:${pendingCount}`);
if (!index.includes('BOOT WAIT')) throw new Error('INDEX_BOOT_WAIT_MISSING');

console.log('INDEX_CONTRACT_OK');
