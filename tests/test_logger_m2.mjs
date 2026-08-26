/**
 * tests/test_logger_m2.mjs - Dedicated Unit & Integration Test Suite for Milestone 2
 * Structured Debug Logging & In-UI Logcat Streaming (Requirement R2)
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
        throw new Error(`Assertion failed: ${message}`);
    }
}

console.log('⚡ Starting Milestone 2 Structured Logging & Logcat Test Suite...\n');

// Section 1: StructuredLogger Unit Tests
console.log('======================================================');
console.log('▶ Section 1: StructuredLogger Engine & Subsystems');
console.log('======================================================');

{
    const l = new StructuredLogger({ consoleDispatch: false });
    
    // Prefix normalization
    const e1 = l.log('v86', 'I', 'v86 message');
    assert(e1.prefix === '[v86]' && e1.subsystem === 'v86', 'v86 prefix formatted to [v86]');

    const e2 = l.log('bridge', 'D', 'bridge message');
    assert(e2.prefix === '[bridge]' && e2.subsystem === 'bridge', 'bridge prefix formatted to [bridge]');

    const e3 = l.log('compositor', 'V', 'compositor message');
    assert(e3.prefix === '[compositor]' && e3.subsystem === 'compositor', 'compositor prefix formatted to [compositor]');

    const e4 = l.log('unknown_subsystem', 'W', 'fallback message');
    assert(e4.prefix === '[unknown]' && e4.subsystem === 'unknown', 'unknown subsystem fallback to [unknown]');

    // Priority levels
    const e5 = l.log('v86', 'INVALID_LEVEL', 'level test');
    assert(e5.level === 'I', 'Invalid level fallback to default I');

    const e6 = l.e('v86', 'Error helper');
    assert(e6.level === 'E', 'Helper method l.e() produces level E');

    // Circular metadata sanitization
    const circular = {};
    circular.ref = circular;
    const e7 = l.log('bridge', 'W', 'circular test', circular);
    assert(e7.metadata && e7.metadata.error === 'circular_or_unserializable_metadata', 'Circular metadata safely sanitized');

    // Listener dispatch
    let receivedListener = null;
    const listener = (entry) => { receivedListener = entry; };
    l.addListener(listener);
    l.log('compositor', 'I', 'listener dispatch test');
    assert(receivedListener && receivedListener.message === 'listener dispatch test', 'Listener receives emitted logs');
    l.removeListener(listener);
}

// Section 2: LogcatBuffer Unit Tests
console.log('\n======================================================');
console.log('▶ Section 2: LogcatBuffer & Line Buffering');
console.log('======================================================');

{
    const buf = new LogcatBuffer(5000);

    // Standard timestamp format MM-DD HH:MM:SS.mmm
    const date = new Date(2026, 7, 26, 14, 5, 9, 42);
    const timeStr = LogcatBuffer.formatTimestamp(date);
    assert(timeStr === '08-26 14:05:09.042', 'Timestamp formatted with 2-digit padding and 3-digit ms');

    // Midnight rollover
    const midnight = new Date(2026, 7, 27, 0, 0, 0, 5);
    const midStr = LogcatBuffer.formatTimestamp(midnight);
    assert(midStr.includes('00:00:00.005'), 'Midnight rollover timestamp correctly formatted');

    // Append and format
    const entry = buf.append('v86Guest', 'Linux kernel starting', 'I', 10042, 10042, date);
    assert(entry.formatted === '08-26 14:05:09.042 10042 10042 I v86Guest: Linux kernel starting', 'Logcat formatted line matches standard Android logcat spec');

    // Circular FIFO eviction at 5000 capacity
    for (let i = 0; i < 5500; i++) {
        buf.append('Tag', `Message ${i}`, 'D');
    }
    assert(buf.entries.length === 5000, 'Capacity clamped at 5000');
    assert(buf.entries[0].msg === 'Message 500', 'Oldest entries evicted FIFO (first is 500)');
    assert(buf.entries[4999].msg === 'Message 5499', 'Latest entries retained (last is 5499)');

    // Priority filter (>= W)
    buf.clear();
    buf.append('T', 'Verb', 'V');
    buf.append('T', 'Deb', 'D');
    buf.append('T', 'Inf', 'I');
    buf.append('T', 'Warn', 'W');
    buf.append('T', 'Err', 'E');
    const filteredW = buf.filter({ minPriority: 'W' });
    assert(filteredW.length === 2 && filteredW[0].priority === 'W' && filteredW[1].priority === 'E', 'Priority filter >= W yields only W and E');

    // Tag and text search
    buf.append('SpecialTag[0].*$', 'Hello world regex test', 'I');
    const tagMatch = buf.filter({ tagQuery: 'specialtag[0].*$' });
    assert(tagMatch.length === 1 && tagMatch[0].tag === 'SpecialTag[0].*$', 'Literal regex characters in tagQuery supported');

    const msgMatch = buf.filter({ msgQuery: 'hello world' });
    assert(msgMatch.length === 1 && msgMatch[0].msg.includes('Hello world'), 'Substring message query supported');

    // Serial character buffering
    buf.clear();
    const emitted = [];
    const onLine = (e) => emitted.push(e.msg);
    const serialInput = "v86 BIOS init\r\nKernel boot\r\nKernel panic: out of memory\n";
    for (const ch of serialInput) {
        buf.feedSerialChar(ch, onLine);
    }
    assert(emitted.length === 3, 'Serial stream split into 3 lines');
    assert(emitted[0] === 'v86 BIOS init', 'First serial line matches');
    assert(emitted[1] === 'Kernel boot', 'Second serial line matches');
    assert(emitted[2] === 'Kernel panic: out of memory', 'Third serial line matches');
    assert(buf.entries[2].priority === 'E', 'Panic detected as priority E');
}

// Section 3: Module Cross-Integration Tests
console.log('\n======================================================');
console.log('▶ Section 3: JS Modules Integration (v86 & Virtio-GPU)');
console.log('======================================================');

{
    logger.clear();
    globalLogcat.clear();

    const mgr = new V86GuestManager({
        mockMode: true
    });

    mgr.setState(VM_STATES.BOOTING);
    mgr.recordMilestone(BOOT_MILESTONES.BIOS_POST);
    mgr.feedSerial('SeaBIOS rel-1.14.0\nLinux version 5.10.0\n[init] system boot completed successfully\n');

    assert(logger.logs.some(l => l.subsystem === 'v86' && l.message.includes('VM State Transition: UNINITIALIZED -> BOOTING')), 'V86 state transition logged with [v86]');
    assert(logger.logs.some(l => l.subsystem === 'v86' && l.message.includes('[BOOT-MILESTONE] Achieved: BIOS_POST')), 'V86 boot milestone logged with [v86]');
    assert(globalLogcat.entries.some(e => e.tag === 'v86Guest' && e.msg.includes('Linux version 5.10.0')), 'Guest serial line captured in globalLogcat');

    // Virtio-GPU logging
    const mockBridge = {
        process_command_packet: (pkt) => new Uint8Array([0x00, 0x11, 0x00, 0x00]),
        get_scanout_framebuffer: (id) => new Uint8Array(800 * 600 * 4),
        get_scanout_damage: (id) => [0, 0, 800, 600],
        clear_scanout_damage: (id) => {}
    };
    const mockCanvas = { width: 800, height: 600, getContext: () => ({ createImageData: () => ({ data: new Uint8Array(800 * 600 * 4) }), putImageData: () => {} }) };
    const dev = new VirtioGpuDevice(null, mockBridge, mockCanvas);

    assert(logger.logs.some(l => l.subsystem === 'bridge' && l.message.includes('Virtio-GPU PCI device')), 'Virtio-GPU PCI init logged with [bridge]');

    dev.processControlQueue(new Uint8Array(24));
    assert(logger.logs.some(l => l.subsystem === 'bridge' && l.message.includes('Processing control queue packet')), 'Control queue logged with [bridge]');
    assert(logger.logs.some(l => l.subsystem === 'bridge' && l.message.includes('damaged rect')), 'Scanout damaged rect logged with [bridge]');
}

console.log('\n======================================================');
console.log(`⚡ ALL MILESTONE 2 LOGGER TESTS PASSED! (${passed} assertions, 0 failed)`);
console.log('======================================================');
