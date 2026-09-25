/**
 * challenger_m1_virtio_fuzz.test.mjs
 * Empirical Challenger stress & fuzz harness for M1:
 * - GuestMem 64-bit bounds & wrap-around
 * - Virtqueue circular chains & graph loops
 * - Virtqueue large queue indices & 16-bit ring index wrap
 * - Pseudo-random adversarial fuzz generator (10,000 randomized vectors)
 * - Edge case explorations (uninitialized rings, multi-buffer boundary crossings)
 */

import assert from 'node:assert/strict';
import {
  GuestMem,
  GuestMemError,
  DmaOutOfBoundsError,
  DmaAlignmentError
} from '../../src/vm/guest_mem.js';
import {
  Virtqueue,
  VirtqueueError,
  VRING_DESC_F_NEXT,
  VRING_DESC_F_WRITE,
  VRING_DESC_F_INDIRECT,
  VRING_AVAIL_F_NO_INTERRUPT,
  VRING_USED_F_NO_NOTIFY,
  VRING_DESC_SIZE
} from '../../src/virtio/virtqueue.js';

let totalTests = 0;
let passedTests = 0;
let challengeStats = {
  wrapAroundVectorsTested: 0,
  circularChainsTested: 0,
  largeIndicesTested: 0,
  fuzzIterations: 0,
  expectedRejections: 0,
  cleanCompletions: 0
};

function runTest(name, fn) {
  totalTests++;
  try {
    fn();
    passedTests++;
    console.log(`  ✓ ${name}`);
  } catch (err) {
    console.error(`  ✗ FAIL: ${name}`);
    console.error(err);
    throw err;
  }
}

console.log('\n=== EMPIRICAL CHALLENGER: M1 GUESTMEM & VIRTQUEUE HARNESS ===\n');

// ============================================================================
// CHALLENGE 1: ADDRESS WRAP-AROUND & EXTREME BOUNDARIES
// ============================================================================
console.log('--- Challenge 1: Address Wrap-Around & Extreme Bounds ---');

runTest('CHAL-1.1: 64-bit integer wrap-around arithmetic in GuestMem.validateRange', () => {
  const ramSize = 16 * 1024 * 1024; // 16 MB
  const mem = new GuestMem(ramSize);

  const wrapVectors = [
    [0xFFFFFFFFFFFFFFFFn, 1n, '2^64 - 1 + 1 wrap'],
    [0xFFFFFFFFFFFFFFF0n, 32n, '2^64 - 16 + 32 wrap'],
    [0xFFFFFFFFFFFFFFF0n, 0xFFFFFFFFFFFFFFFFn, 'massive 64-bit overflow'],
    [0n, 0xFFFFFFFFFFFFFFFFn, '0 + 2^64 - 1'],
    [0xFFFFFFFFFFFFFFFFn, 0n, '2^64 - 1 + 0'],
    [BigInt(ramSize), 1n, 'exact ramSize + 1'],
    [BigInt(ramSize) - 1n, 2n, 'boundary span past ramSize'],
    [BigInt(ramSize) + 1000n, 10n, 'far out of bounds'],
    [18446744073709551616n, 1n, '2^64 exact (65-bit)'],
    [18446744073709551616n * 2n, 1n, '2^65'],
    [-1n, 1n, 'negative paddr BigInt'],
    [0n, -1n, 'negative len BigInt'],
    [-100n, -100n, 'negative both BigInt']
  ];

  for (const [paddr, len, desc] of wrapVectors) {
    challengeStats.wrapAroundVectorsTested++;
    assert.throws(
      () => mem.validateRange(paddr, len),
      (err) => err instanceof DmaOutOfBoundsError,
      `Expected DmaOutOfBoundsError for vector: ${desc} (paddr=${paddr}, len=${len})`
    );
  }
});

