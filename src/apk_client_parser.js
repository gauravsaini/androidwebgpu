/**
 * AndroidWebGPU - Client-Side APK, AXML & ARSC Parser & PMS Registry
 * 
 * Provides pure JavaScript, zero-dependency implementations of:
 * 1. inflateRaw: Pure-JS RFC 1951 DEFLATE decompressor.
 * 2. ApkZipReader: Client-side ZIP archive unpacker (EOCD, Central Directory, Local Headers).
 * 3. AxmlDecoder: Binary Android XML (AXML) chunk decoder (RES_XML_TYPE, RES_STRING_POOL, RES_XML_RESOURCE_MAP, START/END_ELEMENT).
 * 4. ArscStringPoolParser: Binary resources.arsc table decoder and string resolver.
 * 5. PackageManagerRegistry: In-memory package manager registry with intent and provider resolution.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

export const RES_NULL_TYPE = 0x0000;
export const RES_STRING_POOL_TYPE = 0x0001;
export const RES_TABLE_TYPE = 0x0002;
export const RES_XML_TYPE = 0x0003;
export const RES_XML_START_NAMESPACE_TYPE = 0x0100;
export const RES_XML_END_NAMESPACE_TYPE = 0x0101;
export const RES_XML_START_ELEMENT_TYPE = 0x0102;
export const RES_XML_END_ELEMENT_TYPE = 0x0103;
export const RES_XML_CDATA_TYPE = 0x0104;
export const RES_XML_RESOURCE_MAP_TYPE = 0x0180;

export const RES_TABLE_PACKAGE_TYPE = 0x0200;
export const RES_TABLE_TYPE_TYPE = 0x0201;
export const RES_TABLE_TYPE_SPEC_TYPE = 0x0202;
export const RES_TABLE_LIBRARY_TYPE = 0x0203;

export const ATTR_LABEL = 0x01010001;
export const ATTR_ICON = 0x01010002;
export const ATTR_NAME = 0x01010003;
export const ATTR_PERMISSION = 0x01010006;
export const ATTR_READ_PERMISSION = 0x01010007;
export const ATTR_WRITE_PERMISSION = 0x01010008;
export const ATTR_HAS_CODE = 0x0101000c;
export const ATTR_ENABLED = 0x0101000e;
export const ATTR_EXPORTED = 0x01010010;
export const ATTR_LAUNCH_MODE = 0x01010011;
export const ATTR_MULTIPROCESS = 0x01010012;
export const ATTR_AUTHORITIES = 0x01010018;
export const ATTR_INIT_ORDER = 0x0101001a;
export const ATTR_GRANT_URI_PERMISSIONS = 0x0101001b;
export const ATTR_VERSION_CODE = 0x0101001b;
export const ATTR_VERSION_NAME = 0x0101001c;
export const ATTR_THEME = 0x01010020;
export const ATTR_MIN_SDK_VERSION = 0x0101020c;
export const ATTR_TARGET_SDK_VERSION = 0x0101021b;
export const ATTR_GLES_VERSION = 0x01010281;
export const ATTR_REQUIRED = 0x0101028e;

// -----------------------------------------------------------------------------
// 1. Pure-JavaScript DEFLATE (RFC 1951) Inflate Implementation
// -----------------------------------------------------------------------------

const LENGTH_BASE = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
    35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258
];
const LENGTH_EXTRA = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
    3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0
];
const DIST_BASE = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
    257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577
];
const DIST_EXTRA = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
    7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13
];
const CODE_ORDER = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

function buildHuffman(lengths) {
    const count = lengths.length;
    let maxLen = 0;
    for (let i = 0; i < count; i++) {
        if (lengths[i] > maxLen) maxLen = lengths[i];
    }
    if (maxLen === 0) return { table: new Int32Array(1).fill(-1), maxLen: 0 };

    const blCount = new Uint16Array(maxLen + 1);
    for (let i = 0; i < count; i++) blCount[lengths[i]]++;

    const nextCode = new Uint16Array(maxLen + 1);
    let code = 0;
    blCount[0] = 0;
    for (let bits = 1; bits <= maxLen; bits++) {
        code = (code + blCount[bits - 1]) << 1;
        nextCode[bits] = code;
    }

    const tableSize = 1 << maxLen;
    const table = new Int32Array(tableSize).fill(-1);
    for (let i = 0; i < count; i++) {
        const len = lengths[i];
        if (len === 0) continue;
        const c = nextCode[len]++;
        let rev = 0;
        for (let j = 0; j < len; j++) {
            rev = (rev << 1) | ((c >> j) & 1);
        }
        const step = 1 << len;
        for (let idx = rev; idx < tableSize; idx += step) {
            table[idx] = (i << 4) | len;
        }
    }
    return { table, maxLen };
}

const fixedLitLengths = new Uint8Array(288);
for (let i = 0; i <= 143; i++) fixedLitLengths[i] = 8;
for (let i = 144; i <= 255; i++) fixedLitLengths[i] = 9;
for (let i = 256; i <= 279; i++) fixedLitLengths[i] = 7;
for (let i = 280; i <= 287; i++) fixedLitLengths[i] = 8;
const fixedLitTable = buildHuffman(fixedLitLengths);

const fixedDistLengths = new Uint8Array(32);
for (let i = 0; i < 32; i++) fixedDistLengths[i] = 5;
const fixedDistTable = buildHuffman(fixedDistLengths);

/**
 * Decompresses raw DEFLATE byte streams (RFC 1951).
 * @param {Uint8Array} input - Compressed byte buffer.
 * @param {number} [outputLength=65536] - Expected output buffer capacity hint.
 * @returns {Uint8Array} Decompressed byte stream.
 */
