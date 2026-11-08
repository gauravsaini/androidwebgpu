# Findings — Track T706: Clean GSI system.img Patched on Linux via Loopback Mount + Resolve HWC Hotplug Abort, Fly for First Queued Buffer + Present Dispatch (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track706-clean-gsi-hwc-hotplug`  
Branch: `t706`, from `d796c8d` (the T705 commit)  
Predecessor: T705 driver-certified honest FAIL (commit d796c8d: virtio-blk kernel rebuild and external mount succeeded, but external image afc0add0 carried inherited services.jar corruption causing Zygote SIGSEGV in ArtField::GetDeclaringClass, and SurfaceFlinger aborted on HWC hotplug failure).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 12 GiB available; Step 1 verified all 10 staged inputs and proved clean base system.img block 145414 reads original fill 4ea21150…; Step 2 Linux loop patch applied on box, block 145414 unshare proof verified 4ea21150… untouched, patched APK 8a28e1db… verified with exact opcodes, oat/arm64 absent, services.jar byte-identical d4108052…, e2fsck clean exit 0, and critical open item resolved reconciling post-hash e2fsck superblock timestamp variance to authoritative SHA 2de0533e…; Step 3a smoke flight proved external virtio-blk /dev/block/vda mounts cleanly as /system at guest 7.05s and Zygote SIGSEGV eliminated [0]; Step 3b Flight-01 reached full 2400 s window [guest 2406.817 s, host 2407.74 s, exit 0, 53,664 lines, log SHA 3e8121d0…]; Step 3c Flight-ab canonical control proved canonical kernel 05c43917… times out polling vda and aborts at guest 17.75s; however, vendor.hwc-2-1 never issued a display hotplug callback to SurfaceFlinger, causing SurfaceFlinger to abort 31 times with 'Initial display configuration failed: HWC did not hotplug' [tombstone_00 at guest 102.68s through tombstone_30 at 2350.22s], which repeatedly triggered init onrestart killing Zygote before forkSystemServer(); system_server did not fork; queueBuffer 0, present 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, Zygote SIGSEGV in `art::ArtField::GetDeclaringClass` was completely eliminated (0 occurrences across 53,664 lines), proving the clean base system.img cured the inherited services.jar corruption. However, `surfaceflinger` (PID 2440, 2493, 2538, 2563... 14423) aborted 31 times with signal 6 (`Abort message: 'Initial display configuration failed: HWC did not hotplug'`) because `vendor.hwc-2-1` never sent a display hotplug notification to SurfaceFlinger. Each SurfaceFlinger death triggered `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`, sending SIGKILL (signal 9) to Zygote before `forkSystemServer()` could be reached. As a result, `forkSystemServer()` did not complete and `sys.boot_completed` remained unset. In control flight `flight-ab`, `boot_completed=` was empty throughout its execution until QEMU clean exit on mount abort at guest 17.76 s.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace display HAL interface, HWC hotplug callback contract, SurfaceFlinger display configuration requirement, and init restart cascading topology are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.473528 s (line 188: `[    3.473528] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.521890 s (line 186: `[    3.521890] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.470378 s (line 180: `[    3.470378] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 7.021142 s (line 515: `[    7.021142] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 7.218450 s (line 517: `[    7.218450] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 7.051126 s (line 512: `[    7.051126] init: init first stage started!`).

system_server:
- `system_server` did not fork in Flight-01 (0). SurfaceFlinger's repeated aborts triggered init to send SIGKILL to Zygote before `forkSystemServer()` could be invoked.
- `system_server` did not fork in Flight-ab (0). Init failed to mount `/system` from `/dev/block/vda` and triggered `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- Composer `executeCommands`: **0**
- `queueBuffer`: **0** (SurfaceFlinger held 0 queued client buffers).

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block devices, but could not detect `/dev/vda` due to lack of `CONFIG_VIRTIO_BLK` in the canonical kernel build. At guest 17.750656 s (line 551), Android init logged: `init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`, followed by line 552 `init: Failed to mount required partitions early ...` and line 553 `init: InitFatalReboot: signal 6`. QEMU cleanly rebooted and terminated under `-no-reboot` at guest 17.757 s (host 20.02 s, exit code 0, 559 lines, log SHA-256 `27163c4573138379c9ce05b9b6574f85e49fef56f2e2be2fe901c0f135b6c2d1`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` never forked and SurfaceFlinger remained trapped in the HWC hotplug abort loop.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T706 attacked the two open defects that remained after T705: (1) the inherited services.jar / block 145414 corruption from macOS Homebrew debugfs in T702, and (2) the HWC hotplug failure. 

### Key Accomplishments
1. **Critical Open Item Resolved (Authoritative SHA Reconciliation)**:
   - Driver identified a pre-flight SHA divergence on Mac `guest-inputs/system.img`: `d9455e0c…` vs Step 2 claimed `2de0533e…`.
   - Investigation proved the Box image `/mnt/sdb1/kernel-critical/lane-images/t706-system.img` hashed to `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601`.
   - A chunk-by-chunk binary diff across all 1,965 MBs proved that chunks 1..1964 were 100% bit-for-bit identical. Exactly 5 bytes in MB 0 differed: ext4 superblock timestamps (`s_wtime` at offset 1072, `s_lastcheck` at offset 1088, `s_checksum` at offset 1400), mutated when `scripts/verify_patch_proof_t706.py` executed `e2fsck -f -y` on Mac.
   - Restored MB 0 to the authoritative Box data, updated the verification harness to use `e2fsck -f -n` (read-only verification, exit 0), and re-proved all Step 2 gates against authoritative file `2de0533e…`.
2. **Inherited services.jar Corruption Completely Cured (0 ArtField SIGSEGV)**:
   - Clean base system.img `98fc0b02…` patched on Linux via extent-safe block allocation.
   - Block 145414 fill verified `4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea` (100% untouched).
   - Patched `Launcher3QuickStep.apk` verified in image: SHA `8a28e1db…`, exact opcode patches verified (`0x199a10=00000000`, `0x199a90=1200`, `0x19b112=0102000000000000`).
   - `oat/arm64` AOT directory confirmed absent (de-shadowed).
   - `services.jar` confirmed byte-identical to original clean base (`d4108052…`).
   - In Flight-01 (2406.8 s guest, 53,664 lines), `art::ArtField::GetDeclaringClass` SIGSEGV dropped from 7 occurrences in T705 to **0 occurrences** in T706. The services.jar corruption wall is closed.
3. **Smoke Flight & External VirtIO-Blk Mount (PASS)**:
   - 120 s smoke flight verified external virtio-blk `/dev/block/vda` mounted cleanly as `/system` at guest line 524 (`__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`).
   - Freeing kernel memory, init first stage, fb0 simplefb registered. Zero kernel panic/Oops.
4. **Full Flight-01 & Canonical A/B Control Flown (PASS Execution)**:
   - Flight-01 ran for 2407.74 host seconds (guest 2406.817 s, exit 0, 53,664 lines, SHA `3e8121d0…`).
   - Flight-ab ran on canonical kernel `05c43917…` and confirmed missing `CONFIG_VIRTIO_BLK=y` triggers vda polling timeout and `InitFatalReboot` at guest 17.75 s.
5. **Exact First Failing Link Isolated (The HWC Hotplug Wall)**:
   - SurfaceFlinger aborted 31 times during Flight-01 with: `Abort message: 'Initial display configuration failed: HWC did not hotplug'`.
   - Each abort triggered init's onrestart handler: `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`, killing Zygote before it could fork `system_server`.
   - The HWC hotplug failure is proven to be an independent display HAL wall: `vendor.hwc-2-1` (the T681 Composer 2.1 service) fails to issue the required `onHotplug(displayId, Connection::CONNECTED)` event to SurfaceFlinger.

---

## Input Verification Table

All inputs verified in `/Users/Shared/wt-track706-clean-gsi-hwc-hotplug` prior to execution without network fetch:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t705.md` | `d99361c77f0a6d07d1a588b3ae6820572e90f6b4d580fe5cfa49f395d9e5a6ef` | `d99361c77f0a6d07d1a588b3ae6820572e90f6b4d580fe5cfa49f395d9e5a6ef` | 17,345 B | MATCH (PASS) |
| `findings-t704.md` | `c572f0dfe9f0dd02e164952d4402ddb27cf20a4c1fb7a4b24dd5fec6f3fb6d34` | `c572f0dfe9f0dd02e164952d4402ddb27cf20a4c1fb7a4b24dd5fec6f3fb6d34` | 19,065 B | MATCH (PASS) |
| `findings-t703.md` | `6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583` | `6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583` | 22,137 B | MATCH (PASS) |
| `findings-t702.md` | `708128c05763266ca5bf5244585c5443a6d4822fbe6520e7e1f4095bb49bb8e5` | `708128c05763266ca5bf5244585c5443a6d4822fbe6520e7e1f4095bb49bb8e5` | 21,909 B | MATCH (PASS) |
| `t705-Image` | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` | 20,126,208 B | MATCH (PASS) |
| `t705-systemserver-fb0-fix.cpio.gz` | `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965` | `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965` | 53,169,644 B | MATCH (PASS) |
| `t705-modified.dtb` | `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01` | `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01` | 2,049 B | MATCH (PASS) |
| `clean-base-system.img` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B | MATCH (PASS) |
| `composer` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| `extracted_kernel.config` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | 133,733 B | MATCH (PASS) |

---

## Step 2 Pre-Flight Proofs Table (Authoritative Clean system.img)

| Proof # | Verification Item | Target Specification | Observed Value | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| **Proof 1** | Patched system.img SHA-256 | Authoritative Box & Mac SHA | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` (2,059,816,960 B) | PASS |
| **Proof 2** | Block 145414 Unshare Proof | Read-only dd of block 145414 | `4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea` (100% UNTOUCHED fill) | PASS |
| **Proof 3** | Patched APK in Image | `system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk` | SHA-256 `8a28e1dbb416f1f6a3279082279dd9441db53cad10353892f8a32c08a85cd97c` (25,654,202 B) | PASS |
| **Proof 4** | DEX Opcode Verification | classes7.dex opcodes at 0x199a10, 0x199a90, 0x19b112 | `0x199a10=00000000`, `0x199a90=1200`, `0x19b112=0102000000000000` | PASS |
| **Proof 5** | oat/arm64 De-shadow Absence | `system/system_ext/priv-app/Launcher3QuickStep/oat` | Directory completely absent (de-shadowed) | PASS |
| **Proof 6** | services.jar Byte-Identity | `system/framework/services.jar` vs clean base | SHA-256 `d41080524ff81b8693a453f4d5cfe8a119be433f3e375a0780683acc71f20d6e` (100% identical) | PASS |
| **Proof 7** | e2fsck Clean Check | `e2fsck -f -n` on patched system.img | Pass 1-5 clean, 7342 files, 477627 blocks, returncode 0 | PASS |

---

## Flights Execution Tables

### Step 3a: Smoke Flight (120 s Target)
- Command: QEMU 11.1.2 with `t705-Image` (`53d9563e…`), `t705-modified.dtb`, `t705-systemserver-fb0-fix.cpio.gz`, `system.img` (`2de0533e…`).
- Host elapsed: 125.27 s | Guest log max: 123.637 s | QEMU exit code: 0
- Log metrics: 4,117 lines | SHA-256 `e43aa379009f63cc7ca6560b6d2543dc5b18853e181b38d49373469a85212791`
- External mount: line 524 `[    7.448726] init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
- Zygote SIGSEGV count: **0**

### Step 3c: Canonical A/B Control (`flight-ab`) (240 s Target)
- Command: QEMU 11.1.2 with `canonical-Image` (`05c43917…`), `t705-modified.dtb`, `t705-systemserver-fb0-fix.cpio.gz`, `system.img`.
- Host elapsed: 20.02 s | Guest log max: 17.757 s | QEMU exit code: 0
- Log metrics: 559 lines | 47,006 bytes | SHA-256 `27163c4573138379c9ce05b9b6574f85e49fef56f2e2be2fe901c0f135b6c2d1`
- Polling timeout: line 551 `[   17.750656] init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`
- Fatal reboot: line 553 `[   17.775180] init: InitFatalReboot: signal 6`
- Clean exit: line 559 `[   17.863788] reboot: Restarting system with command 'bootloader'`

### Step 3b: Flight-01 (Route B Kernel, 2400 s Target)
- Command: QEMU 11.1.2 with `t705-Image` (`53d9563e…`), `t705-modified.dtb`, `t705-systemserver-fb0-fix.cpio.gz`, `system.img` (`2de0533e…`).
- Host elapsed: 2407.74 s | Guest log max: 2406.817 s | QEMU exit code: 0
- Log metrics: 53,664 lines | 5,735,995 bytes | SHA-256 `3e8121d08c8e1f2176eb3d1e5274137f7019220baefb264c2730abc0367ca1d0`
- External mount: line 524 `[    7.411158] init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
- Zygote SIGSEGV in `art::ArtField::GetDeclaringClass`: **0** (completely eliminated!)
- SurfaceFlinger aborts: 31 tombstones, all with `Abort message: 'Initial display configuration failed: HWC did not hotplug'`
- Binder ioctl `40046210` (-22 EINVAL): 2,929 occurrences

---

## First Failing Link & Exact Next Fix

### First Failing Link
The first failing link is: **SurfaceFlinger aborts on primary display hotplug absence (`Initial display configuration failed: HWC did not hotplug`), cascading into Zygote process-group termination via init onrestart**.

Evidence:
1. `flights/flight-01/uart.log` line 3660 (guest 102.68 s):
   `Abort message: 'Initial display configuration failed: HWC did not hotplug'`
   `tombstone_00 pid: 2440, tid: 2440, name: surfaceflinger`
2. Followed immediately by line 3668:
   `init: Command 'restart --only-if-running zygote' action=onrestart (<Service 'surfaceflinger' onrestart>:1) took 271ms and succeeded`
   `init: Sending signal 9 to service 'zygote' (pid 2434) process group...`
3. This exact sequence repeated 31 times (tombstone_00 through tombstone_30).
4. `vendor.hwc-2-1` (the T681 Composer 2.1 service, PID 2371) received `-22` (`EINVAL`) on binder ioctl `40046210` (`BINDER_ENABLE_ONEWAY_SPAM_DETECTION`), but never sent the required `IComposerCallback::onHotplug(displayId, Connection::CONNECTED)` event to SurfaceFlinger upon client callback registration. Because SurfaceFlinger in Android 14 mandates a primary display hotplug during initialization, it aborts by design when HWC fails to hotplug a display.

### Exact Next Fix (Track T707)
1. **Instrument and Fix Composer 2.1 Service (`android.hardware.graphics.composer@2.1-service`)**:
   - In `ComposerClientImpl::registerCallback(const sp<IComposerCallback>& callback)`, immediately invoke:
     `callback->onHotplug(PRIMARY_DISPLAY_ID, IComposerCallback::Connection::CONNECTED);`
   - Rebuild the Composer 2.1 service ELF with this hotplug dispatch on the Linux box, verify the binary, and package it into the vendor partition / initramfs.
2. **Alternative Init Decoupling / Fallback Configuration**:
   - Alternatively, decouple SurfaceFlinger failure from Zygote termination in `init.rc` (`surfaceflinger.rc` onrestart action) or configure SurfaceFlinger virtual display fallback so that Zygote is permitted to fork `system_server`.
   - Once SurfaceFlinger receives the display hotplug, `system_server` will fork and initialize `DisplayManagerService`, unblocking `queueBuffer`, `presentDisplay`, and Launcher3 execution.
