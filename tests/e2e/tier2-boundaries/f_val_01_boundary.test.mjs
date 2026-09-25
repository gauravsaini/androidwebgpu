import { describe, test } from 'node:test';
import { assertEqual, assertThrows, assertOk } from '../harness/assertions.mjs';
// Production imports: real gate validators from src/.
import { validateGateResult, validateRunResult, GATE_IDS } from '../../../src/validation/validation_loop.js';

function createValidRunResult(overrides = {}) {
  const gates = {};
  for (const gid of GATE_IDS) {
    gates[gid] = { status: 'PASSED', evidence: ['test_ok'], error: null };
  }
  return {
    runId: 'test-run-123',
    epoch: 1,
    startedAt: new Date(Date.now() - 1000).toISOString(),
    endedAt: new Date().toISOString(),
    ready: true,
    gates,
    ...overrides
  };
}

describe('Tier 2: F-VAL-01 Boundary & Corner Cases (production)', () => {
  test('F-VAL-01-B01: unknown gate ID throws VALIDATION_GATE_ID_INVALID', () => {
    assertThrows(() => validateGateResult('g10', { status: 'PASSED', evidence: [], error: null }), /VALIDATION_GATE_ID_INVALID:g10/);
    assertThrows(() => validateGateResult('g_extra', { status: 'PASSED', evidence: [], error: null }), /VALIDATION_GATE_ID_INVALID:g_extra/);
  });

  test('F-VAL-01-B02: illegal gate status throws VALIDATION_GATE_STATUS_INVALID', () => {
    assertThrows(() => validateGateResult('g0', { status: 'SUCCESS', evidence: [], error: null }), /VALIDATION_GATE_STATUS_INVALID/);
    assertThrows(() => validateGateResult('g1', { status: 'COMPLETED', evidence: [], error: null }), /VALIDATION_GATE_STATUS_INVALID/);
  });

  test('F-VAL-01-B03: PASSED gate containing non-null error is rejected', () => {
    assertThrows(
      () => validateGateResult('g2', { status: 'PASSED', evidence: [], error: 'ANOMALY_DETECTED' }),
      /VALIDATION_GATE_STATUS_INVALID/
    );
  });

  test('F-VAL-01-B04: FAILED or BLOCKED gate without error string is rejected', () => {
    assertThrows(
      () => validateGateResult('g3', { status: 'FAILED', evidence: [], error: null }),
      /VALIDATION_RESULT_INVALID/
    );
    assertThrows(
      () => validateGateResult('g4', { status: 'BLOCKED', evidence: [], error: null }),
      /VALIDATION_RESULT_INVALID/
    );
  });

  test('F-VAL-01-B05: false-green validation detects ready:true when any gate is not PASSED', () => {
    const invalidResult = createValidRunResult();
    invalidResult.gates.g5 = { status: 'BLOCKED', evidence: [], error: 'RESOURCE_TIMEOUT' };
    invalidResult.ready = true; // Illegal false green

    assertThrows(() => validateRunResult(invalidResult), /ready-without-all-passed/);
  });
});
