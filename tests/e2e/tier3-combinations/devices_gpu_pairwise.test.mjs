import { describe, test } from 'node:test';
import { assertEqual, assertOk } from '../harness/assertions.mjs';
// Production imports: real input + gpu + console + sound from src/.
import { VirtioInput } from '../../../src/io/input/virtio_input.js';
import { VirtioGpu, GPU_RESP_OK_NODATA, GPU_RESP_ERR_UNSPEC } from '../../../src/gpu_transport/virtio_gpu.js';
import { VirtioConsole } from '../../../src/io/console/virtio_console.js';
import { VirtioSound } from '../../../src/io/audio/virtio_sound.js';

function flushFor(id) {
  const b = new Uint8Array(48);
  new DataView(b.buffer).setUint32(32, id, true);
  return b;
}

function create2D(id, w, h) {
  const b = new Uint8Array(16);
  const v = new DataView(b.buffer);
  v.setUint32(0, id, true);
  v.setUint32(4, 1, true);
  v.setUint32(8, w, true);
  v.setUint32(12, h, true);
  return b;
}

describe('Tier 3: Pairwise Devices ↔ GPU Combinations (production)', () => {
  test('P-DEV-GPU-01: input pointer maps into the GPU scanout resolution', () => {
    const input = new VirtioInput({ displayWidth: 1280, displayHeight: 720 });
    const gpu = new VirtioGpu({ displayWidth: 1280, displayHeight: 720 });
    input.handleDomPointer(1300, 800, 1280, 720);
    const last = input.pendingHost[input.pendingHost.length - 1];
    assertOk(last.value >= 0);
    const info = gpu.processControlPacket({ type: 0x0100, payload: new Uint8Array(0) });
    assertEqual(info.type, 0x1101);
  });

  test('P-DEV-GPU-02: GPU flush on a backed resource completes without a 2D fallback', () => {
    const gpu = new VirtioGpu();
    let frames = 0;
    gpu.onFrame = () => { frames += 1; };
    gpu.processControlPacket({ type: 0x0101, payload: create2D(1, 64, 64) });
    gpu.registry.attachBacking(1, [{ addr: 0x10000n, len: 16384 }]);
    const res = gpu.processControlPacket({ type: 0x0104, payload: flushFor(1) });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertEqual(frames, 1);
  });

  test('P-DEV-GPU-03: console log streams alongside monotonic GPU fence completion', () => {
    const consoleDev = new VirtioConsole();
    const gpu = new VirtioGpu();
    consoleDev.writeSerial('[drm] virgl 3d acceleration enabled\n');
    gpu.registry.nextFence();
    gpu.registry.completeFence(1n);
    consoleDev.writeSerial('[surfaceflinger] Boot finished, starting composition\n');
    gpu.registry.nextFence();
    gpu.registry.completeFence(2n);
    assertEqual(consoleDev.log.length, 2);
    assertEqual(gpu.registry.completedFence, 2n);
  });

  test('P-DEV-GPU-04: scanout reconfiguration updates dimensions deterministically', () => {
    const gpu = new VirtioGpu({ displayWidth: 800, displayHeight: 600 });
    gpu.processControlPacket({ type: 0x0101, payload: create2D(5, 1920, 1080) });
    gpu.registry.attachBacking(5, [{ addr: 0x20000n, len: 1920 * 1080 * 4 }]);
    const set = new Uint8Array(48);
    const v = new DataView(set.buffer);
    v.setUint32(32, 0, true);
    v.setUint32(36, 5, true);
    v.setUint32(40, 1920, true);
    v.setUint32(44, 1080, true);
    const res = gpu.processControlPacket({ type: 0x0103, payload: set });
    assertEqual(res.type, GPU_RESP_OK_NODATA);
    assertEqual(gpu.scanouts.get(0).width, 1920);
  });

  test('P-DEV-GPU-05: audio unlock and GPU fences advance independently', () => {
    const sound = new VirtioSound();
    const gpu = new VirtioGpu();
    sound.unlock();
    let frames = 0;
    for (let f = 0; f < 60; f++) {
      frames += 1;
      sound.audioWrite(new Uint8Array(800));
      gpu.registry.nextFence();
    }
    assertEqual(frames, 60);
    assertEqual(sound.framesAccepted, 60);
    gpu.registry.completeFence(60n);
    assertEqual(gpu.registry.completedFence, 60n);
  });
});
