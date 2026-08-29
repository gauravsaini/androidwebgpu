/**
 * AndroidWebGPU - Challenger 1 Empirical & Adversarial Stress Test Suite
 * 
 * Target: APK Ingestion, AXML Decoding, Multi-DEX Parsing, DalvikVM Class Resolution,
 * and PackageManagerRegistry Robustness.
 * 
 * Complies with ASD-STE100 Simplified Technical English and /ponytail /caveman rules.
 */

import fs from 'fs';
import {
    ApkZipReader,
    AxmlDecoder,
    ArscStringPoolParser,
    PackageManagerRegistry,
    inflateRaw,
    parseApk,
    RES_XML_TYPE,
    RES_STRING_POOL_TYPE,
    RES_XML_START_ELEMENT_TYPE,
    RES_XML_END_ELEMENT_TYPE
} from '../src/apk_client_parser.js';

import {
    DexParser,
    DalvikVM,
    OP_NOP,
    OP_MOVE,
    OP_RETURN_VOID,
    OP_RETURN,
    OP_CONST_4,
    OP_CONST_16,
    OP_CONST,
    OP_CONST_STRING,
    OP_ADD_INT,
    OP_SUB_INT,
    OP_MUL_INT,
    OP_DIV_INT,
    OP_GOTO
} from '../src/dex_vm.js';

import { AndroidRuntime } from '../src/android_runtime.js';

let passed = 0;
let failed = 0;
const results = [];

function assert(condition, message) {
    if (!condition) {
        console.error(`❌ FAIL: ${message}`);
        failed++;
        results.push({ name: message, status: 'FAIL' });
        throw new Error(message);
    } else {
        passed++;
        results.push({ name: message, status: 'PASS' });
        console.log(`  ✔ [PASS] ${message}`);
    }
}

console.log("===================================================================");
console.log("🔥 CHALLENGER 1: ADVERSARIAL STRESS-TEST HARNESS (R1 MULTI-DEX & INGESTION)");
console.log("===================================================================");

const firefoxBuffer = fs.readFileSync("firefox.apk");
assert(firefoxBuffer && firefoxBuffer.length > 100000000, `firefox.apk exists and is genuine (${firefoxBuffer.length} bytes)`);

// =============================================================================
// SUITE 1: ApkZipReader Edge Cases & Corrupted Headers
// =============================================================================
console.log("\n--- Suite 1: ApkZipReader Corrupted & Boundary Stress Tests ---");

// 1.1 Invalid buffer inputs
let errThrown = false;
try { new ApkZipReader(null); } catch (_) { errThrown = true; }
assert(errThrown, "1.1 ApkZipReader(null) throws clean Error");

errThrown = false;
try { new ApkZipReader(12345); } catch (_) { errThrown = true; }
assert(errThrown, "1.2 ApkZipReader(invalid_type) throws clean Error");

// 1.2 Buffer shorter than EOCD (< 22 bytes)
const shortBuf = new Uint8Array(20);
const shortZip = new ApkZipReader(shortBuf);
errThrown = false;
try { shortZip.readEntries(); } catch (e) {
    errThrown = e.message.includes("Buffer too short");
}
assert(errThrown, "1.3 Truncated buffer (< 22 bytes) rejected with 'Buffer too short'");

// 1.3 Random noise buffer (1024 bytes with no EOCD)
const noiseBuf = new Uint8Array(1024);
for (let i = 0; i < 1024; i++) noiseBuf[i] = (i * 37 + 13) & 0xFF;
const noiseZip = new ApkZipReader(noiseBuf);
errThrown = false;
try { noiseZip.readEntries(); } catch (e) {
    errThrown = e.message.includes("Cannot find End of Central Directory");
}
assert(errThrown, "1.4 Random noise buffer rejected with missing EOCD signature error");

