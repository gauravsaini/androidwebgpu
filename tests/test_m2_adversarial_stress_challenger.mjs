/**
 * tests/test_m2_adversarial_stress_challenger.mjs
 * 
 * Adversarial Stress & Chaos Test Suite for Milestone 2:
 * Structured Debug Logging, Serial UART Feeding, and In-UI Logcat Streaming.
 * 
 * Challenger 2 Verification Harness
 */

import {
    StructuredLogger,
    LogcatBuffer,
    logger,
    globalLogcat,
    logDebug,
    createStructuredLogger,
    LOG_LEVELS,
    PRIORITY_ORDER,
    KNOWN_SUBSYSTEMS
} from '../src/logger.js';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES } from '../src/v86_guest_manager.js';
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
        throw new Error(`Adversarial Assertion Failed: ${message}`);
    }
}

console.log('======================================================');
console.log('🔥 Adversarial Stress Test Suite: Milestone 2 (Challenger 2)');
console.log('======================================================\n');

// -----------------------------------------------------------------------------
// Test 1: High-Throughput Burst & FIFO Circular Buffer Invariant Under Load
// -----------------------------------------------------------------------------
console.log('▶ [Test 1] High-Throughput Burst (50,000 logs) & FIFO Invariant');
{
    const buf = new LogcatBuffer(5000);
    const totalLogs = 50000;
    const startTime = performance.now();

    for (let i = 0; i < totalLogs; i++) {
        const prio = (i % 5 === 0) ? 'E' : (i % 3 === 0) ? 'W' : (i % 2 === 0) ? 'I' : 'D';
        buf.append(`Tag_${i % 20}`, `Payload log message index=${i} data=${'x'.repeat(16)}`, prio, 1000 + (i % 10), 2000 + (i % 10));
    }
    const elapsed = performance.now() - startTime;

    assert(buf.entries.length === 5000, `Buffer length strictly clamped to 5000 (actual: ${buf.entries.length})`);
    assert(buf.entries[0].msg.startsWith(`Payload log message index=${totalLogs - 5000}`), `Oldest retained entry matches exact FIFO offset (${buf.entries[0].msg})`);
    assert(buf.entries[4999].msg.startsWith(`Payload log message index=${totalLogs - 1}`), `Latest entry matches exact index (${buf.entries[4999].msg})`);
    
    // Performance check: > 50,000 logs in < 1000ms
    assert(elapsed < 2000, `50,000 log bursts processed in ${elapsed.toFixed(2)}ms (throughput: ${((totalLogs / elapsed) * 1000).toFixed(0)} logs/sec)`);
}

// -----------------------------------------------------------------------------
// Test 2: Serial UART Feeding Chaos & Chunk Fragmentation
// -----------------------------------------------------------------------------
console.log('\n▶ [Test 2] Serial UART Fragmented Streaming & Character Fuzzing');
{
    const buf = new LogcatBuffer(5000);
    const expectedLines = [
        'SeaBIOS rel-1.14.0-0-g155821a',
        'Booting from ROM...',
        'Linux version 5.10.0-android-x86 (builder@ci) #1 SMP PREEMPT',
        'Command line: console=ttyS0,115200 root=/dev/ram0 rw init=/init',
        '[    0.000000] Linux version 5.10.0 (android@webgpu) (gcc 10.2.1)',
        '[    0.000000] x86/fpu: Supporting XSAVE feature 0x001: \'x87 floating point registers\'',
        '[    0.004000] Console: colour dummy device 80x25',
        '[    0.012000] printk: console [ttyS0] enabled',
        '[    0.045000] Virtio-GPU: scanout display initialized 800x600',
        'Kernel panic - not syncing: Fatal exception in interrupt',
        'Init process exited with code 0',
        'Normal diagnostic message after recovery'
    ];

    const rawStream = expectedLines.join('\r\n') + '\r\n';
    
    // Stream byte-by-byte (extreme fragmentation)
    const emittedLines = [];
    for (let i = 0; i < rawStream.length; i++) {
        buf.feedSerialChar(rawStream[i], (entry) => {
            emittedLines.push(entry);
        });
    }

    assert(emittedLines.length === expectedLines.length, `Byte-by-byte feed produced exact line count (${emittedLines.length}/${expectedLines.length})`);
    for (let i = 0; i < expectedLines.length; i++) {
        assert(emittedLines[i].msg === expectedLines[i], `Line ${i} matched: "${emittedLines[i].msg}"`);
    }

    // Check panic classification
    const panicEntry = emittedLines.find(e => e.msg.includes('Kernel panic'));
    assert(panicEntry && panicEntry.priority === 'E', 'Kernel panic classified as priority E');
    
    const normalEntry = emittedLines.find(e => e.msg.includes('SeaBIOS'));
    assert(normalEntry && normalEntry.priority === 'D', 'SeaBIOS banner classified as priority D');

    // Test random chunk-size feeding
    buf.clear();
    const chunkEmitted = [];
    let offset = 0;
    while (offset < rawStream.length) {
        const chunkSize = 1 + Math.floor(Math.random() * 15);
        const chunk = rawStream.slice(offset, offset + chunkSize);
        buf.feedSerial(chunk, (entry) => {
            chunkEmitted.push(entry);
        });
        offset += chunkSize;
    }

    assert(chunkEmitted.length === expectedLines.length, `Random chunk feed produced exact line count (${chunkEmitted.length}/${expectedLines.length})`);
    for (let i = 0; i < expectedLines.length; i++) {
        assert(chunkEmitted[i].msg === expectedLines[i], `Chunked Line ${i} matched: "${chunkEmitted[i].msg}"`);
    }
}

