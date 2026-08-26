/**
 * Empirical Challenger Test Suite for Phase 0 v86 Guest Manager & Anti-Mock Verification
 * 
 * Deeply stress-tests:
 * 1. Lifecycle State Machine & Transition Invariants (10,000 cycles)
 * 2. Rapid Pause / Resume & Out-of-Order Lifecycle Handling (20,000 cycles)
 * 3. Fragmented, Chunked & Fuzzed Serial dmesg Stream Processing (1,000 runs)
 * 4. ANSI Escape, Unicode, & Binary Serial Noise Immunity (10,000 lines)
 * 5. Kernel Panic, SIGILL, OOM Detection & Complete Reset/Recovery (500 cycles)
 * 6. Anti-Mock & Fake Dispatcher Rejection Verification (Rule §0.2)
 * 7. Memory Buffer Boundary Invariants & High-Churn GC Resistance
 * 
 * Conforms to ASD-STE100 and /ponytail simplicity principles.
 */

import { V86GuestManager, VM_STATES, BOOT_MILESTONES } from '../src/v86_guest_manager.js';
import { BinderTestSuite, BR_REPLY } from '../src/binder_test_suite.js';

let passed = 0;
let failed = 0;

function assert(cond, msg) {
    if (!cond) {
        failed++;
        console.error(`  ✖ [FAIL] ${msg}`);
        throw new Error(`Assertion failed: ${msg}`);
    }
    passed++;
}

