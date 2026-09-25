/**
 * resource_registry.js - Entity E12 (part): GPU resource registry.
 * Owns resource lifetimes; release on guest teardown; unmap before destroy.
 */

export class GpuResourceError extends Error {
  constructor(message, code = 'GPU_RESOURCE_INVALID') {
    super(`${message} [${code}]`);
    this.name = 'GpuResourceError';
    this.code = code;
  }
}

export class ResourceRegistry {
  constructor() {
    this.resources = new Map(); // id -> {id, format, width, height, backing, scanout}
    this.fence = 0n;
    this.completedFence = 0n;
  }

  create2D({ id, format, width, height }) {
    if (!Number.isInteger(id) || id <= 0) throw new GpuResourceError(`Invalid resource id ${id}`);
    if (this.resources.has(id)) throw new GpuResourceError(`Resource id ${id} already exists`);
    if (!(width > 0 && height > 0 && width <= 8192 && height <= 8192)) {
      throw new GpuResourceError(`Invalid resource dimensions ${width}x${height}`, 'GPU_RESOURCE_INVALID');
    }
    const rec = Object.freeze({ id, format, width, height, backing: null, scanout: null });
    this.resources.set(id, { ...rec });
    return rec;
  }

  attachBacking(id, entries) {
    const r = this.resources.get(id);
    if (!r) throw new GpuResourceError(`Invalid resource id ${id}`);
    if (!Array.isArray(entries) || entries.length === 0) throw new GpuResourceError('Empty backing list');
    r.backing = entries.map((e) => ({ addr: BigInt(e.addr), len: Number(e.len) }));
    return r.backing.length;
  }

  detachBacking(id) {
    const r = this.resources.get(id);
    if (!r) throw new GpuResourceError(`Invalid resource id ${id}`);
    r.backing = null;
  }

  unref(id) {
    const r = this.resources.get(id);
    if (!r) throw new GpuResourceError(`Invalid resource id ${id}`);
    // Unmap before destruction.
    r.backing = null;
    this.resources.delete(id);
  }

  get(id) {
    const r = this.resources.get(id);
    if (!r) throw new GpuResourceError(`Invalid resource id ${id}`);
    return r;
  }

  has(id) {
    return this.resources.has(id);
  }

  nextFence() {
    this.fence += 1n;
    return this.fence;
  }

  completeFence(id) {
    if (id <= this.completedFence) throw new GpuResourceError(`Fence ${id} out of order`, 'GPU_FENCE_OUT_OF_ORDER');
    this.completedFence = id;
  }

  destroyAll() {
    this.resources.clear();
  }

  size() {
    return this.resources.size;
  }
}
