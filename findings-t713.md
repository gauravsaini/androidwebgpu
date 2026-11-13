Verdict: FAIL

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan

# Track T713 Findings: Option C Bypass for In-Process Native Sensor Start & 2400s Flight Verification

## Executive Summary

Track T713 investigated and attempted the Option C bypass for the in-process native sensor initialization hang identified in Track T712. In T712, `SensorsMultiHal` successfully loaded `/odm/etc/sensors/hals.conf` and registered `ISensors/default`, but `startSensorServiceNative()` hung inside `system_server`, causing `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` to block indefinitely on `mSensorStartDone.get()` (`FutureTask.get()`), resulting in 10 consecutive Watchdog kills after 81–82 s each.

In T713:
1. **Binary Re-derivation (V5 Certified)**: We independently extracted and disassembled `libandroid_servers.so` (Build ID `5412435f791e84a20f92ff4802c114389fbff2ea`, SHA-256 `2ce96adb…`) from `guest-inputs/system.img`. Disassembly of `_ZN7android19NativeSensorServiceC1EP7_JNIEnvP8_jobject` (starting at offset `0xaae04`) confirmed:
   - At `0xaaeb0`: `mov w1, #0x1` loads the default boolean value `true`.
   - At `0xaaed0`: `bl GetBoolProperty("system_init.startsensorservice", true)` (string at offset `0x2ef7c`).
   - At `0xaaed8`: `mov w20, w0` captures the boolean return value.
   - At `0xaaee8`: `tbz w20, #0, aafec` tests bit 0 of `w20`. If `0` (false), it branches directly to `0xaafec` (function epilogue), bypassing the native thread instantiation and executing `0xab010: ret` immediately!
2. **Delivery Mechanism Failure (Forensic Discovery)**: The Option C bypass property was packaged into `t713-fix.cpio.gz` across multiple early property files (`system/etc/prop.default`, `second_stage_resources/system/etc/ramdisk/build.prop`, `system/build.prop`, `system/etc/build.prop`, `system/etc/ramdisk/build.prop`, `build.prop`), and labeled in `plat_property_contexts`. However, during Android bootstrap, first-stage init mounts `/dev/block/vda` (`guest-inputs/system.img`) at `/system` and executes `switch_root /system /system`. Consequently, the property files inside the CPIO ramdisk under `/system` were completely shadowed by the ext4 filesystem of `system.img`. Furthermore, the probe command `/system/bin/setprop` in `early-ramdisk` executed at guest 7.85s, before `/system` was mounted and before Android's `property_service` daemon had started, logging `/early-ramdisk[248]: /system/bin/getprop: inaccessible or not found` (line 430: `[T713-PROBE] early system_init.startsensorservice configured: ''`).
3. **Flight-01 (Full 2400 s Guest Target)**: Flown on Mac QEMU 11.1.2 with `t705-Image`, `t713-modified.dtb`, `t713-fix.cpio.gz`, `system.img`, and `t712-vendor.img`. The flight executed for 2412.73 s host (2411.08 s guest, exit code 0, 55,823 lines, SHA-256 `dbd2108d4cda477a6d1c67ad80d4fd58f1fecd4e23a6c6be34204fc4cdac49cf`). Because `system_init.startsensorservice` remained unset, `GetBoolProperty` evaluated to `true`, and `startSensorServiceNative()` hung in native sensor initialization in every fork. Android Watchdog killed `system_server` exactly 10 times after 81–82 s each, precisely reproducing the T712 failure mode.
4. **Flight-ab (Canonical Control)**: Flown with canonical kernel `05c43917…` for 25.04 s host (19.70 s guest, exit code 0, 574 lines, SHA-256 `243f07249f1b1c730137d558befb20296336453d34b9e1922ba30bfd9cbf323e`). It timed out polling for `vda` after 10003 ms and aborted via `InitFatalReboot: signal 6`, verifying that `CONFIG_VIRTIO_BLK=y` in the patched kernel is the sole enabler of the external mount.

---

## 1. Disassembly & Binary Re-derivation of the Gating Mechanism

Per Driver Variance V5, we did not accept previous lane claims blindly and re-derived the gating mechanism directly from the binary.

### Binary Identification
- **File**: `libandroid_servers.so`
- **Location in Image**: `/system/lib64/libandroid_servers.so`
- **Size**: 6,864,184 bytes
- **SHA-256**: `2ce96adb6f376cfd02517544062a78bfb020e9803f27f08b3dbefd194fc2810a`
- **Target Function**: `_ZN7android19NativeSensorServiceC1EP7_JNIEnvP8_jobject` (`android::NativeSensorService::NativeSensorService(_JNIEnv*, _jobject*)`)

