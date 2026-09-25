/**
 * verify-e2e-prod.mjs - M-E2E black-box coverage gate (informational).
 * Counts E2E suites importing production code (src/ or pkg/) vs mock-only.
 * Exits 0 only when every suite imports production; until then it reports
 * the gap and exits 1. NOT part of `npm run verify` (P0-P5 stay green);
 * it gates M-E2E sign-off only.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';

const root = new URL('..', import.meta.url);
const dirs = ['tests/e2e/tier1-features', 'tests/e2e/tier2-boundaries', 'tests/e2e/tier3-combinations', 'tests/e2e/tier4-scenarios'];

function listTests(dir) {
  const out = [];
  const base = new URL(`${dir}/`, root);
  for (const name of readdirSync(base)) {
    const full = new URL(`${dir}/${name}`, root);
    if (statSync(full).isFile() && name.endsWith('.test.mjs')) out.push(`${dir}/${name}`);
  }
  return out;
}

const files = dirs.flatMap(listTests);
const prod = [];
const mockOnly = [];
for (const f of files) {
  const src = readFileSync(new URL(`${f}`, root), 'utf8');
  // Production surface: src//pkg imports, the servable SUT server, the
  // production verify/vendor/fetch scripts executed as black boxes, and the
  // guest HAL sources pinned by hash-relevant assertions.
  const usesProd = src.includes('../../src/') || src.includes('../../../src/')
    || src.includes('../../pkg/') || src.includes('../../../pkg/')
    || src.includes('scripts/serve.mjs')
    || src.includes('scripts/verify-') || src.includes('scripts/vendor-v86') || src.includes('scripts/fetch-android')
    || src.includes('guest/patches/');
  (usesProd ? prod : mockOnly).push(f);
}

console.log(`E2E_PROD_COVERAGE:${prod.length}/${files.length}`);
for (const f of mockOnly) console.log(`  MOCK_ONLY:${f}`);
if (mockOnly.length > 0) {
  console.error(`E2E_PROD_BLOCKED:${mockOnly.length}-mock-only-suites`);
  process.exit(1);
}
console.log('E2E_PROD_OK');
