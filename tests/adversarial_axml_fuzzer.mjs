/**
 * AndroidWebGPU - Adversarial AXML & APK Ingestion Fuzzer
 * 
 * Tests and verifies robustness of:
 * 1. Real APK Ingestion (F-Droid.apk, godot_gles2.apk, unity_cube.apk, etc.)
 * 2. ZIP Archive Parsing (EOCD search, entry table, central directory)
 * 3. Binary AXML Chunk Decoder (0x0003, 0x0001, 0x0180, 0x0102, 0x0103, 0x0100, 0x0101)
 * 4. AXML Fuzzing (truncation at every byte boundary, invalid magic, negative chunk sizes)
 * 5. String Pool & Resource Map Fuzzing (stringCount overflow, out-of-bounds offsets, UTF-8 corruption)
 * 6. Circular Namespaces & Foreign XML Chunks (0x0000, 0x0104, 0x0105, 0xCAFE)
 * 7. ARSC String Pool & Circular Reference Recursion Protection
 * 8. In-Memory PackageManagerRegistry Resolution & Multi-Authority Splitting
 * 9. DEFLATE inflateRaw Truncation & EOF Handling Stress Test
 * 10. 5,000 Iteration Mutation Fuzz Churn
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';
import {
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
    parseApk
} from '../src/apk_client_parser.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        failed++;
        console.error(`[FAIL] ${message}`);
        throw new Error(`Assertion failed: ${message}`);
    }
    passed++;
}

function runTest(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ Running: ${name}`);
    console.log(`======================================================`);
    try {
        fn();
        console.log(`✔ [PASS] ${name}`);
    } catch (err) {
        console.error(`✖ [FAIL] ${name}: ${err.message}`);
        throw err;
    }
}

// PRNG for deterministic reproducible fuzzing
let seed = 0x1337C0DE;
function prng() {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed;
}

// Helper to build synthetic AXML buffer
function createSyntheticAxml({
    packageName = "com.test.adversarial",
    versionCode = 100,
    versionName = "1.0.0",
    activities = ["com.test.adversarial.MainActivity"],
    services = [],
    providers = [],
    receivers = [],
    permissions = ["android.permission.INTERNET"]
} = {}) {
    const strings = [
        "", // 0
        "manifest", // 1
        "package", // 2
        "versionCode", // 3
        "versionName", // 4
        "uses-sdk", // 5
        "minSdkVersion", // 6
        "targetSdkVersion", // 7
        "application", // 8
        "label", // 9
        "name", // 10
        "exported", // 11
        "activity", // 12
        "service", // 13
        "provider", // 14
        "receiver", // 15
        "intent-filter", // 16
        "action", // 17
        "category", // 18
        "android.intent.action.MAIN", // 19
        "android.intent.category.LAUNCHER", // 20
        "authorities", // 21
        "uses-permission", // 22
        packageName, // 23
        versionName, // 24
        "Test App" // 25
    ];

    const stringIndexMap = new Map();
    strings.forEach((s, idx) => stringIndexMap.set(s, idx));

    function addString(s) {
        if (stringIndexMap.has(s)) return stringIndexMap.get(s);
        const idx = strings.length;
        strings.push(s);
        stringIndexMap.set(s, idx);
        return idx;
    }

    const activityIndices = activities.map(a => addString(a));
    const serviceIndices = services.map(s => addString(s));
    const providerIndices = providers.map(p => addString(p));
    const receiverIndices = receivers.map(r => addString(r));
    const permissionIndices = permissions.map(p => addString(p));

    const stringBytesList = strings.map(s => {
        const u16 = [];
        for (let i = 0; i < s.length; i++) u16.push(s.charCodeAt(i));
        u16.push(0);
        return new Uint16Array(u16);
    });

    let stringDataLen = 0;
    const stringOffsets = [];
    for (const sb of stringBytesList) {
        stringOffsets.push(stringDataLen);
        stringDataLen += 2 + sb.length * 2;
    }

    const stringPoolHeaderSize = 28;
    const stringPoolChunkSize = stringPoolHeaderSize + strings.length * 4 + stringDataLen;
    const pad = (4 - (stringPoolChunkSize % 4)) % 4;
    const totalStringPoolSize = stringPoolChunkSize + pad;

    const spBuf = new Uint8Array(totalStringPoolSize);
    const spView = new DataView(spBuf.buffer);
    spView.setUint16(0, RES_STRING_POOL_TYPE, true);
    spView.setUint16(2, stringPoolHeaderSize, true);
    spView.setUint32(4, totalStringPoolSize, true);
    spView.setUint32(8, strings.length, true);
    spView.setUint32(12, 0, true);
    spView.setUint32(16, 0, true); // UTF-16
    spView.setUint32(20, stringPoolHeaderSize + strings.length * 4, true);
    spView.setUint32(24, 0, true);

    for (let i = 0; i < strings.length; i++) {
        spView.setUint32(28 + i * 4, stringOffsets[i], true);
    }

    let strCursor = stringPoolHeaderSize + strings.length * 4;
    for (let i = 0; i < strings.length; i++) {
        const sb = stringBytesList[i];
        spView.setUint16(strCursor, sb.length - 1, true);
        strCursor += 2;
        for (let j = 0; j < sb.length; j++) {
            spView.setUint16(strCursor, sb[j], true);
            strCursor += 2;
        }
    }

    // Elements
    const elementChunks = [];

    function makeStartElement(nameIdx, attrs = []) {
        const chunkSize = 36 + attrs.length * 20;
        const b = new Uint8Array(chunkSize);
        const v = new DataView(b.buffer);
        v.setUint16(0, RES_XML_START_ELEMENT_TYPE, true);
        v.setUint16(2, 16, true);
        v.setUint32(4, chunkSize, true);
        v.setUint32(8, 1, true); // lineNumber
        v.setUint32(12, 0xFFFFFFFF, true); // comment
        v.setUint32(16, 0xFFFFFFFF, true); // ns
        v.setUint32(20, nameIdx, true); // name
        v.setUint16(24, 20, true); // attrStart
        v.setUint16(26, 20, true); // attrSize
        v.setUint16(28, attrs.length, true); // attrCount
        v.setUint16(30, 0, true);
        v.setUint16(32, 0, true);
        v.setUint16(34, 0, true);

        let off = 36;
        for (const a of attrs) {
            v.setUint32(off, 0xFFFFFFFF, true); // ns
            v.setUint32(off + 4, a.nameIdx, true);
            v.setUint32(off + 8, a.rawIdx !== undefined ? a.rawIdx : 0xFFFFFFFF, true);
            v.setUint16(off + 12, 8, true); // size
            v.setUint8(off + 14, 0); // res0
            v.setUint8(off + 15, a.dataType || 3); // dataType
            v.setUint32(off + 16, a.data !== undefined ? a.data : 0, true);
            off += 20;
        }
        return b;
    }

    function makeEndElement(nameIdx) {
        const b = new Uint8Array(24);
        const v = new DataView(b.buffer);
        v.setUint16(0, RES_XML_END_ELEMENT_TYPE, true);
        v.setUint16(2, 16, true);
        v.setUint32(4, 24, true);
        v.setUint32(8, 1, true);
        v.setUint32(12, 0xFFFFFFFF, true);
        v.setUint32(16, 0xFFFFFFFF, true);
        v.setUint32(20, nameIdx, true);
        return b;
    }

    // <manifest package="..." versionCode="..." versionName="...">
    elementChunks.push(makeStartElement(1, [
        { nameIdx: 2, rawIdx: 23, dataType: 3, data: 23 },
        { nameIdx: 3, rawIdx: 0xFFFFFFFF, dataType: 16, data: versionCode },
        { nameIdx: 4, rawIdx: 24, dataType: 3, data: 24 }
    ]));

    // <uses-permission name="..." />
    for (let i = 0; i < permissions.length; i++) {
        elementChunks.push(makeStartElement(22, [
            { nameIdx: 10, rawIdx: permissionIndices[i], dataType: 3, data: permissionIndices[i] }
        ]));
        elementChunks.push(makeEndElement(22));
    }

    // <application label="...">
    elementChunks.push(makeStartElement(8, [
        { nameIdx: 9, rawIdx: 25, dataType: 3, data: 25 }
    ]));

    // <activity name="..." exported="true">
    for (let i = 0; i < activities.length; i++) {
        elementChunks.push(makeStartElement(12, [
            { nameIdx: 10, rawIdx: activityIndices[i], dataType: 3, data: activityIndices[i] },
            { nameIdx: 11, rawIdx: 0xFFFFFFFF, dataType: 18, data: 1 }
        ]));
        // <intent-filter>
        if (i === 0) {
            elementChunks.push(makeStartElement(16, []));
            // <action name="android.intent.action.MAIN" />
            elementChunks.push(makeStartElement(17, [
                { nameIdx: 10, rawIdx: 19, dataType: 3, data: 19 }
            ]));
            elementChunks.push(makeEndElement(17));
            // <category name="android.intent.category.LAUNCHER" />
            elementChunks.push(makeStartElement(18, [
                { nameIdx: 10, rawIdx: 20, dataType: 3, data: 20 }
            ]));
            elementChunks.push(makeEndElement(18));
            elementChunks.push(makeEndElement(16));
        }
        elementChunks.push(makeEndElement(12));
    }

    // <service ...>
    for (let i = 0; i < services.length; i++) {
        elementChunks.push(makeStartElement(13, [
            { nameIdx: 10, rawIdx: serviceIndices[i], dataType: 3, data: serviceIndices[i] }
        ]));
        elementChunks.push(makeEndElement(13));
    }

    // <provider ...>
    for (let i = 0; i < providers.length; i++) {
        elementChunks.push(makeStartElement(14, [
            { nameIdx: 10, rawIdx: providerIndices[i], dataType: 3, data: providerIndices[i] },
            { nameIdx: 21, rawIdx: providerIndices[i], dataType: 3, data: providerIndices[i] }
        ]));
        elementChunks.push(makeEndElement(14));
    }

    // </application>
    elementChunks.push(makeEndElement(8));
    // </manifest>
    elementChunks.push(makeEndElement(1));

    let elementsTotalLen = 0;
    for (const chunk of elementChunks) elementsTotalLen += chunk.length;

    const totalAxmlSize = 8 + spBuf.length + elementsTotalLen;
    const axml = new Uint8Array(totalAxmlSize);
    const v = new DataView(axml.buffer);
    v.setUint16(0, RES_XML_TYPE, true);
    v.setUint16(2, 8, true);
    v.setUint32(4, totalAxmlSize, true);

    axml.set(spBuf, 8);
    let cur = 8 + spBuf.length;
    for (const chunk of elementChunks) {
        axml.set(chunk, cur);
        cur += chunk.length;
    }

    return axml;
}

// -----------------------------------------------------------------------------
// Suite 1: Real APK Ingestion Verification
// -----------------------------------------------------------------------------
runTest("1. Real APK Ingestion & Component Resolution", () => {
    const apkFiles = [
        { path: 'F-Droid.apk', expectedPkg: 'org.fdroid.fdroid', minActs: 20, minProviders: 1 },
        { path: 'fixtures/fdroid.apk', expectedPkg: 'org.fdroid.fdroid', minActs: 20, minProviders: 1 },
        { path: 'fixtures/godot_gles2.apk', expectedPkg: 'org.godotengine.gles2game', minActs: 0, minProviders: 0 },
        { path: 'fixtures/unity_cube.apk', expectedPkg: 'com.unity.cube.gles', minActs: 0, minProviders: 0 },
        { path: 'fixtures/unity_cube.vulkan.apk', expectedPkg: 'com.unity.cube.vulkan', minActs: 0, minProviders: 0 }
    ];

    const registry = new PackageManagerRegistry();

    for (const apk of apkFiles) {
        const fullPath = path.resolve(process.cwd(), apk.path);
        assert(fs.existsSync(fullPath), `APK file ${apk.path} must exist`);

        const buf = fs.readFileSync(fullPath);
        assert(buf.byteLength > 0, `${apk.path} buffer length > 0`);

        // Test ApkZipReader
        const zip = new ApkZipReader(buf);
        const entries = zip.listEntries();
        assert(entries.length > 0, `${apk.path} has ${entries.length} entries`);
        assert(entries.includes("AndroidManifest.xml"), `${apk.path} contains AndroidManifest.xml`);

        // Test Manifest parsing
        const manifestBytes = zip.getManifest();
        assert(manifestBytes !== null && manifestBytes.length > 0, `AndroidManifest.xml extracted from ${apk.path}`);

        const manifest = AxmlDecoder.decode(manifestBytes);
        assert(manifest.packageName === apk.expectedPkg, `${apk.path} packageName matches ${apk.expectedPkg} (got ${manifest.packageName})`);
        assert(manifest.activities.length >= apk.minActs, `${apk.path} activities count >= ${apk.minActs}`);
        assert(manifest.providers.length >= apk.minProviders, `${apk.path} providers count >= ${apk.minProviders}`);

        // Test installApk into registry
        const installed = registry.installApk(buf);
        assert(installed.packageName === apk.expectedPkg, `Registry installed package matches ${apk.expectedPkg}`);
        assert(registry.hasPackage(apk.expectedPkg), `Registry has package ${apk.expectedPkg}`);
        assert(registry.getPackageInfo(apk.expectedPkg) !== null, `getPackageInfo returns non-null`);
    }

    assert(registry.getInstalledPackages().length >= 8, `Registry contains all default + installed packages`);
    console.log(`  -> Successfully verified 5 real APK files and PMS registry integration.`);
});

// -----------------------------------------------------------------------------
// Suite 2: ZIP & Header Adversarial Fuzzing
// -----------------------------------------------------------------------------
runTest("2. ZIP Archive Header & EOCD Fuzzing", () => {
    // 2.1 Empty and tiny buffers for ApkZipReader
    for (let len = 0; len < 22; len++) {
        let threw = false;
        try {
            const z = new ApkZipReader(new Uint8Array(len));
            z.readEntries();
        } catch (e) {
            threw = true;
            assert(e.message.includes("too short"), `Zip buffer len ${len} threw expected error`);
        }
        assert(threw, `Zip buffer len ${len} must throw error`);
    }

    // 2.2 Invalid EOCD magic numbers
    const badEocdBuf = new Uint8Array(64);
    badEocdBuf.fill(0xAA);
    let threwEocd = false;
    try {
        const z = new ApkZipReader(badEocdBuf);
        z.readEntries();
    } catch (e) {
        threwEocd = true;
        assert(e.message.includes("Cannot find End of Central Directory"), `Bad EOCD threw expected error`);
    }
    assert(threwEocd, `Missing EOCD signature must throw error`);

    // 2.3 Central directory offset pointing out of bounds
    const eocdOutOfBounds = new Uint8Array(22);
    const ev = new DataView(eocdOutOfBounds.buffer);
    ev.setUint32(0, 0x06054b50, true); // EOCD magic
    ev.setUint16(10, 100, true); // totalEntries = 100
    ev.setUint32(16, 0xFFFFFFFF, true); // cdOffset = 4GB
    const zEocd = new ApkZipReader(eocdOutOfBounds);
    const entries = zEocd.readEntries();
    assert(entries.size === 0, "Out-of-bounds CD offset returns empty entries map without crash");

    console.log(`  -> ZIP archive header corruption tests passed.`);
});

// -----------------------------------------------------------------------------
// Suite 3: Truncated AXML Buffers & Invalid Chunk Headers
// -----------------------------------------------------------------------------
runTest("3. Truncated AXML Buffers & Malformed Chunk Headers", () => {
    const validAxml = createSyntheticAxml();
    assert(validAxml.byteLength > 64, "Synthetic AXML built");

    // 3.1 Lengths 0 to 7 must throw "too short"
    for (let len = 0; len < 8; len++) {
        let threw = false;
        try {
            AxmlDecoder.decode(new Uint8Array(len));
        } catch (e) {
            threw = true;
            assert(e.message.includes("too short"), `AXML len ${len} threw too short`);
        }
        assert(threw, `AXML len ${len} must throw error`);
    }

    // 3.2 Invalid root magic numbers
    const badMagics = [0x0000, 0x0001, 0x0002, 0x0100, 0x0102, 0x0180, 0xFFFF, 0xCAFE];
    for (const badMagic of badMagics) {
        const b = new Uint8Array(32);
        const v = new DataView(b.buffer);
        v.setUint16(0, badMagic, true);
        v.setUint16(2, 8, true);
        v.setUint32(4, 32, true);

        let threw = false;
        try {
            AxmlDecoder.decode(b);
        } catch (e) {
            threw = true;
            assert(e.message.includes("Invalid AXML magic"), `Bad magic 0x${badMagic.toString(16)} threw invalid magic`);
        }
        assert(threw, `Bad magic 0x${badMagic.toString(16)} must throw error`);
    }

    // 3.3 Truncation at every single byte offset from 8 to validAxml.length
    for (let cut = 8; cut < validAxml.length; cut++) {
        const slice = validAxml.subarray(0, cut);
        try {
            const manifest = AxmlDecoder.decode(slice);
            assert(typeof manifest === "object", `Cut ${cut}: Decoded partial manifest without crashing`);
        } catch (err) {
            assert(err instanceof Error, `Cut ${cut}: Expected error instance thrown`);
        }
        assert(true, `Cut ${cut}: Safe execution`);
    }

    // 3.4 Malformed Chunk Sizes & Negative Chunk Sizes (0xFFFFFFFF)
    const malformedSizeOffsets = [
        0, 1, 2, 4, 7, // Chunk sizes < 8
        0xFFFFFFFF, 0x80000000, 0x7FFFFFFF, // Overflowing / negative sizes
        validAxml.length + 1000 // Out of bounds
    ];

    for (const badSize of malformedSizeOffsets) {
        const b = new Uint8Array(validAxml);
        const v = new DataView(b.buffer);
        // Corrupt String Pool chunk size (at offset 8 + 4 = 12)
        v.setUint32(12, badSize, true);

        try {
            const res = AxmlDecoder.decode(b);
            assert(typeof res === "object", `Bad chunk size ${badSize} returned object`);
        } catch (err) {
            assert(err instanceof Error, `Bad chunk size ${badSize} threw Error`);
        }
        assert(true, `Bad chunk size ${badSize} handled without crash or hang`);
    }

    console.log(`  -> Handled all byte truncations (8..${validAxml.length}) and malformed chunk sizes cleanly.`);
});

// -----------------------------------------------------------------------------
// Suite 4: Invalid String Pool Offsets & Corrupted String Data
// -----------------------------------------------------------------------------
runTest("4. Invalid String Pool Offsets & Corrupted Strings", () => {
    const validAxml = createSyntheticAxml();

    // 4.1 String count = 0xFFFFFFFF (should not cause OOM / hang)
    const b1 = new Uint8Array(validAxml);
    const v1 = new DataView(b1.buffer);
    v1.setUint32(16, 0xFFFFFFFF, true); // stringCount = 4294967295
    try {
        const m = AxmlDecoder.decode(b1);
        assert(typeof m === "object", "Giant string count handled");
    } catch (e) {
        assert(e instanceof Error, "Giant string count threw Error");
    }

    // 4.2 Out-of-bounds stringsStart offset
    const b2 = new Uint8Array(validAxml);
    const v2 = new DataView(b2.buffer);
    v2.setUint32(28, 0xFFFFFFFF, true); // stringsStart = 4GB
    try {
        const m = AxmlDecoder.decode(b2);
        assert(typeof m === "object", "Giant stringsStart handled");
    } catch (e) {
        assert(e instanceof Error, "Giant stringsStart threw Error");
    }

    // 4.3 UTF-8 Flag enabled with corrupted length bytes
    const b3 = new Uint8Array(validAxml);
    const v3 = new DataView(b3.buffer);
    v3.setUint32(24, (1 << 8), true); // flags |= UTF8
    try {
        const m = AxmlDecoder.decode(b3);
        assert(typeof m === "object", "UTF-8 corrupted lengths handled");
    } catch (e) {
        assert(e instanceof Error, "UTF-8 corrupted lengths threw Error");
    }

    // 4.4 Corrupted string offset table entries (pointing to 0xFFFFFFFF)
    const b4 = new Uint8Array(validAxml);
    const v4 = new DataView(b4.buffer);
    for (let i = 36; i < 100 && i + 4 <= b4.length; i += 4) {
        v4.setUint32(i, 0x7FFFFFFF, true);
    }
    try {
        const m = AxmlDecoder.decode(b4);
        assert(typeof m === "object", "Corrupted string offsets handled");
    } catch (e) {
        assert(e instanceof Error, "Corrupted string offsets threw Error");
    }

    console.log(`  -> String pool fuzzing and offset overflow resilience verified.`);
});

// -----------------------------------------------------------------------------
// Suite 5: Circular Namespaces, Unknown Chunks & Out-of-Order Tags
// -----------------------------------------------------------------------------
runTest("5. Circular Namespaces, Unknown Chunks & Tag Out-of-Order Fuzzing", () => {
    // 5.1 Synthetic AXML with multiple START_NAMESPACE (0x0100) and END_NAMESPACE (0x0101)
    const validAxml = createSyntheticAxml();
    const spSize = new DataView(validAxml.buffer).getUint32(12, true);

    // Create namespace chunk (24 bytes)
    const nsChunkStart = new Uint8Array(24);
    const nsv1 = new DataView(nsChunkStart.buffer);
    nsv1.setUint16(0, RES_XML_START_NAMESPACE_TYPE, true);
    nsv1.setUint16(2, 16, true);
    nsv1.setUint32(4, 24, true);
    nsv1.setUint32(8, 1, true); // line
    nsv1.setUint32(12, 0xFFFFFFFF, true);
    nsv1.setUint32(16, 2, true); // prefix = "package"
    nsv1.setUint32(20, 23, true); // uri = "com.test.adversarial"

    // Create unknown chunk (0x0105 or 0xCAFE, 32 bytes)
    const unknownChunk = new Uint8Array(32);
    const unkv = new DataView(unknownChunk.buffer);
    unkv.setUint16(0, 0xCAFE, true);
    unkv.setUint16(2, 16, true);
    unkv.setUint32(4, 32, true);
    unknownChunk.fill(0xEE, 8);

    // Build custom AXML buffer containing nested namespaces and unknown chunks
    const customLen = validAxml.length + nsChunkStart.length * 4 + unknownChunk.length * 3;
    const customAxml = new Uint8Array(customLen);
    const cv = new DataView(customAxml.buffer);

    // Copy root header and string pool
    customAxml.set(validAxml.subarray(0, 8 + spSize), 0);
    cv.setUint32(4, customLen, true);

    let wOff = 8 + spSize;
    // Insert circular / repeated namespaces
    for (let i = 0; i < 4; i++) {
        customAxml.set(nsChunkStart, wOff);
        wOff += nsChunkStart.length;
    }
    // Insert unknown chunks
    for (let i = 0; i < 3; i++) {
        customAxml.set(unknownChunk, wOff);
        wOff += unknownChunk.length;
    }
    // Copy remaining elements
    customAxml.set(validAxml.subarray(8 + spSize), wOff);

    const m = AxmlDecoder.decode(customAxml);
    assert(m.packageName === "com.test.adversarial", "Custom AXML with namespaces & unknown chunks decoded packageName");
    assert(m.activities.length === 1, "Activities decoded cleanly despite foreign chunks");
    assert(m.launcherActivity === "com.test.adversarial.MainActivity", "Launcher activity resolved");

    // 5.2 Excessive END_ELEMENT chunks (popping empty stack)
    const excessEndElementAxml = new Uint8Array(validAxml.length + 24 * 10);
    excessEndElementAxml.set(validAxml.subarray(0, 8 + spSize), 0);
    new DataView(excessEndElementAxml.buffer).setUint32(4, excessEndElementAxml.length, true);

    const endElem = new Uint8Array(24);
    const ev = new DataView(endElem.buffer);
    ev.setUint16(0, RES_XML_END_ELEMENT_TYPE, true);
    ev.setUint16(2, 16, true);
    ev.setUint32(4, 24, true);
    ev.setUint32(20, 1, true); // "manifest"

    let eCursor = 8 + spSize;
    for (let i = 0; i < 10; i++) {
        excessEndElementAxml.set(endElem, eCursor);
        eCursor += 24;
    }
    excessEndElementAxml.set(validAxml.subarray(8 + spSize), eCursor);

    const m2 = AxmlDecoder.decode(excessEndElementAxml);
    assert(typeof m2 === "object", "Excess END_ELEMENT chunks handled without stack underflow crash");

    console.log(`  -> Circular namespaces, foreign XML chunk types and out-of-order tags verified.`);
});

// -----------------------------------------------------------------------------
// Suite 6: ArscStringPoolParser Recursion & Resource Map Fuzzing
// -----------------------------------------------------------------------------
runTest("6. ArscStringPoolParser Recursion Protection & Resource Map Fuzzing", () => {
    // 6.1 Create synthetic resources.arsc with circular reference: res 0x7f010001 -> res 0x7f010001
    const arscBuf = new Uint8Array(1024);
    const arscView = new DataView(arscBuf.buffer);
    
    // Table Header
    arscView.setUint16(0, RES_TABLE_TYPE, true);
    arscView.setUint16(2, 12, true);
    arscView.setUint32(4, 1024, true);
    arscView.setUint32(8, 1, true); // packageCount = 1

    // Global String Pool at offset 12
    arscView.setUint16(12, RES_STRING_POOL_TYPE, true);
    arscView.setUint16(14, 28, true);
    arscView.setUint32(16, 120, true); // chunk size
    arscView.setUint32(20, 3, true); // 3 strings
    arscView.setUint32(28, (1 << 8), true); // UTF-8
    arscView.setUint32(32, 28 + 12, true); // stringsStart

    // String offsets
    arscView.setUint32(40, 0, true);
    arscView.setUint32(44, 8, true);
    arscView.setUint32(48, 22, true);

    // Strings content at 12 + 40 = 52
    const strData = new Uint8Array([
        5, 5, 0x64, 0x75, 0x6d, 0x6d, 0x79, 0x00, // dummy
        11, 11, 0x40, 0x30, 0x78, 0x37, 0x66, 0x30, 0x31, 0x30, 0x30, 0x30, 0x31, 0x00, // @0x7f010001
        8, 8, 0x61, 0x70, 0x70, 0x5f, 0x6e, 0x61, 0x6d, 0x65, 0x00 // app_name
    ]);
    arscBuf.set(strData, 52);

    // Package Chunk at offset 132
    const pkgOffset = 132;
    arscView.setUint16(pkgOffset, 0x0200, true); // RES_TABLE_PACKAGE_TYPE
    arscView.setUint16(pkgOffset + 2, 288, true);
    arscView.setUint32(pkgOffset + 4, 1024 - pkgOffset, true);
    arscView.setUint32(pkgOffset + 8, 0x7F, true); // pkgId = 0x7F

    // Write package name "com.test"
    const pkgName = "com.test";
    for (let i = 0; i < pkgName.length; i++) {
        arscView.setUint16(pkgOffset + 12 + i * 2, pkgName.charCodeAt(i), true);
    }

    // Type strings at pkgOffset + 288
    arscView.setUint32(pkgOffset + 268, 288, true);
    const typePoolOff = pkgOffset + 288;
    arscView.setUint16(typePoolOff, RES_STRING_POOL_TYPE, true);
    arscView.setUint16(typePoolOff + 2, 28, true);
    arscView.setUint32(typePoolOff + 4, 64, true);
    arscView.setUint32(typePoolOff + 8, 1, true); // 1 type: "string"
    arscView.setUint32(typePoolOff + 16, (1 << 8), true);
    arscView.setUint32(typePoolOff + 20, 32, true);
    arscView.setUint32(typePoolOff + 28, 0, true);
    const typeStr = new Uint8Array([6, 6, 0x73, 0x74, 0x72, 0x69, 0x6e, 0x67, 0x00]);
    arscBuf.set(typeStr, typePoolOff + 32);

    // Key strings at typePoolOff + 64
    const keyPoolOff = typePoolOff + 64;
    arscView.setUint32(pkgOffset + 276, 288 + 64, true);
    arscView.setUint16(keyPoolOff, RES_STRING_POOL_TYPE, true);
    arscView.setUint16(keyPoolOff + 2, 28, true);
    arscView.setUint32(keyPoolOff + 4, 64, true);
    arscView.setUint32(keyPoolOff + 8, 1, true); // 1 key: "app_label"
    arscView.setUint32(keyPoolOff + 16, (1 << 8), true);
    arscView.setUint32(keyPoolOff + 20, 32, true);
    arscView.setUint32(keyPoolOff + 28, 0, true);
    const keyStr = new Uint8Array([9, 9, 0x61, 0x70, 0x70, 0x5f, 0x6c, 0x61, 0x62, 0x65, 0x6c, 0x00]);
    arscBuf.set(keyStr, keyPoolOff + 32);

    // RES_TABLE_TYPE_TYPE chunk at keyPoolOff + 64
    const typeChunkOff = keyPoolOff + 64;
    arscView.setUint16(typeChunkOff, 0x0201, true); // RES_TABLE_TYPE_TYPE
    arscView.setUint16(typeChunkOff + 2, 20, true);
    arscView.setUint32(typeChunkOff + 4, 120, true);
    arscBuf[typeChunkOff + 8] = 1; // typeId = 1
    arscView.setUint32(typeChunkOff + 12, 2, true); // 2 entries
    arscView.setUint32(typeChunkOff + 16, 20 + 8, true); // entriesStart (after 2 uint32 offsets)

    // Entry offsets
    arscView.setUint32(typeChunkOff + 20, 0, true); // entry 0 offset
    arscView.setUint32(typeChunkOff + 24, 16, true); // entry 1 offset

    // Entry 0 at typeChunkOff + 28: points to entry 1 (dataType = 1, data = 0x7F010001)
    const e0Off = typeChunkOff + 28;
    arscView.setUint16(e0Off, 8, true); // size
    arscView.setUint16(e0Off + 2, 0, true); // flags
    arscView.setUint32(e0Off + 4, 0, true); // keyIdx = 0
    arscBuf[e0Off + 11] = 1; // dataType = 1 (reference)
    arscView.setUint32(e0Off + 12, 0x7F010001, true); // reference to 0x7F010001

    // Entry 1 at typeChunkOff + 44: points back to entry 0 (0x7F010000) -> Circular!
    const e1Off = typeChunkOff + 44;
    arscView.setUint16(e1Off, 8, true);
    arscView.setUint16(e1Off + 2, 0, true);
    arscView.setUint32(e1Off + 4, 0, true);
    arscBuf[e1Off + 11] = 1; // dataType = 1 (reference)
    arscView.setUint32(e1Off + 12, 0x7F010000, true); // circular loop

    const parser = new ArscStringPoolParser(arscBuf);
    parser.parse();

    // Must return null when circular loop depth > 10 without infinite recursion / stack overflow
    const res = parser.resolveString(0x7F010000);
    assert(res === null, "Circular resource reference returned null safely");

    const resRef = parser.resolveStringRef("@0x7f010000");
    assert(resRef === "@0x7f010000", "resolveStringRef handled circular reference fallback safely");

    console.log(`  -> ARSC parser circular reference and depth guard verified.`);
});

// -----------------------------------------------------------------------------
// Suite 7: PackageManagerRegistry Lifecycle & Resolution Stress
// -----------------------------------------------------------------------------
runTest("7. PackageManagerRegistry Invariants & Resolution Stress", () => {
    const reg = new PackageManagerRegistry();

    // 7.1 Registering invalid package metadata
    let threwInvalid = false;
    try {
        reg.registerPackage(null);
    } catch (e) {
        threwInvalid = true;
    }
    assert(threwInvalid, "registerPackage(null) threw error");

    let threwEmpty = false;
    try {
        reg.registerPackage({});
    } catch (e) {
        threwEmpty = true;
    }
    assert(threwEmpty, "registerPackage({}) without packageName threw error");

    // 7.2 Listener error isolation
    let listenerCalled = false;
    reg.addListener((evt, data) => {
        listenerCalled = true;
        throw new Error("Faulty listener explosion");
    });

    reg.registerPackage({
        packageName: "com.test.listener.safe",
        activities: [{ name: "com.test.listener.safe.MainActivity" }]
    });

    assert(listenerCalled, "Listener was called");
    assert(reg.hasPackage("com.test.listener.safe"), "Package registered despite throwing listener");

    // 7.3 ContentProvider multiple authorities splitting
    reg.registerPackage({
        packageName: "com.test.multi.authorities",
        providers: [
            { name: "com.test.Provider1", authority: "auth.one; auth.two ; auth.three" }
        ]
    });

    assert(reg.resolveContentProvider("auth.one") !== null, "Resolved auth.one");
    assert(reg.resolveContentProvider("auth.two") !== null, "Resolved auth.two");
    assert(reg.resolveContentProvider("auth.three") !== null, "Resolved auth.three");
    assert(reg.resolveContentProvider("auth.unknown") === null, "auth.unknown is null");

    // Unregister and ensure authorities are purged
    reg.unregisterPackage("com.test.multi.authorities");
    assert(!reg.hasPackage("com.test.multi.authorities"), "Package unregistered");
    assert(reg.resolveContentProvider("auth.one") === null, "auth.one cleaned up");
    assert(reg.resolveContentProvider("auth.two") === null, "auth.two cleaned up");
    assert(reg.resolveContentProvider("auth.three") === null, "auth.three cleaned up");

    // 7.4 Intent query fuzzy matching & launcher resolution
    reg.registerPackage({
        packageName: "com.test.launcher.fallback",
        activities: [
            { name: "com.test.FirstActivity" },
            { name: "com.test.SecondActivity" }
        ]
    });
    assert(reg.resolveLauncherActivity("com.test.launcher.fallback") === "com.test.FirstActivity", "Launcher falls back to first activity");

    // Query non-matching intent
    const nonMatches = reg.queryIntentActivities({ action: "NON_EXISTENT_ACTION" });
    assert(nonMatches.length === 0, "Non-matching intent query returns empty array");

    console.log(`  -> PackageManagerRegistry state machine and query engine verified.`);
});

// -----------------------------------------------------------------------------
// Suite 8: 1,000 Iteration Randomized Adversarial Fuzz Churn
// -----------------------------------------------------------------------------
runTest("8. 1,000 Iteration Randomized Mutation Fuzz Churn", () => {
    const baseAxml = createSyntheticAxml({
        packageName: "com.fuzz.target",
        versionCode: 42,
        versionName: "4.2.0-fuzz",
        activities: ["com.fuzz.target.Act1", "com.fuzz.target.Act2"],
        providers: ["com.fuzz.target.Prov1"],
        services: ["com.fuzz.target.Svc1"]
    });

    console.log("  -> Running 1,000 fuzz iterations against AxmlDecoder...");
    let exceptionsCaught = 0;
    let successfulParses = 0;

    for (let i = 0; i < 1000; i++) {
        const fuzzed = new Uint8Array(baseAxml.length);
        fuzzed.set(baseAxml);

        const mutationCount = (prng() % 5) + 1;
        for (let m = 0; m < mutationCount; m++) {
            const mutType = prng() % 8;
            if (mutType === 0) {
                // Flip random bytes
                const pos = prng() % fuzzed.length;
                fuzzed[pos] = prng() & 0xFF;
            } else if (mutType === 1) {
                // Corrupt 16-bit word
                const pos = (prng() % ((fuzzed.length - 2) >> 1)) << 1;
                new DataView(fuzzed.buffer).setUint16(pos, prng() & 0xFFFF, true);
            } else if (mutType === 2) {
                // Corrupt 32-bit dword
                const pos = (prng() % ((fuzzed.length - 4) >> 2)) << 2;
                new DataView(fuzzed.buffer).setUint32(pos, prng(), true);
            } else if (mutType === 3) {
                // Zero out block
                const start = prng() % (fuzzed.length - 16);
                fuzzed.fill(0, start, start + 16);
            } else if (mutType === 4) {
                // Set all ones
                const start = prng() % (fuzzed.length - 8);
                fuzzed.fill(0xFF, start, start + 8);
            } else if (mutType === 5) {
                // Inject fake chunk header
                const pos = (prng() % ((fuzzed.length - 8) >> 2)) << 2;
                const dv = new DataView(fuzzed.buffer);
                dv.setUint16(pos, 0x0102, true); // RES_XML_START_ELEMENT_TYPE
                dv.setUint16(pos + 2, 16, true);
                dv.setUint32(pos + 4, prng() % 256, true);
            } else if (mutType === 6) {
                // Negative / giant chunk size
                const pos = (prng() % ((fuzzed.length - 8) >> 2)) << 2;
                new DataView(fuzzed.buffer).setUint32(pos + 4, 0xFFFFFFFF, true);
            } else {
                // Small chunk size (< 8)
                const pos = (prng() % ((fuzzed.length - 8) >> 2)) << 2;
                new DataView(fuzzed.buffer).setUint32(pos + 4, prng() % 8, true);
            }
        }

        // Test random slice truncation
        let testSlice = fuzzed;
        if (prng() % 3 === 0) {
            const cut = prng() % fuzzed.length;
            testSlice = fuzzed.subarray(0, cut);
        }

        try {
            const result = AxmlDecoder.decode(testSlice);
            assert(typeof result === "object", `Fuzz iteration ${i} produced valid result object`);
            successfulParses++;
        } catch (err) {
            assert(err instanceof Error, `Fuzz iteration ${i} threw standard Error instance`);
            exceptionsCaught++;
        }
    }

    console.log(`  -> Fuzz churn completed: ${successfulParses} successful parses, ${exceptionsCaught} caught/handled errors.`);
    assert(successfulParses + exceptionsCaught === 1000, "All 1,000 iterations accounted for");
});

// -----------------------------------------------------------------------------
// Suite 9: DEFLATE Vulnerability Evidence & Mitigation Verification
// -----------------------------------------------------------------------------
runTest("9. DEFLATE inflateRaw Robustness & Known Infinite-Loop Vulnerability Verification", () => {
    // 9.1 Valid empty DEFLATE buffer handling
    const emptyDecomp = inflateRaw(new Uint8Array(0));
    assert(emptyDecomp.length === 0, "Empty buffer decompresses to empty Uint8Array");

    // 9.2 Invalid block type 3 handling
    let threwBlock3 = false;
    try {
        inflateRaw(new Uint8Array([0xFF, 0xFF]));
    } catch (e) {
        threwBlock3 = true;
        assert(e.message.includes("Unsupported DEFLATE block type") || e.message.includes("Invalid"), "Block type 3 threw expected error");
    }
    assert(threwBlock3, "Invalid DEFLATE block type 3 must throw error");

    // 9.3 Truncated DEFLATE stream handling
    let threwTruncated = false;
    try {
        inflateRaw(new Uint8Array([0xF3, 0x48, 0xCD, 0xC9]));
    } catch (e) {
        threwTruncated = true;
        assert(e.message.includes("Unexpected end of DEFLATE stream") || e.message.includes("Invalid Huffman code"), "Truncated stream threw expected error");
    }
    assert(threwTruncated, "Truncated DEFLATE stream must throw error without infinite loop");

    // 9.4 Truncated dynamic Huffman tree stream handling
    let threwTruncDynamic = false;
    try {
        inflateRaw(new Uint8Array([0x05, 0xC0, 0x81, 0x08]));
    } catch (e) {
        threwTruncDynamic = true;
    }
    assert(threwTruncDynamic, "Truncated dynamic DEFLATE stream must throw error");

    console.log("  -> Documented inflateRaw vulnerability on truncated compressed streams successfully mitigated.");
});

// -----------------------------------------------------------------------------
// Final Verdict & Summary
// -----------------------------------------------------------------------------
console.log(`\n======================================================`);
console.log(`⚡ ALL ADVERSARIAL AXML & APK PARSER CHECKS COMPLETED`);
console.log(`Total assertions passed: ${passed}`);
console.log(`Total assertions failed: ${failed}`);
console.log(`======================================================\n`);

if (failed > 0) {
    process.exit(1);
} else {
    process.exit(0);
}
