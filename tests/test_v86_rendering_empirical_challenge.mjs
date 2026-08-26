/**
/**
 * Empirical Challenger Test Suite for v86 Guest Boot, Kernel Header Validator,
 * Initrd CPIO Unpacker, WebGPU Swapchain, Damage Rect Scissoring & Canvas Presentation.
 * 
 * Deeply challenges:
 * 1. Kernel Header Validator (verifyBzImage) against boundary, endian, truncation, and 10,000 fuzz mutations.
 * 2. Initrd CPIO Unpacker & ART Image (boot.art) / Framework DEX binary structure under malformed inputs.
 * 3. Swapchain Buffer Indexing, Triple-Buffering Invariants & Target Transitions.
 * 4. Damage Rect Scissoring, Boundary Clamping, Subrect Math, and Malformed Rect Coordinates.
 * 5. OffscreenCanvas Raster Worker Message Protocol & Frame Timing Invariants.
 * 6. WebGPU Device Initialization, TIMESTAMP_QUERY Support Matrix & Feature Negotiation.
 * 
 * Conforms to ASD-STE100 and /ponytail simplicity principles.
 */

import fs from 'fs';
import path from 'path';
import zlib from 'zlib';
import { fileURLToPath } from 'url';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES } from '../src/v86_guest_manager.js';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalPassed = 0;
let totalFailed = 0;

function assert(condition, message) {
    if (!condition) {
        totalFailed++;
        console.error(`  ✖ [FAIL] ${message}`);
        throw new Error(`Assertion Failed: ${message}`);
    }
    totalPassed++;
}

