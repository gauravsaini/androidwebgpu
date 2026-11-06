Verdict: FAIL

# Track T684 — Composer 2.1 present-path diagnosis

**First failing links:** (1) SurfaceFlinger-to-Composer command dispatch: the guest evidence shows a reachable HIDL Composer 2.1 service, but no `executeCommands`, present call, or frame commit. (2) First-cycle startup: Zygote never produced an observed `system_server` PID by guest uptime 1175.91s. The display path therefore was not exercised end to end. The supplied DTB and guest `/dev/fb0` evidence show no real framebuffer target.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface is required regardless of implementation language.
sys.boot_completed: not observed (referenced T683 cycle 1; UART line 49952 at guest uptime 1175.91s has an empty `boot_completed=` field. T684 ran no new cycle).

## Step 0 — Mac disk gate

PASS. `df -h /System/Volumes/Data` reported 16 GiB available (228 GiB total, 182 GiB used, 92% capacity), above the 8 GiB minimum. No build, copy, or download was made for a disk gate.

## Step 1 — Staged input verification

PASS. All six staged inputs matched the exact SHA-256 values in the brief. `stat` sizes also matched the brief where specified. Streaming `gzip -dc guest-inputs/base-t683-fix.cpio.gz | cpio -itv` listed `ramdisk-images/vendor.img` as a 134,217,728-byte CPIO member; the listing had 244 entries. The base archive was not unpacked.

| Input | Observed SHA-256 | Observed size |
|---|---|---:|
| `guest-inputs/Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B |
| `guest-inputs/minimal-virt-fixed.dtb` | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` | 1,465 B |
| `guest-inputs/system.img` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B |
| `guest-inputs/base-t683-fix.cpio.gz` | `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70` | 958,780,579 B |
| `guest-inputs/vendor-t683.img` | `bed3d6e989e16c8bbd7dd7e0208753c3a0d8e3407c96d291cd778ff43a7daaf1` | 134,217,728 B |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B |

The read-only T683 UART also matched its pinned SHA-256 `73cfd8d61e6c992b913e3c83975a314ea0f39cd3d41c505464d70aa55b5dbe70` and 50,043 lines. The referenced `findings-t683.md` SHA-256 was `fd06523c4f37e68b291afd729aa4e62934cef9d974f752cdb3532855da0e26b0`.

## Step 2 — Offline diagnosis on the T683 UART

Evidence source: `/Users/Shared/wt-track683-composer-2-1-flight-recovery/artifacts-t683/flight-t670/uart.log` (50,043 lines; line numbers below are from this file).

1. **IComposer registration and SurfaceFlinger visibility.** At UART line 42724, the guest says: `servicemanager: Could not find android.hardware.graphics.composer3.IComposer/default in the VINTF manifest.` That is the Composer 3 AIDL lookup. At line 46290, SurfaceFlinger says: `Using HWComposer service: default`. The name `default` is the instance name, not a Composer 3 service or a software fallback. In the same startup window the live service logs as `android.hardware.graphics.composer@2.1-service` with PID 20236 (lines 46319 onward). Its inbound transaction is `BR_TRANSACTION ... code='\x0fCHN'` (line 46330); the parcel identifies `android.hidl.base@1.0::IBase` (lines 46343–46346), and its returned interface chain contains `android.hardware.graphics.composer@2.1::IComposer` (hex/ascii dump at lines 46427–46430). Taken together, SurfaceFlinger selected instance `default` and the HIDL 2.1 service was reachable and advertised the expected interface. The UART does not print a standalone HIDL “registered” line, so registration is inferred from this service-manager/interfaceChain handshake and SurfaceFlinger's selection. The Composer 3 VINTF miss is not evidence that the Composer 2.1 HIDL path was blocked; the 2.1 path was reached.

2. **Composer dispatch.** The only decoded inbound service transaction is the `IBase::interfaceChain` request above, not a display command. The service logs `Unknown command: 136` at line 46390, then `Finished read/write, write size = 76` at line 46394. It decodes the returned binder protocol as `BR_NOOP` and `BR_TRANSACTION_COMPLETE` at lines 46404–46405. These are generic binder driver/protocol diagnostics; `BR_TRANSACTION_COMPLETE` acknowledges a binder transaction and is not a frame commit. A full-log search found no `executeCommands`, `presentOrValidate`, `presentDisplay`, or `setClientTarget` call/success line. No guest-origin successful present or fb0 commit is present. Thus no guest call is evidenced as carrying an execute/present operation to this service.

3. **Why `system_server` did not appear.** Init reports `start zygote ... took 1465ms and succeeded` at line 40891. Zygote PID 20189 is still doing framework work near the end of the window: line 49947 logs a method verification taking 966.698ms; the final probe at lines 49951–49952 reports `uptime=1175.91 system_server_prop= system_server_pids= zygote=running zypid=20189 ... boot_completed= ce_available= ... gatekeeperd=2525 gnss=2524 fingerprint=2526`. Therefore the observed state is that Zygote was running but `system_server` had not been forked by the last probe. The full UART search found no `forkSystemServer`, `SystemServer`, `FATAL EXCEPTION`, or `Fatal signal` trace; it does not show an early `system_server` crash. The evidence pins the first failing startup edge to Zygote not reaching an observed system-server fork during the flight window; it does not prove a deeper cause for the prolonged Zygote work.

   APEX activation itself passed: line 1669 says `init: Wait for property 'apexd.status=activated' took 488ms`. The T642 probe found Wi-Fi and tethering APEX state `ok` at lines 49951–49952; the earlier diagnostics mark connectivity JAR OK at line 3536. There is a separate `T647_APEX_CONFIGINFRASTRUCTURE_JAR_MISSING` marker at line 3533. The probe script checks the specific `/apex/com.android.configinfrastructure/javalib/framework-mediaprovider.jar` path; this is not an APEX activation failure and no fatal trace ties it to the absent fork. Gatekeeper, GNSS, and fingerprint were all alive at the final probe (PIDs 2525, 2524, and 2526).

   Keystore2's reports are maintenance-call latency diagnostics, not Android Watchdog kills: line 5113 reports `IKeystoreMaintenance::earlyBootEnded Pending: 502.002ms Overdue 2.048992ms`; line 5200 says the trusted-environment call succeeded. There is no system-process FATAL or Android Watchdog kill in the UART. With `system_server` absent, user DE/CE unavailable, and `sys.boot_completed` blank, the system-server-driven boot/display path was not exercised end to end.

