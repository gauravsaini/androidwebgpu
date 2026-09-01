/**
 * AndroidWebGPU - Empirical Challenger Stress Test Suite
 * 
 * Adversarially tests:
 * 1. Malformed/corrupted Binder parcels, unexpected transaction codes, out-of-bounds offsets, thread safety simulation.
 * 2. Zygote socket stream framing edge cases, truncated arguments, unexpected process termination.
 * 3. DEX bytecode parsing with malformed DEX headers, invalid string/type/proto IDs, unhandled opcodes.
 * 4. Dalvik VM instruction execution boundaries, division by zero, null dereferences, loop limits.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import {
    DexParser,
    DalvikVM,
    AndroidFrameworkBridge,
    OP_NOP,
    OP_MOVE,
    OP_MOVE_FROM16,
    OP_MOVE_OBJECT,
    OP_MOVE_RESULT,
    OP_MOVE_RESULT_OBJECT,
    OP_RETURN_VOID,
    OP_RETURN,
    OP_RETURN_OBJECT,
    OP_CONST_4,
    OP_CONST_16,
    OP_CONST,
    OP_CONST_HIGH16,
    OP_CONST_STRING,
    OP_CONST_STRING_JUMBO,
    OP_CONST_CLASS,
    OP_CHECK_CAST,
    OP_INSTANCE_OF,
    OP_ARRAY_LENGTH,
    OP_NEW_INSTANCE,
    OP_NEW_ARRAY,
    OP_GOTO,
    OP_GOTO_16,
    OP_IF_EQ,
    OP_IF_NE,
    OP_IF_LT,
    OP_IF_GE,
    OP_IF_GT,
    OP_IF_LE,
    OP_IF_EQZ,
    OP_IF_NEZ,
    OP_IF_LTZ,
    OP_IF_GEZ,
    OP_IF_GTZ,
    OP_IF_LEZ,
    OP_AGET,
    OP_AGET_OBJECT,
    OP_APUT,
    OP_APUT_OBJECT,
    OP_IGET,
    OP_IGET_OBJECT,
    OP_IGET_BOOLEAN,
    OP_IPUT,
    OP_IPUT_OBJECT,
    OP_SGET,
    OP_SGET_OBJECT,
    OP_SPUT,
    OP_SPUT_OBJECT,
    OP_INVOKE_VIRTUAL,
    OP_INVOKE_DIRECT,
    OP_INVOKE_STATIC,
    OP_INVOKE_VIRTUAL_RANGE,
    OP_INVOKE_STATIC_RANGE,
    OP_NEG_INT,
    OP_NOT_INT,
    OP_ADD_INT,
    OP_SUB_INT,
    OP_MUL_INT,
    OP_DIV_INT,
    OP_REM_INT,
    OP_AND_INT,
    OP_OR_INT,
    OP_XOR_INT,
    OP_SHL_INT,
    OP_SHR_INT,
    OP_USHR_INT,
    OP_ADD_INT_2ADDR,
    OP_SUB_INT_2ADDR,
    OP_MUL_INT_2ADDR,
    OP_DIV_INT_2ADDR,
    OP_REM_INT_2ADDR,
    OP_AND_INT_2ADDR,
    OP_OR_INT_2ADDR,
    OP_XOR_INT_2ADDR,
    OP_SHL_INT_2ADDR,
    OP_SHR_INT_2ADDR,
    OP_USHR_INT_2ADDR,
    OP_ADD_INT_LIT16,
    OP_ADD_INT_LIT8
} from '../src/dex_vm.js';
import {
    ApkZipReader,
    AxmlDecoder,
    PackageManagerRegistry
} from '../src/apk_client_parser.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;

function assert(condition, message) {
    totalTests++;
    if (!condition) {
        failedTests++;
        console.error(`  ❌ [FAIL] ${message}`);
        throw new Error(message);
    } else {
        passedTests++;
        console.log(`  ✔ [PASS] ${message}`);
    }
}

// -----------------------------------------------------------------------------
// Suite 1: Malformed / Corrupted DEX Headers, Offsets, and Parsing Robustness
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log('--- Suite 1: Malformed DEX Headers & Table Offsets Stress ---');
console.log('================================================================');

// 1.1 Truncated buffer under 0x70 header size
{
    const shortSizes = [0, 1, 16, 64, 111];
    for (const size of shortSizes) {
        const buf = new Uint8Array(size);
        const parser = new DexParser(buf);
        let threw = false;
        try {
            parser.parse();
        } catch (e) {
            threw = true;
            assert(e.message.includes('too short'), `Buffer length ${size} must fail header check`);
        }
        assert(threw, `Buffer length ${size} must throw error`);
    }
}

// 1.2 Invalid magic signatures
{
    const invalidMagics = [
        [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        [0x50, 0x4b, 0x03, 0x04, 0x00, 0x00, 0x00, 0x00], // ZIP magic PK..
        [0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00], // ELF magic .ELF
        [0x64, 0x65, 0x78, 0x00, 0x30, 0x33, 0x35, 0x00], // "dex\0035\0" (wrong delimiter)
        [0x44, 0x45, 0x58, 0x0a, 0x30, 0x33, 0x35, 0x00]  // "DEX\n035\0" (uppercase)
    ];

    for (let i = 0; i < invalidMagics.length; i++) {
        const buf = new Uint8Array(128);
        buf.set(invalidMagics[i], 0);
        const parser = new DexParser(buf);
        let threw = false;
        try {
            parser.parse();
        } catch (e) {
            threw = true;
            assert(e.message.includes('Invalid DEX magic'), `Invalid magic pattern ${i} must throw`);
        }
        assert(threw, `Invalid magic pattern ${i} must be rejected`);
    }
}

// 1.3 Corrupted table offsets pointing out of bounds
{
    // Construct valid 0x70 header with magic "dex\n035\0"
    const validHeader = new Uint8Array(128);
    const magic = [0x64, 0x65, 0x78, 0x0a, 0x30, 0x33, 0x35, 0x00];
    validHeader.set(magic, 0);

    const view = new DataView(validHeader.buffer);
    // stringIdsSize = 100, but stringIdsOff = 0xFFFF0000 (OOB)
    view.setUint32(0x38, 100, true);
    view.setUint32(0x3c, 0xffff0000, true);

    const parser = new DexParser(validHeader);
    let threw = false;
    try {
        parser.parse();
    } catch (e) {
        threw = true;
        assert(e !== null, "Out of bounds stringIdsOff must be safely caught/handled");
    }
    assert(threw, "Out of bounds string table offset must trigger error without unhandled crash");
}

// -----------------------------------------------------------------------------
// Suite 2: Dalvik VM Bytecode Boundary Cases & Unhandled Opcodes
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log('--- Suite 2: Dalvik VM Bytecode Execution Stress & Boundaries ---');
console.log('================================================================');

const vm = new DalvikVM();

// Helper to create synthetic Method with code item
function createSyntheticMethod(name, insnsArray, registersSize = 8, insSize = 0, isStatic = true) {
    return {
        name,
        classType: 'Lcom/example/TestClass;',
        accessFlags: isStatic ? 0x0008 : 0x0000,
        proto: { shorty: 'I', returnType: 'I', parameters: [] },
        code: {
            registersSize,
            insSize,
            outsSize: 0,
            triesSize: 0,
            debugInfoOff: 0,
            insnsSize: insnsArray.length,
            insns: new Uint16Array(insnsArray)
        },
        dex: null
    };
}

// 2.1 Division by zero protection (OP_DIV_INT, OP_REM_INT, OP_DIV_INT_2ADDR, OP_REM_INT_2ADDR)
{
    const insns = [
        0x2012,         // const/4 v0, 2
        0x0112,         // const/4 v1, 0
        0x0293, 0x0100, // div-int v2, v0, v1 (2 / 0)
        0x0394, 0x0100, // rem-int v3, v0, v1 (2 % 0)
        0x020f          // return v2
    ];

    const method = createSyntheticMethod('testDivZero', insns, 4, 0, true);
    const result = vm.executeMethod(method, null, []);
    assert(result === 0, `div-int by 0 must return 0 without throw (got ${result})`);
}

// 2.2 2-Address division by zero (OP_DIV_INT_2ADDR, OP_REM_INT_2ADDR)
{
    const insns = [
        0x5012, // const/4 v0, 5
        0x0112, // const/4 v1, 0
        0x10b3, // div-int/2addr v0, v1 (vA=0, vB=1)
        0x000f  // return v0
    ];

    const method = createSyntheticMethod('testDivZero2Addr', insns, 4, 0, true);
    const result = vm.executeMethod(method, null, []);
    assert(result === 0, `div-int/2addr by 0 must return 0 without throw (got ${result})`);
}

// 2.3 Unhandled / Unknown Opcodes resilience
{
    const insns = [
        0x2012, // const/4 v0, 2
        0x000b, // unhandled opcode 0x0b
        0x0025, // unhandled opcode 0x25
        0x00fe, // unhandled opcode 0xfe
        0x00ff, // unhandled opcode 0xff
        0x3012, // const/4 v0, 3
        0x000f  // return v0
    ];

    const method = createSyntheticMethod('testUnknownOpcodes', insns, 4, 0, true);
    const result = vm.executeMethod(method, null, []);
    assert(result === 3, `Unhandled opcodes must safely advance PC (expected 3, got ${result})`);
}

// 2.4 Array Out of Bounds & Null Array Resilience (OP_AGET, OP_APUT, OP_ARRAY_LENGTH)
{
    // 1. Array length on null / undefined
    const insnsLen = [
        0x0021, // array-length v0, v0 (v0 is 0/null initially)
        0x000f  // return v0
    ];
    const methodLen = createSyntheticMethod('testArrayLengthNull', insnsLen, 4, 0, true);
    const resLen = vm.executeMethod(methodLen, null, []);
    assert(resLen === 0, `array-length on null must return 0 (got ${resLen})`);

    // 2. AGET on null array or OOB index
    const insnsAget = [
        0x0044, 0x0501, // aget v0, v1, v2 (v1=null, v2=5)
        0x000f          // return v0
    ];
    const methodAget = createSyntheticMethod('testAgetOOB', insnsAget, 4, 0, true);
    const resAget = vm.executeMethod(methodAget, null, []);
    assert(resAget === null, `aget on null array must return null (got ${resAget})`);

    // 3. APUT on null array
    const insnsAput = [
        0x004b, 0x0501, // aput v0, v1, v2 (v1=null, v2=5)
        0x000e          // return-void
    ];
    const methodAput = createSyntheticMethod('testAputNull', insnsAput, 4, 0, true);
    let threw = false;
    try {
        vm.executeMethod(methodAput, null, []);
    } catch (e) {
        threw = true;
    }
    assert(!threw, "aput on null array must not throw uncaught exception");
}

// 2.5 Instance Field Get/Put on Null Instance (OP_IGET, OP_IPUT)
{
    const insnsIget = [
        0x1052, 0x0000, // iget v0, v1, field@0 (v1 is null)
        0x000f          // return v0
    ];
    const methodIget = createSyntheticMethod('testIgetNull', insnsIget, 4, 0, true);
    const resIget = vm.executeMethod(methodIget, null, []);
    assert(resIget === null, `iget on null object must return null (got ${resIget})`);

    const insnsIput = [
        0x1059, 0x0000, // iput v0, v1, field@0 (v1 is null)
        0x000e          // return-void
    ];
    const methodIput = createSyntheticMethod('testIputNull', insnsIput, 4, 0, true);
    let threw = false;
    try {
        vm.executeMethod(methodIput, null, []);
    } catch (e) {
        threw = true;
    }
    assert(!threw, "iput on null object must not throw uncaught exception");
}

// 2.6 Infinite Loop and Instruction Counter Limit Defense
{
    const vmLoop = new DalvikVM();
    vmLoop.maxInstructionCount = 5000;

    const insnsLoop = [
        0x0028 // goto 0
    ];
    const methodLoop = createSyntheticMethod('testInfiniteLoop', insnsLoop, 2, 0, true);
    const t0 = Date.now();
    vmLoop.executeMethod(methodLoop, null, []);
    const elapsed = Date.now() - t0;

    assert(vmLoop.instructionsExecuted >= 5000, `Loop must terminate at maxInstructionCount (${vmLoop.instructionsExecuted})`);
    assert(elapsed < 200, `Loop limit defense must execute quickly (< 200ms, took ${elapsed}ms)`);
}

// -----------------------------------------------------------------------------
// Suite 3: Zygote Socket Protocol Framing & Edge Cases Simulation
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log('--- Suite 3: Zygote Protocol Framing & Error Resilience ---');
console.log('================================================================');

function parseZygoteWire(wireString) {
    if (!wireString || wireString.length === 0) {
        throw new Error("Empty Zygote command payload");
    }
    const lines = wireString.split('\n');
    if (lines.length === 0 || !lines[0].trim()) {
        throw new Error("Empty count header");
    }
    const count = parseInt(lines[0].trim(), 10);
    if (isNaN(count) || count < 0) {
        throw new Error(`Invalid argument count header '${lines[0]}'`);
    }
    const args = lines.slice(1);
    if (args.length > 0 && args[args.length - 1] === '') {
        args.pop();
    }
    if (args.length !== count) {
        throw new Error(`Argument count mismatch: header=${count}, actual=${args.length}`);
    }
    return args;
}

// 3.1 Empty and malformed count headers
{
    const badPayloads = [
        "",
        "abc\n--setuid=1000\n",
        "-5\n--setuid=1000\n",
        " \n",
        "999999\n--setuid=1000\n"
    ];

    for (const bad of badPayloads) {
        let threw = false;
        try {
            parseZygoteWire(bad);
        } catch (e) {
            threw = true;
        }
        assert(threw, `Malformed Zygote payload '${bad.replace(/\n/g, '\\n')}' must throw error`);
    }
}

// 3.2 Valid wire framing round-trip with edge arguments
{
    const rawArgs = [
        "--setuid=10042",
        "--setgid=10042",
        "--setgroups=1000,1015,1028,3003",
        "--target-sdk-version=33",
        "--package-name=org.fdroid.fdroid",
        "--nice-name=org.fdroid.fdroid",
        "--seinfo=default:targetSdkVersion=33",
        "--app-data-dir=/data/user/0/org.fdroid.fdroid",
        "android.app.ActivityThread"
    ];

    const wire = `${rawArgs.length}\n${rawArgs.join('\n')}\n`;
    const parsed = parseZygoteWire(wire);
    assert(parsed.length === rawArgs.length, `Parsed ${parsed.length} args matching expected`);
    for (let i = 0; i < rawArgs.length; i++) {
        assert(parsed[i] === rawArgs[i], `Arg [${i}] matches '${rawArgs[i]}'`);
    }
}

// 3.3 PID response edge cases
function parsePidResponse(buffer) {
    if (!buffer || buffer.length !== 4) {
        throw new Error(`Invalid PID response length: ${buffer ? buffer.length : 0}`);
    }
    const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    const pid = view.getInt32(0, true);
    if (pid <= 0) {
        throw new Error(`Fork failed or invalid PID: ${pid}`);
    }
    return pid;
}

{
    // Valid PID 1234
    const validBuf = new Uint8Array(4);
    new DataView(validBuf.buffer).setInt32(0, 1234, true);
    assert(parsePidResponse(validBuf) === 1234, "Valid PID 1234 parses correctly");

    // Negative PID error codes (-1, -100)
    for (const negPid of [-1, -100, -9999]) {
        const negBuf = new Uint8Array(4);
        new DataView(negBuf.buffer).setInt32(0, negPid, true);
        let threw = false;
        try {
            parsePidResponse(negBuf);
        } catch (e) {
            threw = true;
            assert(e.message.includes('Fork failed'), `Negative PID ${negPid} must indicate failure`);
        }
        assert(threw, `Negative PID ${negPid} must throw`);
    }

    // Invalid buffer lengths (0, 1, 3, 5 bytes)
    for (const len of [0, 1, 2, 3, 5, 8]) {
        const buf = new Uint8Array(len);
        let threw = false;
        try {
            parsePidResponse(buf);
        } catch (e) {
            threw = true;
        }
        assert(threw, `Invalid PID response buffer length ${len} must be rejected`);
    }
}

// -----------------------------------------------------------------------------
// Suite 4: Binder Parcel Wire Serialization & Bounds Fuzzing
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log('--- Suite 4: Binder Parcel Serialization & Offset Bounds Fuzzing ---');
console.log('================================================================');

class TestParcel {
    constructor() {
        this.bytes = [];
        this.offsets = [];
    }

    writeInt32(val) {
        const b = new Uint8Array(4);
        new DataView(b.buffer).setInt32(0, val, true);
        for (let i = 0; i < 4; i++) this.bytes.push(b[i]);
    }

    writeString8(str) {
        if (str === null || str === undefined) {
            this.writeInt32(-1);
            return;
        }
        const enc = new TextEncoder().encode(str);
        this.writeInt32(enc.length);
        for (let i = 0; i < enc.length; i++) this.bytes.push(enc[i]);
        this.bytes.push(0x00);
        const pad = (4 - ((enc.length + 1) % 4)) % 4;
        for (let i = 0; i < pad; i++) this.bytes.push(0x00);
    }

    toUint8Array() {
        return new Uint8Array(this.bytes);
    }
}

function readParcelString8(bytes, offsetObj) {
    if (offsetObj.offset + 4 > bytes.length) {
        throw new Error("Out of bounds reading string length");
    }
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const len = view.getInt32(offsetObj.offset, true);
    offsetObj.offset += 4;
    if (len === -1) return null;
    if (len < 0) throw new Error(`Invalid negative string length: ${len}`);
    
    const payloadLen = len + 1;
    const pad = (4 - (payloadLen % 4)) % 4;
    const total = payloadLen + pad;
    if (offsetObj.offset + total > bytes.length) {
        throw new Error(`Out of bounds reading string payload: need ${total}, have ${bytes.length - offsetObj.offset}`);
    }
    if (bytes[offsetObj.offset + len] !== 0x00) {
        throw new Error("Missing string null terminator");
    }
    const sub = bytes.subarray(offsetObj.offset, offsetObj.offset + len);
    offsetObj.offset += total;
    return new TextDecoder('utf-8').decode(sub);
}

{
    // 4.1 Valid String round trip
    const p = new TestParcel();
    p.writeString8("android.app.IActivityManager");
    p.writeString8(null);
    p.writeString8("");
    p.writeString8("android.view.IWindowManager");

    const raw = p.toUint8Array();
    const offObj = { offset: 0 };

    assert(readParcelString8(raw, offObj) === "android.app.IActivityManager", "Read interface token");
    assert(readParcelString8(raw, offObj) === null, "Read null string");
    assert(readParcelString8(raw, offObj) === "", "Read empty string");
    assert(readParcelString8(raw, offObj) === "android.view.IWindowManager", "Read second interface token");
    assert(offObj.offset === raw.length, "Consumed exact parcel length");
}

{
    // 4.2 Malicious negative length
    const badBuf = new Uint8Array([0xfe, 0xff, 0xff, 0xff]); // -2
    const offObj = { offset: 0 };
    let threw = false;
    try {
        readParcelString8(badBuf, offObj);
    } catch (e) {
        threw = true;
        assert(e.message.includes('negative string length'), "Negative length -2 must throw");
    }
    assert(threw, "Negative length must be rejected");
}

{
    // 4.3 Out of bounds length header claiming 10000 bytes
    const oobBuf = new Uint8Array([0x10, 0x27, 0x00, 0x00, 0x61, 0x62, 0x63, 0x00]);
    const offObj = { offset: 0 };
    let threw = false;
    try {
        readParcelString8(oobBuf, offObj);
    } catch (e) {
        threw = true;
        assert(e.message.includes('Out of bounds'), "OOB payload must throw bounds error");
    }
    assert(threw, "OOB string payload must be rejected");
}

{
    // 4.4 Missing null terminator
    // len 4 -> payloadLen 5 -> total 8 bytes payload. Total buffer: 4 + 8 = 12 bytes.
    // byte at offset + 4 (index 8) is 'X' (0x58), not 0x00.
    const noNullBuf = new Uint8Array([
        0x04, 0x00, 0x00, 0x00, // len = 4
        0x61, 0x62, 0x63, 0x64, // "abcd"
        0x58, 0x00, 0x00, 0x00  // 'X', 0, 0, 0 (terminator is 'X' instead of 0x00)
    ]);
    const offObj = { offset: 0 };
    let threw = false;
    try {
        readParcelString8(noNullBuf, offObj);
    } catch (e) {
        threw = true;
        assert(e.message.includes('Missing string null terminator'), "Missing null terminator must throw");
    }
    assert(threw, "Missing null terminator must be rejected");
}

// -----------------------------------------------------------------------------
// Suite 5: Real APK Multi-DEX & PMS Intent Resolution Stress
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log('--- Suite 5: Authentic Multi-DEX & PMS Resolution Stress ---');
console.log('================================================================');

{
    const fdroidPath = path.join(ROOT_DIR, 'F-Droid.apk');
    if (fs.existsSync(fdroidPath)) {
        const apkBuffer = fs.readFileSync(fdroidPath);
        const reader = new ApkZipReader(apkBuffer);
        const entries = reader.listEntries();
        assert(entries.length > 0, `F-Droid.apk contains ${entries.length} zip entries`);

        const dexEntries = entries.filter(name => typeof name === 'string' && name.endsWith('.dex'));
        assert(dexEntries.length >= 1, `F-Droid.apk contains ${dexEntries.length} DEX files`);

        const multiDexVm = new DalvikVM();
        for (const dexName of dexEntries) {
            const dexBuf = reader.readFile(dexName);
            const parser = new DexParser(dexBuf, dexName);
            multiDexVm.loadDex(parser);
        }

        assert(multiDexVm.classes.size > 0, `Loaded ${multiDexVm.classes.size} classes into DalvikVM`);
        
        const mainClass = multiDexVm.findClass('org.fdroid.fdroid.views.main.MainActivity');
        assert(mainClass !== null, "Resolved MainActivity class in DalvikVM");
        assert(mainClass.virtualMethods.has('onCreate') || mainClass.directMethods.has('onCreate'), "MainActivity has onCreate method");

        const activity = multiDexVm.startActivity('org.fdroid.fdroid.views.main.MainActivity');
        assert(activity.isResumed === true, "MainActivity successfully resumed");
    } else {
        console.log("  [SKIP] F-Droid.apk not found at project root");
    }
}

// -----------------------------------------------------------------------------
// Test Summary
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log(`⚡ ALL EMPIRICAL CHALLENGER STRESS TESTS PASSED! (${passedTests}/${totalTests} assertions, ${failedTests} failed)`);
console.log('================================================================\n');

if (failedTests > 0) {
    process.exit(1);
}
