import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real DiskActor + GuestMem + validation loop from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { DiskActor } from '../../../src/storage/disk_actor.js';
import { runValidationGates, GATE_IDS } from '../../../src/validation/validation_loop.js';

describe('Tier 4: Scenario 3 — Disk Commit & Persistence (production)', () => {
  test('SCN-DSK-01: guest SQLite header write survives flush and DMA read-back', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 8192 });
    const mem = new GuestMem(1024 * 1024);
    const testPayload = new Uint8Array(512);
    testPayload.set([0x53, 0x51, 0x4c, 0x69, 0x74, 0x65, 0x20, 0x66, 0x6f, 0x72, 0x6d, 0x61, 0x74, 0x20, 0x33, 0x00]);
    for (let i = 16; i < 512; i++) testPayload[i] = (i * 7) & 0xFF;
    await disk.writeSectors(2048, testPayload);
    await disk.flush();
    mem.writeBytes(0x10000, await disk.readSectors(2048, 1));
    const guestRead = mem.readBytes(0x10000, 512);
    assertEqual(guestRead[0], 0x53);
    assertEqual(guestRead[1], 0x51);
    assertEqual(guestRead[16], (16 * 7) & 0xFF);
    assertEqual(guestRead[511], (511 * 7) & 0xFF);
  });

  test('SCN-DSK-02: multi-sector 4KB cluster write and read verification', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 8192 });
    const clusterSectors = 8;
    const baseSector = 4096;
    for (let s = 0; s < clusterSectors; s++) {
      await disk.writeSectors(baseSector + s, new Uint8Array(512).fill(s + 1));
    }
    await disk.flush();
    for (let s = 0; s < clusterSectors; s++) {
      const data = await disk.readSectors(baseSector + s, 1);
      assertEqual(data[0], s + 1);
      assertEqual(data[511], s + 1);
    }
  });

  test('SCN-DSK-03: ordered transactions serialize concurrent writes', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 4096 });
    await disk.writeSectors(100, new Uint8Array(512).fill(0x11));
    await disk.flush();
    // Concurrent writes to the same sector serialize through the queue.
    await Promise.all([
      disk.writeSectors(100, new Uint8Array(512).fill(0x22)),
      disk.writeSectors(100, new Uint8Array(512).fill(0x33)),
    ]);
    await disk.flush();
    const recovered = await disk.readSectors(100, 1);
    assertOk(recovered[0] === 0x22 || recovered[0] === 0x33);
    assertOk(disk.verifyBlockChecksum(100, recovered));
  });

  test('SCN-DSK-04: CRC checksums detect committed content', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 4096 });
    const data = new Uint8Array(512).map((_, i) => (i * 13) & 0xff);
    await disk.writeSectors(5000 - 4096, data);
    const back = await disk.readSectors(5000 - 4096, 1);
    assertOk(disk.verifyBlockChecksum(5000 - 4096, back));
    back[0] ^= 0xff;
    assertEqual(disk.verifyBlockChecksum(5000 - 4096, back), false);
  });

  test('SCN-DSK-05: production loop marks G3 PASSED on flush+checksum proof', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 4096 });
    await disk.writeSectors(1, new Uint8Array(512).fill(0xAB));
    await disk.flush();
    const check = await disk.readSectors(1, 1);
    const persistenceVerified = check[0] === 0xAB && disk.verifyBlockChecksum(1, check);
    const probes = Object.fromEntries(GATE_IDS.map((id) => [id, async () => ({ status: 'PASSED', evidence: ['ok'], error: null })]));
    probes.g3 = async () => persistenceVerified
      ? { status: 'PASSED', evidence: ['sector-commit-checksum-verified'], error: null }
      : { status: 'BLOCKED', evidence: [], error: 'DISK_OUT_OF_BOUNDS' };
    const result = await runValidationGates({ runId: 'scn-disk-run', epoch: 1, probes });
    assertEqual(result.gates.g3.status, 'PASSED');
  });
});
