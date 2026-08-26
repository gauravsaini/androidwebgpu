/**
 * tests/adversarial_virtio_logging_stress.mjs
 * 
 * Adversarial stress test for Virtio-GPU logging discipline:
 * 1. Simulates 10,000 scanout damage rect events in VirtioGpuDevice.
 * 2. Asserts ZERO [I] info logs are produced during hot-path render execution.
 * 3. Asserts high-frequency scanout events produce strictly [D] debug logs.
 * 4. Asserts discrete state events (device init, v86 attach, state transitions) produce [I] logs.
 * 5. Fuzzes damage rect dimensions and boundaries across 10,000 cycles.
 * 
 * Conforms to ASD-STE100 and /ponytail simplicity principles.
 */

import { logger, globalLogcat, StructuredLogger, LogcatBuffer } from '../src/logger.js';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (condition) {
        passed++;
        console.log(`  ✔ [PASS] ${message}`);
    } else {
        failed++;
        console.error(`  ✖ [FAIL] ${message}`);
        throw new Error(`Assertion failed: ${message}`);
    }
}

console.log('⚡ Starting Adversarial Virtio-GPU Logging Stress Test (10,000 cycles)...\n');

// 1. Setup Mock Environment
const WIDTH = 800;
const HEIGHT = 600;
const FB_SIZE = WIDTH * HEIGHT * 4;
const mockFb = new Uint8Array(FB_SIZE);

let currentDamageRect = [0, 0, WIDTH, HEIGHT];
let damageClearedCount = 0;

const mockBridge = {
    get_scanout_framebuffer: (id) => mockFb,
    get_scanout_damage: (id) => currentDamageRect,
    clear_scanout_damage: (id) => { damageClearedCount++; },
    process_command_packet: (pkt) => new Uint8Array([0x00, 0x11, 0x00, 0x00])
};

const mockCanvas = {
    width: WIDTH,
    height: HEIGHT,
    getContext: (type, opts) => ({
        createImageData: (w, h) => ({
            width: w,
            height: h,
            data: new Uint8Array(w * h * 4)
        }),
        putImageData: (imgData, dx, dy, dirtyX, dirtyY, dirtyW, dirtyH) => {}
    })
};

const mockV86 = {
    cpu: {
        devices: {},
        pci: {
            register_device: (slot, dev) => {}
        },
        io: {
            register_read: (port, size, fn) => {},
            register_write: (port, size, fn) => {}
        }
    }
};

// 2. Discrete State Event Verification
console.log('▶ Section 1: Discrete State Transitions Emit [I]');
logger.clear();
globalLogcat.clear();

const device = new VirtioGpuDevice(null, mockBridge, mockCanvas);

const initLogs = logger.logs.filter(l => l.subsystem === 'bridge' && l.level === 'I');
assert(initLogs.length === 1, 'Virtio-GPU device initialization emits exactly 1 [I] log entry');
assert(initLogs[0].message.includes('Virtio-GPU PCI device'), 'Init log message specifies PCI device');

device.registerWithV86(mockV86);
const attachLogs = logger.logs.filter(l => l.subsystem === 'bridge' && l.level === 'I' && l.message.includes('attached to v86'));
assert(attachLogs.length === 1, 'Virtio-GPU attach to v86 emits exactly 1 [I] log entry');

// 3. Hot-Path 10,000 Scanout Damaged Rect Adversarial Stress Loop
console.log('\n▶ Section 2: 10,000 Scanout Damage Rect Events Produce ZERO [I] Logs');

const logCounts = { V: 0, D: 0, I: 0, W: 0, E: 0 };
const listener = (entry) => {
    if (entry.subsystem === 'bridge') {
        logCounts[entry.level] = (logCounts[entry.level] || 0) + 1;
    }
};

logger.addListener(listener);

const ITERATIONS = 10000;
const startTime = performance.now();

for (let i = 0; i < ITERATIONS; i++) {
    // Generate fuzzed damage rect patterns
    const pattern = i % 5;
    if (pattern === 0) {
        // Full screen damage
        currentDamageRect = [0, 0, WIDTH, HEIGHT];
    } else if (pattern === 1) {
        // Top status bar damage
        currentDamageRect = [0, 0, WIDTH, 24];
    } else if (pattern === 2) {
        // Dynamic subrect animation damage
        const rx = (i * 17) % (WIDTH - 100);
        const ry = (i * 23) % (HEIGHT - 100);
        currentDamageRect = [rx, ry, 100, 100];
    } else if (pattern === 3) {
        // Boundary clamp edge case (exceeds width/height)
        currentDamageRect = [WIDTH - 10, HEIGHT - 10, 50, 50];
    } else {
        // Null damage fallback (full blit)
        currentDamageRect = null;
    }

    device.renderScanoutToCanvas(0);
}

const elapsedMs = performance.now() - startTime;
logger.removeListener(listener);

console.log(`  Completed ${ITERATIONS} cycles in ${elapsedMs.toFixed(2)}ms (${(ITERATIONS / (elapsedMs / 1000)).toFixed(0)} fps equiv)`);
console.log(`  Emitted Bridge Logs: D=${logCounts.D}, I=${logCounts.I}, W=${logCounts.W}, E=${logCounts.E}`);

assert(logCounts.I === 0, `ZERO [I] logs produced during 10,000 scanout damage rect events (actual: ${logCounts.I})`);
assert(logCounts.D === ITERATIONS, `All ${ITERATIONS} scanout events emitted level [D] logs (actual: ${logCounts.D})`);
assert(logCounts.W === 0, `ZERO warning logs produced during valid rendering (actual: ${logCounts.W})`);
assert(logCounts.E === 0, `ZERO error logs produced during rendering (actual: ${logCounts.E})`);
assert(damageClearedCount === 8000, `Damage cleared for all 8,000 non-null damage iterations (actual: ${damageClearedCount})`);
assert(device.damage_rects_count === 8000, `Damage rect count correctly incremented (actual: ${device.damage_rects_count})`);

// 4. Verification that discrete events still work after stress loop
console.log('\n▶ Section 3: Post-Stress Discrete Logging Verification');
const postEvent = logger.log('bridge', 'I', 'Discrete state milestone post-render');
assert(postEvent.level === 'I', 'Post-stress discrete state event emitted as [I]');
assert(logger.logs[logger.logs.length - 1].level === 'I', 'Latest log entry in history is level [I]');

// 5. Memory & Buffer Stability
console.log('\n▶ Section 4: Buffer Capacity & Memory Stability');
assert(logger.logs.length <= logger.maxLogHistory, `StructuredLogger capped at maxLogHistory (${logger.maxLogHistory})`);

console.log('\n======================================================');
console.log(`⚡ ALL ADVERSARIAL VIRTIO LOGGING TESTS PASSED! (${passed} assertions, 0 failed)`);
console.log('======================================================');
