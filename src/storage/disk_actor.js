/**
 * disk_actor.js - Entity E06: Storage Persistence Actor
 * Implements ordered block I/O transactions with OPFS (Origin Private File System),
 * IndexedDB fallback, and in-memory fallback for headless/Node environments.
 * Provides IEEE 802.3 CRC-32 checksum calculation and validation,
 * boundary validation, and flush serialization.
 */

export const DEFAULT_SECTOR_SIZE = 512;

export class DiskError extends Error {
  constructor(message, code = 'DISK_ERROR') {
    super(`${message} [${code}]`);
    this.name = 'DiskError';
    this.code = code;
  }
}

export class DiskBoundsError extends DiskError {
  constructor(sector, count, totalSectors) {
    super(
      `Disk access out of bounds: sector=${sector}, count=${count}, totalSectors=${totalSectors}`,
      'DISK_OUT_OF_BOUNDS'
    );
    this.sector = sector;
    this.count = count;
    this.totalSectors = totalSectors;
  }
}

// Precomputed CRC-32 table (IEEE 802.3 standard polynomial 0xEDB88320)
const CRC32_TABLE = new Uint32Array(256);
for (let i = 0; i < 256; i++) {
  let c = i;
  for (let k = 0; k < 8; k++) {
    c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
  }
  CRC32_TABLE[i] = c >>> 0;
}

/**
 * Computes standard IEEE 802.3 CRC-32 checksum over a Uint8Array.
 * @param {Uint8Array} data
 * @param {number} [prevCrc=0]
 * @returns {number} 32-bit unsigned integer
 */
export function crc32(data, prevCrc = 0) {
  let c = (prevCrc ^ 0xFFFFFFFF) >>> 0;
  for (let i = 0; i < data.byteLength; i++) {
    c = (CRC32_TABLE[(c ^ data[i]) & 0xFF] ^ (c >>> 8)) >>> 0;
  }
  return (c ^ 0xFFFFFFFF) >>> 0;
}

export class MemoryDiskBackend {
  constructor(totalBytes) {
    this.totalBytes = totalBytes;
    this.blocks = new Map(); // sectorIndex -> Uint8Array(sectorSize)
  }

  async read(offset, length) {
    const result = new Uint8Array(length);
    const startSector = Math.floor(offset / DEFAULT_SECTOR_SIZE);
    const endSector = Math.ceil((offset + length) / DEFAULT_SECTOR_SIZE);

    for (let s = startSector; s < endSector; s++) {
      const block = this.blocks.get(s);
      if (block) {
        const blkOffset = s * DEFAULT_SECTOR_SIZE;
        const copyStart = Math.max(0, offset - blkOffset);
        const copyEnd = Math.min(DEFAULT_SECTOR_SIZE, offset + length - blkOffset);
        const targetStart = Math.max(0, blkOffset - offset);
        result.set(block.subarray(copyStart, copyEnd), targetStart);
      }
    }
    return result;
  }

  async write(offset, data) {
    const startSector = Math.floor(offset / DEFAULT_SECTOR_SIZE);
    let written = 0;
    let s = startSector;

    while (written < data.byteLength) {
      let block = this.blocks.get(s);
      if (!block) {
        block = new Uint8Array(DEFAULT_SECTOR_SIZE);
        this.blocks.set(s, block);
      }
      const blkOffset = s * DEFAULT_SECTOR_SIZE;
      const copyStart = Math.max(0, (offset + written) - blkOffset);
      const toCopy = Math.min(DEFAULT_SECTOR_SIZE - copyStart, data.byteLength - written);
      block.set(data.subarray(written, written + toCopy), copyStart);
      written += toCopy;
      s++;
    }
  }

  async flush() {
    // In-memory writes are immediately visible
    return Promise.resolve();
  }

  async close() {
    this.blocks.clear();
  }
}

export class IndexedDbDiskBackend {
  constructor(dbName = 'android_vm_storage', storeName = 'disk_blocks') {
    this.dbName = dbName;
    this.storeName = storeName;
    this.db = null;
  }

  async init() {
    if (typeof indexedDB === 'undefined') {
      throw new DiskError('IndexedDB not supported in this environment', 'NOT_SUPPORTED');
    }

    return new Promise((resolve, reject) => {
      const req = indexedDB.open(this.dbName, 1);
      req.onupgradeneeded = (e) => {
        const db = e.target.result;
        if (!db.objectStoreNames.contains(this.storeName)) {
          db.createObjectStore(this.storeName, { keyPath: 'sector' });
        }
      };
      req.onsuccess = (e) => {
        this.db = e.target.result;
        resolve();
      };
      req.onerror = () => reject(req.error);
    });
  }