runTest('CHAL-1.2: Boundary edge cases (0-length at limits, exact upper bound)', () => {
  const ramSize = 1024;
  const mem = new GuestMem(ramSize);

  // Exact last byte
  mem.writeU8(1023, 0xEE);
  assert.equal(mem.readU8(1023), 0xEE);

  // Offset at ramSize with length 0 is valid (empty range [ramSize, ramSize))
  assert.equal(mem.validateRange(1024, 0), 1024);
  const emptySlice = mem.readBytes(1024, 0);
  assert.equal(emptySlice.byteLength, 0);

  // Offset at ramSize with length 1 MUST throw
  assert.throws(() => mem.validateRange(1024, 1), DmaOutOfBoundsError);
  // Offset past ramSize with length 0 MUST throw
  assert.throws(() => mem.validateRange(1025, 0), DmaOutOfBoundsError);
});

runTest('CHAL-1.3: Number type coercion and invalid types in GuestMem', () => {
  const mem = new GuestMem(1024 * 1024);

  const invalidInputs = [
    [NaN, 10],
    [10, NaN],
    [Infinity, 10],
    [10, Infinity],
    [-Infinity, 10],
    [null, 10], // BigInt(null) -> 0n
    [undefined, 10],
    ['invalid', 10],
    [Symbol('bad'), 10]
  ];

  for (const [p, l] of invalidInputs) {
    challengeStats.wrapAroundVectorsTested++;
    if (p === null) {
      // null coerces to 0n, so validateRange(0, 10) should succeed
      assert.equal(mem.validateRange(p, l), 0);
    } else if (typeof p === 'symbol' || typeof l === 'symbol') {
      // Symbol causes TypeError during formatting inside DmaOutOfBoundsError constructor
      assert.throws(
        () => mem.validateRange(p, l),
        (err) => (err instanceof DmaOutOfBoundsError) || (err instanceof TypeError)
      );
    } else {
      assert.throws(
        () => mem.validateRange(p, l),
        (err) => err instanceof DmaOutOfBoundsError,
        `Expected rejection for p=${String(p)}, l=${String(l)}`
      );
    }
  }
});

runTest('CHAL-1.4: Virtqueue 64-bit addresses in Avail/Used/Desc rings', () => {
  const ramSize = 1024 * 1024;
  const mem = new GuestMem(ramSize);
  const vq = new Virtqueue(0, 16);

  // 1. Desc table at out-of-bounds address
  vq.setDescTableHigh(0xFFFFFFFF);
  vq.setDescTableLow(0xFFFFFFFF);
  vq.setEnabled(true);
  assert.equal(vq.descTableAddr, 0xFFFFFFFFFFFFFFFFn);

  // Set avail ring within bounds so hasAvailable succeeds
  const availAddr = 0x2000n;
  vq.setAvailRingLow(Number(availAddr));
  mem.writeU16(availAddr + 2n, 1); // avail.idx = 1
  mem.writeU16(availAddr + 4n, 0); // head desc 0

  assert.equal(vq.hasAvailable(mem), true);
  // Popping must throw DmaOutOfBoundsError because descTableAddr is out of bounds
  assert.throws(() => vq.popDescriptorChain(mem), DmaOutOfBoundsError);

  // 2. Avail ring at out-of-bounds address
  const vq2 = new Virtqueue(0, 16);
  vq2.setAvailRingHigh(0xFFFFFFFF);
  vq2.setAvailRingLow(0xFFFFFFFF);
  vq2.setEnabled(true);
  assert.throws(() => vq2.hasAvailable(mem), DmaOutOfBoundsError);

  // 3. Used ring at out-of-bounds address
  const vq3 = new Virtqueue(0, 16);
  vq3.setUsedRingHigh(0xFFFFFFFF);
  vq3.setUsedRingLow(0xFFFFFFFF);
  vq3.setEnabled(true);
  assert.throws(() => vq3.pushUsed(0, 10, mem), DmaOutOfBoundsError);
});

// ============================================================================
// CHALLENGE 2: CIRCULAR CHAINS & GRAPH TOPOLOGIES
// ============================================================================
console.log('\n--- Challenge 2: Circular Chains & Graph Topologies ---');

function createQueueHelper(size = 16) {
  const ramSize = 1024 * 1024;
  const mem = new GuestMem(ramSize);
  const vq = new Virtqueue(0, size);

  const descAddr = 0x1000n;
  const availAddr = 0x2000n;
  const usedAddr = 0x3000n;

  vq.setDescTableLow(Number(descAddr));
  vq.setAvailRingLow(Number(availAddr));
  vq.setUsedRingLow(Number(usedAddr));
  vq.setEnabled(true);

  return { mem, vq, descAddr, availAddr, usedAddr, size };
}

