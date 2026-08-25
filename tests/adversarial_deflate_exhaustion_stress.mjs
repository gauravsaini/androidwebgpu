/**
 * AndroidWebGPU - Adversarial DEFLATE Stream Exhaustion Stress Harness
 * 
 * Verifies RFC 1951 DEFLATE decompression robustness against truncated dynamic/fixed
 * Huffman bitstreams and byte-boundary corruptions.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import zlib from 'zlib';
import fs from 'fs';
import path from 'path';
import { inflateRaw } from '../src/apk_client_parser.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        failed++;
        console.error(`[FAIL] ${message}`);
        throw new Error(message);
    }
    passed++;
}

console.log(`======================================================`);
console.log(`▶ Running: Adversarial DEFLATE Stream Exhaustion Tests`);
console.log(`======================================================`);

// 1. Truncated Dynamic Huffman Tree Test (Manifest payload 1-byte slice)
const manifest = Buffer.from("<manifest package=\"com.test\"><application><activity name=\".Main\"/></application></manifest>");
const compressedManifest = zlib.deflateRawSync(manifest, { level: 6 });
const slice1 = new Uint8Array(compressedManifest.subarray(0, 1));

console.log("Testing truncated dynamic stream slice [0x55] (1 byte)...");
try {
    // Note: If bug exists, this will enter an infinite loop in unpatched parser
    inflateRaw(slice1);
    console.log("[FAIL] Expected error on truncated stream [0x55], but returned normally");
    failed++;
} catch (err) {
    if (err.message.includes("Unexpected end of DEFLATE stream") || 
        err.message.includes("Invalid Huffman code")) {
        console.log(`[PASS] Correctly rejected truncated stream: ${err.message}`);
        passed++;
    } else {
        console.log(`[FAIL] Unexpected error: ${err.message}`);
        failed++;
    }
}

// 2. Fixed Huffman truncation at 1, 2, 3, 4, 7, 13, 19 bytes
const fixedPayload = zlib.deflateRawSync(Buffer.from("A".repeat(100)), { strategy: zlib.constants.Z_FIXED });
for (const len of [1, 2, 3, 4, 7, 13, 19]) {
    if (len < fixedPayload.length) {
        const slice = new Uint8Array(fixedPayload.subarray(0, len));
        try {
            inflateRaw(slice);
            console.log(`[FAIL] Expected error on fixed slice len ${len}`);
            failed++;
        } catch (e) {
            passed++;
        }
    }
}

console.log(`\n======================================================`);
console.log(`Results: ${passed} passed, ${failed} failed`);
console.log(`======================================================`);

if (failed > 0) {
    process.exit(1);
} else {
    process.exit(0);
}