// 1.4 Corrupted EOCD: CD offset pointing past buffer length
const malformedEocdBuf = new Uint8Array(64);
const dvEocd = new DataView(malformedEocdBuf.buffer);
dvEocd.setUint32(32, 0x06054b50, true); // EOCD signature at offset 32
dvEocd.setUint16(32 + 10, 10, true);   // 10 entries claimed
dvEocd.setUint32(32 + 16, 100000, true); // CD offset = 100,000 (out of bounds)
const malformedZip = new ApkZipReader(malformedEocdBuf);
const entries = malformedZip.readEntries();
assert(entries.size === 0, "1.5 Out-of-bounds CD offset terminates safely with 0 entries (no crash)");

// 1.5 Corrupted CD Entry: Invalid CD magic
const corruptCdBuf = new Uint8Array(128);
const dvCd = new DataView(corruptCdBuf.buffer);
dvCd.setUint32(0, 0x02014b51, true); // Corrupted CD magic (ending in 51 instead of 50)
dvCd.setUint32(64, 0x06054b50, true); // EOCD signature at 64
dvCd.setUint16(64 + 10, 5, true);     // 5 entries claimed
dvCd.setUint32(64 + 16, 0, true);      // CD offset = 0
const corruptCdZip = new ApkZipReader(corruptCdBuf);
const cdEntries = corruptCdZip.readEntries();
assert(cdEntries.size === 0, "1.6 Corrupt Central Directory signature terminates loop gracefully");

// 1.6 Corrupted Local File Header offset in CD entry
const corruptLhBuf = new Uint8Array(256);
const dvLh = new DataView(corruptLhBuf.buffer);
// CD Header at offset 0
dvLh.setUint32(0, 0x02014b50, true); // CD Magic
dvLh.setUint16(10, 0, true);          // Method 0 (stored)
dvLh.setUint32(20, 10, true);         // compSize = 10
dvLh.setUint32(24, 10, true);         // uncompSize = 10
dvLh.setUint16(28, 8, true);          // nameLen = 8
dvLh.setUint16(30, 0, true);          // extraLen = 0
dvLh.setUint16(32, 0, true);          // commentLen = 0
dvLh.setUint32(42, 50000, true);      // localHeaderOffset = 50,000 (out of bounds)
new TextEncoder().encodeInto("test.txt", corruptLhBuf.subarray(46, 54));

// EOCD at offset 100
dvLh.setUint32(100, 0x06054b50, true);
dvLh.setUint16(100 + 10, 1, true);
dvLh.setUint32(100 + 16, 0, true);

const corruptLhZip = new ApkZipReader(corruptLhBuf);
const fileRes = corruptLhZip.readFile("test.txt");
assert(fileRes === null, "1.7 Out-of-bounds localHeaderOffset returns null without throwing uncaught crash");

// 1.7 Local Header Magic mismatch
dvLh.setUint32(42, 60, true); // Local header offset = 60
dvLh.setUint32(60, 0x04034b51, true); // Invalid Local header magic (51 instead of 50)
const corruptLhZip2 = new ApkZipReader(corruptLhBuf);
const fileRes2 = corruptLhZip2.readFile("test.txt");
assert(fileRes2 === null, "1.8 Invalid Local Header magic returns null safely");

// 1.8 Unsupported compression method (e.g. 99)
dvLh.setUint16(10, 99, true); // method 99
dvLh.setUint32(60, 0x04034b50, true); // valid local header magic
dvLh.setUint16(60 + 26, 8, true); // nameLen = 8
dvLh.setUint16(60 + 28, 0, true); // extraLen = 0
const corruptMethodZip = new ApkZipReader(corruptLhBuf);
errThrown = false;
try { corruptMethodZip.readFile("test.txt"); } catch (e) {
    errThrown = e.message.includes("Unsupported ZIP compression method: 99");
}
assert(errThrown, "1.9 Unsupported compression method 99 throws clean descriptive error");

// 1.9 Pure JS inflateRaw stress test on malformed bitstreams
const emptyInflate = inflateRaw(new Uint8Array(0));
assert(emptyInflate.length === 0, "1.10 inflateRaw(empty) returns empty Uint8Array");

const truncatedInflate = inflateRaw(new Uint8Array([0x01, 0x02, 0x03]));
assert(truncatedInflate instanceof Uint8Array, "1.11 inflateRaw(truncated) executes without hanging");