export function inflateRaw(input, outputLength = 65536) {
    if (!input || input.length === 0) return new Uint8Array(0);

    let inPos = 0;
    let bitBuf = 0;
    let bitLen = 0;

    function getBits(n) {
        while (bitLen < n) {
            if (inPos >= input.length) return 0;
            bitBuf |= input[inPos++] << bitLen;
            bitLen += 8;
        }
        const val = bitBuf & ((1 << n) - 1);
        bitBuf >>>= n;
        bitLen -= n;
        return val;
    }

    function decodeSymbol(huff) {
        if (huff.maxLen === 0) {
            throw new Error("Invalid Huffman code in DEFLATE stream (empty tree)");
        }
        while (bitLen < huff.maxLen) {
            if (inPos >= input.length) break;
            bitBuf |= input[inPos++] << bitLen;
            bitLen += 8;
        }
        const masked = bitBuf & ((1 << huff.maxLen) - 1);
        const entry = huff.table[masked];
        if (entry === -1) throw new Error("Invalid Huffman code in DEFLATE stream");
        const symbol = entry >> 4;
        const len = entry & 0xF;
        if (len === 0 || (inPos >= input.length && bitLen < len)) {
            throw new Error("Unexpected end of DEFLATE stream");
        }
        bitBuf >>>= len;
        bitLen -= len;
        return symbol;
    }

    let out = new Uint8Array(Math.max(outputLength, 1024));
    let outPos = 0;

    function ensureCapacity(needed) {
        if (outPos + needed <= out.length) return;
        const newCap = Math.max(out.length * 2, outPos + needed + 4096);
        const newOut = new Uint8Array(newCap);
        newOut.set(out.subarray(0, outPos));
        out = newOut;
    }

    let isLast = false;
    while (!isLast) {
        isLast = getBits(1) === 1;
        const btype = getBits(2);
        if (btype === 0) {
            bitBuf = 0;
            bitLen = 0;
            if (inPos + 4 > input.length) break;
            const len = input[inPos] | (input[inPos + 1] << 8);
            inPos += 4;
            ensureCapacity(len);
            out.set(input.subarray(inPos, inPos + len), outPos);
            inPos += len;
            outPos += len;
        } else if (btype === 1 || btype === 2) {
            let litTable, distTable;
            if (btype === 1) {
                litTable = fixedLitTable;
                distTable = fixedDistTable;
            } else {
                const hlit = getBits(5) + 257;
                const hdist = getBits(5) + 1;
                const hclen = getBits(4) + 4;
                const codeLengths = new Uint8Array(19);
                for (let i = 0; i < hclen; i++) {
                    codeLengths[CODE_ORDER[i]] = getBits(3);
                }
                const codeTable = buildHuffman(codeLengths);

                const totalCodes = hlit + hdist;
                const treeLengths = new Uint8Array(totalCodes);
                let idx = 0;
                while (idx < totalCodes) {
                    const sym = decodeSymbol(codeTable);
                    if (sym < 16) {
                        treeLengths[idx++] = sym;
                    } else if (sym === 16) {
                        const repeat = getBits(2) + 3;
                        const val = idx > 0 ? treeLengths[idx - 1] : 0;
                        for (let r = 0; r < repeat; r++) treeLengths[idx++] = val;
                    } else if (sym === 17) {
                        const repeat = getBits(3) + 3;
                        for (let r = 0; r < repeat; r++) treeLengths[idx++] = 0;
                    } else if (sym === 18) {
                        const repeat = getBits(7) + 11;
                        for (let r = 0; r < repeat; r++) treeLengths[idx++] = 0;
                    }
                }
                litTable = buildHuffman(treeLengths.subarray(0, hlit));
                distTable = buildHuffman(treeLengths.subarray(hlit));
            }

            while (true) {
                const symbol = decodeSymbol(litTable);
                if (symbol < 256) {
                    ensureCapacity(1);
                    out[outPos++] = symbol;
                } else if (symbol === 256) {
                    break;
                } else {
                    const lenIdx = symbol - 257;
                    if (lenIdx < 0 || lenIdx >= LENGTH_BASE.length) {
                        throw new Error("Invalid match length code in DEFLATE stream");
                    }
                    const length = LENGTH_BASE[lenIdx] + (LENGTH_EXTRA[lenIdx] ? getBits(LENGTH_EXTRA[lenIdx]) : 0);
                    const distIdx = decodeSymbol(distTable);
                    if (distIdx < 0 || distIdx >= DIST_BASE.length) {
                        throw new Error("Invalid distance code in DEFLATE stream");
                    }
                    const dist = DIST_BASE[distIdx] + (DIST_EXTRA[distIdx] ? getBits(DIST_EXTRA[distIdx]) : 0);
                    if (dist > outPos) {
                        throw new Error("Invalid distance reference before start of output buffer");
                    }
                    ensureCapacity(length);
                    let src = outPos - dist;
                    for (let k = 0; k < length; k++) {
                        out[outPos++] = out[src++];
                    }
                }
            }
        } else {
            throw new Error(`Unsupported DEFLATE block type: ${btype}`);
        }
    }

    return out.subarray(0, outPos);
}

// -----------------------------------------------------------------------------
// 2. Client-Side APK ZIP Archive Reader (ApkZipReader)
// -----------------------------------------------------------------------------

export class ApkZipReader {
    /**
     * @param {ArrayBuffer | Uint8Array} buffer - Raw APK buffer.
     */
    constructor(buffer) {
        if (buffer instanceof ArrayBuffer) {
            this.bytes = new Uint8Array(buffer);
        } else if (buffer && buffer.buffer) {
            this.bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        } else {
            throw new Error("Invalid buffer provided to ApkZipReader");
        }
        this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
        this.entries = new Map();
        this.isParsed = false;
    }

    /**
     * Parses Central Directory entries from the ZIP archive.
     */
    readEntries() {
        if (this.isParsed) return this.entries;

        const totalBytes = this.bytes.byteLength;
        if (totalBytes < 22) throw new Error("Buffer too short to be a valid ZIP archive");

        // Scan backwards for End of Central Directory (EOCD: 0x06054b50)
        let eocdOffset = -1;
        const maxScan = Math.max(0, totalBytes - 65557);
        for (let i = totalBytes - 22; i >= maxScan; i--) {
            if (this.view.getUint32(i, true) === 0x06054b50) {
                eocdOffset = i;
                break;
            }
        }

        if (eocdOffset === -1) {
            throw new Error("Cannot find End of Central Directory (EOCD) signature");
        }

        const totalEntries = this.view.getUint16(eocdOffset + 10, true);
        const cdOffset = this.view.getUint32(eocdOffset + 16, true);

        let cursor = cdOffset;
        const textDecoder = new TextDecoder("utf-8");

        for (let i = 0; i < totalEntries && cursor + 46 <= totalBytes; i++) {
            const magic = this.view.getUint32(cursor, true);
            if (magic !== 0x02014b50) break; // Central directory file header magic

            const method = this.view.getUint16(cursor + 10, true);
            const compSize = this.view.getUint32(cursor + 20, true);
            const uncompSize = this.view.getUint32(cursor + 24, true);
            const nameLen = this.view.getUint16(cursor + 28, true);
            const extraLen = this.view.getUint16(cursor + 30, true);
            const commentLen = this.view.getUint16(cursor + 32, true);
            const localHeaderOffset = this.view.getUint32(cursor + 42, true);

            const nameBytes = this.bytes.subarray(cursor + 46, cursor + 46 + nameLen);
            const filename = textDecoder.decode(nameBytes);

            this.entries.set(filename, {
                filename,
                method,
                compressedSize: compSize,
                uncompressedSize: uncompSize,
                localHeaderOffset
            });

            cursor += 46 + nameLen + extraLen + commentLen;
        }

        this.isParsed = true;
        return this.entries;
    }

    /**
     * Lists all file names in the ZIP.
     * @returns {string[]}
     */
    listEntries() {
        this.readEntries();
        return Array.from(this.entries.keys());
    }

    /**
     * Reads and decompresses a specific file from the archive.
     * @param {string} filename - Path to file inside APK.
     * @returns {Uint8Array | null} Decompressed byte stream or null if not found.
     */
    readFile(filename) {
        this.readEntries();
        const entry = this.entries.get(filename);
        if (!entry) return null;

        const lhOff = entry.localHeaderOffset;
        if (lhOff + 30 > this.bytes.byteLength) return null;

        const lhMagic = this.view.getUint32(lhOff, true);
        if (lhMagic !== 0x04034b50) return null; // Local file header signature

        const lhNameLen = this.view.getUint16(lhOff + 26, true);
        const lhExtraLen = this.view.getUint16(lhOff + 28, true);
        const dataOffset = lhOff + 30 + lhNameLen + lhExtraLen;

        const rawSlice = this.bytes.subarray(dataOffset, dataOffset + entry.compressedSize);

        if (entry.method === 0) {
            return rawSlice;
        } else if (entry.method === 8) {
            return inflateRaw(rawSlice, entry.uncompressedSize);
        } else {
            throw new Error(`Unsupported ZIP compression method: ${entry.method}`);
        }
    }

    /**
     * Extracts AndroidManifest.xml bytes.
     * @returns {Uint8Array | null}
     */
    getManifest() {
        return this.readFile("AndroidManifest.xml");
    }

    /**
     * Extracts resources.arsc bytes.
     * @returns {Uint8Array | null}
     */
    getArsc() {
        return this.readFile("resources.arsc");
    }

