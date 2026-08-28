/**
 * Adversarial & Empirical Stress Test Suite for Milestone 1:
 * Kernel Boot & Memory Boundaries
 *
 * Evaluates:
 * 1. Memory configuration boundaries (128MB, 256MB, 512MB, 1024MB, 2048MB, invalid values).
 * 2. bzImage header validation against malformed magic, corrupt signatures, protocol edge cases, and truncations.
 * 3. Kernel boot cmdline parameter strict formation and critical flag validation.
 * 4. Concurrent hypervisor lifecycle stress, rapid state churn, and serial stream fuzzing.
 * 5. Kernel defconfig invariants verification (VMAP_STACK, STACKPROTECTOR_NONE, DRM, Binder).
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES, verifyBzImage } from '../src/v86_guest_manager.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;

function assert(condition, message) {
    totalTests++;
    if (!condition) {
        failedTests++;
        console.error(`  ✖ [FAIL] ${message}`);
        throw new Error(`Assertion Failed: ${message}`);
    }
    passedTests++;
}

async function testSection(title, fn) {
    console.log(`\n▶ [CHALLENGE SUITE] ${title}`);
    try {
        await fn();
        console.log(`  ✔ Passed: ${title}`);
    } catch (err) {
        console.error(`  ✖ FAILED: ${title} -> ${err.message}`);
        throw err;
    }
}

async function runAdversarialSuite() {
    console.log("===============================================================================");
    console.log("⚡ STARTING EMPIRICAL ADVERSARIAL CHALLENGE SUITE: MILESTONE 1");
    console.log("===============================================================================\n");

    const realBzImage = fs.readFileSync(path.join(rootDir, 'guest/build/bzImage'));

    // =========================================================================
    // SECTION 1: Memory Configuration Boundaries (128MB, 256MB, 512MB, 1024MB, etc.)
    // =========================================================================
    await testSection("1. Memory Configuration Boundaries in V86GuestManager", async () => {
        const testSizes = [128, 256, 512, 1024, 2048];
        
        for (const mb of testSizes) {
            const mgr = new V86GuestManager({
                memorySizeMb: mb,
                kernelUrl: './guest/build/bzImage',
                initrdUrl: './guest/build/initrd.img'
            });

            assert(mgr.config.memorySizeMb === mb, `Manager should accept memorySizeMb=${mb}`);
            await mgr.start();
            assert(mgr.getState() === VM_STATES.BOOTING, `VM should enter BOOTING with ${mb}MB`);

            const mem = mgr.getGuestMemory();
            const expectedBytes = mb * 1024 * 1024;
            assert(mem instanceof Uint8Array, `Memory view must be Uint8Array for ${mb}MB`);
            assert(mem.length === expectedBytes, `Memory size must exactly match ${expectedBytes} bytes for ${mb}MB (got ${mem.length})`);

            // Verify kernel payload mapped at 0x100000
            const kernelAt1MB = mem.subarray(0x100000, 0x100000 + 0x208);
            const bootSig = kernelAt1MB[0x1FE] | (kernelAt1MB[0x1FF] << 8);
            assert(bootSig === 0xAA55, `Kernel header at 0x100000 must have 0xAA55 signature for ${mb}MB`);

            const stats = mgr.getStats();
            assert(stats.memoryAllocatedMb === mb, `Stats memoryAllocatedMb must report ${mb}`);

            mgr.destroy();
            assert(mgr.getState() === VM_STATES.UNINITIALIZED, `VM destroyed cleanly for ${mb}MB`);
        }

        // Test memory default fallback
        const defaultMgr = new V86GuestManager();
        assert(defaultMgr.config.memorySizeMb === 512, "Default memorySizeMb must be 512MB as required by Milestone 1");
        const defaultMem = defaultMgr.getGuestMemory();
        assert(defaultMem.length === 512 * 1024 * 1024, "Default guest memory buffer must be 512MB (536870912 bytes)");
    });

    // =========================================================================
    // SECTION 2: bzImage Header Validation & Malformed Fuzzing
    // =========================================================================
    await testSection("2. bzImage Header Validation & Malformed Magic/Truncations", async () => {
        // 2.1 Baseline validation of real bzImage
        const realRes = verifyBzImage(realBzImage);
        assert(realRes.valid === true, "Real bzImage must pass verifyBzImage");
        assert(realRes.bootFlag === 0xAA55, "Real bzImage bootFlag must be 0xAA55");
        assert(realRes.headerMagic === 'HdrS', "Real bzImage headerMagic must be 'HdrS'");
        assert(realRes.protocol >= 0x0200, `Real bzImage protocol must be >= 0x0200 (got 0x${realRes.protocol.toString(16)})`);

        // 2.2 Truncated buffers
        const truncLengths = [0, 1, 16, 511, 512, 519];
        for (const len of truncLengths) {
            const buf = Buffer.alloc(len);
            const res = verifyBzImage(buf);
            assert(res.valid === false, `verifyBzImage must reject buffer of length ${len}`);
            assert(typeof res.error === 'string' && res.error.length > 0, `Error message required for length ${len}`);
        }

        // Exact boundary length: 520 bytes (0x208)
        const exactBoundary = Buffer.alloc(0x208);
        realBzImage.copy(exactBoundary, 0, 0, 0x208);
        const exactRes = verifyBzImage(exactBoundary);
        assert(exactRes.valid === true, "verifyBzImage must accept minimum valid header length (520 bytes)");

        // 2.3 Malformed Boot Sector Signature at 0x1FE
        const badSignatures = [0x0000, 0x55AA, 0xAA54, 0xFFFF, 0x1234];
        for (const sig of badSignatures) {
            const corrupted = Buffer.from(realBzImage.subarray(0, 0x400));
            corrupted.writeUInt16LE(sig, 0x1FE);
            const res = verifyBzImage(corrupted);
            assert(res.valid === false, `verifyBzImage must reject bad boot signature 0x${sig.toString(16)}`);
            assert(res.error.includes('boot sector signature'), `Error must mention boot sector signature for 0x${sig.toString(16)}`);
        }

        // 2.4 Malformed Setup Header Magic at 0x202
        const badMagics = ['HDRS', 'hdrs', 'Hdr\0', 'ELFA', 'V86M', '\0\0\0\0'];
        for (const magic of badMagics) {
            const corrupted = Buffer.from(realBzImage.subarray(0, 0x400));
            corrupted.write(magic, 0x202, 'ascii');
            const res = verifyBzImage(corrupted);
            assert(res.valid === false, `verifyBzImage must reject bad magic '${magic}'`);
            assert(res.error.includes('setup header magic'), `Error must mention setup header magic for '${magic}'`);
        }

        // 2.5 Boot Protocol Version Boundaries at 0x206
        const protocolTests = [
            { proto: 0x0000, expected: false },
            { proto: 0x0100, expected: false },
            { proto: 0x01FF, expected: false },
            { proto: 0x0200, expected: true },
            { proto: 0x0202, expected: true },
            { proto: 0x020F, expected: true },
            { proto: 0x0215, expected: true }
        ];
        for (const { proto, expected } of protocolTests) {
            const corrupted = Buffer.from(realBzImage.subarray(0, 0x400));
            corrupted.writeUInt16LE(proto, 0x206);
            const res = verifyBzImage(corrupted);
            assert(res.valid === expected, `verifyBzImage for proto 0x${proto.toString(16)} should be valid=${expected}`);
        }

        // 2.6 Input Types: Uint8Array, ArrayBuffer, Buffer, null, undefined, invalid objects
        assert(verifyBzImage(null).valid === false, "verifyBzImage(null) must be false");
        assert(verifyBzImage(undefined).valid === false, "verifyBzImage(undefined) must be false");
        assert(verifyBzImage("string-not-buffer").valid === false, "verifyBzImage(string) must be false");
        assert(verifyBzImage(12345).valid === false, "verifyBzImage(number) must be false");
        assert(verifyBzImage({}).valid === false, "verifyBzImage({}) must be false");

        const ab = realBzImage.buffer.slice(realBzImage.byteOffset, realBzImage.byteOffset + 0x400);
        assert(verifyBzImage(ab).valid === true, "verifyBzImage must accept ArrayBuffer");
        assert(verifyBzImage(new Uint8Array(ab)).valid === true, "verifyBzImage must accept Uint8Array");

        // 2.7 Hypervisor reject on corrupted bzImage during boot
        const badKernelMgr = new V86GuestManager({
            kernelUrl: './guest/build/nonexistent_or_bad'
        });
        // Mock fetchBuffer to return corrupt buffer
        badKernelMgr.fetchBuffer = async () => Buffer.alloc(100);
        
        let caughtErr = null;
        try {
            await badKernelMgr.start();
        } catch (e) {
            caughtErr = e;
        }
        assert(caughtErr !== null, "badKernelMgr.start() must throw on invalid bzImage");
        assert(badKernelMgr.getState() === VM_STATES.ERROR, "VM state must transition to ERROR on bad bzImage");
    });

    // =========================================================================
    // SECTION 3: Kernel Boot Cmdline Strict Formation
    // =========================================================================
    await testSection("3. Kernel Boot Cmdline Parameter Strict Formation", async () => {
        const mgr = new V86GuestManager();
        const cmdline = mgr.config.cmdline;
        assert(typeof cmdline === 'string', "cmdline must be a string");
        
        // Critical parameter requirements
        const requiredParams = [
            'console=ttyS0',
            'earlyprintk=serial,ttyS0,115200',
            'root=/dev/ram0',
            'rdinit=/init',
            'panic=1',
            'androidboot.hardware=android_x86',
            'androidboot.selinux=permissive',
            'video=virtio-gpu'
        ];

        for (const param of requiredParams) {
            assert(cmdline.includes(param), `Default cmdline must contain strictly formed parameter: '${param}'`);
        }

        // Validate cmdline token structure
        const tokens = cmdline.split(/\s+/);
        assert(tokens.length >= 8, `Cmdline must have at least 8 arguments (got ${tokens.length})`);
        for (const token of tokens) {
            assert(token.includes('='), `Every cmdline argument must follow key=value format: '${token}'`);
            const [key, val] = token.split('=');
            assert(key.length > 0 && val.length > 0, `Key and value must both be non-empty in '${token}'`);
        }

        // Custom cmdline override test
        const customCmd = 'console=ttyS0 root=/dev/ram0 rdinit=/init panic=1 custom_flag=1';
        const customMgr = new V86GuestManager({ cmdline: customCmd });
        assert(customMgr.config.cmdline === customCmd, "Custom cmdline must be preserved");
    });

    // =========================================================================
    // SECTION 4: Empirical Concurrency, State Churn, and Serial Stream Fuzzing
    // =========================================================================
    await testSection("4. Hypervisor Concurrency, State Churn, & Serial Fuzzing", async () => {
        // 4.1 Concurrent Manager Instances (10 simultaneous VMs)
        const instances = [];
        for (let i = 0; i < 10; i++) {
            const vm = new V86GuestManager({
                memorySizeMb: (i % 4 + 1) * 128 // 128, 256, 384, 512
            });
            instances.push(vm);
        }

        // Start all in parallel
        await Promise.all(instances.map(vm => vm.start()));
        for (const vm of instances) {
            assert(vm.getState() === VM_STATES.BOOTING, "Concurrent VM must reach BOOTING");
            assert(vm.hasMilestone(BOOT_MILESTONES.BIOS_POST), "Concurrent VM must record BIOS_POST");
        }

        // Feed serial streams concurrently
        const bootStream = [
            "SeaBIOS (version 1.14.0)\n",
            "Linux version 5.10.0-android-x86 (androidwebgpu@v86)\n",
            "virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb\n",
            "Android Binder IPC Driver initialized (protocol version 8)\n",
            "Freeing unused kernel memory: 1024K\n",
            "[init] servicemanager started (handle 0 context manager)\n",
            "Zygote: listening on socket /dev/socket/zygote\n"
        ];

        for (const vm of instances) {
            for (const line of bootStream) {
                vm.feedSerial(line);
            }
            assert(vm.getState() === VM_STATES.RUNNING, "Concurrent VM must reach RUNNING after serial progression");
            assert(vm.hasMilestone(BOOT_MILESTONES.SYSTEM_BOOT_COMPLETED), "Concurrent VM must achieve SYSTEM_BOOT_COMPLETED");
            assert(vm.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Concurrent VM must achieve SERVICEMANAGER_READY");
        }

        // Clean up all
        for (const vm of instances) {
            vm.destroy();
            assert(vm.getState() === VM_STATES.UNINITIALIZED, "Concurrent VM must reset to UNINITIALIZED");
        }

        // 4.2 Serial Stream Chunking / Fragmentation Fuzzing
        const fuzzMgr = new V86GuestManager();
        await fuzzMgr.start();

        // Feed line broken into single characters
        const testLine = "virtio_gpu virtio0: [drm] fb0: virtio_gpudrmfb\n";
        for (let i = 0; i < testLine.length; i++) {
            fuzzMgr.feedSerial(testLine[i]);
        }
        assert(fuzzMgr.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), "Milestone must be detected even when fed character-by-character");

        // Feed line broken into irregular multi-byte chunks
        const multiLine = "Android Binder IPC Driver initialized (protocol version 8)\n[init] servicemanager started\n";
        fuzzMgr.feedSerial(multiLine.slice(0, 15));
        fuzzMgr.feedSerial(multiLine.slice(15, 40));
        fuzzMgr.feedSerial(multiLine.slice(40));
        assert(fuzzMgr.hasMilestone(BOOT_MILESTONES.BINDERFS_MOUNT), "BINDERFS_MOUNT detected on fragmented chunking");
        assert(fuzzMgr.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "SERVICEMANAGER_READY detected on fragmented chunking");

        fuzzMgr.destroy();
    });

    // =========================================================================
    // SECTION 5: Kernel Defconfig Invariants & Stack Guard Oops Prevention
    // =========================================================================
    await testSection("5. Kernel Defconfig Invariants & Stack Guard Verification", async () => {
        const defconfigPath = path.join(rootDir, 'guest/kernel/android_x86_defconfig');
        assert(fs.existsSync(defconfigPath), "guest/kernel/android_x86_defconfig must exist");
        const defconfig = fs.readFileSync(defconfigPath, 'utf8');

        // M1 Critical Invariants
        assert(defconfig.includes('# CONFIG_VMAP_STACK is not set'), "Defconfig must explicitly unset CONFIG_VMAP_STACK");
        assert(defconfig.includes('# CONFIG_STACKPROTECTOR is not set'), "Defconfig must explicitly unset CONFIG_STACKPROTECTOR");
        assert(defconfig.includes('CONFIG_STACKPROTECTOR_NONE=y'), "Defconfig must set CONFIG_STACKPROTECTOR_NONE=y");
        assert(defconfig.includes('CONFIG_X86_32=y'), "Defconfig must target 32-bit x86 (CONFIG_X86_32=y)");
        assert(defconfig.includes('CONFIG_ANDROID_BINDERFS=y'), "Defconfig must enable CONFIG_ANDROID_BINDERFS=y");
        assert(defconfig.includes('CONFIG_DRM_VIRTIO_GPU=y'), "Defconfig must enable CONFIG_DRM_VIRTIO_GPU=y");
        assert(defconfig.includes('CONFIG_FRAMEBUFFER_CONSOLE=y'), "Defconfig must enable CONFIG_FRAMEBUFFER_CONSOLE=y");
        assert(defconfig.includes('CONFIG_FB_CFB_FILLRECT=y'), "Defconfig must enable CONFIG_FB_CFB_FILLRECT=y");
        assert(defconfig.includes('CONFIG_VT=y'), "Defconfig must enable CONFIG_VT=y");
        assert(defconfig.includes('CONFIG_FONT_8x16=y'), "Defconfig must enable CONFIG_FONT_8x16=y");
    });

    console.log("\n===============================================================================");
    console.log(`⚡ ADVERSARIAL CHALLENGE COMPLETED SUCCESSFULLY`);
    console.log(`Total empirical assertions: ${totalTests}`);
    console.log(`Passed: ${passedTests}`);
    console.log(`Failed: ${failedTests}`);
    console.log("===============================================================================\n");

    if (failedTests > 0) {
        process.exit(1);
    }
}

runAdversarialSuite().catch(err => {
    console.error("Adversarial suite failure:", err);
    process.exit(1);
});
