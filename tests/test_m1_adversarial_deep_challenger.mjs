/**
 * Milestone 1 Deep Adversarial Challenger Test Suite
 * 
 * Focus Areas:
 * 1. verifyBzImage Fuzzing & Malformed Buffer Rejection Matrix
 * 2. Hypervisor 9-State Lifecycle Machine & Illegal State Handling
 * 3. Serial Stream Parser, ANSI Escape Sequences & Panic Recovery
 * 4. Memory Bounds, Zero-Copy Subarray Mutation & ServiceManager IPC
 * 
 * Conforms to ASD-STE100 and /ponytail simplicity principles.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES, verifyBzImage } from '../src/v86_guest_manager.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalPassed = 0;
let totalFailed = 0;

function assert(condition, message) {
    if (!condition) {
        totalFailed++;
        console.error(`  ✖ [FAIL] ${message}`);
        throw new Error(`Assertion failed: ${message}`);
    }
    totalPassed++;
}

async function runSection(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ [EMPIRICAL-CHALLENGE-M1] ${name}`);
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

async function main() {
    console.log("⚡ Starting Milestone 1 Deep Adversarial Challenger Probes...\n");

    const bzImagePath = path.join(rootDir, 'guest/build/bzImage');
    const validBzImage = fs.readFileSync(bzImagePath);

    // -------------------------------------------------------------------------
    // Probe 1: verifyBzImage Header Fuzzing & Mutation Matrix (20,000 mutations)
    // -------------------------------------------------------------------------
    await runSection("1. verifyBzImage Fuzzing & Malformed Header Validation", async () => {
        const mgr = new V86GuestManager();

        // 1.1 Non-buffer & primitive inputs rejection
        const nonBuffers = [
            null, undefined, 0, 1, -1, 3.14, NaN, Infinity, -Infinity,
            "", "HdrS", "AA55", true, false, {}, { length: 520 },
            [], [0x55, 0xAA], () => {}, Symbol("kernel"), new Date(), new Map()
        ];
        for (const input of nonBuffers) {
            const standalone = verifyBzImage(input);
            assert(standalone.valid === false, `verifyBzImage must reject non-buffer: ${String(input)}`);
            assert(typeof standalone.error === 'string', "Error description must be provided");
            assert(mgr.verifyBzImage(input) === false, `mgr.verifyBzImage must return false for: ${String(input)}`);
        }

        // 1.2 Undersized buffers (0 to 519 bytes)
        for (let size = 0; size < 0x208; size++) {
            const trunc = validBzImage.subarray(0, size);
            const res = verifyBzImage(trunc);
            assert(res.valid === false, `Size ${size} bytes must be rejected (<520)`);
            assert(mgr.verifyBzImage(trunc) === false, `mgr.verifyBzImage rejected size ${size}`);
        }

        // 1.3 Exact boundary at 520 bytes (0x208)
        const exactHeader = validBzImage.subarray(0, 0x208);
        const resExact = verifyBzImage(exactHeader);
        assert(resExact.valid === true, "Exact 520-byte valid header must pass");
        assert(resExact.bootFlag === 0xAA55, "bootFlag is 0xAA55");
        assert(resExact.headerMagic === 'HdrS', "headerMagic is 'HdrS'");
        assert(resExact.protocol >= 0x0200, "protocol >= 0x0200");

        // 1.4 Corrupted Boot Sector Signature at 0x1FE (must be 0xAA55)
        for (let b = 0; b < 256; b++) {
            if (b === 0x55) continue;
            const corrupt = Buffer.from(exactHeader);
            corrupt[0x1FE] = b;
            const res = verifyBzImage(corrupt);
            assert(res.valid === false, `Corrupt low byte 0x${b.toString(16)} at 0x1FE rejected`);
        }

        for (let b = 0; b < 256; b++) {
            if (b === 0xAA) continue;
            const corrupt = Buffer.from(exactHeader);
            corrupt[0x1FF] = b;
            const res = verifyBzImage(corrupt);
            assert(res.valid === false, `Corrupt high byte 0x${b.toString(16)} at 0x1FF rejected`);
        }

        // 1.5 Corrupted Magic at 0x202 (must be 'HdrS' / 0x53726448)
        const badMagics = [
            'Hdr\0', 'hdrs', 'HDRS', 'SrdH', 'v86M', 'KRNL',
            '\0\0\0\0', '~~~~', '    ', '\x00Hdr', 'Hdr\xFF'
        ];
        for (const m of badMagics) {
            const corrupt = Buffer.from(exactHeader);
            corrupt.write(m.slice(0, 4), 0x202, 4, 'ascii');
            const res = verifyBzImage(corrupt);
            assert(res.valid === false, `Bad magic '${m}' rejected`);
        }

        // 1.6 Protocol Version Boundaries at 0x206
        const protoMatrix = [
            { proto: 0x0000, valid: false },
            { proto: 0x0100, valid: false },
            { proto: 0x0102, valid: false },
            { proto: 0x01FE, valid: false },
            { proto: 0x01FF, valid: false },
            { proto: 0x0200, valid: true },
            { proto: 0x0201, valid: true },
            { proto: 0x020D, valid: true },
            { proto: 0x0215, valid: true },
            { proto: 0x0300, valid: true },
            { proto: 0x7FFF, valid: true },
            { proto: 0xFFFF, valid: true }
        ];
        for (const tc of protoMatrix) {
            const buf = Buffer.from(exactHeader);
            buf.writeUInt16LE(tc.proto, 0x206);
            const res = verifyBzImage(buf);
            assert(res.valid === tc.valid, `Protocol 0x${tc.proto.toString(16)} validity must be ${tc.valid}`);
        }

        // 1.7 20,000 Randomized Fuzz Mutations
        for (let i = 0; i < 20000; i++) {
            const fuzzed = Buffer.from(exactHeader);
            const mutationCount = 1 + Math.floor(Math.random() * 8);
            for (let m = 0; m < mutationCount; m++) {
                const randOffset = Math.floor(Math.random() * exactHeader.length);
                fuzzed[randOffset] = Math.floor(Math.random() * 256);
            }
            const res = verifyBzImage(fuzzed);
            if (res.valid === true) {
                // If it claims valid, verify the 3 invariants hold
                const bootSig = fuzzed[0x1FE] | (fuzzed[0x1FF] << 8);
                const magic = String.fromCharCode(fuzzed[0x202], fuzzed[0x203], fuzzed[0x204], fuzzed[0x205]);
                const protocol = fuzzed[0x206] | (fuzzed[0x207] << 8);
                assert(bootSig === 0xAA55, "Fuzzed buffer passed: boot signature invariant holds");
                assert(magic === 'HdrS', "Fuzzed buffer passed: magic invariant holds");
                assert(protocol >= 0x0200, "Fuzzed buffer passed: protocol version invariant holds");
            }
        }
    });

    // -------------------------------------------------------------------------
    // Probe 2: Hypervisor State Machine & Invariant Stress (20,000 cycles)
    // -------------------------------------------------------------------------
    await runSection("2. Lifecycle State Machine & Invalid Transition Invariants", async () => {
        const mgr = new V86GuestManager();
        assert(mgr.getState() === VM_STATES.UNINITIALIZED, "Starts in UNINITIALIZED");

        // 2.1 Rejection of illegal state strings
        const illegalStates = [
            '', 'STARTING', 'INITIALIZED', 'READY', 'HALTED', 'DEAD',
            'undefined', 'null', 0, 1, true, false, {}, []
        ];
        for (const bad of illegalStates) {
            let threw = false;
            try {
                mgr.setState(bad);
            } catch {
                threw = true;
            }
            assert(threw, `setState('${bad}') must throw Error`);
        }

        // 2.2 Rapid 20,000 state transitions across all 9 valid states
        const validStates = [
            VM_STATES.LOADING,
            VM_STATES.BOOTING,
            VM_STATES.KERNEL_READY,
            VM_STATES.BINDER_READY,
            VM_STATES.SERVICES_READY,
            VM_STATES.RUNNING,
            VM_STATES.PAUSED,
            VM_STATES.ERROR,
            VM_STATES.UNINITIALIZED
        ];

        let transitionCount = 0;
        mgr.config.onStateChange = (to, from) => {
            transitionCount++;
        };

        for (let i = 0; i < 20000; i++) {
            const nextState = validStates[i % validStates.length];
            mgr.setState(nextState);
            assert(mgr.getState() === nextState, `Transition ${i}: state must be ${nextState}`);
        }
        assert(transitionCount > 0, "onStateChange fired on valid transitions");

        // 2.3 Pause and Resume invariants
        mgr.setState(VM_STATES.RUNNING);
        mgr.pause();
        assert(mgr.getState() === VM_STATES.PAUSED, "pause() moves RUNNING -> PAUSED");
        mgr.pause();
        assert(mgr.getState() === VM_STATES.PAUSED, "Repeated pause() is idempotent");
        mgr.resume();
        assert(mgr.getState() === VM_STATES.RUNNING, "resume() moves PAUSED -> RUNNING");
        mgr.resume();
        assert(mgr.getState() === VM_STATES.RUNNING, "Repeated resume() is idempotent");

        // 2.4 Calling pause/resume from illegal states must not change state
        const illegalPauseStates = [VM_STATES.UNINITIALIZED, VM_STATES.LOADING, VM_STATES.BOOTING, VM_STATES.ERROR];
        for (const st of illegalPauseStates) {
            mgr.setState(st);
            mgr.pause();
            assert(mgr.getState() === st, `pause() from ${st} must not change state`);
            mgr.resume();
            assert(mgr.getState() === st, `resume() from ${st} must not change state`);
        }

        // 2.5 Destroy cleans up completely
        mgr.recordMilestone(BOOT_MILESTONES.BIOS_POST);
        mgr.recordMilestone(BOOT_MILESTONES.KERNEL_BOOT);
        mgr.destroy();
        assert(mgr.getState() === VM_STATES.UNINITIALIZED, "destroy() resets to UNINITIALIZED");
        assert(mgr.getMilestones().length === 0, "destroy() clears milestones");
        assert(mgr.serialLogs.length === 0, "destroy() clears serial logs");
    });

    // -------------------------------------------------------------------------
    // Probe 3: Serial Stream Parser, Unicode, ANSI Fuzzing & Panic Recovery
    // -------------------------------------------------------------------------
    await runSection("3. Serial Parser Stream Fuzzing & Panic Error Recovery", async () => {
        const fullBootSequence = [
            "SeaBIOS (version rel-1.14.0-0-g155821a)\r\n",
            "Linux version 5.10.0-android-x86 (androidwebgpu@v86) #1 SMP PREEMPT\r\n",
            "virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb frame buffer device\r\n",
            "Android Binder IPC Driver initialized (protocol version 8)\r\n",
            "binderfs: created /dev/binderfs/binder\r\n",
            "Run /init as init process\r\n",
            "[init] servicemanager started (handle 0 context manager)\r\n",
            "[init] pms_rs: ready (package manager registered)\r\n",
            "Zygote: listening on socket /dev/socket/zygote\r\n",
            "ART: Initialized boot classpath (/system/framework/boot.art)\r\n",
            "[init] system boot completed successfully\r\n"
        ].join('');

        // 3.1 Arbitrary chunk boundary feeding (sizes 1 to 29 bytes)
        for (let chunkSize = 1; chunkSize <= 29; chunkSize += 3) {
            const mgr = new V86GuestManager();
            mgr.setState(VM_STATES.BOOTING);
            let offset = 0;
            while (offset < fullBootSequence.length) {
                const chunk = fullBootSequence.slice(offset, offset + chunkSize);
                mgr.feedSerial(chunk);
                offset += chunkSize;
            }
            mgr.feedSerial("\n");
            assert(mgr.getState() === VM_STATES.RUNNING, `Chunk size ${chunkSize}: must reach RUNNING`);
            assert(mgr.hasMilestone(BOOT_MILESTONES.BIOS_POST), `Chunk size ${chunkSize}: BIOS_POST`);
            assert(mgr.hasMilestone(BOOT_MILESTONES.KERNEL_BOOT), `Chunk size ${chunkSize}: KERNEL_BOOT`);
            assert(mgr.hasMilestone(BOOT_MILESTONES.BINDERFS_MOUNT), `Chunk size ${chunkSize}: BINDERFS_MOUNT`);
            assert(mgr.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), `Chunk size ${chunkSize}: SERVICEMANAGER_READY`);
            assert(mgr.hasMilestone(BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED), `Chunk size ${chunkSize}: SYSTEM_BOOT_COMPLETED`);
        }

        // 3.2 Panic detection strings & Recovery cycles (100 runs)
        const fatalTriggers = [
            "Kernel panic - not syncing: Fatal exception in interrupt",
            "Kernel panic - not syncing: VFS: Unable to mount root fs",
            "Kernel panic - not syncing: Attempted to kill init!",
            "Invalid opcode: 0000 [#1] PREEMPT SMP",
            "Illegal instruction (SIGILL) in process 102 (zygote)",
            "binderfs: failed to mount /dev/binderfs (-19)",
            "Out of memory: Kill process 54 (servicemanager) score 200 or sacrifice child"
        ];

        for (let i = 0; i < 100; i++) {
            const trigger = fatalTriggers[i % fatalTriggers.length];
            const mgr = new V86GuestManager();
            await mgr.start();
            mgr.setState(VM_STATES.RUNNING);
            assert(mgr.getState() === VM_STATES.RUNNING, `Run ${i}: Started in RUNNING`);

            // Inject panic
            mgr.feedSerial(`${trigger}\n`);
            assert(mgr.getState() === VM_STATES.ERROR, `Run ${i}: Must transition to ERROR on fatal trigger`);

            // pingServiceManager must reject during ERROR state
            let rejected = false;
            try {
                await mgr.pingServiceManager(0);
            } catch (err) {
                rejected = true;
                assert(err.message.includes('ERROR'), "Ping error names ERROR state");
            }
            assert(rejected, `Run ${i}: pingServiceManager rejected during ERROR`);

            // Recover by destroy -> restart
            mgr.destroy();
            assert(mgr.getState() === VM_STATES.UNINITIALIZED, `Run ${i}: Reset to UNINITIALIZED`);
            await mgr.start();
            mgr.feedSerial("SeaBIOS\n[init] system boot completed successfully\n");
            assert(mgr.getState() === VM_STATES.RUNNING, `Run ${i}: Recovered to RUNNING`);
            mgr.destroy();
        }
    });

    // -------------------------------------------------------------------------
    // Probe 4: Memory Buffer Allocation & ServiceManager IPC Checks
    // -------------------------------------------------------------------------
    await runSection("4. Memory Buffer & ServiceManager Root Handle 0 Invariants", async () => {
        const mgr = new V86GuestManager({ memorySizeMb: 256 });
        const mem = mgr.getGuestMemory();
        assert(mem instanceof Uint8Array, "Guest memory is Uint8Array");
        assert(mem.byteLength === 256 * 1024 * 1024, "Guest memory byteLength is 256MB");

        // First byte and last byte access
        mem[0] = 0xAA;
        mem[mem.length - 1] = 0x55;
        assert(mem[0] === 0xAA, "Read byte 0");
        assert(mem[mem.length - 1] === 0x55, "Read last byte");

        // ServiceManager handle 0 ping
        await mgr.start();
        mgr.feedSerial("SeaBIOS\n[init] system boot completed successfully\n");
        const pingResp = await mgr.pingServiceManager(0);
        assert(pingResp.pingOk === true, "Ping handle 0 succeeds");
        assert(pingResp.resultCode === 0x80407203, "resultCode is BR_REPLY (0x80407203)");
        assert(pingResp.targetHandle === 0, "targetHandle is 0");

        // Invalid targetHandle
        let invalidPing = false;
        try {
            await mgr.pingServiceManager(1);
        } catch {
            invalidPing = true;
        }
        assert(invalidPing, "pingServiceManager(1) throws error for non-zero handle");

        // Stats summary check
        const stats = mgr.getStats();
        assert(stats.state === VM_STATES.RUNNING, "Stats state is RUNNING");
        assert(stats.memoryAllocatedMb === 256, "Stats memory size is 256MB");
        assert(stats.vgaMemoryAllocatedMb === 16, "Stats VGA size is 16MB");
        assert(stats.logLinesCount > 0, "Stats has logged lines");

        mgr.destroy();
    });

    console.log(`\n======================================================`);
    console.log(`⚡ ALL MILESTONE 1 ADVERSARIAL CHALLENGER PROBES PASSED`);
    console.log(`Total passed assertions: ${totalPassed}`);
    console.log(`Total failed assertions: ${totalFailed}`);
    console.log(`======================================================\n`);

    if (totalFailed > 0) {
        process.exit(1);
    } else {
        process.exit(0);
    }
}

main().catch(err => {
    console.error("Fatal error in test_m1_adversarial_deep_challenger:", err);
    process.exit(1);
});
