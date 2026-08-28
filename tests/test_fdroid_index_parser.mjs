/**
 * Test Suite: F-Droid Repository Index Ingestion & Parser Verification
 * 
 * Target: src/fdroid_index_parser.js
 * Specification: PROJECT.md Milestone 1
 * 
 * Tests:
 * 1. parseIndexJar: Uncompressed (Store) & DEFLATE compressed index-v1.jar.
 * 2. parseIndexJar: Multi-entry JAR with META-INF signature files and candidate names.
 * 3. parseIndexJar: Buffer compatibility (ArrayBuffer, Uint8Array, Buffer, direct JSON fallback).
 * 4. parseIndexJson: V1 schema with apps array & packages map.
 * 5. parseIndexJson: suggestedVersionCode vs highest versionCode resolution.
 * 6. parseIndexJson: V2 schema with localized strings & versions map.
 * 7. parseIndexJson: Simplified flat V2 schema & bare array schemas.
 * 8. Edge Cases: Missing fields, fallbacks, empty packages, sparse app entries.
 * 9. Adversarial Stress: Truncated ZIP, zero-byte buffer, missing EOCD, malformed JSON, corrupted streams.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import { strict as assert } from 'node:assert';
import zlib from 'node:zlib';
import { FdroidIndexParser, deriveDeterministicColor, resolveLocalized, resolveLocalizedIcon } from '../src/fdroid_index_parser.js';

let totalTests = 0;
let passedTests = 0;

function check(description, condition) {
    totalTests++;
    if (!condition) {
        console.error(`  ✖ [FAIL] ${description}`);
        throw new Error(`Assertion failed: ${description}`);
    }
    passedTests++;
    console.log(`  ✔ [PASS] ${description}`);
}

// -----------------------------------------------------------------------------
// Pure-JS In-Memory ZIP/JAR Generator Helper
// -----------------------------------------------------------------------------
function createZipArchive(files = []) {
    // files: Array<{ name: string, data: Uint8Array | string, method?: number }>
    const localHeaders = [];
    const cdHeaders = [];
    let offset = 0;

    for (const file of files) {
        const nameBytes = Buffer.from(file.name, 'utf-8');
        const rawBytes = typeof file.data === 'string' ? Buffer.from(file.data, 'utf-8') : Buffer.from(file.data);
        const method = file.method !== undefined ? file.method : 0; // 0=Store, 8=Deflate

        let compBytes = rawBytes;
        if (method === 8) {
            compBytes = zlib.deflateRawSync(rawBytes);
        }

        // Compute CRC32
        const crc = computeCrc32(rawBytes);

        // Local Header (30 bytes + nameLen + compSize)
        const lh = Buffer.alloc(30);
        lh.writeUInt32LE(0x04034b50, 0); // Signature
        lh.writeUInt16LE(20, 4);         // Version needed
        lh.writeUInt16LE(0, 6);          // Flags
        lh.writeUInt16LE(method, 8);     // Method
        lh.writeUInt16LE(0, 10);         // Mod time
        lh.writeUInt16LE(0, 12);         // Mod date
        lh.writeUInt32LE(crc, 14);       // CRC32
        lh.writeUInt32LE(compBytes.length, 18); // Comp size
        lh.writeUInt32LE(rawBytes.length, 22);  // Uncomp size
        lh.writeUInt16LE(nameBytes.length, 26); // Name len
        lh.writeUInt16LE(0, 28);                // Extra len

        const lhBlock = Buffer.concat([lh, nameBytes, compBytes]);
        localHeaders.push(lhBlock);

        // Central Directory Header (46 bytes + nameLen)
        const cdh = Buffer.alloc(46);
        cdh.writeUInt32LE(0x02014b50, 0); // Signature
        cdh.writeUInt16LE(20, 4);         // Version made by
        cdh.writeUInt16LE(20, 6);         // Version needed
        cdh.writeUInt16LE(0, 8);          // Flags
        cdh.writeUInt16LE(method, 10);    // Method
        cdh.writeUInt16LE(0, 12);         // Mod time
        cdh.writeUInt16LE(0, 14);         // Mod date
        cdh.writeUInt32LE(crc, 16);       // CRC32
        cdh.writeUInt32LE(compBytes.length, 20); // Comp size
        cdh.writeUInt32LE(rawBytes.length, 24);  // Uncomp size
        cdh.writeUInt16LE(nameBytes.length, 28); // Name len
        cdh.writeUInt16LE(0, 30);                // Extra len
        cdh.writeUInt16LE(0, 32);                // Comment len
        cdh.writeUInt16LE(0, 34);                // Disk start
        cdh.writeUInt16LE(0, 36);                // Internal attr
        cdh.writeUInt32LE(0, 38);                // External attr
        cdh.writeUInt32LE(offset, 42);           // Local header offset

        cdHeaders.push(Buffer.concat([cdh, nameBytes]));
        offset += lhBlock.length;
    }

    const cdBlock = Buffer.concat(cdHeaders);
    const cdOffset = offset;
    const cdSize = cdBlock.length;

    // End of Central Directory (22 bytes)
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0); // Signature
    eocd.writeUInt16LE(0, 4);          // Disk num
    eocd.writeUInt16LE(0, 6);          // Disk with CD
    eocd.writeUInt16LE(files.length, 8); // Entries on disk
    eocd.writeUInt16LE(files.length, 10); // Total entries
    eocd.writeUInt32LE(cdSize, 12);    // CD size
    eocd.writeUInt32LE(cdOffset, 16);  // CD offset
    eocd.writeUInt16LE(0, 20);         // Comment len

    return Buffer.concat([...localHeaders, cdBlock, eocd]);
}

function computeCrc32(buf) {
    let crc = ~0;
    for (let i = 0; i < buf.length; i++) {
        crc ^= buf[i];
        for (let j = 0; j < 8; j++) {
            crc = (crc >>> 1) ^ (crc & 1 ? 0xEDB88320 : 0);
        }
    }
    return (~crc) >>> 0;
}

// -----------------------------------------------------------------------------
// Test Execution
// -----------------------------------------------------------------------------
async function runTests() {
    console.log("================================================================================");
    console.log("⚡ STARTING F-DROID INDEX PARSER UNIT VERIFICATION");
    console.log("================================================================================\n");

    // =========================================================================
    // Section 1: parseIndexJar Archive Extraction
    // =========================================================================
    console.log("▶ Section 1: parseIndexJar Archive Extraction");

    const sampleV1Json = JSON.stringify({
        repo: {
            name: "F-Droid Official",
            timestamp: 1690000000000,
            icon: "icon.png",
            version: 17
        },
        apps: [
            {
                packageName: "org.mozilla.firefox",
                name: "Firefox",
                summary: "Fast, private browser",
                description: "Mozilla Firefox for Android",
                icon: "org.mozilla.firefox.png",
                suggestedVersionCode: "12345",
                categories: ["Internet"]
            }
        ],
        packages: {
            "org.mozilla.firefox": [
                {
                    versionName: "124.0.1",
                    versionCode: 12345,
                    apkName: "firefox_12345.apk"
                }
            ]
        }
    });

    // 1.1 Store ZIP (Method 0)
    const storeZip = createZipArchive([
        { name: "index-v1.json", data: sampleV1Json, method: 0 }
    ]);
    const res1 = FdroidIndexParser.parseIndexJar(storeZip);
    check("1.1: parseIndexJar reads uncompressed (Store) index-v1.json", res1 && res1.repo.name === "F-Droid Official");
    check("1.2: parseIndexJar extracts app record from Store ZIP", res1.apps.length === 1 && res1.apps[0].packageName === "org.mozilla.firefox");

    // 1.3 DEFLATE ZIP (Method 8)
    const deflateZip = createZipArchive([
        { name: "index-v1.json", data: sampleV1Json, method: 8 }
    ]);
    const res2 = FdroidIndexParser.parseIndexJar(deflateZip);
    check("1.3: parseIndexJar reads DEFLATE-compressed index-v1.json", res2 && res2.repo.name === "F-Droid Official");
    check("1.4: parseIndexJar extracts app version from DEFLATE ZIP", res2.apps[0].versionName === "124.0.1" && res2.apps[0].versionCode === 12345);

    // 1.5 Multi-file JAR with META-INF signatures
    const signedJar = createZipArchive([
        { name: "META-INF/MANIFEST.MF", data: "Manifest-Version: 1.0\n", method: 0 },
        { name: "META-INF/FDROID.SF", data: "Signature-Version: 1.0\n", method: 0 },
        { name: "META-INF/FDROID.RSA", data: new Uint8Array([0x30, 0x82, 0x01]), method: 0 },
        { name: "index-v1.json", data: sampleV1Json, method: 8 }
    ]);
    const res3 = FdroidIndexParser.parseIndexJar(signedJar);
    check("1.5: parseIndexJar ignores META-INF and selects index-v1.json", res3 && res3.apps.length === 1);

    // 1.6 Alternate candidate names (index.json, entry.json)
    const legacyJar = createZipArchive([
        { name: "index.json", data: sampleV1Json, method: 8 }
    ]);
    const resLegacy = FdroidIndexParser.parseIndexJar(legacyJar);
    check("1.6: parseIndexJar extracts index.json fallback", resLegacy && resLegacy.apps.length === 1);

    const entryJar = createZipArchive([
        { name: "entry.json", data: sampleV1Json, method: 8 }
    ]);
    const resEntry = FdroidIndexParser.parseIndexJar(entryJar);
    check("1.7: parseIndexJar extracts entry.json fallback", resEntry && resEntry.apps.length === 1);

    // 1.8 ArrayBuffer and Uint8Array compatibility
    const ab = storeZip.buffer.slice(storeZip.byteOffset, storeZip.byteOffset + storeZip.byteLength);
    const resAb = FdroidIndexParser.parseIndexJar(ab);
    check("1.8: parseIndexJar accepts ArrayBuffer input", resAb && resAb.repo.name === "F-Droid Official");

    const u8 = new Uint8Array(storeZip);
    const resU8 = FdroidIndexParser.parseIndexJar(u8);
    check("1.9: parseIndexJar accepts Uint8Array input", resU8 && resU8.apps.length === 1);

    // 1.10 Direct JSON string or buffer passed to parseIndexJar
    const resDirectStr = FdroidIndexParser.parseIndexJar(sampleV1Json);
    check("1.10: parseIndexJar accepts raw JSON string directly", resDirectStr && resDirectStr.apps.length === 1);

    const resDirectBuf = FdroidIndexParser.parseIndexJar(Buffer.from(sampleV1Json, 'utf-8'));
    check("1.11: parseIndexJar accepts raw JSON Buffer directly", resDirectBuf && resDirectBuf.apps.length === 1);

    // =========================================================================
    // Section 2: parseIndexJson V1 Schema Ingestion
    // =========================================================================
    console.log("\n▶ Section 2: parseIndexJson V1 Schema Ingestion");

    const v1Data = {
        repo: {
            name: "Main Repo",
            timestamp: 1680000000000,
            icon: "repo.png"
        },
        apps: [
            {
                packageName: "org.videolan.vlc",
                name: "VLC",
                summary: "Media player",
                description: "<p>VLC media player for Android</p>",
                icon: "vlc.png",
                suggestedVersionCode: 300,
                categories: ["Multimedia", "Video"]
            },
            {
                packageName: "com.termux",
                name: "Termux",
                summary: "Terminal emulator",
                description: "Linux environment for Android",
                icon: "termux.png",
                categories: ["System"]
            }
        ],
        packages: {
            "org.videolan.vlc": [
                { versionName: "3.4.0", versionCode: 200 },
                { versionName: "3.5.0", versionCode: 300 },
                { versionName: "3.6.0-beta", versionCode: 400 }
            ],
            "com.termux": [
                { versionName: "0.117.0", versionCode: 117 },
                { versionName: "0.118.0", versionCode: 118 }
            ]
        }
    };

    const resV1 = FdroidIndexParser.parseIndexJson(v1Data);
    check("2.1: parseIndexJson parses V1 object structure", resV1.apps.length === 2);
    check("2.2: parseIndexJson matches suggestedVersionCode (300 -> 3.5.0)", resV1.apps[0].versionCode === 300 && resV1.apps[0].versionName === "3.5.0");
    check("2.3: parseIndexJson selects highest versionCode when suggestedVersionCode is omitted (118 -> 0.118.0)", resV1.apps[1].versionCode === 118 && resV1.apps[1].versionName === "0.118.0");
    check("2.4: parseIndexJson populates applicationLabel equal to name", resV1.apps[0].applicationLabel === "VLC");
    check("2.5: parseIndexJson cleans HTML tags from description", resV1.apps[0].description === "VLC media player for Android");
    check("2.6: parseIndexJson preserves categories array", Array.isArray(resV1.apps[0].categories) && resV1.apps[0].categories[0] === "Multimedia");
    check("2.7: parseIndexJson assigns deterministic color hex", typeof resV1.apps[0].color === 'string' && resV1.apps[0].color.startsWith('#'));

    // String JSON input
    const resV1Str = FdroidIndexParser.parseIndexJson(JSON.stringify(v1Data));
    check("2.8: parseIndexJson accepts JSON string input", resV1Str && resV1Str.apps.length === 2);

    // =========================================================================
    // Section 3: parseIndexJson V2 Schema Ingestion
    // =========================================================================
    console.log("\n▶ Section 3: parseIndexJson V2 Schema Ingestion");

    const v2Data = {
        repo: {
            name: { "en-US": "F-Droid V2 Repository" },
            timestamp: 1710000000000,
            icon: { "en-US": { name: "/icons/v2.png" } }
        },
        packages: {
            "org.schabi.newpipe": {
                metadata: {
                    name: { "en-US": "NewPipe", "de": "NewPipe DE" },
                    summary: { "en-US": "Lightweight YouTube frontend" },
                    description: { "en-US": "Free and lightweight streaming front-end." },
                    icon: { "en-US": { name: "newpipe.png" } },
                    categories: ["Internet", "Video"]
                },
                versions: {
                    "v270": { manifest: { versionCode: 270, versionName: "0.27.0" } },
                    "v260": { manifest: { versionCode: 260, versionName: "0.26.0" } }
                }
            }
        }
    };

    const resV2 = FdroidIndexParser.parseIndexJson(v2Data);
    check("3.1: parseIndexJson parses V2 schema package count", resV2.apps.length === 1);
    check("3.2: parseIndexJson extracts localized repo name", resV2.repo.name === "F-Droid V2 Repository");
    check("3.3: parseIndexJson extracts localized repo icon", resV2.repo.icon === "/icons/v2.png");
    check("3.4: parseIndexJson extracts localized app metadata", resV2.apps[0].name === "NewPipe" && resV2.apps[0].summary === "Lightweight YouTube frontend");
    check("3.5: parseIndexJson resolves highest version in V2 manifest map", resV2.apps[0].versionCode === 270 && resV2.apps[0].versionName === "0.27.0");
    check("3.6: parseIndexJson extracts icon from localized object", resV2.apps[0].icon === "newpipe.png");

    // Simplified flat V2 structure (as used in android_network.js)
    const flatV2Data = {
        repo: { name: "Simulated Repo", timestamp: 123456 },
        packages: {
            "org.mozilla.firefox": { name: "Firefox", version: "124.0", versionCode: 1240 }
        }
    };
    const resFlatV2 = FdroidIndexParser.parseIndexJson(flatV2Data);
    check("3.7: parseIndexJson parses simplified flat V2 structure", resFlatV2.apps.length === 1 && resFlatV2.apps[0].name === "Firefox" && resFlatV2.apps[0].versionName === "124.0");

    // Bare Array Input
    const bareArrayData = [
        { packageName: "org.test.bare", name: "Bare App", versionName: "2.0", versionCode: 20 }
    ];
    const resBareArray = FdroidIndexParser.parseIndexJson(bareArrayData);
    check("3.8: parseIndexJson handles bare array of apps", resBareArray.apps.length === 1 && resBareArray.apps[0].name === "Bare App");

    // =========================================================================
    // Section 4: Edge Cases & Field Fallbacks
    // =========================================================================
    console.log("\n▶ Section 4: Edge Cases & Field Fallbacks");

    const sparseData = {
        apps: [
            { packageName: "com.example.minimal" }
        ]
    };
    const resSparse = FdroidIndexParser.parseIndexJson(sparseData);
    check("4.1: Missing name falls back to packageName", resSparse.apps[0].name === "com.example.minimal");
    check("4.2: Missing summary defaults to empty string", resSparse.apps[0].summary === "");
    check("4.3: Missing description defaults to empty string", resSparse.apps[0].description === "");
    check("4.4: Missing icon defaults to empty string", resSparse.apps[0].icon === "");
    check("4.5: Missing categories defaults to empty array", Array.isArray(resSparse.apps[0].categories) && resSparse.apps[0].categories.length === 0);
    check("4.6: Missing repo defaults to standard F-Droid repo object", resSparse.repo && resSparse.repo.name === "F-Droid");
    check("4.7: Missing version defaults to '1.0' and 1", resSparse.apps[0].versionName === "1.0" && resSparse.apps[0].versionCode === 1);

    // Empty packages handling
    const emptyRes1 = FdroidIndexParser.parseIndexJson({});
    check("4.8: Empty object returns empty apps array", Array.isArray(emptyRes1.apps) && emptyRes1.apps.length === 0);

    const emptyRes2 = FdroidIndexParser.parseIndexJson({ apps: [], packages: {} });
    check("4.9: Empty apps and packages returns empty apps array", emptyRes2.apps.length === 0);

    const nullRes = FdroidIndexParser.parseIndexJson(null);
    check("4.10: Null JSON input returns empty apps array", Array.isArray(nullRes.apps) && nullRes.apps.length === 0);

    // Deduplication of packages
    const dupData = {
        apps: [
            { packageName: "com.dup.app", name: "Dup 1" },
            { packageName: "com.dup.app", name: "Dup 2" }
        ]
    };
    const resDup = FdroidIndexParser.parseIndexJson(dupData);
    check("4.11: Deduplicates duplicate package records", resDup.apps.length === 1 && resDup.apps[0].name === "Dup 1");

    // Helper functions coverage
    check("4.12: deriveDeterministicColor returns deterministic hex", deriveDeterministicColor("com.termux") === deriveDeterministicColor("com.termux"));
    check("4.13: resolveLocalized resolves en-US or fallback", resolveLocalized({ "en-US": "Hello" }, "en-US") === "Hello");
    check("4.14: resolveLocalizedIcon extracts .name from object", resolveLocalizedIcon({ "en-US": { name: "test.png" } }) === "test.png");

    // =========================================================================
    // Section 5: Adversarial & Corrupted Archive Invariants
    // =========================================================================
    console.log("\n▶ Section 5: Adversarial & Corrupted Archive Invariants");

    // 5.1 Invalid JSON String
    let threwJson = false;
    try {
        FdroidIndexParser.parseIndexJson("{ bad json syntax: true ");
    } catch (err) {
        threwJson = err.message.includes("Malformed JSON");
    }
    check("5.1: Malformed JSON syntax throws descriptive Error", threwJson === true);

    // 5.2 Zero-byte buffer to parseIndexJar
    let threwEmptyZip = false;
    try {
        FdroidIndexParser.parseIndexJar(new Uint8Array(0));
    } catch (_) {
        threwEmptyZip = true;
    }
    check("5.2: Zero-byte buffer throws Error", threwEmptyZip === true);

    // 5.3 Truncated buffer (< 22 bytes)
    let threwShortZip = false;
    try {
        FdroidIndexParser.parseIndexJar(new Uint8Array([0x50, 0x4b, 0x03, 0x04]));
    } catch (_) {
        threwShortZip = true;
    }
    check("5.3: Truncated ZIP buffer throws Error", threwShortZip === true);

    // 5.4 Missing index JSON in ZIP
    const missingIndexZip = createZipArchive([
        { name: "README.md", data: "# Empty Repo", method: 0 }
    ]);
    let threwMissingIndex = false;
    try {
        FdroidIndexParser.parseIndexJar(missingIndexZip);
    } catch (err) {
        threwMissingIndex = err.message.includes("No index JSON found inside archive");
    }
    check("5.4: ZIP without index-v1.json throws Error", threwMissingIndex === true);

    // 5.5 Null buffer to parseIndexJar
    let threwNullJar = false;
    try {
        FdroidIndexParser.parseIndexJar(null);
    } catch (err) {
        threwNullJar = err.message.includes("Input buffer is null or undefined");
    }
    check("5.5: Null buffer throws Error", threwNullJar === true);

    console.log("\n================================================================================");
    console.log(`📊 EXECUTION SUMMARY: ${passedTests}/${totalTests} Tests Passed`);
    console.log("================================================================================");

    if (passedTests === totalTests) {
        console.log("✔ F-Droid Index Parser Test Suite PASSED cleanly with zero failures!\n");
        process.exit(0);
    } else {
        console.error(`✖ ${totalTests - passedTests} tests FAILED!`);
        process.exit(1);
    }
}

runTests().catch(err => {
    console.error("Fatal test runner error:", err);
    process.exit(1);
});