function writeDesc(mem, descTableBase, idx, addr, len, flags, next) {
  const off = descTableBase + BigInt(idx * VRING_DESC_SIZE);
  mem.writeU64(off, BigInt(addr));
  mem.writeU32(off + 8n, len);
  mem.writeU16(off + 12n, flags);
  mem.writeU16(off + 14n, next);
}

runTest('CHAL-2.1: Self-loop (1-cycle: 0 -> 0)', () => {
  const { mem, vq, descAddr, availAddr } = createQueueHelper(16);
  writeDesc(mem, descAddr, 0, 0x10000, 16, VRING_DESC_F_NEXT, 0);
  mem.writeU16(availAddr + 4n, 0); // head 0
  mem.writeU16(availAddr + 2n, 1); // avail.idx 1

  challengeStats.circularChainsTested++;
  assert.throws(
    () => vq.popDescriptorChain(mem),
    (err) => err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED'
  );
});

runTest('CHAL-2.2: 2-cycle ping-pong (0 -> 1 -> 0)', () => {
  const { mem, vq, descAddr, availAddr } = createQueueHelper(16);
  writeDesc(mem, descAddr, 0, 0x10000, 16, VRING_DESC_F_NEXT, 1);
  writeDesc(mem, descAddr, 1, 0x10020, 16, VRING_DESC_F_NEXT, 0);
  mem.writeU16(availAddr + 4n, 0);
  mem.writeU16(availAddr + 2n, 1);

  challengeStats.circularChainsTested++;
  assert.throws(
    () => vq.popDescriptorChain(mem),
    (err) => err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED'
  );
});

runTest('CHAL-2.3: Full queue size cycle (0 -> 1 -> 2 -> ... -> 15 -> 0)', () => {
  const { mem, vq, descAddr, availAddr, size } = createQueueHelper(16);
  for (let i = 0; i < size; i++) {
    const next = (i + 1) % size;
    writeDesc(mem, descAddr, i, 0x10000 + i * 16, 16, VRING_DESC_F_NEXT, next);
  }
  mem.writeU16(availAddr + 4n, 0);
  mem.writeU16(availAddr + 2n, 1);

  challengeStats.circularChainsTested++;
  assert.throws(
    () => vq.popDescriptorChain(mem),
    (err) => err instanceof VirtqueueError && (err.code === 'DESCRIPTOR_LOOP_DETECTED' || err.code === 'DESCRIPTOR_CHAIN_TOO_LONG')
  );
});

runTest('CHAL-2.4: Lasso cycle (tail 0 -> 1 -> 2, loop 3 -> 4 -> 5 -> 3)', () => {
  const { mem, vq, descAddr, availAddr } = createQueueHelper(16);
  writeDesc(mem, descAddr, 0, 0x10000, 16, VRING_DESC_F_NEXT, 1);
  writeDesc(mem, descAddr, 1, 0x10020, 16, VRING_DESC_F_NEXT, 2);
  writeDesc(mem, descAddr, 2, 0x10040, 16, VRING_DESC_F_NEXT, 3);
  writeDesc(mem, descAddr, 3, 0x10060, 16, VRING_DESC_F_NEXT, 4);
  writeDesc(mem, descAddr, 4, 0x10080, 16, VRING_DESC_F_NEXT, 5);
  writeDesc(mem, descAddr, 5, 0x100A0, 16, VRING_DESC_F_NEXT, 3); // loops to 3!

  mem.writeU16(availAddr + 4n, 0);
  mem.writeU16(availAddr + 2n, 1);

  challengeStats.circularChainsTested++;
  assert.throws(
    () => vq.popDescriptorChain(mem),
    (err) => err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED'
  );
});