// 1.10 Authentic Firefox APK Ingestion Verification
const realZip = new ApkZipReader(firefoxBuffer);
const allEntries = realZip.listEntries();
assert(allEntries.length === 3399, `1.12 Authentic firefox.apk contains 3,399 entries (got: ${allEntries.length})`);

const manifestBytes = realZip.getManifest();
assert(manifestBytes !== null && manifestBytes.length === 93568, `1.13 AndroidManifest.xml decompressed: 93,568 bytes (got: ${manifestBytes?.length})`);

const dexFiles = realZip.getAllDexFiles();
assert(dexFiles.length === 3, `1.14 Exactly 3 DEX files extracted: ${dexFiles.map(d => d.name).join(', ')}`);
assert(dexFiles[0].name === 'classes.dex', "1.15 First DEX file is classes.dex");
assert(dexFiles[1].name === 'classes2.dex', "1.16 Second DEX file is classes2.dex");
assert(dexFiles[2].name === 'classes3.dex', "1.17 Third DEX file is classes3.dex");

const nativeLibs = realZip.getNativeLibraries();
assert(nativeLibs.length === 18, `1.18 Exactly 18 native x86_64 ELF libraries identified (got: ${nativeLibs.length})`);
assert(nativeLibs.some(l => l.libName === 'libxul.so'), "1.19 libxul.so identified in native libraries");
assert(nativeLibs.some(l => l.libName === 'libmozglue.so'), "1.20 libmozglue.so identified in native libraries");


// =============================================================================
// SUITE 2: AxmlDecoder Edge Cases, Corrupt Chunks & Prototype Pollution
// =============================================================================
console.log("\n--- Suite 2: AxmlDecoder Malformed Chunks & Prototype Pollution ---");

// 2.1 Invalid buffer inputs
errThrown = false;
try { AxmlDecoder.decode(null); } catch (_) { errThrown = true; }
assert(errThrown, "2.1 AxmlDecoder.decode(null) throws clean Error");

// 2.2 Truncated buffer (< 8 bytes)
errThrown = false;
try { AxmlDecoder.decode(new Uint8Array(6)); } catch (e) {
    errThrown = e.message.includes("< 8 bytes");
}
assert(errThrown, "2.2 Truncated buffer (< 8 bytes) throws header length Error");

// 2.3 Invalid AXML Magic (not 0x0003)
const badMagicBuf = new Uint8Array(16);
new DataView(badMagicBuf.buffer).setUint16(0, 0x1234, true);
errThrown = false;
try { AxmlDecoder.decode(badMagicBuf); } catch (e) {
    errThrown = e.message.includes("Invalid AXML magic");
}
assert(errThrown, "2.3 Invalid AXML magic (0x1234) rejected cleanly");

// 2.4 Infinite Loop Prevention with chunk size <= 0
const loopBuf = new Uint8Array(64);
const dvLoop = new DataView(loopBuf.buffer);
dvLoop.setUint16(0, RES_XML_TYPE, true);
dvLoop.setUint32(4, 64, true); // Total size 64
dvLoop.setUint16(8, RES_STRING_POOL_TYPE, true);
dvLoop.setUint32(12, 0, true); // Chunk size = 0 (could trigger infinite loop if not guarded)

const loopStart = Date.now();
const loopDecoded = AxmlDecoder.decode(loopBuf);
const loopElapsed = Date.now() - loopStart;
assert(loopElapsed < 100, `2.4 Zero-size chunk does not freeze CPU (completed in ${loopElapsed}ms)`);
assert(typeof loopDecoded === 'object', "2.4 Zero-size chunk returns manifest object cleanly");

