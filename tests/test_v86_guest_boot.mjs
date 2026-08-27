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
import zlib from 'zlib';
import { fileURLToPath } from 'url';
import { V86GuestManager, VM_STATES, BOOT_MILESTONES, verifyBzImage } from '../src/v86_guest_manager.js';
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

function parseCpioNewc(buffer) {
    const files = new Map();
    let offset = 0;
    while (offset + 110 <= buffer.length) {
        const magic = buffer.toString('ascii', offset, offset + 6);
        if (magic !== '070701') break;
        const fileSize = parseInt(buffer.toString('ascii', offset + 54, offset + 62), 16);
        const nameSize = parseInt(buffer.toString('ascii', offset + 94, offset + 102), 16);
        const nameOffset = offset + 110;
        const name = buffer.toString('ascii', nameOffset, nameOffset + nameSize - 1);
        if (name === 'TRAILER!!!') break;

        let headerAndNameLen = 110 + nameSize;
        if (headerAndNameLen % 4 !== 0) {
            headerAndNameLen += 4 - (headerAndNameLen % 4);
        }
        const dataOffset = offset + headerAndNameLen;
        const fileData = buffer.subarray(dataOffset, dataOffset + fileSize);
        files.set(name.replace(/^\.\//, ''), fileData);

        let totalLen = headerAndNameLen + fileSize;
        if (totalLen % 4 !== 0) {
            totalLen += 4 - (totalLen % 4);
        }
        offset += totalLen;
    }
    return files;
}

function parseZipEntries(zipBuf) {
    const entries = new Map();
    let offset = 0;
    while (offset + 30 <= zipBuf.length) {
        const sig = zipBuf.readUInt32LE(offset);
        if (sig !== 0x04034B50) break;
        const compressedSize = zipBuf.readUInt32LE(offset + 18);
        const nameLen = zipBuf.readUInt16LE(offset + 26);
        const extraLen = zipBuf.readUInt16LE(offset + 28);
        const name = zipBuf.toString('utf8', offset + 30, offset + 30 + nameLen);
        const dataStart = offset + 30 + nameLen + extraLen;
        const data = zipBuf.subarray(dataStart, dataStart + compressedSize);
        entries.set(name, { offset: dataStart, data, size: compressedSize });
        offset = dataStart + compressedSize;
    }
    return entries;
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
        assert(!initContent.includes('media_host_rs'), "Init script must NOT reference media_host_rs");

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

        // 1.5 Linux x86 bzImage binary header inspection
        const bzImagePath = path.join(rootDir, 'guest/build/bzImage');
        assert(fs.existsSync(bzImagePath), "guest/build/bzImage must exist");
        const bzImageBuf = fs.readFileSync(bzImagePath);
        assert(bzImageBuf.length >= 0x208, "bzImage must be at least 520 bytes");
        const bootSig = bzImageBuf.readUInt16LE(0x1FE);
        assert(bootSig === 0xAA55, `bzImage boot sector signature at 0x1FE must be 0xAA55, got 0x${bootSig.toString(16)}`);
        const hdrMagic = bzImageBuf.toString('ascii', 0x202, 0x206);
        assert(hdrMagic === 'HdrS', `bzImage setup header magic at 0x202 must be 'HdrS', got '${hdrMagic}'`);
        const bootProtocol = bzImageBuf.readUInt16LE(0x206);
        assert(bootProtocol >= 0x0200, `bzImage boot protocol at 0x206 must be >= 0x0200, got 0x${bootProtocol.toString(16)}`);

        // 1.6 Initrd archive & ART Boot classpath verification
        const initrdPath = path.join(rootDir, 'guest/build/initrd.img');
        assert(fs.existsSync(initrdPath), "guest/build/initrd.img must exist");
        const initrdGz = fs.readFileSync(initrdPath);
        const rawCpio = zlib.gunzipSync(initrdGz);
        const cpioEntries = parseCpioNewc(rawCpio);

        assert(cpioEntries.has('system/framework/boot.art'), "initrd.img must contain system/framework/boot.art");
        const bootArt = cpioEntries.get('system/framework/boot.art');
        assert(bootArt.length >= 0x40, "boot.art must have at least 64 bytes header");
        const artMagic = bootArt.toString('ascii', 0, 8);
        assert(artMagic === 'art\n018\0', `boot.art magic must be 'art\\n018\\0', got ${JSON.stringify(artMagic)}`);
        assert(bootArt.readUInt32LE(0x08) === 0x70000000, "boot.art image_begin_ must be 0x70000000");
        assert(bootArt.readUInt32LE(0x24) === 4, "boot.art pointer_size_ must be 4 (32-bit x86)");
        assert(bootArt.readUInt32LE(0x20) === 0x70001000, "boot.art image_roots_ must be 0x70001000");

        assert(cpioEntries.has('system/framework/framework.jar'), "initrd.img must contain system/framework/framework.jar");
        const frameworkJar = cpioEntries.get('system/framework/framework.jar');
        assert(frameworkJar.length >= 30, "framework.jar must have at least ZIP header");
        assert(frameworkJar.readUInt32LE(0) === 0x04034B50, "framework.jar must be a valid ZIP archive (magic PK\\x03\\x04)");

        const dexOffset = 30 + 11; // 30 bytes local header + 11 bytes 'classes.dex'
        const dexMagic = frameworkJar.toString('ascii', dexOffset, dexOffset + 8);
        assert(dexMagic === 'dex\n035\0', `classes.dex magic must be 'dex\\n035\\0', got ${JSON.stringify(dexMagic)}`);

        const jarEntries = parseZipEntries(frameworkJar);
        assert(jarEntries.has('META-INF/MANIFEST.MF'), "framework.jar must contain META-INF/MANIFEST.MF entry");
        assert(jarEntries.has('classes.dex'), "framework.jar must contain classes.dex entry");

        // Validate classes.dex inside framework.jar
        const dexEntry = jarEntries.get('classes.dex');
        assert(dexEntry && dexEntry.data.length >= 0x70, "classes.dex must have valid DEX binary payload");
        assert(dexEntry.data.includes(Buffer.from('Landroid/app/Activity;')), "classes.dex must contain descriptor Landroid/app/Activity;");
        assert(dexEntry.data.includes(Buffer.from('Landroid/os/ServiceManager;')), "classes.dex must contain descriptor Landroid/os/ServiceManager;");
        assert(dexEntry.data.includes(Buffer.from('Landroid/view/SurfaceControl;')), "classes.dex must contain descriptor Landroid/view/SurfaceControl;");
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
        assert(manager.getState() === VM_STATES.BOOTING, "After start without simulated lines, state must reach BOOTING");
        
        // Feed serial to transition to RUNNING
        manager.feedSerial("SeaBIOS (version 1.14.0)\nLinux version 5.10.0-android-x86\nZygote: listening on socket /dev/socket/zygote\n");
        assert(manager.getState() === VM_STATES.RUNNING, "After serial feed, state reaches RUNNING");
        assert(stateTransitions.length >= 3, "State transitions must occur during boot and serial progression");

        // 2.4 Pause & Resume
        manager.pause();
        assert(manager.getState() === VM_STATES.PAUSED, "State must transition to PAUSED");

        manager.resume();
        assert(manager.getState() === VM_STATES.RUNNING, "State must transition back to RUNNING");

        // 2.5 Destroy & Reset
        manager.destroy();
        assert(manager.getState() === VM_STATES.UNINITIALIZED, "State must reset to UNINITIALIZED");

        // 2.6 V86GuestManager verifyBzImage() method tests
        assert(typeof manager.verifyBzImage === 'function', "V86GuestManager must implement verifyBzImage(buffer)");
        assert(typeof verifyBzImage === 'function', "v86_guest_manager must export standalone verifyBzImage(buffer)");
        const bzImageBuf = fs.readFileSync(path.join(rootDir, 'guest/build/bzImage'));
        assert(manager.verifyBzImage(bzImageBuf) === true, "manager.verifyBzImage must return boolean true for valid bzImage binary");
        
        const standaloneRes = verifyBzImage(bzImageBuf);
        assert(typeof standaloneRes === 'object' && standaloneRes !== null, "standalone verifyBzImage must return structured object");
        assert(standaloneRes.valid === true, "standalone verifyBzImage valid property must be true");
        assert(standaloneRes.bootFlag === 0xAA55, `standalone verifyBzImage bootFlag must be 0xAA55, got 0x${standaloneRes.bootFlag ? standaloneRes.bootFlag.toString(16) : ''}`);
        assert(standaloneRes.headerMagic === 'HdrS', `standalone verifyBzImage headerMagic must be 'HdrS', got '${standaloneRes.headerMagic}'`);
        assert(typeof standaloneRes.protocol === 'number' && standaloneRes.protocol >= 0x0200, "standalone verifyBzImage protocol version must be >= 0x0200");

        // Corrupted boot signature at 0x1FE
        const badSigBuf = Buffer.from(bzImageBuf);
        badSigBuf.writeUInt16LE(0x0000, 0x1FE);
        assert(manager.verifyBzImage(badSigBuf) === false, "verifyBzImage must reject invalid boot sector signature");
        const badSigRes = verifyBzImage(badSigBuf);
        assert(badSigRes.valid === false && typeof badSigRes.error === 'string', "standalone verifyBzImage must return valid=false with error for bad boot sig");

        // Corrupted magic at 0x202
        const badMagicBuf = Buffer.from(bzImageBuf);
        badMagicBuf.write('BAD!', 0x202, 'ascii');
        assert(manager.verifyBzImage(badMagicBuf) === false, "verifyBzImage must reject invalid setup header magic");
        const badMagicRes = verifyBzImage(badMagicBuf);
        assert(badMagicRes.valid === false && typeof badMagicRes.error === 'string', "standalone verifyBzImage must return valid=false with error for bad magic");

        // Corrupted boot protocol at 0x206
        const badProtoBuf = Buffer.from(bzImageBuf);
        badProtoBuf.writeUInt16LE(0x0100, 0x206);
        assert(manager.verifyBzImage(badProtoBuf) === false, "verifyBzImage must reject boot protocol < 0x0200");
        const badProtoRes = verifyBzImage(badProtoBuf);
        assert(badProtoRes.valid === false && typeof badProtoRes.error === 'string', "standalone verifyBzImage must return valid=false with error for bad protocol");

        // Null and truncated buffer handling
        assert(manager.verifyBzImage(null) === false, "verifyBzImage must reject null");
        assert(verifyBzImage(null).valid === false, "standalone verifyBzImage must return valid=false for null");
        assert(manager.verifyBzImage(new Uint8Array(100)) === false, "verifyBzImage must reject truncated buffer");
        assert(verifyBzImage(new Uint8Array(100)).valid === false, "standalone verifyBzImage must return valid=false for truncated buffer");

        // 2.7 V86GuestManager initWebGpuDevice() method tests with TIMESTAMP_QUERY
        assert(typeof manager.initWebGpuDevice === 'function', "V86GuestManager must implement initWebGpuDevice()");

        let requestedFeatures = null;
        const mockAdapterWithTimestamp = {
            features: new Set(['timestamp-query']),
            requestDevice: async (desc) => {
                requestedFeatures = desc.requiredFeatures;
                return { label: 'mock-gpu-device-ts', features: new Set(desc.requiredFeatures) };
            }
        };

        const deviceWithTs = await manager.initWebGpuDevice(mockAdapterWithTimestamp);
        assert(deviceWithTs !== null, "initWebGpuDevice must return device");
        assert(Array.isArray(requestedFeatures) && requestedFeatures.includes('timestamp-query'), "initWebGpuDevice must enable 'timestamp-query' when supported");
        assert(manager.gpuFeatures.includes('timestamp-query'), "manager.gpuFeatures must record timestamp-query");

        const mockAdapterWithoutTimestamp = {
            features: new Set([]),
            requestDevice: async (desc) => {
                requestedFeatures = desc.requiredFeatures;
                return { label: 'mock-gpu-device-no-ts', features: new Set(desc.requiredFeatures) };
            }
        };

        const deviceWithoutTs = await manager.initWebGpuDevice(mockAdapterWithoutTimestamp);
        assert(deviceWithoutTs !== null, "initWebGpuDevice must return device without timestamp-query");
        assert(Array.isArray(requestedFeatures) && requestedFeatures.length === 0, "requiredFeatures must be empty when timestamp-query unsupported");
        assert(manager.gpuFeatures.length === 0, "manager.gpuFeatures must be empty");

        const nullDevice = await manager.initWebGpuDevice(null);
        assert(nullDevice === null, "initWebGpuDevice with null adapter returns null gracefully");

        // 2.8 Default configuration values check
        const defaultMgr = new V86GuestManager();
        assert(defaultMgr.config.wasmPath === './v86/v86.wasm', "Default wasmPath must be './v86/v86.wasm'");
        assert(defaultMgr.config.cdromUrl === null, "Default cdromUrl must be null");
        assert(defaultMgr.config.bootMode === 'direct' || defaultMgr.config.bootMode === 'kernel', "Default bootMode must be 'direct' or 'kernel'");
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

        // Test alternate virtio-gpu patterns
        const altMgr1 = new V86GuestManager();
        altMgr1.setState(VM_STATES.BOOTING);
        altMgr1.feedSerial("virtio-gpu 0000:00:02.0: vgaarb: deactivate vga console\n");
        assert(altMgr1.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), "Milestone VIRTIO_GPU_INIT on 'virtio-gpu'");

        const altMgr2 = new V86GuestManager();
        altMgr2.setState(VM_STATES.BOOTING);
        altMgr2.feedSerial("[drm: virtio-gpu] modeset initialized\n");
        assert(altMgr2.hasMilestone(BOOT_MILESTONES.VIRTIO_GPU_INIT), "Milestone VIRTIO_GPU_INIT on 'drm: virtio-gpu'");

        manager.feedSerial("Android Binder IPC Driver initialized (protocol version 8)\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.BINDERFS_MOUNT), "Milestone BINDERFS_MOUNT must be recorded");
        assert(manager.getState() === VM_STATES.BINDER_READY, "State must transition to BINDER_READY");

        // Test alternate init milestone patterns
        const initMgr1 = new V86GuestManager();
        initMgr1.setState(VM_STATES.BOOTING);
        initMgr1.feedSerial("Freeing unused kernel memory: 1024K\n");
        assert(initMgr1.hasMilestone(BOOT_MILESTONES.INIT_USERSPACE), "Milestone INIT_USERSPACE on 'Freeing unused kernel memory'");

        const initMgr2 = new V86GuestManager();
        initMgr2.setState(VM_STATES.BOOTING);
        initMgr2.feedSerial("init: init first stage started\n");
        assert(initMgr2.hasMilestone(BOOT_MILESTONES.INIT_USERSPACE), "Milestone INIT_USERSPACE on 'init:'");

        manager.feedSerial("[init] servicemanager started (handle 0 context manager)\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Milestone SERVICEMANAGER_READY must be recorded");
        assert(manager.getState() === VM_STATES.SERVICES_READY, "State must transition to SERVICES_READY");

        // Test alternate servicemanager milestone patterns
        const smMgr1 = new V86GuestManager();
        smMgr1.setState(VM_STATES.BINDER_READY);
        smMgr1.feedSerial("servicemanager: ready for binder transactions\n");
        assert(smMgr1.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Milestone SERVICEMANAGER_READY on 'servicemanager: ready'");
        assert(smMgr1.getState() === VM_STATES.SERVICES_READY, "State transitions to SERVICES_READY on 'servicemanager: ready'");

        const smMgr2 = new V86GuestManager();
        smMgr2.setState(VM_STATES.BINDER_READY);
        smMgr2.feedSerial("servicemanager: root context manager registered\n");
        assert(smMgr2.hasMilestone(BOOT_MILESTONES.SERVICEMANAGER_READY), "Milestone SERVICEMANAGER_READY on 'servicemanager: root context manager'");
        assert(smMgr2.getState() === VM_STATES.SERVICES_READY, "State transitions to SERVICES_READY on 'servicemanager: root context manager'");

        manager.feedSerial("Zygote: listening on socket /dev/socket/zygote\n");
        assert(manager.hasMilestone(BOOT_MILESTONES.ZYGOTE_ART_READY), "Milestone ZYGOTE_ART_READY must be recorded");
        assert(manager.getState() === VM_STATES.RUNNING, "State must transition to RUNNING");

        // 3.2 Adversarial Panic / SIGILL Error Detector Tests
        const panicManager = new V86GuestManager();
        await panicManager.start();
        assert(panicManager.getState() === VM_STATES.BOOTING, "Panic manager started in BOOTING");

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
        manager.setState(VM_STATES.RUNNING);

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
        realManager.feedSerial(
            "SeaBIOS (version 1.14.0)\n" +
            "Linux version 5.10.0-android-x86\n" +
            "Android Binder IPC Driver initialized (protocol version 8)\n" +
            "[init] servicemanager started (handle 0 context manager)\n" +
            "Zygote: listening on socket /dev/socket/zygote\n"
        );

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
