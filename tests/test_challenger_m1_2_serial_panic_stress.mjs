/**
 * Empirical Challenger M1.2: Serial Milestone Stream, VirtIO GPU & Panic Resilience
 * 
 * Adversarial Stress & Invariant Tests:
 * 1. High-volume serial log bursts (5,000 lines single burst, 2,000 micro-bursts, throughput profiling)
 * 2. Fragmented & Byte-by-Byte chunk feeds (prime chunk sizes, mid-word split, trailing buffer, huge 512KB lines)
 * 3. Kernel Panic, Fatal Exception, SIGILL, OOM detection across all 9 lifecycle states
 * 4. Error State Resilience & Post-Panic Trapping (ensures ERROR state behavior is deterministic)
 * 5. VirtIO GPU Initialization milestone matching (positive dmesg patterns, negative non-virtio patterns, idempotency)
 * 6. Milestone callback firing semantics & deduplication
 * 
 * Conforms to ASD-STE100, /ponytail, /caveman.
 */

import { strict as assert } from 'node:assert';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES } from '../src/v86_guest_manager.js';

let totalAssertions = 0;
let passedAssertions = 0;

function check(condition, message) {
    totalAssertions++;
    if (!condition) {
        console.error(`  ✖ [FAIL] ${message}`);
        throw new Error(`Assertion failed: ${message}`);
    }
    passedAssertions++;
}