// 2.5 Prototype Pollution Testing via Synthetic AXML
// Construct a binary XML string pool containing __proto__, constructor, etc.
const makeAxmlWithStrings = (strings) => {
    const encoder = new TextEncoder();
    const encodedStrings = strings.map(s => encoder.encode(s + "\0"));
    let stringDataSize = 0;
    for (const es of encodedStrings) stringDataSize += es.length + 2; // +2 for len prefix

    const poolHeaderSize = 28;
    const offsetsSize = strings.length * 4;
    const stringsStart = poolHeaderSize + offsetsSize;
    const poolChunkSize = stringsStart + stringDataSize;
    const totalAxmlSize = 8 + poolChunkSize;

    const buf = new Uint8Array(totalAxmlSize);
    const dv = new DataView(buf.buffer);

    // Root XML Header
    dv.setUint16(0, RES_XML_TYPE, true);
    dv.setUint16(2, 8, true);
    dv.setUint32(4, totalAxmlSize, true);

    // String Pool Header
    dv.setUint16(8, RES_STRING_POOL_TYPE, true);
    dv.setUint16(10, poolHeaderSize, true);
    dv.setUint32(12, poolChunkSize, true);
    dv.setUint32(16, strings.length, true); // stringCount
    dv.setUint32(20, 0, true); // styleCount
    dv.setUint32(24, 1 << 8, true); // UTF-8 flag
    dv.setUint32(28, stringsStart, true); // stringsStart

    let curOffset = 0;
    let curDataPos = 8 + stringsStart;
    for (let i = 0; i < strings.length; i++) {
        dv.setUint32(8 + poolHeaderSize + i * 4, curOffset, true);
        const strBytes = encodedStrings[i];
        buf[curDataPos++] = strBytes.length - 1; // length
        buf[curDataPos++] = strBytes.length - 1; // utf8 length
        buf.set(strBytes, curDataPos);
        curDataPos += strBytes.length;
        curOffset += 2 + strBytes.length;
    }
    return buf;
};

const attackStrings = ["__proto__", "constructor", "prototype", "polluted", "valueOf", "toString"];
const attackAxml = makeAxmlWithStrings(attackStrings);
const attackDecoded = AxmlDecoder.decode(attackAxml);

assert(attackDecoded !== null, "2.5 Attack AXML with prototype keys parsed successfully");
assert(({}).polluted === undefined, "2.6 Object.prototype.polluted is undefined (No prototype pollution)");
assert(Object.prototype.toString instanceof Function, "2.7 Object.prototype.toString remains unpolluted standard function");

// 2.6 Authentic Firefox AndroidManifest.xml Verification
const realManifest = AxmlDecoder.decode(manifestBytes);
assert(realManifest.packageName === 'org.mozilla.firefox', `2.8 Manifest package name is 'org.mozilla.firefox' (got: ${realManifest.packageName})`);
assert(realManifest.targetSdkVersion === 37, `2.9 Manifest targetSdkVersion is 37 (got: ${realManifest.targetSdkVersion})`);
assert(realManifest.minSdkVersion === 26, `2.10 Manifest minSdkVersion is 26 (got: ${realManifest.minSdkVersion})`);
assert(realManifest.activities.length === 45, `2.11 Manifest contains 45 activities (got: ${realManifest.activities.length})`);
assert(realManifest.services.length === 108, `2.12 Manifest contains 108 services (got: ${realManifest.services.length})`);
assert(realManifest.permissions.length === 36, `2.13 Manifest contains 36 permissions (got: ${realManifest.permissions.length})`);
assert(realManifest.permissions.includes("android.permission.INTERNET"), "2.14 Manifest requests android.permission.INTERNET");
assert(realManifest.launcherActivity === 'org.mozilla.firefox.App' || realManifest.launcherActivity === 'org.mozilla.firefox.AppCool', `2.15 Manifest launcher activity identified (${realManifest.launcherActivity})`);


// =============================================================================
// SUITE 3: DexParser Edge Cases & Corrupt Header Stress Tests
// =============================================================================
console.log("\n--- Suite 3: DexParser Corrupt Headers & Truncated Bounds ---");

// 3.1 Invalid buffer inputs
errThrown = false;
try { new DexParser(null); } catch (_) { errThrown = true; }
assert(errThrown, "3.1 DexParser(null) throws clean Error");

