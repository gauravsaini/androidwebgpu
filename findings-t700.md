# Findings — Track T700: extended-window flight (Mac agyx, fly the T699 fix stack for 2400 s)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track700-extended-window`
Branch: `t700`, from `446510d` (the T699 commit)
Predecessor: T699 driver-certified honest FAIL (both T698 barriers cured, system_server forked PID 9098, fb0 chr_file, display power mode 2 NORMAL; no present, no boot_completed in 1200 s cap).

## Verdict

Verdict: FAIL

## Required markers

sys.boot_completed: observed (1) — `sys.boot_completed=1` was observed and processed by init in flight `flight-01` (`t700-flight-01`). The first occurrence was processed at guest 1679.607510 s (line 74612: `[ 1679.607510] init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1227)`), triggering per-boot cleanup (line 74644), bootstat actions (line 75147), and wifi tracing instances (lines 75829, 75966). A second action wave occurred at guest 2050.763230 s (line 92063) after a system_server restart, triggering dmesgd (line 92203), persist device config (line 92205), and logd actions (line 92207). In control flight `flight-ab`, periodic `T642_BOOT_STATE` watcher lines reported `boot_completed=` empty throughout the 240 s window.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace boot and display paths are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 25.923093 s (line 186: `[   25.923093] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 26.235364 s (line 188: `[   26.235364] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 29.580946 s (line 596: `[   29.580946] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 30.008940 s (line 601: `[   30.008940] init: init first stage started!`).

system_server: Adjudicated from flight-01 UART evidence (lines 31156, 31221, 40822, 47263, 51965, 52027, 74612, 76976, 81151, 81198, 108483). Zygote successfully forked `system_server` across three successive instances:
1. Instance 1: Forked by Zygote (PID 8090) at guest 00:09:17.816 (line 31156: `01-01 00:09:17.816  8090  8090 I Zygote  : System server process 9301 has been created`). Entered at guest 00:09:19.920 (line 31221: `Entered the Android system server!`), bootstrapped core services (`ActivityManager`, `PowerManager`, `DisplayManagerService`, `WindowManagerService`), and unlocked all storage domains (`user_de=ok mirror_de=ok mirror_ce=ok` confirmed by `T642_BOOT_STATE` at guest 641.17 s). At guest 11-10 23:29:06.022 (line 47263), Android Watchdog thread 9368 timed out (`Watchdog: *** GOODBYE!`) and killed PID 9301 via SIGKILL.
2. Instance 2: Zygote restarted (PID 12180) and forked PID 12708 at 11-10 23:32:01.561 (line 51965), entering at 11-10 23:32:03.726 (line 52027). PID 12708 completed user storage unlocking, bootstrapped User 0 (`onStartUser-0`), and triggered `sys.boot_completed=1` at guest 1679.607510 s (line 74612). At 11-10 23:36:45.591 (line 76976), Watchdog thread 12770 timed out (`Watchdog: *** GOODBYE!`) and killed PID 12708 via SIGKILL.
3. Instance 3: Zygote restarted (PID 14043) and forked PID 14487 at 11-10 23:38:52.423 (line 81151), entering at 11-10 23:38:54.143 (line 81198). PID 14487 spawned and resumed Launcher3 (`com.android.launcher3/.uioverrides.QuickstepLauncher`, line 108483), remaining active through the 2400 s cutoff.

present: Adjudicated from flight-01 UART evidence (lines 97, 22593, 24575–24576, 40822, 58128, 69987, 77723, 86856, 92481, 108483, 108493, 108502). Userspace gralloc opened `/dev/fb0` as a character device node (lines 24575–24576: `tclass=chr_file`) pointing to physical simplefb at MMIO `0x5f000000` (line 97). Display power mode 2 (NORMAL) was commanded 4 times (lines 40822, 58128, 77723, 86856) and SurfaceFlinger declared `Boot is finished` twice (lines 69987 and 92481). However, SurfaceFlinger never dispatched present to Composer 2.1: 0 `validateDisplay` calls, 0 `presentDisplay` calls, 0 `presentOrValidate` calls, 0 Composer `executeCommands` writes to fb0, and 0 guest-origin pixel writes to `/dev/fb0`. At the 2403.0 s guest cutoff, `QuickstepLauncher` was resumed (line 108483), `DisplayController` configured window bounds for 1024x768 (line 108493), and SwiftShader GLES was loaded (line 108494), but `OpenGLRenderer` encountered `Unable to match the desired swap behavior` (line 108502), leaving the display pipeline without frame presentation.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.

Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

## First failing link

First failing link: Frame presentation was never dispatched to Composer 2.1 (0 `presentDisplay`, 0 `validateDisplay`, 0 `presentOrValidate`, 0 Composer `executeCommands` writes to fb0, 0 guest-origin fb0 frame writes). Although `sys.boot_completed=1` was successfully achieved and executed by init at guest 1679.6 s, the userspace graphics presentation pipeline stalled before first frame commit to simplefb due to Watchdog-induced `system_server` restarts (PID 9301 -> 12708 -> 14487) and OpenGLRenderer swap behavior mismatch (`Unable to match the desired swap behavior` at line 108502).

---

## Executive Summary

Track T700 executed the extended-window flight on Mac QEMU 11.1.2 to investigate whether doubling the flight window (1200 s -> 2400 s) would allow the T699 fix stack (Route B kernel + Fix A/B initramfs) to reach full boot completion and display frame commit. No kernel rebuild, no DTB change, and no new fix were introduced; only the guest window was extended.

### Key Discoveries & Milestones in T700

1. **`sys.boot_completed=1` Reached and Executed for the First Time in the Lineage**:
   - In all previous tracks (T680–T699), `sys.boot_completed=1` had never appeared.
   - In T700, at guest uptime 1679.607510 s (line 74612), Android init processed the trigger `(sys.boot_completed=1)` from `/system/etc/init/hw/init.rc:1227`, executing post-boot cleanup, bootstat actions, and wifi tracing instances.
   - A second action wave occurred at guest 2050.763230 s (line 92063) after a system_server respawn.
2. **`system_server` Bootstrapped, Unlocked Storage, and Spawned Launcher3**:
   - `odrefresh` compiled all artifacts cleanly and exited code 80 (`kCompilationSuccess`, line 22593) with zero timeouts.
   - Zygote forked `system_server` across three iterations (PIDs 9301, 12708, and 14487).
   - Storage domains were fully unlocked (`user_de=ok mirror_de=ok mirror_ce=ok` confirmed by `T642_BOOT_STATE` watcher lines from guest 641.17 s through flight end).
   - `system_server` successfully bootstrapped User 0 (`onStartUser-0`), initialized `WindowManagerShell`, and launched `com.android.launcher3` (QuickstepLauncher), which reached `wm_set_resumed_activity` at line 108483.
3. **Display Power Mode 2 Commanded and "Boot is finished" Logged by SurfaceFlinger**:
   - Gralloc bound to the hardware `/dev/fb0` character device node (`tclass=chr_file`, lines 24575–24576) at physical MMIO `0x5f000000`.
   - `DisplayManagerService` / `PowerManagerService` commanded SurfaceFlinger to set display power mode 2 (NORMAL) 4 times (lines 40822, 58128, 77723, 86856).
   - SurfaceFlinger logged `Boot is finished (1219684 ms)` at line 69987 and `Boot is finished (1616738 ms)` at line 92481.
4. **The New Barrier: Watchdog Restarts & OpenGLRenderer Swap Behavior Mismatch**:
   - Under QEMU TCG emulation on the Mac host, heavy background binder transactions and lock contention caused Android Watchdog threads to fire `Watchdog: *** GOODBYE!` (line 47263 for PID 9301, line 76976 for PID 12708), killing `system_server` and forcing Zygote restarts.
   - In the third instance (PID 14487), Launcher3 resumed (line 108483) and SwiftShader GLES was loaded (line 108494), but `OpenGLRenderer` encountered `Unable to match the desired swap behavior` (line 108502).
   - SurfaceFlinger never issued `validateDisplay`, `presentOrValidate`, or `presentDisplay` (all 0 calls). Composer 2.1 received only buffer cleanup commands (`BC_FREE_BUFFER`, 58 occurrences), with zero `executeCommands` display writes. Zero guest-origin pixels reached `/dev/fb0`.

---

## Detailed Step Records

### Step 0 — Mac Disk Gate: PASS

- Command: `df -h /System/Volumes/Data`
- Free space: 14 GiB available (>= 8 GiB threshold). No WAIT-DISK.
- Recorded in `progress-t700.txt` at 2026-11-08T03:17:39Z.

### Step 1 — Staged Input Verification: PASS

All staged inputs were verified byte-for-byte in the worktree by hardlink and SHA-256:

| Input File | SHA-256 Checksum | Size | Verification Status |
|:---|:---:|:---:|:---:|
| `guest-inputs/routeB-Image` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | 20,126,208 B | MATCH (PASS) |
| `guest-inputs/Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B | MATCH (PASS) |
| `guest-inputs/extracted_kernel.config` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | 133,733 B | MATCH (PASS) |
| `guest-inputs/modified.dtb` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B | MATCH (PASS) |
| `guest-image/t699-systemserver-fb0-fix.cpio.gz` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | 958,780,579 B | MATCH (PASS) |
| `guest-inputs/system.img` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B | MATCH (PASS) |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| `findings-t699.md` | `c84167c485ee13ffb7c00f0aafdcaa3d4953585f9bc53333b8b423cdafaf1926` | 17,329 B | MATCH (PASS) |
| `findings-t698.md` | `19b5638804075c9aa67a03ae33dc0329a85c1ede6e4d8536ad338df0e7e4d3aa` | 25,093 B | MATCH (PASS) |
| QEMU 11.1.2 binary | `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869` | 31,498,864 B | MATCH (PASS) |

All files in `guest-inputs/` and `guest-image/` were confirmed hardlinked (`nlink > 1`). Reference documents `findings-t699.md` and `findings-t698.md` were read and audited in full. Recorded in `progress-t700.txt` at 2026-11-08T03:19:04Z.

### Step 2 — Supervisor Adaptation to 2400 s Window: PASS

Adapted `scripts/supervisor_t699.py` into `scripts/supervisor_t700.py`:
- Configured `TARGET_GUEST_SECONDS = 2400` (configurable via `T700_GUEST_TARGET`).
- Configured `HOST_CAP_SECONDS = 2500` (configurable via `T700_HOST_CAP`).
- Renamed drive id: `t700system` (virtio-blk-device).
- Assigned unique GDB port: `1243` for flight-01, `1244` for flight-ab.
- Updated kernel command line androidboot tags to `T700` while retaining legacy tags.
- Verified syntax compilation and preflight verification clean.
- Recorded in `progress-t700.txt` at 2026-11-08T03:22:24Z.

### Step 3 — Flight on Route B Kernel (`flight-01`): PASS (harness) / FAIL (frame commit)

Supervisor `scripts/supervisor_t700.py` launched Mac QEMU 11.1.2 with Route B kernel (`routeB-Image`, SHA `3f45272f…`), modified DTB (`modified.dtb`, SHA `e83a6437…`), fixed initramfs (`t699-systemserver-fb0-fix.cpio.gz`, SHA `c9fe657d…`), and system image (`system.img`, SHA `98fc0b02…`).
- **Duration**: Ran full 2400 s guest target window (guest 2403.03 s, host 2404.55 s, QEMU PID 54762, exit code 0).
- **Log metrics**: 108,514 raw lines (109,048 numbered lines), 15,936,275 bytes, SHA-256 `7ef491cc1b6740393e6ba2f08319f53b5b7154ac73a138d437acb3eb2d37f6b5`.
- **Key Milestones Observed**:
  * Line 97: `[    0.348088] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!`
  * Line 186: `[   25.923093] Freeing unused kernel memory: 960K`
  * Line 596: `[   29.580946] init: init first stage started!`
  * Line 22593: `01-01 00:05:45.542  2539  2539 I odrefresh: odrefresh terminated by exit(80)` (0 timeouts)
  * Lines 24575–24576: `hal_graphics_allocator_default` (PID 8111) opened `/dev/fb0` with `tclass=chr_file` (character device node bound to simplefb)
  * Line 31156: `01-01 00:09:17.816  8090  8090 I Zygote  : System server process 9301 has been created`
  * Line 31221: `01-01 00:09:19.920  9301  9301 I SystemServer: Entered the Android system server!`
  * Line 40822: `11-10 23:27:13.715  8734  8734 D SurfaceFlinger: Setting power mode 2 on display 4612927614820412928`
  * Line 47263: `11-10 23:29:06.022  9301  9368 W Watchdog: *** GOODBYE!` (PID 9301 killed by Watchdog)
  * Line 51965: `11-10 23:32:01.561 12180 12180 I Zygote  : System server process 12708 has been created`
  * Line 52027: `11-10 23:32:03.726 12708 12708 I SystemServer: Entered the Android system server!`
  * Line 58128: `11-10 23:34:16.780  8734  8734 D SurfaceFlinger: Setting power mode 2 on display 4612927614820412928`
  * Line 69987: `11-10 23:35:58.780  8734  8767 I SurfaceFlinger: Boot is finished (1219684 ms)`
  * Line 74612: `[ 1679.607510] init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1227)`
  * Line 76976: `11-10 23:36:45.591 12708 12770 W Watchdog: *** GOODBYE!` (PID 12708 killed by Watchdog)
  * Line 77723: `11-10 23:36:57.749  8734  8734 D SurfaceFlinger: Setting power mode 2 on display 4612927614820412928`
  * Line 81151: `11-10 23:38:52.423 14043 14043 I Zygote  : System server process 14487 has been created`
  * Line 81198: `11-10 23:38:54.143 14487 14487 I SystemServer: Entered the Android system server!`
  * Line 86856: `11-10 23:41:04.813  8734  8734 D SurfaceFlinger: Setting power mode 2 on display 4612927614820412928`
  * Line 92063: `[ 2050.763230] init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1227)`
  * Line 92481: `11-10 23:42:35.834  8734  8766 I SurfaceFlinger: Boot is finished (1616738 ms)`
  * Line 108483: `11-10 23:47:58.299 14487 15499 I wm_set_resumed_activity: [0,com.android.launcher3/.uioverrides.QuickstepLauncher,minimalResumeActivityLocked - onActivityStateChanged]`
  * Line 108493: `11-10 23:47:58.791 17866 17866 I DisplayController: (CTOR) perDisplayBounds: {CachedDisplayInfo{size=Point(1024, 768), cutout=Rect(0, 0 - 0, 0), rotation=0}=...`
  * Line 108494: `11-10 23:47:58.796 17866 17894 D libEGL  : loaded /vendor/lib64/egl/libGLES_swiftshader.so`
  * Line 108502: `11-10 23:48:00.016 17866 17891 E OpenGLRenderer: Unable to match the desired swap behavior.`
- **SELinux Audit Count**: Exactly 20,781 `avc: denied` lines (20,742 permissive=1, 39 permissive=0).
- **Tombstones**: Exactly 2 tombstones written (`tombstone_00` at line 74326, `tombstone_01` at line 91196); 1,051 case-insensitive tombstoned mentions.
- Recorded in `progress-t700.txt` at 2026-11-08T04:11:08Z.

### Step 4 — A/B Control Flight (`flight-ab`): PASS

Flown on canonical kernel `guest-inputs/Image` (SHA `05c43917…`) on Mac QEMU 11.1.2 with identical hardware harness.
- **Duration**: Ran full 240 s guest target window (guest 241.11 s, host 245.60 s, QEMU PID 56316, exit code 0).
- **Log metrics**: 12,119 lines, 1,946,273 bytes, SHA-256 `8ac276e1a895fd0a6dc2cecf3b99414ea49911143313be7a2811f95929fed738`.
- **Milestones**:
  * Line 188: `[   26.235364] Freeing unused kernel memory: 1024K`
  * Line 601: `[   30.008940] init: init first stage started!`
  * Clean userspace boot initialization without panic or crash; `system_server_pids` empty, `boot_completed` empty.
- Recorded in `progress-t700.txt` at 2026-11-08T04:19:27Z.

---

## Regression Check Against T664–T699

| Check / Component | Expected State | Actual Observed State (T700) | Status |
|:---|:---|:---|:---:|
| Route B Kernel | Image `3f45272f…` (unchanged) | SHA-256 verified byte-for-byte; zero hang, zero `udf #0x0` | PASS |
| Modified DTB | `e83a6437…` (unchanged) | SHA-256 verified; simplefb node at `0x5f000000` registered | PASS |
| Canonical System Image | `system.img` `98fc0b02…` (unchanged) | SHA-256 verified; mounted cleanly as `t700system` | PASS |
| Composer 2.1 Service | ELF `ddc1e4dd…` (unchanged) | Runs without crash; handles binder transactions | PASS |
| simplefb Driver Registration | Registered at guest ~0.35 s | Observed at guest 0.348088 s (line 97, `fb0: simplefb registered!`) | PASS |
| Freeing Kernel Memory | Observed at guest ~26 s | Observed at guest 25.923093 s (line 186, 960K) | PASS |
| Init Stages | Stage 1 at ~29 s | Stage 1 at 29.580946 s (line 596) | PASS |
| `odrefresh` Execution | Clean compilation, no 120s timeout | Exited code 80 (`kCompilationSuccess`, line 22593) | PASS |
| Zygote Method Verification | Bypassed via `-Xverify:none` | Class preload finished and skipped runtime method verification | PASS |
| `system_server` Fork | Forks from Zygote, non-empty PID | Forked PIDs 9301, 12708, and 14487 | PASS |
| Storage Domains Unlock | `user_de`, `mirror_de`, `mirror_ce` | Confirmed `ok` by `T642_BOOT_STATE` from guest 641.17 s | PASS |
| Gralloc `/dev/fb0` Device Node | Character device `c 29 0` | Opened with `tclass=chr_file` (lines 24575–24576) | PASS |
| Display Power Mode | Power mode 2 (NORMAL) | Commanded 4 times (lines 40822, 58128, 77723, 86856) | PASS |
| `sys.boot_completed` | Property set to `1` | Observed at guest 1679.6 s (line 74612) and 2050.8 s (line 92063) | PASS |
| Launcher3 Start | QuickstepLauncher resumed | Resumed at guest 11-10 23:47:58.299 (line 108483) | PASS |
| SurfaceFlinger Present Dispatch | Present dispatched to Composer 2.1 | 0 `presentDisplay` / 0 `validateDisplay` within 2400 s cap | FAIL |
