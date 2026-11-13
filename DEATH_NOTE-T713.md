# DEATH NOTE: Track T713 — In-Process Sensor Start Bypass Failure Autopsy

## 1. Summary of Demise
- **Track**: T713
- **Verdict**: FAIL
- **Core Objective**: Implement the Option C bypass for in-process `startSensorServiceNative()` so that `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` does not hang on `FutureTask.get()`, preventing Watchdog kills and enabling first `queueBuffer` + `presentDisplay` within a 2400 s guest window.
- **Immediate Cause of Death**: The Option C bypass delivery mechanism (packaging `system_init.startsensorservice=0` in CPIO ramdisk property files) failed because first-stage Android init mounts `system.img` at `/system` and pivots root, shadowing all ramdisk property files. The property remained unset at runtime, defaulting `GetBoolProperty` to `true`. `startSensorServiceNative()` was invoked in-process and hung, causing Watchdog to kill `system_server` 10 consecutive times after 81–82 s each.

---

## 2. Re-Derivation of the Binary Gate (Verified vs Unverified)
In Track T712, agent analysis claimed that `libandroid_servers.so` gates native sensor service instantiation via a boolean property. In Track T713, we independently extracted and disassembled `libandroid_servers.so` (Build ID `5412435f791e84a20f92ff4802c114389fbff2ea`, SHA-256 `2ce96adb6f376cfd02517544062a78bfb020e9803f27f08b3dbefd194fc2810a`):

1. **Function Symbol**: `_ZN7android19NativeSensorServiceC1EP7_JNIEnvP8_jobject`
2. **Key Instructions**:
   - `0xaaeb0`: `mov w1, #0x1` (loads literal 1 = boolean true as default)
   - `0xaaeb4`: `adrp x0, 2ef000`
   - `0xaaeb8`: `add x0, x0, #0x77c` (points to string "system_init.startsensorservice")
   - `0xaaebc`: `bl android::base::GetBoolProperty` (offset `0x194470`)
   - `0xaaed8`: `mov w20, w0`
   - `0xaaee8`: `tbz w20, #0, aafec` (tests bit 0 of w20; branches to `0xaafec` if 0/false)
   - `0xaafec`: Function epilogue restoring registers `x20-x24`, `x27-x30`, `sp`
   - `0xab000`: `ret` (immediate return to Java)

The binary gate is 100% real and verified. If `system_init.startsensorservice` returns `false`, `NativeSensorService` immediately returns without launching the native initialization thread.

---

## 3. Autopsy of the Delivery Failure
Why did the property not evaluate to `false` in Flight-01?

1. **Ramdisk File Shadowing**:
   - We modified `system/etc/prop.default`, `system/build.prop`, `system/etc/build.prop`, `system/etc/ramdisk/build.prop`, and `build.prop` inside the CPIO archive `t713-fix.cpio.gz`.
   - In modern Android boot (system-as-root), the kernel unpacks the CPIO into ramfs, executes `early-ramdisk`, and calls `/init`.
   - `/init` (first-stage init) mounts `/dev/block/vda` (`system.img`) onto `/system` and executes `switch_root`.
   - The mount completely shadows all files under `/system` from the ramdisk.
   - Second-stage init reads `/system/etc/prop.default` and `/system/build.prop` directly from `system.img`. Since `system.img` was unpatched, the property was missing.

2. **Probe Timing in early-ramdisk**:
   - The backup command `/system/bin/setprop system_init.startsensorservice 0` in `early-ramdisk` executed at guest timestamp 7.85s.
   - At 7.85s, `/system` was not mounted and `property_service` was not running.
   - `/early-ramdisk[248]: /system/bin/getprop: inaccessible or not found` was emitted.
   - Line 430 in UART confirmed: `[T713-PROBE] early system_init.startsensorservice configured: ''`.
   - The command was never re-executed after `property_service` started.

---

## 4. Flight Evidence Summary
- **Flight-01 (Target 2400 s)**:
  - Duration: Host 2412.73 s, Guest 2411.08 s, Exit code: 0
  - Lines: 55,823, Log SHA-256: `dbd2108d4cda477a6d1c67ad80d4fd58f1fecd4e23a6c6be34204fc4cdac49cf`
  - SystemServer forks: 11 (PIDs 2576, 3241, 3883, 4521, 5155, 5794, 6439, 7077, 7711, 8354, 8984)
  - Watchdog kills: Exactly 10 (all on `FutureTask.get()` in `SensorService.onBootPhase(89)`)
  - `queueBuffer`: 0
  - `presentDisplay`: 0
  - `sys.boot_completed`: 0

- **Flight-ab (Canonical Control, 240 s Target)**:
  - Duration: Host 25.04 s, Guest 19.70 s, Exit code: 0
  - Lines: 574, Log SHA-256: `243f07249f1b1c730137d558befb20296336453d34b9e1922ba30bfd9cbf323e`
  - Result: Failed to find partition `vda` after 10003 ms timeout -> `InitFatalReboot: signal 6`. Proves `CONFIG_VIRTIO_BLK=y` is essential.

---

## 5. Guidance for Next Lane (Track T714)
To successfully clear boot phase 200, Track T714 must bypass the in-process native start using one of two proven vectors:

1. **Vector A (Direct Binary Patch)**:
   - Patch `libandroid_servers.so` at offset `0xaaee8` replacing `36000834` (`tbz w20, #0, aafec`) with `14000041` (`b 0xaafec`).
   - Repack `system.img` on the Linux box via loopback.
   - Guarantees immediate return from `NativeSensorService` regardless of property state.

2. **Vector B (Active Polling Setprop Daemon)**:
   - In `early-ramdisk`, launch a background loop that waits for `/dev/socket/property_service` and repeatedly executes `/system/bin/setprop system_init.startsensorservice 0` until verified by `getprop`.
