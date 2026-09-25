import { describe, test } from 'node:test';
import { assertEqual, assertDeepEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real DiskActor + VirtioBlk constants from src/.
import { DiskActor, DEFAULT_SECTOR_SIZE } from '../../../src/storage/disk_actor.js';
import {
  VirtioBlk,
  VIRTIO_BLK_T_IN,
  VIRTIO_BLK_T_OUT,
  VIRTIO_BLK_T_FLUSH,
  VIRTIO_BLK_S_OK,
} from '../../../src/io/block/virtio_blk.js';

describe('Tier 1: F-DEV-01 VirtioBlk Device with Persistent Storage (production)', () => {
  test('F-DEV-01-01: sector size is 512 and device identity is virtio-blk', () => {
    assertEqual(DEFAULT_SECTOR_SIZE, 512);
    const blk = new VirtioBlk();
    assertEqual(blk.name, 'virtio-blk');
    assertEqual(blk.subsystemDeviceId, 2);
    assertEqual(VIRTIO_BLK_S_OK, 0);
  });

  test('F-DEV-01-02: write then read round-trips sector bytes', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    const data = new Uint8Array(512).map((_, i) => i & 0xff);
    await disk.writeSectors(7, data);
    assertDeepEqual(await disk.readSectors(7, 1), data);
  });

  test('F-DEV-01-03: request types IN/OUT/FLUSH are distinct opcodes', () => {
    assertOk(VIRTIO_BLK_T_IN !== VIRTIO_BLK_T_OUT);
    assertOk(VIRTIO_BLK_T_OUT !== VIRTIO_BLK_T_FLUSH);
    assertOk(VIRTIO_BLK_T_IN !== VIRTIO_BLK_T_FLUSH);
  });

  test('F-DEV-01-04: flush serializes ordered writes', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    await disk.writeSectors(3, new Uint8Array(512).fill(0xAB));
    await disk.flush();
    assertDeepEqual(await disk.readSectors(3, 1), new Uint8Array(512).fill(0xAB));
  });

  test('F-DEV-01-05: unwritten sectors read back as zeros', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    assertDeepEqual(await disk.readSectors(1000, 1), new Uint8Array(512));
  });
});
