# DEATH NOTE — T700

Lane: Track T700 — extended-window flight (Mac agyx, fly the T699 fix stack for 2400 s)
Status: CLOSED — FAIL (sys.boot_completed=1 observed and executed at guest 1679.6 s, system_server ran across 3 instances, services bootstrapped, Launcher3 resumed; however SurfaceFlinger present not dispatched and 0 frame writes to fb0 within 2400 s window)
UTC close: 2026-11-08T04:25:00Z
Worktree: /Users/Shared/wt-track700-extended-window
Branch: t700 (from T699 commit 446510d)

## What ended

Track T700 was opened to fly the T699 fix stack (Route B kernel `Image 3f45272f`, modified DTB `modified.dtb` `e83a6437`, and fix initramfs `t699-systemserver-fb0-fix.cpio.gz` `c9fe657d…`) to an extended 2400 s guest target window on Mac QEMU 11.1.2. No kernel rebuild, no DTB change, and no new fix were introduced; only the flight window was doubled from 1200 s to 2400 s.

1. **Major Lineage Breakthrough**:
   - `sys.boot_completed=1` was observed and executed by init for the first time in the project lineage: at guest uptime 1679.607510 s (line 74612), Android init processed the trigger `(sys.boot_completed=1)` from `/system/etc/init/hw/init.rc:1227`, executing post-boot cleanup (`/data/per_boot`), bootstat actions, and wifi tracing instances. A second wave occurred at guest 2050.763230 s (line 92063).
   - `system_server` started across three successive instances (PIDs 9301, 12708, 14487), bootstrapped core services (`ActivityManager`, `PowerManager`, `DisplayManagerService`, `WindowManagerService`), unlocked storage (`user_de=ok mirror_de=ok mirror_ce=ok`), bootstrapped User 0 (`onStartUser-0`), and launched `com.android.launcher3` (`QuickstepLauncher`), which reached `wm_set_resumed_activity` at line 108483.
   - Display power mode 2 (NORMAL) was commanded 4 times (lines 40822, 58128, 77723, 86856), and SurfaceFlinger declared `Boot is finished` twice (lines 69987 and 92481).

## Why the verdict is FAIL

Under the strict project Done criteria, Verdict PASS requires: (a) system_server started, AND (b) guest-origin frame commit observed (SurfaceFlinger present success / composer dispatch / guest fb0 writes), AND (c) sys.boot_completed=1.

While (a) and (c) were conclusively achieved:
1. **0 Frame Present Dispatches**: SurfaceFlinger never dispatched present or validate calls to Composer 2.1 (0 `presentDisplay`, 0 `validateDisplay`, 0 `presentOrValidate`, 0 Composer `executeCommands` writes to fb0, 0 guest-origin pixel writes to `/dev/fb0`).
2. **Watchdog-Induced Process Churn**: Under QEMU TCG emulation, heavy lock contention and binder transaction latency caused Android Watchdog threads to fire `Watchdog: *** GOODBYE!` (line 47263 for PID 9301, line 76976 for PID 12708), killing `system_server` via SIGKILL and restarting Zygote.
3. **OpenGLRenderer Swap Behavior Mismatch**: In the final instance (PID 14487), while Launcher3 was resumed (line 108483), `DisplayController` configured bounds 1024x768 (line 108493), and SwiftShader GLES was loaded (line 108494), `OpenGLRenderer` encountered `Unable to match the desired swap behavior` (line 108502), leaving the display pipeline before first frame commit to simplefb at the 2403.0 s cutoff.

## Final artifacts

- `findings-t700.md` — full flight findings with verbatim evidence citations, line numbers, and markers.
- `outcome-t700.txt` — single word `FAIL`.
- `DEATH_NOTE-T700.md` — this death certificate.
- `progress-t700.txt` — Steps 0–5 progress log.
- `heartbeat.json` — status and timestamp record.
- `GATES.md` — unlazy acceptance gates verification record.
- `TIMELINE.md` — updated project timeline entry.
- `scripts/supervisor_t700.py` — adapted supervisor script for 2400 s window.
- `flights/flight-01/` — Route B extended flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `flights/flight-ab/` — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).

## What is not claimed

- No frame commit claimed: SurfaceFlinger did not dispatch present or validate to Composer 2.1 within the 2400 s window, and zero pixels reached `/dev/fb0`.
- Snapshot byte differences were not claimed or used as frame commit proof (per T673 standing rule).
- TEN-PASSES was NOT run in this lane (one Route B flight to 2400 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane

Track T700 has established that the Android boot sequence can reach `sys.boot_completed=1` and start `QuickstepLauncher` on Route B kernel + T699 fix initramfs. The remaining barriers to achieving first frame commit are:
1. **Watchdog Throttling**: Increase Watchdog timeout thresholds or disable Watchdog kills under emulation to prevent `system_server` restarts during heavy background service startup.
2. **EGL / OpenGLRenderer Swap Behavior**: Investigate and fix `OpenGLRenderer: Unable to match the desired swap behavior` under SwiftShader / SurfaceFlinger EGL configs to enable buffer swap and queueBuffer latching to the display.
3. **Display Composition Pipeline**: Trace why SurfaceFlinger does not issue `validateDisplay` / `presentDisplay` once display power mode 2 is active and BootAnimation / Launcher3 windows are added.
