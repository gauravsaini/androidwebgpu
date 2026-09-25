import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production import: real fence registry from src/.
import { ResourceRegistry } from '../../../src/gpu_transport/resource_registry.js';

describe('Tier 2: F-GPU-05 Boundary & Corner Cases (production)', () => {
  test('F-GPU-05-B01: fence ID zero on a fresh registry is rejected', () => {
    const reg = new ResourceRegistry();
    let rejected = false;
    try {
      reg.completeFence(0n);
    } catch (e) {
      rejected = e.code === 'GPU_FENCE_OUT_OF_ORDER';
    }
    assertEqual(rejected, true);
  });

  test('F-GPU-05-B02: non-monotonic fence IDs are rejected', () => {
    const reg = new ResourceRegistry();
    reg.completeFence(10n);
    let rejected = false;
    try {
      reg.completeFence(9n);
    } catch (e) {
      rejected = e.code === 'GPU_FENCE_OUT_OF_ORDER';
    }
    assertEqual(rejected, true);
    assertEqual(reg.completedFence, 10n);
  });

  test('F-GPU-05-B03: 64-bit maximum fence ID completes exactly once', () => {
    const reg = new ResourceRegistry();
    const max = (1n << 64n) - 1n;
    reg.completeFence(max);
    assertEqual(reg.completedFence, max);
    let rejected = false;
    try {
      reg.completeFence(max);
    } catch (_e) {
      rejected = true;
    }
    assertEqual(rejected, true);
  });

  test('F-GPU-05-B04: nextFence stays monotonic across 1000 allocations', () => {
    const reg = new ResourceRegistry();
    let prev = reg.nextFence();
    for (let i = 0; i < 1000; i++) {
      const cur = reg.nextFence();
      assertOk(cur > prev);
      prev = cur;
    }
  });

  test('F-GPU-05-B05: fence and resource ID spaces are independent', () => {
    const reg = new ResourceRegistry();
    reg.create2D({ id: 1, format: 1, width: 4, height: 4 });
    reg.completeFence(1n);
    assertEqual(reg.has(1), true);
    assertEqual(reg.completedFence, 1n);
  });
});
