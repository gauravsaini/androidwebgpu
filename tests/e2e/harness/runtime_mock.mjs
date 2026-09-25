/**
 * Specification-Compliant AndroidRuntime & ValidationLoop Contract Oracle
 * Implements validation rules for F-VAL-01, F-VAL-02, F-VAL-03.
 */

export const GATE_IDS = ['g0', 'g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7', 'g8', 'g9'];
export const VALID_STATUSES = ['PENDING', 'RUNNING', 'PASSED', 'FAILED', 'BLOCKED'];

export function validateRuntimeEvent(event) {
  if (typeof event !== 'object' || event === null) throw new Error('INVALID_RUNTIME_EVENT:not_an_object');
  if (typeof event.epoch !== 'number') throw new Error('INVALID_RUNTIME_EVENT:epoch_must_be_number');
  if (typeof event.seq !== 'number') throw new Error('INVALID_RUNTIME_EVENT:seq_must_be_number');
  if (typeof event.ts !== 'number') throw new Error('INVALID_RUNTIME_EVENT:ts_must_be_number');
  if (typeof event.src !== 'string' || !event.src) throw new Error('INVALID_RUNTIME_EVENT:src_must_be_nonempty_string');
  if (typeof event.kind !== 'string' || !event.kind) throw new Error('INVALID_RUNTIME_EVENT:kind_must_be_nonempty_string');
  return true;
}

export function validateGateResult(gateId, gateResult) {
  if (!GATE_IDS.includes(gateId)) throw new Error(`UNKNOWN_GATE_ID:${gateId}`);
  if (!VALID_STATUSES.includes(gateResult.status)) throw new Error(`INVALID_GATE_STATUS:${gateResult.status}`);
  if (!Array.isArray(gateResult.evidence)) throw new Error('GATE_EVIDENCE_NOT_ARRAY');
  if (gateResult.status === 'PASSED' && gateResult.error !== null) {
    throw new Error('GATE_PASSED_WITH_ERROR');
  }
  if ((gateResult.status === 'FAILED' || gateResult.status === 'BLOCKED') && typeof gateResult.error !== 'string') {
    throw new Error('GATE_FAILED_WITHOUT_ERROR_CODE');
  }
  return true;
}

export function validateRuntimeRunResult(result) {
  if (!result || typeof result !== 'object') throw new Error('RUN_RESULT_NOT_OBJECT');
  if (typeof result.runId !== 'string' || !result.runId) throw new Error('RUN_RESULT_INVALID_RUN_ID');
  if (typeof result.epoch !== 'number') throw new Error('RUN_RESULT_INVALID_EPOCH');
  if (typeof result.startedAt !== 'string' || Number.isNaN(Date.parse(result.startedAt))) {
    throw new Error('RUN_RESULT_INVALID_STARTED_AT');
  }
  if (typeof result.endedAt !== 'string' || Number.isNaN(Date.parse(result.endedAt))) {
    throw new Error('RUN_RESULT_INVALID_ENDED_AT');
  }
  if (typeof result.ready !== 'boolean') throw new Error('RUN_RESULT_READY_NOT_BOOLEAN');
  if (!result.gates || typeof result.gates !== 'object') throw new Error('RUN_RESULT_MISSING_GATES');

  let allPassed = true;
  for (const gid of GATE_IDS) {
    const g = result.gates[gid];
    if (!g) throw new Error(`RUN_RESULT_MISSING_GATE:${gid}`);
    validateGateResult(gid, g);
    if (g.status !== 'PASSED') allPassed = false;
  }

  // Strict contract: ready === true ONLY if all 10 gates passed in this epoch
  if (result.ready && !allPassed) {
    throw new Error('FALSE_GREEN_STATE:ready_is_true_while_not_all_gates_passed');
  }
  if (!result.ready && allPassed) {
    throw new Error('UNSET_READY_STATE:ready_is_false_while_all_gates_passed');
  }

  return true;
}

export class MockAndroidRuntime {
  constructor({ canvas, onEvent } = {}) {
    this.canvas = canvas;
    this.onEvent = onEvent || (() => {});
    this.epoch = 0;
    this.destroyed = false;
  }

  async runValidationLoop({ runId = 'test-run', epoch = 1, onGate = () => {}, evaluateGate = null } = {}) {
    if (this.destroyed) throw new Error('RUNTIME_DESTROYED');
    this.epoch = epoch;
    const startedAt = new Date().toISOString();

    const gates = {};
    for (const gid of GATE_IDS) {
      gates[gid] = { status: 'PENDING', evidence: [], error: null };
    }

    for (const gid of GATE_IDS) {
      gates[gid].status = 'RUNNING';
      const evaluated = evaluateGate ? await evaluateGate(gid, epoch) : { status: 'BLOCKED', evidence: [], error: 'NOT_IMPLEMENTED' };
      gates[gid] = evaluated;
      onGate({ gateId: gid, ...evaluated });
    }

    const endedAt = new Date().toISOString();
    const allPassed = Object.values(gates).every(g => g.status === 'PASSED');

    const result = {
      runId,
      epoch,
      startedAt,
      endedAt,
      ready: allPassed,
      gates
    };

    validateRuntimeRunResult(result);
    return result;
  }

  async destroy() {
    this.destroyed = true;
  }
}
