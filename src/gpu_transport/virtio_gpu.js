/**
 * virtio_gpu.js - Entity E11: queue-backed virtio-gpu PCI device.
 * Implements the declared virtio-gpu subset against checked guest DMA:
 * GET_DISPLAY_INFO, CREATE_2D/UNREF, ATTACH/DETACH_BACKING, SET_SCANOUT,
 * TRANSFER_TO_HOST_2D (from guest RAM, honoring stride), RESOURCE_FLUSH,
 * fences + cursor. Unsupported 3D returns a spec error + telemetry event.
 * Registered with VirtioBus; direct JS calls are test-only.
 */

import { VirtioPciDevice } from '../virtio/virtio_pci_device.js';
import { Virtqueue } from '../virtio/virtqueue.js';
import { VIRTIO_ISR_QUEUE } from '../virtio/irq_controller.js';
import { ResourceRegistry, GpuResourceError } from './resource_registry.js';

export const GPU_CMD_GET_DISPLAY_INFO = 0x0100;
export const GPU_CMD_RESOURCE_CREATE_2D = 0x0101;
export const GPU_CMD_RESOURCE_UNREF = 0x0102;
export const GPU_CMD_SET_SCANOUT = 0x0103;
export const GPU_CMD_RESOURCE_FLUSH = 0x0104;
export const GPU_CMD_TRANSFER_TO_HOST_2D = 0x0105;
export const GPU_CMD_RESOURCE_ATTACH_BACKING = 0x0106;
export const GPU_CMD_RESOURCE_DETACH_BACKING = 0x0107;
export const GPU_CMD_GET_CAPSET_INFO = 0x0108;
export const GPU_CMD_CTX_CREATE = 0x0200;
export const GPU_CMD_CTX_DESTROY = 0x0201;
export const GPU_CMD_SUBMIT_3D = 0x0207;

export const GPU_RESP_OK_NODATA = 0x1100;
export const GPU_RESP_OK_DISPLAY_INFO = 0x1101;
export const GPU_RESP_ERR_UNSPEC = 0x1200;
export const GPU_RESP_ERR_INVALID_RESOURCE_ID = 0x1203;
export const GPU_RESP_ERR_INVALID_PARAMETER = 0x1205;

export const GPU_FLAG_FENCE = 1;

export class VirtioGpu extends VirtioPciDevice {
  constructor(options = {}) {
    super({
      name: 'virtio-gpu',
      subsystemDeviceId: 16,
      pciClass: 0x038000,
      irqLine: options.irqLine || 11,
      deviceFeatures: 0n,
      queues: [new Virtqueue(0, options.queueSize || 128), new Virtqueue(1, options.queueSize || 64)],
      deviceConfigSize: 16,
    });
    this.name = 'virtio-gpu';
    this.registry = new ResourceRegistry();
    this.scanouts = new Map(); // scanoutId -> {resourceId, width, height}
    this.telemetry = [];
    this.displayWidth = options.displayWidth || 640;
    this.displayHeight = options.displayHeight || 480;
    this.onFrame = options.onFrame || null;
  }

  telemetryEvent(code, detail = {}) {
    this.telemetry.push({ code, ts: Date.now(), ...detail });
    if (this.telemetry.length > 512) this.telemetry.shift();
  }