runTest('CHAL-2.5: Indirect table loop detection (self-loop, 2-cycle, lasso)', () => {
  const { mem, vq, descAddr, availAddr } = createQueueHelper(16);
  const indirectTable = 0x20000n;

  // Case A: Indirect self-loop
  writeDesc(mem, descAddr, 0, indirectTable, 32, VRING_DESC_F_INDIRECT, 0);
  writeDesc(mem, indirectTable, 0, 0x40000, 16, VRING_DESC_F_NEXT, 0); // self-loop at index 0
  mem.writeU16(availAddr + 4n, 0);
  mem.writeU16(availAddr + 2n, 1);

  challengeStats.circularChainsTested++;
  assert.throws(
    () => vq.popDescriptorChain(mem),
    (err) => err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED'
  );

  // Case B: Indirect lasso loop (0 -> 1 -> 2 -> 1 in table of 4)
  writeDesc(mem, descAddr, 0, indirectTable, 64, VRING_DESC_F_INDIRECT, 0);
  writeDesc(mem, indirectTable, 0, 0x40000, 16, VRING_DESC_F_NEXT, 1);
  writeDesc(mem, indirectTable, 1, 0x40020, 16, VRING_DESC_F_NEXT, 2);
  writeDesc(mem, indirectTable, 2, 0x40040, 16, VRING_DESC_F_NEXT, 1); // loops to 1
  writeDesc(mem, indirectTable, 3, 0x40060, 16, 0, 0);

  challengeStats.circularChainsTested++;
  assert.throws(
    () => vq.popDescriptorChain(mem),
    (err) => err instanceof VirtqueueError && err.code === 'DESCRIPTOR_LOOP_DETECTED'
  );
});

runTest('CHAL-2.6: Valid maximal chain (exactly size elements, no loop)', () => {
  const { mem, vq, descAddr, availAddr, size } = createQueueHelper(16);
  for (let i = 0; i < size; i++) {
    const isLast = i === size - 1;
    writeDesc(
      mem,
      descAddr,
      i,
      0x10000 + i * 32,
      32,
      isLast ? 0 : VRING_DESC_F_NEXT,
      isLast ? 0 : (i + 1)
    );
  }
  mem.writeU16(availAddr + 4n, 0);
  mem.writeU16(availAddr + 2n, 1);

  const chain = vq.popDescriptorChain(mem);
  assert.notEqual(chain, null);
  assert.equal(chain.headIndex, 0);
  assert.equal(chain.readable.length, 16);
  assert.equal(chain.totalReadLen, 16 * 32);
});

// ============================================================================
// CHALLENGE 3: LARGE QUEUE INDICES & RING INDEX WRAP-AROUND
// ============================================================================
console.log('\n--- Challenge 3: Large Queue Indices & Ring Wrap-Around ---');

runTest('CHAL-3.1: Continuous 16-bit Avail/Used ring wrap-around (100,000 requests)', () => {
  const size = 16;
  const { mem, vq, descAddr, availAddr, usedAddr } = createQueueHelper(size);

  for (let i = 0; i < size; i++) {
    writeDesc(mem, descAddr, i, 0x10000 + i * 16, 16, VRING_DESC_F_WRITE, 0);
  }

  const iterations = 100_000;
  let simulatedAvailIdx = 0;

  for (let iter = 0; iter < iterations; iter++) {
    challengeStats.largeIndicesTested++;
    const descIdx = iter % size;
    const ringSlot = simulatedAvailIdx % size;

    mem.writeU16(availAddr + 4n + BigInt(ringSlot * 2), descIdx);
    simulatedAvailIdx = (simulatedAvailIdx + 1) & 0xFFFF;
    mem.writeU16(availAddr + 2n, simulatedAvailIdx);

    assert.equal(vq.hasAvailable(mem), true);
    const chain = vq.popDescriptorChain(mem);
    assert.equal(chain.headIndex, descIdx);
    assert.equal(chain.writable.length, 1);

    vq.pushUsed(chain.headIndex, 16, mem);

    assert.equal(vq.lastAvailIdx, simulatedAvailIdx);
    assert.equal(vq.lastUsedIdx, simulatedAvailIdx);
    assert.equal(mem.readU16(usedAddr + 2n), simulatedAvailIdx);
  }

  assert.equal(vq.lastAvailIdx, iterations % 65536);
  assert.equal(vq.lastUsedIdx, iterations % 65536);
});

