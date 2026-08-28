/**
 * AndroidWebGPU - Authentic F-Droid Repository Index Ingestion & Decoder
 * 
 * Provides pure-JS ingestion and normalization of F-Droid repository index metadata.
 * Supports index-v1.jar / index-v1.json (V1 schema) and index-v2.json / entry.json (V2 schema).
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import { ApkZipReader } from './apk_client_parser.js';

// Deterministic Material You color palette
const MATERIAL_PALETTE = [
    '#10b981', '#ef4444', '#f97316', '#0ea5e9',
    '#8b5cf6', '#059669', '#14b8a6', '#6366f1',
    '#3b82f6', '#a855f7', '#0284c7', '#06b6d4',
    '#ec4899', '#22c55e'
];

/**
 * Derives a deterministic hex color from a string.
 * @param {string} str
 * @returns {string}
 */
export function deriveDeterministicColor(str) {
    if (!str || typeof str !== 'string') return '#334155';
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
        hash = ((hash << 5) - hash) + str.charCodeAt(i);
        hash |= 0;
    }
    return MATERIAL_PALETTE[Math.abs(hash) % MATERIAL_PALETTE.length];
}

/**
 * Resolves localized string dictionary to a single string value.
 * Fallback order: locale (default 'en-US') -> base language (e.g. 'en') -> 'en-US' -> 'en' -> 'default' -> first non-empty string -> fallback.
 * @param {string | object} field
 * @param {string} [locale='en-US']
 * @param {string} [fallback='']
 * @returns {string}
 */
export function resolveLocalized(field, locale = 'en-US', fallback = '') {
    if (field === null || field === undefined) return fallback;
    if (typeof field === 'string') return field;
    if (typeof field === 'number' || typeof field === 'boolean') return String(field);
    if (typeof field === 'object') {
        if (typeof field[locale] === 'string' && field[locale].length > 0) return field[locale];
        const baseLang = typeof locale === 'string' ? locale.split('-')[0] : 'en';
        if (typeof field[baseLang] === 'string' && field[baseLang].length > 0) return field[baseLang];
        for (const key of ['en-US', 'en', 'default']) {
            if (typeof field[key] === 'string' && field[key].length > 0) return field[key];
        }
        for (const val of Object.values(field)) {
            if (typeof val === 'string' && val.length > 0) return val;
        }
    }
    return fallback;
}

/**
 * Resolves localized icon object or string to an icon identifier/filename.
 * @param {string | object} field
 * @param {string} [locale='en-US']
 * @param {string} [fallback='']
 * @returns {string}
 */
export function resolveLocalizedIcon(field, locale = 'en-US', fallback = '') {
    if (!field) return fallback;
    if (typeof field === 'string') return field;
    if (typeof field === 'object') {
        if (typeof field.name === 'string') return field.name;
        const entry = field[locale] || field['en-US'] || field['en'] || Object.values(field)[0];
        if (typeof entry === 'string') return entry;
        if (entry && typeof entry === 'object' && typeof entry.name === 'string') return entry.name;
    }
    return fallback;
}

/**
 * Strips HTML tags and collapses whitespace.
 * @param {string} text
 * @returns {string}
 */
function cleanText(text) {
    if (!text || typeof text !== 'string') return '';
    return text.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
}

