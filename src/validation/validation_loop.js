/**
 * validation_loop.js - Entity E17: truth-based gate evaluator (G0-G9).
 * Initial status PENDING; a gate passes only from an observed assertion in
 * the current run epoch. Missing prerequisites yield BLOCKED. Isolated
 * synthetic tests never count toward OS readiness.
 */

export const GATE_IDS = Object.freeze(['g0', 'g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7', 'g8', 'g9']);
export const TERMINAL_STATUSES = Object.freeze(['PASSED', 'FAILED', 'BLOCKED']);

export function emptyGate() {
  return { status: 'PENDING', evidence: [], error: null };
}

export function validateGateResult(id, gate) {
  if (!GATE_IDS.includes(id)) throw new Error(`VALIDATION_GATE_ID_INVALID:${id}`);
  if (!gate || typeof gate !== 'object') throw new Error(`VALIDATION_RESULT_INVALID:${id}`);
  if (!TERMINAL_STATUSES.includes(gate.status)) throw new Error(`VALIDATION_GATE_STATUS_INVALID:${id}`);
  if (!Array.isArray(gate.evidence)) throw new Error(`VALIDATION_RESULT_INVALID:${id}`);
  if (gate.status === 'PASSED' && gate.error !== null) throw new Error(`VALIDATION_GATE_STATUS_INVALID:${id}`);
  if (gate.status !== 'PASSED' && typeof gate.error !== 'string') throw new Error(`VALIDATION_RESULT_INVALID:${id}`);
  return true;
}

export function validateRunResult(result) {
  if (!result || typeof result !== 'object') throw new Error('VALIDATION_RESULT_INVALID');
  if (typeof result.runId !== 'string' || !result.runId) throw new Error('VALIDATION_RESULT_INVALID');
  if (typeof result.epoch !== 'number') throw new Error('VALIDATION_RESULT_INVALID');
  if (!result.gates || typeof result.gates !== 'object') throw new Error('VALIDATION_RESULT_INVALID');
  for (const id of GATE_IDS) {
    if (!result.gates[id]) throw new Error(`VALIDATION_GATE_MISSING:${id}`);
    validateGateResult(id, result.gates[id]);
  }
  const allPassed = GATE_IDS.every((id) => result.gates[id].status === 'PASSED');
  if (result.ready === true && !allPassed) throw new Error('VALIDATION_RESULT_INVALID:ready-without-all-passed');
  if (result.ready !== true && allPassed) throw new Error('VALIDATION_RESULT_INVALID:all-passed-without-ready');
  return true;
}

/**
 * Evaluate gates G0-G9 against live probes. Each probe is an async function
 * returning {status, evidence, error}; omitted probes default to BLOCKED.
 */
export async function runValidationGates({ runId, epoch, probes = {}, onGate = null } = {}) {
  const startedAt = new Date().toISOString();
  const gates = {};
  for (const id of GATE_IDS) {
    const probe = probes[id];
    if (typeof probe !== 'function') {
      gates[id] = { status: 'BLOCKED', evidence: [], error: 'RUNTIME_NOT_BUILT' };
    } else {
      try {
        const out = await probe({ runId, epoch });
        validateGateResult(id, out);
        gates[id] = out;
      } catch (err) {
        gates[id] = { status: 'FAILED', evidence: [], error: err instanceof Error ? err.message : String(err) };
      }
    }
    if (typeof onGate === 'function') {
      try { onGate({ id, ...gates[id] }); } catch (_e) {}
    }
  }
  const endedAt = new Date().toISOString();
  const ready = GATE_IDS.every((id) => gates[id].status === 'PASSED');
  const result = { runId, epoch, startedAt, endedAt, ready, gates };
  validateRunResult(result);
  return Object.freeze(result);
}
