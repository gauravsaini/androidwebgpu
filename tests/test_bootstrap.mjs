/**
 * Automated Integration Test Suite: System Bootstrap & Injectable V86 Mock
 * Covers all 5 stages of hypervisor lifecycle, serial dispatch, and single-instance validation.
 */

import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { SystemBootstrap, VM_STATES, BOOT_MILESTONES, verifyBzImage } from '../src/system_bootstrap.js';
import { globalLogcat } from '../src/logger.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

let totalPassed = 0;
function pass(msg) {
    totalPassed++;
    console.log(`  ✔ [PASS] ${msg}`);
}

/**
 * Mock V86 hypervisor implementation for headless testing.
 */
class MockV86Starter {
    constructor(options) {
        this.options = options;
        this.listeners = new Map();
        this.destroyed = false;
        this.lastSentSerial = null;
    }

    add_listener(event, cb) {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event).push(cb);
    }

    remove_listener(event, cb) {
        if (this.listeners.has(event)) {
            const list = this.listeners.get(event).filter(fn => fn !== cb);
            this.listeners.set(event, list);
        }
    }

    emit(event, ...args) {
        const list = this.listeners.get(event) || [];
        for (const cb of list) {
            cb(...args);
        }
    }

    emitSerialString(str) {
        for (const ch of str) {
            this.emit('serial0-output-char', ch);
        }
    }

    serial0_send(text) {
        this.lastSentSerial = text;
    }

    destroy() {
        this.destroyed = true;
    }
}

