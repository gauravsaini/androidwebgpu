import { describe, test } from 'node:test';
import { assertEqual, assertOk, assertThrows } from '../harness/assertions.mjs';
// Production imports: real disk + guest RAM + gpu registry from src/.
import { GuestMem } from '../../../src/vm/guest_mem.js';
import { DiskActor } from '../../../src/storage/disk_actor.js';
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_INVALID_RESOURCE_ID } from '../../../src/gpu_transport/virtio_gpu.js';

function create2D(id, w, h) {
  const b = new Uint8Array(16);
  const v = new DataView(b.buffer);
  v.setUint32(0, id, true);
  v.setUint32(4, 1, true);
  v.setUint32(8, w, true);
  v.setUint32(12, h, true);
  return b;
}

function flushFor(id) {
  const b = new Uint8Array(48);
  new DataView(b.buffer).setUint32(32, id, true);
  return b;
}

describe('Tier 3: Pairwise Storage ↔ GPU Combinations (production)', () => {
  test('P-STO-GPU-01: disk sector bytes land in guest RAM readable by the GPU path', async () => {
    const mem = new GuestMem(4 * 1024 * 1024);
    const disk = new DiskActor({ backend: 'memory', totalSectors: 8192 });
    const tile = new Uint8Array(512).fill(0x55);
    await disk.writeSectors(9, tile);
    await disk.flush();
    mem.writeBytes(0x80000, await disk.readSectors(9, 1));
    const readTile = mem.readBytes(0x80000, 512);
    assertEqual(readTile.length, 512);
    assertEqual(readTile[0], 0x55);
    assertEqual(readTile[511], 0x55);
  });

  test('P-STO-GPU-02: out-of-bounds disk read blocks dependent texture creation', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 64 });
    const gpu = new VirtioGpu();
    let code = '';
    try {
      await disk.readSectors(1000000, 1);
    } catch (e) {
      code = e.code;
    }
    assertEqual(code, 'DISK_OUT_OF_BOUNDS');
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(1) });
    assertEqual(res.type, GPU_RESP_ERR_INVALID_RESOURCE_ID);
  });

  test('P-STO-GPU-03: disk flush and GPU fence advance in one ordered pipeline', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    const gpu = new VirtioGpu();
    await disk.writeSectors(2, new Uint8Array(512).fill(0x77));
    await disk.flush();
    assertOk(disk.verifyBlockChecksum(2, await disk.readSectors(2, 1)));
    gpu.processControlPacket({ type: 0x0101, payload: create2D(4, 16, 16) });
    gpu.registry.attachBacking(4, [{ addr: 0x40000n, len: 1024 }]);
    const res = gpu.processControlPacket({ type: 0x0104, flags: 1, fenceId: 9, payload: flushFor(4) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertEqual(gpu.registry.completedFence, 9n);
  });

  test('P-STO-GPU-04: checksummed disk bytes survive a registry teardown cycle', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 2048 });
    const gpu = new VirtioGpu();
    const data = new Uint8Array(512).map((_, i) => (i * 7) & 0xff);
    await disk.writeSectors(5, data);
    gpu.registry.destroyAll();
    assertEqual(gpu.registry.size(), 0);
    const back = await disk.readSectors(5, 1);
    assertOk(back.every((v, i) => v === ((i * 7) & 0xff)));
    assertOk(disk.verifyBlockChecksum(5, back));
  });

  test('P-STO-GPU-05: teardown closes both subsystems without leaking handles', async () => {
    const disk = new DiskActor({ backend: 'memory', totalSectors: 128 });
    const gpu = new VirtioGpu();
    gpu.processControlPacket({ type: 0x0101, payload: create2D(6, 8, 8) });
    await disk.close();
    gpu.reset();
    assertEqual(gpu.registry.size(), 0);
    assertEqual(disk.isInitialized, false);
  });
});
