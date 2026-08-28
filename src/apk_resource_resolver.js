/**
 * AndroidWebGPU - Authentic APK Resource Resolver & ARSC Binary Parser
 * 
 * Provides:
 * 1. TypedValue: Complex dimension unit decoder (dp, sp, px, pt, in, mm), hex color decoder, float/int/boolean converter.
 * 2. ArscDecoder: Full binary resources.arsc table decoder with ResTable_config locale/density matching and complex bag support.
 * 3. ArscResourceTable: In-memory resource repository with resolveString, resolveColor, resolveDimension, resolveLayoutPath, resolveIdentifier.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

// -----------------------------------------------------------------------------
// 1. Constants & ResTable Chunk Identifiers
// -----------------------------------------------------------------------------

export const RES_NULL_TYPE = 0x0000;
export const RES_STRING_POOL_TYPE = 0x0001;
export const RES_TABLE_TYPE = 0x0002;
export const RES_XML_TYPE = 0x0003;

export const RES_TABLE_PACKAGE_TYPE = 0x0200;
export const RES_TABLE_TYPE_TYPE = 0x0201;
export const RES_TABLE_TYPE_SPEC_TYPE = 0x0202;
export const RES_TABLE_LIBRARY_TYPE = 0x0203;

// TypedValue Types
export const TYPE_NULL = 0x00;
export const TYPE_REFERENCE = 0x01;
export const TYPE_ATTRIBUTE = 0x02;
export const TYPE_STRING = 0x03;
export const TYPE_FLOAT = 0x04;
export const TYPE_DIMENSION = 0x05;
export const TYPE_FRACTION = 0x06;
export const TYPE_DYNAMIC_REFERENCE = 0x07;
export const TYPE_DYNAMIC_ATTRIBUTE = 0x08;

export const TYPE_FIRST_INT = 0x10;
export const TYPE_INT_DEC = 0x10;
export const TYPE_INT_HEX = 0x11;
export const TYPE_INT_BOOLEAN = 0x12;
export const TYPE_FIRST_COLOR_INT = 0x1c;
export const TYPE_INT_COLOR_ARGB8 = 0x1c;
export const TYPE_INT_COLOR_RGB8 = 0x1d;
export const TYPE_INT_COLOR_ARGB4 = 0x1e;
export const TYPE_INT_COLOR_RGB4 = 0x1f;
export const TYPE_LAST_COLOR_INT = 0x1f;
export const TYPE_LAST_INT = 0x1f;

// Dimension Units
export const COMPLEX_UNIT_PX = 0;
export const COMPLEX_UNIT_DIP = 1;
export const COMPLEX_UNIT_SP = 2;
export const COMPLEX_UNIT_PT = 3;
export const COMPLEX_UNIT_IN = 4;
export const COMPLEX_UNIT_MM = 5;

export const COMPLEX_UNIT_MASK = 0x0F;
export const COMPLEX_RADIX_SHIFT = 4;
export const COMPLEX_RADIX_MASK = 0x03;
export const COMPLEX_MANTISSA_SHIFT = 8;
export const COMPLEX_MANTISSA_MASK = 0xFFFFFF;

const RADIX_MULTS = [
    1.0 / 256.0,
    1.0 / (256.0 * 128.0),
    1.0 / (256.0 * 32768.0),
    1.0 / (256.0 * 8388608.0)
];

const UNIT_NAMES = ['px', 'dp', 'sp', 'pt', 'in', 'mm'];

// -----------------------------------------------------------------------------
// 2. TypedValue Unit & Type Decoder
// -----------------------------------------------------------------------------

export class TypedValue {
    static TYPE_NULL = TYPE_NULL;
    static TYPE_REFERENCE = TYPE_REFERENCE;
    static TYPE_ATTRIBUTE = TYPE_ATTRIBUTE;
    static TYPE_STRING = TYPE_STRING;
    static TYPE_FLOAT = TYPE_FLOAT;
    static TYPE_DIMENSION = TYPE_DIMENSION;
    static TYPE_FRACTION = TYPE_FRACTION;
    static TYPE_INT_DEC = TYPE_INT_DEC;
    static TYPE_INT_HEX = TYPE_INT_HEX;
    static TYPE_INT_BOOLEAN = TYPE_INT_BOOLEAN;
    static TYPE_INT_COLOR_ARGB8 = TYPE_INT_COLOR_ARGB8;
    static TYPE_INT_COLOR_RGB8 = TYPE_INT_COLOR_RGB8;
    static TYPE_INT_COLOR_ARGB4 = TYPE_INT_COLOR_ARGB4;
    static TYPE_INT_COLOR_RGB4 = TYPE_INT_COLOR_RGB4;

    static COMPLEX_UNIT_PX = COMPLEX_UNIT_PX;
    static COMPLEX_UNIT_DIP = COMPLEX_UNIT_DIP;
    static COMPLEX_UNIT_SP = COMPLEX_UNIT_SP;
    static COMPLEX_UNIT_PT = COMPLEX_UNIT_PT;
    static COMPLEX_UNIT_IN = COMPLEX_UNIT_IN;
    static COMPLEX_UNIT_MM = COMPLEX_UNIT_MM;

    /**
     * Converts a complex dimension data word to a 32-bit floating point value.
     * @param {number} data - 32-bit complex dimension integer.
     * @returns {number}
     */
    static complexToFloat(data) {
        const radix = (data >> COMPLEX_RADIX_SHIFT) & COMPLEX_RADIX_MASK;
        // Sign-extend mantissa (bits 8-31)
        const mantissa = (data & ~0xFF) | 0;
        return mantissa * RADIX_MULTS[radix];
    }

    /**
     * Converts a complex dimension data word to pixels using density scaling.
     * @param {number} data - 32-bit complex dimension integer.
     * @param {number} [density=1.0] - Display density multiplier (1.0 = mdpi, 2.0 = xhdpi, etc.).
     * @returns {number} Pixel value.
     */
    static complexToDimension(data, density = 1.0) {
        const val = this.complexToFloat(data);
        const unit = data & COMPLEX_UNIT_MASK;
        switch (unit) {
            case COMPLEX_UNIT_PX:
                return val;
            case COMPLEX_UNIT_DIP:
                return val * density;
            case COMPLEX_UNIT_SP:
                return val * density;
            case COMPLEX_UNIT_PT:
                return val * density * (160.0 / 72.0);
            case COMPLEX_UNIT_IN:
                return val * density * 160.0;
            case COMPLEX_UNIT_MM:
                return val * density * (160.0 / 25.4);
            default:
                return val;
        }
    }

    /**
     * Converts dimension to rounded integer pixel size (minimum 1 if value > 0).
     */
    static complexToDimensionPixelSize(data, density = 1.0) {
        const dim = this.complexToDimension(data, density);
        const res = Math.round(dim);
        if (res !== 0) return res;
        if (dim === 0) return 0;
        if (dim > 0) return 1;
        return -1;
    }

    /**
     * Converts dimension to integer pixel offset (truncated).
     */
    static complexToDimensionPixelOffset(data, density = 1.0) {
        return Math.trunc(this.complexToDimension(data, density));
    }

    /**
     * Formats complex dimension as human-readable string (e.g. "48dp", "16sp").
     */
    static formatDimension(data) {
        const val = this.complexToFloat(data);
        const unit = data & COMPLEX_UNIT_MASK;
        const unitStr = UNIT_NAMES[unit] || 'px';
        return `${Number(val.toFixed(2))}${unitStr}`;
    }

    /**
     * Converts an integer color value to hex string `#AARRGGBB` or `#RRGGBB`.
     * @param {number} data - 32-bit color integer.
     * @param {number} [dataType=TYPE_INT_COLOR_ARGB8] - Color format type.
     * @returns {string} Hex color string.
     */
    static decodeColor(data, dataType = TYPE_INT_COLOR_ARGB8) {
        const u = data >>> 0;
        if (dataType === TYPE_INT_COLOR_ARGB8) {
            const a = (u >>> 24) & 0xFF;
            const r = (u >>> 16) & 0xFF;
            const g = (u >>> 8) & 0xFF;
            const b = u & 0xFF;
            if (a === 0xFF) {
                return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
            }
            return `#${a.toString(16).padStart(2, '0')}${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
        }
        if (dataType === TYPE_INT_COLOR_RGB8) {
            const r = (u >>> 16) & 0xFF;
            const g = (u >>> 8) & 0xFF;
            const b = u & 0xFF;
            return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
        }
        if (dataType === TYPE_INT_COLOR_ARGB4) {
            const a = ((u >>> 12) & 0x0F) * 17;
            const r = ((u >>> 8) & 0x0F) * 17;
            const g = ((u >>> 4) & 0x0F) * 17;
            const b = (u & 0x0F) * 17;
            if (a === 0xFF) {
                return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
            }
            return `#${a.toString(16).padStart(2, '0')}${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
        }
        if (dataType === TYPE_INT_COLOR_RGB4) {
            const r = ((u >>> 8) & 0x0F) * 17;
            const g = ((u >>> 4) & 0x0F) * 17;
            const b = (u & 0x0F) * 17;
            return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
        }
        return `#${u.toString(16).padStart(8, '0')}`;
    }

    /**
     * Decodes a raw Res_value into a JavaScript representation.
     * @param {number} dataType - Res_value data type.
     * @param {number} data - Res_value data integer.
     * @param {string[]} [stringPool=[]] - Global string pool for string lookups.
     * @param {number} [density=1.0] - Display density.
     * @returns {any}
     */
    static decodeValue(dataType, data, stringPool = [], density = 1.0) {
        if (dataType === TYPE_NULL) return null;
        if (dataType === TYPE_STRING) return stringPool[data] || '';
        if (dataType === TYPE_FLOAT) {
            const buf = new ArrayBuffer(4);
            new Uint32Array(buf)[0] = data >>> 0;
            return new Float32Array(buf)[0];
        }
        if (dataType === TYPE_DIMENSION) {
            return this.complexToDimension(data, density);
        }
        if (dataType === TYPE_INT_BOOLEAN) {
            return data !== 0;
        }
        if (dataType >= TYPE_FIRST_COLOR_INT && dataType <= TYPE_LAST_COLOR_INT) {
            return this.decodeColor(data, dataType);
        }
        if (dataType === TYPE_INT_DEC) {
            return data | 0; // signed 32-bit int (-1 match_parent, -2 wrap_content)
        }
        if (dataType === TYPE_INT_HEX) {
            return data >>> 0;
        }
        if (dataType === TYPE_REFERENCE || dataType === TYPE_DYNAMIC_REFERENCE) {
            return data >>> 0;
        }
        if (dataType === TYPE_ATTRIBUTE || dataType === TYPE_DYNAMIC_ATTRIBUTE) {
            return data >>> 0;
        }
        return data;
    }
}

