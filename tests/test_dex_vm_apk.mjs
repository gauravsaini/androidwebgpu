/**
 * Unit & Integration Test Suite for DEX Bytecode Parser, Dalvik Virtual Machine,
 * and Real Android APK Runtime Execution.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import { ApkZipReader } from '../src/apk_client_parser.js';
import { DexParser, DalvikVM, OP_ADD_INT, OP_CONST_16, OP_RETURN } from '../src/dex_vm.js';
import { AndroidRuntime } from '../src/android_runtime.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`❌ FAIL: ${message}`);
        failed++;
        throw new Error(message);
    } else {
        passed++;
    }
}

console.log("======================================================");
console.log("▶ Running DEX Parser & Dalvik VM Test Suite");
console.log("======================================================");

// 1. Test F-Droid.apk classes.dex and classes2.dex reading and parsing
console.log("1. Testing real F-Droid.apk DEX extraction & parsing...");
const buf = fs.readFileSync("F-Droid.apk");
const zip = new ApkZipReader(buf);

const dex1 = zip.readFile("classes.dex");
assert(dex1 && dex1.length > 0, "classes.dex must exist in F-Droid.apk");

const parser1 = new DexParser(dex1, "classes.dex").parse();
assert(parser1.classes.size > 10000, `Parsed classes (${parser1.classes.size}) must exceed 10,000`);
assert(parser1.methods.length > 50000, `Parsed methods (${parser1.methods.length}) must exceed 50,000`);
assert(parser1.strings.length > 50000, `Parsed strings (${parser1.strings.length}) must exceed 50,000`);

const dex2 = zip.readFile("classes2.dex");
assert(dex2 && dex2.length > 0, "classes2.dex must exist in F-Droid.apk");
const parser2 = new DexParser(dex2, "classes2.dex").parse();
assert(parser2.classes.size > 10000, `Parsed classes2 (${parser2.classes.size}) must exceed 10,000`);

// 2. Test Dalvik VM Class Registry & Method Resolution
console.log("2. Testing Dalvik VM Class Loading & Symbol Lookup...");
const vm = new DalvikVM();
vm.loadDex(parser1);
vm.loadDex(parser2);

const mainActivityDef = vm.findClass("org.fdroid.fdroid.views.main.MainActivity");
assert(mainActivityDef !== null, "MainActivity class definition must be found in Dalvik VM");
assert(mainActivityDef.normalizedName === "org.fdroid.fdroid.views.main.MainActivity", "Class name normalized properly");
assert(mainActivityDef.directMethods.has("<init>"), "MainActivity must have <init>() direct method");
assert(mainActivityDef.virtualMethods.has("onCreate"), "MainActivity must have onCreate() virtual method");

// 3. Test Dalvik VM Opcode Interpreter Engine
console.log("3. Testing Dalvik Bytecode Execution Engine...");
const syntheticMethod = {
    name: 'calculateSum',
    accessFlags: 0x0008, // static
    code: {
        registersSize: 4,
        insSize: 2,
        outsSize: 0,
        triesSize: 0,
        debugInfoOff: 0,
        insnsSize: 6,
        // v2 = const/16 (100) -> 0x13
        // v3 = const/16 (250) -> 0x13
        // v0 = add-int v2, v3 -> 0x90
        // return v0 -> 0x0f
        insns: new Uint16Array([
            0x0013, 100,      // const/16 v0, 100
            0x0113, 250,      // const/16 v1, 250
            0x0090, 0x0100,   // add-int v0, v0, v1 (vA=0, vB=0, vC=1)
            0x000f            // return v0
        ])
    }
};

const result = vm.executeMethod(syntheticMethod, null, []);
assert(result === 350, `Bytecode calculation must return 350, got ${result}`);

// 4. Test Android Activity Lifecycle Dispatch
console.log("4. Testing Android Activity Lifecycle dispatch...");
const activityInstance = vm.startActivity("org.fdroid.fdroid.views.main.MainActivity");
assert(activityInstance !== null, "startActivity must return Activity instance");
assert(activityInstance.className === "org.fdroid.fdroid.views.main.MainActivity", "Activity className match");
assert(activityInstance.isResumed === true, "Activity state resumed after lifecycle dispatch");

// 5. Test AndroidRuntime Engine Container Render
console.log("5. Testing AndroidRuntime Ingestion & UI Presentation...");
const runtime = new AndroidRuntime();

const mockContainer = {
    innerHTML: '',
    style: {},
    appendChild(child) { this.children = this.children || []; this.children.push(child); },
    querySelector(sel) { return null; }
};

let errorLogged = false;
try {
    await runtime.loadAndRunApk(buf, mockContainer);
} catch (e) {
    errorLogged = true;
}
assert(!errorLogged, "loadAndRunApk must successfully parse and register real APK without exception");
assert(runtime.activeApps.has("org.fdroid.fdroid"), "F-Droid must be registered in active runtime apps");

console.log("======================================================");
console.log(`⚡ ALL DEX VM & REAL APK TESTS PASSED! (${passed} assertions passed)`);
console.log("======================================================");
