import { describe, test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { assertEqual, assertOk } from '../harness/assertions.mjs';

function runArtifactCheck() {
  return spawnSync(process.execPath, ['scripts/verify-runtime-artifacts.mjs'], { encoding: 'utf8' });
}

describe('Tier 1: F-VAL-02 Runtime Artifact Acceptance Check', () => {
  test('F-VAL-02-01: verifier is fail-closed (exit 0 iff ACCEPTED, else non-zero)', () => {
    const res = runArtifactCheck();
    const output = res.stderr + res.stdout;
    assertEqual(output.includes('ANDROID_RUNTIME_ACCEPTED'), res.status === 0);
    if (res.status !== 0) assertOk(output.includes('ANDROID_RUNTIME_BLOCKED'));
  });

  test('F-VAL-02-02: bootstrap fixtures are BLOCKED, never ACCEPTED (no false green)', () => {
    const res = runArtifactCheck();
    const output = res.stderr + res.stdout;
    // Until the pinned external release + full WASM land, fixtures must block.
    assertEqual(res.status, 1);
    assertOk(output.includes('ANDROID_RUNTIME_BLOCKED'));
    assertEqual(output.includes('ANDROID_RUNTIME_ACCEPTED'), false);
  });

  test('F-VAL-02-03: blocked output names the exact reason', () => {
    const res = runArtifactCheck();
    const output = res.stderr + res.stdout;
    assertOk(/ANDROID_RUNTIME_BLOCKED:\S+/.test(output));
  });

  test('F-VAL-02-04: success state requires images/manifest.json with version fields', async () => {
    const { readFileSync } = await import('node:fs');
    const manifest = JSON.parse(readFileSync(new URL('../../../images/manifest.json', import.meta.url), 'utf8'));
    assertOk(typeof manifest.androidVersion === 'string');
    assertOk(typeof manifest.abi === 'string');
    assertOk(manifest.sha256 && typeof manifest.sha256 === 'object');
  });

  test('F-VAL-02-05: never emits ACCEPTED alongside BLOCKED', () => {
    const res = runArtifactCheck();
    const output = res.stderr + res.stdout;
    assertEqual(output.includes('ANDROID_RUNTIME_ACCEPTED') && output.includes('ANDROID_RUNTIME_BLOCKED'), false);
  });
});
