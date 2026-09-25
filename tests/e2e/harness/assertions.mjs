import assert from 'node:assert';

export function assertEqual(actual, expected, message) {
  if (message !== undefined) {
    assert.strictEqual(actual, expected, message);
  } else {
    assert.strictEqual(actual, expected);
  }
}

export function assertDeepEqual(actual, expected, message) {
  if (message !== undefined) {
    assert.deepStrictEqual(actual, expected, message);
  } else {
    assert.deepStrictEqual(actual, expected);
  }
}

export function assertOk(value, message) {
  if (message !== undefined) {
    assert.ok(value, message);
  } else {
    assert.ok(value);
  }
}

export function assertThrows(fn, expectedErr, message) {
  if (message !== undefined) {
    assert.throws(fn, expectedErr, message);
  } else {
    assert.throws(fn, expectedErr);
  }
}

export async function assertRejects(promise, expectedErr, message) {
  if (message !== undefined) {
    await assert.rejects(promise, expectedErr, message);
  } else {
    await assert.rejects(promise, expectedErr);
  }
}

export function assertContract(condition, contractId, detail) {
  if (!condition) {
    const err = new Error(`CONTRACT_VIOLATION:${contractId}${detail ? ' ' + detail : ''}`);
    err.contractId = contractId;
    throw err;
  }
}

export function assertGateStatus(gate, expectedStatus) {
  const allowed = ['PENDING', 'RUNNING', 'PASSED', 'FAILED', 'BLOCKED'];
  assertOk(allowed.includes(gate.status), `Invalid gate status: ${gate.status}`);
  assertEqual(gate.status, expectedStatus, `Expected gate status ${expectedStatus} but got ${gate.status}`);
}
