/**
 * image_store.js - Entity E03: GuestImageStore.
 * Fetch, verify (SHA-256 + size), cache, and stream Android boot media.
 * No image is mounted before hash and size checks pass. Missing required
 * role blocks boot with the exact role in the error.
 */

export const REQUIRED_IMAGE_ROLES = Object.freeze(['kernel', 'initrd.img', 'system.img', 'vendor.img', 'product.img']);

export class ImageStoreError extends Error {
  constructor(message, code = 'IMAGE_ERROR') {
    super(message);
    this.name = 'ImageStoreError';
    this.code = code;
  }
}

async function sha256Hex(bytes) {
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    const digest = await crypto.subtle.digest('SHA-256', bytes);
    return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
  }
  const { createHash } = await import('node:crypto');
  return createHash('sha256').update(bytes).digest('hex');
}

export class GuestImageStore {
  constructor({ manifest = null, fetchImpl = null } = {}) {
    this.manifest = manifest;
    this.fetchImpl = fetchImpl || (typeof fetch !== 'undefined' ? fetch.bind(globalThis) : null);
    this.cache = new Map(); // role -> Uint8Array
  }

  setManifest(manifest) {
    this.manifest = manifest;
  }

  manifestRoles() {
    return REQUIRED_IMAGE_ROLES.slice();
  }

  async verify(role, bytes) {
    if (!this.manifest) throw new ImageStoreError('No image manifest loaded', 'IMAGE_ROLE_MISSING');
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const entry = this.manifest.images?.[role];
    if (!entry) throw new ImageStoreError(`Missing required image role: ${role}`, 'IMAGE_ROLE_MISSING');
    if (entry.size !== undefined && u8.byteLength !== Number(entry.size)) {
      throw new ImageStoreError(`Image size mismatch for ${role}: got ${u8.byteLength}, want ${entry.size}`, 'IMAGE_SIZE_MISMATCH');
    }
    if (entry.sha256) {
      const actual = await sha256Hex(u8);
      if (actual !== String(entry.sha256).toLowerCase()) {
        throw new ImageStoreError(`Image hash mismatch for ${role}`, 'IMAGE_HASH_MISMATCH');
      }
    }
    // Block-role images must be sector-aligned; kernel/initrd load into RAM
    // and carry no such requirement.
    if (u8.byteLength % 512 !== 0 && role !== 'kernel' && role !== 'initrd.img') {
      throw new ImageStoreError(`Image ${role} not 512-byte aligned`, 'IMAGE_SIZE_MISMATCH');
    }
    return { role, bytes: u8.byteLength, sha256: entry.sha256 || null };
  }

  async load(role, source) {
    // source: Uint8Array | ArrayBuffer | URL string | Request
    let bytes;
    if (source instanceof Uint8Array) bytes = source;
    else if (source instanceof ArrayBuffer) bytes = new Uint8Array(source);
    else if (typeof source === 'string') {
      if (!this.fetchImpl) throw new ImageStoreError('No fetch implementation', 'ASSET_FETCH_FAILED');
      const res = await this.fetchImpl(source);
      if (!res.ok) throw new ImageStoreError(`Fetch failed for ${role}: HTTP ${res.status}`, 'ASSET_FETCH_FAILED');
      bytes = new Uint8Array(await res.arrayBuffer());
    } else {
      throw new ImageStoreError(`Unsupported image source for ${role}`, 'IMAGE_ROLE_MISSING');
    }
    const receipt = await this.verify(role, bytes);
    this.cache.set(role, bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes));
    return receipt;
  }

  get(role) {
    const bytes = this.cache.get(role);
    if (!bytes) throw new ImageStoreError(`Missing required image role: ${role}`, 'IMAGE_ROLE_MISSING');
    return bytes;
  }

  has(role) {
    return this.cache.has(role);
  }

  missingRoles() {
    return REQUIRED_IMAGE_ROLES.filter((r) => !this.cache.has(r));
  }
}