### Disassembly Analysis
Disassembly of `_ZN7android19NativeSensorServiceC1EP7_JNIEnvP8_jobject` from offset `0xaae04` to `0xab014`:

```text
00000000000aae04 <_ZN7android19NativeSensorServiceC1EP7_JNIEnvP8_jobject>:
   aae04: d10183ff  sub  sp, sp, #0x60
   aae08: a9027bfd  stp  x29, x30, [sp, #32]
   aae0c: 910083fd  add  x29, sp, #0x20
   aae10: a9036ffc  stp  x28, x27, [sp, #48]
   aae14: a90453f8  stp  x24, x20, [sp, #64]
   aae18: a9054bf6  stp  x22, x21, [sp, #80]
   ...
   aaeb0: 52800021  mov  w1, #0x1           // arg1: default_val = true (1)
   aaeb4: 90000280  adrp x0, 2ef000         // arg0: property name address page
   aaeb8: 911df000  add  x0, x0, #0x77c     // arg0: "system_init.startsensorservice" at 0x2ef77c
   aaebc: 9403a56d  bl   194470             // call android::base::GetBoolProperty
   aaed8: 2a0003f4  mov  w20, w0            // w20 = bool return value (0 or 1)
   aaedc: d10043bf  sub  sp, x29, #0x10
   ...
   aaee8: 36000834  tbz  w20, #0, aafec     // IF w20 bit 0 == 0 (FALSE): BRANCH to 0xaafec
   ...
   [0xaaef0 - 0xaafdc: native thread creation & startSensorServiceNative() invocation]
   ...
   aafec: a94453f8  ldp  x24, x20, [sp, #64] // Function Epilogue
   aaff0: a9454bf6  ldp  x22, x21, [sp, #80]
   aaff4: a9436ffc  ldp  x28, x27, [sp, #48]
   aaff8: a9427bfd  ldp  x29, x30, [sp, #32]
   aaffc: 910183ff  add  sp, sp, #0x60
   ab000: d65f03c0  ret                     // Return immediately to Java caller!
```

### Verification Findings
1. The property name checked is literally `system_init.startsensorservice`.
2. The default value is `true` (`mov w1, #0x1`).
3. If the property evaluates to `false` (`0`), `tbz w20, #0, aafec` skips all native sensor initialization and branches directly to the return epilogue.
4. If this bypass executes, `NativeSensorService` constructor completes without starting the background thread, `startSensorServiceNative()` returns immediately, and `SensorService.onBootPhase(200)` does not hang on `mSensorStartDone.get()`.

---

## 2. Root Cause Analysis: Why Option C Bypass Did Not Take Effect in Flight-01

In Step 2, we attempted to set `system_init.startsensorservice=0` via:
1. Appending `system_init.startsensorservice=0` to 6 property files in `t713-fix.cpio.gz` (`system/etc/prop.default`, `second_stage_resources/system/etc/ramdisk/build.prop`, `system/build.prop`, `system/etc/build.prop`, `system/etc/ramdisk/build.prop`, `build.prop`).
2. Mapping `system_init.startsensorservice u:object_r:system_prop:s0 exact bool` in `plat_property_contexts`.
3. Adding early probe and setprop calls in `early-ramdisk`.

### Why This Failed (The Shadowing Mechanism)
- **Mount Pivot**: In `early-ramdisk`, `/dev/block/vda` is attached to virtio-blk. Android first-stage init mounts `/dev/block/vda` (`system.img`) as the root filesystem `/system` and executes `switch_root` to `/system`.
- **Property File Shadowing**: All files in `t713-fix.cpio.gz` located under `/system` were shadowed and replaced by the actual ext4 filesystem on `system.img`. The unpatched `system.img` (which we could not modify per lane rules: "no new system.img") contains `/system/etc/prop.default` and `/system/build.prop` which lack `system_init.startsensorservice`.
- **Timing of early-ramdisk setprop**: In `early-ramdisk`, `/system/bin/setprop` was invoked at guest 7.85s before `/system` was mounted. The binary did not exist at that path, logging:
  - Line 427: `/early-ramdisk[248]: /system/bin/getprop: inaccessible or not found`
  - Line 430: `[T713-PROBE] early system_init.startsensorservice configured: ''`
- **Result**: `system_init.startsensorservice` was never set in Android's property area. `GetBoolProperty` defaulted to `1` (`true`), `startSensorServiceNative()` ran and hung in-process, blocking `SensorService.onBootPhase(200)` on `FutureTask.get()`.