// -----------------------------------------------------------------------------
// 3. String Pool Parser Helper
// -----------------------------------------------------------------------------

function parseArscStringPool(bytes, view, offset, size) {
    if (size < 28) return [];
    const stringCount = view.getUint32(offset + 8, true);
    const flags = view.getUint32(offset + 16, true);
    const stringsStart = view.getUint32(offset + 20, true);
    const isUtf8 = (flags & (1 << 8)) !== 0;
    const textDecoder = new TextDecoder('utf-8');

    const offsets = [];
    for (let i = 0; i < stringCount; i++) {
        const offPos = offset + 28 + i * 4;
        if (offPos + 4 <= offset + size) {
            offsets.push(view.getUint32(offPos, true));
        }
    }

    const pool = [];
    for (let i = 0; i < offsets.length; i++) {
        const strAbs = offset + stringsStart + offsets[i];
        if (strAbs >= offset + size) {
            pool.push('');
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
            pool.push(textDecoder.decode(bytes.subarray(cursor, end)).replace(/\0+$/, ''));
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

// -----------------------------------------------------------------------------
// 4. ArscResourceTable
// -----------------------------------------------------------------------------

export class ArscResourceTable {
    constructor(globalStrings = [], packages = new Map(), targetLocale = '', targetDensity = 160) {
        this.globalStrings = globalStrings;
        this.packages = packages;
        this.targetLocale = targetLocale;
        this.targetDensity = targetDensity;
        // Index by 32-bit resId -> Array of entry objects with config metadata
        this.entriesById = new Map();
        // Index by "pkg:type/name" -> 32-bit resId
        this.idByName = new Map();

        this._buildIndexes();
    }

    _buildIndexes() {
        for (const [pkgId, pkg] of this.packages.entries()) {
            for (const [typeId, typeObj] of pkg.types.entries()) {
                const typeName = typeObj.name;
                for (const [entryIdx, entryList] of typeObj.entries.entries()) {
                    const resId = (pkgId << 24) | (typeId << 16) | entryIdx;
                    this.entriesById.set(resId, entryList);

                    if (entryList.length > 0) {
                        const keyName = entryList[0].key;
                        // Index variants
                        this.idByName.set(`${pkg.name}:${typeName}/${keyName}`, resId);
                        this.idByName.set(`${typeName}/${keyName}`, resId);
                        this.idByName.set(keyName, resId);
                    }
                }
            }
        }
    }

    /**
     * Computes matching score for ResTable_config against target locale and density.
     */
    _scoreConfig(config, reqLocale = this.targetLocale, reqDensity = this.targetDensity) {
        if (!config) return 0;
        let score = 0;

        const reqLang = reqLocale ? reqLocale.split(/[-_]/)[0].toLowerCase() : '';
        const reqCountry = reqLocale && reqLocale.includes('-') ? reqLocale.split(/[-_]/)[1].toLowerCase() : '';
        const cfgLang = (config.lang || '').toLowerCase();
        const cfgCountry = (config.country || '').toLowerCase();

        if (reqLang) {
            if (cfgLang === reqLang) {
                score += 1000;
                if (reqCountry && cfgCountry === reqCountry) {
                    score += 500;
                } else if (!cfgCountry) {
                    score += 200;
                }
            } else if (!cfgLang) {
                // Default fallback
                score += 100;
            } else {
                // Mismatched language: lowest score
                return -1;
            }
        } else {
            // Default requested: prefer neutral/default config
            if (!cfgLang) score += 500;
            else if (cfgLang === 'en') score += 300;
            else score += 10;
        }

        // Density matching
        if (config.density === reqDensity) {
            score += 50;
        } else if (config.density === 0 || config.density === 65534) {
            score += 20;
        } else if (config.density > 0) {
            const diff = Math.abs(config.density - reqDensity);
            score += Math.max(0, 10 - Math.floor(diff / 100));
        }

        return score;
    }

    /**
     * Gets best matching entry record for a given resource ID.
     * @param {number} resId - 32-bit resource ID.
     * @param {string} [locale] - Desired locale override.
     * @returns {object|null}
     */
    getEntry(resId, locale = this.targetLocale) {
        const list = this.entriesById.get(resId >>> 0);
        if (!list || list.length === 0) return null;
        if (list.length === 1) return list[0];

        let bestEntry = list[0];
        let bestScore = -2;

        for (const item of list) {
            const score = this._scoreConfig(item.config, locale, this.targetDensity);
            if (score > bestScore) {
                bestScore = score;
                bestEntry = item;
            }
        }

        return bestEntry;
    }

    /**
     * Resolves a resource ID to its string value with recursive reference resolution.
     * @param {number} resId - 32-bit resource ID.
     * @param {string} [locale] - Optional locale code (e.g. 'en', 'de', 'zh').
     * @param {number} [depth=0] - Recursion guard.
     * @returns {string|null}
     */
    resolveString(resId, locale = this.targetLocale, depth = 0) {
        if (!resId || depth > 10) return null;
        const entry = this.getEntry(resId, locale);
        if (!entry) return null;

        if (entry.isComplex) return null;

        if (entry.dataType === TYPE_STRING) {
            return this.globalStrings[entry.data] || '';
        }
        if (entry.dataType === TYPE_REFERENCE || entry.dataType === TYPE_DYNAMIC_REFERENCE) {
            return this.resolveString(entry.data, locale, depth + 1);
        }
        if (entry.dataType >= TYPE_FIRST_COLOR_INT && entry.dataType <= TYPE_LAST_COLOR_INT) {
            return TypedValue.decodeColor(entry.data, entry.dataType);
        }
        if (entry.dataType === TYPE_INT_DEC || entry.dataType === TYPE_INT_HEX) {
            return String(entry.data);
        }
        if (entry.dataType === TYPE_INT_BOOLEAN) {
            return entry.data !== 0 ? 'true' : 'false';
        }
        if (entry.dataType === TYPE_DIMENSION) {
            return TypedValue.formatDimension(entry.data);
        }
        return entry.val !== undefined && entry.val !== null ? String(entry.val) : null;
    }

    /**
     * Resolves a color resource ID to a hex color string `#AARRGGBB` or `#RRGGBB`.
     * @param {number} resId - 32-bit resource ID.
     * @param {number} [depth=0] - Recursion guard.
     * @returns {string|null}
     */
    resolveColor(resId, depth = 0) {
        if (!resId || depth > 10) return null;
        const entry = this.getEntry(resId);
        if (!entry || entry.isComplex) return null;

        if (entry.dataType >= TYPE_FIRST_COLOR_INT && entry.dataType <= TYPE_LAST_COLOR_INT) {
            return TypedValue.decodeColor(entry.data, entry.dataType);
        }
        if (entry.dataType === TYPE_REFERENCE || entry.dataType === TYPE_DYNAMIC_REFERENCE) {
            return this.resolveColor(entry.data, depth + 1);
        }
        if (entry.dataType === TYPE_STRING) {
            const str = this.globalStrings[entry.data] || '';
            if (str.startsWith('#')) return str;
            if (str.startsWith('@') || str.startsWith('?')) {
                const refId = this.resolveIdentifierRef(str);
                if (refId) return this.resolveColor(refId, depth + 1);
            }
            return str;
        }
        return TypedValue.decodeColor(entry.data, TYPE_INT_COLOR_ARGB8);
    }

    /**
     * Resolves a dimension resource ID to pixel size.
     * @param {number} resId - 32-bit resource ID.
     * @param {number} [density=1.0] - Display density.
     * @param {number} [depth=0] - Recursion guard.
     * @returns {number|null}
     */
    resolveDimension(resId, density = 1.0, depth = 0) {
        if (!resId || depth > 10) return null;
        const entry = this.getEntry(resId);
        if (!entry || entry.isComplex) return null;

        if (entry.dataType === TYPE_DIMENSION) {
            return TypedValue.complexToDimension(entry.data, density);
        }
        if (entry.dataType === TYPE_REFERENCE || entry.dataType === TYPE_DYNAMIC_REFERENCE) {
            return this.resolveDimension(entry.data, density, depth + 1);
        }
        if (entry.dataType === TYPE_INT_DEC) {
            return entry.data | 0;
        }
        if (entry.dataType === TYPE_FLOAT) {
            return TypedValue.decodeValue(entry.dataType, entry.data, this.globalStrings, density);
        }
        return null;
    }

    /**
     * Resolves a layout resource ID to its APK file path (e.g. "res/v9.xml").
     * @param {number|string} resId - 32-bit resource ID or identifier string.
     * @returns {string|null}
     */
    resolveLayoutPath(resId) {
        if (typeof resId === 'string') {
            const num = this.resolveIdentifierRef(resId) || this.resolveIdentifier(resId, 'layout');
            if (num) resId = num;
            else return null;
        }
        const entry = this.getEntry(resId);
        if (!entry || entry.isComplex) return null;

        if (entry.dataType === TYPE_STRING) {
            return this.globalStrings[entry.data] || null;
        }
        if (entry.dataType === TYPE_REFERENCE) {
            return this.resolveLayoutPath(entry.data);
        }
        return typeof entry.val === 'string' ? entry.val : null;
    }

    /**
     * Resolves a drawable resource ID to its file path (e.g. "res/0E.png" or "res/k9.xml").
     * @param {number|string} resId - 32-bit resource ID or string reference.
     * @returns {string|null}
     */
    resolveDrawablePath(resId) {
        if (typeof resId === 'string') {
            const num = this.resolveIdentifierRef(resId) || this.resolveIdentifier(resId, 'drawable') || this.resolveIdentifier(resId, 'mipmap');
            if (num) resId = num;
            else return null;
        }
        const entry = this.getEntry(resId);
        if (!entry || entry.isComplex) return null;

        if (entry.dataType === TYPE_STRING) {
            return this.globalStrings[entry.data] || null;
        }
        if (entry.dataType === TYPE_REFERENCE) {
            return this.resolveDrawablePath(entry.data);
        }
        return typeof entry.val === 'string' ? entry.val : null;
    }

    /**
     * Resolves a drawable resource ID into a drawable descriptor.
     * @param {number|string} resId - 32-bit resource ID or string reference.
     * @param {object} [apkZip] - Optional ApkZipReader to load binary entry buffers.
     * @returns {{ type: 'vector'|'bitmap'|'color', path?: string, data?: Uint8Array|ArrayBuffer, color?: string }|null}
     */
    resolveDrawable(resId, apkZip = null) {
        if (typeof resId === 'string') {
            const num = this.resolveIdentifierRef(resId) || this.resolveIdentifier(resId, 'drawable') || this.resolveIdentifier(resId, 'mipmap');
            if (num) resId = num;
        }

        const entry = this.getEntry(resId);
        if (entry && !entry.isComplex) {
            if (entry.dataType >= TYPE_FIRST_COLOR_INT && entry.dataType <= TYPE_LAST_COLOR_INT) {
                return { type: 'color', color: TypedValue.decodeColor(entry.data, entry.dataType) };
            }
        }

        const path = this.resolveDrawablePath(resId);
        if (!path) return null;

        let data = null;
        if (apkZip && typeof apkZip.getFile === 'function') {
            data = apkZip.getFile(path);
        }

        if (path.endsWith('.xml')) {
            return { type: 'vector', path, data };
        } else if (path.endsWith('.png') || path.endsWith('.webp') || path.endsWith('.jpg') || path.endsWith('.jpeg')) {
            return { type: 'bitmap', path, data };
        }
        return { type: 'vector', path, data };
    }

    /**
     * Resolves a named identifier (name, type, package) to a 32-bit resource ID.
     * @param {string} name - Resource entry name (e.g. 'activity_main', 'icon').
     * @param {string} [type] - Resource type name (e.g. 'layout', 'id', 'string', 'color').
     * @param {string} [pkg] - Package name (e.g. 'org.fdroid.fdroid').
     * @returns {number|null} 32-bit resource ID.
     */
    resolveIdentifier(name, type = '', pkg = '') {
        if (!name) return null;
        // Clean name if prefixed with @type/ or ?attr/
        if (name.startsWith('@') || name.startsWith('?')) {
            const parsed = this.resolveIdentifierRef(name);
            if (parsed) return parsed;
        }

        if (pkg && type) {
            const fullKey = `${pkg}:${type}/${name}`;
            if (this.idByName.has(fullKey)) return this.idByName.get(fullKey);
        }
        if (type) {
            const typeKey = `${type}/${name}`;
            if (this.idByName.has(typeKey)) return this.idByName.get(typeKey);
        }
        if (this.idByName.has(name)) return this.idByName.get(name);

        return null;
    }

    /**
     * Parses reference strings like "@string/app_name", "@0x7f120075", "?attr/selectableItemBackground".
     * @param {string} refStr
     * @returns {number|null}
     */
    resolveIdentifierRef(refStr) {
        if (!refStr || typeof refStr !== 'string') return null;
        const trimmed = refStr.trim();

        if (trimmed.startsWith('@0x') || trimmed.startsWith('@0X') || trimmed.startsWith('0x') || trimmed.startsWith('0X')) {
            const hex = trimmed.replace(/^[@]/, '');
            const parsed = parseInt(hex, 16);
            if (!isNaN(parsed) && parsed > 0) return parsed;
        }

        if (trimmed.startsWith('@+id/')) {
            const name = trimmed.slice(5);
            return this.resolveIdentifier(name, 'id');
        }

        if (trimmed.startsWith('@id/')) {
            const name = trimmed.slice(4);
            return this.resolveIdentifier(name, 'id');
        }

        if (trimmed.startsWith('@')) {
            const slashIdx = trimmed.indexOf('/');
            if (slashIdx > 0) {
                let type = trimmed.slice(1, slashIdx);
                const name = trimmed.slice(slashIdx + 1);
                let pkg = '';
                if (type.includes(':')) {
                    const parts = type.split(':');
                    pkg = parts[0];
                    type = parts[1];
                }
                return this.resolveIdentifier(name, type, pkg);
            }
        }

        if (trimmed.startsWith('?attr/')) {
            const name = trimmed.slice(6);
            return this.resolveIdentifier(name, 'attr');
        }
        if (trimmed.startsWith('?android:attr/')) {
            const name = trimmed.slice(14);
            return this.resolveIdentifier(name, 'attr', 'android');
        }
        if (trimmed.startsWith('?')) {
            const rest = trimmed.slice(1);
            const slashIdx = rest.indexOf('/');
            if (slashIdx > 0) {
                const type = rest.slice(0, slashIdx);
                const name = rest.slice(slashIdx + 1);
                return this.resolveIdentifier(name, type);
            }
            return this.resolveIdentifier(rest, 'attr');
        }

        return null;
    }

    /**
     * Resolves any resource reference or raw value into its final primitive value.
     * @param {any} value - Value, resource ID, or reference string.
     * @param {number} [density=1.0] - Display density.
     * @param {string} [locale] - Target locale.
     * @returns {any}
     */
    resolveValue(value, density = 1.0, locale = this.targetLocale) {
        if (value === null || value === undefined) return null;

        if (typeof value === 'number') {
            // Check if it's a valid resource ID (0xPPTTEEEE)
            if ((value >>> 24) >= 0x01) {
                const entry = this.getEntry(value, locale);
                if (entry) {
                    if (entry.typeName === 'string') return this.resolveString(value, locale);
                    if (entry.typeName === 'color') return this.resolveColor(value);
                    if (entry.typeName === 'dimen') return this.resolveDimension(value, density);
                    if (entry.typeName === 'layout') return this.resolveLayoutPath(value);
                    if (entry.typeName === 'drawable' || entry.typeName === 'mipmap') return this.resolveDrawablePath(value);
                    if (entry.dataType) return TypedValue.decodeValue(entry.dataType, entry.data, this.globalStrings, density);
                }
            }
            return value;
        }

        if (typeof value === 'string') {
            const refId = this.resolveIdentifierRef(value);
            if (refId) {
                return this.resolveValue(refId, density, locale);
            }
            return value;
        }

        return value;
    }

    /**
     * Resolves a style bag resource ID to a map of attribute key-value pairs.
     * @param {number} resId - Style resource ID.
     * @returns {object|null}
     */
    resolveStyle(resId) {
        const entry = this.getEntry(resId);
        if (!entry || !entry.isComplex) return null;

        const result = {
            key: entry.key,
            parent: entry.parent,
            attributes: {}
        };

        if (entry.items && Array.isArray(entry.items)) {
            for (const item of entry.items) {
                const decodedVal = TypedValue.decodeValue(item.dataType, item.data, this.globalStrings, 1.0);
                result.attributes[item.name] = decodedVal;
            }
        }

        return result;
    }

    /**
     * Returns all entry records for a given type name (e.g. 'layout', 'string', 'drawable').
     * @param {string} [typeName] - Optional type filter.
     * @returns {Array<object>}
     */
    getAllEntries(typeName = '') {
        const results = [];
        for (const [resId, list] of this.entriesById.entries()) {
            if (list.length > 0) {
                const best = this.getEntry(resId);
                if (!typeName || best.typeName === typeName) {
                    results.push({ resId, ...best });
                }
            }
        }
        return results;
    }
}

// -----------------------------------------------------------------------------
// 5. ArscDecoder
// -----------------------------------------------------------------------------

export class ArscDecoder {
    /**
     * Decodes a binary resources.arsc buffer into an ArscResourceTable.
     * @param {ArrayBuffer | Uint8Array} buffer - Binary ARSC byte buffer.
     * @param {string} [targetLocale=''] - Target locale filter (e.g. 'en', 'de').
     * @param {number} [targetDensity=160] - Target screen density (160 = mdpi).
     * @returns {ArscResourceTable}
     */
    static decode(buffer, targetLocale = '', targetDensity = 160) {
        return new ArscDecoder().decode(buffer, targetLocale, targetDensity);
    }

    /**
     * Instance decode method.
     * @param {ArrayBuffer | Uint8Array} buffer
     * @param {string} [targetLocale='']
     * @param {number} [targetDensity=160]
     * @returns {ArscResourceTable}
     */
    decode(buffer, targetLocale = '', targetDensity = 160) {
        let bytes;
        if (buffer instanceof ArrayBuffer) {
            bytes = new Uint8Array(buffer);
        } else if (buffer && buffer.buffer) {
            bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        } else {
            return new ArscResourceTable();
        }

        if (bytes.byteLength < 12) {
            return new ArscResourceTable();
        }

        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const fileType = view.getUint16(0, true);
        if (fileType !== RES_TABLE_TYPE) {
            return new ArscResourceTable();
        }

        const headerSize = view.getUint16(2, true);
        const totalSize = view.getUint32(4, true);
        const limit = Math.min(totalSize, bytes.byteLength);

        let globalStrings = [];
        const packages = new Map();

        let pos = headerSize;
        while (pos + 8 <= limit) {
            const chunkType = view.getUint16(pos, true);
            const chunkSize = view.getUint32(pos + 4, true);
            if (chunkSize < 8 || pos + chunkSize > limit) {
                pos += 4;
                continue;
            }

            if (chunkType === RES_STRING_POOL_TYPE) {
                globalStrings = parseArscStringPool(bytes, view, pos, chunkSize);
            } else if (chunkType === RES_TABLE_PACKAGE_TYPE && chunkSize >= 288) {
                const pkgId = view.getUint32(pos + 8, true);

                // Package name (128 utf-16 characters)
                const nameChars = [];
                for (let i = 0; i < 128; i++) {
                    const ch = view.getUint16(pos + 12 + i * 2, true);
                    if (ch === 0) break;
                    nameChars.push(ch);
                }
                const pkgName = String.fromCharCode(...nameChars);

                const typeStringsOffset = view.getUint32(pos + 268, true);
                const keyStringsOffset = view.getUint32(pos + 276, true);

                let typeStrings = [];
                if (typeStringsOffset > 0 && pos + typeStringsOffset < pos + chunkSize) {
                    const poolSz = view.getUint32(pos + typeStringsOffset + 4, true);
                    typeStrings = parseArscStringPool(bytes, view, pos + typeStringsOffset, poolSz);
                }

                let keyStrings = [];
                if (keyStringsOffset > 0 && pos + keyStringsOffset < pos + chunkSize) {
                    const poolSz = view.getUint32(pos + keyStringsOffset + 4, true);
                    keyStrings = parseArscStringPool(bytes, view, pos + keyStringsOffset, poolSz);
                }

                const pkgObj = {
                    id: pkgId,
                    name: pkgName,
                    typeStrings,
                    keyStrings,
                    types: new Map() // typeId -> { id, name, entries: Map(entryIdx -> Array of entries) }
                };
                packages.set(pkgId, pkgObj);

                // Inner chunks inside package
                const typePoolSize = typeStringsOffset > 0 ? view.getUint32(pos + typeStringsOffset + 4, true) : 0;
                const keyPoolSize = keyStringsOffset > 0 ? view.getUint32(pos + keyStringsOffset + 4, true) : 0;
                let innerPos = Math.max(288, typeStringsOffset + typePoolSize, keyStringsOffset + keyPoolSize);

                while (pos + innerPos + 8 <= pos + chunkSize) {
                    const iType = view.getUint16(pos + innerPos, true);
                    const iHeaderSize = view.getUint16(pos + innerPos + 2, true);
                    const iChunkSize = view.getUint32(pos + innerPos + 4, true);
                    if (iChunkSize < 8 || pos + innerPos + iChunkSize > pos + chunkSize) {
                        innerPos += 4;
                        continue;
                    }

                    if (iType === RES_TABLE_TYPE_TYPE) {
                        const typeId = bytes[pos + innerPos + 8];
                        const entryCount = view.getUint32(pos + innerPos + 12, true);
                        const entriesStart = view.getUint32(pos + innerPos + 16, true);
                        const typeName = typeStrings[typeId - 1] || `type_${typeId}`;

                        if (!pkgObj.types.has(typeId)) {
                            pkgObj.types.set(typeId, { id: typeId, name: typeName, entries: new Map() });
                        }
                        const arscType = pkgObj.types.get(typeId);

                        // Parse ResTable_config
                        const configOff = pos + innerPos + 20;
                        const configSize = view.getUint32(configOff, true);
                        let lang = '';
                        let country = '';
                        let density = 0;
                        if (configSize >= 28 && configOff + configSize <= pos + innerPos + iHeaderSize) {
                            const l1 = bytes[configOff + 8];
                            const l2 = bytes[configOff + 9];
                            const c1 = bytes[configOff + 10];
                            const c2 = bytes[configOff + 11];
                            if (l1 && l2) lang = String.fromCharCode(l1, l2);
                            if (c1 && c2) country = String.fromCharCode(c1, c2);
                            density = view.getUint16(configOff + 14, true);
                        }
                        const config = { lang, country, density, configSize };

                        const offsetsPos = pos + innerPos + iHeaderSize;
                        for (let entryIdx = 0; entryIdx < entryCount; entryIdx++) {
                            const offCursor = offsetsPos + entryIdx * 4;
                            if (offCursor + 4 > pos + innerPos + iChunkSize) break;

                            const entryRelOffset = view.getUint32(offCursor, true);
                            if (entryRelOffset === 0xFFFFFFFF) continue;

                            const entryAbs = pos + innerPos + entriesStart + entryRelOffset;
                            if (entryAbs + 8 > pos + innerPos + iChunkSize) continue;

                            const flags = view.getUint16(entryAbs + 2, true);
                            const keyIdx = view.getUint32(entryAbs + 4, true);
                            const keyStr = keyStrings[keyIdx] || `key_${keyIdx}`;
                            const isComplex = (flags & 0x0001) !== 0;

                            let entryRecord;
                            if (!isComplex) {
                                const valAbs = entryAbs + 8;
                                if (valAbs + 8 <= pos + innerPos + iChunkSize) {
                                    const dataType = bytes[valAbs + 3];
                                    const data = view.getUint32(valAbs + 4, true);
                                    let val = data;
                                    if (dataType === TYPE_STRING) {
                                        val = globalStrings[data] || '';
                                    }
                                    entryRecord = {
                                        key: keyStr,
                                        typeId,
                                        typeName,
                                        config,
                                        isComplex: false,
                                        dataType,
                                        data,
                                        val
                                    };
                                }
                            } else {
                                // Complex entry
                                if (entryAbs + 16 <= pos + innerPos + iChunkSize) {
                                    const parent = view.getUint32(entryAbs + 8, true);
                                    const count = view.getUint32(entryAbs + 12, true);
                                    const items = [];
                                    let itemOff = entryAbs + 16;
                                    for (let j = 0; j < count && itemOff + 12 <= pos + innerPos + iChunkSize; j++) {
                                        const name = view.getUint32(itemOff, true);
                                        const dataType = bytes[itemOff + 7];
                                        const data = view.getUint32(itemOff + 8, true);
                                        items.push({ name, dataType, data });
                                        itemOff += 12;
                                    }
                                    entryRecord = {
                                        key: keyStr,
                                        typeId,
                                        typeName,
                                        config,
                                        isComplex: true,
                                        parent,
                                        count,
                                        items
                                    };
                                }
                            }

                            if (entryRecord) {
                                if (!arscType.entries.has(entryIdx)) {
                                    arscType.entries.set(entryIdx, []);
                                }
                                arscType.entries.get(entryIdx).push(entryRecord);
                            }
                        }
                    }

                    innerPos += iChunkSize;
                }
            }

            pos += chunkSize;
        }

        return new ArscResourceTable(globalStrings, packages, targetLocale, targetDensity);
    }
}
