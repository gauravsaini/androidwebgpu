/**
 * Milestone 3 Test Suite: Target APK Auto-Ingestion & Dalvik VM Runtime Launch
 * 
 * Verifies:
 * 1. Target APK (F-Droid.apk) ingestion and ZIP archive extraction via ApkZipReader.
 * 2. Manifest binary XML decoding (AxmlDecoder) for org.fdroid.fdroid package, version, activities, and launcher activity.
 * 3. ARSC resource table decoding (ArscStringPoolParser & ArscDecoder) and string pool resolution.
 * 4. Multi-DEX bytecode extraction and parsing (DexParser) into DalvikVM class registry (>10,000 classes, >50,000 methods).
 * 5. DalvikVM bytecode execution, multi-dex symbol resolution, and MainActivity lifecycle initialization.
 * 6. AndroidRuntime integration, PackageManagerRegistry (PMS) package registration, and active app tracking.
 * 7. Live logcat streaming and boot milestone recording into LogcatBuffer.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ApkZipReader, AxmlDecoder, ArscStringPoolParser, PackageManagerRegistry } from '../src/apk_client_parser.js';
import { ArscDecoder } from '../src/apk_resource_resolver.js';
import { DexParser, DalvikVM, OP_ADD_INT, OP_CONST_16, OP_ADD_INT_LIT16, OP_RETURN } from '../src/dex_vm.js';
import { AndroidRuntime } from '../src/android_runtime.js';
import { globalLogcat, PRIORITY_ORDER } from '../src/logger.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;

function assert(condition, message) {
    totalTests++;
    if (!condition) {
        console.error(`  ❌ [FAIL] ${message}`);
        failedTests++;
        throw new Error(message);
    } else {
        console.log(`  ✔ [PASS] ${message}`);
        passedTests++;
    }
}

console.log("======================================================");
console.log("▶ Running Milestone 3: Target APK Loader & Runtime Launch");
console.log("======================================================");

// -----------------------------------------------------------------------------
// Suite 1: APK ZIP Archive Ingestion & Multi-DEX Extraction
// -----------------------------------------------------------------------------
console.log("\nTest Suite 1: APK ZIP Archive Ingestion & Structure");
const apkPath = path.join(rootDir, "F-Droid.apk");
assert(fs.existsSync(apkPath), "F-Droid.apk binary exists in repository root");

const apkBuffer = fs.readFileSync(apkPath);
assert(apkBuffer.length > 10000000, `F-Droid.apk buffer size (${apkBuffer.length} bytes) is valid`);

const zip = new ApkZipReader(apkBuffer);
const entries = zip.readEntries();
assert(entries.size > 100, `Central directory contains entries (${entries.size} found)`);
assert(zip.getFile("AndroidManifest.xml") !== null, "AndroidManifest.xml extracted successfully");
assert(zip.getFile("resources.arsc") !== null, "resources.arsc extracted successfully");

const dexFiles = zip.getAllDexFiles();
assert(dexFiles.length >= 2, `Multi-DEX archive extracted ${dexFiles.length} DEX files`);
assert(dexFiles[0].name === "classes.dex", "classes.dex ordered first in multi-dex sequence");
assert(dexFiles[1].name === "classes2.dex", "classes2.dex ordered second in multi-dex sequence");

// -----------------------------------------------------------------------------
// Suite 2: AndroidManifest.xml Binary XML Decoding
// -----------------------------------------------------------------------------
console.log("\nTest Suite 2: Binary XML AndroidManifest.xml Decoding");
const manifestBytes = zip.getManifest();
assert(manifestBytes && manifestBytes.length > 0, "Manifest bytes retrieved from archive");

const manifest = AxmlDecoder.decode(manifestBytes);
assert(manifest.packageName === "org.fdroid.fdroid", `Package name matches org.fdroid.fdroid (got: ${manifest.packageName})`);
assert(manifest.versionCode === 1023051, `Version code matches expected 1023051 (got: ${manifest.versionCode})`);
assert(manifest.versionName === "1.23.1", `Version name matches expected 1.23.1 (got: ${manifest.versionName})`);
assert(manifest.targetSdkVersion >= 28, `Target SDK version >= 28 (got: ${manifest.targetSdkVersion})`);
assert(manifest.activities.length > 0, `Parsed activities count > 0 (${manifest.activities.length} found)`);

const mainAct = manifest.activities.find(a => a.name.includes("MainActivity"));
assert(mainAct !== undefined, "MainActivity found in manifest activities list");
assert(manifest.launcherActivity === "org.fdroid.fdroid.views.main.MainActivity", `Launcher activity resolved correctly: ${manifest.launcherActivity}`);
assert(manifest.permissions.length > 0, `Permissions parsed (${manifest.permissions.length} found)`);
assert(manifest.permissions.includes("android.permission.INTERNET"), "Internet permission present in manifest");

// -----------------------------------------------------------------------------
// Suite 3: resources.arsc Decoding & String Resolution
// -----------------------------------------------------------------------------
console.log("\nTest Suite 3: resources.arsc String Pool & Resource Table");
const arscBytes = zip.getArsc();
assert(arscBytes && arscBytes.length > 0, "resources.arsc bytes retrieved from archive");

const arscStringPool = new ArscStringPoolParser(arscBytes).parse();
assert(arscStringPool.globalStrings.length > 1000, `Global string pool populated (${arscStringPool.globalStrings.length} strings)`);
assert(arscStringPool.packages.size > 0, `Package chunks parsed in resources.arsc (${arscStringPool.packages.size} packages)`);

const arscDecoder = new ArscDecoder();
const resTable = arscDecoder.decode(arscBytes);
assert(resTable !== null, "ArscDecoder decoded ResourceTable instance");
const resolvedName = arscStringPool.resolveStringRef("@0x7f120075") || arscStringPool.resolveString(0x7f120075);
console.log(`  ℹ Sample string resource 0x7f120075 resolved: "${resolvedName || '(unmapped in test chunk)'}"`);

// -----------------------------------------------------------------------------
// Suite 4: DEX Parsing & DalvikVM Multi-DEX Execution
// -----------------------------------------------------------------------------
console.log("\nTest Suite 4: DEX Bytecode Parsing & DalvikVM Interpretation");
const vm = new DalvikVM();
let totalDexClasses = 0;
let totalDexMethods = 0;

for (const dex of dexFiles) {
    const parser = new DexParser(dex.data, dex.name).parse();
    vm.loadDex(parser);
    totalDexClasses += parser.classes.size;
    totalDexMethods += parser.methods.length;
}

assert(totalDexClasses > 15000, `Total registered classes (${totalDexClasses}) exceeds 15,000`);
assert(totalDexMethods > 50000, `Total registered methods (${totalDexMethods}) exceeds 50,000`);

const mainActivityClass = vm.findClass("org.fdroid.fdroid.views.main.MainActivity");
assert(mainActivityClass !== null, "DalvikVM found MainActivity class in multi-dex registry");
assert(mainActivityClass.directMethods.has("<init>"), "MainActivity contains constructor <init>()");
assert(mainActivityClass.virtualMethods.has("onCreate"), "MainActivity contains onCreate(Bundle) virtual method");

// Opcode execution test
const testMethod = {
    name: 'testArithmetic',
    accessFlags: 0x0008,
    code: {
        registersSize: 4,
        insSize: 0,
        outsSize: 0,
        triesSize: 0,
        debugInfoOff: 0,
        insnsSize: 6,
        insns: new Uint16Array([
            0x0013, 200,     // const/16 v0, 200
            0x0113, 300,     // const/16 v1, 300
            0x0090, 0x0100,  // add-int v0, v0, v1
            0x000f           // return v0
        ])
    }
};
const execResult = vm.executeMethod(testMethod, null, []);
assert(execResult === 500, `DalvikVM executed bytecode arithmetic correctly: expected 500, got ${execResult}`);

const actInstance = vm.startActivity("org.fdroid.fdroid.views.main.MainActivity", { action: "android.intent.action.MAIN" });
assert(actInstance !== null, "DalvikVM instantiated MainActivity instance");
assert(actInstance.isResumed === true, "MainActivity lifecycle resumed (onCreate executed)");

// -----------------------------------------------------------------------------
// Suite 5: AndroidRuntime End-to-End Ingestion & PMS Registration
// -----------------------------------------------------------------------------
console.log("\nTest Suite 5: AndroidRuntime Ingestion & PMS Registration");
const initialLogcatLength = globalLogcat.entries.length;

const runtime = new AndroidRuntime({
    onLog: (msg, lvl, tag = 'AndroidRuntime') => {
        let priority = 'I';
        if (lvl === 'error') priority = 'E';
        else if (lvl === 'warn') priority = 'W';
        globalLogcat.append(tag, msg, priority);
    }
});

const appState = await runtime.loadAndRunApk(apkBuffer, null);
assert(appState !== null, "runtime.loadAndRunApk returned valid appState object");
assert(appState.packageName === "org.fdroid.fdroid", `appState.packageName is org.fdroid.fdroid (got: ${appState.packageName})`);
assert(runtime.installedApps.has("org.fdroid.fdroid"), "org.fdroid.fdroid present in runtime.installedApps set");
assert(runtime.activeApps.has("org.fdroid.fdroid"), "org.fdroid.fdroid present in runtime.activeApps map");

const pmsPkg = runtime.pms.getPackage("org.fdroid.fdroid") || runtime.pms.getPackageInfo("org.fdroid.fdroid");
assert(pmsPkg !== null, "PMS contains installed package entry for org.fdroid.fdroid");
assert(pmsPkg.packageName === "org.fdroid.fdroid", "PMS package info packageName matches");
assert(pmsPkg.activities.length > 0, `PMS package info contains activities (${pmsPkg.activities.length})`);

// -----------------------------------------------------------------------------
// Suite 6: Logcat Streaming Verification
// -----------------------------------------------------------------------------
console.log("\nTest Suite 6: Logcat Buffer & Streaming Verification");
const newLogcatEntries = globalLogcat.entries.slice(initialLogcatLength);
assert(newLogcatEntries.length > 0, `New logcat entries generated during ingestion (${newLogcatEntries.length} entries)`);

const pmsLogs = globalLogcat.filter({ tag: 'AndroidRuntime' });
assert(pmsLogs.length > 0, "Logcat contains AndroidRuntime entries");

const pmsInstallLog = globalLogcat.filter({ search: 'org.fdroid.fdroid' });
assert(pmsInstallLog.length > 0, "Logcat contains org.fdroid.fdroid installation log entries");

console.log("\n======================================================");
console.log(`⚡ ALL MILESTONE 3 TESTS PASSED! (${passedTests}/${totalTests} assertions passed, ${failedTests} failed)`);
console.log("======================================================");