4. **`/dev/fb0` and real framebuffer path.** UART line 1140 shows init creating `/dev/fb0` with `/system/bin/toybox truncate -s 16M /dev/fb0`; line 1145 ties that command to `/vendor/etc/init/hw/init.virt.rc:4`. Line 1158 records the toybox denial with `path="/dev/fb0" dev="tmpfs"`, and line 41186 records the allocator opening `/dev/fb0` with `dev="tmpfs"`. These identify a regular tmpfs placeholder, not a hardware framebuffer device. Decompiling the verified DTB shows `compatible = "arm,virt"` and no `framebuffer` or `simple-framebuffer` node. No guest mapping of `/dev/mem` to a framebuffer was observed, and the SimpleFB physical address/stride are unknown. There is no evidenced route in this flight by which a present could land in real framebuffer memory; an unobserved `/dev/mem` implementation cannot be ruled out from this UART alone.

## Step 3 — Fix decision

**FAIL — no fixable cause was established, so no image was built.** The Composer 3 VINTF miss does not block the observed HIDL 2.1 `default` service. The service log has no `executeCommands` or present dispatch to repair, and the target framebuffer has neither a DTB node nor known SimpleFB physical/stride values. Separately, the first-cycle startup failure is upstream of `system_server`: Zygote is still active, no system-server PID or crash trace is present, and the UART does not establish why the fork was not reached. A vendor service rebuild or speculative manifest/framebuffer edit would not address a diagnosed failing cause. No new fix initramfs or service ELF was built.

## Step 4 — QEMU flight

SKIPPED per Step 3: no fix image was built. The existing T683 flight was not repeated unchanged. No new host/guest elapsed time, QEMU exit code, UART log, or logcat capture is claimed.

## Step 5 — Present path, boot, service, and regression adjudication

- **Present path: FAIL.** The 50,043-line T683 UART contains no guest-origin successful `presentOrValidate`, `presentDisplay`, `setClientTarget`, `executeCommands`, or fb0 commit. The only decoded Composer request is `IBase::interfaceChain` (lines 46330, 46343–46346); the 76-byte binder exchange completes at lines 46393–46405. This does not prove a frame commit.
- **Boot bar: not re-tested in T684.** Step 3 built no fix, so there was no T684 flight. In the examined T683 cycle, line 49952 at uptime 1175.91s has blank `boot_completed=`, empty `system_server_pids=`, and missing user DE/CE; no first-cycle pass is claimed.
- **Service and linker evidence from T683:** The init service short name `vendor.hwc-2-1` is not printed by UART, but the guest process `android.hardware.graphics.composer@2.1-service` ran as PID 20236 and logged the binder exchange at lines 46319–46405. The T683 findings report that the device's `/system/bin/linker64` loaded the staged ELF; no Composer `CANNOT LINK EXECUTABLE` or linker-load failure appears in the UART. The T683 findings also report SELinux permissive mode. The allocator's fb0 access was still logged as an AVC denial at UART line 41186: `01-01 00:10:56.108 20216 20216 I auditd  : type=1400 audit(0.0:827): avc:  denied  { open } for  comm="android.hardwar" path="/dev/fb0" dev="tmpfs" ino=8659 scontext=u:r:hal_graphics_allocator_default:s0 tcontext=u:object_r:device:s0 tclass=file permissive=1`. No Composer-specific `/dev/mem` denial was found, but actual `/dev/mem` access was not proven.
- **T681 runtime risks:** The guest ELF ran under the guest's own linker, which establishes a successful load on this flight; the device API level was not independently read, so the `>= API 34` LIBC_R unwinder-tag requirement is not separately verified. A target-panel SimpleFB physical address and stride were not observed, and the supplied DTB has no framebuffer node. Vendor policy permission for `/dev/mem`, correct physical/stride values, and an actual hardware framebuffer mapping therefore remain unproven.
- **T664–T683 regression check:** T684 made no guest artifact, service, initramfs, toolchain, or linker-lineage changes, and ran no regression flight. The prior T683 evidence itself has an incomplete boot path (no `system_server`, CE, or `sys.boot_completed=1`); its ACL/CE gate was consequently unverified. The T683 findings report that media.c2 kept respawning while `IComponentStore/software` registrations appeared, and that the T681 ELF was used byte-for-byte. No fresh T684 pass is claimed for boot, ACL, media.c2, or the prior toolchain/link lineage.

## Lane result

FAIL. The diagnosis is complete, but no fixable cause was established. The exact observed display wall is the absent SurfaceFlinger-to-Composer execute/present dispatch; the concurrent first-cycle boot wall is the missing Zygote-to-`system_server` fork. No fix image, second flight, guest-origin present, or frame commit is claimed.