async function runTests() {
    console.log('\n======================================================');
    console.log('⚡ Starting System Bootstrap & V86 Mock Test Suite...');
    console.log('======================================================\n');

    // =========================================================================
    // Stage 1: Constructor Options & Dependency Injection
    // =========================================================================
    console.log('▶ Stage 1: Constructor Options & Dependency Injection');
    {
        const milestones = [];
        const states = [];
        const serials = [];

        const bootstrap = new SystemBootstrap({
            V86Class: MockV86Starter,
            memorySizeMb: 256,
            vgaMemorySizeMb: 16,
            autostart: false,
            onMilestone: (m) => milestones.push(m),
            onStateChange: (s) => states.push(s),
            onSerial: (line) => serials.push(line)
        });

        assert.equal(bootstrap.V86Class, MockV86Starter, 'Injected V86Class matches MockV86Starter');
        assert.equal(bootstrap.options.memorySizeMb, 256, 'Memory size 256MB properly configured');
        assert.equal(bootstrap.options.vgaMemorySizeMb, 16, 'VGA memory size 16MB properly configured');
        assert.equal(bootstrap.getState(), VM_STATES.UNINITIALIZED, 'Initial state is UNINITIALIZED');
        pass('Constructor accepts dependency injection options');

        // Test custom event subscriptions
        let customFired = false;
        bootstrap.on('customEvent', (val) => { customFired = val; });
        bootstrap.emit('customEvent', true);
        assert.equal(customFired, true, 'Event emitter mechanism functions properly');
        pass('Event subscription and dispatch methods work');
    }

    // =========================================================================
    // Stage 2: Serial Listener Attachment on Boot
    // =========================================================================
    console.log('\n▶ Stage 2: Serial Listener Attachment on Boot');
    let bootstrap;
    let mockEmulator;
    const recordedMilestones = [];
    const recordedSerials = [];
    const recordedStates = [];

    {
        bootstrap = new SystemBootstrap({
            V86Class: MockV86Starter,
            autostart: false,
            onMilestone: (m) => recordedMilestones.push(m),
            onSerial: (l) => recordedSerials.push(l),
            onStateChange: (s) => recordedStates.push(s)
        });

        await bootstrap.init({ isHeadless: true });
        const guestMgr = bootstrap.getGuestManager();
        assert(guestMgr !== null, 'Guest manager created');

        await guestMgr.start();
        mockEmulator = guestMgr.emulator;
        assert(mockEmulator instanceof MockV86Starter, 'Emulator instance constructed from injected MockV86Starter');
        assert(mockEmulator.listeners.has('serial0-output-char'), 'serial0-output-char listener attached');
        assert(mockEmulator.listeners.has('emulator-started'), 'emulator-started listener attached');
        assert(mockEmulator.listeners.has('emulator-ready'), 'emulator-ready listener attached');
        pass('MockV86Starter constructed and listeners registered');

        // Fire emulator-started
        mockEmulator.emit('emulator-started');
        assert.equal(bootstrap.getState(), VM_STATES.BOOTING, 'State remains BOOTING on emulator-started (POST active)');
        pass('Hypervisor state remains BOOTING during BIOS POST');
    }

    // =========================================================================
    // Stage 3: Milestone Dispatch from dmesg Patterns
    // =========================================================================
    console.log('\n▶ Stage 3: Milestone Dispatch from dmesg Patterns');
    {
        assert(mockEmulator, 'Mock emulator active');

        const dmesgSequence = [
            { text: "[    0.000000] Linux version 5.10.0-android-x86\n", expected: BOOT_MILESTONES.KERNEL_BOOT },
            { text: "[    0.100000] virtio_gpu 0000:00:02.0: DRM/KMS active\n", expected: BOOT_MILESTONES.VIRTIO_GPU_INIT },
            { text: "[    0.200000] binderfs: mounted on /dev/binderfs\n", expected: BOOT_MILESTONES.BINDERFS_MOUNT },
            { text: "Run /init as init process\n", expected: BOOT_MILESTONES.INIT_USERSPACE },
            { text: "servicemanager: ready for IPC\n", expected: BOOT_MILESTONES.SERVICEMANAGER_READY },
            { text: "pms_rs: ready on binder handle 5\n", expected: BOOT_MILESTONES.RUST_SERVICES_READY },
            { text: "Zygote: ready, boot completed\n", expected: BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED }
        ];

        for (const { text, expected } of dmesgSequence) {
            mockEmulator.emitSerialString(text);
            assert(bootstrap.getMilestones().includes(expected), `Milestone ${expected} recorded`);
            assert(recordedMilestones.includes(expected), `Milestone ${expected} dispatched to listener`);
            pass(`Dispatched milestone ${expected} from dmesg pattern`);
        }

        assert.equal(bootstrap.getState(), VM_STATES.RUNNING, 'State transitions to RUNNING only after real boot completion');
        pass('State transitions to RUNNING after SYSTEM_BOOT_COMPLETED');

        assert(recordedSerials.length > 0, 'onSerial received guest serial lines');
        assert(recordedSerials.some(l => l.includes('Linux version')), 'onSerial received Linux boot banner');
        pass('onSerial stream dispatched to registered listener');

        // Test sending serial shell command
        bootstrap.sendSerialCommand('uname -a');
        assert.equal(mockEmulator.lastSentSerial, 'uname -a\n', 'Serial command sent to hypervisor serial0_send');
        pass('Guest serial command routed to emulator');
    }

    // =========================================================================
    // Stage 4: Error Handling & State Machine Transitions
    // =========================================================================
    console.log('\n▶ Stage 4: Error Handling & State Machine Transitions');
    {
        // 1. Kernel Panic Detection
        mockEmulator.emitSerialString("Kernel panic - not syncing: VFS: Unable to mount root fs\n");
        assert.equal(bootstrap.getState(), VM_STATES.ERROR, 'State transitions to ERROR on kernel panic');
        pass('Kernel panic triggers transition to ERROR state');

        // 2. verifyBzImage Validation
        // Create valid 1024-byte kernel header with AA55 at 510 and HdrS at 514
        const validHeader = new Uint8Array(1024);
        validHeader[510] = 0x55;
        validHeader[511] = 0xAA;
        validHeader[514] = 0x48; // 'H'
        validHeader[515] = 0x64; // 'd'
        validHeader[516] = 0x72; // 'r'
        validHeader[517] = 0x53; // 'S'
        validHeader[518] = 0x00; // version 2.00
        validHeader[519] = 0x02;

        const resValid = bootstrap.verifyBzImage(validHeader.buffer);
        assert.equal(resValid.valid, true, 'Valid bzImage header passes verification');
        pass('Valid bzImage header passes verification');

        // Corrupt magic
        const invalidHeader = new Uint8Array(1024);
        const resInvalid = bootstrap.verifyBzImage(invalidHeader.buffer);
        assert.equal(resInvalid.valid, false, 'Invalid bzImage header rejected');
        pass('Invalid bzImage header rejected');

        // Truncated buffer
        const truncated = new Uint8Array(100);
        const resTruncated = bootstrap.verifyBzImage(truncated.buffer);
        assert.equal(resTruncated.valid, false, 'Truncated bzImage header rejected');
        pass('Truncated bzImage buffer rejected');
    }

    // =========================================================================
    // Stage 5: Static Verification of Single Hypervisor Instantiation
    // =========================================================================
    console.log('\n▶ Stage 5: Static Verification of Single Hypervisor Instantiation');
    {
        const filesToScan = [
            'android.html',
            'src/main_android.js',
            'src/system_bootstrap.js',
            'src/app_controller.js',
            'src/ui_render.js',
            'src/android_runtime.js',
            'src/virtio_gpu_device.js',
            'src/logger.js',
            'src/apk_client_parser.js',
            'src/dex_vm.js',
            'src/android_network.js'
        ];

        const instantiationRegex = /new\s+(V86Class|V86Starter)\s*\(/g;
        const violations = [];

        for (const relPath of filesToScan) {
            const fullPath = path.join(ROOT_DIR, relPath);
            if (!fs.existsSync(fullPath)) continue;
            const content = fs.readFileSync(fullPath, 'utf8');
            const matches = content.match(instantiationRegex);
            if (matches && matches.length > 0) {
                violations.push({ file: relPath, count: matches.length });
            }
        }

        assert.equal(violations.length, 0, `Forbidden new V86Class/V86Starter found in: ${JSON.stringify(violations)}`);
        pass('Static scan confirms zero bypassed new V86Class/V86Starter instances across frontend modules');

        // Verify only v86_guest_manager.js contains the genuine instantiation
        const managerContent = fs.readFileSync(path.join(ROOT_DIR, 'src/v86_guest_manager.js'), 'utf8');
        const managerMatches = managerContent.match(instantiationRegex);
        assert(managerMatches && managerMatches.length === 1, 'v86_guest_manager.js contains exactly 1 new V86Class instantiation');
        pass('src/v86_guest_manager.js is the sole module instantiating V86Class');
    }

    console.log('\n======================================================');
    console.log(`⚡ ALL 5 STAGES OF BOOTSTRAP INTEGRATION TEST PASSED! (${totalPassed} assertions)`);
    console.log('======================================================\n');
}

runTests().catch((err) => {
    console.error('Fatal test error:', err);
    process.exit(1);
});