runTest('CHAL-3.2: Multi-buffer burst crossing 16-bit wrap-around boundary (65534 -> 3)', () => {
  const size = 16;
  const { mem, vq, descAddr, availAddr, usedAddr } = createQueueHelper(size);

  for (let i = 0; i < size; i++) {
    writeDesc(mem, descAddr, i, 0x10000 + i * 16, 16, 0, 0);
  }

  // Set lastAvailIdx to 65534 (2 before wrap)
  vq.lastAvailIdx = 65534;
  vq.lastUsedIdx = 65534;

  // Driver posts 5 buffers: slots at indices 65534, 65535, 0, 1, 2
  const postedDescs = [0, 1, 2, 3, 4];
  for (let i = 0; i < postedDescs.length; i++) {
    const ringIdx = (65534 + i) & 0xFFFF;
    const slot = ringIdx % size;
    mem.writeU16(availAddr + 4n + BigInt(slot * 2), postedDescs[i]);
  }
  // Driver updates avail.idx to 3
  mem.writeU16(availAddr + 2n, 3);

  // Consume all 5 buffers in order
  for (let i = 0; i < postedDescs.length; i++) {
    challengeStats.largeIndicesTested++;
    assert.equal(vq.hasAvailable(mem), true);
    const chain = vq.popDescriptorChain(mem);
    assert.equal(chain.headIndex, postedDescs[i]);
    vq.pushUsed(chain.headIndex, 0, mem);
  }

  // After consuming 5 buffers, lastAvailIdx and lastUsedIdx should be 3
  assert.equal(vq.lastAvailIdx, 3);
  assert.equal(vq.lastUsedIdx, 3);
  assert.equal(vq.hasAvailable(mem), false);
});

runTest('CHAL-3.3: Out-of-bounds headIndex in available ring', () => {
  const { mem, vq, descAddr, availAddr, size } = createQueueHelper(16);

  const badHeads = [16, 17, 255, 1000, 65535];

  for (let i = 0; i < badHeads.length; i++) {
    challengeStats.largeIndicesTested++;
    const badHead = badHeads[i];
    const slot = vq.lastAvailIdx % size;

    mem.writeU16(availAddr + 4n + BigInt(slot * 2), badHead);
    mem.writeU16(availAddr + 2n, (vq.lastAvailIdx + 1) & 0xFFFF);

    assert.throws(
      () => vq.popDescriptorChain(mem),
      (err) => err instanceof VirtqueueError && err.code === 'HEAD_INDEX_OUT_OF_BOUNDS',
      `Expected HEAD_INDEX_OUT_OF_BOUNDS for headIndex=${badHead}`
    );

    vq.lastAvailIdx = (vq.lastAvailIdx + 1) & 0xFFFF;
  }
});

runTest('CHAL-3.4: Out-of-bounds next index in descriptor chain', () => {
  const { mem, vq, descAddr, availAddr, size } = createQueueHelper(16);

  const badNexts = [16, 17, 256, 1024, 65535];

  for (let i = 0; i < badNexts.length; i++) {
    challengeStats.largeIndicesTested++;
    const badNext = badNexts[i];
    writeDesc(mem, descAddr, 0, 0x10000, 16, VRING_DESC_F_NEXT, badNext);

    const slot = vq.lastAvailIdx % size;
    mem.writeU16(availAddr + 4n + BigInt(slot * 2), 0);
    mem.writeU16(availAddr + 2n, (vq.lastAvailIdx + 1) & 0xFFFF);

    assert.throws(
      () => vq.popDescriptorChain(mem),
      (err) => err instanceof VirtqueueError && err.code === 'OUT_OF_BOUNDS_DESCRIPTOR_NEXT',
      `Expected OUT_OF_BOUNDS_DESCRIPTOR_NEXT for next=${badNext}`
    );

    vq.lastAvailIdx = (vq.lastAvailIdx + 1) & 0xFFFF;
  }
});

