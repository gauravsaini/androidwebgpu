import { describe, test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { assertEqual, assertThrows, assertOk } from '../harness/assertions.mjs';

function validateGatesText(gates) {
  if (!/^OWNS:\s+.+$/m.test(gates)) throw new Error('GATES_OWNS_MISSING');
  if (!/^Scope:\s+.+$/m.test(gates)) throw new Error('GATES_SCOPE_MISSING');

  const blocks = gates.split(/\n(?=- \[[ x]\] P\d+:)/).filter((block) => /^- \[[ x]\] P\d+:/m.test(block));
  if (blocks.length === 0) throw new Error('GATES_EMPTY');

  const ids = new Set();
  for (const block of blocks) {
    const id = block.match(/^- \[[ x]\] (P\d+):/m)?.[1];
    if (!id || ids.has(id)) throw new Error(`GATE_ID_INVALID_OR_DUPLICATE:${id || 'missing'}`);
    ids.add(id);
    if (!/^  CHECK:\s+\S.+$/m.test(block)) throw new Error(`GATE_CHECK_MISSING:${id}`);
    if (!/^  EXPECT:\s+\S.+$/m.test(block)) throw new Error(`GATE_EXPECT_MISSING:${id}`);
    if (!/^  EVIDENCE:\s+\S.+$/m.test(block)) throw new Error(`GATE_EVIDENCE_MISSING:${id}`);
  }

  if (gates.includes('DEMO_ONLY')) throw new Error('FORBIDDEN_STATUS:DEMO_ONLY');
  return true;
}

function validateIndexContent(index) {
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

  const gateCount = (index.match(/id="badge-g[0-9]"/g) || []).length;
  if (gateCount !== 10) throw new Error(`INDEX_GATE_COUNT:${gateCount}`);
  return true;
}

describe('Tier 2: F-BLD-02 Boundary & Corner Cases (production)', () => {
  test('F-BLD-02-B01: gates validator rejects missing OWNS or Scope headers', () => {
    const missingOwns = 'Scope: test\n- [x] P0: valid\n  CHECK: test\n  EXPECT: ok\n  EVIDENCE: ok\n';
    assertThrows(() => validateGatesText(missingOwns), /GATES_OWNS_MISSING/);

    const missingScope = 'OWNS: test\n- [x] P0: valid\n  CHECK: test\n  EXPECT: ok\n  EVIDENCE: ok\n';
    assertThrows(() => validateGatesText(missingScope), /GATES_SCOPE_MISSING/);
  });

  test('F-BLD-02-B02: gates validator detects duplicate gate IDs', () => {
    const duplicateGates = `OWNS: test
Scope: test

- [x] P0: first
  CHECK: test
  EXPECT: ok
  EVIDENCE: ok

- [x] P0: second duplicate
  CHECK: test
  EXPECT: ok
  EVIDENCE: ok
`;
    assertThrows(() => validateGatesText(duplicateGates), /GATE_ID_INVALID_OR_DUPLICATE:P0/);
  });

  test('F-BLD-02-B03: gates validator rejects forbidden DEMO_ONLY status', () => {
    const demoGate = `OWNS: test
Scope: test

- [x] P0: first
  CHECK: test
  EXPECT: ok
  EVIDENCE: DEMO_ONLY
`;
    assertThrows(() => validateGatesText(demoGate), /FORBIDDEN_STATUS:DEMO_ONLY/);
  });

  test('F-BLD-02-B04: index validator rejects forbidden tokens (putImageData, Arcade3DScene)', () => {
    const invalidIndex = '<html><body>putImageData</body></html>';
    assertThrows(() => validateIndexContent(invalidIndex), /FORBIDDEN_INDEX_TOKEN:putImageData/);

    const arcadeIndex = '<html><body>Arcade3DScene</body></html>';
    assertThrows(() => validateIndexContent(arcadeIndex), /FORBIDDEN_INDEX_TOKEN:Arcade3DScene/);
  });

  test('F-BLD-02-B05: production verify scripts accept the real repo files', () => {
    const only9Badges = Array.from({ length: 9 }, (_, i) => `<div id="badge-g${i}"></div>`).join('\n');
    assertThrows(() => validateIndexContent(only9Badges), /INDEX_GATE_COUNT:9/);

    // Black-box: the real scripts pass on the real work product.
    const gates = spawnSync(process.execPath, ['scripts/verify-gates.mjs'], { encoding: 'utf8' });
    assertEqual(gates.status, 0);
    assertOk(gates.stdout.includes('GATES_FORMAT_OK'));
    const index = spawnSync(process.execPath, ['scripts/verify-index.mjs'], { encoding: 'utf8' });
    assertEqual(index.status, 0);
    assertOk(readFileSync(new URL('../../../GATES.md', import.meta.url), 'utf8').includes('OWNS:'));
  });
});