    /**
     * Extracts icon asset bytes.
     * @param {string} [preferredPath] - Optional preferred path.
     * @returns {{ path: string, data: Uint8Array } | null}
     */
    getIcon(preferredPath) {
        this.readEntries();
        if (preferredPath && this.entries.has(preferredPath)) {
            const data = this.readFile(preferredPath);
            if (data) return { path: preferredPath, data };
        }

        const iconPatterns = [
            /res\/mipmap-xxxhdpi.*\/ic_launcher.*\.(png|webp)$/i,
            /res\/mipmap-xxhdpi.*\/ic_launcher.*\.(png|webp)$/i,
            /res\/mipmap-xhdpi.*\/ic_launcher.*\.(png|webp)$/i,
            /res\/mipmap-hdpi.*\/ic_launcher.*\.(png|webp)$/i,
            /res\/mipmap-mdpi.*\/ic_launcher.*\.(png|webp)$/i,
            /res\/drawable.*\/ic_launcher.*\.(png|webp)$/i,
            /res\/drawable.*\/icon.*\.(png|webp)$/i,
            /res\/mipmap.*\/ic_launcher.*\.(png|webp)$/i
        ];

        for (const pattern of iconPatterns) {
            for (const name of this.entries.keys()) {
                if (pattern.test(name)) {
                    const data = this.readFile(name);
                    if (data) return { path: name, data };
                }
            }
        }
        return null;
    }

    /**
     * Extracts all DEX file entries.
     * @returns {Array<{ name: string, data: Uint8Array }>}
     */
    getAllDexFiles() {
        this.readEntries();
        const dexFiles = [];
        for (const name of this.entries.keys()) {
            if (name.endsWith('.dex')) {
                const data = this.readFile(name);
                if (data) {
                    dexFiles.push({ name, data });
                }
            }
        }
        return dexFiles;
    }

    /**
     * Alias for readFile.
     */
    getFile(filename) {
        return this.readFile(filename);
    }
}

// -----------------------------------------------------------------------------
// 3. Binary Android XML (AXML) Parser (AxmlDecoder)
// -----------------------------------------------------------------------------

export class AxmlDecoder {
    constructor(buffer) {
        this.buffer = buffer;
    }

    parse() {
        return AxmlDecoder.decode(this.buffer);
    }

