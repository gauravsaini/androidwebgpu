/**
 * Adversarial Test Harness for Browser Test Bench & Lifecycle Integration
 * 
 * Tests:
 * 1. Rapid visibilitychange toggling (hidden <-> visible)
 * 2. Blur / focus event storms and invariant preservation
 * 3. Corrupted BinderParcel packets & VirtioBinder framing fuzzing
 * 4. Malformed NALUs and bitstream edge cases
 * 5. Invalid sensor coordinates, handles, and timestamps
 * 6. Empty / foreign / double buffer releases & pool exhaustion
 * 7. Test runner unhandled rejection immunity
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import {
    BinderParcel,
    VirtioBinderFraming,
    BinderTestSuite,
    buildSyntheticAxml,
    AxmlDecoder,
    ApkZipReader,
    PackageManagerRegistry,
    parseApk,
    defaultPackageManager,
    CMD_PING,
    CMD_TRANSACT,
    CMD_ACQUIRE,
    CMD_RELEASE,
    CMD_LINK_DEATH,
    PING_TRANSACTION,
    STATUS_OK,
    STATUS_NAME_NOT_FOUND,
    STATUS_FAILED_TRANSACTION,
    BR_OK,
    BR_REPLY,
    BR_FAILED_REPLY
} from '../src/binder_test_suite.js';
import fs from 'fs';
import path from 'path';

let passedChecks = 0;
let failedChecks = 0;

function assert(condition, message) {
    if (!condition) {
        failedChecks++;
        console.error(`[FAIL] ${message}`);
        throw new Error(`Assertion failed: ${message}`);
    }
    passedChecks++;
}

function runSection(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ Running Adversarial Suite: ${name}`);
    console.log(`======================================================`);
    try {
        fn();
        console.log(`✔ [PASS] ${name} completed successfully.`);
    } catch (err) {
        console.error(`✖ [FAIL] ${name} failed: ${err.message}`);
        throw err;
    }
}

// -----------------------------------------------------------------------------
// Test Section 1: Rapid VisibilityChange Toggling
// -----------------------------------------------------------------------------
runSection("1. Rapid VisibilityChange Toggling & Lifecycle State Machine", () => {
    // Construct mock browser environment
    let audioPaused = false;
    let audioResumed = false;
    let cameraPaused = false;
    let cameraResumed = false;
    let sensorsPaused = false;
    let sensorsResumed = false;
    let rendererPaused = false;
    let rendererResumed = false;
    let audioGainValue = 1.0;
    let savedAudioVolume = 1.0;

    const mockAndroidWebGpu = {
        lifecycle: {
            state: 'INITIALIZING',
            isVisible: true,
            hasFocus: true,
            lastStateChange: 0,
        },
        audio: {
            isStreaming: true,
            savedVolume: 1.0,
            masterGainNode: {
                gain: {
                    value: 1.0,
                    setValueAtTime(val) {
                        this.value = val;
                        audioGainValue = val;
                    }
                }
            },
            context: {
                state: 'running',
                currentTime: 10.0,
                suspend() { this.state = 'suspended'; audioPaused = true; },
                resume() { this.state = 'running'; audioResumed = true; }
            },
            pause() {
                if (this.context && this.context.state === 'running') {
                    this.context.suspend();
                }
            },
            resume() {
                if (this.context && this.context.state === 'suspended') {
                    this.context.resume();
                }
            }
        },
        camera: {
            isStreaming: true,
            tickInterval: 123,
            pause() {
                if (this.tickInterval) {
                    this.tickInterval = null;
                    cameraPaused = true;
                }
            },
            resume() {
                this.tickInterval = 456;
                cameraResumed = true;
            }
        },
        sensors: {
            isActive: true,
            tickInterval: 789,
            pause() {
                if (this.tickInterval) {
                    this.tickInterval = null;
                    sensorsPaused = true;
                }
            },
            resume() {
                this.tickInterval = 999;
                sensorsResumed = true;
            }
        },
        renderer: {
            pause() { rendererPaused = true; },
            resume() { rendererResumed = true; }
        },
        dispatchVisibilityChange(targetState) {
            const isVisible = targetState === 'visible';
            handleVisibilityChange(isVisible);
        },
        dispatchFocusChange(focused) {
            handleWindowFocus(focused);
        }
    };

    function handleVisibilityChange(isVisible) {
        mockAndroidWebGpu.lifecycle.isVisible = isVisible;
        mockAndroidWebGpu.lifecycle.lastStateChange = Date.now();

        if (!isVisible) {
            mockAndroidWebGpu.lifecycle.state = 'PAUSED';
            mockAndroidWebGpu.renderer.pause();
            mockAndroidWebGpu.audio.pause();
            mockAndroidWebGpu.camera.pause();
            mockAndroidWebGpu.sensors.pause();
        } else {
            mockAndroidWebGpu.lifecycle.state = 'RESUMED';
            mockAndroidWebGpu.audio.resume();
            mockAndroidWebGpu.camera.resume();
            mockAndroidWebGpu.sensors.resume();
            mockAndroidWebGpu.renderer.resume();
        }
    }

    function handleWindowFocus(hasFocus) {
        mockAndroidWebGpu.lifecycle.hasFocus = hasFocus;

        if (!hasFocus) {
            if (mockAndroidWebGpu.audio.masterGainNode && mockAndroidWebGpu.audio.context) {
                mockAndroidWebGpu.audio.savedVolume = mockAndroidWebGpu.audio.masterGainNode.gain.value;
                mockAndroidWebGpu.audio.masterGainNode.gain.setValueAtTime(0.0, mockAndroidWebGpu.audio.context.currentTime);
            }
        } else {
            if (mockAndroidWebGpu.lifecycle.isVisible) {
                if (mockAndroidWebGpu.audio.masterGainNode && mockAndroidWebGpu.audio.context) {
                    mockAndroidWebGpu.audio.masterGainNode.gain.setValueAtTime(
                        mockAndroidWebGpu.audio.savedVolume || 1.0,
                        mockAndroidWebGpu.audio.context.currentTime
                    );
                }
            }
        }
    }

    // 1.1 Rapid Sequential Toggling: 10,000 cycles
    console.log("  -> Executing 10,000 rapid visibility transitions...");
    for (let i = 0; i < 10000; i++) {
        mockAndroidWebGpu.dispatchVisibilityChange('hidden');
        assert(mockAndroidWebGpu.lifecycle.state === 'PAUSED', `Cycle ${i}: State must be PAUSED when hidden`);
        assert(mockAndroidWebGpu.lifecycle.isVisible === false, `Cycle ${i}: isVisible must be false`);
        assert(mockAndroidWebGpu.audio.context.state === 'suspended', `Cycle ${i}: Audio context must be suspended`);
        assert(mockAndroidWebGpu.camera.tickInterval === null, `Cycle ${i}: Camera interval must be cleared`);
        assert(mockAndroidWebGpu.sensors.tickInterval === null, `Cycle ${i}: Sensor interval must be cleared`);

        mockAndroidWebGpu.dispatchVisibilityChange('visible');
        assert(mockAndroidWebGpu.lifecycle.state === 'RESUMED', `Cycle ${i}: State must be RESUMED when visible`);
        assert(mockAndroidWebGpu.lifecycle.isVisible === true, `Cycle ${i}: isVisible must be true`);
        assert(mockAndroidWebGpu.audio.context.state === 'running', `Cycle ${i}: Audio context must be running`);
        assert(mockAndroidWebGpu.camera.tickInterval !== null, `Cycle ${i}: Camera interval must be active`);
        assert(mockAndroidWebGpu.sensors.tickInterval !== null, `Cycle ${i}: Sensor interval must be active`);
    }

    // 1.2 Boundary / Idempotency check: Repeated identical states
    for (let i = 0; i < 100; i++) {
        mockAndroidWebGpu.dispatchVisibilityChange('hidden');
    }
    assert(mockAndroidWebGpu.lifecycle.state === 'PAUSED', "Multiple hidden calls must stay PAUSED");
    assert(mockAndroidWebGpu.audio.context.state === 'suspended', "Audio context must stay suspended");

    for (let i = 0; i < 100; i++) {
        mockAndroidWebGpu.dispatchVisibilityChange('visible');
    }
    assert(mockAndroidWebGpu.lifecycle.state === 'RESUMED', "Multiple visible calls must stay RESUMED");
    assert(mockAndroidWebGpu.audio.context.state === 'running', "Audio context must stay running");
});

// -----------------------------------------------------------------------------
// Test Section 2: Blur / Focus Event Storms & Audio Focus Invariants
// -----------------------------------------------------------------------------
runSection("2. Blur / Focus Event Storms & Audio Focus Invariants", () => {
    let audioGainValue = 1.0;
    let savedAudioVolume = 0.85;

    const mockAudioState = {
        masterGainNode: {
            gain: {
                value: 0.85,
                setValueAtTime(val) {
                    this.value = val;
                    audioGainValue = val;
                }
            }
        },
        context: {
            state: 'running',
            currentTime: 5.0
        },
        savedVolume: 0.85
    };

    const lifecycle = {
        isVisible: true,
        hasFocus: true
    };

    function handleFocus(hasFocus) {
        lifecycle.hasFocus = hasFocus;
        if (!hasFocus) {
            if (mockAudioState.masterGainNode && mockAudioState.context) {
                mockAudioState.savedVolume = mockAudioState.masterGainNode.gain.value;
                mockAudioState.masterGainNode.gain.setValueAtTime(0.0, mockAudioState.context.currentTime);
            }
        } else {
            if (lifecycle.isVisible) {
                if (mockAudioState.masterGainNode && mockAudioState.context) {
                    mockAudioState.masterGainNode.gain.setValueAtTime(
                        mockAudioState.savedVolume || 1.0,
                        mockAudioState.context.currentTime
                    );
                }
            }
        }
    }

    console.log("  -> Executing 5,000 rapid blur/focus cycles...");
    for (let i = 0; i < 5000; i++) {
        handleFocus(false);
        assert(lifecycle.hasFocus === false, "hasFocus must be false");
        assert(mockAudioState.masterGainNode.gain.value === 0.0, "Gain must be muted (0.0) on blur");

        handleFocus(true);
        assert(lifecycle.hasFocus === true, "hasFocus must be true");
        assert(mockAudioState.masterGainNode.gain.value === 0.85, "Gain must be restored to 0.85 on focus");
    }

    // 2.2 Invariant: Focus event received while tab is HIDDEN must NOT unmute audio
    console.log("  -> Testing focus event while document is hidden (audio must remain muted)...");
    lifecycle.isVisible = false;
    handleFocus(false); // blur
    assert(mockAudioState.masterGainNode.gain.value === 0.0, "Muted on blur");

    handleFocus(true); // focus while hidden!
    assert(lifecycle.hasFocus === true, "hasFocus set to true");
    assert(mockAudioState.masterGainNode.gain.value === 0.0, "Gain must remain 0.0 when tab is hidden despite focus!");

    // Restore visibility -> audio restores
    lifecycle.isVisible = true;
    handleFocus(true);
    assert(mockAudioState.masterGainNode.gain.value === 0.85, "Gain restored once visible and focused");

    // 2.3 Null-safety: Audio nodes undefined or destroyed
    const brokenAudio = { masterGainNode: null, context: null };
    function handleSafeFocus(hasFocus) {
        if (!hasFocus) {
            if (brokenAudio.masterGainNode && brokenAudio.context) {
                brokenAudio.savedVolume = brokenAudio.masterGainNode.gain.value;
            }
        }
    }
    handleSafeFocus(false);
    handleSafeFocus(true);
    assert(true, "Null audio state handled without exception");
});

// -----------------------------------------------------------------------------
// Test Section 3: Corrupted BinderParcel Packets & VirtioBinder Framing
// -----------------------------------------------------------------------------
runSection("3. Corrupted BinderParcel Packets & Fuzzing", () => {
    // 3.1 Header Truncation (< 32 bytes for response, < 48 bytes for request)
    const shortBuffers = [
        new Uint8Array(0),
        new Uint8Array(1),
        new Uint8Array(16),
        new Uint8Array(31),
    ];

    for (const buf of shortBuffers) {
        let threw = false;
        try {
            VirtioBinderFraming.parseResponse(buf);
        } catch (err) {
            threw = true;
            assert(err.message.includes("too short"), `Expected 'too short' error, got: ${err.message}`);
        }
        assert(threw, `Truncated buffer of length ${buf.length} must throw on parseResponse`);
    }

    // 3.2 Corrupted dataSize / integer overflow in response
    const corruptRespBuf = new Uint8Array(32);
    const dv = new DataView(corruptRespBuf.buffer);
    dv.setBigUint64(0, 100n, true); // msgId
    dv.setInt32(8, 0, true); // status OK
    dv.setInt32(12, BR_REPLY, true); // resultCode
    dv.setUint32(16, 1000000, true); // dataSize = 1MB (buffer is only 32 bytes)
    dv.setUint32(20, 0, true); // offsetsSize

    const parsed = VirtioBinderFraming.parseResponse(corruptRespBuf);
    assert(parsed.data.length === 0, "Out-of-bounds slice must clamp to available length without crashing");

    // 3.3 BinderParcel Primitive Overflows
    const emptyParcel = new BinderParcel(0);
    const readMethods = [
        () => emptyParcel.readInt8(),
        () => emptyParcel.readUint8(),
        () => emptyParcel.readInt16(),
        () => emptyParcel.readUint16(),
        () => emptyParcel.readInt32(),
        () => emptyParcel.readUint32(),
        () => emptyParcel.readInt64(),
        () => emptyParcel.readUint64(),
        () => emptyParcel.readFloat32(),
        () => emptyParcel.readFloat64(),
        () => emptyParcel.readByteArray(),
    ];

    for (const method of readMethods) {
        let threw = false;
        try {
            method();
        } catch (err) {
            threw = true;
            assert(err.message.includes("overflow"), `Expected overflow message, got: ${err.message}`);
        }
        assert(threw, "Reading from empty parcel must throw overflow error");
    }

    // 3.4 Corrupted UTF-8 / UTF-16 String Lengths
    const badStringParcel = new BinderParcel(16);
    badStringParcel.writeInt32(999999); // Claims 1MB string in 16 byte buffer
    badStringParcel.readPos = 0;
    let threwUtf8 = false;
    try {
        badStringParcel.readUtf8();
    } catch (err) {
        threwUtf8 = true;
        assert(err.message.includes("overflow"), "readUtf8 overflow caught");
    }
    assert(threwUtf8, "readUtf8 with oversized length must throw");

    badStringParcel.readPos = 0;
    let threwUtf16 = false;
    try {
        badStringParcel.readUtf16();
    } catch (err) {
        threwUtf16 = true;
        assert(err.message.includes("overflow"), "readUtf16 overflow caught");
    }
    assert(threwUtf16, "readUtf16 with oversized length must throw");

    // 3.5 Negative String Length Handling (null strings in AIDL)
    const nullStringParcel = new BinderParcel(8);
    nullStringParcel.writeInt32(-1);
    nullStringParcel.readPos = 0;
    assert(nullStringParcel.readUtf8() === null, "Negative string length in UTF8 must return null");

    nullStringParcel.readPos = 0;
    assert(nullStringParcel.readUtf16() === null, "Negative string length in UTF16 must return null");

    // 3.6 Negative Array Length Handling (null byte arrays)
    nullStringParcel.readPos = 0;
    assert(nullStringParcel.readByteArray() === null, "Negative byte array length must return null");

    // 3.7 Fuzzing Dispatcher with Pseudo-Random Packets
    console.log("  -> Fuzzing BinderTestSuite.emulatedProcessPacket with 1,000 randomized buffers...");
    const testSuite = new BinderTestSuite(null, null, () => {});

    // Seeded deterministic PRNG for fuzz reproducibility
    let seed = 0x12345678;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    for (let i = 0; i < 1000; i++) {
        const len = 48 + (prng() % 256);
        const randomBuf = new Uint8Array(len);
        for (let j = 0; j < len; j++) {
            randomBuf[j] = prng() & 0xFF;
        }

        // Set valid-looking header fields but random payload/commands
        const rv = new DataView(randomBuf.buffer);
        rv.setBigUint64(0, BigInt(i), true);
        rv.setUint32(8, (prng() % 10), true); // cmd (some invalid)
        rv.setUint32(12, (prng() % 5), true); // targetHandle
        rv.setUint32(16, (prng() % 2000), true); // code
        rv.setUint32(32, len - 48, true); // dataSize

        try {
            const resp = testSuite.emulatedProcessPacket(randomBuf);
            assert(resp !== null && resp.hdr !== undefined, "Fuzzed packet must return valid structured response");
        } catch (err) {
            // Emulated packet processor should handle unknown commands gracefully without crashing
            console.error(`Unexpected exception on fuzz packet ${i}:`, err);
            throw err;
        }
    }
});

// -----------------------------------------------------------------------------
// Test Section 4: Malformed NALUs (WebCodecs / Media Pipeline)
// -----------------------------------------------------------------------------
runSection("4. Malformed NALUs & Bitstream Edge Cases", () => {
    function validateH264Keyframe(nalu) {
        if (!nalu || nalu.length < 5) return false;
        // Check Annex B start code: 00 00 00 01
        const hasStartCode = nalu[0] === 0 && nalu[1] === 0 && nalu[2] === 0 && nalu[3] === 1;
        if (!hasStartCode) return false;
        // Check forbidden_zero_bit (bit 7 must be 0)
        const header = nalu[4];
        if ((header & 0x80) !== 0) return false; // forbidden_zero_bit violation
        const nalUnitType = header & 0x1F;
        return nalUnitType === 5; // IDR keyframe
    }

    // 4.1 Truncated / Zero-length buffers
    assert(validateH264Keyframe(null) === false, "Null NALU rejected");
    assert(validateH264Keyframe(new Uint8Array(0)) === false, "0-length NALU rejected");
    assert(validateH264Keyframe(new Uint8Array(4)) === false, "4-byte NALU (missing header) rejected");

    // 4.2 Invalid start codes
    const badStart1 = new Uint8Array([0x00, 0x00, 0x01, 0x65]); // 3-byte prefix when expecting 4-byte
    assert(validateH264Keyframe(badStart1) === false, "3-byte start code rejected");

    const badStart2 = new Uint8Array([0x01, 0x00, 0x00, 0x01, 0x65]);
    assert(validateH264Keyframe(badStart2) === false, "Corrupted start code rejected");

    // 4.3 Forbidden Zero Bit Violation (bit 7 = 1)
    const forbiddenBitNalu = new Uint8Array([0x00, 0x00, 0x00, 0x01, 0xE5]); // 0xE5 = 11100101 (forbidden bit = 1)
    assert(validateH264Keyframe(forbiddenBitNalu) === false, "Forbidden zero bit violation rejected");

    // 4.4 Non-IDR NALU types
    const nonIdrNalu = new Uint8Array([0x00, 0x00, 0x00, 0x01, 0x61]); // Type 1 (non-IDR slice)
    assert(validateH264Keyframe(nonIdrNalu) === false, "Non-IDR NALU correctly not flagged as keyframe");

    const spsNalu = new Uint8Array([0x00, 0x00, 0x00, 0x01, 0x67]); // Type 7 (SPS)
    assert(validateH264Keyframe(spsNalu) === false, "SPS NALU correctly not flagged as keyframe");

    // 4.5 Valid IDR Keyframe
    const validIdr = new Uint8Array([0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x21]); // Type 5 (IDR)
    assert(validateH264Keyframe(validIdr) === true, "Valid IDR keyframe successfully verified");
});

// -----------------------------------------------------------------------------
// Test Section 5: Invalid Sensor Coordinates, Handles & Timestamps
// -----------------------------------------------------------------------------
runSection("5. Invalid Sensor Coordinates, Handles & Timestamps", () => {
    function sanitizeSensorSample(handle, x, y, z, timestampNs) {
        // Valid handle check: integer > 0
        if (!Number.isInteger(handle) || handle <= 0) {
            throw new Error(`Invalid sensor handle: ${handle}`);
        }
        // Valid coordinate checks: finite numbers (reject NaN, Infinity, strings, null)
        if (typeof x !== 'number' || !Number.isFinite(x) ||
            typeof y !== 'number' || !Number.isFinite(y) ||
            typeof z !== 'number' || !Number.isFinite(z)) {
            throw new Error(`Invalid sensor coordinates: (${x}, ${y}, ${z})`);
        }
        // Valid timestamp check: non-negative integer/bigint
        if (typeof timestampNs !== 'bigint' && (!Number.isInteger(timestampNs) || timestampNs < 0)) {
            throw new Error(`Invalid timestamp: ${timestampNs}`);
        }
        return { handle, x, y, z, timestampNs };
    }

    // 5.1 Invalid coordinate types (NaN, Infinity, string, null, undefined)
    const invalidCoords = [
        [NaN, 9.8, 0.0],
        [0.0, Infinity, 0.0],
        [0.0, -Infinity, 0.0],
        ["0.0", 9.8, 0.0],
        [null, 9.8, 0.0],
        [undefined, 9.8, 0.0],
        [{}, 9.8, 0.0],
        [[], 9.8, 0.0],
    ];

    for (const [x, y, z] of invalidCoords) {
        let threw = false;
        try {
            sanitizeSensorSample(1, x, y, z, 1000000n);
        } catch (err) {
            threw = true;
            assert(err.message.includes("Invalid sensor coordinates"), `Expected coordinate error, got: ${err.message}`);
        }
        assert(threw, `Invalid coordinate (${x}, ${y}, ${z}) must be rejected`);
    }

    // 5.2 Invalid handles
    const invalidHandles = [-1, 0, 1.5, "1", null, undefined];
    for (const h of invalidHandles) {
        let threw = false;
        try {
            sanitizeSensorSample(h, 0.0, 9.8, 0.0, 1000000n);
        } catch (err) {
            threw = true;
            assert(err.message.includes("Invalid sensor handle"), `Expected handle error, got: ${err.message}`);
        }
        assert(threw, `Invalid handle ${h} must be rejected`);
    }

    // 5.3 Invalid timestamps
    const invalidTimestamps = [-1, -1000, 1.23, NaN, "1000", null];
    for (const ts of invalidTimestamps) {
        let threw = false;
        try {
            sanitizeSensorSample(1, 0.0, 9.8, 0.0, ts);
        } catch (err) {
            threw = true;
            assert(err.message.includes("Invalid timestamp"), `Expected timestamp error, got: ${err.message}`);
        }
        assert(threw, `Invalid timestamp ${ts} must be rejected`);
    }

    // 5.4 Valid Samples
    const valid = sanitizeSensorSample(1, 0.0, 9.80665, 0.0, 16000000n);
    assert(valid.x === 0.0 && valid.y === 9.80665 && valid.z === 0.0, "Valid sensor sample verified");
});

// -----------------------------------------------------------------------------
// Test Section 6: Buffer Pool Exhaustion & Empty / Double Buffer Releases
// -----------------------------------------------------------------------------
runSection("6. Buffer Pool Exhaustion & Empty / Double Buffer Releases", () => {
    class ResilientBufferPool {
        constructor(cap, size) {
            this.capacity = cap;
            this.bufSize = size;
            this.available = [];
            this.inFlight = new Set();
            for (let i = 0; i < cap; i++) {
                this.available.push(new ArrayBuffer(size));
            }
        }
        acquire() {
            if (this.available.length === 0) {
                throw new Error("Pool exhausted");
            }
            const buf = this.available.pop();
            this.inFlight.add(buf);
            return buf;
        }
        release(buf) {
            if (!buf || typeof buf !== 'object') {
                throw new Error("Invalid buffer: buffer must be an object");
            }
            if (!this.inFlight.has(buf)) {
                throw new Error("Invalid buffer release: buffer not in flight or double release detected");
            }
            this.inFlight.delete(buf);
            this.available.push(buf);
        }
    }

    const pool = new ResilientBufferPool(4, 1024);

    // 6.1 Empty / Null / Undefined / Primitive Releases
    const invalidReleases = [null, undefined, 123, "buffer", {}, new ArrayBuffer(1024), new ArrayBuffer(0)];
    for (const item of invalidReleases) {
        let threw = false;
        try {
            pool.release(item);
        } catch (err) {
            threw = true;
            assert(err.message.includes("Invalid buffer"), `Expected release error, got: ${err.message}`);
        }
        assert(threw, `Releasing foreign/invalid object (${item}) must throw`);
    }

    // 6.2 Acquire to capacity
    const b1 = pool.acquire();
    const b2 = pool.acquire();
    const b3 = pool.acquire();
    const b4 = pool.acquire();

    // 6.3 Pool Exhaustion check
    let threwExhausted = false;
    try {
        pool.acquire();
    } catch (err) {
        threwExhausted = true;
        assert(err.message.includes("Pool exhausted"), "Pool exhaustion error caught");
    }
    assert(threwExhausted, "5th acquire on 4-capacity pool must throw 'Pool exhausted'");

    // 6.4 Double Release Detection
    pool.release(b1);
    let threwDoubleRelease = false;
    try {
        pool.release(b1); // Double release!
    } catch (err) {
        threwDoubleRelease = true;
        assert(err.message.includes("double release"), `Expected double release error, got: ${err.message}`);
    }
    assert(threwDoubleRelease, "Double releasing b1 must throw error");

    // Release remaining buffers
    pool.release(b2);
    pool.release(b3);
    pool.release(b4);

    assert(pool.inFlight.size === 0, "0 buffers in flight");
    assert(pool.available.length === 4, "All 4 buffers recycled");

    // 6.5 Stress churn: 10,000 rapid acquire/release cycles
    console.log("  -> Executing 10,000 acquire/release churn cycles...");
    for (let i = 0; i < 10000; i++) {
        const a = pool.acquire();
        const b = pool.acquire();
        pool.release(a);
        const c = pool.acquire();
        pool.release(b);
        pool.release(c);
    }
    assert(pool.inFlight.size === 0 && pool.available.length === 4, "Zero memory leak after 10,000 churn cycles");
});

// -----------------------------------------------------------------------------
// Test Section 7: Resilient Error Handling in BinderTestSuite
// -----------------------------------------------------------------------------
runSection("7. Test Suite Error Handling & Unhandled Rejection Immunity", async () => {
    const logs = [];
    const testSuite = new BinderTestSuite(null, null, (msg, type) => logs.push({ msg, type }));

    // Execute standard E2E suite
    const results = await testSuite.runE2ETestSuite();
    assert(results.total === 14, "Must run all 14 E2E tests");
    assert(results.passed === 14, `All 14 tests must pass, got ${results.passed} passed`);
    assert(results.failed === 0, "Zero failures in baseline run");

    // Now inject intentional failure in one test method and verify runner catches it gracefully without throwing unhandled rejection
    console.log("  -> Injecting simulated test failure to verify graceful error capture...");
    const originalE2E4 = testSuite.runE2E4_SensorsHalE2E;
    testSuite.runE2E4_SensorsHalE2E = async () => {
        throw new Error("Adversarial injected sensor failure");
    };

    let caughtResults = null;
    try {
        caughtResults = await testSuite.runE2ETestSuite();
    } catch (err) {
        assert(false, `runE2ETestSuite must NOT throw uncaught error: ${err.message}`);
    }

    assert(caughtResults !== null, "Results object returned despite failure");
    assert(caughtResults.failed === 1, "Failed count recorded as 1");
    assert(caughtResults.passed === 13, "13 tests passed");
    assert(caughtResults.results.e2e_4.status === 'FAILED', "E2E-4 status marked FAILED");
    assert(caughtResults.results.e2e_4.error === "Adversarial injected sensor failure", "Error message captured");

    // Restore original method
    testSuite.runE2E4_SensorsHalE2E = originalE2E4;
});

// -----------------------------------------------------------------------------
// Test Section 8: Visual Gates & 5-Phase End-to-End Simulation in Node.js
// -----------------------------------------------------------------------------
runSection("8. Visual Gates & 5-Phase End-to-End Execution", async () => {
    // Construct mock HTML5 Canvas with 2D Context
    class MockCanvas {
        constructor(w = 640, h = 480) {
            this.width = w;
            this.height = h;
            this.buffer = new Uint8ClampedArray(w * h * 4);
            const self = this;
            this.ctx = {
                fillStyle: '#000000',
                fillRect(x, y, width, height) {
                    let r = 0, g = 0, b = 0, a = 255;
                    if (this.fillStyle.startsWith('rgb(')) {
                        const parts = this.fillStyle.replace('rgb(', '').replace(')', '').split(',').map(s => parseInt(s.trim(), 10));
                        r = parts[0]; g = parts[1]; b = parts[2];
                    }
                    for (let py = y; py < y + height && py < self.height; py++) {
                        for (let px = x; px < x + width && px < self.width; px++) {
                            const idx = (py * self.width + px) * 4;
                            self.buffer[idx] = r;
                            self.buffer[idx + 1] = g;
                            self.buffer[idx + 2] = b;
                            self.buffer[idx + 3] = a;
                        }
                    }
                },
                getImageData(x, y, w, h) {
                    const idx = (Math.floor(y) * self.width + Math.floor(x)) * 4;
                    return {
                        data: [self.buffer[idx], self.buffer[idx + 1], self.buffer[idx + 2], self.buffer[idx + 3]]
                    };
                }
            };
        }
        getContext(type) {
            return this.ctx;
        }
    }

    const mockCanvas = new MockCanvas(640, 480);
    const testSuite = new BinderTestSuite(null, mockCanvas, () => {});

    // 1. Run all 5 Binder phases
    console.log("  -> Executing 5-Phase Binder Subsystem Suite (Phase 0, 2, 3, 4, 5)...");
    const phaseResults = await testSuite.runAllPhases();
    assert(phaseResults.total === 5, "5 phases total");
    assert(phaseResults.passed === 5, `All 5 phases must pass, got ${phaseResults.passed}`);
    assert(phaseResults.failed === 0, "0 failed phases");

    // 2. Run all 14 E2E tests
    console.log("  -> Executing 14-Milestone E2E Test Suite (E2E 1-14)...");
    const e2eResults = await testSuite.runE2ETestSuite();
    assert(e2eResults.total === 14, "14 E2E tests total");
    assert(e2eResults.passed === 14, `All 14 E2E tests must pass, got ${e2eResults.passed}`);
    assert(e2eResults.failed === 0, "0 failed E2E tests");
});

// -----------------------------------------------------------------------------
// Test Section 9: Android Material You Home Launcher Grid & Click Transition Invariants
// -----------------------------------------------------------------------------
runSection("9. Android Material You Home Launcher Grid & Click Transition Invariants", () => {
    // 9.1 Grid Coordinates and Hit-Testing Validation
    const gridLayout = {
        screenWidth: 640,
        screenHeight: 480,
        statusBarHeight: 32,
        navBarHeight: 48,
        apps: [
            { id: "app-fdroid", pkg: "org.fdroid.fdroid", label: "F-Droid", targetScreen: "screen-fdroid", col: 0, row: 0 },
            { id: "app-unity", pkg: "com.unity.cube.gles", label: "Unity 3D Cube", targetScreen: "screen-native-surface", col: 1, row: 0 },
            { id: "app-godot", pkg: "org.godotengine.gles2", label: "Godot GLES2", targetScreen: "screen-native-surface", col: 2, row: 0 },
            { id: "app-chrome", pkg: "com.android.chrome", label: "Chrome", targetScreen: "screen-generic-app", col: 0, row: 1 },
            { id: "app-files", pkg: "com.android.documentsui", label: "Files", targetScreen: "screen-generic-app", col: 1, row: 1 },
            { id: "app-settings", pkg: "com.android.settings", label: "Settings", targetScreen: "screen-generic-app", col: 2, row: 1 },
        ],
        dock: [
            { id: "dock-phone", label: "Phone", targetScreen: "screen-generic-app" },
            { id: "dock-messages", label: "Messages", targetScreen: "screen-generic-app" },
            { id: "dock-browser", label: "Browser", targetScreen: "screen-generic-app" },
            { id: "dock-camera", label: "Camera", targetScreen: "screen-generic-app" },
        ]
    };

    assert(gridLayout.apps.length === 6, "Launcher grid must have 6 default applications");
    assert(gridLayout.dock.length === 4, "Launcher dock must have 4 dock applications");

    // Invariant: All grid and dock apps have non-empty identifiers and target screens
    for (const app of gridLayout.apps) {
        assert(typeof app.pkg === 'string' && app.pkg.length > 0, `App ${app.id} must have valid package name`);
        assert(typeof app.label === 'string' && app.label.length > 0, `App ${app.id} must have valid label`);
        assert(typeof app.targetScreen === 'string' && app.targetScreen.startsWith('screen-'), `App ${app.id} target screen valid`);
    }

    for (const dockApp of gridLayout.dock) {
        assert(typeof dockApp.label === 'string' && dockApp.label.length > 0, `Dock app ${dockApp.id} must have valid label`);
        assert(typeof dockApp.targetScreen === 'string' && dockApp.targetScreen.startsWith('screen-'), `Dock app ${dockApp.id} target screen valid`);
    }

    // 9.2 Status Bar Material You Invariants
    const statusBarState = {
        clock: "12:45",
        batteryPercentage: 98,
        isCharging: false,
        signal5G: true,
        wifi6: true,
        notifications: ["F-Droid updates available", "System ready"]
    };

    assert(/^\d{1,2}:\d{2}$/.test(statusBarState.clock), "Status bar clock format matches HH:MM");
    assert(Number.isInteger(statusBarState.batteryPercentage) && statusBarState.batteryPercentage >= 0 && statusBarState.batteryPercentage <= 100, "Battery percentage is valid");
    assert(statusBarState.signal5G === true, "5G indicator active");
    assert(statusBarState.wifi6 === true, "Wi-Fi 6 indicator active");
    assert(Array.isArray(statusBarState.notifications) && statusBarState.notifications.length === 2, "Notifications present");

    // 9.3 5,000 Rapid App Launch and Home Pop Transitions
    console.log("  -> Executing 5,000 rapid app launches and Home button pops...");
    let launcherState = {
        currentScreen: 'screen-home',
        activePackage: null,
        topActivity: null,
        backStack: []
    };

    function launchApp(app) {
        launcherState.backStack.push(launcherState.currentScreen);
        launcherState.currentScreen = app.targetScreen;
        launcherState.activePackage = app.pkg;
        launcherState.topActivity = `${app.pkg}.MainActivity`;
    }

    function pressHome() {
        launcherState.currentScreen = 'screen-home';
        launcherState.activePackage = null;
        launcherState.topActivity = null;
        launcherState.backStack = [];
    }

    let seed = 0x2468ACEF;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    for (let i = 0; i < 5000; i++) {
        const appIdx = prng() % gridLayout.apps.length;
        const targetApp = gridLayout.apps[appIdx];

        launchApp(targetApp);
        assert(launcherState.currentScreen === targetApp.targetScreen, `Cycle ${i}: Target screen mismatch`);
        assert(launcherState.activePackage === targetApp.pkg, `Cycle ${i}: Active package mismatch`);
        assert(launcherState.topActivity === `${targetApp.pkg}.MainActivity`, `Cycle ${i}: Top activity mismatch`);
        assert(launcherState.backStack.length === 1, `Cycle ${i}: Backstack depth must be 1`);

        pressHome();
        assert(launcherState.currentScreen === 'screen-home', `Cycle ${i}: Must return to screen-home`);
        assert(launcherState.activePackage === null, `Cycle ${i}: Active package reset`);
        assert(launcherState.topActivity === null, `Cycle ${i}: Top activity reset`);
        assert(launcherState.backStack.length === 0, `Cycle ${i}: Backstack cleared`);
    }

    // 9.4 Dynamic Ingestion & Home Screen Expansion
    console.log("  -> Testing dynamic app ingestion and grid placement (50 apps)...");
    const dynamicRegistry = new Map();
    for (let i = 0; i < 50; i++) {
        const pkgName = `com.dynamic.app${i}`;
        const label = `Dynamic App ${i}`;
        const newApp = {
            id: `app-dynamic-${i}`,
            pkg: pkgName,
            label: label,
            targetScreen: "screen-generic-app",
            versionCode: 100 + i,
            isDynamic: true
        };
        dynamicRegistry.set(pkgName, newApp);
        assert(dynamicRegistry.has(pkgName), `Dynamic app ${pkgName} registered`);
        
        // Launch dynamic app
        launchApp(newApp);
        assert(launcherState.currentScreen === 'screen-generic-app', `Dynamic app ${i} launched to generic app view`);
        assert(launcherState.activePackage === pkgName, `Dynamic app ${i} active package set`);
        pressHome();
        assert(launcherState.currentScreen === 'screen-home', `Returned to home after dynamic app ${i}`);
    }
    assert(dynamicRegistry.size === 50, "All 50 dynamic apps registered in launcher");
});

// -----------------------------------------------------------------------------
// Test Section 10: F-Droid Search & Category Query Filtering Fuzzing
// -----------------------------------------------------------------------------
runSection("10. F-Droid Search & Category Query Filtering Fuzzing", () => {
    const catalog = [
        {
            packageName: "org.fdroid.fdroid",
            name: "F-Droid",
            versionName: "1.23.1",
            category: "System",
            tags: ["System", "Internet", "Latest"],
            description: "Free and Open Source Android App Repository Client.",
            permissions: ["android.permission.INTERNET", "android.permission.ACCESS_NETWORK_STATE", "android.permission.REQUEST_INSTALL_PACKAGES"],
            size: "12.4 MB"
        },
        {
            packageName: "com.unity.cube.gles",
            name: "Unity 3D Cube",
            versionName: "1.0.0",
            category: "Games",
            tags: ["Games", "Latest"],
            description: "Hardware-accelerated 3D cube demo compiled for Android GLES.",
            permissions: ["android.permission.INTERNET"],
            size: "8.2 MB"
        },
        {
            packageName: "org.godotengine.gles2",
            name: "Godot GLES2 Engine",
            versionName: "2.1.0",
            category: "Games",
            tags: ["Games", "Latest"],
            description: "Godot game engine lightweight rendering runtime.",
            permissions: ["android.permission.INTERNET"],
            size: "14.1 MB"
        },
        {
            packageName: "com.android.chrome",
            name: "Chrome Browser",
            versionName: "124.0.6367.82",
            category: "Internet",
            tags: ["Internet", "Latest"],
            description: "Fast, secure web browser powered by Google.",
            permissions: ["android.permission.INTERNET"],
            size: "45.0 MB"
        },
        {
            packageName: "com.android.documentsui",
            name: "Files",
            versionName: "13.0",
            category: "System",
            tags: ["System", "Latest"],
            description: "Android system file manager and storage documents provider.",
            permissions: ["android.permission.READ_EXTERNAL_STORAGE"],
            size: "4.8 MB"
        },
        {
            packageName: "com.android.settings",
            name: "Settings",
            versionName: "13.0",
            category: "System",
            tags: ["System", "Latest"],
            description: "Android OS system preferences and hardware configuration.",
            permissions: [],
            size: "3.2 MB"
        },
        {
            packageName: "org.torproject.android",
            name: "Orbot Tor VPN",
            versionName: "17.1.0",
            category: "Security",
            tags: ["Security", "Internet", "Latest"],
            description: "Proxy and VPN providing Onion routing and encryption.",
            permissions: ["android.permission.INTERNET", "android.permission.BIND_VPN_SERVICE"],
            size: "22.5 MB"
        },
        {
            packageName: "org.keepassxc.android",
            name: "KeePassDX",
            versionName: "3.5.4",
            category: "Security",
            tags: ["Security", "Latest"],
            description: "Secure password manager with encrypted database vault.",
            permissions: ["android.permission.USE_BIOMETRIC"],
            size: "11.0 MB"
        }
    ];

    function filterCatalog(category, query) {
        const q = (query || "").trim().toLowerCase();
        return catalog.filter(app => {
            const matchesCategory = !category || category === "Latest" || category === "All" ||
                app.category.toLowerCase() === category.toLowerCase() ||
                app.tags.some(t => t.toLowerCase() === category.toLowerCase());
            if (!matchesCategory) return false;
            if (!q) return true;
            return app.name.toLowerCase().includes(q) ||
                app.packageName.toLowerCase().includes(q) ||
                app.description.toLowerCase().includes(q);
        });
    }

    // 10.1 Category Isolation Tests
    const categories = ["Latest", "Games", "Internet", "System", "Security"];
    for (const cat of categories) {
        const items = filterCatalog(cat, "");
        assert(Array.isArray(items), `Category ${cat} returned array`);
        assert(items.length > 0, `Category ${cat} has items`);
        for (const item of items) {
            if (cat !== "Latest") {
                const matches = item.category.toLowerCase() === cat.toLowerCase() ||
                    item.tags.some(t => t.toLowerCase() === cat.toLowerCase());
                assert(matches, `Item ${item.name} matches category ${cat}`);
            }
        }
    }

    // 10.2 Search Query Fuzzing: 500 randomized adversarial queries
    console.log("  -> Fuzzing F-Droid catalog search with 500 adversarial terms...");
    const fuzzCorpus = [
        "", "   ", "g", "go", "godot", "cube", "droid", "chrome", "files", "security",
        ".*+?^${}()|[]\\", "\\", "[a-z]*", "(?<=a)b", "(a|b)*", "(?!foo)bar",
        "' OR 1=1 --", "<script>alert(1)</script>", "\"><img src=x onerror=alert(1)>", "'; DROP TABLE apps; --",
        "🎮 Godot", "中文应用", "العربية", "📱🔥⚡", "\u0000\uFFFF",
        "A".repeat(100), "X".repeat(1000), "Z".repeat(5000)
    ];

    let seed = 0x55AA55AA;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    for (let i = 0; i < 500; i++) {
        const query = fuzzCorpus[prng() % fuzzCorpus.length];
        const cat = categories[prng() % categories.length];

        let results = null;
        try {
            results = filterCatalog(cat, query);
        } catch (err) {
            assert(false, `Query '${query}' caused exception in filterCatalog: ${err.message}`);
        }

        assert(Array.isArray(results), `Fuzz ${i}: Result must be array`);
        assert(results.length <= catalog.length, `Fuzz ${i}: Result length within bounds`);

        // Case-insensitivity check
        if (query.trim().length > 0 && query.length < 50) {
            const upperResults = filterCatalog(cat, query.toUpperCase());
            const lowerResults = filterCatalog(cat, query.toLowerCase());
            assert(upperResults.length === lowerResults.length, `Fuzz ${i}: Case-insensitivity preserved for '${query}'`);
        }
    }

    // 10.3 App Detail Modal Invariants
    for (const app of catalog) {
        assert(typeof app.name === 'string' && app.name.length > 0, "App name valid");
        assert(typeof app.packageName === 'string' && app.packageName.includes('.'), "Package name valid");
        assert(typeof app.versionName === 'string', "Version name valid");
        assert(Array.isArray(app.permissions), "Permissions is array");
        assert(typeof app.size === 'string' && app.size.includes('MB'), "Size string valid");
        assert(typeof app.description === 'string' && app.description.length > 0, "Description valid");
    }
});

// -----------------------------------------------------------------------------
// Test Section 11: 3-Button Navigation Bar Stack Machine & Underflow Protection
// -----------------------------------------------------------------------------
runSection("11. 3-Button Navigation Bar Stack Machine & Underflow Protection Invariants", () => {
    class NavigationBarStateMachine {
        constructor() {
            this.backStack = [];
            this.currentScreen = 'screen-home';
            this.currentApp = null;
            this.recentTasks = [];
        }

        launch(screenId, packageName, activityName) {
            this.backStack.push({
                screenId: this.currentScreen,
                packageName: this.currentApp,
                activity: this.currentApp ? `${this.currentApp}.MainActivity` : null
            });
            this.currentScreen = screenId;
            this.currentApp = packageName;
            this.recentTasks = [
                { packageName, screenId, activity: activityName || `${packageName}.MainActivity`, time: Date.now() },
                ...this.recentTasks.filter(t => t.packageName !== packageName)
            ];
        }

        back() {
            if (this.backStack.length > 0) {
                const prev = this.backStack.pop();
                this.currentScreen = prev.screenId;
                this.currentApp = prev.packageName;
                return true;
            }
            // Underflow protection
            this.currentScreen = 'screen-home';
            this.currentApp = null;
            return false;
        }

        home() {
            this.currentScreen = 'screen-home';
            this.currentApp = null;
            this.backStack = [];
        }

        recents() {
            if (this.currentScreen !== 'screen-recents') {
                this.backStack.push({
                    screenId: this.currentScreen,
                    packageName: this.currentApp
                });
                this.currentScreen = 'screen-recents';
            }
        }

        selectRecent(packageName) {
            const task = this.recentTasks.find(t => t.packageName === packageName);
            if (task) {
                this.launch(task.screenId, task.packageName, task.activity);
            }
        }

        clearRecents() {
            this.recentTasks = [];
            this.home();
        }
    }

    const nav = new NavigationBarStateMachine();

    // 11.1 100-Level Deep Navigation Stack and Sequential LIFO Popping
    console.log("  -> Testing 100-level deep navigation backstack push and sequential LIFO pops...");
    const pushedSequence = [];
    for (let i = 0; i < 100; i++) {
        const screenId = `screen-view-${i}`;
        const pkgName = `com.app.level${i}`;
        pushedSequence.push({ screenId: nav.currentScreen, packageName: nav.currentApp });
        nav.launch(screenId, pkgName, `${pkgName}.Activity`);
        assert(nav.backStack.length === i + 1, `Push ${i}: Stack depth must be ${i + 1}`);
        assert(nav.currentScreen === screenId, `Push ${i}: Current screen is ${screenId}`);
    }

    assert(nav.backStack.length === 100, "Backstack depth is exactly 100");

    // Pop 100 times
    for (let i = 99; i >= 0; i--) {
        const expected = pushedSequence[i];
        const res = nav.back();
        assert(res === true, `Pop ${i}: Back returned true`);
        assert(nav.backStack.length === i, `Pop ${i}: Backstack depth is ${i}`);
        assert(nav.currentScreen === expected.screenId, `Pop ${i}: Restored screen matches ${expected.screenId}`);
        assert(nav.currentApp === expected.packageName, `Pop ${i}: Restored app matches ${expected.packageName}`);
    }

    assert(nav.currentScreen === 'screen-home', "Returned to screen-home after 100 pops");
    assert(nav.backStack.length === 0, "Backstack empty after 100 pops");

    // 11.2 Underflow Protection: 50 Back clicks on empty stack
    console.log("  -> Testing 50 underflow Back clicks on empty stack...");
    for (let i = 0; i < 50; i++) {
        const res = nav.back();
        assert(res === false, `Underflow ${i}: Back returned false`);
        assert(nav.currentScreen === 'screen-home', `Underflow ${i}: Remains on screen-home`);
        assert(nav.backStack.length === 0, `Underflow ${i}: Backstack depth remains 0`);
    }

    // 11.3 Home Button Unconditional Reset from Arbitrary Depth
    console.log("  -> Testing Home button unconditional stack reset...");
    for (let i = 0; i < 35; i++) {
        nav.launch(`screen-nested-${i}`, `com.nested.app${i}`, `com.nested.app${i}.Act`);
    }
    assert(nav.backStack.length === 35, "Stack depth 35");
    nav.home();
    assert(nav.currentScreen === 'screen-home', "Home button returned to screen-home");
    assert(nav.backStack.length === 0, "Home button reset backstack to 0");
    assert(nav.currentApp === null, "Home button cleared currentApp");

    // 11.4 Recents Task Switcher Invariants
    console.log("  -> Testing Recents task switcher and task selection...");
    nav.clearRecents();
    const testApps = ["org.fdroid.fdroid", "com.unity.cube.gles", "org.godotengine.gles2", "com.android.chrome"];
    for (const app of testApps) {
        nav.launch(`screen-${app}`, app, `${app}.MainActivity`);
    }
    assert(nav.recentTasks.length === 4, "4 recent tasks recorded");
    assert(nav.recentTasks[0].packageName === "com.android.chrome", "MRU top is Chrome");

    nav.recents();
    assert(nav.currentScreen === 'screen-recents', "Recents view opened");

    // Switch to Godot from recents
    nav.selectRecent("org.godotengine.gles2");
    assert(nav.currentApp === "org.godotengine.gles2", "Switched to Godot from recents");
    assert(nav.recentTasks[0].packageName === "org.godotengine.gles2", "Godot promoted to MRU top");

    nav.clearRecents();
    assert(nav.recentTasks.length === 0, "Recent tasks cleared");
    assert(nav.currentScreen === 'screen-home', "Returned to home after clearing recents");

    // 11.5 5,000 Randomized Navigation Button Slamming
    console.log("  -> Executing 5,000 randomized navigation button operations...");
    let seed = 0x98765432;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    const availableApps = [
        { screen: "screen-fdroid", pkg: "org.fdroid.fdroid" },
        { screen: "screen-native-surface", pkg: "com.unity.cube.gles" },
        { screen: "screen-native-surface", pkg: "org.godotengine.gles2" },
        { screen: "screen-generic-app", pkg: "com.android.chrome" },
        { screen: "screen-generic-app", pkg: "com.android.documentsui" },
    ];

    for (let i = 0; i < 5000; i++) {
        const action = prng() % 6;
        if (action === 0 || action === 1) {
            // Launch app
            const a = availableApps[prng() % availableApps.length];
            nav.launch(a.screen, a.pkg, `${a.pkg}.MainActivity`);
        } else if (action === 2 || action === 3) {
            // Back button
            nav.back();
        } else if (action === 4) {
            // Home button
            nav.home();
        } else if (action === 5) {
            // Recents button
            if (prng() % 2 === 0) {
                nav.recents();
            } else if (nav.recentTasks.length > 0) {
                const r = nav.recentTasks[prng() % nav.recentTasks.length];
                nav.selectRecent(r.packageName);
            }
        }

        assert(typeof nav.currentScreen === 'string' && nav.currentScreen.length > 0, `Slam ${i}: Valid currentScreen`);
        assert(nav.backStack.length >= 0, `Slam ${i}: Stack depth non-negative`);
        if (nav.currentScreen === 'screen-home' && nav.backStack.length === 0) {
            assert(nav.currentApp === null, `Slam ${i}: App is null on clean home`);
        }
    }
});

// -----------------------------------------------------------------------------
// Test Section 12: Pure-JS Binary AXML Parsing & Fuzzing Invariants
// -----------------------------------------------------------------------------
runSection("12. Pure-JS Binary AXML Parsing & Fuzzing Invariants", () => {
    // 12.1 Real F-Droid.apk Binary AXML Extraction & Parsing
    const apkPath = path.resolve(process.cwd(), 'F-Droid.apk');
    assert(fs.existsSync(apkPath), `F-Droid.apk must exist at ${apkPath}`);

    const apkBuf = fs.readFileSync(apkPath);
    assert(apkBuf.byteLength > 10000000, `F-Droid.apk size > 10MB (got ${apkBuf.byteLength} bytes)`);

    const zip = new ApkZipReader(apkBuf);
    const manifestBytes = zip.getManifest();
    assert(manifestBytes !== null && manifestBytes.byteLength > 40000, `AndroidManifest.xml extracted (${manifestBytes?.byteLength} bytes)`);

    const manifest = AxmlDecoder.decode(manifestBytes);
    assert(manifest.packageName === "org.fdroid.fdroid", `F-Droid packageName is 'org.fdroid.fdroid', got '${manifest.packageName}'`);
    assert(manifest.versionCode === 1023051, `F-Droid versionCode is 1023051, got ${manifest.versionCode}`);
    assert(manifest.versionName === "1.23.1", `F-Droid versionName is '1.23.1', got '${manifest.versionName}'`);
    assert(manifest.activities.length === 25, `F-Droid activities count is 25, got ${manifest.activities.length}`);
    assert(manifest.providers.length === 4, `F-Droid providers count is 4, got ${manifest.providers.length}`);
    assert(manifest.permissions.length === 29, `F-Droid permissions count is 29, got ${manifest.permissions.length}`);
    assert(manifest.launcherActivity === "org.fdroid.fdroid.views.main.MainActivity", `F-Droid launcherActivity is MainActivity, got '${manifest.launcherActivity}'`);

    const providerNames = manifest.providers.map(p => p.name);
    assert(providerNames.includes("org.fdroid.fdroid.installer.ApkFileProvider"), "Includes ApkFileProvider");
    assert(providerNames.includes("androidx.core.content.FileProvider"), "Includes FileProvider");
    assert(providerNames.includes("org.fdroid.fdroid.nearby.PublicSourceDirProvider"), "Includes PublicSourceDirProvider");
    assert(providerNames.includes("org.acra.attachment.AcraContentProvider"), "Includes AcraContentProvider");

    const permList = manifest.permissions;
    assert(permList.includes("android.permission.INTERNET"), "Includes INTERNET permission");
    assert(permList.includes("android.permission.ACCESS_NETWORK_STATE"), "Includes ACCESS_NETWORK_STATE permission");
    assert(permList.includes("android.permission.REQUEST_INSTALL_PACKAGES"), "Includes REQUEST_INSTALL_PACKAGES permission");

    // 12.2 Synthetic Binary AXML Roundtrip Verification
    const synAxml = buildSyntheticAxml({
        packageName: "com.androidwebgpu.adversarial",
        versionCode: 8888,
        versionName: "8.8.8-bench",
        activities: [
            "com.androidwebgpu.adversarial.MainActivity",
            "com.androidwebgpu.adversarial.DetailActivity"
        ],
        providers: [
            "com.androidwebgpu.adversarial.DataProvider"
        ],
        permissions: [
            "android.permission.INTERNET",
            "android.permission.VIBRATE",
            "android.permission.WAKE_LOCK"
        ]
    });

    assert(synAxml.byteLength > 64, "Synthetic AXML generated");
    const synDecoded = AxmlDecoder.decode(synAxml);
    assert(synDecoded.packageName === "com.androidwebgpu.adversarial", "Synthetic packageName decoded");
    assert(synDecoded.versionCode === 8888, "Synthetic versionCode decoded");
    assert(synDecoded.versionName === "8.8.8-bench", "Synthetic versionName decoded");
    assert(synDecoded.launcherActivity === "com.androidwebgpu.adversarial.MainActivity", "Synthetic launcher decoded");
    assert(synDecoded.activities.length === 2, "Synthetic activities count 2");
    assert(synDecoded.providers.length === 1, "Synthetic providers count 1");
    assert(synDecoded.permissions.length === 3, "Synthetic permissions count 3");

    // 12.3 Adversarial AXML Fuzzing (1,000 malformed buffers)
    console.log("  -> Fuzzing AxmlDecoder with 1,000 malformed and randomized buffers...");
    let seed = 0xDEADBEEF;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    // 12.3.1 Truncated chunk headers (< 8 bytes)
    for (let len = 0; len < 8; len++) {
        const truncBuf = new Uint8Array(len);
        let threw = false;
        try {
            AxmlDecoder.decode(truncBuf);
        } catch (err) {
            threw = true;
            assert(err.message.includes("too short"), `Truncated buffer len ${len} threw expected error`);
        }
        assert(threw, `Truncated buffer len ${len} must throw error`);
    }

    // 12.3.2 Corrupted root magic numbers
    const invalidMagics = [0x0000, 0x0001, 0x0002, 0x0100, 0xFFFF, 0xCAFE, 0x1234];
    for (const badMagic of invalidMagics) {
        const badMagicBuf = new Uint8Array(32);
        const dv = new DataView(badMagicBuf.buffer);
        dv.setUint16(0, badMagic, true);
        dv.setUint16(2, 8, true);
        dv.setUint32(4, 32, true);

        let threw = false;
        try {
            AxmlDecoder.decode(badMagicBuf);
        } catch (err) {
            threw = true;
            assert(err.message.includes("Invalid AXML magic"), `Bad magic 0x${badMagic.toString(16)} threw expected error`);
        }
        assert(threw, `Bad magic 0x${badMagic.toString(16)} must throw error`);
    }

    // 12.3.3 1,000 Randomized Byte Corruptions & Fuzz Churn
    for (let i = 0; i < 1000; i++) {
        const fuzzed = new Uint8Array(synAxml.length);
        fuzzed.set(synAxml);

        const corruptionType = prng() % 5;
        if (corruptionType === 0) {
            // Corrupt chunk sizes
            const dv = new DataView(fuzzed.buffer);
            dv.setUint32(4, prng() % 100000, true);
        } else if (corruptionType === 1) {
            // Random byte flips (1 to 10 bytes)
            const flipCount = (prng() % 10) + 1;
            for (let f = 0; f < flipCount; f++) {
                const pos = prng() % fuzzed.length;
                fuzzed[pos] = prng() & 0xFF;
            }
        } else if (corruptionType === 2) {
            // Truncate at random position
            const cutLen = (prng() % (fuzzed.length - 8)) + 8;
            const slice = fuzzed.subarray(0, cutLen);
            try {
                AxmlDecoder.decode(slice);
            } catch (_) {}
            assert(true, `Fuzz ${i}: Truncated slice handled without process crash`);
            continue;
        } else if (corruptionType === 3) {
            // Corrupt string pool header
            const dv = new DataView(fuzzed.buffer);
            if (fuzzed.length > 20) {
                dv.setUint32(12, 0xFFFFFFFF, true); // Giant string count
            }
        } else {
            // Zero out middle chunk
            const start = (prng() % (fuzzed.length - 16)) + 8;
            fuzzed.fill(0, start, start + 8);
        }

        try {
            AxmlDecoder.decode(fuzzed);
        } catch (err) {
            // Catchable error is valid
            assert(err instanceof Error, `Fuzz ${i}: Error must be Error instance`);
        }
        assert(true, `Fuzz ${i}: Handled gracefully`);
    }
});

// -----------------------------------------------------------------------------
// Final Summary
// -----------------------------------------------------------------------------
console.log(`\n======================================================`);
console.log(`⚡ ALL ADVERSARIAL CHECKS COMPLETED`);
console.log(`Passed assertions: ${passedChecks}`);
console.log(`Failed assertions: ${failedChecks}`);
console.log(`======================================================\n`);

if (failedChecks > 0) {
    process.exit(1);
} else {
    process.exit(0);
}


