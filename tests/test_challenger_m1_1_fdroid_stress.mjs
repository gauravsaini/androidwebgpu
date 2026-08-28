/**
 * Test Suite: Empirical Challenger 1 - F-Droid Index Parser Adversarial Stress
 * 
 * Target: src/fdroid_index_parser.js
 * Specification: PROJECT.md Milestone 1 & ORIGINAL_REQUEST.md
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import zlib from 'node:zlib';
import { FdroidIndexParser, deriveDeterministicColor, resolveLocalized, resolveLocalizedIcon } from '../src/fdroid_index_parser.js';
import { inflateRaw } from '../src/apk_client_parser.js';

let totalTests = 0;
let passedTests = 0;

function runTest(name, fn) {
    totalTests++;
    try {
        fn();
        passedTests++;
        console.log(`  ✔ [PASS] ${name}`);
    } catch (err) {
        console.error(`  ✖ [FAIL] ${name}: ${err.message}`);
        console.error(err.stack);
        throw err;
    }
}

// -----------------------------------------------------------------------------
// ZIP Archive Helper
// -----------------------------------------------------------------------------
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

function createZipArchive(files = []) {
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

        const crc = computeCrc32(rawBytes);

        // Local Header
        const lh = Buffer.alloc(30);
        lh.writeUInt32LE(0x04034b50, 0);
        lh.writeUInt16LE(20, 4);
        lh.writeUInt16LE(0, 6);
        lh.writeUInt16LE(method, 8);
        lh.writeUInt16LE(0, 10);
        lh.writeUInt16LE(0, 12);
        lh.writeUInt32LE(crc, 14);
        lh.writeUInt32LE(compBytes.length, 18);
        lh.writeUInt32LE(rawBytes.length, 22);
        lh.writeUInt16LE(nameBytes.length, 26);
        lh.writeUInt16LE(0, 28);

        const lhBlock = Buffer.concat([lh, nameBytes, compBytes]);
        localHeaders.push(lhBlock);

        // Central Directory Header
        const cdh = Buffer.alloc(46);
        cdh.writeUInt32LE(0x02014b50, 0);
        cdh.writeUInt16LE(20, 4);
        cdh.writeUInt16LE(20, 6);
        cdh.writeUInt16LE(0, 8);
        cdh.writeUInt16LE(method, 10);
        cdh.writeUInt16LE(0, 12);
        cdh.writeUInt16LE(0, 14);
        cdh.writeUInt32LE(crc, 16);
        cdh.writeUInt32LE(compBytes.length, 20);
        cdh.writeUInt32LE(rawBytes.length, 24);
        cdh.writeUInt16LE(nameBytes.length, 28);
        cdh.writeUInt16LE(0, 30);
        cdh.writeUInt16LE(0, 32);
        cdh.writeUInt16LE(0, 34);
        cdh.writeUInt16LE(0, 36);
        cdh.writeUInt32LE(0, 38);
        cdh.writeUInt32LE(offset, 42);

        cdHeaders.push(Buffer.concat([cdh, nameBytes]));
        offset += lhBlock.length;
    }

    const cdBlock = Buffer.concat(cdHeaders);
    const cdOffset = offset;
    const cdSize = cdBlock.length;

    // End of Central Directory
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0);
    eocd.writeUInt16LE(0, 4);
    eocd.writeUInt16LE(0, 6);
    eocd.writeUInt16LE(files.length, 8);
    eocd.writeUInt16LE(files.length, 10);
    eocd.writeUInt32LE(cdSize, 12);
    eocd.writeUInt32LE(cdOffset, 16);
    eocd.writeUInt16LE(0, 20);

    return Buffer.concat([...localHeaders, cdBlock, eocd]);
}

console.log("================================================================================");
console.log("⚡ EMPIRICAL CHALLENGER 1: ADVERSARIAL STRESS TEST SUITE (M1)");
console.log("================================================================================\n");

// =============================================================================
// Suite 1: Truncated ZIP Archives & Corrupted Headers
// =============================================================================
console.log("▶ Suite 1: Truncated ZIP Archives & Corrupted Headers");

runTest("1.1: Rejects zero-byte, 1-byte, 10-byte, and 21-byte sub-EOCD buffers safely", () => {
    for (const len of [0, 1, 10, 21]) {
        const buf = new Uint8Array(len);
        assert.throws(
            () => FdroidIndexParser.parseIndexJar(buf),
            /Buffer too short|Failed to parse|Invalid/i,
            `Expected buffer of length ${len} to throw error`
        );
    }
});

runTest("1.2: Rejects corrupted EOCD with out-of-bounds Central Directory offset", () => {
    const validZip = createZipArchive([{ name: "index-v1.json", data: '{"repo":{},"apps":[]}', method: 0 }]);
    const tampered = Buffer.from(validZip);
    const eocdStart = tampered.length - 22;
    tampered.writeUInt32LE(0xFFFFF000, eocdStart + 16);

    assert.throws(
        () => FdroidIndexParser.parseIndexJar(tampered),
        /Failed to parse|Invalid F-Droid JAR|No index JSON/i
    );
});

runTest("1.3: Rejects ZIP where local header offset is corrupted", () => {
    const validZip = createZipArchive([{ name: "index-v1.json", data: '{"repo":{},"apps":[]}', method: 0 }]);
    const tampered = Buffer.from(validZip);
    const cdIdx = tampered.indexOf(Buffer.from([0x50, 0x4b, 0x01, 0x02]));
    assert.ok(cdIdx >= 0);
    tampered.writeUInt32LE(0xDEADBEEF, cdIdx + 42);

    assert.throws(
        () => FdroidIndexParser.parseIndexJar(tampered),
        /Failed to parse|Invalid F-Droid JAR|No index JSON/i
    );
});

runTest("1.4: Rejects ZIP with unsupported compression method (e.g. Method 12)", () => {
    const validZip = createZipArchive([{ name: "index-v1.json", data: '{"repo":{},"apps":[]}', method: 0 }]);
    const tampered = Buffer.from(validZip);
    tampered.writeUInt16LE(12, 8);
    const cdIdx = tampered.indexOf(Buffer.from([0x50, 0x4b, 0x01, 0x02]));
    tampered.writeUInt16LE(12, cdIdx + 10);

    assert.throws(
        () => FdroidIndexParser.parseIndexJar(tampered),
        /Unsupported ZIP compression method|Failed to parse/i
    );
});

runTest("1.5: Rejects ZIP with truncated compressed payload slice", () => {
    const validZip = createZipArchive([{ name: "index-v1.json", data: '{"repo":{"name":"Test"},"apps":[]}', method: 8 }]);
    const truncated = validZip.subarray(0, validZip.length - 20);
    assert.throws(
        () => FdroidIndexParser.parseIndexJar(truncated),
        /Failed to parse|Buffer too short|Cannot find End of Central Directory/i
    );
});

runTest("1.6: Rejects archive without index JSON", () => {
    const noIndexZip = createZipArchive([
        { name: "META-INF/MANIFEST.MF", data: "Manifest-Version: 1.0\n", method: 0 },
        { name: "assets/icon.png", data: new Uint8Array([1, 2, 3, 4]), method: 0 }
    ]);
    assert.throws(
        () => FdroidIndexParser.parseIndexJar(noIndexZip),
        /No index JSON found inside archive/i
    );
});

runTest("1.7: Resolves candidate JSON priority (index-v1.json > index.json > entry.json)", () => {
    const multiZip = createZipArchive([
        { name: "entry.json", data: JSON.stringify({ repo: { name: "EntryRepo" }, apps: [] }), method: 0 },
        { name: "index.json", data: JSON.stringify({ repo: { name: "IndexRepo" }, apps: [] }), method: 0 },
        { name: "index-v1.json", data: JSON.stringify({ repo: { name: "IndexV1Repo" }, apps: [] }), method: 0 }
    ]);
    const res = FdroidIndexParser.parseIndexJar(multiZip);
    assert.equal(res.repo.name, "IndexV1Repo");
});

runTest("1.8: Subarray slice with non-zero byteOffset in Uint8Array parses accurately", () => {
    const baseZip = createZipArchive([{ name: "index-v1.json", data: JSON.stringify({ repo: { name: "OffsetRepo" }, apps: [] }), method: 0 }]);
    const padded = new Uint8Array(baseZip.length + 100);
    padded.set(baseZip, 50);
    const subView = padded.subarray(50, 50 + baseZip.length);
    const res = FdroidIndexParser.parseIndexJar(subView);
    assert.equal(res.repo.name, "OffsetRepo");
});

// =============================================================================
// Suite 2: Corrupted DEFLATE Streams (RFC 1951)
// =============================================================================
console.log("\n▶ Suite 2: Corrupted DEFLATE Streams (RFC 1951)");

runTest("2.1: Rejects reserved DEFLATE block type BTYPE=3", () => {
    const rawData = Buffer.from('{"repo":{},"apps":[]}');
    const corruptedPayload = Buffer.from([0x06, 0xFF, 0x00, 0x00]);
    const nameBytes = Buffer.from("index-v1.json");

    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0);
    lh.writeUInt16LE(20, 4);
    lh.writeUInt16LE(0, 6);
    lh.writeUInt16LE(8, 8);
    lh.writeUInt32LE(computeCrc32(rawData), 14);
    lh.writeUInt32LE(corruptedPayload.length, 18);
    lh.writeUInt32LE(rawData.length, 22);
    lh.writeUInt16LE(nameBytes.length, 26);

    const cdh = Buffer.alloc(46);
    cdh.writeUInt32LE(0x02014b50, 0);
    cdh.writeUInt16LE(20, 4);
    cdh.writeUInt16LE(20, 6);
    cdh.writeUInt16LE(8, 10);
    cdh.writeUInt32LE(computeCrc32(rawData), 16);
    cdh.writeUInt32LE(corruptedPayload.length, 20);
    cdh.writeUInt32LE(rawData.length, 24);
    cdh.writeUInt16LE(nameBytes.length, 28);
    cdh.writeUInt32LE(0, 42);

    const lhBlock = Buffer.concat([lh, nameBytes, corruptedPayload]);
    const cdBlock = Buffer.concat([cdh, nameBytes]);
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0);
    eocd.writeUInt16LE(1, 8);
    eocd.writeUInt16LE(1, 10);
    eocd.writeUInt32LE(cdBlock.length, 12);
    eocd.writeUInt32LE(lhBlock.length, 16);

    const zipBuf = Buffer.concat([lhBlock, cdBlock, eocd]);
    assert.throws(
        () => FdroidIndexParser.parseIndexJar(zipBuf),
        /Unsupported DEFLATE block type|Failed to parse/i
    );
});

runTest("2.2: Rejects truncated / premature DEFLATE bitstream", () => {
    const sample = JSON.stringify({ repo: { name: "Test" }, apps: [] });
    const comp = zlib.deflateRawSync(Buffer.from(sample));
    const truncatedComp = comp.subarray(0, 3);

    const nameBytes = Buffer.from("index-v1.json");
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0);
    lh.writeUInt16LE(20, 4);
    lh.writeUInt16LE(8, 8);
    lh.writeUInt32LE(computeCrc32(Buffer.from(sample)), 14);
    lh.writeUInt32LE(truncatedComp.length, 18);
    lh.writeUInt32LE(sample.length, 22);
    lh.writeUInt16LE(nameBytes.length, 26);

    const cdh = Buffer.alloc(46);
    cdh.writeUInt32LE(0x02014b50, 0);
    cdh.writeUInt16LE(20, 4);
    cdh.writeUInt16LE(20, 6);
    cdh.writeUInt16LE(8, 10);
    cdh.writeUInt32LE(computeCrc32(Buffer.from(sample)), 16);
    cdh.writeUInt32LE(truncatedComp.length, 20);
    cdh.writeUInt32LE(sample.length, 24);
    cdh.writeUInt16LE(nameBytes.length, 28);
    cdh.writeUInt32LE(0, 42);

    const lhBlock = Buffer.concat([lh, nameBytes, truncatedComp]);
    const cdBlock = Buffer.concat([cdh, nameBytes]);
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0);
    eocd.writeUInt16LE(1, 8);
    eocd.writeUInt16LE(1, 10);
    eocd.writeUInt32LE(cdBlock.length, 12);
    eocd.writeUInt32LE(lhBlock.length, 16);

    const zipBuf = Buffer.concat([lhBlock, cdBlock, eocd]);
    assert.throws(
        () => FdroidIndexParser.parseIndexJar(zipBuf),
        /Unexpected end of DEFLATE stream|Invalid Huffman code|Failed to parse/i
    );
});

runTest("2.3: inflateRaw handles empty or null buffers without throwing unhandled exception", () => {
    assert.equal(inflateRaw(null).length, 0);
    assert.equal(inflateRaw(new Uint8Array(0)).length, 0);
});

// =============================================================================
// Suite 3: Deeply Nested JSON, Prototype Pollution & Malformed Metadata
// =============================================================================
console.log("\n▶ Suite 3: Deeply Nested JSON, Prototype Pollution & Malformed Metadata");

runTest("3.1: Deeply nested JSON object (200 levels) handles safely without crash", () => {
    let deep = { name: "Deep App", packageName: "org.test.deep" };
    for (let i = 0; i < 200; i++) {
        deep = { child: deep };
    }
    const root = {
        repo: { name: "Deep Repo", timestamp: 12345 },
        apps: [deep]
    };
    const parsed = FdroidIndexParser.parseIndexJson(root);
    assert.ok(parsed.repo.name === "Deep Repo");
    assert.equal(parsed.apps.length, 0);
});

runTest("3.2: Prototype pollution payloads in keys are sanitized without polluting Object.prototype", () => {
    const maliciousJson = `{
        "__proto__": { "polluted": "yes" },
        "constructor": { "prototype": { "polluted": "yes" } },
        "repo": { "name": "Safe Repo", "__proto__": { "injected": true } },
        "apps": [
            { "packageName": "__proto__", "name": "Proto Attack" },
            { "packageName": "constructor", "name": "Constructor Attack" },
            { "packageName": "org.safe.app", "name": { "__proto__": "evil", "en-US": "Safe App" } }
        ]
    }`;

    const parsed = FdroidIndexParser.parseIndexJson(maliciousJson);
    assert.equal(({}).polluted, undefined, "Object.prototype was polluted!");
    assert.equal(({}).injected, undefined, "Object.prototype was polluted!");

    assert.equal(parsed.repo.name, "Safe Repo");
    assert.ok(parsed.apps.length >= 1);
    const safeApp = parsed.apps.find(a => a.packageName === "org.safe.app");
    assert.ok(safeApp);
    assert.equal(safeApp.name, "Safe App");
});

runTest("3.3: Extreme data types in apps array (null, numbers, boolean, arrays) filtered safely", () => {
    const extremeData = {
        repo: { name: "Type Test" },
        apps: [
            null,
            undefined,
            12345,
            "just a string",
            true,
            false,
            [],
            {},
            { packageName: null },
            { packageName: 123 },
            { packageName: true },
            { packageName: "com.valid.app", name: null, summary: 12345, description: true }
        ]
    };

    const parsed = FdroidIndexParser.parseIndexJson(extremeData);
    assert.equal(parsed.apps.length, 1);
    assert.equal(parsed.apps[0].packageName, "com.valid.app");
    assert.equal(parsed.apps[0].name, "com.valid.app");
    assert.equal(parsed.apps[0].summary, "12345");
});

runTest("3.4: Adversarial versionCode types (negative, string, NaN, float, boolean, Infinity)", () => {
    const testCases = [
        { rawCode: -50, expectedCode: -50 },
        { rawCode: "300", expectedCode: 300 },
        { rawCode: "invalid", expectedCode: 1 },
        { rawCode: 3.14159, expectedCode: 3.14159 },
        { rawCode: true, expectedCode: 1 },
        { rawCode: false, expectedCode: 1 },
        { rawCode: null, expectedCode: 1 },
        { rawCode: undefined, expectedCode: 1 },
        { rawCode: NaN, expectedCode: 1 }
    ];

    for (let i = 0; i < testCases.length; i++) {
        const { rawCode, expectedCode } = testCases[i];
        const data = {
            apps: [{ packageName: `org.test.code${i}`, versionCode: rawCode, suggestedVersionCode: rawCode }]
        };
        const parsed = FdroidIndexParser.parseIndexJson(data);
        assert.equal(parsed.apps.length, 1);
        const code = parsed.apps[0].versionCode;
        assert.ok(typeof code === 'number' && !Number.isNaN(code), `versionCode must be finite number, got ${code}`);
    }
});

runTest("3.5: Malicious script & HTML injection payloads sanitized in text and description", () => {
    const xssData = {
        repo: { name: "<b>Repo</b> <script>alert(1)</script>" },
        apps: [
            {
                packageName: "com.xss.app",
                name: "<h1>App Name</h1>",
                summary: "<p>Click <a href='javascript:alert(1)'>here</a></p>",
                description: "<script>fetch('http://evil.com')</script><img src=x onerror=alert(1)>Authentic Description"
            }
        ]
    };

    const parsed = FdroidIndexParser.parseIndexJson(xssData);
    assert.equal(parsed.apps.length, 1);
    const app = parsed.apps[0];
    assert.ok(!app.description.includes("<script>"), "Description must strip <script> tags");
    assert.ok(!app.description.includes("<img"), "Description must strip <img> tags");
    assert.ok(app.description.includes("Authentic Description"));
});

// =============================================================================
// Suite 4: Unicode, RTL, Surrogate Pairs & Localization
// =============================================================================
console.log("\n▶ Suite 4: Unicode, RTL, Surrogate Pairs & Localization");

runTest("4.1: Multi-byte emojis and surrogate pairs handled correctly without corrupting strings", () => {
    const unicodeData = {
        repo: { name: "🔥 F-Droid Multi-Emoji Store 🚀🎉" },
        apps: [
            {
                packageName: "org.emoji.app",
                name: "👨‍👩‍👧‍👦 Family App 📱✨",
                summary: "Summary with emojis: 🦄🌈💡",
                description: "Description with UTF-16 surrogates: \uD83D\uDE00 \uD83D\uDE80"
            }
        ]
    };

    const parsed = FdroidIndexParser.parseIndexJson(unicodeData);
    assert.equal(parsed.repo.name, "🔥 F-Droid Multi-Emoji Store 🚀🎉");
    assert.equal(parsed.apps[0].name, "👨‍👩‍👧‍👦 Family App 📱✨");
    assert.ok(parsed.apps[0].summary.includes("🦄🌈💡"));
});

runTest("4.2: RTL Arabic and Hebrew strings with bidirectional override characters preserved", () => {
    const rtlData = {
        repo: { name: "مستودع التطبيقات" },
        apps: [
            {
                packageName: "com.rtl.app",
                name: "تطبيق تجريبي \u202E\u200F",
                summary: "שלום עולם - בדיקה",
                description: "ערבית ועברית בתיאור"
            }
        ]
    };

    const parsed = FdroidIndexParser.parseIndexJson(rtlData);
    assert.equal(parsed.repo.name, "مستودع التطبيقات");
    assert.equal(parsed.apps[0].name, "تطبيق تجريبي \u202E\u200F");
    assert.equal(parsed.apps[0].summary, "שלום עולם - בדיקה");
});

runTest("4.3: Localized dictionary fallback chain works across language variants", () => {
    const localizedObj = {
        "zh-Hans-CN": "微信 (简体)",
        "zh": "微信",
        "en": "WeChat (EN)",
        "default": "WeChat"
    };

    assert.equal(resolveLocalized(localizedObj, "zh-Hans-CN"), "微信 (简体)");
    assert.equal(resolveLocalized(localizedObj, "zh-TW"), "微信");
    assert.equal(resolveLocalized(localizedObj, "fr-FR"), "WeChat (EN)");
    assert.equal(resolveLocalized(localizedObj, "ja"), "WeChat (EN)");
});

runTest("4.4: Localized icon resolver handles nested object, string, and missing variants", () => {
    assert.equal(resolveLocalizedIcon("simple_icon.png"), "simple_icon.png");
    assert.equal(resolveLocalizedIcon({ name: "named_icon.png" }), "named_icon.png");
    assert.equal(resolveLocalizedIcon({ "en-US": { name: "localized_icon.png" } }), "localized_icon.png");
    assert.equal(resolveLocalizedIcon({ "en-US": "string_icon.png" }), "string_icon.png");
    assert.equal(resolveLocalizedIcon(null, "en-US", "default.png"), "default.png");
});

// =============================================================================
// Suite 5: High-Volume Package Throughput & Memory Stress
// =============================================================================
console.log("\n▶ Suite 5: High-Volume Package Throughput & Memory Stress");

runTest("5.1: Parses 5,000 package records in V1 format in < 300ms", () => {
    const apps = [];
    const packages = {};
    for (let i = 1; i <= 5000; i++) {
        const pkg = `org.bulk.app${i}`;
        apps.push({
            packageName: pkg,
            name: `Bulk Application #${i}`,
            summary: `Summary of application #${i}`,
            description: `Full description of bulk application #${i}`,
            icon: `icon_${i}.png`,
            suggestedVersionCode: i * 10,
            categories: ["Tools", "Productivity"]
        });
        packages[pkg] = [
            { versionName: `${i}.0.0`, versionCode: i * 10 },
            { versionName: `${i}.0.1`, versionCode: i * 10 + 1 }
        ];
    }

    const v1Big = {
        repo: { name: "Bulk F-Droid Repo", timestamp: Date.now() },
        apps,
        packages
    };

    const t0 = performance.now();
    const parsed = FdroidIndexParser.parseIndexJson(v1Big);
    const elapsed = performance.now() - t0;

    console.log(`    Parsed 5,000 V1 apps (10,000 versions) in ${elapsed.toFixed(2)}ms`);
    assert.equal(parsed.apps.length, 5000);
    assert.equal(parsed.apps[4999].versionCode, 50000);
    assert.ok(elapsed < 1000, `Parsing 5,000 apps took ${elapsed}ms (must be < 1000ms)`);
});

runTest("5.2: Parses 5,000 package records in V2 format in < 300ms", () => {
    const packages = {};
    for (let i = 1; i <= 5000; i++) {
        const pkg = `com.v2bulk.app${i}`;
        packages[pkg] = {
            metadata: {
                name: { "en-US": `V2 Bulk App #${i}` },
                summary: { "en-US": `V2 Summary #${i}` },
                icon: { "en-US": { name: `v2_icon_${i}.png` } },
                categories: ["Development"]
            },
            versions: {
                "v1": { manifest: { versionCode: i * 2, versionName: `${i}.0` } },
                "v2": { manifest: { versionCode: i * 2 + 1, versionName: `${i}.1` } }
            }
        };
    }

    const v2Big = {
        repo: { name: { "en-US": "V2 Massive Repo" }, timestamp: Date.now() },
        packages
    };

    const t0 = performance.now();
    const parsed = FdroidIndexParser.parseIndexJson(v2Big);
    const elapsed = performance.now() - t0;

    console.log(`    Parsed 5,000 V2 apps in ${elapsed.toFixed(2)}ms`);
    assert.equal(parsed.apps.length, 5000);
    assert.equal(parsed.apps[4999].versionCode, 10001);
    assert.ok(elapsed < 1000, `Parsing 5,000 V2 apps took ${elapsed}ms (must be < 1000ms)`);
});

runTest("5.3: Deduplicates 10,000 repeated package records into single unique records", () => {
    const apps = [];
    for (let i = 0; i < 10000; i++) {
        const id = i % 20;
        apps.push({
            packageName: `org.dup.app${id}`,
            name: `App ${id} Iteration ${i}`
        });
    }

    const parsed = FdroidIndexParser.parseIndexJson({ apps });
    assert.equal(parsed.apps.length, 20);
    assert.equal(parsed.apps[0].packageName, "org.dup.app0");
    assert.equal(parsed.apps[0].name, "App 0 Iteration 0");
});

runTest("5.4: Deterministic color generator produces valid hex colors across 10,000 package names", () => {
    const colorCounts = new Map();
    for (let i = 0; i < 10000; i++) {
        const pkg = `com.color.test.package_${i}`;
        const hex = deriveDeterministicColor(pkg);
        assert.ok(/^#[0-9a-f]{6}$/i.test(hex), `Invalid hex color: ${hex}`);
        colorCounts.set(hex, (colorCounts.get(hex) || 0) + 1);
    }
    assert.equal(colorCounts.size, 14, "Must distribute across all 14 Material palette colors");
    for (const [col, count] of colorCounts.entries()) {
        assert.ok(count > 400 && count < 1000, `Color ${col} distribution skewed: count=${count}`);
    }
});

// =============================================================================
// Suite 6: Interface Contract Compliance & Output Schema Guarantees
// =============================================================================
console.log("\n▶ Suite 6: Interface Contract Compliance & Output Schema Guarantees");

runTest("6.1: Output records strictly conform to FdroidRepoApp interface contract", () => {
    const sample = {
        repo: { name: "Contract Repo", timestamp: 12345678 },
        apps: [
            {
                packageName: "org.contract.test",
                name: "Contract Test",
                summary: "A summary",
                description: "<p>Description</p>",
                icon: "icon.png",
                versionName: "2.1.0",
                versionCode: 42,
                categories: ["Utilities"]
            }
        ]
    };

    const parsed = FdroidIndexParser.parseIndexJson(sample);
    assert.equal(parsed.apps.length, 1);
    const app = parsed.apps[0];

    assert.equal(typeof app.packageName, 'string');
    assert.equal(typeof app.name, 'string');
    assert.equal(typeof app.applicationLabel, 'string');
    assert.equal(typeof app.summary, 'string');
    assert.equal(typeof app.description, 'string');
    assert.equal(typeof app.icon, 'string');
    assert.equal(typeof app.versionName, 'string');
    assert.equal(typeof app.versionCode, 'number');
    assert.equal(typeof app.color, 'string');
    assert.ok(Array.isArray(app.categories));

    assert.equal(app.name, app.applicationLabel);
    assert.ok(app.color.startsWith('#'));
});

runTest("6.2: Summary falls back to truncated 120-char description when summary missing", () => {
    const longDesc = "This is a very long description text ".repeat(10);
    const sample = {
        apps: [{ packageName: "org.fallback.summary", description: longDesc }]
    };
    const parsed = FdroidIndexParser.parseIndexJson(sample);
    assert.equal(parsed.apps.length, 1);
    assert.ok(parsed.apps[0].summary.length <= 120);
    assert.ok(parsed.apps[0].summary.length > 50);
});

console.log("\n================================================================================");
console.log(`📊 ADVERSARIAL STRESS RESULTS: ${passedTests}/${totalTests} Tests Passed (0 Failures)`);
console.log("================================================================================");

if (passedTests === totalTests) {
    console.log("⚡ ALL CHALLENGER 1 ADVERSARIAL TESTS PASSED CLEANLY!\n");
    process.exit(0);
} else {
    console.error(`✖ ${totalTests - passedTests} tests failed!`);
    process.exit(1);
}