    /**
     * Decodes a binary Android XML buffer into structured manifest data.
     * @param {ArrayBuffer | Uint8Array} buffer - Binary AXML buffer.
     * @returns {object} Parsed manifest metadata.
     */
    static decode(buffer) {
        let bytes;
        if (buffer instanceof ArrayBuffer) {
            bytes = new Uint8Array(buffer);
        } else if (buffer && buffer.buffer) {
            bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        } else {
            throw new Error("Invalid buffer provided to AxmlDecoder");
        }

        if (bytes.byteLength < 8) {
            throw new Error("Buffer too short for AXML header (< 8 bytes)");
        }

        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const magic = view.getUint16(0, true);
        if (magic !== RES_XML_TYPE) {
            throw new Error(`Invalid AXML magic: expected 0x0003, got 0x${magic.toString(16)}`);
        }

        const totalSize = view.getUint32(4, true);
        const limit = Math.min(totalSize, bytes.byteLength);

        let stringPool = [];
        let resourceMap = [];

        function parseStringPool(offset, size) {
            if (size < 28) return [];
            const rawStringCount = view.getUint32(offset + 8, true);
            const stringCount = Math.min(rawStringCount, Math.floor((size - 28) / 4));
            const flags = view.getUint32(offset + 16, true);
            const stringsStart = view.getUint32(offset + 20, true);
            const isUtf8 = (flags & (1 << 8)) !== 0;

            const offsets = [];
            for (let i = 0; i < stringCount; i++) {
                const offPos = offset + 28 + i * 4;
                if (offPos + 4 <= offset + size) {
                    offsets.push(view.getUint32(offPos, true));
                }
            }

            const pool = [];
            const textDecoder = new TextDecoder("utf-8");

            for (let i = 0; i < offsets.length; i++) {
                const strAbs = offset + stringsStart + offsets[i];
                if (strAbs >= offset + size) {
                    pool.push("");
                    continue;
                }

                if (isUtf8) {
                    let cursor = strAbs;
                    if (bytes[cursor] & 0x80) cursor += 2; else cursor += 1;
                    let utf8Len = 0;
                    const b = bytes[cursor];
                    if (b & 0x80) {
                        const next = cursor + 1 < offset + size ? bytes[cursor + 1] : 0;
                        cursor += 2;
                        utf8Len = ((b & 0x7F) << 8) | next;
                    } else {
                        cursor += 1;
                        utf8Len = b;
                    }
                    const end = Math.min(cursor + utf8Len, offset + size);
                    const str = textDecoder.decode(bytes.subarray(cursor, end)).replace(/\0+$/, "");
                    pool.push(str);
                } else {
                    let cursor = strAbs;
                    let charLen = 0;
                    const lenPrefix = cursor + 2 <= offset + size ? view.getUint16(cursor, true) : 0;
                    if (lenPrefix & 0x8000) {
                        cursor += 4;
                        const next = cursor <= offset + size ? view.getUint16(cursor - 2, true) : 0;
                        charLen = ((lenPrefix & 0x7FFF) << 16) | next;
                    } else {
                        cursor += 2;
                        charLen = lenPrefix;
                    }
                    const u16 = [];
                    for (let c = 0; c < charLen; c++) {
                        if (cursor + 2 > offset + size) break;
                        const ch = view.getUint16(cursor, true);
                        if (ch === 0) break;
                        u16.push(ch);
                        cursor += 2;
                    }
                    pool.push(String.fromCharCode(...u16));
                }
            }
            return pool;
        }

        // Pass 1: String Pool and Resource Map
        let pos = 8;
        while (pos + 8 <= limit) {
            const chunkType = view.getUint16(pos, true);
            const headerSize = view.getUint16(pos + 2, true);
            const chunkSize = view.getUint32(pos + 4, true);
            if (chunkSize < 8 || pos + chunkSize > bytes.byteLength) {
                pos += 4;
                continue;
            }

            if (chunkType === RES_STRING_POOL_TYPE) {
                stringPool = parseStringPool(pos, chunkSize);
            } else if (chunkType === RES_XML_RESOURCE_MAP_TYPE) {
                const count = Math.floor((chunkSize - headerSize) / 4);
                for (let i = 0; i < count; i++) {
                    resourceMap.push(view.getUint32(pos + headerSize + i * 4, true));
                }
            }
            pos += chunkSize;
        }

        // Pass 2: XML Elements
        const manifest = {
            packageName: "",
            versionCode: 1,
            versionName: "1.0",
            applicationName: null,
            applicationLabel: "",
            applicationIcon: 0,
            applicationTheme: 0,
            hasCode: true,
            minSdkVersion: 1,
            targetSdkVersion: 33,
            launcherActivity: null,
            activities: [],
            services: [],
            receivers: [],
            providers: [],
            permissions: [],
            usesFeatures: [],
            metaData: {},
            activityCount: 0,
            serviceCount: 0,
            receiverCount: 0,
            providerCount: 0,
            permissionCount: 0
        };

        const tagStack = [];
        let currentActivity = null;
        let currentService = null;
        let currentReceiver = null;
        let currentProvider = null;
        let currentIntentFilter = null;

        pos = 8;
        while (pos + 8 <= limit) {
            const chunkType = view.getUint16(pos, true);
            const chunkSize = view.getUint32(pos + 4, true);
            if (chunkSize < 8 || pos + chunkSize > bytes.byteLength) {
                pos += 4;
                continue;
            }

            if (chunkType === RES_XML_START_ELEMENT_TYPE) {
                if (pos + 28 <= bytes.byteLength) {
                    const tagIdx = view.getUint32(pos + 20, true);
                    const tagName = stringPool[tagIdx] || `tag_${tagIdx}`;
                    const attrCount = view.getUint16(pos + 28, true);
                    const attrs = [];

                    let attrOff = pos + 36;
                    for (let i = 0; i < attrCount; i++) {
                        if (attrOff + 20 <= pos + chunkSize) {
                            const nameIdx = view.getUint32(attrOff + 4, true);
                            const rawIdx = view.getUint32(attrOff + 8, true);
                            const dataType = bytes[attrOff + 15];
                            const data = view.getUint32(attrOff + 16, true);

                            const attrName = stringPool[nameIdx] || "";
                            const rawValue = rawIdx !== 0xFFFFFFFF ? stringPool[rawIdx] : null;
                            const resId = resourceMap[nameIdx] || null;

                            let value = rawValue;
                            if (value === null) {
                                if (dataType === 3) value = stringPool[data];
                                else if (dataType === 1) value = `@0x${data.toString(16).padStart(8, "0")}`;
                                else if (dataType === 16 || dataType === 17) value = data;
                                else if (dataType === 18) value = data !== 0;
                                else value = data;
                            }

                            attrs.push({
                                name: attrName,
                                resId,
                                dataType,
                                data,
                                value,
                                rawValue
                            });
                            attrOff += 20;
                        }
                    }

                    // Process Tag Attributes
                    for (const a of attrs) {
                        if (!manifest.packageName && (a.name === "package" || a.name === "packageName")) {
                            manifest.packageName = String(a.value || "");
                        } else if (a.name === "glEsVersion" || a.resId === ATTR_GLES_VERSION) {
                            manifest.usesFeatures.push({
                                name: "android.hardware.opengles.version",
                                glEsVersion: a.data,
                                required: true
                            });
                        } else if (a.name === "name" || a.resId === ATTR_NAME) {
                            const s = String(a.value || "");
                            if (s.startsWith("android.permission.") && !manifest.permissions.includes(s)) {
                                manifest.permissions.push(s);
                            } else if (s.includes("vulkan") || s.includes("opengles")) {
                                manifest.usesFeatures.push({ name: s, glEsVersion: 0, required: true });
                            }
                        }
                    }

                    if (tagName === "manifest") {
                        for (const a of attrs) {
                            if (a.name === "package" || a.name === "packageName") manifest.packageName = String(a.value || "");
                            else if (a.name === "versionCode" || a.resId === ATTR_VERSION_CODE) manifest.versionCode = Number(a.value) || a.data;
                            else if (a.name === "versionName" || a.resId === ATTR_VERSION_NAME) manifest.versionName = String(a.value || "");
                        }
                    } else if (tagName === "uses-sdk") {
                        for (const a of attrs) {
                            if (a.name === "minSdkVersion" || a.resId === ATTR_MIN_SDK_VERSION) manifest.minSdkVersion = Number(a.value) || a.data;
                            else if (a.name === "targetSdkVersion" || a.resId === ATTR_TARGET_SDK_VERSION) manifest.targetSdkVersion = Number(a.value) || a.data;
                        }
                    } else if (tagName === "application") {
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) manifest.applicationName = String(a.value || "");
                            else if (a.name === "label" || a.resId === ATTR_LABEL) manifest.applicationLabel = String(a.value || "");
                            else if (a.name === "icon" || a.resId === ATTR_ICON) manifest.applicationIcon = a.data;
                            else if (a.name === "theme" || a.resId === ATTR_THEME) manifest.applicationTheme = a.data;
                            else if (a.name === "hasCode" || a.resId === ATTR_HAS_CODE) manifest.hasCode = a.value !== false && a.value !== 0;
                        }
                    } else if (tagName === "activity" || tagName === "activity-alias") {
                        let actName = "";
                        let actLabel = "";
                        let exported = false;
                        let icon = 0;
                        let theme = 0;
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) actName = String(a.value || "");
                            else if (a.name === "label" || a.resId === ATTR_LABEL) actLabel = String(a.value || "");
                            else if (a.name === "icon" || a.resId === ATTR_ICON) icon = a.data;
                            else if (a.name === "theme" || a.resId === ATTR_THEME) theme = a.data;
                            else if (a.name === "exported" || a.resId === ATTR_EXPORTED) exported = !!a.value;
                        }
                        if (actName.startsWith(".")) actName = manifest.packageName + actName;
                        else if (!actName.includes(".") && manifest.packageName) actName = manifest.packageName + "." + actName;
                        currentActivity = {
                            name: actName,
                            label: actLabel,
                            icon,
                            theme,
                            exported,
                            packageName: manifest.packageName,
                            intentFilters: []
                        };
                    } else if (tagName === "service") {
                        let svcName = "";
                        let exported = false;
                        let permission = null;
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) svcName = String(a.value || "");
                            else if (a.name === "exported" || a.resId === ATTR_EXPORTED) exported = !!a.value;
                            else if (a.name === "permission" || a.resId === ATTR_PERMISSION) permission = String(a.value || "");
                        }
                        if (svcName.startsWith(".")) svcName = manifest.packageName + svcName;
                        else if (!svcName.includes(".") && manifest.packageName) svcName = manifest.packageName + "." + svcName;
                        currentService = {
                            name: svcName,
                            exported,
                            permission,
                            packageName: manifest.packageName,
                            intentFilters: []
                        };
                    } else if (tagName === "provider") {
                        let provName = "";
                        let authority = "";
                        let exported = false;
                        let grantUriPermissions = false;
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) provName = String(a.value || "");
                            else if (a.name === "authorities" || a.name === "authority" || a.resId === ATTR_AUTHORITIES) authority = String(a.value || "");
                            else if (a.name === "exported" || a.resId === ATTR_EXPORTED) exported = !!a.value;
                            else if (a.name === "grantUriPermissions" || a.resId === ATTR_GRANT_URI_PERMISSIONS) grantUriPermissions = !!a.value;
                        }
                        if (provName.startsWith(".")) provName = manifest.packageName + provName;
                        else if (!provName.includes(".") && manifest.packageName) provName = manifest.packageName + "." + provName;
                        currentProvider = {
                            name: provName,
                            authority,
                            exported,
                            grantUriPermissions,
                            packageName: manifest.packageName
                        };
                    } else if (tagName === "receiver") {
                        let rcvName = "";
                        let exported = false;
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) rcvName = String(a.value || "");
                            else if (a.name === "exported" || a.resId === ATTR_EXPORTED) exported = !!a.value;
                        }
                        if (rcvName.startsWith(".")) rcvName = manifest.packageName + rcvName;
                        else if (!rcvName.includes(".") && manifest.packageName) rcvName = manifest.packageName + "." + rcvName;
                        currentReceiver = {
                            name: rcvName,
                            exported,
                            packageName: manifest.packageName,
                            intentFilters: []
                        };
                    } else if (tagName === "intent-filter") {
                        currentIntentFilter = { actions: [], categories: [], schemes: [] };
                    } else if (tagName === "action") {
                        if (currentIntentFilter) {
                            for (const a of attrs) {
                                if (a.name === "name" || a.resId === ATTR_NAME) currentIntentFilter.actions.push(String(a.value));
                            }
                        }
                    } else if (tagName === "category") {
                        if (currentIntentFilter) {
                            for (const a of attrs) {
                                if (a.name === "name" || a.resId === ATTR_NAME) currentIntentFilter.categories.push(String(a.value));
                            }
                        }
                    } else if (tagName === "data") {
                        if (currentIntentFilter) {
                            for (const a of attrs) {
                                if (a.name === "scheme") currentIntentFilter.schemes.push(String(a.value));
                            }
                        }
                    } else if (tagName === "uses-permission" || tagName === "uses-permission-sdk-23") {
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) {
                                const perm = String(a.value || "");
                                if (perm && !manifest.permissions.includes(perm)) manifest.permissions.push(perm);
                            }
                        }
                    } else if (tagName === "uses-feature") {
                        let name = "";
                        let glEsVersion = 0;
                        let required = true;
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) name = String(a.value || "");
                            else if (a.name === "glEsVersion" || a.resId === ATTR_GLES_VERSION) glEsVersion = a.data;
                            else if (a.name === "required" || a.resId === ATTR_REQUIRED) required = a.value !== false && a.value !== 0;
                        }
                        if (name || glEsVersion) {
                            manifest.usesFeatures.push({ name, glEsVersion, required });
                        }
                    } else if (tagName === "meta-data") {
                        let key = "";
                        let val = "";
                        for (const a of attrs) {
                            if (a.name === "name" || a.resId === ATTR_NAME) key = String(a.value || "");
                            else if (a.name === "value") val = String(a.value || "");
                        }
                        if (key) manifest.metaData[key] = val;
                    }

                    tagStack.push(tagName);
                }
            } else if (chunkType === RES_XML_END_ELEMENT_TYPE) {
                const tagName = tagStack.pop();
                if (tagName === "intent-filter" && currentIntentFilter) {
                    if (currentActivity) {
                        currentActivity.intentFilters.push(currentIntentFilter);
                        if (currentIntentFilter.actions.includes("android.intent.action.MAIN") &&
                            currentIntentFilter.categories.includes("android.intent.category.LAUNCHER")) {
                            manifest.launcherActivity = currentActivity.name;
                        }
                    } else if (currentService) {
                        currentService.intentFilters.push(currentIntentFilter);
                    } else if (currentReceiver) {
                        currentReceiver.intentFilters.push(currentIntentFilter);
                    }
                    currentIntentFilter = null;
                } else if ((tagName === "activity" || tagName === "activity-alias") && currentActivity) {
                    if (!currentActivity.exported && currentActivity.intentFilters.length > 0) {
                        currentActivity.exported = true;
                    }
                    manifest.activities.push(currentActivity);
                    if (!manifest.launcherActivity) manifest.launcherActivity = currentActivity.name;
                    currentActivity = null;
                } else if (tagName === "service" && currentService) {
                    manifest.services.push(currentService);
                    currentService = null;
                } else if (tagName === "provider" && currentProvider) {
                    manifest.providers.push(currentProvider);
                    currentProvider = null;
                } else if (tagName === "receiver" && currentReceiver) {
                    manifest.receivers.push(currentReceiver);
                    currentReceiver = null;
                }
            }
            pos += chunkSize;
        }

        // Flush any unclosed component
        if (currentActivity) {
            manifest.activities.push(currentActivity);
            if (!manifest.launcherActivity) manifest.launcherActivity = currentActivity.name;
        }
        if (currentService) manifest.services.push(currentService);
        if (currentProvider) manifest.providers.push(currentProvider);
        if (currentReceiver) manifest.receivers.push(currentReceiver);

        manifest.activityCount = manifest.activities.length;
        manifest.serviceCount = manifest.services.length;
        manifest.receiverCount = manifest.receivers.length;
        manifest.providerCount = manifest.providers.length;
        manifest.permissionCount = manifest.permissions.length;

        return manifest;
    }

    /**
     * Decodes any general binary Android XML layout / drawable file into an AST node tree.
     * @param {ArrayBuffer | Uint8Array} buffer
     * @returns {object|null} Root AST node { tag, attrs, children }
     */
    static decodeXmlTree(buffer) {
        let bytes;
        if (buffer instanceof ArrayBuffer) bytes = new Uint8Array(buffer);
        else if (buffer && buffer.buffer) bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        else return null;

        if (bytes.byteLength < 8) return null;
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        if (view.getUint16(0, true) !== RES_XML_TYPE) return null;

        let stringPool = [];
        const textDecoder = new TextDecoder("utf-8");

        // Pass 1: String pool
        let pos = 8;
        while (pos + 8 <= bytes.byteLength) {
            const chunkType = view.getUint16(pos, true);
            const chunkSize = view.getUint32(pos + 4, true);
            if (chunkSize < 8 || pos + chunkSize > bytes.byteLength) break;

            if (chunkType === RES_STRING_POOL_TYPE) {
                const stringCount = view.getUint32(pos + 8, true);
                const stringsStart = view.getUint32(pos + 20, true);
                const isUtf8 = (view.getUint32(pos + 16, true) & (1 << 8)) !== 0;
                for (let i = 0; i < stringCount; i++) {
                    const strOff = view.getUint32(pos + 28 + i * 4, true);
                    const strAbs = pos + stringsStart + strOff;
                    if (isUtf8) {
                        let cursor = strAbs;
                        if (bytes[cursor] & 0x80) cursor += 2; else cursor += 1;
                        let utf8Len = 0;
                        const b = bytes[cursor];
                        if (b & 0x80) {
                            cursor += 2;
                            utf8Len = ((b & 0x7F) << 8) | bytes[cursor - 1];
                        } else {
                            cursor += 1;
                            utf8Len = b;
                        }
                        stringPool.push(textDecoder.decode(bytes.subarray(cursor, cursor + utf8Len)).replace(/\0+$/, ""));
                    } else {
                        let cursor = strAbs;
                        const charLen = view.getUint16(cursor, true);
                        cursor += 2;
                        const u16 = [];
                        for (let c = 0; c < charLen; c++) {
                            u16.push(view.getUint16(cursor + c * 2, true));
                        }
                        stringPool.push(String.fromCharCode(...u16));
                    }
                }
            }
            pos += chunkSize;
        }

        // Pass 2: Tags & Elements
        pos = 8;
        const stack = [];
        let root = null;
        while (pos + 8 <= bytes.byteLength) {
            const chunkType = view.getUint16(pos, true);
            const chunkSize = view.getUint32(pos + 4, true);
            if (chunkSize < 8 || pos + chunkSize > bytes.byteLength) break;

            if (chunkType === RES_XML_START_ELEMENT_TYPE) {
                const tagIdx = view.getUint32(pos + 20, true);
                const tagName = stringPool[tagIdx] || `tag_${tagIdx}`;
                const attrCount = view.getUint16(pos + 28, true);
                const attrs = {};
                let attrOff = pos + 36;
                for (let i = 0; i < attrCount; i++) {
                    if (attrOff + 20 <= pos + chunkSize) {
                        const nameIdx = view.getUint32(attrOff + 4, true);
                        const rawIdx = view.getUint32(attrOff + 8, true);
                        const name = stringPool[nameIdx] || `attr_${nameIdx}`;
                        const val = rawIdx !== 0xFFFFFFFF ? stringPool[rawIdx] : view.getUint32(attrOff + 16, true);
                        attrs[name] = val;
                        attrOff += 20;
                    }
                }
                const node = { tag: tagName, attrs, children: [] };
                if (stack.length > 0) stack[stack.length - 1].children.push(node);
                else root = node;
                stack.push(node);
            } else if (chunkType === RES_XML_END_ELEMENT_TYPE) {
                stack.pop();
            }
            pos += chunkSize;
        }
        return root;
    }
}