runTest('CHAL-3.5: Extreme queue capacities (size=1, size=2, size=32768, non-powers of 2)', () => {
  const mem = new GuestMem(1024 * 1024);

  // Valid extreme powers of two
  const validSizes = [1, 2, 4, 8, 16, 32, 64, 128, 256, 1024, 32768];
  for (const s of validSizes) {
    const q = new Virtqueue(0, 32768);
    q.setSize(s);
    assert.equal(q.size, s);
  }

  // Invalid sizes
  const invalidSizes = [0, -1, -16, 3, 5, 7, 15, 33, 100, 1000, 32769, 65536];
  for (const s of invalidSizes) {
    challengeStats.largeIndicesTested++;
    const q = new Virtqueue(0, 32768);
    assert.throws(
      () => q.setSize(s),
      (err) => err instanceof VirtqueueError && err.code === 'INVALID_QUEUE_SIZE',
      `Expected INVALID_QUEUE_SIZE for size=${s}`
    );
  }
});

runTest('CHAL-3.6: Event index wrap-around boundary at 65535 / 0 in shouldNotify', () => {
  const { mem, vq, availAddr, usedAddr, size } = createQueueHelper(16);

  mem.writeU16(availAddr, 0);

  // Case 1: Without event_idx, respects VRING_AVAIL_F_NO_INTERRUPT
  assert.equal(vq.shouldNotify(mem, false), true);
  mem.writeU16(availAddr, VRING_AVAIL_F_NO_INTERRUPT);
  assert.equal(vq.shouldNotify(mem, false), false);

  // Case 2: With event_idx, test wrap around 65535 -> 0
  const eventOffset = usedAddr + 4n + BigInt(size * 8);

  // Scenario A: lastUsedIdx = 0, avail_event = 65535 => (0 - 65535 - 1) & 0xFFFF = 0 < 1 => true!
  vq.lastUsedIdx = 0;
  mem.writeU16(eventOffset, 65535);
  assert.equal(vq.shouldNotify(mem, true), true);

  // Scenario B: lastUsedIdx = 0, avail_event = 65534 => (0 - 65534 - 1) & 0xFFFF = 1 => false
  mem.writeU16(eventOffset, 65534);
  assert.equal(vq.shouldNotify(mem, true), false);

  // Scenario C: lastUsedIdx = 100, avail_event = 99 => true!
  vq.lastUsedIdx = 100;
  mem.writeU16(eventOffset, 99);
  assert.equal(vq.shouldNotify(mem, true), true);

  // Scenario D: lastUsedIdx = 100, avail_event = 100 => false!
  mem.writeU16(eventOffset, 100);
  assert.equal(vq.shouldNotify(mem, true), false);
});

// ============================================================================
// CHALLENGE 4: PSEUDO-RANDOM ADVERSARIAL FUZZ HARNESS (10,000 VECTORS)
// ============================================================================
console.log('\n--- Challenge 4: Adversarial Fuzz Generator (10,000 Iterations) ---');

