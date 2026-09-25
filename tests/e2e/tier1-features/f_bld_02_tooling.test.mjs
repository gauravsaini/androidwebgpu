import { describe, test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { assertEqual, assertOk } from '../harness/assertions.mjs';

function runScript(scriptPath) {
  return spawnSync(process.execPath, [scriptPath], { encoding: 'utf8' });
}

describe('Tier 1: F-BLD-02 Build Scripts & Tooling (production)', () => {
  test('F-BLD-02-01: verify-gates.mjs parses GATES.md successfully', () => {
    const res = runScript('scripts/verify-gates.mjs');
    assertEqual(res.status, 0);
    assertOk(res.stdout.includes('GATES_FORMAT_OK'));
  });

  test('F-BLD-02-02: verify-plan.mjs validates complete plan contract', () => {
    const res = runScript('scripts/verify-plan.mjs');
    assertEqual(res.status, 0);
    assertOk(res.stdout.includes('PLAN_CONTRACT_OK'));
  });

  test('F-BLD-02-03: verify-index.mjs verifies production runtime contract in index.html', () => {
    const res = runScript('scripts/verify-index.mjs');
    assertEqual(res.status, 0);
    assertOk(res.stdout.includes('INDEX_CONTRACT_OK'));
  });

  test('F-BLD-02-04: verify-js.mjs validates JavaScript source parsing without errors', () => {
    const res = runScript('scripts/verify-js.mjs');
    assertEqual(res.status, 0);
    assertOk(res.stdout.includes('JS_CONTRACT_OK'));
  });

  test('F-BLD-02-05: verify-http-entry.mjs validates fail-closed state', () => {
    const res = runScript('scripts/verify-http-entry.mjs');
    assertEqual(res.status, 0);
    assertOk(res.stdout.includes('HTTP_FAIL_CLOSED_OK'));
  });
});