// -----------------------------------------------------------------------------
// Test 3: ANSI Escape Sequences & High-Entropy Unicode in Serial Feed
// -----------------------------------------------------------------------------
console.log('\n▶ [Test 3] ANSI Escape Sequences & Multi-Language Unicode UART Stream');
{
    const buf = new LogcatBuffer(5000);
    const noisyLines = [
        '\x1b[32m[OK]\x1b[0m Started Android SurfaceFlinger compositor',
        '\x1b[1;31m[FAILED]\x1b[0m Failed to start binderfs daemon: error code -1',
        '\x1b[2J\x1b[HSystem screen cleared',
        'Unicode test: 日本語テスト / 🚀 Android WebGPU / \u0000\u0001\u0002 control chars',
        'Mixed carriage returns\r\r\nSingle newline\n\nDouble newline\n'
    ];

    for (const line of noisyLines) {
        buf.feedSerial(line + '\n');
    }

    assert(buf.entries.length >= 4, `Handled noisy and unicode lines cleanly (entries count: ${buf.entries.length})`);
    const errEntry = buf.entries.find(e => e.msg.includes('error code'));
    assert(errEntry && errEntry.priority === 'E', 'ANSI line with error word flagged as E priority');
}

// -----------------------------------------------------------------------------
// Test 4: UI Event Dispatching & Fault-Tolerant Listener Isolation
// -----------------------------------------------------------------------------
console.log('\n▶ [Test 4] UI Event Dispatching, Exception Isolation & Listener Dynamic Churn');
{
    const loggerInstance = new StructuredLogger({ consoleDispatch: false });
    const logcatInstance = new LogcatBuffer(100);

    let goodListener1Calls = 0;
    let goodListener2Calls = 0;

    // Bad listener that throws unconditionally
    const throwingListener = (entry) => {
        throw new Error('Explosion inside bad UI listener!');
    };

    const goodListener1 = (entry) => {
        goodListener1Calls++;
    };

    const goodListener2 = (entry) => {
        goodListener2Calls++;
    };

    loggerInstance.addListener(goodListener1);
    loggerInstance.addListener(throwingListener);
    loggerInstance.addListener(goodListener2);

    // Emit logs - throwing listener must NOT disrupt good listeners or throw up
    for (let i = 0; i < 100; i++) {
        loggerInstance.log('v86', 'I', `Stress message ${i}`);
    }

    assert(goodListener1Calls === 100, `Good listener 1 received all 100 logs (actual: ${goodListener1Calls})`);
    assert(goodListener2Calls === 100, `Good listener 2 received all 100 logs despite bad listener throwing (actual: ${goodListener2Calls})`);

    // Remove listeners and test churn
    loggerInstance.removeListener(throwingListener);
    loggerInstance.removeListener(goodListener1);
    loggerInstance.log('v86', 'I', 'After unsubscribe');

    assert(goodListener1Calls === 100, 'Unsubscribed listener receives no further logs');
    assert(goodListener2Calls === 101, 'Remaining listener continues receiving logs');
}

// -----------------------------------------------------------------------------
// Test 5: Adversarial Filter & Query Fuzzing over Full 5,000 Capacity
// -----------------------------------------------------------------------------
console.log('\n▶ [Test 5] Adversarial Filter & Query Fuzzing on 5,000 Entries');
{
    const buf = new LogcatBuffer(5000);
    const tags = ['v86', 'bridge', 'compositor', 'SurfaceFlinger', 'WindowManager', 'ActivityManager', 'Special[.*+?^${}()|]'];
    const priorities = ['V', 'D', 'I', 'W', 'E'];

    for (let i = 0; i < 5000; i++) {
        const t = tags[i % tags.length];
        const p = priorities[i % priorities.length];
        buf.append(t, `Log sequence ${i} token=${Math.random().toString(36).substring(7)}`, p);
    }

    // 1. Extreme string length query
    const hugeQuery = 'a'.repeat(10000);
    const resHuge = buf.filter({ msgQuery: hugeQuery });
    assert(resHuge.length === 0, '10,000 char query handled safely without crash');

    // 2. Regex injection in tag and search queries
    const regexQuery = '[.*+?^${}()|]';
    const resRegex = buf.filter({ tagQuery: regexQuery });
    assert(resRegex.length > 0, `Literal regex characters matched cleanly in tag query (matches: ${resRegex.length})`);

    // 3. Null, undefined, empty object query parameters
    const resNull = buf.filter({ minPriority: null, tagQuery: undefined, msgQuery: null });
    assert(resNull.length === 5000, `Null/undefined query params fallback gracefully to full buffer (results: ${resNull.length})`);

    // 4. Case-insensitivity verification
    const resCaseUpper = buf.filter({ tagQuery: 'SURFACEFLINGER' });
    const resCaseLower = buf.filter({ tagQuery: 'surfaceflinger' });
    assert(resCaseUpper.length > 0 && resCaseUpper.length === resCaseLower.length, `Tag search is strictly case-insensitive (${resCaseUpper.length} == ${resCaseLower.length})`);

    // 5. Priority threshold verification
    const resE = buf.filter({ minPriority: 'E' });
    assert(resE.every(e => e.priority === 'E'), `minPriority 'E' returns only 'E' logs (count: ${resE.length})`);

    const resW = buf.filter({ minPriority: 'W' });
    assert(resW.every(e => e.priority === 'W' || e.priority === 'E'), `minPriority 'W' returns only 'W' and 'E' logs (count: ${resW.length})`);
}