// 3.2 Buffer < 0x70 bytes header
errThrown = false;
try { new DexParser(new Uint8Array(64)).parse(); } catch (e) {
    errThrown = e.message.includes("DEX file too short");
}
assert(errThrown, "3.2 Buffer < 0x70 bytes throws 'DEX file too short'");

// 3.3 Invalid Magic
const badDexBuf = new Uint8Array(128);
new TextEncoder().encodeInto("dey\n035\0", badDexBuf);
errThrown = false;
try { new DexParser(badDexBuf).parse(); } catch (e) {
    errThrown = e.message.includes("Invalid DEX magic header");
}
assert(errThrown, "3.3 Invalid DEX magic header 'dey\\n035\\0' rejected cleanly");

// 3.4 Authentic Firefox Multi-DEX Parsing
console.log("3.4 Parsing authentic Multi-DEX bytecode archives...");
const p1 = new DexParser(dexFiles[0].data, dexFiles[0].name).parse();
assert(p1.classes.size === 30592, `3.4.1 classes.dex loaded 30,592 classes (got: ${p1.classes.size})`);
assert(p1.methods.length === 65536, `3.4.2 classes.dex contains 65,536 methods (got: ${p1.methods.length})`);

const p2 = new DexParser(dexFiles[1].data, dexFiles[1].name).parse();
assert(p2.classes.size === 24666, `3.4.3 classes2.dex loaded 24,666 classes (got: ${p2.classes.size})`);
assert(p2.methods.length === 62468, `3.4.4 classes2.dex contains 62,468 methods (got: ${p2.methods.length})`);

const p3 = new DexParser(dexFiles[2].data, dexFiles[2].name).parse();
assert(p3.classes.size === 24754, `3.4.5 classes3.dex loaded 24,754 classes (got: ${p3.classes.size})`);
assert(p3.methods.length === 40932, `3.4.6 classes3.dex contains 40,932 methods (got: ${p3.methods.length})`);

const totalUniqueClasses = p1.classes.size + p2.classes.size + p3.classes.size;
assert(totalUniqueClasses === 80012, `3.4.7 Multi-DEX contains exactly 80,012 classes (got: ${totalUniqueClasses})`);


// =============================================================================
// SUITE 4: DalvikVM Multi-DEX Class Resolution, Performance & Bytecode Execution
// =============================================================================
console.log("\n--- Suite 4: DalvikVM 80,012 Class Resolution & Benchmark ---");

const vm = new DalvikVM();
vm.loadDex(p1);
vm.loadDex(p2);
vm.loadDex(p3);

assert(vm.dexParsers.length === 3, "4.1 DalvikVM loaded all 3 DexParsers");
assert(vm.classes.size >= 80012, `4.2 DalvikVM class registry contains >= 80,012 classes (got: ${vm.classes.size})`);

// 4.2 Cross-DEX symbol lookup verification
const firstP1Key = Array.from(p1.classes.keys())[0];
const firstP2Key = Array.from(p2.classes.keys())[0];
const firstP3Key = Array.from(p3.classes.keys())[0];

const class1 = vm.findClass(firstP1Key);
assert(class1 !== null, `4.3 findClass('${firstP1Key}') resolved from classes.dex`);

const class2 = vm.findClass(firstP2Key);
assert(class2 !== null, `4.4 findClass('${firstP2Key}') resolved from classes2.dex`);

const class3 = vm.findClass(firstP3Key);
assert(class3 !== null, `4.5 findClass('${firstP3Key}') resolved from classes3.dex`);

// 4.3 Non-existent class lookup
const missingClass = vm.findClass("com.nonexistent.NoSuchClass");
assert(missingClass === null, "4.6 findClass for missing class returns null safely");

// 4.4 Prototype pollution & dangerous key lookup
const protoLookup = vm.findClass("__proto__");
assert(!protoLookup, "4.7 findClass('__proto__') returns falsy without throwing TypeError");

const constructorLookup = vm.findClass("constructor");
assert(!constructorLookup, "4.8 findClass('constructor') returns falsy without throwing TypeError");

const toStringLookup = vm.findClass("toString");
assert(!toStringLookup, "4.9 findClass('toString') returns falsy without throwing TypeError");

