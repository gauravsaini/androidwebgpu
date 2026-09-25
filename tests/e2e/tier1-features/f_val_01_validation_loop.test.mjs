import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real truth-based gate evaluator from src/.
import { runValidationGates, GATE_IDS } from '../../../src/validation/validation_loop.js';

const blocked = (error) => async () => ({ status: 'BLOCKED', evidence: [], error });
const passed = (evidence) => async () => ({ status: 'PASSED', evidence, error: null });

describe('Tier 1: F-VAL-01 Dynamic Truth-Based ValidationLoop G0-G9 (production)', () => {
  test('F-VAL-01-01: evaluates all ten gates G0 through G9', async () => {
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, blocked('RUNTIME_NOT_BUILT')]));
    const result = await runValidationGates({ runId: 'run-1', epoch: 1, probes });
    assertEqual(Object.keys(result.gates).length, 10);
    for (const gid of GATE_IDS) {
      assertOk(result.gates[gid]);
    }
  });

  test('F-VAL-01-02: terminal gate states are exactly PASSED, FAILED, or BLOCKED', async () => {
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, blocked('RUNTIME_NOT_BUILT')]));
    const result = await runValidationGates({ runId: 'run-2', epoch: 1, probes });
    for (const gid of GATE_IDS) {
      const status = result.gates[gid].status;
      assertOk(['PASSED', 'FAILED', 'BLOCKED'].includes(status));
    }
  });

  test('F-VAL-01-03: returns evidence array and error code for each gate', async () => {
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, blocked('RUNTIME_NOT_BUILT')]));
    const result = await runValidationGates({ runId: 'run-3', epoch: 1, probes });
    for (const gid of GATE_IDS) {
      assertEqual(result.gates[gid].error, 'RUNTIME_NOT_BUILT');
      assertOk(Array.isArray(result.gates[gid].evidence));
    }
  });

  test('F-VAL-01-04: ready is false when any gate is BLOCKED or FAILED', async () => {
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, passed(['ok'])]));
    probes.g9 = blocked('NOT_TESTED');
    const result = await runValidationGates({ runId: 'run-4', epoch: 1, probes });
    assertEqual(result.ready, false);
  });

  test('F-VAL-01-05: invokes onGate progress callback for each evaluated gate', async () => {
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, blocked('RUNTIME_NOT_BUILT')]));
    const observed = [];
    await runValidationGates({ runId: 'run-5', epoch: 1, probes, onGate: (g) => observed.push(g.id) });
    assertEqual(observed.length, 10);
    assertOk(observed.includes('g0'));
    assertOk(observed.includes('g9'));
  });
});
