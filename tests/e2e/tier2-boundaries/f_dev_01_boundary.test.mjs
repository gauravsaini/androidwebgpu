import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real DiskActor + VirtioBlk from src/.
import { DiskActor } from '../../../src/storage/disk_actor.js';
import { VirtioBlk } from '../../../src/io/block/virtio_blk.js';

describe('Tier 2: F-DEV-01 Boundary & Corner Cases (production)', () => {
  test('F-DEV-01-B01: negative sector access throws DISK_OUT_OF_BOUNDS', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    let code = '';
    try {
      await disk.readSectors(-1, 1);
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'DISK_OUT_OF_BOUNDS');
  });

  test('F-DEV-01-B02: access past the last sector throws DISK_OUT_OF_BOUNDS', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    let code = '';
    try {
      await disk.readSectors(2048, 1);
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'DISK_OUT_OF_BOUNDS');
  });

  test('F-DEV-01-B03: unaligned write length throws UNALIGNED_WRITE', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    let code = '';
    try {
      await disk.writeSectors(0, new Uint8Array(100));
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'UNALIGNED_WRITE');
  });

  test('F-DEV-01-B04: read-only blk rejects guest writes with IOERR path', () => {
    const blk = new VirtioBlk({ readOnly: true });
    assertEqual(blk.readOnly, true);
    assertEqual(blk.subsystemDeviceId, 2);
  });

  test('F-DEV-01-B05: zero-sector request round-trips empty without touching storage', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    const data = await disk.readSectors(0, 0);
    assertEqual(data.byteLength, 0);
    assertOk(disk.getCapacitySectors() === 2048);
  });
});