export class FdroidIndexParser {
    /**
     * Parses an F-Droid repository JAR archive (index-v1.jar or index.jar).
     * @param {ArrayBuffer | Uint8Array | Buffer | string} jarBuffer
     * @returns {{ repo: { name: string, timestamp: number, icon?: string }, apps: Array<object> }}
     */
    static parseIndexJar(jarBuffer) {
        if (!jarBuffer) {
            throw new Error("Input buffer is null or undefined");
        }

        // Direct string input fallback
        if (typeof jarBuffer === 'string') {
            return FdroidIndexParser.parseIndexJson(jarBuffer);
        }

        // Validate buffer length
        const byteLen = jarBuffer.byteLength !== undefined ? jarBuffer.byteLength : jarBuffer.length;
        if (typeof byteLen !== 'number' || byteLen === 0) {
            throw new Error("Buffer too short to be a valid ZIP archive");
        }

        // Attempt ZIP archive decompression
        try {
            const zip = new ApkZipReader(jarBuffer);
            zip.readEntries();

            const candidates = [
                'index-v1.json',
                'index.json',
                'index-v2.json',
                'entry.json'
            ];

            let jsonBytes = null;
            for (const name of candidates) {
                jsonBytes = zip.readFile(name);
                if (jsonBytes) break;
            }

            if (!jsonBytes) {
                const entries = zip.listEntries();
                const anyJson = entries.find(e => e.endsWith('.json') && !e.startsWith('META-INF/'));
                if (anyJson) {
                    jsonBytes = zip.readFile(anyJson);
                }
            }

            if (!jsonBytes) {
                throw new Error("Invalid F-Droid JAR: No index JSON found inside archive (index-v1.json or index.json)");
            }

            const jsonText = new TextDecoder('utf-8').decode(jsonBytes);
            return FdroidIndexParser.parseIndexJson(jsonText);
        } catch (zipErr) {
            // Direct JSON byte stream fallback if not a valid ZIP
            if (byteLen >= 2) {
                try {
                    const text = new TextDecoder('utf-8').decode(
                        jarBuffer instanceof ArrayBuffer ? new Uint8Array(jarBuffer) : jarBuffer
                    );
                    const trimmed = text.trim();
                    if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
                        return FdroidIndexParser.parseIndexJson(trimmed);
                    }
                } catch (_) {}
            }

            throw new Error(`Failed to parse F-Droid index JAR archive: ${zipErr.message}`);
        }
    }

    /**
     * Parses F-Droid repository JSON text or parsed object.
     * @param {string | object} jsonTextOrObj
     * @returns {{ repo: { name: string, timestamp: number, icon?: string }, apps: Array<object> }}
     */
    static parseIndexJson(jsonTextOrObj) {
        if (jsonTextOrObj === null || jsonTextOrObj === undefined) {
            return {
                repo: { name: "F-Droid", timestamp: Date.now() },
                apps: []
            };
        }

        let rawObj;
        if (typeof jsonTextOrObj === 'string') {
            const trimmed = jsonTextOrObj.trim();
            if (!trimmed) {
                return {
                    repo: { name: "F-Droid", timestamp: Date.now() },
                    apps: []
                };
            }
            try {
                rawObj = JSON.parse(trimmed);
            } catch (err) {
                throw new Error(`Malformed JSON in F-Droid index: ${err.message}`);
            }
        } else if (typeof jsonTextOrObj === 'object') {
            rawObj = jsonTextOrObj;
        } else {
            throw new Error("Invalid input type: expected JSON string or object");
        }

        // Direct array of application objects
        if (Array.isArray(rawObj)) {
            return {
                repo: { name: "F-Droid", timestamp: Date.now() },
                apps: FdroidIndexParser._normalizeAppArray(rawObj)
            };
        }

        const repo = FdroidIndexParser._normalizeRepo(rawObj.repo);

        // V1 Format: apps array + optional packages dictionary
        if (Array.isArray(rawObj.apps)) {
            return {
                repo,
                apps: FdroidIndexParser._parseV1Apps(rawObj.apps, rawObj.packages || {})
            };
        }

        // V2 Format or Flat Map: packages object
        if (rawObj.packages && typeof rawObj.packages === 'object') {
            return {
                repo,
                apps: FdroidIndexParser._parseV2Packages(rawObj.packages)
            };
        }

        return { repo, apps: [] };
    }

    /**
     * Normalizes repository header metadata.
     */
    static _normalizeRepo(rawRepo) {
        if (!rawRepo || typeof rawRepo !== 'object') {
            return { name: "F-Droid", timestamp: Date.now() };
        }
        const name = resolveLocalized(rawRepo.name, 'en-US', 'F-Droid');
        const icon = resolveLocalizedIcon(rawRepo.icon, 'en-US', undefined);
        const timestamp = Number(rawRepo.timestamp) || Date.now();
        const result = { name, timestamp };
        if (icon) result.icon = icon;
        return result;
    }

    /**
     * Parses V1 apps array with packages map for version matching.
     */
    static _parseV1Apps(appsList, packagesMap) {
        const result = [];
        const seenPackages = new Set();

        for (const app of appsList) {
            if (!app || typeof app !== 'object') continue;
            const pkgName = app.packageName || app.id || app.package;
            if (!pkgName || typeof pkgName !== 'string' || seenPackages.has(pkgName)) continue;
            seenPackages.add(pkgName);

            const pkgList = (packagesMap && typeof packagesMap === 'object' && Array.isArray(packagesMap[pkgName]))
                ? packagesMap[pkgName]
                : [];

            let matchedPkg = null;
            if (pkgList.length > 0) {
                if (app.suggestedVersionCode !== undefined && app.suggestedVersionCode !== null) {
                    const targetCode = Number(app.suggestedVersionCode);
                    matchedPkg = pkgList.find(p => p && Number(p.versionCode) === targetCode);
                }
                if (!matchedPkg) {
                    matchedPkg = [...pkgList].filter(Boolean).sort((a, b) => (Number(b.versionCode) || 0) - (Number(a.versionCode) || 0))[0];
                }
            }

            const locObj = (app.localized && typeof app.localized === 'object') ? (app.localized['en-US'] || app.localized['en'] || Object.values(app.localized)[0]) : null;

            const name = resolveLocalized(app.name || app.applicationLabel || locObj?.name, 'en-US', pkgName);
            const versionName = matchedPkg?.versionName || app.suggestedVersionName || "1.0";
            const versionCode = Number(matchedPkg?.versionCode ?? app.suggestedVersionCode) || 1;
            const summary = resolveLocalized(app.summary || locObj?.summary, 'en-US', '');
            const rawDesc = resolveLocalized(app.description || locObj?.description, 'en-US', summary);
            const description = cleanText(rawDesc) || summary;
            const cleanSummary = summary || (description ? cleanText(description).slice(0, 120) : "");
            const icon = resolveLocalizedIcon(app.icon || locObj?.icon, 'en-US', '');
            const categories = Array.isArray(app.categories) ? app.categories.filter(c => typeof c === 'string') : [];
            const color = app.color || deriveDeterministicColor(pkgName);

            result.push({
                packageName: pkgName,
                name,
                applicationLabel: name,
                summary: cleanSummary,
                description,
                icon,
                versionName: String(versionName),
                versionCode,
                color,
                categories
            });
        }
        return result;
    }

    /**
     * Parses V2 packages map (or simplified flat packages map).
     */
    static _parseV2Packages(packagesMap) {
        const result = [];
        const seenPackages = new Set();

        for (const [pkgName, pkgData] of Object.entries(packagesMap)) {
            if (!pkgName || typeof pkgName !== 'string' || seenPackages.has(pkgName)) continue;
            seenPackages.add(pkgName);

            if (!pkgData || typeof pkgData !== 'object') continue;

            if (Array.isArray(pkgData)) {
                // V1 packages map entry without prior app entry
                const firstPkg = pkgData[0] || {};
                const name = pkgName.split('.').pop() || pkgName;
                result.push({
                    packageName: pkgName,
                    name,
                    applicationLabel: name,
                    summary: '',
                    description: '',
                    icon: '',
                    versionName: String(firstPkg.versionName || '1.0'),
                    versionCode: Number(firstPkg.versionCode) || 1,
                    color: deriveDeterministicColor(pkgName),
                    categories: []
                });
                continue;
            }

            // V2 package item or flat object
            const metadata = pkgData.metadata || pkgData;
            const versions = pkgData.versions || {};
            const versionEntries = Array.isArray(versions) ? versions : Object.values(versions);

            let bestVersion = null;
            let maxCode = -1;
            for (const v of versionEntries) {
                if (!v || typeof v !== 'object') continue;
                const code = Number(v.manifest?.versionCode ?? v.versionCode ?? 0);
                if (code > maxCode) {
                    maxCode = code;
                    bestVersion = v;
                }
            }

            const name = resolveLocalized(metadata.name || metadata.applicationLabel, 'en-US', pkgName);
            const summary = resolveLocalized(metadata.summary, 'en-US', '');
            const rawDesc = resolveLocalized(metadata.description, 'en-US', summary);
            const description = cleanText(rawDesc) || summary;
            const cleanSummary = summary || (description ? cleanText(description).slice(0, 120) : "");
            const icon = resolveLocalizedIcon(metadata.icon, 'en-US', '');
            const categories = Array.isArray(metadata.categories) ? metadata.categories.filter(c => typeof c === 'string') : [];
            const versionName = bestVersion?.manifest?.versionName ?? bestVersion?.versionName ?? pkgData.version ?? metadata.versionName ?? "1.0";
            const versionCode = Number(bestVersion?.manifest?.versionCode ?? bestVersion?.versionCode ?? pkgData.versionCode ?? metadata.versionCode ?? (maxCode > 0 ? maxCode : 1)) || 1;
            const color = metadata.color || pkgData.color || deriveDeterministicColor(pkgName);

            result.push({
                packageName: pkgName,
                name,
                applicationLabel: name,
                summary: cleanSummary,
                description,
                icon,
                versionName: String(versionName),
                versionCode,
                color,
                categories
            });
        }
        return result;
    }

    /**
     * Normalizes bare array of app objects.
     */
    static _normalizeAppArray(arr) {
        const result = [];
        const seenPackages = new Set();
        for (const item of arr) {
            if (!item || typeof item !== 'object') continue;
            const pkgName = item.packageName || item.id || item.package;
            if (!pkgName || typeof pkgName !== 'string' || seenPackages.has(pkgName)) continue;
            seenPackages.add(pkgName);

            const name = resolveLocalized(item.name || item.applicationLabel, 'en-US', pkgName);
            const summary = resolveLocalized(item.summary, 'en-US', '');
            const rawDesc = resolveLocalized(item.description, 'en-US', summary);
            const description = cleanText(rawDesc) || summary;
            const cleanSummary = summary || (description ? cleanText(description).slice(0, 120) : "");
            const icon = resolveLocalizedIcon(item.icon, 'en-US', '');
            const categories = Array.isArray(item.categories) ? item.categories.filter(c => typeof c === 'string') : [];
            const versionName = item.versionName || item.version || "1.0";
            const versionCode = Number(item.versionCode) || 1;
            const color = item.color || deriveDeterministicColor(pkgName);

            result.push({
                packageName: pkgName,
                name,
                applicationLabel: name,
                summary: cleanSummary,
                description,
                icon,
                versionName: String(versionName),
                versionCode,
                color,
                categories
            });
        }
        return result;
    }
}
