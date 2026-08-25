/**
 * Automated 5-Stage Test Runner for Phase 0 v86 Guest Boot & Baseline Integration
 * 
 * Verifies:
 * 1. Stage 1: Guest Boot Artifacts & x86 ISA Configuration
 * 2. Stage 2: V86GuestManager State Machine & Options Matrix (9 Lifecycle States)
 * 3. Stage 3: Serial Boot Milestones Parser & Panic/SIGILL Error Detector
 * 4. Stage 4: /dev/binder Device Nodes, Protocol V8, & ServiceManager Handle 0 Ping
 * 5. Stage 5: Zero-Mock Invariant & Anti-Mock Certification Traps
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES } from '../src/v86_guest_manager.js';
import { BinderTestSuite } from '../src/binder_test_suite.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalAssertions = 0;
let failedAssertions = 0;

function assert(condition, message) {
    totalAssertions++;
    if (!condition) {
        failedAssertions++;
        console.error(`  ✖ [FAIL] ${message}`);
        throw new Error(`Assertion Failed: ${message}`);
    }
    // console.log(`  ✔ [PASS] ${message}`);
}

async function runStage(stageNum, name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ Stage ${stageNum}: ${name}`);
    console.log(`======================================================`);
    try {
        await fn();
        console.log(`✔ [STAGE ${stageNum} PASS] ${name}`);
    } catch (err) {
        console.error(`✖ [STAGE ${stageNum} FAIL] ${err.message}`);
        throw err;
    }
}

async function main() {
    console.log("⚡ Starting Phase 0 v86 Guest Boot & Baseline Test Runner...\n");

    // -------------------------------------------------------------------------
    // Stage 1: Guest Boot Artifacts & x86 ISA Configuration
    // -------------------------------------------------------------------------
    await runStage(1, "Guest Boot Artifacts & x86 ISA Configuration", async () => {
        // 1.1 Kernel defconfig inspection
        const defconfigPath = path.join(rootDir, 'guest/kernel/android_x86_defconfig');
        assert(fs.existsSync(defconfigPath), "guest/kernel/android_x86_defconfig must exist");
        const defconfigContent = fs.readFileSync(defconfigPath, 'utf8');

        assert(defconfigContent.includes('CONFIG_X86_32=y'), "Defconfig must target 32-bit x86 (CONFIG_X86_32=y)");
        assert(defconfigContent.includes('CONFIG_ANDROID_BINDER_IPC=y'), "Defconfig must enable CONFIG_ANDROID_BINDER_IPC=y");
        assert(defconfigContent.includes('CONFIG_ANDROID_BINDERFS=y'), "Defconfig must enable CONFIG_ANDROID_BINDERFS=y");
        assert(defconfigContent.includes('CONFIG_DRM_VIRTIO_GPU=y'), "Defconfig must enable CONFIG_DRM_VIRTIO_GPU=y");
        assert(defconfigContent.includes('CONFIG_VIRTIO=y'), "Defconfig must enable CONFIG_VIRTIO=y");
        assert(defconfigContent.includes('CONFIG_SERIAL_8250=y'), "Defconfig must enable CONFIG_SERIAL_8250=y");
        assert(defconfigContent.includes('CONFIG_ASHMEM=y'), "Defconfig must enable CONFIG_ASHMEM=y");

        // 1.2 Root Init Script inspection
        const initPath = path.join(rootDir, 'guest/initrd/init');
        assert(fs.existsSync(initPath), "guest/initrd/init must exist");
        const initContent = fs.readFileSync(initPath, 'utf8');

        assert(initContent.includes('mount -t binder binder /dev/binderfs'), "Init script must mount binderfs");
        assert(initContent.includes('ln -sf /dev/binderfs/binder /dev/binder'), "Init script must symlink /dev/binder");
        assert(initContent.includes('/system/bin/servicemanager'), "Init script must start servicemanager");
        assert(initContent.includes('/system/bin/pms_rs'), "Init script must start pms_rs");
        assert(initContent.includes('/system/bin/ams_rs'), "Init script must start ams_rs");
        assert(initContent.includes('/system/bin/wms_rs'), "Init script must start wms_rs");
        assert(initContent.includes('/system/bin/inputflinger_rs'), "Init script must start inputflinger_rs");

        // 1.3 Initrd packaging script
        const buildInitrdPath = path.join(rootDir, 'guest/tools/build_initrd.sh');
        assert(fs.existsSync(buildInitrdPath), "guest/tools/build_initrd.sh must exist");

        // 1.4 VINTF Device Manifest inspection
        const vintfPath = path.join(rootDir, 'guest/etc/vintf/device_manifest.xml');
        assert(fs.existsSync(vintfPath), "guest/etc/vintf/device_manifest.xml must exist");
        const vintfContent = fs.readFileSync(vintfPath, 'utf8');
        assert(vintfContent.includes('target-level="7"'), "VINTF manifest must declare target-level 7");
        assert(vintfContent.includes('android.hardware.sensors'), "VINTF manifest must declare ISensors");
        assert(vintfContent.includes('android.hardware.audio'), "VINTF manifest must declare IModule");
        assert(vintfContent.includes('android.hardware.camera.provider'), "VINTF manifest must declare ICameraProvider");
    });

    // -------------------------------------------------------------------------
    // Stage 2: V86GuestManager State Machine & Options Matrix
    // -------------------------------------------------------------------------
    await runStage(2, "V86GuestManager State Machine & Options Matrix", async () => {
        // 2.1 Validate all 9 lifecycle states
        const expectedStates = [
            'UNINITIALIZED', 'LOADING', 'BOOTING', 'KERNEL_READY',
            'BINDER_READY', 'SERVICES_READY', 'RUNNING', 'PAUSED', 'ERROR'
        ];
        for (const s of expectedStates) {
            assert(VM_STATES[s] === s, `VM_STATES must contain ${s}`);
        }

        // 2.2 Instance initialization
        const stateTransitions = [];
        const manager = new V86GuestManager({
            memorySizeMb: 512,
            vgaMemorySizeMb: 16,
            onStateChange: (newState, oldState) => {
                stateTransitions.push({ from: oldState, to: newState });
            }
        });

        assert(manager.getState() === VM_STATES.UNINITIALIZED, "Initial state must be UNINITIALIZED");

        // 2.3 Boot execution
        await manager.start();
        assert(manager.getState() === VM_STATES.RUNNING, "After start, state must reach RUNNING");
        assert(stateTransitions.length >= 4, "State transitions must occur during boot");

        // 2.4 Pause & Resume
        manager.pause();
        assert(manager.getState() === VM_STATES.PAUSED, "State must transition to PAUSED");

        manager.resume();
        assert(manager.getState() === VM_STATES.RUNNING, "State must transition back to RUNNING");

        // 2.5 Destroy & Reset
        manager.destroy();
        assert(manager.getState() === VM_STATES.UNINITIALIZED, "State must reset to UNINITIALIZED");
    });

    // -------------------------------------------------------------------------
    // Stage 3: Serial Boot Milestones Parser & Error Detector
    // -------------------------------------------------------------------------
    await runStage(3, "Serial Boot Milestones Parser & Panic/SIGILL Error Detector", async () => {
        const recordedMilestones = [];
        const manager = new V86GuestManager({
            onMilestone: (m) => recordedMilestones.push(m)
        });

        // 3.1 Feed sequential serial lines and assert milestone detection
        manager.setState(VM_STATES.BOOTING);
        manager.feedSerial("SeaBIOS (version 1.14.0)\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.BIOS_POST), "Milestone BIOS_POST must be recorded");

        manager.feedSerial("Linux version 5.10.0-android-x86 (androidwebgpu@v86)\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.KERNEL_BOOT), "Milestone KERNEL_BOOT must be recorded");
        assert(manager.getState() === VM_STATES.KERNEL_READY, "State must transition to KERNEL_READY");

        manager.feedSerial("virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), "Milestone VIRTIO_GPU_INIT must be recorded");

        manager.feedSerial("Android Binder IPC Driver initialized (protocol version 8)\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.BINDERFS_MOUNT), "Milestone BINDERFS_MOUNT must be recorded");
        assert(manager.getState() === VM_STATES.BINDER_READY, "State must transition to BINDER_READY");

        manager.feedSerial("[init] servicemanager started (handle 0 context manager)\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Milestone SERVICEMANAGER_READY must be recorded");
        assert(manager.getState() === VM_STATES.SERVICES_READY, "State must transition to SERVICES_READY");

        manager.feedSerial("Zygote: listening on socket /dev/socket/zygote\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.ZYGOTE_ART_READY), "Milestone ZYGOTE_ART_READY must be recorded");
        assert(manager.getState() === VM_STATES.RUNNING, "State must transition to RUNNING");

        // 3.2 Adversarial Panic / SIGILL Error Detector Tests
        const panicManager = new V86GuestManager();
        await panicManager.start();
        assert(panicManager.getState() === VM_STATES.RUNNING, "Panic manager started in RUNNING");

        panicManager.feedSerial("Kernel panic - not syncing: Fatal exception in interrupt\n");
        assert(panicManager.getState() === VM_STATES.ERROR, "Kernel panic line must transition VM to ERROR");

        const sigillManager = new V86GuestManager();
        await sigillManager.start();
        sigillManager.feedSerial("Illegal instruction (SIGILL) in process 102 (zygote)\n");
        assert(sigillManager.getState() === VM_STATES.ERROR, "SIGILL line must transition VM to ERROR");

        const oomManager = new V86GuestManager();
        await oomManager.start();
        oomManager.feedSerial("Out of memory: Kill process 54 (servicemanager)\n");
        assert(oomManager.getState() === VM_STATES.ERROR, "OOM kill of core daemon must transition VM to ERROR");
    });

    // -------------------------------------------------------------------------
    // Stage 4: /dev/binder Device Nodes, Protocol V8, & ServiceManager Ping
    // -------------------------------------------------------------------------
    await runStage(4, "/dev/binder Device Nodes, Protocol V8, & ServiceManager Handle 0 Ping", async () => {
        const manager = new V86GuestManager();
        await manager.start();

        // 4.1 Check /dev/binder nodes via guest exec
        const binderCheck = await manager.exec('test -c /dev/binder && test -c /dev/hwbinder && echo BINDER_NODES_OK');
        assert(binderCheck.includes('BINDER_NODES_OK'), "/dev/binder character devices must exist in guest");

        const lsOutput = await manager.exec('ls -l /dev/binder /dev/hwbinder /dev/vndbinder');
        assert(lsOutput.includes('crw-rw-rw-'), "/dev/binder must have permissions 0666 (crw-rw-rw-)");
        assert(lsOutput.includes('/dev/binder'), "/dev/binder node listed");
        assert(lsOutput.includes('/dev/hwbinder'), "/dev/hwbinder node listed");
        assert(lsOutput.includes('/dev/vndbinder'), "/dev/vndbinder node listed");

        // 4.2 Protocol version 8
        const verOutput = await manager.exec('cat /sys/module/binder/version');
        assert(verOutput.trim() === '8', "Kernel Binder protocol version must be 8");

        // 4.3 Dumpsys services listing
        const dumpsysOutput = await manager.exec('dumpsys');
        assert(dumpsysOutput.includes('activity'), "dumpsys must report 'activity' service");
        assert(dumpsysOutput.includes('window'), "dumpsys must report 'window' service");
        assert(dumpsysOutput.includes('package'), "dumpsys must report 'package' service");
        assert(dumpsysOutput.includes('input'), "dumpsys must report 'input' service");

        // 4.4 ServiceManager Root Handle 0 Ping
        const pingResp = await manager.pingServiceManager(0);
        assert(pingResp.pingOk === true, "Ping to root handle 0 must succeed");
        assert(pingResp.resultCode === 0x80407203, "Result code must match BR_REPLY (0x80407203)");
        assert(pingResp.targetHandle === 0, "Target handle must be 0");

        // 4.5 Ping with non-root handle must fail
        let pingErr = null;
        try {
            await manager.pingServiceManager(999);
        } catch (e) {
            pingErr = e;
        }
        assert(pingErr !== null, "Ping to invalid handle 999 must throw error");

        // 4.6 Memory linear buffer access
        const mem = manager.getGuestMemory();
        assert(mem instanceof Uint8Array, "Guest memory must be Uint8Array view");
        assert(mem.length === 512 * 1024 * 1024, "Guest memory must be 512 MB");
    });

    // -------------------------------------------------------------------------
    // Stage 5: Zero-Mock Invariant & Anti-Mock Certification Traps
    // -------------------------------------------------------------------------
    await runStage(5, "Zero-Mock Invariant & Anti-Mock Certification Traps", async () => {
        // 5.1 Rule §0.2 enforcement: Unattached test suite cannot certify Phase 0
        const suiteNoVM = new BinderTestSuite(null, null, () => {});
        const unattachedRes = await suiteNoVM.runPhase0_GuestBaseline();
        assert(unattachedRes.certified === false, "Unattached test suite must NOT certify Phase 0 (Rule §0.2)");
        assert(unattachedRes.status === 'UNVERIFIED_MOCK' || unattachedRes.isMock === true, "Unattached suite must flag UNVERIFIED_MOCK or isMock");

        // 5.2 Attached test suite certifies Phase 0
        const realManager = new V86GuestManager();
        await realManager.start();

        const suiteWithVM = new BinderTestSuite(null, null, () => {}, realManager);
        const attachedRes = await suiteWithVM.runPhase0_GuestBaseline();
        assert(attachedRes.status === 'PASSED', "Attached test suite Phase 0 must PASS");
        assert(attachedRes.certified === true, "Attached test suite must certify Phase 0");
        assert(attachedRes.details.vmState === 'RUNNING', "VM State must be RUNNING");

        // 5.3 E2E-0 Test Method Execution
        const e2e0Res = await suiteWithVM.runE2E0_GuestBaseline();
        assert(e2e0Res.status === 'PASSED', "E2E-0 method must PASS");
        assert(e2e0Res.certified === true, "E2E-0 must certify guest baseline");

        // 5.4 Stats object validation
        const stats = realManager.getStats();
        assert(stats.state === 'RUNNING', "Stats state must be RUNNING");
        assert(stats.memoryAllocatedMb === 512, "Stats RAM must be 512 MB");
        assert(Array.isArray(stats.milestones) && stats.milestones.length >= 5, "Stats must record milestones");
    });

    console.log(`\n======================================================`);
    console.log(`⚡ ALL 5 STAGES OF PHASE 0 GUEST BOOT VERIFIED`);
    console.log(`Total assertions passed: ${totalAssertions}`);
    console.log(`Total assertions failed: ${failedAssertions}`);
    console.log(`======================================================\n`);

    if (failedAssertions > 0) {
        process.exit(1);
    } else {
        process.exit(0);
    }
}

main().catch(err => {
    console.error("Fatal error in test_v86_guest_boot:", err);
    process.exit(1);
});
