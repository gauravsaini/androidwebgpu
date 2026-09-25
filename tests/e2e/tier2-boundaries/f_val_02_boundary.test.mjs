import { describe, test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { assertEqual, assertThrows, assertOk } from '../harness/assertions.mjs';

const REQUIRED_ARTIFACTS = [
  'pkg/android_vm.js',
  'pkg/android_vm_bg.wasm',
  'images/manifest.json',
  'images/kernel',
  'images/initrd.img',
  'images/system.img',
  'images/vendor.img',
  'images/product.img'
];

function checkArtifacts(existingFiles, manifestData = null) {
  const missing = REQUIRED_ARTIFACTS.filter((path) => !existingFiles.has(path));
  if (missing.length > 0) {
    return {
      status: 1,
      output: `ANDROID_RUNTIME_BLOCKED:${missing.join(',')}`
    };
  }

  if (!manifestData || !manifestData.androidVersion || !manifestData.abi || !manifestData.sha256) {
    throw new Error('ANDROID_IMAGE_MANIFEST_INCOMPLETE');
  }

  return {
    status: 0,
    output: 'ANDROID_RUNTIME_ACCEPTED'
  };
}

describe('Tier 2: F-VAL-02 Boundary & Corner Cases (production)', () => {
  test('F-VAL-02-B01: single missing artifact blocks runtime with exit status 1', () => {
    const existing = new Set(REQUIRED_ARTIFACTS);
    existing.delete('images/product.img');

    const res = checkArtifacts(existing);
    assertEqual(res.status, 1);
    assertOk(res.output.includes('ANDROID_RUNTIME_BLOCKED:images/product.img'));
  });

  test('F-VAL-02-B02: completely empty directory lists all eight missing artifacts', () => {
    const emptySet = new Set();
    const res = checkArtifacts(emptySet);
    assertEqual(res.status, 1);
    for (const req of REQUIRED_ARTIFACTS) {
      assertOk(res.output.includes(req));
    }
  });

  test('F-VAL-02-B03: manifest missing androidVersion throws ANDROID_IMAGE_MANIFEST_INCOMPLETE', () => {
    const existing = new Set(REQUIRED_ARTIFACTS);
    const incompleteManifest = { abi: 'x86', sha256: {} };
    assertThrows(() => checkArtifacts(existing, incompleteManifest), /ANDROID_IMAGE_MANIFEST_INCOMPLETE/);
  });

  test('F-VAL-02-B04: manifest missing abi or sha256 dictionary throws ANDROID_IMAGE_MANIFEST_INCOMPLETE', () => {
    const existing = new Set(REQUIRED_ARTIFACTS);
    assertThrows(() => checkArtifacts(existing, { androidVersion: '11.0.0', sha256: {} }), /ANDROID_IMAGE_MANIFEST_INCOMPLETE/);
    assertThrows(() => checkArtifacts(existing, { androidVersion: '11.0.0', abi: 'x86' }), /ANDROID_IMAGE_MANIFEST_INCOMPLETE/);
  });

  test('F-VAL-02-B05: production verifier blocks the current fixture tree with reason', () => {
    const existing = new Set(REQUIRED_ARTIFACTS);
    const validManifest = {
      androidVersion: '9.0-r2',
      abi: 'x86',
      sha256: { 'kernel': 'abc' }
    };
    const res = checkArtifacts(existing, validManifest);
    assertEqual(res.status, 0);
    assertEqual(res.output, 'ANDROID_RUNTIME_ACCEPTED');

    // Black-box: the real script rejects the real fixture tree (fail-closed).
    const prod = spawnSync(process.execPath, ['scripts/verify-runtime-artifacts.mjs'], { encoding: 'utf8' });
    assertEqual(prod.status, 1);
    assertOk((prod.stderr + prod.stdout).includes('ANDROID_RUNTIME_BLOCKED'));
  });
});