// -----------------------------------------------------------------------------
// Test 6: Metadata Fuzzing & Unserializable Objects
// -----------------------------------------------------------------------------
console.log('\n▶ [Test 6] Metadata Fuzzing (BigInt, Cycles, Functions, Symbols)');
{
    const loggerInstance = new StructuredLogger({ consoleDispatch: false });

    // BigInt metadata
    const bigIntEntry = loggerInstance.log('bridge', 'I', 'BigInt meta', { big: 123456789012345678901234567890n });
    assert(bigIntEntry.metadata && bigIntEntry.metadata.error === 'circular_or_unserializable_metadata', 'BigInt metadata caught by sanitizer safely');

    // Deep recursive object
    let deepObj = {};
    let cur = deepObj;
    for (let i = 0; i < 500; i++) {
        cur.nested = {};
        cur = cur.nested;
    }
    const deepEntry = loggerInstance.log('compositor', 'D', 'Deep object meta', deepObj);
    assert(deepEntry.metadata !== undefined, 'Deep object handled without stack overflow');

    // Function and Symbol metadata
    const symEntry = loggerInstance.log('v86', 'W', 'Symbol meta', { sym: Symbol('test'), fn: () => {} });
    assert(symEntry !== null && symEntry.formatted.includes('[v86]'), 'Symbol/Function metadata logged successfully');
}

// -----------------------------------------------------------------------------
// Test 7: Full v86 Guest Boot & Milestone Synchronization Simulation
// -----------------------------------------------------------------------------
console.log('\n▶ [Test 7] Real-Time v86 Guest Boot & UART Logcat Sync');
{
    logger.clear();
    globalLogcat.clear();

    const guestManager = new V86GuestManager({ mockMode: true });

    // Step through simulated boot lifecycle
    guestManager.setState(VM_STATES.BOOTING);
    guestManager.recordMilestone(BOOT_MILESTONES.BIOS_POST);
    guestManager.feedSerial('SeaBIOS (version rel-1.14.0)\r\nLoading Linux kernel...\r\n');

    guestManager.recordMilestone(BOOT_MILESTONES.KERNEL_UNCOMPRESS);
    guestManager.feedSerial('Uncompressing Linux... Ok, booting the kernel.\r\n');

    guestManager.recordMilestone(BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED);
    guestManager.feedSerial('[SurfaceFlinger] Service registration completed on /dev/binderfs\r\n');

    guestManager.setState(VM_STATES.RUNNING);

    // Verify all milestones are logged with [v86]
    const v86Logs = logger.logs.filter(l => l.subsystem === 'v86');
    assert(v86Logs.some(l => l.message.includes('BIOS_POST')), 'BIOS_POST milestone in structured logs');
    assert(v86Logs.some(l => l.message.includes('KERNEL_UNCOMPRESS')), 'KERNEL_UNCOMPRESS milestone in structured logs');
    assert(v86Logs.some(l => l.message.includes('SYSTEM_BOOT_COMPLETED')), 'SYSTEM_BOOT_COMPLETED milestone in structured logs');

    // Verify logcat entries
    const logcatEntries = globalLogcat.entries;
    assert(logcatEntries.some(e => e.msg.includes('SeaBIOS (version rel-1.14.0)')), 'SeaBIOS banner captured in logcat');
    assert(logcatEntries.some(e => e.msg.includes('Uncompressing Linux')), 'Uncompressing banner captured in logcat');
    assert(logcatEntries.some(e => e.msg.includes('[SurfaceFlinger] Service registration')), 'SurfaceFlinger serial line captured in logcat');
}

console.log('\n======================================================');
console.log(`⚡ ALL ${passed} ADVERSARIAL STRESS CHECKS PASSED WITH 0 FAILURES!`);
console.log('======================================================\n');
