/**
 * Milestone M1 Empirical Challenger Test Suite:
 * Stress-testing src/apk_resource_resolver.js (ArscDecoder, TypedValue, ResTable_config, reference chains, dimen units)
 * 
 * Probes:
 * 1. TypedValue Radix, Mantissa, Unit Scaling & Boundary Invariants (Float32, IEEE 754, Density, Radix math)
 * 2. Binary ARSC Parsing & Malformed/Corrupt Header Fuzzing (10,000 fuzz cycles, boundary sizes, corrupted chunks)
 * 3. Real F-Droid.apk ARSC Resolution, Locale Matching & Density Selection Matrix
 * 4. Complex Reference Chains, Cycles, Style Bags & Identifier Syntax Parsing
 * 5. High-Throughput Resolution & Stress Stability
 * 
 * Conforms to ASD-STE100, /ponytail, and /caveman.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { ApkZipReader } from '../src/apk_client_parser.js';
import {
    ArscDecoder,
    ArscResourceTable,
    TypedValue,
    RES_NULL_TYPE,
    RES_STRING_POOL_TYPE,
    RES_TABLE_TYPE,
    RES_TABLE_PACKAGE_TYPE,
    RES_TABLE_TYPE_TYPE,
    TYPE_NULL,
    TYPE_REFERENCE,
    TYPE_ATTRIBUTE,
    TYPE_STRING,
    TYPE_FLOAT,
    TYPE_DIMENSION,
    TYPE_FRACTION,
    TYPE_DYNAMIC_REFERENCE,
    TYPE_DYNAMIC_ATTRIBUTE,
    TYPE_INT_DEC,
    TYPE_INT_HEX,
    TYPE_INT_BOOLEAN,
    TYPE_FIRST_COLOR_INT,
    TYPE_INT_COLOR_ARGB8,
    TYPE_INT_COLOR_RGB8,
    TYPE_INT_COLOR_ARGB4,
    TYPE_INT_COLOR_RGB4,
    TYPE_LAST_COLOR_INT,
    COMPLEX_UNIT_PX,
    COMPLEX_UNIT_DIP,
    COMPLEX_UNIT_SP,
    COMPLEX_UNIT_PT,
    COMPLEX_UNIT_IN,
    COMPLEX_UNIT_MM,
    COMPLEX_RADIX_SHIFT,
    COMPLEX_RADIX_MASK,
    COMPLEX_MANTISSA_SHIFT
} from '../src/apk_resource_resolver.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

let totalChecks = 0;
let passedChecks = 0;
let failedChecks = 0;

function assertCheck(name, condition, extraInfo = '') {
    totalChecks++;
    if (condition) {
        passedChecks++;
    } else {
        failedChecks++;
        console.error(`  ✖ [FAIL] ${name} ${extraInfo}`);
        throw new Error(`Assertion failed: ${name} ${extraInfo}`);
    }
}

async function runProbe(title, fn) {
    console.log(`\n================================================================================`);
    console.log(`▶ [PROBE] ${title}`);
    console.log(`================================================================================`);
    const start = performance.now();
    await fn();
    const elapsed = (performance.now() - start).toFixed(2);
    console.log(`✔ [PASS] ${title} (${elapsed}ms)`);
}

async function main() {
    console.log("⚡ STARTING MILESTONE M1 EMPIRICAL CHALLENGER TEST SUITE\n");

    const apkPath = path.join(ROOT_DIR, 'F-Droid.apk');
    const apkBytes = fs.readFileSync(apkPath);
    const apkZip = new ApkZipReader(apkBytes);
    const arscBytes = apkZip.readFile('resources.arsc');

    // =========================================================================
    // PROBE 1: TypedValue Radix, Mantissa, Unit Scaling & Boundary Invariants
    // =========================================================================
    await runProbe("1. TypedValue Radix, Mantissa, Unit Scaling & Boundary Invariants", async () => {
        function makeComplex(mantissa, radix, unit) {
            const m = (mantissa & 0xFFFFFF) << COMPLEX_MANTISSA_SHIFT;
            const r = (radix & COMPLEX_RADIX_MASK) << COMPLEX_RADIX_SHIFT;
            const u = unit & 0x0F;
            return (m | r | u) | 0;
        }

        // 1.1 Test all 4 radices with positive, negative, zero, and fractional values
        // Radix 0: 23p0 (mantissa * 1.0)
        assertCheck("Radix 0 pos int (100)", TypedValue.complexToFloat(makeComplex(100, 0, COMPLEX_UNIT_PX)) === 100);
        assertCheck("Radix 0 neg int (-100)", TypedValue.complexToFloat(makeComplex(-100, 0, COMPLEX_UNIT_PX)) === -100);
        assertCheck("Radix 0 zero", TypedValue.complexToFloat(makeComplex(0, 0, COMPLEX_UNIT_PX)) === 0);
        assertCheck("Radix 0 max mantissa (8388607)", TypedValue.complexToFloat(makeComplex(8388607, 0, COMPLEX_UNIT_PX)) === 8388607);
        assertCheck("Radix 0 min mantissa (-8388608)", TypedValue.complexToFloat(makeComplex(-8388608, 0, COMPLEX_UNIT_PX)) === -8388608);

        // Radix 1: 16p7 (mantissa * 1/128)
        assertCheck("Radix 1 pos (1.5)", Math.abs(TypedValue.complexToFloat(makeComplex(192, 1, COMPLEX_UNIT_DIP)) - 1.5) < 1e-6);
        assertCheck("Radix 1 neg (-1.5)", Math.abs(TypedValue.complexToFloat(makeComplex(-192, 1, COMPLEX_UNIT_DIP)) - (-1.5)) < 1e-6);
        assertCheck("Radix 1 small step (1/128)", Math.abs(TypedValue.complexToFloat(makeComplex(1, 1, COMPLEX_UNIT_DIP)) - (1 / 128)) < 1e-6);

        // Radix 2: 8p15 (mantissa * 1/32768)
        assertCheck("Radix 2 pos (0.5)", Math.abs(TypedValue.complexToFloat(makeComplex(16384, 2, COMPLEX_UNIT_SP)) - 0.5) < 1e-6);
        assertCheck("Radix 2 neg (-0.25)", Math.abs(TypedValue.complexToFloat(makeComplex(-8192, 2, COMPLEX_UNIT_SP)) - (-0.25)) < 1e-6);
        assertCheck("Radix 2 step (1/32768)", Math.abs(TypedValue.complexToFloat(makeComplex(1, 2, COMPLEX_UNIT_SP)) - (1 / 32768)) < 1e-7);

        // Radix 3: 0p23 (mantissa * 1/8388608)
        assertCheck("Radix 3 pos (0.5)", Math.abs(TypedValue.complexToFloat(makeComplex(4194304, 3, COMPLEX_UNIT_PT)) - 0.5) < 1e-6);
        assertCheck("Radix 3 neg (-0.125)", Math.abs(TypedValue.complexToFloat(makeComplex(-1048576, 3, COMPLEX_UNIT_PT)) - (-0.125)) < 1e-6);
        assertCheck("Radix 3 step (1/8388608)", Math.abs(TypedValue.complexToFloat(makeComplex(1, 3, COMPLEX_UNIT_PT)) - (1 / 8388608)) < 1e-8);

        // 1.2 Unit scaling invariants across densities
        const densities = [0.75, 1.0, 1.33, 1.5, 2.0, 2.625, 3.0, 3.5, 4.0];
        for (const d of densities) {
            // PX: independent of density
            const pxWord = makeComplex(50, 0, COMPLEX_UNIT_PX);
            assertCheck(`PX at density ${d}`, TypedValue.complexToDimension(pxWord, d) === 50);

            // DP: val * d
            const dpWord = makeComplex(24, 0, COMPLEX_UNIT_DIP);
            assertCheck(`DP at density ${d}`, Math.abs(TypedValue.complexToDimension(dpWord, d) - (24 * d)) < 1e-5);

            // SP: val * d
            const spWord = makeComplex(14, 0, COMPLEX_UNIT_SP);
            assertCheck(`SP at density ${d}`, Math.abs(TypedValue.complexToDimension(spWord, d) - (14 * d)) < 1e-5);

            // PT: val * d * (160 / 72)
            const ptWord = makeComplex(72, 0, COMPLEX_UNIT_PT);
            assertCheck(`PT at density ${d}`, Math.abs(TypedValue.complexToDimension(ptWord, d) - (72 * d * (160 / 72))) < 1e-5);

            // IN: val * d * 160
            const inWord = makeComplex(2, 0, COMPLEX_UNIT_IN);
            assertCheck(`IN at density ${d}`, Math.abs(TypedValue.complexToDimension(inWord, d) - (2 * d * 160)) < 1e-5);

            // MM: val * d * (160 / 25.4)
            const mmWord = makeComplex(254, 0, COMPLEX_UNIT_MM);
            assertCheck(`MM at density ${d}`, Math.abs(TypedValue.complexToDimension(mmWord, d) - (254 * d * (160 / 25.4))) < 1e-4);
        }

        // 1.3 complexToDimensionPixelSize & PixelOffset edge cases
        const tinyPos = makeComplex(1, 3, COMPLEX_UNIT_PX); // 1/8388608 px
        assertCheck("PixelSize tiny positive rounds to 1", TypedValue.complexToDimensionPixelSize(tinyPos, 1.0) === 1);

        const tinyNeg = makeComplex(-1, 3, COMPLEX_UNIT_PX); // -1/8388608 px
        assertCheck("PixelSize tiny negative rounds to -1", TypedValue.complexToDimensionPixelSize(tinyNeg, 1.0) === -1);

        const zeroWord = makeComplex(0, 0, COMPLEX_UNIT_PX);
        assertCheck("PixelSize exact zero returns 0", TypedValue.complexToDimensionPixelSize(zeroWord, 1.0) === 0);

        const normalPos = makeComplex(10, 0, COMPLEX_UNIT_DIP);
        assertCheck("PixelSize normal 10dp at 1.5x -> 15", TypedValue.complexToDimensionPixelSize(normalPos, 1.5) === 15);

        const fracOffset = makeComplex(192, 1, COMPLEX_UNIT_DIP); // 1.5dp * 1.5 = 2.25px -> trunc to 2
        assertCheck("PixelOffset truncates 2.25 to 2", TypedValue.complexToDimensionPixelOffset(fracOffset, 1.5) === 2);

        // 1.4 Color decoding edge cases
        assertCheck("ARGB8 full opaque black", TypedValue.decodeColor(0xFF000000, TYPE_INT_COLOR_ARGB8) === '#000000');
        assertCheck("ARGB8 full opaque white", TypedValue.decodeColor(0xFFFFFFFF, TYPE_INT_COLOR_ARGB8) === '#ffffff');
        assertCheck("ARGB8 transparent black", TypedValue.decodeColor(0x00000000, TYPE_INT_COLOR_ARGB8) === '#00000000');
        assertCheck("ARGB8 alpha 0x01", TypedValue.decodeColor(0x01112233, TYPE_INT_COLOR_ARGB8) === '#01112233');
        assertCheck("RGB8 color", TypedValue.decodeColor(0x00AABBCC, TYPE_INT_COLOR_RGB8) === '#aabbcc');
        assertCheck("ARGB4 color opaque", TypedValue.decodeColor(0xFABC, TYPE_INT_COLOR_ARGB4) === '#aabbcc');
        assertCheck("ARGB4 color translucent", TypedValue.decodeColor(0x8ABC, TYPE_INT_COLOR_ARGB4) === '#88aabbcc');
        assertCheck("RGB4 color", TypedValue.decodeColor(0x0ABC, TYPE_INT_COLOR_RGB4) === '#aabbcc');

        // 1.5 Float32 IEEE 754 decoding
        function floatToDword(val) {
            const buf = new ArrayBuffer(4);
            new Float32Array(buf)[0] = val;
            return new Uint32Array(buf)[0];
        }

        const floatPi = floatToDword(Math.PI);
        const decodedPi = TypedValue.decodeValue(TYPE_FLOAT, floatPi);
        assertCheck("Float32 PI decoding", Math.abs(decodedPi - Math.fround(Math.PI)) < 1e-7);

        const floatNegZero = floatToDword(-0.0);
        const decodedNegZero = TypedValue.decodeValue(TYPE_FLOAT, floatNegZero);
        assertCheck("Float32 -0.0 decoding", Object.is(decodedNegZero, -0));

        const floatInfinity = floatToDword(Infinity);
        assertCheck("Float32 +Infinity decoding", TypedValue.decodeValue(TYPE_FLOAT, floatInfinity) === Infinity);

        const floatNegInfinity = floatToDword(-Infinity);
        assertCheck("Float32 -Infinity decoding", TypedValue.decodeValue(TYPE_FLOAT, floatNegInfinity) === -Infinity);

        const floatNaN = floatToDword(NaN);
        assertCheck("Float32 NaN decoding", Number.isNaN(TypedValue.decodeValue(TYPE_FLOAT, floatNaN)));

        // 1.6 Signed Integer & Special Constants
        assertCheck("Signed int -1 (MATCH_PARENT)", TypedValue.decodeValue(TYPE_INT_DEC, 0xFFFFFFFF) === -1);
        assertCheck("Signed int -2 (WRAP_CONTENT)", TypedValue.decodeValue(TYPE_INT_DEC, 0xFFFFFFFE) === -2);
        assertCheck("Signed int -2147483648", TypedValue.decodeValue(TYPE_INT_DEC, 0x80000000) === -2147483648);
        assertCheck("Signed int 2147483647", TypedValue.decodeValue(TYPE_INT_DEC, 0x7FFFFFFF) === 2147483647);
        assertCheck("Hex uint 0x80000000", TypedValue.decodeValue(TYPE_INT_HEX, 0x80000000) === 2147483648);
        assertCheck("Boolean true", TypedValue.decodeValue(TYPE_INT_BOOLEAN, 1) === true);
        assertCheck("Boolean false", TypedValue.decodeValue(TYPE_INT_BOOLEAN, 0) === false);
        assertCheck("Boolean non-zero -> true", TypedValue.decodeValue(TYPE_INT_BOOLEAN, 0xFF) === true);
        assertCheck("Null type -> null", TypedValue.decodeValue(TYPE_NULL, 0) === null);
    });

    // =========================================================================
    // PROBE 2: Binary ARSC Parsing & Corrupt/Malformed Header Fuzzing
    // =========================================================================
    await runProbe("2. Binary ARSC Parsing & Corrupt/Malformed Header Fuzzing", async () => {
        // 2.1 Non-buffer & primitive inputs
        const invalidInputs = [
            null, undefined, 0, 1, -1, 3.14, NaN, Infinity, "", "ARSC", true, false,
            {}, { length: 100 }, [], [0x02, 0x00], () => {}, new Date()
        ];
        for (const input of invalidInputs) {
            const tbl = ArscDecoder.decode(input);
            assertCheck(`Reject invalid input ${typeof input}`, tbl instanceof ArscResourceTable);
            assertCheck(`Empty table string resolve returns null`, tbl.resolveString(0x7f010001) === null);
        }

        // 2.2 Truncated buffer sizes (0 to 11 bytes)
        for (let sz = 0; sz < 12; sz++) {
            const buf = new Uint8Array(sz);
            const tbl = ArscDecoder.decode(buf);
            assertCheck(`Truncated size ${sz} returns safe empty table`, tbl instanceof ArscResourceTable);
        }

        // 2.3 Wrong fileType (not 0x0002)
        const wrongTypeBuf = new Uint8Array(64);
        const wrongView = new DataView(wrongTypeBuf.buffer);
        wrongView.setUint16(0, 0x0003, true);
        wrongView.setUint16(2, 12, true);
        wrongView.setUint32(4, 64, true);
        const wrongTbl = ArscDecoder.decode(wrongTypeBuf);
        assertCheck("Wrong fileType returns safe empty table", wrongTbl instanceof ArscResourceTable);

        // 2.4 Corrupted headerSize and totalSize
        const badSizes = [
            { headerSize: 0, totalSize: 64 },
            { headerSize: 65, totalSize: 64 },
            { headerSize: 12, totalSize: 4 },
            { headerSize: 12, totalSize: 0xFFFFFFFF },
            { headerSize: 0xFFFF, totalSize: 0xFFFF }
        ];
        for (const tc of badSizes) {
            const buf = new Uint8Array(128);
            const v = new DataView(buf.buffer);
            v.setUint16(0, RES_TABLE_TYPE, true);
            v.setUint16(2, tc.headerSize, true);
            v.setUint32(4, tc.totalSize, true);
            const tbl = ArscDecoder.decode(buf);
            assertCheck(`Bad size {h:${tc.headerSize}, t:${tc.totalSize}} handled safely`, tbl instanceof ArscResourceTable);
        }

        // 2.5 10,000 Fuzz Mutations on Real resources.arsc Header & Chunks
        const sampleArsc = arscBytes.subarray(0, 4096);
        for (let i = 0; i < 10000; i++) {
            const fuzzed = new Uint8Array(sampleArsc.length);
            fuzzed.set(sampleArsc);

            const numMutations = 1 + Math.floor(Math.random() * 10);
            for (let m = 0; m < numMutations; m++) {
                const off = Math.floor(Math.random() * fuzzed.length);
                fuzzed[off] = Math.floor(Math.random() * 256);
            }

            let threw = false;
            let res = null;
            try {
                res = ArscDecoder.decode(fuzzed);
            } catch (err) {
                threw = true;
                console.error(`Fuzz crash at iteration ${i}:`, err);
            }
            assertCheck(`Fuzz iteration ${i} no crash`, !threw);
            assertCheck(`Fuzz iteration ${i} returned ArscResourceTable`, res instanceof ArscResourceTable);
        }
    });

    // =========================================================================
    // PROBE 3: Real F-Droid.apk ARSC Resolution, Locale Matching & Density Selection
    // =========================================================================
    await runProbe("3. Real F-Droid.apk ARSC Resolution, Locale Matching & Density Selection", async () => {
        const tableDefault = ArscDecoder.decode(arscBytes);
        assertCheck("Default table decodes F-Droid resources", tableDefault.packages.has(0x7f));

        const pkg = tableDefault.packages.get(0x7f);
        assertCheck("Package name is org.fdroid.fdroid", pkg.name === 'org.fdroid.fdroid');
        assertCheck("Type strings count > 10", pkg.typeStrings.length > 10);
        assertCheck("Key strings count > 500", pkg.keyStrings.length > 500);

        // 3.1 All 197 Layouts Resolution
        const layoutEntries = tableDefault.getAllEntries('layout');
        assertCheck("All 197 layouts discovered in resources.arsc", layoutEntries.length === 197);

        let validLayoutPaths = 0;
        for (const entry of layoutEntries) {
            const pathRes = tableDefault.resolveLayoutPath(entry.resId);
            if (pathRes && pathRes.startsWith('res/') && pathRes.endsWith('.xml')) {
                validLayoutPaths++;
            }
        }
        assertCheck("197/197 layout paths resolve to res/*.xml", validLayoutPaths === 197);

        // 3.2 Locale-specific string resolution matrix
        const locales = ['de', 'fr', 'es', 'zh', 'ru', 'ja', 'ar', 'pt-BR', 'zh-CN', 'en-US'];
        for (const loc of locales) {
            const locTable = ArscDecoder.decode(arscBytes, loc);
            const str = locTable.resolveString(0x7f120075, loc); // app_name
            assertCheck(`Locale '${loc}' resolves app_name`, typeof str === 'string' && str.length > 0);
        }

        // Test English vs German vs French vs Spanish specific strings on 0x7f120000 (SignatureMismatch)
        const mismatchEn = ArscDecoder.decode(arscBytes, 'en').resolveString(0x7f120000, 'en');
        const mismatchDe = ArscDecoder.decode(arscBytes, 'de').resolveString(0x7f120000, 'de');
        const mismatchEs = ArscDecoder.decode(arscBytes, 'es').resolveString(0x7f120000, 'es');
        const mismatchFr = ArscDecoder.decode(arscBytes, 'fr').resolveString(0x7f120000, 'fr');

        assertCheck("En string matches English text", mismatchEn.includes('The new version is signed'));
        assertCheck("De string matches German text", mismatchDe.includes('Die neue Version wurde'));
        assertCheck("Es string matches Spanish text", mismatchEs.includes('La nueva versión está firmada'));
        assertCheck("Fr string matches French text", mismatchFr.includes('La nouvelle version est signée'));

        // 3.3 Density configuration selection
        const densities = [120, 160, 240, 320, 480, 640];
        for (const dens of densities) {
            const densTable = ArscDecoder.decode(arscBytes, '', dens);
            assertCheck(`Density ${dens} table initialized`, densTable instanceof ArscResourceTable);
        }
    });

    // =========================================================================
    // PROBE 4: Complex Reference Chains, Cycles, Style Bags & Identifier Syntax
    // =========================================================================
    await runProbe("4. Complex Reference Chains, Cycles, Style Bags & Identifier Syntax", async () => {
        const table = ArscDecoder.decode(arscBytes);

        // 4.1 Identifier Syntax Parsing Matrix
        const testSyntax = [
            { query: '@string/app_name', expectedType: 'number' },
            { query: '@layout/activity_main', expected: 0x7f0c0020 },
            { query: '@+id/icon', expected: 0x7f09013e },
            { query: '@id/icon', expected: 0x7f09013e },
            { query: '@0x7f0c0020', expected: 0x7f0c0020 },
            { query: '@0X7F0C0020', expected: 0x7f0c0020 },
            { query: '0x7f0c0020', expected: 0x7f0c0020 },
            { query: '@org.fdroid.fdroid:layout/activity_main', expected: 0x7f0c0020 }
        ];

        for (const tc of testSyntax) {
            const resolved = table.resolveIdentifierRef(tc.query);
            if (tc.expected !== undefined) {
                assertCheck(`Identifier ref syntax '${tc.query}' -> 0x${tc.expected.toString(16)}`, resolved === tc.expected);
            } else if (tc.expectedType) {
                assertCheck(`Identifier ref syntax '${tc.query}' returns ${tc.expectedType}`, typeof resolved === tc.expectedType);
            }
        }

        // 4.2 Synthetic Circular Reference Resolution (Depth Guard Testing)
        const syntheticGlobalStrings = ['String A', 'String B', 'Resolved Success!'];
        const syntheticPackages = new Map();
        const pkgTypes = new Map();

        // type 1 (string)
        const stringEntries = new Map();
        stringEntries.set(0, [{
            key: 'circ_a',
            typeId: 1,
            typeName: 'string',
            config: { lang: '', country: '', density: 0 },
            isComplex: false,
            dataType: TYPE_REFERENCE,
            data: 0x7f010001,
            val: 0x7f010001
        }]);
        stringEntries.set(1, [{
            key: 'circ_b',
            typeId: 1,
            typeName: 'string',
            config: { lang: '', country: '', density: 0 },
            isComplex: false,
            dataType: TYPE_REFERENCE,
            data: 0x7f010000,
            val: 0x7f010000
        }]);
        stringEntries.set(2, [{
            key: 'hop_1',
            typeId: 1,
            typeName: 'string',
            config: { lang: '', country: '', density: 0 },
            isComplex: false,
            dataType: TYPE_REFERENCE,
            data: 0x7f010003,
            val: 0x7f010003
        }]);
        stringEntries.set(3, [{
            key: 'hop_2',
            typeId: 1,
            typeName: 'string',
            config: { lang: '', country: '', density: 0 },
            isComplex: false,
            dataType: TYPE_STRING,
            data: 2,
            val: 'Resolved Success!'
        }]);
        pkgTypes.set(1, { id: 1, name: 'string', entries: stringEntries });

        // type 2 (dimen)
        const dimenEntries = new Map();
        dimenEntries.set(0, [{
            key: 'circ_dim_a',
            typeId: 2,
            typeName: 'dimen',
            config: { lang: '', country: '', density: 0 },
            isComplex: false,
            dataType: TYPE_REFERENCE,
            data: 0x7f020001,
            val: 0x7f020001
        }]);
        dimenEntries.set(1, [{
            key: 'circ_dim_b',
            typeId: 2,
            typeName: 'dimen',
            config: { lang: '', country: '', density: 0 },
            isComplex: false,
            dataType: TYPE_REFERENCE,
            data: 0x7f020000,
            val: 0x7f020000
        }]);
        pkgTypes.set(2, { id: 2, name: 'dimen', entries: dimenEntries });

        syntheticPackages.set(0x7f, {
            id: 0x7f,
            name: 'com.test.app',
            typeStrings: ['string', 'dimen'],
            keyStrings: ['circ_a', 'circ_b', 'hop_1', 'hop_2', 'circ_dim_a', 'circ_dim_b'],
            types: pkgTypes
        });

        const circTable = new ArscResourceTable(syntheticGlobalStrings, syntheticPackages);

        // Circular string resolution must terminate cleanly and return null
        assertCheck("Circular string reference terminates with null", circTable.resolveString(0x7f010000) === null);
        assertCheck("Circular dimen reference terminates with null", circTable.resolveDimension(0x7f020000) === null);

        // 4.3 Multi-Hop Valid Reference Chain
        assertCheck("Multi-hop reference chain resolves value", circTable.resolveString(0x7f010002) === 'Resolved Success!');
        assertCheck("resolveValue resolves multi-hop reference string", circTable.resolveValue('@string/hop_1') === 'Resolved Success!');

        // 4.4 Style Bag Resolution
        const allStyles = table.getAllEntries('style');
        assertCheck("Styles present in F-Droid ARSC", allStyles.length > 0);
        const firstStyle = allStyles[0];
        const styleRes = table.resolveStyle(firstStyle.resId);
        assertCheck("resolveStyle returns structured style object", styleRes !== null && typeof styleRes === 'object');
        assertCheck("resolveStyle contains attributes map", styleRes.attributes !== undefined);
    });

    // =========================================================================
    // PROBE 5: High-Throughput Resolution & Stress Stability
    // =========================================================================
    await runProbe("5. High-Throughput Resolution & Stress Stability (100,000 queries)", async () => {
        const table = ArscDecoder.decode(arscBytes);

        const testIds = [
            0x7f0c0020,
            0x7f09013e,
            0x7f09006d,
            0x7f120075,
            0x7f0c0001,
            0x7f090088
        ];

        const start = performance.now();
        for (let i = 0; i < 100000; i++) {
            const resId = testIds[i % testIds.length];
            const entry = table.getEntry(resId);
            if (entry.typeName === 'layout') {
                const p = table.resolveLayoutPath(resId);
                if (!p) throw new Error("Null layout path during stress");
            } else if (entry.typeName === 'string') {
                const s = table.resolveString(resId);
                if (!s) throw new Error("Null string during stress");
            }
        }
        const totalDuration = performance.now() - start;
        const qps = (100000 / (totalDuration / 1000)).toFixed(0);
        console.log(`  ⚡ High throughput rate: ${qps} resolutions/second (${totalDuration.toFixed(2)}ms for 100,000 ops)`);
        assertCheck("High-throughput completed 100,000 queries without error", true);
    });

    console.log("\n================================================================================");
    console.log(`📊 EXECUTION SUMMARY: ${passedChecks}/${totalChecks} Checks Passed`);
    console.log("================================================================================\n");

    if (failedChecks === 0) {
        console.log("✔ Milestone M1 Empirical Challenger PASSED cleanly with 100% invariant adherence!\n");
        process.exit(0);
    } else {
        console.error(`✖ ${failedChecks} checks FAILED!`);
        process.exit(1);
    }
}

main().catch(err => {
    console.error("Fatal error in empirical challenger:", err);
    process.exit(1);
});
