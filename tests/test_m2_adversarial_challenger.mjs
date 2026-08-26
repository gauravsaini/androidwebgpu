/**
 * tests/test_m2_adversarial_challenger.mjs
 * 
 * Adversarial empirical test suite for Milestone 2:
 * Structured Debug Logging & In-UI Logcat Streaming.
 * 
 * Tests:
 * 1. Burst Stress: 20,000 rapid log dispatches (StructuredLogger & LogcatBuffer)
 * 2. Circular References & Unserializable Metadata: Deep cycles, BigInt, Throwing getters
 * 3. Buffer Bounds & Strict FIFO: 10,000 logs into 5,000-entry capacity
 * 4. Malformed Regex & Fuzz Search Filters: Unclosed brackets, invalid quantifiers, symbols
 * 5. Serial Stream Splitting & Edge Cases: Chunked UART byte feeds, CR/LF boundaries, panic prioritization
 * 
 * Conforms to ASD-STE100 and /ponytail simplicity principles.
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

let passed = 0;
let failed = 0;
const failures = [];

function assert(condition, testName, details = '') {
    if (condition) {
        passed++;
        console.log(`  ✔ [PASS] ${testName}`);
    } else {
        failed++;
        const msg = details ? `${testName} - ${details}` : testName;
        failures.push(msg);
        console.error(`  ✖ [FAIL] ${msg}`);
    }
}

console.log('================================================================================');
console.log('⚡ ADVERSARIAL STRESS TEST SUITE: MILESTONE 2 LOGGING & LOGCAT');
console.log('================================================================================\n');

// -----------------------------------------------------------------------------
// Test Suite 1: Burst Stress (20,000 rapid log dispatches)
// -----------------------------------------------------------------------------
console.log('▶ Category 1: 20,000 Rapid Log Burst Stress & Memory Safety');
{
    // 1.1: 20,000 logs to StructuredLogger
    const testLogger = new StructuredLogger({ consoleDispatch: false, maxLogHistory: 5000 });
    let listenerCount = 0;
    testLogger.addListener((entry) => {
        listenerCount++;
    });

    const startMem = process.memoryUsage().heapUsed;
    const startTime = performance.now();

    for (let i = 0; i < 20000; i++) {
        testLogger.log('v86', i % 2 === 0 ? 'D' : 'I', `Burst log payload sequence ${i}`, { index: i, sub: 'v86' });
    }

    const duration = performance.now() - startTime;
    const endMem = process.memoryUsage().heapUsed;
    const memDeltaMb = ((endMem - startMem) / (1024 * 1024)).toFixed(2);

    assert(listenerCount === 20000, 'StructuredLogger listener received all 20,000 dispatched logs', `received ${listenerCount}`);
    assert(testLogger.logs.length === 5000, 'StructuredLogger history clamped to maxLogHistory (5000)', `got ${testLogger.logs.length}`);
    assert(testLogger.logs[4999].message === 'Burst log payload sequence 19999', 'StructuredLogger retains latest log after 20,000 burst');
    assert(testLogger.logs[0].message === 'Burst log payload sequence 15000', 'StructuredLogger evicted first 15,000 logs FIFO');
    console.log(`    ℹ 20,000 logs dispatched in ${duration.toFixed(2)}ms (${(20000 / (duration / 1000)).toFixed(0)} logs/sec), heap delta: ${memDeltaMb} MB`);

    // 1.2: 20,000 rapid appends to LogcatBuffer
    const logcat = new LogcatBuffer(5000);
    let logcatListenerCount = 0;
    logcat.addListener((entry) => {
        logcatListenerCount++;
    });

    const startLogcat = performance.now();
    for (let i = 0; i < 20000; i++) {
        logcat.append('v86Guest', `Guest kernel log line ${i}`, i % 5 === 0 ? 'E' : 'D', 1000, 1001);
    }
    const durationLogcat = performance.now() - startLogcat;

    assert(logcatListenerCount === 20000, 'LogcatBuffer listener received all 20,000 entries', `received ${logcatListenerCount}`);
    assert(logcat.entries.length === 5000, 'LogcatBuffer clamped to exactly 5,000 entries', `got ${logcat.entries.length}`);
    assert(logcat.entries[0].msg === 'Guest kernel log line 15000', 'LogcatBuffer FIFO evicted first 15,000 entries');
    assert(logcat.entries[4999].msg === 'Guest kernel log line 19999', 'LogcatBuffer retains latest entry');
    console.log(`    ℹ 20,000 Logcat appends in ${durationLogcat.toFixed(2)}ms`);

    // 1.3: Top-level logDebug() 20,000 burst synchronization
    logger.clear();
    globalLogcat.clear();
    for (let i = 0; i < 20000; i++) {
        logDebug('bridge', 'W', `Virtio-GPU command flush burst ${i}`, { cmd: i });
    }
    assert(logger.logs.length === 5000, 'Global logger clamped to 5000 after 20,000 logDebug calls');
    assert(globalLogcat.entries.length === 5000, 'Global logcat clamped to 5000 after 20,000 logDebug calls');
    assert(globalLogcat.entries[4999].tag === 'bridge', 'Global logcat records correct subsystem tag from logDebug');
}

// -----------------------------------------------------------------------------
// Test Suite 2: Circular References & Unserializable Metadata
// -----------------------------------------------------------------------------
console.log('\n▶ Category 2: Circular References & Unserializable Metadata Stress');
{
    const l = new StructuredLogger({ consoleDispatch: false });

    // 2.1: Simple direct circular object
    const obj1 = { name: 'direct' };
    obj1.self = obj1;
    const e1 = l.log('compositor', 'I', 'Direct circular test', obj1);
    assert(e1.metadata && e1.metadata.error === 'circular_or_unserializable_metadata', 'Direct circular metadata handled safely');

    // 2.2: Deeply nested circular object
    const deepObj = { level1: { level2: { level3: { level4: { level5: {} } } } } };
    deepObj.level1.level2.level3.level4.level5.root = deepObj;
    const e2 = l.log('bridge', 'D', 'Deep circular test', deepObj);
    assert(e2.metadata && e2.metadata.error === 'circular_or_unserializable_metadata', 'Deeply nested circular object handled safely');

    // 2.3: Multiple interconnected circular cycles
    const nodeA = { name: 'A' };
    const nodeB = { name: 'B' };
    nodeA.neighbor = nodeB;
    nodeB.neighbor = nodeA;
    const e3 = l.log('v86', 'E', 'Interconnected cycle', nodeA);
    assert(e3.metadata && e3.metadata.error === 'circular_or_unserializable_metadata', 'Mutual circular references handled safely');

    // 2.4: BigInt metadata (cannot be serialized by JSON.stringify)
    const bigIntObj = { count: 12345678901234567890n };
    const e4 = l.log('runtime', 'I', 'BigInt metadata', bigIntObj);
    assert(e4.metadata && e4.metadata.error === 'circular_or_unserializable_metadata', 'BigInt unserializable metadata caught safely');

    // 2.5: Throwing getter in metadata
    const maliciousObj = {
        get boom() {
            throw new Error('Explosion during property access');
        }
    };
    const e5 = l.log('system', 'W', 'Throwing getter metadata', maliciousObj);
    assert(e5.metadata && e5.metadata.error === 'circular_or_unserializable_metadata', 'Throwing getter in metadata caught safely');

    // 2.6: Array with circular reference
    const arr = [];
    arr.push(arr);
    const e6 = l.log('v86', 'V', 'Circular array metadata', arr);
    assert(e6.metadata && e6.metadata.error === 'circular_or_unserializable_metadata', 'Circular array metadata caught safely');

    // 2.7: Null, undefined, primitive, symbol, and valid metadata
    assert(l.log('v86', 'I', 'null meta', null).metadata === null, 'null metadata returns null');
    assert(l.log('v86', 'I', 'undefined meta', undefined).metadata === null, 'undefined metadata returns null');
    assert(l.log('v86', 'I', 'valid meta', { a: 1, b: 'test', c: [1, 2, 3] }).metadata.a === 1, 'Valid JSON metadata preserved intact');
}

// -----------------------------------------------------------------------------
// Test Suite 3: Buffer Bounds Tests (10,000 into 5,000 Strict FIFO)
// -----------------------------------------------------------------------------
console.log('\n▶ Category 3: Buffer Bounds & Strict FIFO Drop Verification');
{
    const buf = new LogcatBuffer(5000);

    // Append 10,000 sequential entries
    for (let i = 0; i < 10000; i++) {
        buf.append('TagTest', `SeqMessage_${i}`, 'I', 10000 + i, 20000 + i);
    }

    assert(buf.entries.length === 5000, 'LogcatBuffer exact length is 5,000 after 10,000 appends');

    // Strict FIFO order check across all 5000 elements
    let isStrictFifo = true;
    let sequenceError = null;

    for (let i = 0; i < 5000; i++) {
        const expectedIndex = i + 5000;
        const entry = buf.entries[i];
        if (entry.msg !== `SeqMessage_${expectedIndex}`) {
            isStrictFifo = false;
            sequenceError = `Mismatch at index ${i}: expected SeqMessage_${expectedIndex}, got ${entry.msg}`;
            break;
        }
        if (entry.pid !== 10000 + expectedIndex) {
            isStrictFifo = false;
            sequenceError = `PID mismatch at index ${i}: expected ${10000 + expectedIndex}, got ${entry.pid}`;
            break;
        }
    }

    assert(isStrictFifo, 'Strict FIFO order preserved for all 5,000 entries (indexes 5000..9999)', sequenceError);
    assert(buf.entries[0].msg === 'SeqMessage_5000', 'First entry is exactly index 5000');
    assert(buf.entries[4999].msg === 'SeqMessage_9999', 'Last entry is exactly index 9999');

    // Edge capacities: Capacity 1
    const tinyBuf = new LogcatBuffer(1);
    tinyBuf.append('T', 'First', 'I');
    tinyBuf.append('T', 'Second', 'I');
    tinyBuf.append('T', 'Third', 'I');
    assert(tinyBuf.entries.length === 1 && tinyBuf.entries[0].msg === 'Third', 'Capacity 1 keeps strictly the latest entry');

    // Edge capacities: Capacity 0 (Zero capacity buffer)
    const zeroBuf = new LogcatBuffer(0);
    zeroBuf.append('T', 'DropMe', 'I');
    assert(zeroBuf.entries.length === 0, 'Capacity 0 immediately drops all entries safely');
}

// -----------------------------------------------------------------------------
// Test Suite 4: Malformed Regex & Fuzz Search Strings in Filters
// -----------------------------------------------------------------------------
console.log('\n▶ Category 4: Malformed Regex & Fuzz Search String Filters');
{
    const buf = new LogcatBuffer(100);
    buf.append('AndroidRuntime', 'java.lang.NullPointerException at com.android.server', 'E');
    buf.append('v86[0]', 'SeaBIOS (version 1.16.0) rel-1.16.0-0-g99fb511', 'I');
    buf.append('bridge-virtio', 'Processing packet opcode 0x0100 with size 48 bytes', 'D');
    buf.append('Compositor$Layer', 'SurfaceFlinger swapchain presentation frame=120', 'V');
    buf.append('Special.Chars+*?^$()[]{}|\\', 'Payload with regex metacharacters: /.*+?^${}()|[]\\', 'W');
    buf.append('UnicodeTag_🚀_🔥', 'Emoji and international payload: こんにちは 世界 1234', 'I');

    const adversarialQueries = [
        '[unclosed_bracket',
        '(unclosed_paren',
        '*leading_star_quantifier',
        '+plus_quantifier',
        '?question_quantifier',
        '\\incomplete_trailing_backslash',
        '^(?=.*[a-z])(?=.*[A-Z]).*$',
        'Special.Chars+*?^$()[]{}|\\',
        '/.*+?^${}()|[]\\',
        '[a-z',
        '{1,5}',
        'v86[0]',
        'Compositor$Layer',
        '🚀',
        'こんにちは',
        'null',
        'undefined',
        '\x00\x01\x02',
        '   ',
        ''
    ];

    let allQueriesSafe = true;
    let queryError = null;

    for (const query of adversarialQueries) {
        try {
            const resultsByMsg = buf.filter({ search: query });
            const resultsByTag = buf.filter({ tag: query });
            const resultsByBoth = buf.filter({ search: query, tag: query });
            const resultsQueryAlias = buf.query({ msgQuery: query, tagQuery: query });

            if (!Array.isArray(resultsByMsg) || !Array.isArray(resultsByTag) || !Array.isArray(resultsQueryAlias)) {
                allQueriesSafe = false;
                queryError = `Query "${query}" did not return an array`;
                break;
            }
        } catch (err) {
            allQueriesSafe = false;
            queryError = `Query "${query}" threw exception: ${err.message}`;
            break;
        }
    }

    assert(allQueriesSafe, 'All 20 adversarial / malformed regex search queries executed without throwing exceptions', queryError);

    // Exact literal matching verification
    const v86TagMatch = buf.filter({ tagQuery: 'v86[0]' });
    assert(v86TagMatch.length === 1 && v86TagMatch[0].tag === 'v86[0]', 'Literal regex brackets in tagQuery "v86[0]" matches exact tag');

    const regexMetaMatch = buf.filter({ search: 'Special.Chars+*?^$()[]{}|\\' });
    assert(regexMetaMatch.length === 1, 'Full regex metacharacter string searched literally without parser error');

    const emojiMatch = buf.filter({ search: '🚀' });
    assert(emojiMatch.length === 1 && emojiMatch[0].tag.includes('🚀'), 'Unicode emoji search query matched correctly');

    // Priority filter edge tests
    assert(buf.filter({ minPriority: 'E' }).length === 1, 'minPriority E returns only Error logs');
    assert(buf.filter({ minPriority: 'V' }).length === 6, 'minPriority V returns all 6 logs');
    assert(buf.filter({ minPriority: 'INVALID' }).length === 6, 'Invalid minPriority fallback to V');
    assert(buf.filter({ minPriority: null }).length === 6, 'null minPriority fallback to V');
    assert(buf.filter(undefined).length === 6, 'undefined options in filter() returns all entries');
}

// -----------------------------------------------------------------------------
// Test Suite 5: Serial Stream Chunking & Panic Detection
// -----------------------------------------------------------------------------
console.log('\n▶ Category 5: Serial UART Stream Chunking & Panic Prioritization');
{
    const buf = new LogcatBuffer(100);
    const received = [];
    const onLine = (e) => received.push(e);

    // 5.1: 1-byte at a time serial character feed
    const rawStream = "Linux version 5.10.0\r\n[    0.000000] Command line: console=ttyS0\nKernel panic - not syncing: VFS: Unable to mount root fs\n";
    for (let i = 0; i < rawStream.length; i++) {
        buf.feedSerialChar(rawStream[i], onLine);
    }

    assert(received.length === 3, 'Serial stream split into exactly 3 complete lines via single-byte feeder');
    assert(received[0].msg === 'Linux version 5.10.0', 'Line 1 matches');
    assert(received[1].msg === '[    0.000000] Command line: console=ttyS0', 'Line 2 matches');
    assert(received[2].msg === 'Kernel panic - not syncing: VFS: Unable to mount root fs', 'Line 3 matches');
    assert(received[2].priority === 'E', 'Kernel panic line automatically assigned priority E');
    assert(received[0].priority === 'D', 'Normal boot line assigned priority D');

    // 5.2: Incomplete line preservation in buffer
    buf.feedSerialChar('P');
    buf.feedSerialChar('a');
    buf.feedSerialChar('r');
    buf.feedSerialChar('t');
    buf.feedSerialChar('i');
    buf.feedSerialChar('a');
    buf.feedSerialChar('l');
    assert(buf.serialBuffer === 'Partial', 'Incomplete characters stored in serialBuffer');
    buf.feedSerialChar('\n', onLine);
    assert(received.length === 4 && received[3].msg === 'Partial', 'Line completed on newline');

    // 5.3: Chunked feedSerial with multiple lines and carriage returns
    buf.clear();
    received.length = 0;
    buf.feedSerial("Booting SeaBIOS...\r\nChecking memory: 512MB\r\nStarting Linux...\nFatal error: init not found\n", onLine);
    assert(received.length === 4, 'feedSerial correctly parsed 4 lines');
    assert(received[3].priority === 'E', 'Fatal error assigned priority E');
}

// -----------------------------------------------------------------------------
// Test Suite Summary
// -----------------------------------------------------------------------------
console.log('\n================================================================================');
console.log(`📊 ADVERSARIAL CHALLENGER SUITE SUMMARY: ${passed} Passed, ${failed} Failed`);
console.log('================================================================================');

if (failed > 0) {
    console.error('\n✖ FAILURES DETECTED:');
    for (const f of failures) {
        console.error(`  - ${f}`);
    }
    process.exit(1);
} else {
    console.log('\n✔ ALL ADVERSARIAL STRESS TESTS PASSED WITH 100% SUCCESS!');
    process.exit(0);
}