// -----------------------------------------------------------------------------
// 4. Binary resources.arsc Parser & String Resolver (ArscStringPoolParser)
// -----------------------------------------------------------------------------

export class ArscStringPoolParser {
    /**
     * @param {ArrayBuffer | Uint8Array} buffer - Binary resources.arsc buffer.
     */
    constructor(buffer) {
        if (buffer instanceof ArrayBuffer) {
            this.bytes = new Uint8Array(buffer);
        } else if (buffer && buffer.buffer) {
            this.bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        } else {
            throw new Error("Invalid buffer provided to ArscStringPoolParser");
        }
        this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
        this.globalStrings = [];
        this.packages = new Map();
        this.isParsed = false;
    }

    parse() {
        if (this.isParsed) return this;

        if (this.bytes.byteLength < 12) {
            this.isParsed = true;
            return this;
        }

        const magic = this.view.getUint16(0, true);
        if (magic !== RES_TABLE_TYPE) {
            this.isParsed = true;
            return this;
        }

        const headerSize = this.view.getUint16(2, true);
        const totalSize = this.view.getUint32(4, true);
        const limit = Math.min(totalSize, this.bytes.byteLength);
        const textDecoder = new TextDecoder("utf-8");

        const parseStringPool = (offset, size) => {
            if (size < 28) return [];
            const stringCount = this.view.getUint32(offset + 8, true);
            const flags = this.view.getUint32(offset + 16, true);
            const stringsStart = this.view.getUint32(offset + 20, true);
            const isUtf8 = (flags & (1 << 8)) !== 0;

            const offsets = [];
            for (let i = 0; i < stringCount; i++) {
                const offPos = offset + 28 + i * 4;
                if (offPos + 4 <= offset + size) {
                    offsets.push(this.view.getUint32(offPos, true));
                }
            }

            const pool = [];
            for (let i = 0; i < offsets.length; i++) {
                const strAbs = offset + stringsStart + offsets[i];
                if (strAbs >= offset + size) {
                    pool.push("");
                    continue;
                }

                if (isUtf8) {
                    let cursor = strAbs;
                    if (this.bytes[cursor] & 0x80) cursor += 2; else cursor += 1;
                    let utf8Len = 0;
                    const b = this.bytes[cursor];
                    if (b & 0x80) {
                        const next = cursor + 1 < offset + size ? this.bytes[cursor + 1] : 0;
                        cursor += 2;
                        utf8Len = ((b & 0x7F) << 8) | next;
                    } else {
                        cursor += 1;
                        utf8Len = b;
                    }
                    const end = Math.min(cursor + utf8Len, offset + size);
                    pool.push(textDecoder.decode(this.bytes.subarray(cursor, end)).replace(/\0+$/, ""));
                } else {
                    let cursor = strAbs;
                    let charLen = 0;
                    const lenPrefix = cursor + 2 <= offset + size ? this.view.getUint16(cursor, true) : 0;
                    if (lenPrefix & 0x8000) {
                        cursor += 4;
                        const next = cursor <= offset + size ? this.view.getUint16(cursor - 2, true) : 0;
                        charLen = ((lenPrefix & 0x7FFF) << 16) | next;
                    } else {
                        cursor += 2;
                        charLen = lenPrefix;
                    }
                    const u16 = [];
                    for (let c = 0; c < charLen; c++) {
                        if (cursor + 2 > offset + size) break;
                        const ch = this.view.getUint16(cursor, true);
                        if (ch === 0) break;
                        u16.push(ch);
                        cursor += 2;
                    }
                    pool.push(String.fromCharCode(...u16));
                }
            }
            return pool;
        };

        let pos = headerSize;
        while (pos + 8 <= limit) {
            const chunkType = this.view.getUint16(pos, true);
            const chunkSize = this.view.getUint32(pos + 4, true);
            if (chunkSize < 8 || pos + chunkSize > this.bytes.byteLength) {
                pos += 4;
                continue;
            }

            if (chunkType === RES_STRING_POOL_TYPE) {
                this.globalStrings = parseStringPool(pos, chunkSize);
            } else if (chunkType === RES_TABLE_PACKAGE_TYPE && chunkSize >= 288) {
                const pkgId = this.view.getUint32(pos + 8, true);
                const nameChars = [];
                for (let i = 0; i < 128; i++) {
                    const ch = this.view.getUint16(pos + 12 + i * 2, true);
                    if (ch === 0) break;
                    nameChars.push(ch);
                }
                const pkgName = String.fromCharCode(...nameChars);
                const typeStringsOffset = this.view.getUint32(pos + 268, true);
                const keyStringsOffset = this.view.getUint32(pos + 276, true);

                let typeStrings = [];
                if (typeStringsOffset > 0 && pos + typeStringsOffset < pos + chunkSize) {
                    typeStrings = parseStringPool(pos + typeStringsOffset, chunkSize - typeStringsOffset);
                }
                let keyStrings = [];
                if (keyStringsOffset > 0 && pos + keyStringsOffset < pos + chunkSize) {
                    keyStrings = parseStringPool(pos + keyStringsOffset, chunkSize - keyStringsOffset);
                }

                const pkgObj = {
                    id: pkgId,
                    name: pkgName,
                    typeStrings,
                    keyStrings,
                    types: new Map()
                };
                this.packages.set(pkgId, pkgObj);

                let innerPos = Math.max(288, keyStringsOffset);
                const maxOffset = Math.max(typeStringsOffset, keyStringsOffset);
                if (maxOffset > 0 && pos + maxOffset + 8 <= pos + chunkSize) {
                    const poolSz = this.view.getUint32(pos + maxOffset + 4, true);
                    innerPos = maxOffset + poolSz;
                }

                while (pos + innerPos + 8 <= pos + chunkSize) {
                    const iType = this.view.getUint16(pos + innerPos, true);
                    const iHeaderSize = this.view.getUint16(pos + innerPos + 2, true);
                    const iChunkSize = this.view.getUint32(pos + innerPos + 4, true);
                    if (iChunkSize < 8 || pos + innerPos + iChunkSize > pos + chunkSize) {
                        innerPos += 4;
                        continue;
                    }

                    if (iType === RES_TABLE_TYPE_TYPE) {
                        const typeId = this.bytes[pos + innerPos + 8];
                        const entryCount = this.view.getUint32(pos + innerPos + 12, true);
                        const entriesStart = this.view.getUint32(pos + innerPos + 16, true);
                        const typeName = typeStrings[typeId - 1] || `type_${typeId}`;

                        if (!pkgObj.types.has(typeId)) {
                            pkgObj.types.set(typeId, { id: typeId, name: typeName, entries: new Map() });
                        }
                        const arscType = pkgObj.types.get(typeId);

                        const offsetsPos = pos + innerPos + iHeaderSize;
                        for (let entryIdx = 0; entryIdx < entryCount; entryIdx++) {
                            const offCursor = offsetsPos + entryIdx * 4;
                            if (offCursor + 4 <= pos + innerPos + iChunkSize) {
                                const entryRelOffset = this.view.getUint32(offCursor, true);
                                if (entryRelOffset !== 0xFFFFFFFF) {
                                    const entryAbs = pos + innerPos + entriesStart + entryRelOffset;
                                    if (entryAbs + 8 <= pos + innerPos + iChunkSize) {
                                        const flags = this.view.getUint16(entryAbs + 2, true);
                                        const keyIdx = this.view.getUint32(entryAbs + 4, true);
                                        const keyStr = keyStrings[keyIdx] || `key_${keyIdx}`;
                                        const isComplex = (flags & 0x0001) !== 0;

                                        if (!isComplex) {
                                            const valAbs = entryAbs + 8;
                                            if (valAbs + 8 <= pos + innerPos + iChunkSize) {
                                                const dataType = this.bytes[valAbs + 3];
                                                const data = this.view.getUint32(valAbs + 4, true);
                                                let val = data;
                                                if (dataType === 3) val = this.globalStrings[data] || "";
                                                arscType.entries.set(entryIdx, { key: keyStr, value: val, dataType });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    innerPos += iChunkSize;
                }
            }
            pos += chunkSize;
        }

        this.isParsed = true;
        return this;
    }

    /**
     * Resolves a resource ID to its string value.
     * @param {number} resId - 32-bit resource ID.
     * @param {number} [depth=0] - Recursion guard.
     * @returns {string | null}
     */
    resolveString(resId, depth = 0) {
        this.parse();
        if (depth > 10 || !resId) return null;

        const pkgId = (resId >> 24) & 0xFF;
        const typeId = (resId >> 16) & 0xFF;
        const entryIdx = resId & 0xFFFF;

        const pkg = this.packages.get(pkgId) || this.packages.values().next().value;
        if (!pkg) return null;

        const arscType = pkg.types.get(typeId);
        if (!arscType) return null;

        const entry = arscType.entries.get(entryIdx);
        if (!entry) return null;

        if (entry.dataType === 3) return String(entry.value);
        if (entry.dataType === 1) return this.resolveString(entry.value, depth + 1);
        return String(entry.value);
    }

    /**
     * Resolves string references like "@0x7f120075" or "@string/app_name".
     * @param {string} text - Raw reference or string value.
     * @returns {string} Human-readable string.
     */
    resolveStringRef(text) {
        if (!text || typeof text !== "string") return "";
        if (text.startsWith("@0x") || text.startsWith("@0X")) {
            const hex = text.slice(3);
            const id = parseInt(hex, 16);
            if (!isNaN(id)) {
                const resolved = this.resolveString(id);
                if (resolved) return resolved;
            }
        } else if (text.startsWith("@") && /^\d+$/.test(text.slice(1))) {
            const id = parseInt(text.slice(1), 10);
            if (!isNaN(id)) {
                const resolved = this.resolveString(id);
                if (resolved) return resolved;
            }
        }
        return text;
    }
}

// -----------------------------------------------------------------------------
// 5. In-Memory Package Manager Registry (PackageManagerRegistry)
// -----------------------------------------------------------------------------

export class PackageManagerRegistry {
    constructor() {
        this.packages = new Map();
        this.authorities = new Map();
        this.listeners = new Set();
        this.initDefaultPackages();
    }

    initDefaultPackages() {
        // Pre-populate system applications
        const systemApps = [
            {
                packageName: "org.fdroid.fdroid",
                applicationLabel: "F-Droid",
                versionCode: 1023051,
                versionName: "1.23.1",
                launcherActivity: "org.fdroid.fdroid.views.main.MainActivity",
                activities: [
                    { name: "org.fdroid.fdroid.views.main.MainActivity", label: "F-Droid", exported: true }
                ],
                services: [],
                receivers: [],
                providers: [
                    { name: "org.fdroid.fdroid.data.AppProvider", authority: "org.fdroid.fdroid.data.AppProvider", exported: false }
                ],
                permissions: ["android.permission.INTERNET", "android.permission.ACCESS_NETWORK_STATE"],
                isSystemApp: true,
                category: "System & App Stores",
                description: "Free and Open Source Android App Repository Client."
            },
            {
                packageName: "com.unity.cube.gles",
                applicationLabel: "Unity 3D Cube",
                versionCode: 100,
                versionName: "1.0.0",
                launcherActivity: "com.unity.cube.gles.UnityPlayerActivity",
                activities: [
                    { name: "com.unity.cube.gles.UnityPlayerActivity", label: "Unity 3D Cube", exported: true }
                ],
                services: [],
                receivers: [],
                providers: [],
                permissions: [],
                isSystemApp: false,
                category: "Games",
                description: "Hardware-accelerated 3D cube demo compiled for Android GLES."
            },
            {
                packageName: "org.godotengine.gles2",
                applicationLabel: "Godot GLES2 Engine",
                versionCode: 200,
                versionName: "2.1.0",
                launcherActivity: "org.godotengine.godot.GodotActivity",
                activities: [
                    { name: "org.godotengine.godot.GodotActivity", label: "Godot GLES2 Engine", exported: true }
                ],
                services: [],
                receivers: [],
                providers: [],
                permissions: [],
                isSystemApp: false,
                category: "Games",
                description: "Godot game engine lightweight rendering runtime."
            },
            {
                packageName: "com.android.chrome",
                applicationLabel: "Chrome",
                versionCode: 5000000,
                versionName: "124.0.6367.82",
                launcherActivity: "com.google.android.apps.chrome.Main",
                activities: [
                    { name: "com.google.android.apps.chrome.Main", label: "Chrome", exported: true }
                ],
                services: [],
                receivers: [],
                providers: [],
                permissions: ["android.permission.INTERNET"],
                isSystemApp: true,
                category: "Internet",
                description: "Fast, secure web browser powered by Google."
            },
            {
                packageName: "com.android.documentsui",
                applicationLabel: "Files",
                versionCode: 33,
                versionName: "13.0",
                launcherActivity: "com.android.documentsui.files.FilesActivity",
                activities: [
                    { name: "com.android.documentsui.files.FilesActivity", label: "Files", exported: true }
                ],
                services: [],
                receivers: [],
                providers: [
                    { name: "com.android.documentsui.archives.ArchivesProvider", authority: "com.android.documentsui.archives", exported: false }
                ],
                permissions: ["android.permission.READ_EXTERNAL_STORAGE"],
                isSystemApp: true,
                category: "System",
                description: "Android system file manager and storage documents provider."
            },
            {
                packageName: "com.android.settings",
                applicationLabel: "Settings",
                versionCode: 33,
                versionName: "13.0",
                launcherActivity: "com.android.settings.Settings",
                activities: [
                    { name: "com.android.settings.Settings", label: "Settings", exported: true }
                ],
                services: [],
                receivers: [],
                providers: [],
                permissions: [],
                isSystemApp: true,
                category: "System",
                description: "Android OS system preferences and hardware configuration."
            }
        ];

        for (const app of systemApps) {
            this.registerPackage(app, false);
        }
    }

    /**
     * Registers or updates a package in the PMS registry.
     * @param {object} pkg - Package info structure.
     * @param {boolean} [notify=true] - Trigger update listener callbacks.
     */
    registerPackage(pkg, notify = true) {
        if (!pkg || !pkg.packageName) throw new Error("Invalid package metadata");

        this.packages.set(pkg.packageName, pkg);

        if (pkg.providers && Array.isArray(pkg.providers)) {
            for (const prov of pkg.providers) {
                if (prov.authority) {
                    const auths = prov.authority.split(";");
                    for (const auth of auths) {
                        this.authorities.set(auth.trim(), prov);
                    }
                }
            }
        }

        if (notify) {
            this.notifyListeners("package_added", pkg);
        }
        return pkg;
    }

    /**
     * Parses an APK buffer and installs it into the registry.
     * @param {ArrayBuffer | Uint8Array} apkBuffer - APK archive buffer.
     * @returns {object} Installed PackageInfo metadata.
     */
    installApk(apkBuffer) {
        const zip = new ApkZipReader(apkBuffer);
        const manifestBytes = zip.getManifest();
        if (!manifestBytes) {
            throw new Error("Invalid APK: AndroidManifest.xml not found in archive");
        }

        const manifest = AxmlDecoder.decode(manifestBytes);

        // Resolve string resources if resources.arsc is present
        const arscBytes = zip.getArsc();
        if (arscBytes) {
            try {
                const arsc = new ArscStringPoolParser(arscBytes);
                if (manifest.applicationLabel) {
                    const resolvedLabel = arsc.resolveStringRef(manifest.applicationLabel);
                    if (resolvedLabel && !resolvedLabel.startsWith("@0x")) {
                        manifest.applicationLabel = resolvedLabel;
                    }
                }
            } catch (_) {
                // Non-fatal if resources.arsc parsing is partially degraded
            }
        }

        if (!manifest.applicationLabel) {
            // Fallback: deduce human-readable label from package name
            const parts = manifest.packageName.split(".");
            const simple = parts[parts.length - 1] || "Application";
            manifest.applicationLabel = simple.charAt(0).toUpperCase() + simple.slice(1);
        }

        const pkgInfo = {
            ...manifest,
            isSystemApp: false,
            installTime: Date.now()
        };

        this.registerPackage(pkgInfo, true);
        return pkgInfo;
    }

    /**
     * Returns list of all installed packages.
     * @returns {object[]}
     */
    getInstalledPackages() {
        return Array.from(this.packages.values());
    }

    /**
     * Retrieves package metadata by package name.
     * @param {string} packageName
     * @returns {object | null}
     */
    getPackageInfo(packageName) {
        return this.packages.get(packageName) || null;
    }

    getPackage(packageName) {
        return this.getPackageInfo(packageName);
    }

    /**
     * Checks if a package is installed.
     * @param {string} packageName
     * @returns {boolean}
     */
    hasPackage(packageName) {
        return this.packages.has(packageName);
    }

    /**
     * Resolves the primary launcher activity for a package.
     * @param {string} packageName
     * @returns {string | null}
     */
    resolveLauncherActivity(packageName) {
        const pkg = this.getPackageInfo(packageName);
        if (!pkg) return null;
        if (pkg.launcherActivity) return pkg.launcherActivity;
        if (pkg.activities && pkg.activities.length > 0) {
            return pkg.activities[0].name;
        }
        return null;
    }

    /**
     * Resolves single activity matching intent descriptor.
     * @param {object | string} intent - Intent filter descriptor or package name.
     * @returns {object | null}
     */
    resolveActivity(intent) {
        if (!intent) return null;
        if (typeof intent === "string") {
            const launcher = this.resolveLauncherActivity(intent);
            return launcher ? { name: launcher, packageName: intent } : null;
        }

        if (intent.packageName) {
            const pkg = this.getPackageInfo(intent.packageName);
            if (pkg && (intent.action === "android.intent.action.MAIN" || !intent.action)) {
                if (pkg.launcherActivity) {
                    const launcherAct = pkg.activities?.find(a => a.name === pkg.launcherActivity);
                    if (launcherAct) return launcherAct;
                    return { name: pkg.launcherActivity, packageName: pkg.packageName };
                }
            }
        }

        const matches = this.queryIntentActivities(intent);
        if (matches && matches.length > 0) {
            if (intent.packageName) {
                const pkg = this.getPackageInfo(intent.packageName);
                const prefer = matches.find(a => a.name === pkg?.launcherActivity);
                if (prefer) return prefer;
            }
            return matches[0];
        }

        if (intent.packageName) {
            const launcher = this.resolveLauncherActivity(intent.packageName);
            if (launcher) return { name: launcher, packageName: intent.packageName };
        }
        return null;
    }

    /**
     * Resolves ContentProvider by authority string.
     * @param {string} authority
     * @returns {object | null}
     */
    resolveContentProvider(authority) {
        return this.authorities.get(authority) || null;
    }

    /**
     * Queries activities matching an intent descriptor.
     * @param {object} intent - Intent filter descriptor.
     * @returns {object[]}
     */
    queryIntentActivities(intent) {
        const results = [];
        if (!intent) return results;

        for (const pkg of this.packages.values()) {
            if (intent.packageName && pkg.packageName !== intent.packageName) continue;
            if (!pkg.activities) continue;

            for (const act of pkg.activities) {
                if (intent.className && act.name !== intent.className) continue;
                if (!intent.action && !intent.category) {
                    results.push(act);
                    continue;
                }
                if (act.intentFilters && Array.isArray(act.intentFilters)) {
                    for (const filter of act.intentFilters) {
                        const actionMatch = !intent.action || filter.actions.includes(intent.action);
                        const categoryMatch = !intent.category || filter.categories.includes(intent.category);
                        if (actionMatch && categoryMatch) {
                            results.push(act);
                            break;
                        }
                    }
                }
            }
        }
        return results;
    }

    /**
     * Unregisters a package from the registry.
     * @param {string} packageName
     * @returns {boolean}
     */
    unregisterPackage(packageName) {
        const pkg = this.packages.get(packageName);
        if (!pkg) return false;

        this.packages.delete(packageName);
        if (pkg.providers) {
            for (const prov of pkg.providers) {
                if (prov.authority) {
                    for (const auth of prov.authority.split(";")) {
                        this.authorities.delete(auth.trim());
                    }
                }
            }
        }
        this.notifyListeners("package_removed", { packageName });
        return true;
    }

    /**
     * Adds an event listener for PMS updates.
     * @param {Function} listener
     */
    addListener(listener) {
        this.listeners.add(listener);
    }

    /**
     * Removes an event listener.
     * @param {Function} listener
     */
    removeListener(listener) {
        this.listeners.delete(listener);
    }

    /**
     * Notifies all listeners of PMS events.
     * @param {string} event
     * @param {any} data
     */
    notifyListeners(event, data) {
        for (const listener of this.listeners) {
            try {
                listener(event, data);
            } catch (err) {
                console.error("Error in PackageManager listener:", err);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Universal Browser & Node Exports
// -----------------------------------------------------------------------------

export const defaultPackageManager = new PackageManagerRegistry();

/**
 * Convenience helper to parse APK buffer into PackageInfo.
 * @param {ArrayBuffer | Uint8Array} apkBuffer
 * @returns {object}
 */
export function parseApk(apkBuffer) {
    const zip = new ApkZipReader(apkBuffer);
    const manifestBytes = zip.getManifest();
    if (!manifestBytes) throw new Error("AndroidManifest.xml not found in APK");

    const manifest = AxmlDecoder.decode(manifestBytes);
    const arscBytes = zip.getArsc();
    if (arscBytes) {
        try {
            const arsc = new ArscStringPoolParser(arscBytes);
            if (manifest.applicationLabel) {
                const label = arsc.resolveStringRef(manifest.applicationLabel);
                if (label && !label.startsWith("@0x")) {
                    manifest.applicationLabel = label;
                }
            }
        } catch (_) {}
    }
    return manifest;
}

// Attach to browser global window if available
if (typeof window !== "undefined") {
    window.AndroidApkParser = {
        ApkZipReader,
        AxmlDecoder,
        ArscStringPoolParser,
        PackageManagerRegistry,
        inflateRaw,
        parseApk,
        defaultPackageManager
    };
    window.AndroidPmsRegistry = defaultPackageManager;
    window.ApkZipReader = ApkZipReader;
    window.AxmlDecoder = AxmlDecoder;
    window.PackageManagerRegistry = PackageManagerRegistry;
}

// CommonJS compatibility
if (typeof module !== "undefined" && module.exports) {
    module.exports = {
        RES_NULL_TYPE,
        RES_STRING_POOL_TYPE,
        RES_TABLE_TYPE,
        RES_XML_TYPE,
        RES_XML_START_NAMESPACE_TYPE,
        RES_XML_END_NAMESPACE_TYPE,
        RES_XML_START_ELEMENT_TYPE,
        RES_XML_END_ELEMENT_TYPE,
        RES_XML_RESOURCE_MAP_TYPE,
        inflateRaw,
        ApkZipReader,
        AxmlDecoder,
        ArscStringPoolParser,
        PackageManagerRegistry,
        parseApk,
        defaultPackageManager
    };
}