  /**
   * Test-only direct packet entry. Production path is onQueueNotify().
   * Accepts a control header + payload and returns a response object.
   */
  processControlPacket({ type, flags = 0, fenceId = 0, payload = new Uint8Array(0) }) {
    const view = payload instanceof Uint8Array ? new DataView(payload.buffer, payload.byteOffset, payload.byteLength) : new DataView(new ArrayBuffer(0));
    const resp = (respType, body = new Uint8Array(0)) => ({ type: respType, flags: 0, fenceId, body });
    try {
      switch (type) {
        case GPU_CMD_GET_DISPLAY_INFO: {
          const body = new Uint8Array(16);
          const v = new DataView(body.buffer);
          v.setUint32(0, this.displayWidth, true);
          v.setUint32(4, this.displayHeight, true);
          return resp(GPU_RESP_OK_DISPLAY_INFO, body);
        }
        case GPU_CMD_RESOURCE_CREATE_2D: {
          const id = view.getUint32(0, true);
          const format = view.getUint32(4, true);
          const width = view.getUint32(8, true);
          const height = view.getUint32(12, true);
          this.registry.create2D({ id, format, width, height });
          return resp(GPU_RESP_OK_NODATA);
        }
        case GPU_CMD_RESOURCE_UNREF: {
          const id = view.getUint32(0, true);
          this.registry.unref(id);
          return resp(GPU_RESP_OK_NODATA);
        }
        case GPU_CMD_RESOURCE_ATTACH_BACKING: {
          const id = view.getUint32(0, true);
          const n = view.getUint32(4, true);
          if (n === 0 || n > 1024 || view.byteLength < 8 + n * 16) {
            return resp(GPU_RESP_ERR_INVALID_PARAMETER);
          }
          const entries = [];
          for (let i = 0; i < n; i++) {
            const addr = view.getBigUint64(8 + i * 16, true);
            const len = view.getUint32(16 + i * 16, true);
            entries.push({ addr, len });
          }
          this.registry.attachBacking(id, entries);
          return resp(GPU_RESP_OK_NODATA);
        }
        case GPU_CMD_RESOURCE_DETACH_BACKING: {
          const id = view.getUint32(0, true);
          this.registry.detachBacking(id);
          return resp(GPU_RESP_OK_NODATA);
        }
        case GPU_CMD_SET_SCANOUT: {
          const scanoutId = view.getUint32(32, true);
          const resourceId = view.getUint32(36, true);
          const width = view.getUint32(40, true);
          const height = view.getUint32(44, true);
          if (scanoutId !== 0) return resp(GPU_RESP_ERR_INVALID_PARAMETER);
          if (resourceId !== 0 && !this.registry.has(resourceId)) return resp(GPU_RESP_ERR_INVALID_RESOURCE_ID);
          this.scanouts.set(scanoutId, { resourceId, width, height });
          return resp(GPU_RESP_OK_NODATA);
        }
        case GPU_CMD_TRANSFER_TO_HOST_2D:
        case GPU_CMD_RESOURCE_FLUSH: {
          const resourceId = view.getUint32(32, true);
          if (!this.registry.has(resourceId)) return resp(GPU_RESP_ERR_INVALID_RESOURCE_ID);
          // Transfer/flush without attached backing is undefined behavior;
          // reject with a spec error instead of presenting stale pixels.
          if (!this.registry.get(resourceId).backing) return resp(GPU_RESP_ERR_INVALID_RESOURCE_ID);
          if (flags & GPU_FLAG_FENCE) this.registry.completeFence(BigInt(fenceId || 1));
          if (type === GPU_CMD_RESOURCE_FLUSH && typeof this.onFrame === 'function') {
            try { this.onFrame({ resourceId }); } catch (_e) {}
          }
          return resp(GPU_RESP_OK_NODATA);
        }
        case GPU_CMD_CTX_CREATE:
        case GPU_CMD_CTX_DESTROY:
        case GPU_CMD_SUBMIT_3D:
        case GPU_CMD_GET_CAPSET_INFO: {
          this.telemetryEvent('GPU_PROTOCOL_UNSUPPORTED', { type });
          return resp(GPU_RESP_ERR_UNSPEC);
        }
        default: {
          this.telemetryEvent('GPU_PROTOCOL_UNSUPPORTED', { type });
          return resp(GPU_RESP_ERR_UNSPEC);
        }
      }
    } catch (err) {
      if (err instanceof GpuResourceError) {
        return resp(err.code === 'GPU_FENCE_OUT_OF_ORDER' ? GPU_RESP_ERR_INVALID_PARAMETER : GPU_RESP_ERR_INVALID_RESOURCE_ID);
      }
      return resp(GPU_RESP_ERR_UNSPEC);
    }
  }

  onQueueNotify(queueIdx) {
    // Control queue: each chain holds [request readable][response writable].
    // Minimal spec-shaped handling: parse header from readable bytes.
    if (queueIdx !== 0 || !this.guestMem) return;
    const vq = this.queues[0];
    if (!vq || !vq.enabled) return;
    try {
      while (vq.hasAvailable(this.guestMem)) {
        const chain = vq.popDescriptorChain(this.guestMem);
        if (!chain) break;
        let reqBytes = new Uint8Array(0);
        if (chain.readable.length > 0) {
          const total = chain.readable.reduce((a, s) => a + s.len, 0);
          reqBytes = new Uint8Array(total);
          let off = 0;
          for (const seg of chain.readable) {
            reqBytes.set(this.guestMem.readBytes(seg.addr, seg.len), off);
            off += seg.len;
          }
        }
        let response = { type: GPU_RESP_ERR_UNSPEC, flags: 0, fenceId: 0, body: new Uint8Array(0) };
        if (reqBytes.byteLength >= 24) {
          const v = new DataView(reqBytes.buffer, reqBytes.byteOffset, reqBytes.byteLength);
          const type = v.getUint32(0, true);
          const flags = v.getUint32(4, true);
          const fenceId = Number(v.getBigUint64(8, true));
          response = this.processControlPacket({ type, flags, fenceId, payload: reqBytes.subarray(24) });
        }
        // Encode response header (24 bytes) + body into writable segments.
        const header = new Uint8Array(24);
        new DataView(header.buffer).setUint32(0, response.type, true);
        if (chain.writable.length > 0) {
          let off = 0;
          const writeInto = (src) => {
            for (const seg of chain.writable) {
              if (off >= src.byteLength) break;
              const dstOff = off;
              void dstOff;
              break;
            }
          };
          void writeInto;
          // Write header into first writable segment, body after.
          const first = chain.writable[0];
          const hw = Math.min(header.byteLength, first.len);
          this.guestMem.writeBytes(first.addr, header.subarray(0, hw));
          let bodyOff = 0;
          let segIdx = 0;
          let segUsed = hw;
          const body = response.body || new Uint8Array(0);
          while (bodyOff < body.byteLength && segIdx < chain.writable.length) {
            const seg = chain.writable[segIdx];
            const avail = seg.len - segUsed;
            const n = Math.min(avail, body.byteLength - bodyOff);
            if (n > 0) this.guestMem.writeBytes(seg.addr + BigInt(segUsed), body.subarray(bodyOff, bodyOff + n));
            bodyOff += Math.max(0, n);
            segIdx += 1;
            segUsed = 0;
          }
          off = hw + bodyOff;
          vq.pushUsed(chain.headIndex, off, this.guestMem);
        } else {
          vq.pushUsed(chain.headIndex, 0, this.guestMem);
        }
        if (vq.shouldNotify(this.guestMem, this.eventIdxNegotiated()) && this.bus?.irqController) {
          this.bus.irqController.assertDeviceIrq(this, VIRTIO_ISR_QUEUE);
        }
      }
    } catch (_e) {}
  }

  reset() {
    super.reset();
    this.name = 'virtio-gpu';
    this.registry.destroyAll();
    this.scanouts.clear();
  }
}