  async read(offset, length) {
    const result = new Uint8Array(length);
    const startSector = Math.floor(offset / DEFAULT_SECTOR_SIZE);
    const count = Math.ceil(length / DEFAULT_SECTOR_SIZE);

    return new Promise((resolve, reject) => {
      const tx = this.db.transaction([this.storeName], 'readonly');
      const store = tx.objectStore(this.storeName);

      let processed = 0;
      for (let i = 0; i < count; i++) {
        const s = startSector + i;
        const req = store.get(s);
        req.onsuccess = () => {
          if (req.result && req.result.data) {
            const blk = req.result.data;
            const blkOffset = s * DEFAULT_SECTOR_SIZE;
            const copyStart = Math.max(0, offset - blkOffset);
            const copyEnd = Math.min(DEFAULT_SECTOR_SIZE, offset + length - blkOffset);
            const targetStart = Math.max(0, blkOffset - offset);
            result.set(blk.subarray(copyStart, copyEnd), targetStart);
          }
          processed++;
          if (processed === count) {
            resolve(result);
          }
        };
        req.onerror = () => reject(req.error);
      }
      if (count === 0) resolve(result);
    });
  }

  async write(offset, data) {
    const startSector = Math.floor(offset / DEFAULT_SECTOR_SIZE);
    const count = Math.ceil(data.byteLength / DEFAULT_SECTOR_SIZE);

    return new Promise((resolve, reject) => {
      const tx = this.db.transaction([this.storeName], 'readwrite');
      const store = tx.objectStore(this.storeName);

      let written = 0;
      for (let i = 0; i < count; i++) {
        const s = startSector + i;
        const slice = data.subarray(written, Math.min(written + DEFAULT_SECTOR_SIZE, data.byteLength));
        let blockData = slice;
        if (slice.byteLength < DEFAULT_SECTOR_SIZE) {
          blockData = new Uint8Array(DEFAULT_SECTOR_SIZE);
          blockData.set(slice);
        }
        store.put({ sector: s, data: blockData });
        written += DEFAULT_SECTOR_SIZE;
      }

      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(new DiskError('IndexedDB write transaction aborted', 'TX_ABORTED'));
    });
  }

  async flush() {
    // IndexedDB commits on transaction completion; flush ensures idle
    return Promise.resolve();
  }

  async close() {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}

export class OpfsDiskBackend {
  constructor(fileName = 'android_disk.img') {
    this.fileName = fileName;
    this.fileHandle = null;
    this.accessHandle = null;
  }

  async init() {
    if (typeof navigator === 'undefined' || !navigator.storage || !navigator.storage.getDirectory) {
      throw new DiskError('OPFS not supported in this environment', 'NOT_SUPPORTED');
    }
    const root = await navigator.storage.getDirectory();
    this.fileHandle = await root.getFileHandle(this.fileName, { create: true });
    if (typeof this.fileHandle.createSyncAccessHandle === 'function') {
      this.accessHandle = await this.fileHandle.createSyncAccessHandle();
    }
  }

  async read(offset, length) {
    const buffer = new Uint8Array(length);
    if (this.accessHandle) {
      this.accessHandle.read(buffer, { at: offset });
      return buffer;
    }
    const file = await this.fileHandle.getFile();
    const slice = file.slice(offset, offset + length);
    const ab = await slice.arrayBuffer();
    buffer.set(new Uint8Array(ab));
    return buffer;
  }

  async write(offset, data) {
    if (this.accessHandle) {
      this.accessHandle.write(data, { at: offset });
      return;
    }
    const writable = await this.fileHandle.createWritable({ keepExistingData: true });
    await writable.seek(offset);
    await writable.write(data);
    await writable.close();
  }

  async flush() {
    if (this.accessHandle) {
      this.accessHandle.flush();
    }
  }

  async close() {
    if (this.accessHandle) {
      this.accessHandle.flush();
      this.accessHandle.close();
      this.accessHandle = null;
    }
  }
}

export class DiskActor {
  /**
   * @param {Object} [options]
   * @param {number} [options.sectorSize=512]
   * @param {number} [options.totalSectors=2097152] // Default 1 GB
   * @param {'auto'|'opfs'|'indexeddb'|'memory'} [options.backend='auto']
   * @param {string} [options.dbName='android_vm_storage']
   * @param {string} [options.fileName='android_disk.img']
   */
  constructor(options = {}) {
    this.sectorSize = options.sectorSize || DEFAULT_SECTOR_SIZE;
    this.totalSectors = options.totalSectors || 2097152;
    this.totalBytes = this.totalSectors * this.sectorSize;
    this.backendType = options.backend || 'auto';
    this.dbName = options.dbName || 'android_vm_storage';
    this.fileName = options.fileName || 'android_disk.img';

    /** @type {MemoryDiskBackend|IndexedDbDiskBackend|OpfsDiskBackend|null} */
    this.backend = null;
    this.isInitialized = false;

    // Transaction serialization queue
    this.transactionQueue = Promise.resolve();

    // Checksum registry: Map<sectorNumber, crc32Value>
    this.blockChecksums = new Map();
  }

