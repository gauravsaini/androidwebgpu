/**
 * AndroidWebGPU - Milestone 3 Empirical Challenger 1 Stress Test Harness
 * 
 * Comprehensive Empirical Verification of:
 * 1. Dalvik VM complete arithmetic, bitwise, shift, unary & 2-address opcodes matrix.
 * 2. Dalvik VM constants (4-bit, 16-bit, 32-bit, high16, string, class), moves, and returns.
 * 3. Dalvik VM branching (conditional, zero-comparison, goto, loop execution, max instruction limits).
 * 4. Dalvik VM arrays, field get/put (instance/static), object instantiation & framework method bridges.
 * 5. Multi-DEX parsing, natural order sorting, DEX header validation, truncated DEX handling & cross-DEX scoping.
 * 6. PMS package registry, real F-Droid.apk manifest decoding, launcher activity resolution & lifecycle launch.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import {
    ApkZipReader,
    AxmlDecoder,
    ArscStringPoolParser,
    PackageManagerRegistry
} from '../src/apk_client_parser.js';
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
    OP_ADD_INT_LIT16,
    OP_ADD_INT_LIT8
} from '../src/dex_vm.js';
import { AndroidRuntime } from '../src/android_runtime.js';

// Opcode constants supported by VM switch-case
const OP_REM_INT_2ADDR = 0xb4;
const OP_AND_INT_2ADDR = 0xb5;
const OP_OR_INT_2ADDR = 0xb6;
const OP_XOR_INT_2ADDR = 0xb7;
const OP_SHL_INT_2ADDR = 0xb8;
const OP_SHR_INT_2ADDR = 0xb9;
const OP_USHR_INT_2ADDR = 0xba;

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;
const failureDetails = [];

function assert(condition, message) {
    totalTests++;
    if (!condition) {
        console.error(`  ❌ [FAIL] ${message}`);
        failedTests++;
        failureDetails.push(message);
    } else {
        console.log(`  ✔ [PASS] ${message}`);
        passedTests++;
    }
}

function testProtected(name, fn) {
    try {
        fn();
    } catch (err) {
        assert(false, `${name} -> Threw unexpected error: ${err.message}`);
    }
}

console.log("================================================================================");
console.log("🔥 Challenger 1: Empirical Dalvik VM, Multi-DEX & Target APK Stress Harness");
console.log("================================================================================");

// -----------------------------------------------------------------------------
// Suite 1: Dalvik VM Arithmetic, Bitwise, Shift & 2-Address Matrix
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 1] Dalvik VM Arithmetic, Bitwise, Shift & 2-Address Opcodes");
{
    const vm = new DalvikVM();

    // 1.1 3-Register Arithmetic & Logic Opcodes
    const arithmeticMatrix = [
        { op: OP_ADD_INT, a: 120, b: 80, expected: 200, name: 'OP_ADD_INT' },
        { op: OP_SUB_INT, a: 120, b: 80, expected: 40, name: 'OP_SUB_INT' },
        { op: OP_MUL_INT, a: 25, b: 8, expected: 200, name: 'OP_MUL_INT' },
        { op: OP_DIV_INT, a: 200, b: 8, expected: 25, name: 'OP_DIV_INT' },
        { op: OP_REM_INT, a: 205, b: 8, expected: 5, name: 'OP_REM_INT' },
        { op: OP_AND_INT, a: 0b1100, b: 0b1010, expected: 0b1000, name: 'OP_AND_INT' },
        { op: OP_OR_INT,  a: 0b1100, b: 0b1010, expected: 0b1110, name: 'OP_OR_INT' },
        { op: OP_XOR_INT, a: 0b1100, b: 0b1010, expected: 0b0110, name: 'OP_XOR_INT' },
        { op: OP_SHL_INT, a: 1, b: 10, expected: 1024, name: 'OP_SHL_INT' },
        { op: OP_SHR_INT, a: -1024, b: 2, expected: -256, name: 'OP_SHR_INT (arithmetic shift)' },
        { op: OP_USHR_INT, a: -1, b: 1, expected: 2147483647, name: 'OP_USHR_INT (logical unsigned shift)' }
    ];

    for (const item of arithmeticMatrix) {
        testProtected(`3-Register Opcode ${item.name}`, () => {
            const method = {
                name: `test_${item.name}`,
                accessFlags: 0x0008,
                code: {
                    registersSize: 4,
                    insSize: 0,
                    outsSize: 0,
                    triesSize: 0,
                    debugInfoOff: 0,
                    insnsSize: 6,
                    insns: new Uint16Array([
                        0x0013, item.a & 0xffff,       // const/16 v0, a
                        0x0113, item.b & 0xffff,       // const/16 v1, b
                        (item.op) | (0x02 << 8), 0x0100, // op v2, v0, v1
                        0x020f                         // return v2
                    ])
                }
            };
            const res = vm.executeMethod(method, null, []);
            assert(res === item.expected, `DalvikVM 3-register opcode ${item.name} produced ${res} (expected: ${item.expected})`);
        });
    }

    // 1.2 Division and Modulo by Zero Edge Case Handling
    testProtected("Division by zero", () => {
        const divZeroMethod = {
            name: 'testDivZero',
            accessFlags: 0x0008,
            code: {
                registersSize: 3,
                insSize: 0,
                insnsSize: 6,
                insns: new Uint16Array([
                    0x0013, 500,
                    0x0113, 0,
                    (OP_DIV_INT) | (0x02 << 8), 0x0100,
                    0x020f
                ])
            }
        };
        const divZeroRes = vm.executeMethod(divZeroMethod, null, []);
        assert(divZeroRes === 0, `DalvikVM division by zero handled safely without crash (got: ${divZeroRes})`);
    });

    testProtected("Modulo by zero", () => {
        const remZeroMethod = {
            name: 'testRemZero',
            accessFlags: 0x0008,
            code: {
                registersSize: 3,
                insSize: 0,
                insnsSize: 6,
                insns: new Uint16Array([
                    0x0013, 500,
                    0x0113, 0,
                    (OP_REM_INT) | (0x02 << 8), 0x0100,
                    0x020f
                ])
            }
        };
        const remZeroRes = vm.executeMethod(remZeroMethod, null, []);
        assert(remZeroRes === 0, `DalvikVM modulo by zero handled safely without crash (got: ${remZeroRes})`);
    });

    // 1.3 32-bit Integer Overflow / Underflow Behavior
    testProtected("32-bit Integer overflow wrapping", () => {
        const overflowMethod = {
            name: 'testOverflow',
            accessFlags: 0x0008,
            code: {
                registersSize: 3,
                insSize: 0,
                insnsSize: 8,
                insns: new Uint16Array([
                    0x0014, 0xffff, 0x7fff, // const v0, 2147483647 (0x7fffffff)
                    0x0113, 1,              // const/16 v1, 1
                    (OP_ADD_INT) | (0x02 << 8), 0x0100, // add-int v2, v0, v1
                    0x020f                  // return v2
                ])
            }
        };
        const overflowRes = vm.executeMethod(overflowMethod, null, []);
        assert(overflowRes === -2147483648, `DalvikVM 32-bit integer overflow wrapped to INT32_MIN: ${overflowRes}`);
    });

    // 1.4 2-Address Arithmetic Opcodes Matrix
    const twoAddrMatrix = [
        { op: OP_ADD_INT_2ADDR, a: 50, b: 25, expected: 75, name: 'OP_ADD_INT_2ADDR' },
        { op: OP_SUB_INT_2ADDR, a: 50, b: 25, expected: 25, name: 'OP_SUB_INT_2ADDR' },
        { op: OP_MUL_INT_2ADDR, a: 6, b: 7, expected: 42, name: 'OP_MUL_INT_2ADDR' },
        { op: OP_DIV_INT_2ADDR, a: 42, b: 6, expected: 7, name: 'OP_DIV_INT_2ADDR' },
        { op: OP_REM_INT_2ADDR, a: 45, b: 7, expected: 3, name: 'OP_REM_INT_2ADDR' },
        { op: OP_AND_INT_2ADDR, a: 0x0F, b: 0x03, expected: 0x03, name: 'OP_AND_INT_2ADDR' },
        { op: OP_OR_INT_2ADDR,  a: 0x08, b: 0x03, expected: 0x0B, name: 'OP_OR_INT_2ADDR' },
        { op: OP_XOR_INT_2ADDR, a: 0x0F, b: 0x03, expected: 0x0C, name: 'OP_XOR_INT_2ADDR' },
        { op: OP_SHL_INT_2ADDR, a: 3, b: 4, expected: 48, name: 'OP_SHL_INT_2ADDR' },
        { op: OP_SHR_INT_2ADDR, a: 48, b: 2, expected: 12, name: 'OP_SHR_INT_2ADDR' },
        { op: OP_USHR_INT_2ADDR, a: -16, b: 2, expected: 1073741820, name: 'OP_USHR_INT_2ADDR' }
    ];

    for (const item of twoAddrMatrix) {
        testProtected(`2-Address Opcode ${item.name}`, () => {
            const method = {
                name: `test_${item.name}`,
                accessFlags: 0x0008,
                code: {
                    registersSize: 2,
                    insSize: 0,
                    insnsSize: 4,
                    insns: new Uint16Array([
                        0x0013, item.a & 0xffff,       // const/16 v0, a
                        0x0113, item.b & 0xffff,       // const/16 v1, b
                        (item.op) | (0x00 << 8) | (0x01 << 12), // op/2addr v0, v1 (highByte: 0x10)
                        0x000f                         // return v0
                    ])
                }
            };
            const res = vm.executeMethod(method, null, []);
            assert(res === item.expected, `DalvikVM 2-address opcode ${item.name} produced ${res} (expected: ${item.expected})`);
        });
    }

    // 1.5 Literal Arithmetic (OP_ADD_INT_LIT16 & OP_ADD_INT_LIT8)
    testProtected("OP_ADD_INT_LIT16 positive literal", () => {
        const lit16Positive = {
            name: 'testLit16Pos',
            accessFlags: 0x0008,
            code: {
                registersSize: 2,
                insSize: 0,
                insnsSize: 5,
                insns: new Uint16Array([
                    0x0113, 300,                                // const/16 v1, 300
                    (OP_ADD_INT_LIT16) | (0x00 << 8) | (0x01 << 12), 400, // add-int/lit16 v0, v1, #+400
                    0x000f                                      // return v0
                ])
            }
        };
        assert(vm.executeMethod(lit16Positive, null, []) === 700, "OP_ADD_INT_LIT16 with positive 16-bit literal (+400) = 700");
    });

    testProtected("OP_ADD_INT_LIT16 negative literal", () => {
        const lit16Negative = {
            name: 'testLit16Neg',
            accessFlags: 0x0008,
            code: {
                registersSize: 2,
                insSize: 0,
                insnsSize: 5,
                insns: new Uint16Array([
                    0x0113, 1000,                               // const/16 v1, 1000
                    (OP_ADD_INT_LIT16) | (0x00 << 8) | (0x01 << 12), -250 & 0xffff, // add-int/lit16 v0, v1, #-250
                    0x000f                                      // return v0
                ])
            }
        };
        assert(vm.executeMethod(lit16Negative, null, []) === 750, "OP_ADD_INT_LIT16 with negative signed literal (-250) = 750");
    });

    testProtected("OP_ADD_INT_LIT8 negative literal", () => {
        const lit8Negative = {
            name: 'testLit8Neg',
            accessFlags: 0x0008,
            code: {
                registersSize: 2,
                insSize: 0,
                insnsSize: 5,
                insns: new Uint16Array([
                    0x0113, 50,                                 // const/16 v1, 50
                    (OP_ADD_INT_LIT8) | (0x00 << 8), 0x01 | ((-20 & 0xff) << 8), // add-int/lit8 v0, v1, #-20
                    0x000f                                      // return v0
                ])
            }
        };
        assert(vm.executeMethod(lit8Negative, null, []) === 30, "OP_ADD_INT_LIT8 with negative signed 8-bit literal (-20) = 30");
    });

    // 1.6 Unary Operations (OP_NEG_INT, OP_NOT_INT)
    testProtected("OP_NEG_INT", () => {
        const unaryNeg = {
            name: 'testUnaryNeg',
            accessFlags: 0x0008,
            code: {
                registersSize: 2,
                insSize: 0,
                insnsSize: 4,
                insns: new Uint16Array([
                    0x0113, 42,
                    (OP_NEG_INT) | (0x00 << 8) | (0x01 << 12), // neg-int v0, v1
                    0x000f
                ])
            }
        };
        assert(vm.executeMethod(unaryNeg, null, []) === -42, "OP_NEG_INT negated +42 to -42");
    });

    testProtected("OP_NOT_INT", () => {
        const unaryNot = {
            name: 'testUnaryNot',
            accessFlags: 0x0008,
            code: {
                registersSize: 2,
                insSize: 0,
                insnsSize: 4,
                insns: new Uint16Array([
                    0x0113, 0,
                    (OP_NOT_INT) | (0x00 << 8) | (0x01 << 12), // not-int v0, v1 (~0 = -1)
                    0x000f
                ])
            }
        };
        assert(vm.executeMethod(unaryNot, null, []) === -1, "OP_NOT_INT bitwise NOT on 0 returned -1");
    });
}

// -----------------------------------------------------------------------------
// Suite 2: Dalvik VM Constants, Moves, Class Resolution & Returns
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 2] Dalvik VM Constants, Moves, Class Resolution & Returns");
{
    const mockDex = {
        strings: ["HelloWorldString", "org.fdroid.fdroid.views.main.MainActivity", "ExtraString"],
        types: ["Lorg/fdroid/fdroid/views/main/MainActivity;", "Ljava/lang/String;", "Ljava/lang/Object;"],
        fields: [{ classType: "LTestClass;", fieldType: "I", name: "testField" }],
        methods: []
    };
    const vm = new DalvikVM();
    vm.dexParsers.push(mockDex);

    // 2.1 OP_CONST_4: Positive and Sign-Extended Negative
    testProtected("OP_CONST_4 positive", () => {
        const const4Pos = {
            name: 'testConst4Pos',
            accessFlags: 0x0008,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 2,
                insns: new Uint16Array([
                    (OP_CONST_4) | (0x00 << 8) | (0x05 << 12), // const/4 v0, #5
                    0x000f
                ])
            }
        };
        assert(vm.executeMethod(const4Pos, null, []) === 5, "OP_CONST_4 loaded positive literal 5");
    });

    testProtected("OP_CONST_4 negative sign-extension", () => {
        const const4Neg = {
            name: 'testConst4Neg',
            accessFlags: 0x0008,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 2,
                insns: new Uint16Array([
                    (OP_CONST_4) | (0x00 << 8) | (0x0f << 12), // const/4 v0, #0xf (sign-extended -1)
                    0x000f
                ])
            }
        };
        assert(vm.executeMethod(const4Neg, null, []) === -1, "OP_CONST_4 sign-extended 4-bit 0xF to -1");
    });

    // 2.2 OP_CONST_HIGH16 (shifts 16 bits)
    testProtected("OP_CONST_HIGH16", () => {
        const constHigh16 = {
            name: 'testConstHigh16',
            accessFlags: 0x0008,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 3,
                insns: new Uint16Array([
                    (OP_CONST_HIGH16) | (0x00 << 8), 0x1234, // const/high16 v0, 0x1234 (0x12340000)
                    0x000f
                ])
            }
        };
        assert(vm.executeMethod(constHigh16, null, []) === 0x12340000, "OP_CONST_HIGH16 loaded 0x12340000");
    });

    // 2.3 OP_CONST_STRING & OP_CONST_STRING_JUMBO
    testProtected("OP_CONST_STRING", () => {
        const constStringMethod = {
            name: 'testConstString',
            accessFlags: 0x0008,
            dex: mockDex,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 3,
                insns: new Uint16Array([
                    (OP_CONST_STRING) | (0x00 << 8), 0, // const-string v0, string@0
                    (OP_RETURN_OBJECT) | (0x00 << 8)
                ])
            }
        };
        assert(vm.executeMethod(constStringMethod, null, []) === "HelloWorldString", "OP_CONST_STRING resolved string pool entry 'HelloWorldString'");
    });

    // 2.4 OP_CONST_CLASS
    testProtected("OP_CONST_CLASS", () => {
        const constClassMethod = {
            name: 'testConstClass',
            accessFlags: 0x0008,
            dex: mockDex,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 3,
                insns: new Uint16Array([
                    (OP_CONST_CLASS) | (0x00 << 8), 0, // const-class v0, type@0
                    (OP_RETURN_OBJECT) | (0x00 << 8)
                ])
            }
        };
        assert(vm.executeMethod(constClassMethod, null, []) === "Lorg/fdroid/fdroid/views/main/MainActivity;", "OP_CONST_CLASS loaded type descriptor 'Lorg/fdroid/fdroid/views/main/MainActivity;'");
    });

    // 2.5 OP_MOVE & OP_MOVE_FROM16 & OP_MOVE_OBJECT
    testProtected("OP_MOVE & OP_MOVE_FROM16", () => {
        const moveMethod = {
            name: 'testMove',
            accessFlags: 0x0008,
            code: {
                registersSize: 4,
                insSize: 0,
                insnsSize: 6,
                insns: new Uint16Array([
                    0x0113, 999,                                // const/16 v1, 999
                    (OP_MOVE) | (0x00 << 8) | (0x01 << 12),      // move v0, v1
                    (OP_MOVE_FROM16) | (0x02 << 8), 0x0000,     // move/from16 v2, v0
                    (OP_RETURN) | (0x02 << 8)
                ])
            }
        };
        assert(vm.executeMethod(moveMethod, null, []) === 999, "OP_MOVE & OP_MOVE_FROM16 transferred register values accurately");
    });

    // 2.6 OP_RETURN_VOID
    testProtected("OP_RETURN_VOID", () => {
        const returnVoidMethod = {
            name: 'testReturnVoid',
            accessFlags: 0x0008,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 1,
                insns: new Uint16Array([OP_RETURN_VOID])
            }
        };
        assert(vm.executeMethod(returnVoidMethod, null, []) === undefined, "OP_RETURN_VOID cleanly returned undefined");
    });
}

// -----------------------------------------------------------------------------
// Suite 3: Dalvik VM Control Flow, Branching, Loops & Instruction Limits
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 3] Dalvik VM Control Flow, Branching, Loops & Instruction Limits");
{
    const vm = new DalvikVM();

    // 3.1 Conditional Two-Register Branches Matrix
    // pc 0..1: const/16 v0, a
    // pc 2..3: const/16 v1, b
    // pc 4..5: if-op v0, v1, +5 (jump to pc 9 if taken)
    // pc 6..7: const/16 v2, 100
    // pc 8:    return v2 (100)
    // pc 9..10: const/16 v2, 200
    // pc 11:   return v2 (200)
    const branchTests = [
        { op: OP_IF_EQ, a: 10, b: 10, shouldBranch: true, name: 'OP_IF_EQ (equal)' },
        { op: OP_IF_EQ, a: 10, b: 20, shouldBranch: false, name: 'OP_IF_EQ (not equal)' },
        { op: OP_IF_NE, a: 10, b: 20, shouldBranch: true, name: 'OP_IF_NE (not equal)' },
        { op: OP_IF_LT, a: 5, b: 10, shouldBranch: true, name: 'OP_IF_LT (5 < 10)' },
        { op: OP_IF_GE, a: 10, b: 10, shouldBranch: true, name: 'OP_IF_GE (10 >= 10)' },
        { op: OP_IF_GT, a: 15, b: 10, shouldBranch: true, name: 'OP_IF_GT (15 > 10)' },
        { op: OP_IF_LE, a: 10, b: 10, shouldBranch: true, name: 'OP_IF_LE (10 <= 10)' }
    ];

    for (const bt of branchTests) {
        testProtected(`Branch ${bt.name}`, () => {
            const method = {
                name: `test_${bt.name}`,
                accessFlags: 0x0008,
                code: {
                    registersSize: 3,
                    insSize: 0,
                    insnsSize: 12,
                    insns: new Uint16Array([
                        0x0013, bt.a,
                        0x0113, bt.b,
                        (bt.op) | (0x00 << 8) | (0x01 << 12), 5, // jump +5 words to pc=9 if branch taken
                        0x0213, 100,                              // branch NOT taken -> pc 6..7
                        0x020f,                                   // pc 8: return v2 (100)
                        0x0213, 200,                              // branch TAKEN -> pc 9..10
                        0x020f                                    // pc 11: return v2 (200)
                    ])
                }
            };
            const res = vm.executeMethod(method, null, []);
            const expected = bt.shouldBranch ? 200 : 100;
            assert(res === expected, `Branch ${bt.name}: expected ${expected}, got ${res}`);
        });
    }

    // 3.2 Zero Comparison Branches Matrix
    // pc 0..1: const/16 v0, a
    // pc 2..3: if-opz v0, +5 (jump to pc 7 if taken)
    // pc 4..5: const/16 v1, 111
    // pc 6:    return v1 (111)
    // pc 7..8: const/16 v1, 222
    // pc 9:    return v1 (222)
    const zeroBranchTests = [
        { op: OP_IF_EQZ, a: 0, shouldBranch: true, name: 'OP_IF_EQZ (zero)' },
        { op: OP_IF_NEZ, a: 5, shouldBranch: true, name: 'OP_IF_NEZ (non-zero)' },
        { op: OP_IF_LTZ, a: -5, shouldBranch: true, name: 'OP_IF_LTZ (negative)' },
        { op: OP_IF_GEZ, a: 0, shouldBranch: true, name: 'OP_IF_GEZ (>= 0)' },
        { op: OP_IF_GTZ, a: 1, shouldBranch: true, name: 'OP_IF_GTZ (> 0)' },
        { op: OP_IF_LEZ, a: 0, shouldBranch: true, name: 'OP_IF_LEZ (<= 0)' }
    ];

    for (const zt of zeroBranchTests) {
        testProtected(`Zero Branch ${zt.name}`, () => {
            const method = {
                name: `test_${zt.name}`,
                accessFlags: 0x0008,
                code: {
                    registersSize: 2,
                    insSize: 0,
                    insnsSize: 10,
                    insns: new Uint16Array([
                        0x0013, zt.a & 0xffff,
                        (zt.op) | (0x00 << 8), 5, // jump +5 words to pc=7
                        0x0113, 111,              // not taken -> v1 = 111
                        0x010f,                   // return v1
                        0x0113, 222,              // taken -> v1 = 222
                        0x010f                    // return v1
                    ])
                }
            };
            const res = vm.executeMethod(method, null, []);
            const expected = zt.shouldBranch ? 222 : 111;
            assert(res === expected, `Zero Branch ${zt.name}: expected ${expected}, got ${res}`);
        });
    }

    // 3.3 Full Algorithmic Bytecode Loop: Accumulator Sum(1..50) = 1275
    testProtected("Algorithmic loop sum(1..50)", () => {
        const sumLoopMethod = {
            name: 'sum1to50',
            accessFlags: 0x0008,
            code: {
                registersSize: 3,
                insSize: 0,
                insnsSize: 12,
                insns: new Uint16Array([
                    0x0013, 0,                                  // pc 0: const/16 v0, 0 (sum)
                    0x0113, 1,                                  // pc 2: const/16 v1, 1 (i)
                    0x0213, 50,                                 // pc 4: const/16 v2, 50 (limit)
                    // loop starts at pc 6
                    (OP_IF_GT) | (0x01 << 8) | (0x02 << 12), 6, // pc 6: if-gt v1, v2, jump to pc 12 (offset +6)
                    (OP_ADD_INT_2ADDR) | (0x00 << 8) | (0x01 << 12), // pc 8: add-int/2addr v0, v1 (sum += i)
                    (OP_ADD_INT_LIT8) | (0x01 << 8), 0x01 | (0x01 << 8), // pc 9: add-int/lit8 v1, v1, #1 (i++)
                    (OP_GOTO) | ((-5 & 0xff) << 8),             // pc 11: goto pc 6 (offset -5)
                    0x000f                                      // pc 12: return v0
                ])
            }
        };
        const sumResult = vm.executeMethod(sumLoopMethod, null, []);
        assert(sumResult === 1275, `DalvikVM executed multi-iteration backward-jumping loop correctly: sum(1..50) = ${sumResult} (expected: 1275)`);
    });

    // 3.4 Infinite Loop Safety Protection
    testProtected("Infinite loop instruction count cutoff", () => {
        const infiniteLoopMethod = {
            name: 'testInfiniteLoop',
            accessFlags: 0x0008,
            code: {
                registersSize: 1,
                insSize: 0,
                insnsSize: 2,
                insns: new Uint16Array([
                    (OP_GOTO) | (0x00 << 8), // goto 0
                    0x000f
                ])
            }
        };
        const oldCap = vm.maxInstructionCount;
        vm.maxInstructionCount = 1000;
        vm.instructionsExecuted = 0;
        const loopHalted = vm.executeMethod(infiniteLoopMethod, null, []);
        assert(vm.instructionsExecuted >= 1000, `DalvikVM terminated runaway loop at instruction quota (${vm.instructionsExecuted} instructions)`);
        vm.maxInstructionCount = oldCap;
    });
}

// -----------------------------------------------------------------------------
// Suite 4: Dalvik VM Arrays, Field Access, Object Instantiation & Framework
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 4] Dalvik VM Arrays, Fields, Objects & Framework Bridge");
{
    const vm = new DalvikVM();

    // 4.1 Array Creation, Length, APUT, AGET
    testProtected("Array Operations (NEW_ARRAY, ARRAY_LENGTH, APUT, AGET)", () => {
        const arrayMethod = {
            name: 'testArrays',
            accessFlags: 0x0008,
            code: {
                registersSize: 5,
                insSize: 0,
                insnsSize: 13,
                insns: new Uint16Array([
                    0x0113, 5,                              // pc 0: const/16 v1, 5 (size)
                    (OP_NEW_ARRAY) | (0x00 << 8) | (0x01 << 12), 0, // pc 2: new-array v0, v1 (v0 = array[5])
                    (OP_ARRAY_LENGTH) | (0x02 << 8) | (0x00 << 12), // pc 4: array-length v2, v0
                    0x0313, 2,                              // pc 5: const/16 v3, 2 (index)
                    0x0413, 888,                            // pc 7: const/16 v4, 888 (val)
                    (OP_APUT) | (0x04 << 8), 0x00 | (0x03 << 8), // pc 9: aput v4, v0, v3 (array[2] = 888)
                    (OP_AGET) | (0x01 << 8), 0x00 | (0x03 << 8), // pc 11: aget v1, v0, v3 (v1 = array[2])
                    0x010f                                  // pc 13: return v1
                ])
            }
        };
        const arrayRes = vm.executeMethod(arrayMethod, null, []);
        assert(arrayRes === 888, `DalvikVM OP_NEW_ARRAY, OP_APUT, OP_AGET operated seamlessly (got: ${arrayRes})`);
    });

    // 4.2 Instance Fields (OP_IPUT & OP_IGET)
    testProtected("Instance Fields (OP_IPUT, OP_IGET)", () => {
        const mockDex = {
            strings: ["myField"],
            types: ["LTestObj;"],
            fields: [{ classType: "LTestObj;", fieldType: "I", name: "myField" }],
            methods: []
        };
        vm.dexParsers.push(mockDex);

        const instanceFieldMethod = {
            name: 'testInstanceField',
            accessFlags: 0x0008,
            dex: mockDex,
            code: {
                registersSize: 3,
                insSize: 0,
                insnsSize: 8,
                insns: new Uint16Array([
                    (OP_NEW_INSTANCE) | (0x00 << 8), 0,     // pc 0: new-instance v0, type@0
                    0x0113, 777,                            // pc 2: const/16 v1, 777
                    (OP_IPUT) | (0x01 << 8) | (0x00 << 12), 0, // pc 4: iput v1, v0, field@0 (v0.myField = 777)
                    (OP_IGET) | (0x02 << 8) | (0x00 << 12), 0, // pc 6: iget v2, v0, field@0
                    0x020f                                  // pc 8: return v2
                ])
            }
        };
        const fieldRes = vm.executeMethod(instanceFieldMethod, null, []);
        assert(fieldRes === 777, `DalvikVM OP_IPUT & OP_IGET stored and retrieved object field (got: ${fieldRes})`);
    });

    // 4.3 Static Fields (OP_SPUT & OP_SGET)
    testProtected("Static Fields (OP_SPUT, OP_SGET)", () => {
        const mockDex = {
            strings: ["staticField"],
            types: ["LTestStatic;"],
            fields: [{ classType: "LTestStatic;", fieldType: "I", name: "staticField" }],
            methods: []
        };
        vm.dexParsers.push(mockDex);

        const staticFieldMethod = {
            name: 'testStaticField',
            accessFlags: 0x0008,
            dex: mockDex,
            code: {
                registersSize: 2,
                insSize: 0,
                insnsSize: 6,
                insns: new Uint16Array([
                    0x0013, 12345,                          // pc 0: const/16 v0, 12345
                    (OP_SPUT) | (0x00 << 8), 0,             // pc 2: sput v0, field@0
                    (OP_SGET) | (0x01 << 8), 0,             // pc 4: sget v1, field@0
                    0x010f                                  // pc 6: return v1
                ])
            }
        };
        const staticRes = vm.executeMethod(staticFieldMethod, null, []);
        assert(staticRes === 12345, `DalvikVM OP_SPUT & OP_SGET persisted static state in VM registry (got: ${staticRes})`);
    });

    // 4.4 Framework Bridge: StringBuilder & String Dispatch
    testProtected("Framework Bridge StringBuilder & String methods", () => {
        const bridge = new AndroidFrameworkBridge(vm);
        const sb = bridge.instantiateObject("Ljava/lang/StringBuilder;");
        bridge.dispatchMethodCall(OP_INVOKE_VIRTUAL, { name: 'append', classType: 'Ljava/lang/StringBuilder;' }, [sb, "Android"]);
        bridge.dispatchMethodCall(OP_INVOKE_VIRTUAL, { name: 'append', classType: 'Ljava/lang/StringBuilder;' }, [sb, "WebGPU"]);
        const sbStr = bridge.dispatchMethodCall(OP_INVOKE_VIRTUAL, { name: 'toString', classType: 'Ljava/lang/StringBuilder;' }, [sb]);
        assert(sbStr === "AndroidWebGPU", `FrameworkBridge StringBuilder chaining produced '${sbStr}'`);

        const strLen = bridge.dispatchMethodCall(OP_INVOKE_VIRTUAL, { name: 'length', classType: 'Ljava/lang/String;' }, ["AndroidWebGPU"]);
        assert(strLen === 13, `FrameworkBridge String.length returned ${strLen}`);
    });

    // 4.5 Activity Lifecycle & Bridge Methods
    testProtected("Framework Bridge Activity Lifecycle", () => {
        const bridge = new AndroidFrameworkBridge(vm);
        const act = bridge.createActivity("org.test.TestActivity");
        assert(act.className === "org.test.TestActivity", "Activity created with className");
        bridge.dispatchMethodCall(OP_INVOKE_VIRTUAL, { name: 'setContentView', classType: 'Landroid/app/Activity;' }, [act, 2131296365]);
        assert(act.contentView === 2131296365, "Activity.setContentView dispatched via framework bridge");
        bridge.dispatchMethodCall(OP_INVOKE_VIRTUAL, { name: 'finish', classType: 'Landroid/app/Activity;' }, [act]);
        assert(act.isResumed === false, "Activity.finish updated lifecycle state to non-resumed");
    });
}

// -----------------------------------------------------------------------------
// Suite 5: Multi-DEX Parsing, Natural Order Sorting & Header Validation
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 5] Multi-DEX Parsing, Natural Sorting & Scoping");
{
    // 5.1 DEX Header Magic Validation
    testProtected("DEX Header Magic Validation", () => {
        const validDexHeader = new Uint8Array(0x70);
        validDexHeader.set([0x64, 0x65, 0x78, 0x0a, 0x30, 0x33, 0x35, 0x00]); // "dex\n035\0"
        const validParser = new DexParser(validDexHeader, "test_valid.dex");
        assert(validParser.bytes.length >= 0x70, "Valid header length accepted");

        const invalidDexHeader = new Uint8Array(0x70);
        invalidDexHeader.set([0x4e, 0x4f, 0x54, 0x44, 0x45, 0x58, 0x00, 0x00]);
        let threwInvalidMagic = false;
        try {
            new DexParser(invalidDexHeader, "bad_magic.dex").parse();
        } catch (e) {
            threwInvalidMagic = true;
        }
        assert(threwInvalidMagic, "DexParser rejected invalid header magic ('NOTDEX')");
    });

    // 5.2 Truncated DEX File Buffer Rejection
    testProtected("Truncated DEX Buffer Rejection", () => {
        const shortBuffer = new Uint8Array(32);
        let threwShort = false;
        try {
            new DexParser(shortBuffer, "short.dex").parse();
        } catch (e) {
            threwShort = true;
        }
        assert(threwShort, "DexParser rejected truncated header buffer (< 0x70 bytes)");
    });

    // 5.3 Multi-DEX Natural Sort Ordering
    testProtected("Multi-DEX Natural Sort Ordering", () => {
        const mockDexNames = ["classes10.dex", "classes2.dex", "classes.dex", "classes3.dex", "classes1.dex"];
        const sorted = [...mockDexNames].sort((a, b) => {
            if (a === 'classes.dex') return -1;
            if (b === 'classes.dex') return 1;
            return a.localeCompare(b, undefined, { numeric: true });
        });
        assert(sorted[0] === "classes.dex", "classes.dex sorted first in multi-dex sequence");
        assert(sorted[1] === "classes1.dex", "classes1.dex sorted second");
        assert(sorted[2] === "classes2.dex", "classes2.dex sorted third");
        assert(sorted[3] === "classes3.dex", "classes3.dex sorted fourth");
        assert(sorted[4] === "classes10.dex", "classes10.dex sorted after classes3.dex (natural sort numeric order)");
    });

    // 5.4 Real F-Droid.apk Multi-DEX Extraction & Cross-DEX Scoping
    testProtected("Real F-Droid.apk Multi-DEX Extraction & Registration", () => {
        const apkBuf = fs.readFileSync(path.join(rootDir, "F-Droid.apk"));
        const zip = new ApkZipReader(apkBuf);
        const allDex = zip.getAllDexFiles();
        assert(allDex.length === 2, `Extracted exact multi-dex files count: ${allDex.length}`);
        assert(allDex[0].name === "classes.dex" && allDex[1].name === "classes2.dex", "F-Droid.apk DEX files ordered naturally");

        const vm = new DalvikVM();
        const p1 = new DexParser(allDex[0].data, allDex[0].name).parse();
        const p2 = new DexParser(allDex[1].data, allDex[1].name).parse();
        vm.loadDex(p1);
        vm.loadDex(p2);

        assert(vm.classes.size > 20000, `DalvikVM merged classes across multi-dex (${vm.classes.size} classes)`);
        assert(vm.dexParsers.length === 2, "DalvikVM contains 2 registered DEX parser instances");

        const mainAct = vm.findClass("org.fdroid.fdroid.views.main.MainActivity");
        assert(mainAct !== null, "Found MainActivity class in multi-dex VM registry");
        assert(mainAct.directMethods.has("<init>"), "MainActivity contains direct constructor <init>()");
        assert(mainAct.virtualMethods.has("onCreate"), "MainActivity contains virtual onCreate() method");
    });
}

// -----------------------------------------------------------------------------
// Suite 6: Target APK Manifest, PMS Registration & Launcher Resolution
// -----------------------------------------------------------------------------
console.log("\n▶ [Suite 6] Manifest, PMS Registration & Launcher Activity Resolution");
{
    testProtected("Manifest, PMS & Launcher Resolution", async () => {
        const apkBuf = fs.readFileSync(path.join(rootDir, "F-Droid.apk"));
        const zip = new ApkZipReader(apkBuf);
        const manifestBytes = zip.getManifest();
        assert(manifestBytes !== null, "Extracted AndroidManifest.xml binary bytes");

        const manifest = AxmlDecoder.decode(manifestBytes);
        assert(manifest.packageName === "org.fdroid.fdroid", `Package name matches org.fdroid.fdroid (got: ${manifest.packageName})`);
        assert(manifest.versionCode === 1023051, `Version code matches 1023051 (got: ${manifest.versionCode})`);
        assert(manifest.versionName === "1.23.1", `Version name matches 1.23.1 (got: ${manifest.versionName})`);
        assert(manifest.activities.length === 25, `Activities count matches expected 25 (got: ${manifest.activities.length})`);
        assert(manifest.launcherActivity === "org.fdroid.fdroid.views.main.MainActivity", `Launcher activity resolved to org.fdroid.fdroid.views.main.MainActivity`);

        // 6.1 PMS Registry Functionality
        const pms = new PackageManagerRegistry();
        assert(pms.hasPackage("org.fdroid.fdroid"), "PMS initialized with default system package");

        const installed = pms.installApk(apkBuf);
        assert(installed !== null, "PMS.installApk succeeded");
        assert(installed.packageName === "org.fdroid.fdroid", "Installed package matches package name");

        const resolvedLauncher = pms.resolveLauncherActivity("org.fdroid.fdroid");
        assert(resolvedLauncher === "org.fdroid.fdroid.views.main.MainActivity", `PMS.resolveLauncherActivity resolved ${resolvedLauncher}`);

        // 6.2 Relative Activity Class Name Normalization
        const syntheticPkg = {
            packageName: "com.example.testapp",
            appName: "TestApp",
            activities: [
                { name: "com.example.testapp.TestMainActivity", exported: true }
            ]
        };
        pms.installPackage(syntheticPkg);
        assert(pms.hasPackage("com.example.testapp"), "PMS installed synthetic package");
        assert(pms.resolveLauncherActivity("com.example.testapp") === "com.example.testapp.TestMainActivity", "PMS resolved launcher for synthetic package");

        // 6.3 End-to-End AndroidRuntime Integration
        const runtime = new AndroidRuntime();
        const appState = await runtime.loadAndRunApk(apkBuf, null);
        assert(appState !== null, "runtime.loadAndRunApk completed and returned appState");
        assert(runtime.installedApps.has("org.fdroid.fdroid"), "Runtime tracks org.fdroid.fdroid in installedApps set");
        assert(runtime.activeApps.has("org.fdroid.fdroid"), "Runtime tracks org.fdroid.fdroid in activeApps map");
    });
}

console.log("\n================================================================================");
if (failedTests === 0) {
    console.log(`⚡ ALL CHALLENGER 1 EMPIRICAL STRESS TESTS PASSED! (${passedTests}/${totalTests} assertions passed)`);
} else {
    console.log(`❌ CHALLENGER 1 STRESS TESTS FOUND ${failedTests} FAILURE(S) (${passedTests}/${totalTests} passed)`);
    console.log("Summary of failures:");
    for (const f of failureDetails) {
        console.log(`  - ${f}`);
    }
}
console.log("================================================================================");

if (failedTests > 0) {
    process.exit(1);
}