---

## 3. Flight-01 Evidence & Forensic Verification

### Flight Parameters
- **Kernel**: `guest-inputs/t705-Image` (SHA-256 `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859`)
- **DTB**: `guest-inputs/t713-modified.dtb` (SHA-256 `d60bc98a67c8b8c014b65412d9af869a432a744f7f1eab24b78a290fdf396161`)
- **Initramfs**: `guest-inputs/t713-fix.cpio.gz` (SHA-256 `edba515eac53a2544ba33aa17cb3749c37a2f336207a3efcdfffb94e56ac3177`)
- **System**: `guest-inputs/system.img` (SHA-256 `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601`)
- **Vendor**: `guest-inputs/t712-vendor.img` (SHA-256 `095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf`)
- **Duration**: Host 2412.73 s, Guest 2411.08 s (Cutoff reached: `Guest target reached (2411.1s >= 2400s)`)
- **Log**: 55,823 lines, 6,861,207 bytes, SHA-256 `dbd2108d4cda477a6d1c67ad80d4fd58f1fecd4e23a6c6be34204fc4cdac49cf`

### Watchdog Kill Enumeration
Watchdog killed `system_server` 10 times after 81–82 s each, all on the identical stack trace:

| Kill # | PID | Kill Line | Time in Flight (s) | Cause / Stack Location |
|---|---|---|---|---|
| 1 | 2576 | 14222 | ~209s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 2 | 3241 | 18253 | ~429s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 3 | 3883 | 22284 | ~651s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 4 | 4521 | 26325 | ~871s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 5 | 5155 | 30072 | ~1087s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 6 | 5794 | 34599 | ~1305s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 7 | 6439 | 39176 | ~1530s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 8 | 7077 | 43784 | ~1747s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 9 | 7711 | 48003 | ~1962s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 10 | 8354 | 52558 | ~2186s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| Active | 8984 | N/A | ~2411s | Running at cutoff |

### Stack Trace Citation (Kill #10, UART L52560-L52573)
```text
11-10 23:54:26.469 W/Watchdog( 8354): main annotated stack trace:
11-10 23:54:26.469 W/Watchdog( 8354):     at jdk.internal.misc.Unsafe.park(Native Method)
11-10 23:54:26.469 W/Watchdog( 8354):     at java.util.concurrent.locks.LockSupport.park(LockSupport.java:211)
11-10 23:54:26.470 W/Watchdog( 8354):     at java.util.concurrent.FutureTask.awaitDone(FutureTask.java:447)
11-10 23:54:26.470 W/Watchdog( 8354):     at java.util.concurrent.FutureTask.get(FutureTask.java:190)
11-10 23:54:26.470 W/Watchdog( 8354):     at com.android.internal.util.ConcurrentUtils.waitForFutureNoInterrupt(ConcurrentUtils.java:86)
11-10 23:54:26.470 W/Watchdog( 8354):     at com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)
11-10 23:54:26.471 W/Watchdog( 8354):     at com.android.server.SystemServiceManager.startBootPhase(SystemServiceManager.java:291)
11-10 23:54:26.471 W/Watchdog( 8354):     at com.android.server.SystemServer.startOtherServices(SystemServer.java:1614)
11-10 23:54:26.471 W/Watchdog( 8354):     at com.android.server.SystemServer.run(SystemServer.java:945)
11-10 23:54:26.471 W/Watchdog( 8354):     at com.android.server.SystemServer.main(SystemServer.java:667)
11-10 23:54:26.472 W/Watchdog( 8354):     at java.lang.reflect.Method.invoke(Native Method)
11-10 23:54:26.472 W/Watchdog( 8354):     at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:552)
11-10 23:54:26.472 W/Watchdog( 8354):     at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:949)
11-10 23:54:26.472 W/Watchdog( 8354): *** GOODBYE!
```

---

## 4. Flight-ab Control Evidence

The canonical kernel A/B control flight was flown to verify test integrity:
- **Command**:
  ```bash
  T713_FLIGHT=flight-ab T713_KERNEL=guest-inputs/canonical-Image T713_KERNEL_SHA=05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 T713_GUEST_TARGET=240 T713_HOST_CAP=260 python3 scripts/supervisor_t713.py
  ```
- **Result**:
  - Host elapsed: 25.04 s, Guest log max: 19.70 s, QEMU exit code: 0
  - Log lines: 574, Log SHA-256: `243f07249f1b1c730137d558befb20296336453d34b9e1922ba30bfd9cbf323e`
  - Exact failure in UART (L562-L566):
    ```text
    [   19.583346] init: Wait for partitions returned after 10003ms
    [   19.584956] init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda
    [   19.592854] init: Failed to mount required partitions early ...
    [   19.605122] init: InitFatalReboot: signal 6
    ```