  /**
   * Initialize underlying storage backend with fallback cascade.
   */
  async init() {
    if (this.backendType === 'opfs') {
      try {
        const opfs = new OpfsDiskBackend(this.fileName);
        await opfs.init();
        this.backend = opfs;
        this.backendType = 'opfs';
      } catch (_e) {
        throw new DiskError('Requested OPFS backend unavailable', 'OPFS_FAILED');
      }
    } else if (this.backendType === 'indexeddb') {
      try {
        const idb = new IndexedDbDiskBackend(this.dbName);
        await idb.init();
        this.backend = idb;
        this.backendType = 'indexeddb';
      } catch (_e) {
        throw new DiskError('Requested IndexedDB backend unavailable', 'IDB_FAILED');
      }
    } else if (this.backendType === 'memory') {
      this.backend = new MemoryDiskBackend(this.totalBytes);
      this.backendType = 'memory';
    } else {
      // Auto-detect: OPFS -> IndexedDB -> Memory
      let selected = null;
      try {
        const opfs = new OpfsDiskBackend(this.fileName);
        await opfs.init();
        selected = opfs;
        this.backendType = 'opfs';
      } catch (_opfsErr) {
        try {
          const idb = new IndexedDbDiskBackend(this.dbName);
          await idb.init();
          selected = idb;
          this.backendType = 'indexeddb';
        } catch (_idbErr) {
          selected = new MemoryDiskBackend(this.totalBytes);
          this.backendType = 'memory';
        }
      }
      this.backend = selected;
    }

    this.isInitialized = true;
  }

  validateSectorRange(sector, count) {
    const s = Number(sector);
    const c = Number(count);
    if (!Number.isFinite(s) || !Number.isFinite(c) || s < 0 || c < 0) {
      throw new DiskBoundsError(sector, count, this.totalSectors);
    }
    if (s + c > this.totalSectors) {
      throw new DiskBoundsError(sector, count, this.totalSectors);
    }
  }

  /**
   * Enqueue an ordered transaction to ensure strict serial block I/O.
   * @param {() => Promise<any>} task
   * @returns {Promise<any>}
   */
  enqueueTransaction(task) {
    const resultPromise = this.transactionQueue.then(task);
    this.transactionQueue = resultPromise.catch(() => {});
    return resultPromise;
  }

  /**
   * Read sectors from storage with boundary check.
   * @param {number|bigint} sector
   * @param {number} count
   * @returns {Promise<Uint8Array>}
   */
  async readSectors(sector, count) {
    if (!this.isInitialized) await this.init();
    const s = Number(sector);
    const c = Number(count);
    this.validateSectorRange(s, c);

    return this.enqueueTransaction(async () => {
      const offset = s * this.sectorSize;
      const length = c * this.sectorSize;
      return await this.backend.read(offset, length);
    });
  }

  /**
   * Write sectors to storage with CRC checksum calculation and boundary check.
   * @param {number|bigint} sector
   * @param {Uint8Array} data
   * @returns {Promise<void>}
   */
  async writeSectors(sector, data) {
    if (!this.isInitialized) await this.init();
    const s = Number(sector);
    if (data.byteLength % this.sectorSize !== 0) {
      throw new DiskError(
        `Write data length (${data.byteLength}) is not a multiple of sector size (${this.sectorSize})`,
        'UNALIGNED_WRITE'
      );
    }
    const count = data.byteLength / this.sectorSize;
    this.validateSectorRange(s, count);

    // Compute CRC checksums per sector
    for (let i = 0; i < count; i++) {
      const sectorBytes = data.subarray(i * this.sectorSize, (i + 1) * this.sectorSize);
      const sectorCrc = crc32(sectorBytes);
      this.blockChecksums.set(s + i, sectorCrc);
    }

    return this.enqueueTransaction(async () => {
      const offset = s * this.sectorSize;
      await this.backend.write(offset, data);
    });
  }

  /**
   * Validate CRC-32 checksum of an in-memory block against stored checksum.
   * @param {number|bigint} sector
   * @param {Uint8Array} data
   * @returns {boolean} true if valid or if no checksum recorded; false if mismatch
   */
  verifyBlockChecksum(sector, data) {
    const s = Number(sector);
    if (!this.blockChecksums.has(s)) return true;
    const expected = this.blockChecksums.get(s);
    const actual = crc32(data.subarray(0, this.sectorSize));
    return expected === actual;
  }

  /**
   * Serialize and commit all writes before returning.
   * @returns {Promise<void>}
   */
  async flush() {
    if (!this.isInitialized) await this.init();
    return this.enqueueTransaction(async () => {
      await this.backend.flush();
    });
  }

  getCapacitySectors() {
    return this.totalSectors;
  }

  getCapacityBytes() {
    return this.totalBytes;
  }

  async close() {
    await this.flush();
    if (this.backend) {
      await this.backend.close();
      this.backend = null;
    }
    this.isInitialized = false;
  }
}