runTest('CHAL-4.1: 10,000 pseudo-random descriptor chains stress test', () => {
  const ramSize = 2 * 1024 * 1024; // 2 MB
  const mem = new GuestMem(ramSize);
  const vq = new Virtqueue(0, 16);

  const descAddr = 0x2000n;
  const availAddr = 0x4000n;
  const usedAddr = 0x6000n;

  vq.setDescTableLow(Number(descAddr));
  vq.setAvailRingLow(Number(availAddr));
  vq.setUsedRingLow(Number(usedAddr));
  vq.setEnabled(true);

  // Seeded LCG PRNG for reproducibility
  let seed = 0x12345678;
  function nextRand() {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed;
  }
  function randRange(min, max) {
    return min + (nextRand() % (max - min + 1));
  }

  const fuzzIterations = 10_000;
  const knownErrorCodes = new Set([
    'DMA_OUT_OF_BOUNDS',
    'DMA_UNALIGNED',
    'DESCRIPTOR_LOOP_DETECTED',
    'DESCRIPTOR_CHAIN_TOO_LONG',
    'HEAD_INDEX_OUT_OF_BOUNDS',
    'OUT_OF_BOUNDS_DESCRIPTOR_NEXT',
    'MALFORMED_DESCRIPTOR_CHAIN',
    'MALFORMED_INDIRECT_DESCRIPTOR',
    'INVALID_INDIRECT_TABLE_SIZE',
    'NESTED_INDIRECT_TABLE_REJECTED'
  ]);

  for (let iter = 0; iter < fuzzIterations; iter++) {
    challengeStats.fuzzIterations++;

    for (let d = 0; d < 16; d++) {
      let addrChoice = nextRand() % 8;
      let addr;
      switch (addrChoice) {
        case 0: addr = 0n; break;
        case 1: addr = BigInt(randRange(0x10000, 0x1F0000)); break; // in-RAM
        case 2: addr = BigInt(ramSize - 16); break;                 // near boundary
        case 3: addr = BigInt(ramSize); break;                      // exact OOB
        case 4: addr = 0xFFFFFFFFn; break;                          // 32-bit max
        case 5: addr = 0xFFFFFFFFFFFFFFF0n; break;                  // 64-bit overflow
        case 6: addr = 0xFFFFFFFFFFFFFFFFn; break;                  // 64-bit max
        case 7: addr = 18446744073709551616n; break;                // 2^64
      }

      let lenChoice = nextRand() % 5;
      let len;
      switch (lenChoice) {
        case 0: len = 0; break;
        case 1: len = randRange(1, 4096); break;
        case 2: len = 16; break;
        case 3: len = 0xFFFFFFFF; break;
        case 4: len = randRange(0x10000, 0x200000); break;
      }

      let flags = 0;
      if (nextRand() % 2 === 0) flags |= VRING_DESC_F_NEXT;
      if (nextRand() % 2 === 0) flags |= VRING_DESC_F_WRITE;
      if (nextRand() % 4 === 0) flags |= VRING_DESC_F_INDIRECT;

      let next = nextRand() % 20;

      writeDesc(mem, descAddr, d, addr, len, flags, next);
    }

    const head = nextRand() % 20;
    const slot = vq.lastAvailIdx % 16;
    mem.writeU16(availAddr + 4n + BigInt(slot * 2), head);
    mem.writeU16(availAddr + 2n, (vq.lastAvailIdx + 1) & 0xFFFF);

    try {
      const chain = vq.popDescriptorChain(mem);
      if (chain) {
        challengeStats.cleanCompletions++;
        assert.ok(chain.headIndex < 16);
        assert.ok(chain.readable.length + chain.writable.length > 0);
        vq.pushUsed(chain.headIndex, chain.totalWriteLen, mem);
      }
    } catch (err) {
      challengeStats.expectedRejections++;
      if (err instanceof VirtqueueError) {
        assert.ok(
          knownErrorCodes.has(err.code),
          `Unknown VirtqueueError code: ${err.code}`
        );
      } else if (err instanceof DmaOutOfBoundsError) {
        assert.equal(err.code, 'DMA_OUT_OF_BOUNDS');
      } else {
        console.error(`Unexpected uncaught error at iteration ${iter}:`, err);
        throw err;
      }
      vq.lastAvailIdx = (vq.lastAvailIdx + 1) & 0xFFFF;
    }
  }

  console.log(`    Fuzzing summary: ${challengeStats.fuzzIterations} vectors evaluated`);
  console.log(`    - Clean completions: ${challengeStats.cleanCompletions}`);
  console.log(`    - Rejections caught safely: ${challengeStats.expectedRejections}`);
  console.log(`    - Uncaught exceptions / crashes: 0`);
});

console.log('\n=== EMPIRICAL CHALLENGE RESULTS SUMMARY ===');
console.log(`Total test suites: ${passedTests} / ${totalTests} passed`);
console.log(`Wrap-around vectors tested: ${challengeStats.wrapAroundVectorsTested}`);
console.log(`Circular chains tested: ${challengeStats.circularChainsTested}`);
console.log(`Large queue indices tested: ${challengeStats.largeIndicesTested}`);
console.log(`Fuzz iterations completed: ${challengeStats.fuzzIterations}`);
console.log('ALL EMPIRICAL CHALLENGES PASSED WITH ZERO CRASHES OR EXPLOITS.\n');