- **Significance**: Confirms that external `/system` mount capability is strictly enabled by `CONFIG_VIRTIO_BLK=y` in the patched kernel.

---

## 5. Comparative Metric Table (T712 vs T713)

| Metric | Track T712 (Option B) | Track T713 (Option C Attempt) | Status |
|---|---|---|---|
| /dev/block/vda mounted /system | Success (L520 @9.10s) | Success (L526 @9.10s) | PASS |
| simplefb registration | Success (L144 @0.30s) | Success (L144 @0.30s) | PASS |
| SensorsMultiHal load hals.conf | Success (0 failures) | Success (0 failures) | PASS |
| ISensors/default published | Yes (L9627) | Yes (L9702) | PASS |
| SubHal config load failure | 0 | 0 | PASS |
| SensorsMultiHal SIGABRT | 0 | 0 | PASS |
| dummy_sensorservice registered | Yes (PID 448) | Yes (PID 448) | PASS |
| sensorservice avc denials | 0 | 0 | PASS |
| SystemServer forks | 10 | 11 (10 killed, 1 active) | Identified Wall |
| Watchdog KILLING count | 10 | 10 | Identified Wall |
| Watchdog kill reason | `FutureTask.get()` in `SensorService.onBootPhase` | `FutureTask.get()` in `SensorService.onBootPhase` | Identified Wall |
| `startsensorservice` property value | unset (default true) | unset (default true, shadowed by system.img) | Root Cause |
| queueBuffer count | 0 | 0 | FAIL |
| presentDisplay count | 0 | 0 | FAIL |
| sys.boot_completed=1 | 0 | 0 | FAIL |
| Flight-01 Guest Duration | 2401.98 s | 2411.08 s | Completed |
| Flight-01 Log Lines | 51,719 | 55,823 | Preserved |

---

## 6. Artifact Registry & SHA-256 Hashes

All artifacts were verified byte-for-byte and re-hashed:

| Artifact | Size (bytes) | SHA-256 Checksum |
|---|---|---|
| `guest-inputs/t705-Image` | 20,126,208 | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` |
| `guest-inputs/canonical-Image` | 23,073,280 | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` |
| `guest-inputs/system.img` | 2,059,816,960 | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` |
| `guest-inputs/t712-vendor.img` | 134,217,728 | `095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf` |
| `guest-inputs/t713-fix.cpio.gz` | 53,502,851 | `edba515eac53a2544ba33aa17cb3749c37a2f336207a3efcdfffb94e56ac3177` |
| `guest-inputs/t713-modified.dtb` | 2,049 | `d60bc98a67c8b8c014b65412d9af869a432a744f7f1eab24b78a290fdf396161` |
| `flights/flight-01/uart.log` | 6,861,207 | `dbd2108d4cda477a6d1c67ad80d4fd58f1fecd4e23a6c6be34204fc4cdac49cf` |
| `flights/flight-ab/uart.log` | 47,846 | `243f07249f1b1c730137d558befb20296336453d34b9e1922ba30bfd9cbf323e` |
| `outcome-t713.txt` | 5 | `1004128f73ddc67b2cb6127da1d529a738bf54f59cb7a3f8983818e38d3ad48e` |

---

## 7. Next Lane Fix Recommendation

To unblock boot phase 200 and enable the first `queueBuffer` and `presentDisplay`, the next lane must ensure that the Option C bypass is delivered through a mechanism that actually reaches Android runtime property space or directly bypasses the branch in binary:

1. **Option C1 (Direct Binary Patch in `libandroid_servers.so`)**:
   - In `_ZN7android19NativeSensorServiceC1EP7_JNIEnvP8_jobject`, replace `0xaaee8: tbz w20, #0, aafec` (`0x36000834`) with an unconditional branch `b 0xaafec` (`0x14000041`).
   - This makes the bypass 100% immune to property resolution, mount shadowing, or init timing issues.
   - Requires repacking `system.img` on the Linux box via loopback (per T707/T708 procedure).
2. **Option C2 (Runtime Polling Daemon in `early-ramdisk`)**:
   - Instead of running `setprop` before `/system` mount, insert a background polling daemon inside `early-ramdisk` that waits for `/dev/socket/property_service` to exist and repeatedly calls `/system/bin/setprop system_init.startsensorservice 0` until confirmed via `getprop`.
   - Requires only a CPIO initramfs repack.