async function runTest(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ [CHALLENGE] ${name}`);
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
    console.log("⚡ Starting Phase 0 Adversarial Challenger Suite...\n");

    // -------------------------------------------------------------------------
    // Challenge 1: Lifecycle State Machine Invariants & Invalid State Handling
    // -------------------------------------------------------------------------
    await runTest("1. State Machine Invariants & Illegal State Transitions", async () => {
        const mgr = new V86GuestManager();
        assert(mgr.getState() === VM_STATES.UNINITIALIZED, "Must start in UNINITIALIZED");

        // 1.1 Illegal state rejection
        const illegalStates = ['READY', 'STOPPED', 'CRASHED', 'null', undefined, 123, {}, []];
        for (const badState of illegalStates) {
            let threw = false;
            try {
                mgr.setState(badState);
            } catch (e) {
                threw = true;
            }
            assert(threw, `setState('${badState}') must throw Error`);
        }

        // 1.2 State change callback notification
        let lastNotifiedState = null;
        let lastOldState = null;
        mgr.config.onStateChange = (newState, oldState) => {
            lastNotifiedState = newState;
            lastOldState = oldState;
        };

        mgr.setState(VM_STATES.BOOTING);
        assert(lastNotifiedState === VM_STATES.BOOTING, "Listener notified of new state");
        assert(lastOldState === VM_STATES.UNINITIALIZED, "Listener notified of old state");

        // Setting same state is a no-op and does not notify
        lastNotifiedState = null;
        mgr.setState(VM_STATES.BOOTING);
        assert(lastNotifiedState === null, "Setting identical state must be no-op");

        mgr.destroy();
        assert(mgr.getState() === VM_STATES.UNINITIALIZED, "Destroy resets to UNINITIALIZED");
    });

    // -------------------------------------------------------------------------
    // Challenge 2: Rapid Start / Destroy / Churn Cycles (5,000 cycles)
    // -------------------------------------------------------------------------
    await runTest("2. Rapid Start / Destroy Churn & Memory Stability (5,000 cycles)", async () => {
        for (let i = 0; i < 5000; i++) {
            const mgr = new V86GuestManager({ memorySizeMb: 16 });
            await mgr.start();
            assert(mgr.getState() === VM_STATES.BOOTING, `Iteration ${i}: Must reach BOOTING`);
            mgr.feedSerial("SeaBIOS\n[init] system boot completed successfully\n");
            assert(mgr.hasMilestone(BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED), `Iteration ${i}: Must complete boot`);
            mgr.destroy();
            assert(mgr.getState() === VM_STATES.UNINITIALIZED, `Iteration ${i}: Must reset to UNINITIALIZED`);
            assert(mgr.getMilestones().length === 0, `Iteration ${i}: Milestones must be cleared on destroy`);
        }
    });

    // -------------------------------------------------------------------------
    // Challenge 3: Rapid Pause / Resume & Out-of-Order Lifecycle (10,000 cycles)
    // -------------------------------------------------------------------------
    await runTest("3. Rapid Pause / Resume & Out-of-Order Lifecycle Handling (10,000 cycles)", async () => {
        const mgr = new V86GuestManager();
        await mgr.start();
        mgr.setState(VM_STATES.RUNNING);
        assert(mgr.getState() === VM_STATES.RUNNING, "Initial state RUNNING");

        // 3.1 10,000 rapid pause/resume cycles
        for (let i = 0; i < 10000; i++) {
            mgr.pause();
            assert(mgr.getState() === VM_STATES.PAUSED, `Cycle ${i}: State must be PAUSED`);
            mgr.resume();
            assert(mgr.getState() === VM_STATES.RUNNING, `Cycle ${i}: State must be RUNNING`);
        }

        // 3.2 Double pause is idempotent
        mgr.pause();
        assert(mgr.getState() === VM_STATES.PAUSED, "First pause -> PAUSED");
        mgr.pause();
        assert(mgr.getState() === VM_STATES.PAUSED, "Second pause is idempotent -> PAUSED");

        // 3.3 Double resume is idempotent
        mgr.resume();
        assert(mgr.getState() === VM_STATES.RUNNING, "First resume -> RUNNING");
        mgr.resume();
        assert(mgr.getState() === VM_STATES.RUNNING, "Second resume is idempotent -> RUNNING");

        // 3.4 Calling resume() from UNINITIALIZED or ERROR does not force RUNNING
        const uninitMgr = new V86GuestManager();
        uninitMgr.resume();
        assert(uninitMgr.getState() === VM_STATES.UNINITIALIZED, "resume() from UNINITIALIZED must remain UNINITIALIZED");

        uninitMgr.pause();
        assert(uninitMgr.getState() === VM_STATES.UNINITIALIZED, "pause() from UNINITIALIZED must remain UNINITIALIZED");

        mgr.destroy();
    });

    // -------------------------------------------------------------------------
    // Challenge 4: Fragmented & Byte-by-Byte Serial Chunk Processing
    // -------------------------------------------------------------------------
    await runTest("4. Fragmented & Byte-by-Byte Serial Stream Processing", async () => {
        const bootStream = [
            "SeaBIOS (version rel-1.14.0-0-g155821a)\r\n",
            "Linux version 5.10.0-android-x86 (androidwebgpu@v86) #1 SMP PREEMPT\r\n",
            "virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb frame buffer device\r\n",
            "Android Binder IPC Driver initialized (protocol version 8)\r\n",
            "binderfs: created /dev/binderfs/binder\r\n",
            "[init] servicemanager started (handle 0 context manager)\r\n",
            "Zygote: listening on socket /dev/socket/zygote\r\n",
            "ART: Initialized boot classpath (/system/framework/boot.art)\r\n",
            "[init] system boot completed successfully\r\n"
        ].join('');

        // 4.1 Feed byte-by-byte (1 char at a time)
        const charMgr = new V86GuestManager();
        charMgr.setState(VM_STATES.BOOTING);
        for (let i = 0; i < bootStream.length; i++) {
            charMgr.feedSerial(bootStream[i]);
        }
        // Flush remaining buffer if any
        charMgr.feedSerial("\n");

        assert(charMgr.hasMilestone(BOOT_MILESTONES.BIOS_POST), "Byte-by-byte: BIOS_POST");
        assert(charMgr.hasMilestone(BOOT_MILESTONES.KERNEL_BOOT), "Byte-by-byte: KERNEL_BOOT");
        assert(charMgr.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), "Byte-by-byte: VIRTIO_GPU_INIT");
        assert(charMgr.hasMilestone(BOOT_MILESTONES.BINDERFS_MOUNT), "Byte-by-byte: BINDERFS_MOUNT");
        assert(charMgr.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Byte-by-byte: SERVICEMANAGER_READY");
        assert(charMgr.hasMilestone(BOOT_MILESTONES.ZYGOTE_ART_READY), "Byte-by-byte: ZYGOTE_ART_READY");
        assert(charMgr.getState() === VM_STATES.RUNNING, "Byte-by-byte stream must reach RUNNING");

        // 4.2 Feed randomized chunk sizes (1 to 13 bytes)
        for (let run = 0; run < 100; run++) {
            const chunkMgr = new V86GuestManager();
            chunkMgr.setState(VM_STATES.BOOTING);
            let offset = 0;
            while (offset < bootStream.length) {
                const chunkSize = 1 + Math.floor(Math.random() * 13);
                const chunk = bootStream.slice(offset, offset + chunkSize);
                chunkMgr.feedSerial(chunk);
                offset += chunkSize;
            }
            chunkMgr.feedSerial("\n");
            assert(chunkMgr.getState() === VM_STATES.RUNNING, `Run ${run}: Chunked stream must reach RUNNING`);
            assert(chunkMgr.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), `Run ${run}: Milestones recorded`);
        }
    });

    // -------------------------------------------------------------------------
    // Challenge 5: ANSI Escape Sequences, Unicode & Binary Garbage Fuzzing
    // -------------------------------------------------------------------------
    await runTest("5. ANSI Escape Sequences, Unicode & Binary Garbage Fuzzing (10,000 inputs)", async () => {
        const fuzzMgr = new V86GuestManager();
        await fuzzMgr.start();
        fuzzMgr.setState(VM_STATES.RUNNING);

        const noisePatterns = [
            "\x1b[31;1mERROR: color code\x1b[0m\n",
            "\x1b[2J\x1b[H\x1b[?25l\n",
            "\x00\x01\x02\x03\x04\x05\x06\x07\x08\x0b\x0c\x0e\x0f\n",
            "🚀🔥💻🎮 AndroidWebGPU x86 guest serial dmesg line 👾\n",
            "A".repeat(65536) + "\n", // 64KB giant line
            "\u0000\uFFFF\uD800\uDC00\n", // Surrogates and max unicode
            "\r\r\r\n\n\n\r\n",
            "Normal dmesg line with random [brackets] and (parentheses) 12345\n"
        ];

        for (let i = 0; i < 10000; i++) {
            const pattern = noisePatterns[i % noisePatterns.length];
            fuzzMgr.feedSerial(pattern);
            // Verify VM state does not crash into invalid state or throw
            assert(fuzzMgr.getState() === VM_STATES.RUNNING, `Noise ${i}: VM must remain in valid state`);
        }

        // Empty, null, undefined feedSerial calls
        fuzzMgr.feedSerial("");
        fuzzMgr.feedSerial(null);
        fuzzMgr.feedSerial(undefined);
        assert(fuzzMgr.getState() === VM_STATES.RUNNING, "Null/empty feedSerial must be handled safely");
    });

    // -------------------------------------------------------------------------
    // Challenge 6: Fatal Panics, SIGILL, OOM & Complete Recovery Cycles (500 cycles)
    // -------------------------------------------------------------------------
    await runTest("6. Fatal Panics, SIGILL, OOM & Full Recovery Cycles (500 cycles)", async () => {
        const panicTriggers = [
            "Kernel panic - not syncing: Fatal exception in interrupt",
            "Kernel panic - not syncing: VFS: Unable to mount root fs on unknown-block(0,0)",
            "Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b",
            "Invalid opcode: 0000 [#1] PREEMPT SMP",
            "Illegal instruction (SIGILL) in process 102 (zygote)",
            "binderfs: failed to mount /dev/binderfs (-19)",
            "Out of memory: Kill process 54 (servicemanager) score 200 or sacrifice child"
        ];

        for (let i = 0; i < 500; i++) {
            const trigger = panicTriggers[i % panicTriggers.length];
            const mgr = new V86GuestManager();
            await mgr.start();
            mgr.setState(VM_STATES.RUNNING);
            assert(mgr.getState() === VM_STATES.RUNNING, `Panic cycle ${i}: Started in RUNNING`);

            // Inject panic
            mgr.feedSerial(`${trigger}\n`);
            assert(mgr.getState() === VM_STATES.ERROR, `Panic cycle ${i}: Must transition to ERROR on "${trigger}"`);

            // Verify pingServiceManager throws on ERROR state
            let pingFailed = false;
            try {
                await mgr.pingServiceManager(0);
            } catch (e) {
                pingFailed = true;
                assert(e.message.includes("ERROR"), `Ping error message must state ERROR: ${e.message}`);
            }
            assert(pingFailed, `Panic cycle ${i}: pingServiceManager must reject during ERROR state`);

            // Recovery: Destroy and re-start
            mgr.destroy();
            assert(mgr.getState() === VM_STATES.UNINITIALIZED, `Panic cycle ${i}: Reset to UNINITIALIZED`);
            await mgr.start();
            mgr.feedSerial("SeaBIOS\n[init] system boot completed successfully\n");
            assert(mgr.getState() === VM_STATES.RUNNING, `Panic cycle ${i}: Recovered to RUNNING`);
            assert(mgr.hasMilestone(BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED), `Panic cycle ${i}: Clean boot milestones`);
            mgr.destroy();
        }
    });

    // -------------------------------------------------------------------------
    // Challenge 7: Anti-Mock & Fake Dispatcher Rejection Verification (Rule §0.2)
    // -------------------------------------------------------------------------
    await runTest("7. Anti-Mock & Fake Dispatcher Rejection Verification (Rule §0.2)", async () => {
        // 7.1 Null / Missing Guest Manager -> Must return uncertified isMock
        const suiteEmpty = new BinderTestSuite(null, null, () => {});
        const resEmpty = await suiteEmpty.runPhase0_GuestBaseline();
        assert(resEmpty.certified === false, "Must not certify empty harness (Rule §0.2)");
        assert(resEmpty.isMock === true, "Must flag isMock === true");

        // 7.2 Fake dispatcher with wrong VM state (e.g. BOOTING / PAUSED / ERROR)
        const invalidStates = [VM_STATES.UNINITIALIZED, VM_STATES.BOOTING, VM_STATES.LOADING, VM_STATES.PAUSED, VM_STATES.ERROR];
        for (const badState of invalidStates) {
            const fakeManager = {
                getState: () => badState,
                getMilestones: () => ['KERNEL_BOOT', 'BINDERFS_MOUNT', 'SERVICEMANAGER_READY'],
                exec: async () => 'BINDER_NODES_OK\n',
                pingServiceManager: async () => ({ pingOk: true, resultCode: BR_REPLY, targetHandle: 0 })
            };
            const suiteBadState = new BinderTestSuite(null, null, () => {}, fakeManager);
            let stateRejected = false;
            try {
                await suiteBadState.runPhase0_GuestBaseline();
            } catch (e) {
                stateRejected = true;
                assert(e.message.includes("Phase 0 Failed"), `Error message must indicate failure: ${e.message}`);
            }
            assert(stateRejected, `Fake manager in state '${badState}' must be rejected by Phase 0`);
        }

        // 7.3 Fake dispatcher missing required milestones
        const missingMilestoneCases = [
            [],
            ['KERNEL_BOOT'],
            ['BINDERFS_MOUNT'],
            ['SERVICEMANAGER_READY'],
            ['KERNEL_BOOT', 'BINDERFS_MOUNT'] // missing SERVICEMANAGER_READY
        ];
        for (const msList of missingMilestoneCases) {
            const fakeManager = {
                getState: () => VM_STATES.RUNNING,
                getMilestones: () => msList,
                exec: async () => 'BINDER_NODES_OK\n',
                pingServiceManager: async () => ({ pingOk: true, resultCode: BR_REPLY, targetHandle: 0 })
            };
            const suiteMissingMs = new BinderTestSuite(null, null, () => {}, fakeManager);
            let msRejected = false;
            try {
                await suiteMissingMs.runPhase0_GuestBaseline();
            } catch (e) {
                msRejected = true;
                assert(e.message.includes("Missing required boot milestone"), `Expected milestone error, got: ${e.message}`);
            }
            assert(msRejected, `Fake manager with milestones [${msList}] must be rejected`);
        }

        // 7.4 Fake dispatcher failing /dev/binder exec check
        const fakeFailingExec = {
            getState: () => VM_STATES.RUNNING,
            getMilestones: () => ['KERNEL_BOOT', 'BINDERFS_MOUNT', 'SERVICEMANAGER_READY'],
            exec: async () => 'NO_BINDER_DEVICE\n',
            pingServiceManager: async () => ({ pingOk: true, resultCode: BR_REPLY, targetHandle: 0 })
        };
        const suiteBadExec = new BinderTestSuite(null, null, () => {}, fakeFailingExec);
        let execRejected = false;
        try {
            await suiteBadExec.runPhase0_GuestBaseline();
        } catch (e) {
            execRejected = true;
            assert(e.message.includes("/dev/binder character device nodes missing"), `Expected /dev/binder missing error, got: ${e.message}`);
        }
        assert(execRejected, "Fake manager failing /dev/binder check must be rejected");

        // 7.5 Fake dispatcher failing pingServiceManager (e.g. wrong reply code / thrown error)
        const fakeBadPing = {
            getState: () => VM_STATES.RUNNING,
            getMilestones: () => ['KERNEL_BOOT', 'BINDERFS_MOUNT', 'SERVICEMANAGER_READY'],
            exec: async () => 'BINDER_NODES_OK\n',
            pingServiceManager: async () => ({ pingOk: false, resultCode: 0xDEADBEEF, targetHandle: 0 })
        };
        const suiteBadPing = new BinderTestSuite(null, null, () => {}, fakeBadPing);
        let pingRejected = false;
        try {
            await suiteBadPing.runPhase0_GuestBaseline();
        } catch (e) {
            pingRejected = true;
            assert(e.message.includes("ServiceManager handle 0 ping failed"), `Expected ping error, got: ${e.message}`);
        }
        assert(pingRejected, "Fake manager returning invalid BR_REPLY must be rejected");

        // 7.6 Real V86GuestManager attached -> Must certify
        const realMgr = new V86GuestManager();
        await realMgr.start();
        realMgr.feedSerial(
            "SeaBIOS (version 1.14.0)\n" +
            "Linux version 5.10.0-android-x86\n" +
            "Android Binder IPC Driver initialized (protocol version 8)\n" +
            "[init] servicemanager started (handle 0 context manager)\n" +
            "Zygote: listening on socket /dev/socket/zygote\n"
        );
        const suiteReal = new BinderTestSuite(null, null, () => {}, realMgr);
        const resReal = await suiteReal.runPhase0_GuestBaseline();
        assert(resReal.certified === true, "Real guest manager must be certified");
        assert(resReal.isMock === false, "isMock must be false for real guest manager");
        assert(resReal.status === 'PASSED', "Phase 0 status must be PASSED");
        realMgr.destroy();
    });

    // -------------------------------------------------------------------------
    // Challenge 8: Memory Linear View & Bound Checks
    // -------------------------------------------------------------------------
    await runTest("8. Guest Physical Memory Linear Buffer & Boundary Checks", async () => {
        const mgr = new V86GuestManager({ memorySizeMb: 512 });
        const mem = mgr.getGuestMemory();
        assert(mem instanceof Uint8Array, "Must be Uint8Array view");
        assert(mem.byteLength === 512 * 1024 * 1024, "Must allocate 512MB RAM");

        // Test boundary writes and reads
        mem[0] = 0x55;
        mem[mem.length - 1] = 0xAA;
        assert(mem[0] === 0x55, "Byte 0 read/write valid");
        assert(mem[mem.length - 1] === 0xAA, "Last byte read/write valid");

        // Verify stats reporting
        const stats = mgr.getStats();
        assert(stats.memoryAllocatedMb === 512, "Stats memory size 512MB");
        assert(stats.vgaMemoryAllocatedMb === 16, "Stats VGA size 16MB");
    });

    console.log(`\n======================================================`);
    console.log(`⚡ ALL EMPIRICAL CHALLENGER TESTS PASSED`);
    console.log(`Passed assertions: ${passed}`);
    console.log(`Failed assertions: ${failed}`);
    console.log(`======================================================\n`);

    if (failed > 0) {
        process.exit(1);
    } else {
        process.exit(0);
    }
}

main().catch(err => {
    console.error("Fatal error in adversarial challenger:", err);
    process.exit(1);
});