// 4.5 Class Lookup Performance Benchmark (100,000 lookups against 80,012 classes)
console.log("4.10 Benchmarking 100,000 class lookups across 80,012 classes...");
const testTargets = [
    "org.mozilla.firefox.App",
    "org.mozilla.geckoview.GeckoSession",
    "mozilla.components.concept.engine.Engine",
    "org.mozilla.geckoview.GeckoRuntime",
    "com.nonexistent.DummyClass",
    "org.mozilla.firefox.MainActivity",
    "android.app.Activity",
    "java.lang.Object",
    "org.mozilla.geckoview.GeckoResult",
    "com.missing.Target"
];

const benchCount = 100000;
const tStart = (typeof performance !== 'undefined') ? performance.now() : Date.now();
for (let i = 0; i < benchCount; i++) {
    const target = testTargets[i % testTargets.length];
    vm.findClass(target);
}
const tEnd = (typeof performance !== 'undefined') ? performance.now() : Date.now();
const elapsedMs = tEnd - tStart;
const opsPerSec = Math.round((benchCount / elapsedMs) * 1000);
const latencyUs = (elapsedMs / benchCount) * 1000;

console.log(`  📊 Benchmark Result: ${benchCount} lookups in ${elapsedMs.toFixed(2)}ms (${opsPerSec.toLocaleString()} ops/sec, ${latencyUs.toFixed(3)} µs/op)`);
assert(opsPerSec > 200000, `4.10 Throughput (${opsPerSec.toLocaleString()} ops/sec) exceeds 200,000 ops/sec threshold`);
assert(latencyUs < 5.0, `4.11 Average lookup latency (${latencyUs.toFixed(3)} µs) is under 5 µs/lookup`);

// 4.6 DalvikVM Opcode Execution & Infinite Loop Protection
console.log("4.12 Testing Dalvik bytecode interpreter execution & bounds...");
const syntheticMethod = {
    name: "computeSum",
    accessFlags: 1,
    code: {
        registersSize: 4,
        insSize: 0,
        outsSize: 0,
        triesSize: 0,
        debugInfoOff: 0,
        insnsSize: 6,
        insns: new Uint16Array([
            OP_CONST_16 | (0 << 8), 100, // v0 = 100
            OP_CONST_16 | (1 << 8), 250, // v1 = 250
            OP_ADD_INT | (2 << 8), (0 | (1 << 8)), // v2 = v0 + v1
            OP_RETURN | (2 << 8) // return v2
        ])
    }
};

const resultVal = vm.executeMethod(syntheticMethod, null, []);
assert(resultVal === 350, `4.12 executeMethod(computeSum) returned 350 (got: ${resultVal})`);

// Infinite loop execution test (GOTO offset 0)
const loopMethod = {
    name: "infiniteLoop",
    accessFlags: 1,
    code: {
        registersSize: 2,
        insSize: 0,
        outsSize: 0,
        triesSize: 0,
        debugInfoOff: 0,
        insnsSize: 2,
        insns: new Uint16Array([
            OP_GOTO, 0 // branch back to pc 0
        ])
    }
};

vm.instructionsExecuted = 0;
const loopResult = vm.executeMethod(loopMethod, null, []);
assert(vm.instructionsExecuted >= vm.maxInstructionCount, "4.13 Infinite bytecode loop terminated safely at maxInstructionCount (1,000,000 instructions)");


// =============================================================================
// SUITE 5: PackageManagerRegistry Edge Cases & Prototype Hardening
// =============================================================================
console.log("\n--- Suite 5: PackageManagerRegistry Robustness & Intent Routing ---");

const registry = new PackageManagerRegistry();

// 5.1 Invalid metadata registration
errThrown = false;
try { registry.registerPackage(null); } catch (_) { errThrown = true; }
assert(errThrown, "5.1 registerPackage(null) throws clean Error");

errThrown = false;
try { registry.registerPackage({}); } catch (_) { errThrown = true; }
assert(errThrown, "5.2 registerPackage({}) missing packageName throws clean Error");