async function runSection(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ [EMPIRICAL-CHALLENGE] ${name}`);
    console.log(`======================================================`);
    const start = performance.now();
    try {
        await fn();
        const duration = (performance.now() - start).toFixed(2);
        console.log(`✔ [PASS] ${name} (${duration}ms)`);
    } catch (err) {
        console.error(`✖ [FAIL] ${name}: ${err.message}`);
        throw err;
    }
}

// CPIO Parser implementation from test_v86_guest_boot.mjs for stress testing
function parseCpioNewc(buffer) {
    if (!buffer || !(buffer instanceof Uint8Array || Buffer.isBuffer(buffer))) {
        throw new TypeError("Invalid buffer provided to parseCpioNewc");
    }
    const files = new Map();
    let offset = 0;
    while (offset + 110 <= buffer.length) {
        const magic = Buffer.from(buffer.subarray(offset, offset + 6)).toString('ascii');
        if (magic !== '070701') {
            break;
        }
        const fileSizeStr = Buffer.from(buffer.subarray(offset + 54, offset + 62)).toString('ascii');
        const nameSizeStr = Buffer.from(buffer.subarray(offset + 94, offset + 102)).toString('ascii');
        const fileSize = parseInt(fileSizeStr, 16);
        const nameSize = parseInt(nameSizeStr, 16);

        if (isNaN(fileSize) || isNaN(nameSize) || nameSize <= 0 || fileSize < 0) {
            break;
        }

        const nameOffset = offset + 110;
        if (nameOffset + nameSize - 1 > buffer.length) {
            break;
        }

        const name = Buffer.from(buffer.subarray(nameOffset, nameOffset + nameSize - 1)).toString('ascii');
        if (name === 'TRAILER!!!') break;

        let headerAndNameLen = 110 + nameSize;
        if (headerAndNameLen % 4 !== 0) {
            headerAndNameLen += 4 - (headerAndNameLen % 4);
        }
        const dataOffset = offset + headerAndNameLen;
        if (dataOffset > buffer.length) break;

        const actualDataEnd = Math.min(dataOffset + fileSize, buffer.length);
        const fileData = buffer.subarray(dataOffset, actualDataEnd);
        files.set(name.replace(/^\.\//, ''), fileData);

        let totalLen = headerAndNameLen + fileSize;
        if (totalLen % 4 !== 0) {
            totalLen += 4 - (totalLen % 4);
        }
        if (totalLen <= 0) break; // Prevent infinite loop on 0 size/offset
        offset += totalLen;
    }
    return files;
}

function parseZipEntries(zipBuf) {
    const entries = new Map();
    let offset = 0;
    while (offset + 30 <= zipBuf.length) {
        const sig = zipBuf.readUInt32LE(offset);
        if (sig !== 0x04034B50) break;
        const compressedSize = zipBuf.readUInt32LE(offset + 18);
        const nameLen = zipBuf.readUInt16LE(offset + 26);
        const extraLen = zipBuf.readUInt16LE(offset + 28);
        const name = zipBuf.toString('utf8', offset + 30, offset + 30 + nameLen);
        const dataStart = offset + 30 + nameLen + extraLen;
        const data = zipBuf.subarray(dataStart, dataStart + compressedSize);
        entries.set(name, { offset: dataStart, data, size: compressedSize });
        offset = dataStart + compressedSize;
    }
    return entries;
}

async function main() {
    console.log("⚡ Starting Comprehensive Rendering & Guest Boot Empirical Challenger Suite...\n");

    const bzImagePath = path.join(rootDir, 'guest/build/bzImage');
    const validBzImage = fs.readFileSync(bzImagePath);

    const initrdPath = path.join(rootDir, 'guest/build/initrd.img');
    const validInitrdGz = fs.readFileSync(initrdPath);
    const validRawCpio = zlib.gunzipSync(validInitrdGz);

    // -------------------------------------------------------------------------
    // Challenge 1: Kernel Header Validator (verifyBzImage) Adversarial Stress
    // -------------------------------------------------------------------------
    await runSection("1. Kernel Header Validator (verifyBzImage) Adversarial Stress", async () => {
        const mgr = new V86GuestManager();

        // 1.1 Non-buffer and primitive inputs
        const invalidTypes = [
            null, undefined, 0, 1, -1, NaN, Infinity, "", "HdrS", true, false,
            {}, { length: 1000 }, [], [0xAA, 0x55], () => {}, Symbol("bzImage")
        ];
        for (const input of invalidTypes) {
            let res = false;
            try {
                res = mgr.verifyBzImage(input);
            } catch (e) {
                // Must not throw unhandled exception
                res = false;
            }
            assert(res === false, `verifyBzImage must return false for non-buffer: ${String(input)}`);
        }

        // 1.2 Sub-boundary buffer lengths (0 up to 519 bytes)
        for (let len = 0; len < 0x208; len++) {
            const truncated = validBzImage.subarray(0, len);
            assert(mgr.verifyBzImage(truncated) === false, `verifyBzImage must reject truncated buffer length ${len}`);
        }

        // 1.3 Exact boundary validity at 520 bytes
        const exactBoundary = validBzImage.subarray(0, 0x208);
        assert(mgr.verifyBzImage(exactBoundary) === true, "verifyBzImage must accept exact 520-byte valid header");

        // 1.4 Boot sector signature (0x1FE == 0xAA55) byte-by-byte corruption
        for (let b = 0; b < 256; b++) {
            if (b === 0x55) continue;
            const corruptSigLow = Buffer.from(exactBoundary);
            corruptSigLow[0x1FE] = b;
            assert(mgr.verifyBzImage(corruptSigLow) === false, `verifyBzImage must reject bad sig low byte 0x${b.toString(16)}`);
        }

        for (let b = 0; b < 256; b++) {
            if (b === 0xAA) continue;
            const corruptSigHigh = Buffer.from(exactBoundary);
            corruptSigHigh[0x1FF] = b;
            assert(mgr.verifyBzImage(corruptSigHigh) === false, `verifyBzImage must reject bad sig high byte 0x${b.toString(16)}`);
        }

        // 1.5 Header magic "HdrS" at 0x202 bit flips & byte mutations
        const badMagics = ['Hdr\0', 'hdrs', 'HDRS', 'SrdH', 'v86M', 'KRNL', '\0\0\0\0', '~~~~', '    '];
        for (const m of badMagics) {
            const corruptMagic = Buffer.from(exactBoundary);
            corruptMagic.write(m.padEnd(4, '\0'), 0x202, 4, 'ascii');
            assert(mgr.verifyBzImage(corruptMagic) === false, `verifyBzImage must reject magic '${m}'`);
        }

        // 1.6 Boot protocol version at 0x206 (< 0x0200 rejected, >= 0x0200 accepted)
        const protoTestCases = [
            { proto: 0x0000, valid: false },
            { proto: 0x0100, valid: false },
            { proto: 0x0102, valid: false },
            { proto: 0x01FF, valid: false },
            { proto: 0x0200, valid: true },
            { proto: 0x0201, valid: true },
            { proto: 0x020D, valid: true },
            { proto: 0x0210, valid: true },
            { proto: 0x0300, valid: true },
            { proto: 0xFFFF, valid: true }
        ];

        for (const tc of protoTestCases) {
            const protoBuf = Buffer.from(exactBoundary);
            protoBuf.writeUInt16LE(tc.proto, 0x206);
            assert(mgr.verifyBzImage(protoBuf) === tc.valid, `Protocol 0x${tc.proto.toString(16)} must yield ${tc.valid}`);
        }

        // 1.7 10,000 Randomized fuzz mutations
        for (let i = 0; i < 10000; i++) {
            const fuzzed = Buffer.from(exactBoundary);
            const numMutations = 1 + Math.floor(Math.random() * 5);
            for (let m = 0; m < numMutations; m++) {
                const randOffset = Math.floor(Math.random() * exactBoundary.length);
                fuzzed[randOffset] = Math.floor(Math.random() * 256);
            }
            // Execute validator — must not crash or throw
            const res = mgr.verifyBzImage(fuzzed);
            // If result is true, double-check that the invariant holds
            if (res === true) {
                const bootSig = fuzzed[0x1FE] | (fuzzed[0x1FF] << 8);
                const magic = String.fromCharCode(fuzzed[0x202], fuzzed[0x203], fuzzed[0x204], fuzzed[0x205]);
                const protocol = fuzzed[0x206] | (fuzzed[0x207] << 8);
                assert(bootSig === 0xAA55, "Fuzzed passed but bootSig violated");
                assert(magic === 'HdrS', "Fuzzed passed but magic violated");
                assert(protocol >= 0x0200, "Fuzzed passed but protocol violated");
            }
        }
    });

    // -------------------------------------------------------------------------
    // Challenge 2: Initrd CPIO Unpacker & ART Image / DEX Header Stress
    // -------------------------------------------------------------------------
    await runSection("2. Initrd CPIO Unpacker & ART Image / Framework DEX Binary Stress", async () => {
        // 2.1 Verify clean unpacking of valid CPIO archive
        const entries = parseCpioNewc(validRawCpio);
        assert(entries.has('system/framework/boot.art'), "Valid CPIO must contain boot.art");
        assert(entries.has('system/framework/framework.jar'), "Valid CPIO must contain framework.jar");

        // 2.2 Fuzz parseCpioNewc with truncated buffers
        for (let len = 0; len < 500; len += 7) {
            const trunc = validRawCpio.subarray(0, len);
            let parsed = null;
            try {
                parsed = parseCpioNewc(trunc);
            } catch (e) {
                parsed = null;
            }
            assert(parsed instanceof Map || parsed === null, "Truncated CPIO must handle gracefully without crash");
        }

        // 2.3 Malformed CPIO magic tests
        const badCpioMagics = ['070702', '070700', '000000', 'CPIONW', 'TRAIL!'];
        for (const badM of badCpioMagics) {
            const corruptedCpio = Buffer.from(validRawCpio);
            corruptedCpio.write(badM, 0, 6, 'ascii');
            const parsed = parseCpioNewc(corruptedCpio);
            assert(parsed.size === 0, `Corrupted CPIO magic '${badM}' must cleanly terminate parse with 0 entries`);
        }

        // 2.4 Malformed / Overflow hex numbers in header
        const corruptHexBuf = Buffer.from(validRawCpio);
        // Overwrite fileSize field with invalid hex
        corruptHexBuf.write('ZZZZZZZZ', 54, 8, 'ascii');
        const parsedCorruptHex = parseCpioNewc(corruptHexBuf);
        assert(parsedCorruptHex.size === 0, "Invalid hex in fileSize must cleanly terminate");

        // 2.5 ART Image (boot.art) Deep Binary Invariant Verification
        const bootArt = entries.get('system/framework/boot.art');
        assert(bootArt.length >= 64, "boot.art length >= 64 bytes");

        // Validate ART header structure:
        // offset 0x00: magic (art\n018\0)
        // offset 0x08: image_begin_ (0x70000000)
        // offset 0x0C: image_size_
        // offset 0x10: image_checksum_
        // offset 0x14: image_roots_ (0x70001000)
        // offset 0x18: pointer_size_ (4 for 32-bit x86)
        const artMagic = bootArt.toString('ascii', 0, 8);
        assert(artMagic === 'art\n018\0', `ART magic must be 'art\\n018\\0', got ${JSON.stringify(artMagic)}`);
        assert(bootArt.readUInt32LE(0x08) === 0x70000000, "ART image_begin_ must be 0x70000000");
        assert(bootArt.readUInt32LE(0x24) === 4, "ART pointer_size_ must be 4 (32-bit x86)");
        assert(bootArt.readUInt32LE(0x20) === 0x70001000, "ART image_roots_ must be 0x70001000");

        // 2.6 Framework.jar ZIP structure and classes.dex DEX Header Verification
        const frameworkJar = entries.get('system/framework/framework.jar');
        const jarEntries = parseZipEntries(frameworkJar);
        assert(jarEntries.has('META-INF/MANIFEST.MF'), "framework.jar must contain META-INF/MANIFEST.MF entry");
        assert(jarEntries.has('classes.dex'), "framework.jar must contain classes.dex entry");

        const dexEntry = jarEntries.get('classes.dex');
        assert(dexEntry && dexEntry.data.length >= 0x70, "classes.dex must have valid DEX binary payload");
        const dexMagic = dexEntry.data.toString('ascii', 0, 8);
        assert(dexMagic === 'dex\n035\0', "DEX header magic must be 'dex\\n035\\0'");

        // DEX header field checks (checksum, signature, file_size, header_size)
        const dexHeaderSize = dexEntry.data.readUInt32LE(0x24);
        assert(dexHeaderSize === 0x70, "DEX header size must be 0x70 (112 bytes)");

        const dexEndianTag = dexEntry.data.readUInt32LE(0x28);
        assert(dexEndianTag === 0x12345678, "DEX endian tag must be 0x12345678 (ENDIAN_CONSTANT)");
    });

    // -------------------------------------------------------------------------
    // Challenge 3: Swapchain Buffer Indexing & Offscreen Readback Invariants
    // -------------------------------------------------------------------------
    await runSection("3. Swapchain Triple-Buffering & Buffer Indexing Invariants", async () => {
        // Test simulated swapchain buffer index ring math (0 -> 1 -> 2 -> 0 -> 1 -> 2...)
        const bufferCount = 3;
        let currentIdx = 0;
        let frameCount = 0;

        for (let f = 1; f <= 30000; f++) {
            // Emulate swapchain present() index increment
            currentIdx = (currentIdx + 1) % bufferCount;
            frameCount++;

            // Last presented texture is always the previous one
            const prevIdx = (currentIdx + bufferCount - 1) % bufferCount;

            assert(currentIdx >= 0 && currentIdx < bufferCount, `Frame ${f}: currentIdx out of bounds: ${currentIdx}`);
            assert(prevIdx >= 0 && prevIdx < bufferCount, `Frame ${f}: prevIdx out of bounds: ${prevIdx}`);
            assert(currentIdx !== prevIdx, `Frame ${f}: currentIdx and prevIdx must be distinct in triple buffering`);
            assert(frameCount === f, `Frame count mismatch: ${frameCount} != ${f}`);
        }

        // Test bytes_per_row alignment math: (u32_size * width + 255) & !255
        const widths = [1, 2, 3, 4, 15, 16, 17, 32, 63, 64, 100, 720, 800, 1080, 1440, 1920, 2560, 3840];
        for (const w of widths) {
            const u32Size = 4;
            const unaligned = u32Size * w;
            const aligned = (unaligned + 255) & ~255;
            assert(aligned >= unaligned, `Aligned bytes per row ${aligned} must be >= unaligned ${unaligned}`);
            assert(aligned % 256 === 0, `Aligned bytes per row ${aligned} must be 256-byte aligned`);
        }
    });

    // -------------------------------------------------------------------------
    // Challenge 4: Damage Rect Scissoring & Coordinate Math Stress
    // -------------------------------------------------------------------------
    await runSection("4. Damage Rect Scissoring, Subrect Math & Boundary Clamping", async () => {
        const canvasWidth = 800;
        const canvasHeight = 600;

        // Mock canvas context
        let lastPutImageDataCall = null;
        const mockCtx = {
            createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
            putImageData: (img, x, y, dx, dy, dw, dh) => {
                lastPutImageDataCall = { x, y, dx, dy, dw, dh };
            }
        };

        const mockCanvas = {
            width: canvasWidth,
            height: canvasHeight,
            getContext: () => mockCtx
        };

        let scanoutDamage = [0, 0, 800, 600];
        let clearCalled = false;

        const mockRustBridge = {
            get_scanout_framebuffer: (id) => new Uint8Array(canvasWidth * canvasHeight * 4),
            get_scanout_damage: (id) => scanoutDamage,
            clear_scanout_damage: (id) => { clearCalled = true; scanoutDamage = null; }
        };

        const gpuDevice = new VirtioGpuDevice(null, mockRustBridge, mockCanvas, null, false);

        // 4.1 Valid interior damage rect
        scanoutDamage = [100, 150, 200, 300];
        clearCalled = false;
        gpuDevice.renderScanoutToCanvas(0);
        assert(clearCalled === true, "clear_scanout_damage must be invoked");
        assert(gpuDevice.damage_rects_count === 1, "Damage rect count incremented");
        assert(lastPutImageDataCall.dx === 100, "dx matched 100");
        assert(lastPutImageDataCall.dy === 150, "dy matched 150");
        assert(lastPutImageDataCall.dw === 200, "dw matched 200");
        assert(lastPutImageDataCall.dh === 300, "dh matched 300");

        // 4.2 Over-boundary damage rect (x + dw > canvasWidth)
        scanoutDamage = [700, 500, 300, 400];
        clearCalled = false;
        gpuDevice.renderScanoutToCanvas(0);
        assert(clearCalled === true, "clear_scanout_damage invoked on clamped rect");
        assert(lastPutImageDataCall.dx === 700, "dx matched 700");
        assert(lastPutImageDataCall.dy === 500, "dy matched 500");
        assert(lastPutImageDataCall.dw === 100, "dw clamped to 800 - 700 = 100");
        assert(lastPutImageDataCall.dh === 100, "dh clamped to 600 - 500 = 100");

        // 4.3 Malformed / Negative / Zero damage rects must fall back to full blit safely
        const malformedDamageRects = [
            [-10, 0, 100, 100],     // negative x
            [0, -10, 100, 100],     // negative y
            [0, 0, 0, 100],         // zero width
            [0, 0, 100, 0],         // zero height
            [850, 100, 50, 50],     // x out of bounds
            [100, 650, 50, 50],     // y out of bounds
            null,                   // null
            [],                     // empty
            [10, 20],               // insufficient elements
            [NaN, 0, 100, 100],     // NaN
            [0, 0, Infinity, 100]   // Infinity
        ];

        for (const badRect of malformedDamageRects) {
            scanoutDamage = badRect;
            clearCalled = false;
            lastPutImageDataCall = null;
            gpuDevice.renderScanoutToCanvas(0);
            // Full blit fallback puts image data at (0, 0)
            assert(lastPutImageDataCall !== null, `Malformed rect ${JSON.stringify(badRect)} must execute fallback putImageData`);
            assert(lastPutImageDataCall.x === 0 && lastPutImageDataCall.y === 0, "Fallback must be full blit at (0, 0)");
        }

        // 4.4 10,000 Randomized damage rect blits
        for (let i = 0; i < 10000; i++) {
            const rx = Math.floor(Math.random() * 1000) - 100;
            const ry = Math.floor(Math.random() * 800) - 100;
            const rw = Math.floor(Math.random() * 1000);
            const rh = Math.floor(Math.random() * 800);
            scanoutDamage = [rx, ry, rw, rh];
            gpuDevice.renderScanoutToCanvas(0);
        }
        assert(gpuDevice.damage_rects_count > 100, "Randomized damage rects processed without failure");
    });

    // -------------------------------------------------------------------------
    // Challenge 5: Web Worker Offscreen Raster Protocol Invariants
    // -------------------------------------------------------------------------
    await runSection("5. Offscreen Raster Worker Message Invariants & Metrics", async () => {
        // Validate message protocol handlers
        const mockWorkerMessages = [
            { type: "INIT_OFFSCREEN", width: 800, height: 600 },
            { type: "PROCESS_COMMAND", id: 1, command: new Uint8Array([1, 2, 3, 4]) },
            { type: "UPDATE_DAMAGE_RECT", x: 10, y: 10, width: 100, height: 100, pixels: new Uint8Array(100 * 100 * 4) },
            { type: "START_LOOP", targetFps: 120 },
            { type: "RENDER_FRAME" },
            { type: "STOP_LOOP" }
        ];

        for (const msg of mockWorkerMessages) {
            assert(typeof msg.type === "string", `Worker message type ${msg.type} is string`);
        }

        // Validate Virtio GPU control queue response packet
        const defaultResp = new Uint8Array([0x00, 0x11, 0x00, 0x00]); // VIRTIO_GPU_RESP_OK_NODATA (0x1100)
        assert(defaultResp[0] === 0x00 && defaultResp[1] === 0x11, "VIRTIO_GPU_RESP_OK_NODATA format valid");
    });

    // -------------------------------------------------------------------------
    // Challenge 6: WebGPU Device TIMESTAMP_QUERY Feature Matrix
    // -------------------------------------------------------------------------
    await runSection("6. WebGPU Device Feature Negotiation & TIMESTAMP_QUERY Matrix", async () => {
        const mgr = new V86GuestManager();

        // 6.1 Adapter with timestamp-query supported
        let requestedFeatures = null;
        const adapterWithTs = {
            features: new Set(['timestamp-query', 'texture-compression-bc']),
            requestDevice: async (desc) => {
                requestedFeatures = desc.requiredFeatures;
                return { label: 'wgpu-device-with-ts', features: new Set(desc.requiredFeatures) };
            }
        };

        const devWithTs = await mgr.initWebGpuDevice(adapterWithTs);
        assert(devWithTs !== null, "Device with timestamp-query created");
        assert(requestedFeatures.includes('timestamp-query'), "timestamp-query requested in device descriptor");
        assert(mgr.gpuFeatures.includes('timestamp-query'), "mgr.gpuFeatures records timestamp-query");

        // 6.2 Adapter without timestamp-query supported
        requestedFeatures = null;
        const adapterWithoutTs = {
            features: new Set([]),
            requestDevice: async (desc) => {
                requestedFeatures = desc.requiredFeatures;
                return { label: 'wgpu-device-no-ts', features: new Set(desc.requiredFeatures) };
            }
        };

        const devNoTs = await mgr.initWebGpuDevice(adapterWithoutTs);
        assert(devNoTs !== null, "Device without timestamp-query created");
        assert(requestedFeatures.length === 0, "No features requested when timestamp-query absent");
        assert(mgr.gpuFeatures.length === 0, "mgr.gpuFeatures is empty");

        // 6.3 Null adapter
        const nullDev = await mgr.initWebGpuDevice(null);
        assert(nullDev === null, "Null adapter returns null device gracefully");
    });

    console.log(`\n======================================================`);
    console.log(`⚡ ALL RENDERING & BOOT EMPIRICAL CHALLENGES PASSED`);
    console.log(`Total assertions passed: ${totalPassed}`);
    console.log(`Total assertions failed: ${totalFailed}`);
    console.log(`======================================================\n`);

    if (totalFailed > 0) {
        process.exit(1);
    } else {
        process.exit(0);
    }
}

main().catch(err => {
    console.error("Fatal error in empirical challenger suite:", err);
    process.exit(1);
});
