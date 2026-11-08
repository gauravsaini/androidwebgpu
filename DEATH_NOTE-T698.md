# DEATH NOTE — T698

Lane: Track T698 — system_server-fork / SurfaceFlinger-present diagnosis (Mac agyx, diagnosis-first, no rebuild)
Status: CLOSED — PASS (Both open questions fully resolved from preserved flight evidence; root causes isolated; exact next fixes named)
UTC close: 2026-11-08T01:15:00Z
Worktree: /Users/Shared/wt-track698-systemserver-present-diagnosis
Branch: t698 (from T697 commit b009e80)

## What ended

Track T698 was opened as a diagnosis-first investigation into the two Stage 5 display path barriers identified in T697 flight-01:
1. Why Zygote never forked `system_server`.
2. Why SurfaceFlinger never dispatched `present`.

The diagnosis is definitively closed from the preserved evidence (`evidence/t697-flight-01-uart.log`, SHA-256 `ae12556e…`, 51,301 lines; and `evidence/t697-flight-ab-uart.log`, SHA-256 `18151827…`, 13,371 lines). Both root causes were isolated with verbatim citations and line numbers:

1. **`system_server` Fork Mechanism**:
   - `system_server` is not managed by init (0 init lifecycle lines, 0 `init.rc` declarations); it is spawned by Zygote via `forkSystemServer()`.
   - Early in boot, `odrefresh` failed when `dex2oat64` exceeded its 120-second timeout under QEMU TCG software emulation on the Mac host (line 12807: `Compilation of services.jar failed ... child process timed out after 120000ms`; line 41405: `Compilation of service-wifi.jar failed ... child process timed out after 120000ms`).
   - Consequently, ART fell back to runtime verification of uncompiled dex bytecode.
   - Zygote (PID 20441) completed class preload at guest 671.7 s (line 47271), underwent a 75.3 s explicit GC pause (lines 47272, 48253), and from guest uptime ~744 s (line 48288) through the 1200 s cutoff (line 51296) was actively verifying methods across 21 APEX jars (272 methods logged, taking ~0.5–2.4 s each).
   - At the 1200 s flight cutoff, Zygote had reached jar 17 of 21 (`framework-adservices.jar`) and was still processing bytecode. There was zero crash, zero abort, and zero SELinux block. Zygote was healthy but throttled by emulation throughput.

2. **SurfaceFlinger Present Mechanism**:
   - SurfaceFlinger (PID 21261) connected to Composer 2.1 (PID 20484) at line 47664 (`createClient called`).
   - Line 47790 `getSupportedContentTypes failed ... Unsupported (8)` on primary display `4612927614820412928` is a benign capability query: Composer 2.1 does not support Composer 2.4 content types (`HWC2_ERROR_UNSUPPORTED = 8`). SurfaceFlinger logged this once and proceeded normally.
   - `bootanim` (PID 21327) logging `No animation file` (line 47794) is an expected benign AOSP fallback to the built-in shimmering "ANDROID" animation (line 47795).
   - SurfaceFlinger recorded 0 `validateDisplay`, 0 `presentOrValidate`, and 0 `presentDisplay` calls because the display was never powered on (`setPowerMode` = 0).
   - In Android architecture, display power state is controlled by `system_server`'s `DisplayManagerService`, which calls `SurfaceControl::setDisplayPowerMode(POWER_MODE_NORMAL)`. Because `system_server` never reached startup, the display remained in an uninitialized/unpowered state.
   - Furthermore, userspace gralloc mapped a dummy file on `tmpfs` created by init line 1133 (`toybox truncate -s 16M /dev/fb0`), rather than the hardware simplefb device node at physical MMIO `0x5f000000`.

## Final artifacts

- `findings-t698.md` — full diagnosis report with verbatim evidence citations, line numbers, and markers.
- `outcome-t698.txt` — single word `PASS`.
- `DEATH_NOTE-T698.md` — this death certificate.
- `progress-t698.txt` — Steps 0–4 progress record.
- `heartbeat.json` — status and timestamp record.
- `GATES.md` — unlazy acceptance gates verification record.
- `TIMELINE.md` — updated project timeline entry.

## What is not claimed

- No `sys.boot_completed=1`: not observed (0 / empty) in any flight.
- No new flight was run: `fly none` (both questions fully resolved from preserved flight evidence without contradiction).
- No rebuild was performed: all images, kernels, and ELFs remained untouched.
- No frame commit claimed: no guest-origin frame commit into fb0 occurred.
- Snapshot byte differences were not claimed or used as frame commit proof.
- TEN-PASSES was NOT run in this lane (diagnosis lane, fly none). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane (T699)

The path forward for Stage 5 frame commit requires addressing the two diagnosed barriers:
1. **For `system_server` fork**: Precompile ART bootclasspath and system server artifacts into the system image (`/data/misc/apexdata/com.android.art/dalvik-cache/arm64/`), or relax the `odrefresh` 120s timeout in init configuration, or disable runtime dex verification (`dalvik.vm.dexopt.disable=true`), allowing Zygote to complete bootclasspath initialization and reach `forkSystemServer()`.
2. **For SurfaceFlinger present**: Once `system_server` boots, `DisplayManagerService` will power on the display. In parallel, update `/vendor/etc/init/hw/init.virt.rc` to point gralloc to the real kernel `simple-framebuffer` device node (`mknod /dev/fb0 c 29 0`) at physical MMIO `0x5f000000`, replacing the init tmpfs truncation hack.