// 5.3 Prototype pollution protection
const protoPkg = registry.getPackageInfo("__proto__");
assert(protoPkg === null, "5.3 getPackageInfo('__proto__') returns null");

registry.registerPackage({
    packageName: "__proto__",
    applicationLabel: "HackerApp"
}, false);
assert(({}).applicationLabel === undefined, "5.4 Registering packageName='__proto__' does not pollute Object.prototype");

// 5.4 Duplicate Package Update
const initialPkg = registry.registerPackage({
    packageName: "com.test.app",
    applicationLabel: "Test App v1",
    versionCode: 1,
    activities: [{ name: "com.test.app.Main", label: "Main", exported: true }]
}, false);

const updatedPkg = registry.registerPackage({
    packageName: "com.test.app",
    applicationLabel: "Test App v2",
    versionCode: 2,
    activities: [{ name: "com.test.app.Main", label: "Main v2", exported: true }]
}, false);

assert(registry.getPackageInfo("com.test.app").versionCode === 2, "5.5 Duplicate package registration updates metadata cleanly");
assert(registry.getPackageInfo("com.test.app").applicationLabel === "Test App v2", "5.6 Application label updated to v2");

// 5.5 Intent Resolution
const allActs = registry.queryIntentActivities({});
assert(Array.isArray(allActs) && allActs.length > 0, "5.7 queryIntentActivities({}) returns registered activities");

const resolved = registry.resolveActivity("org.fdroid.fdroid");
assert(resolved && resolved.name === "org.fdroid.fdroid.views.main.MainActivity", "5.8 resolveActivity('org.fdroid.fdroid') resolves MainActivity");

// 5.6 Install authentic firefox.apk into registry
const installedFirefox = registry.installApk(firefoxBuffer);
assert(installedFirefox.packageName === 'org.mozilla.firefox', "5.9 installApk(firefoxBuffer) registers 'org.mozilla.firefox'");
assert(registry.getPackageInfo('org.mozilla.firefox') !== null, "5.10 getPackageInfo('org.mozilla.firefox') returns installed record");


// =============================================================================
// SUITE 6: End-to-End Ingestion & Runtime Resilience
// =============================================================================
console.log("\n--- Suite 6: End-to-End Runtime Ingestion Resilience ---");

const mockCanvas = {
    width: 720,
    height: 1440,
    getContext: () => ({
        fillStyle: '',
        fillRect: () => {},
        fillText: () => {},
        beginPath: () => {},
        arc: () => {},
        fill: () => {},
        stroke: () => {},
        measureText: (text) => ({ width: text.length * 8 }),
        createImageData: (w, h) => ({ data: new Uint8Array(w * h * 4) }),
        putImageData: () => {}
    })
};

const runtime = new AndroidRuntime(mockCanvas);

// 6.1 loadAndRunApk on null buffer
let loadErr = false;
try {
    await runtime.loadAndRunApk(null);
} catch (_) {
    loadErr = true;
}
assert(loadErr, "6.1 loadAndRunApk(null) rejects with Error");

// 6.2 loadAndRunApk on corrupted truncated buffer
loadErr = false;
try {
    await runtime.loadAndRunApk(new ArrayBuffer(16));
} catch (_) {
    loadErr = true;
}
assert(loadErr, "6.2 loadAndRunApk(truncated_buffer) rejects cleanly without process crash");

// 6.3 loadAndRunApk on authentic firefox.apk
const appState = await runtime.loadAndRunApk(firefoxBuffer);
assert(appState.packageName === 'org.mozilla.firefox', "6.3 loadAndRunApk sets appState.packageName = 'org.mozilla.firefox'");
assert(appState.activeUrl === 'https://www.google.com', "6.4 loadAndRunApk initializes activeUrl = 'https://www.google.com'");
assert(appState.currentPage === 'Google', "6.5 loadAndRunApk initializes currentPage = 'Google'");

console.log("\n===================================================================");
console.log(`🎉 ALL ADVERSARIAL STRESS TESTS PASSED! (${passed} assertions, ${failed} failures)`);
console.log("===================================================================");