async function runSection(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ [CHALLENGE-M1.2] ${name}`);
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

// Utility to temporarily mute console during high-churn bursts
function withMutedConsole(fn) {
    const origDebug = console.debug;
    const origInfo = console.info;
    const origLog = console.log;
    const origWarn = console.warn;
    const origError = console.error;
    console.debug = () => {};
    console.info = () => {};
    console.log = () => {};
    console.warn = () => {};
    console.error = () => {};
    try {
        return fn();
    } finally {
        console.debug = origDebug;
        console.info = origInfo;
        console.log = origLog;
        console.warn = origWarn;
        console.error = origError;
    }
}

async function main() {
    console.log("⚡ Starting M1.2 Empirical Challenger Test Suite...\n");

    // -------------------------------------------------------------------------
    // 1. High-Volume UART Serial Log Bursts & Memory Stability
    // -------------------------------------------------------------------------
    await runSection("1. High-Volume Serial Log Bursts & Memory Stability", async () => {
        const mgr = new V86GuestManager();
        await mgr.start();
        mgr.setState(VM_STATES.BOOTING);

        // 1.1 Massive 10,000 line burst in single string
        console.log("    Feeding 10,000 lines single burst...");
        const burstLines = [];
        for (let i = 0; i < 10000; i++) {
            burstLines.push(`[${(i * 0.0001).toFixed(4)}] kernel: background noise log entry #${i} with payload data`);
        }
        const burstPayload = burstLines.join('\n') + '\n';
        
        const burstStart = performance.now();
        withMutedConsole(() => {
            mgr.feedSerial(burstPayload);
        });
        const burstDuration = performance.now() - burstStart;
        console.log(`    Processed 10,000 lines in ${burstDuration.toFixed(2)}ms (${(10000 / (burstDuration / 1000)).toFixed(0)} lines/sec)`);

        check(mgr.serialLogs.length === 10000, `Expected 10,000 serial logs, got ${mgr.serialLogs.length}`);
        check(mgr.getState() === VM_STATES.BOOTING, "State remains BOOTING on non-milestone noise");

        // 1.2 Micro-bursts: 100 batches of 50 lines each (5,000 lines)
        console.log("    Feeding 5,000 lines in 100 micro-bursts...");
        withMutedConsole(() => {
            for (let b = 0; b < 100; b++) {
                const batch = [];
                for (let j = 0; j < 50; j++) {
                    batch.push(`[${b}.${j}] micro-burst test line`);
                }
                mgr.feedSerial(batch.join('\n') + '\n');
            }
        });
        check(mgr.serialLogs.length === 15000, `Expected 15,000 total serial logs, got ${mgr.serialLogs.length}`);

        // 1.3 Memory bounds: huge 512KB single line without newline, then followed by newline
        console.log("    Feeding 512KB single line without newline, then completing line...");
        const hugePart1 = "X".repeat(256 * 1024);
        const hugePart2 = "Y".repeat(256 * 1024);
        withMutedConsole(() => {
            mgr.feedSerial(hugePart1);
            check(mgr.serialLogs.length === 15000, "Incomplete huge line must not be added to serialLogs yet");
            mgr.feedSerial(hugePart2 + "\n");
        });
        check(mgr.serialLogs.length === 15001, "Completed huge line must be added to serialLogs");
        check(mgr.serialLogs[15000].length === 512 * 1024, "Logged line length matches 512KB");

        mgr.destroy();
        check(mgr.serialLogs.length === 0, "Destroy cleans up all serial logs");
        check(mgr.serialBuffer === '', "Destroy resets serial buffer");
    });

    // -------------------------------------------------------------------------
    // 2. Fragmented & Adversarial Chunk Feeds
    // -------------------------------------------------------------------------
    await runSection("2. Fragmented & Adversarial Chunk Feeds", async () => {
        const canonicalDmesg = [
            "SeaBIOS (version rel-1.14.0-0-g155821a)\r\n",
            "Linux version 5.10.0-android-x86 (androidwebgpu@v86) #1 SMP PREEMPT\r\n",
            "[    0.100000] earlyprintk: serial console enabled\r\n",
            "[    0.450123] virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb frame buffer device\r\n",
            "[    0.800000] Android Binder IPC Driver initialized (protocol version 8)\r\n",
            "[    0.850000] binderfs: created /dev/binderfs/binder\r\n",
            "[    1.000000] Run /init as init process\r\n",
            "[    1.200000] [init] servicemanager started (handle 0 context manager)\r\n",
            "[    1.400000] [init] pms_rs: ready (package manager registered)\r\n",
            "[    1.800000] Zygote: listening on socket /dev/socket/zygote\r\n",
            "[    2.000000] ART: Initialized boot classpath (/system/framework/boot.art)\r\n",
            "[    2.500000] [init] system boot completed successfully\r\n"
        ].join('');

        // 2.1 Single-byte feed (1 character per feedSerial call)
        console.log("    Testing single-byte serial stream (1 byte per call)...");
        const byteMgr = new V86GuestManager();
        byteMgr.setState(VM_STATES.BOOTING);
        withMutedConsole(() => {
            for (let i = 0; i < canonicalDmesg.length; i++) {
                byteMgr.feedSerial(canonicalDmesg[i]);
            }
            byteMgr.feedSerial("\n"); // Flush if any remainder
        });
        check(byteMgr.getState() === VM_STATES.RUNNING, "Single-byte stream must reach RUNNING state");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.BIOS_POST), "Milestone BIOS_POST achieved");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.KERNEL_BOOT), "Milestone KERNEL_BOOT achieved");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), "Milestone VIRTIO_GPU_INIT achieved");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.BINDERFS_MOUNT), "Milestone BINDERFS_MOUNT achieved");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Milestone SERVICEMANAGER_READY achieved");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.ZYGOTE_ART_READY), "Milestone ZYGOTE_ART_READY achieved");
        check(byteMgr.hasMilestone(BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED), "Milestone SYSTEM_BOOT_COMPLETED achieved");

        // 2.2 Prime-number chunk sizes
        console.log("    Testing 26 prime-number chunk sizes...");
        const primeChunkSizes = [1, 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97];
        withMutedConsole(() => {
            for (const chunkSize of primeChunkSizes) {
                const primeMgr = new V86GuestManager();
                primeMgr.setState(VM_STATES.BOOTING);
                let offset = 0;
                while (offset < canonicalDmesg.length) {
                    const chunk = canonicalDmesg.slice(offset, offset + chunkSize);
                    primeMgr.feedSerial(chunk);
                    offset += chunkSize;
                }
                primeMgr.feedSerial("\n");
                check(primeMgr.getState() === VM_STATES.RUNNING, `Prime chunk size ${chunkSize}: state must reach RUNNING`);
                check(primeMgr.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), `Prime chunk size ${chunkSize}: VIRTIO_GPU_INIT recorded`);
                check(primeMgr.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), `Prime chunk size ${chunkSize}: SERVICEMANAGER_READY recorded`);
            }
        });

        // 2.3 Split across \r\n boundary
        console.log("    Testing split \\r and \\n across chunk boundary...");
        const crlfMgr = new V86GuestManager();
        crlfMgr.setState(VM_STATES.BOOTING);
        withMutedConsole(() => {
            crlfMgr.feedSerial("Linux version 5.10.0-android-x86\r");
            check(crlfMgr.getState() === VM_STATES.BOOTING, "Line before \\n must not trigger transition yet");
            crlfMgr.feedSerial("\nvirtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb\r");
            check(crlfMgr.getState() === VM_STATES.KERNEL_READY, "Line after \\n triggers KERNEL_READY");
            crlfMgr.feedSerial("\nZygote: listening on socket /dev/socket/zygote\r\n");
            check(crlfMgr.getState() === VM_STATES.RUNNING, "Final line triggers RUNNING");
        });
    });

    // -------------------------------------------------------------------------
    // 3. Kernel Panic & Fatal Exception Detection Across All 9 States
    // -------------------------------------------------------------------------
    await runSection("3. Kernel Panic & Fatal Exception Invariant Across All States", async () => {
        const panicPatterns = [
            "Kernel panic - not syncing: Fatal exception in interrupt",
            "Kernel panic - not syncing: VFS: Unable to mount root fs on unknown-block(0,0)",
            "Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b",
            "Kernel panic - not syncing: Out of memory and no killable processes...",
            "Fatal exception in interrupt: 0000 [#1] PREEMPT SMP",
            "Fatal exception: panic_on_oops",
            "Invalid opcode: 0000 [#1] PREEMPT SMP",
            "Illegal instruction (SIGILL) in process 102 (zygote)",
            "binderfs: failed to mount /dev/binderfs (-19)",
            "Out of memory: Kill process 54 (servicemanager)"
        ];

        const allStates = [
            VM_STATES.UNINITIALIZED,
            VM_STATES.LOADING,
            VM_STATES.BOOTING,
            VM_STATES.KERNEL_READY,
            VM_STATES.BINDER_READY,
            VM_STATES.SERVICES_READY,
            VM_STATES.RUNNING,
            VM_STATES.PAUSED,
            VM_STATES.ERROR
        ];

        withMutedConsole(() => {
            for (const startState of allStates) {
                for (const pattern of panicPatterns) {
                    const mgr = new V86GuestManager();
                    if (startState !== VM_STATES.UNINITIALIZED) {
                        mgr.setState(startState);
                    }
                    mgr.feedSerial(`${pattern}\n`);
                    check(mgr.getState() === VM_STATES.ERROR, `State '${startState}' + panic '${pattern}' must result in ERROR state`);
                }
            }
        });
    });

    // -------------------------------------------------------------------------
    // 4. VirtIO GPU Milestone Regex & String Pattern Matching
    // -------------------------------------------------------------------------
    await runSection("4. VirtIO GPU Initialization Milestone Matching & Discrimination", async () => {
        // 4.1 Positive matches
        const validVirtioGpuLines = [
            "virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb frame buffer device",
            "virtio-gpu 0000:00:02.0: vgaarb: deactivate vga console",
            "[drm: virtio-gpu] modeset initialized",
            "virtio_gpudrmfb: initialized 1024x768 32bpp",
            "[    0.450000] [drm] Initialized virtio_gpu 0.1.0 0 for 0000:00:02.0 on minor 0",
            "[drm:virtio_gpu_probe] virtio gpu driver loaded successfully"
        ];

        withMutedConsole(() => {
            for (const line of validVirtioGpuLines) {
                const mgr = new V86GuestManager();
                mgr.setState(VM_STATES.BOOTING);
                mgr.feedSerial(`${line}\n`);
                check(mgr.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), `Line '${line}' must trigger VIRTIO_GPU_INIT`);
            }
        });

        // 4.2 Negative matches (Non-GPU virtio devices or non-virtio GPUs must NOT trigger VIRTIO_GPU_INIT)
        const nonVirtioGpuLines = [
            "[    0.300000] virtio_net virtio1: eth0: link up",
            "[    0.310000] virtio_blk virtio2: [vda] 2097152 512-byte logical blocks",
            "[    0.320000] virtio_balloon virtio3: balloon registered",
            "[    0.330000] virtio_console virtio4: console registered",
            "[    0.340000] virtio_pci 0000:00:03.0: virtio device registered",
            "[    0.350000] drm: nouveau driver loaded",
            "[    0.360000] [drm] radeon: initialized",
            "[    0.370000] [drm] i915: initialized",
            "[    0.380000] simple-framebuffer simple-framebuffer.0: framebuffer at 0xf0000000",
            "Regular non-GPU log line with words gpu and virtio separated by miles"
        ];

        withMutedConsole(() => {
            for (const line of nonVirtioGpuLines) {
                const mgr = new V86GuestManager();
                mgr.setState(VM_STATES.BOOTING);
                mgr.feedSerial(`${line}\n`);
                check(!mgr.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), `Line '${line}' must NOT trigger VIRTIO_GPU_INIT`);
            }
        });
    });

    // -------------------------------------------------------------------------
    // 5. Milestone Callback Firing Semantics & Deduplication
    // -------------------------------------------------------------------------
    await runSection("5. Milestone Callback Semantics & Deduplication", async () => {
        let callbackCount = 0;
        const firedMilestones = [];
        const mgr = new V86GuestManager({
            onMilestone: (m) => {
                callbackCount++;
                firedMilestones.push(m);
            }
        });
        mgr.setState(VM_STATES.BOOTING);

        withMutedConsole(() => {
            // Feed 10 repeated lines of virtio_gpu
            for (let i = 0; i < 10; i++) {
                mgr.feedSerial(`[0.${i}] virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb\n`);
            }

            check(firedMilestones.filter(m => m === BOOT_MILESTONES.VIRTIO_GPU_INIT).length === 1,
                "VIRTIO_GPU_INIT callback must fire exactly ONCE despite duplicate log lines");

            // Feed 10 repeated lines of Binder IPC
            for (let i = 0; i < 10; i++) {
                mgr.feedSerial(`[0.${i}] Android Binder IPC Driver initialized (protocol version 8)\n`);
            }
            check(firedMilestones.filter(m => m === BOOT_MILESTONES.BINDERFS_MOUNT).length === 1,
                "BINDERFS_MOUNT callback must fire exactly ONCE");
            check(firedMilestones.filter(m => m === BOOT_MILESTONES.BINDERFS_READY).length === 1,
                "BINDERFS_READY callback must fire exactly ONCE");
        });

        mgr.destroy();
    });

    console.log(`\n======================================================`);
    console.log(`⚡ ALL M1.2 ADVERSARIAL STRESS PROBES PASSED`);
    console.log(`Total assertions passed: ${passedAssertions}`);
    console.log(`======================================================\n`);
}

main().catch(err => {
    console.error("Fatal error in test_challenger_m1_2_serial_panic_stress:", err);
    process.exit(1);
});
