# Findings — Track T697: Frame commit on the Route B kernel (Mac agyx, flight only, no rebuild)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track697-framecommit-routeb-agyx`
Branch: `t697`, from `cb0ff77` (the T696 commit)
Predecessor: T696 driver-certified PASS (Route B rebuilt and flown).

## Verdict

Verdict: FAIL

## Required markers

sys.boot_completed: not observed (0 / empty) — no `sys.boot_completed=1` appeared in any flight in this lane. The 1200 s guest window was not expected to reach a completed Android boot. In flight `t697-flight-01`, all 82 periodic `T642_BOOT_STATE` lines (from guest uptime 143.41 s through 1189.07 s, guest max 1201.365 s) reported `boot_completed=` empty. In flight `t697-flight-ab`, `T642_BOOT_STATE` also reported `boot_completed=` empty. No cycle in this lane observed `sys.boot_completed=1`.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest kernel and display path are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `t697-flight-01` (Route B kernel `3f45272f…`): observed at guest 26.061646 s (`[   26.061646] Freeing unused kernel memory: 960K`).
- Flight `t697-flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 24.374130 s (`[   24.374130] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `t697-flight-01` (Route B kernel `3f45272f…`): observed at guest 29.070790 s (`[   29.070790] init: init first stage started!`).
- Flight `t697-flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 27.269416 s (`[   27.269416] init: init first stage started!`).

## First failing link

First failing link: `system_server` never forked from Zygote AND SurfaceFlinger never dispatched present calls to the Composer 2.1 service (0 `presentOrValidate` calls, 0 `presentDisplay` calls, 0 Composer `executeCommands` writes to `fb0`, 0 guest `fb0` writes).

While the Route B kernel provides working `fb0` simplefb registration (`[    0.337292] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!`) and boots cleanly into userspace (Zygote64 pid 20441, SurfaceFlinger pid 21261, bootanim pid 21327), SurfaceFlinger never dispatched present, and no guest-origin evidence of a frame commit into fb0 was observed across the full 1200 s guest flight window.

## Executive Summary

Track T697 re-flew the T683 fix initramfs (`base-t683-fix.cpio.gz`, carrying the replacement Composer 2.1 service ELF `ddc1e4dd…`) on the certified Route B kernel (`routeB-Image`, `3f45272f…`, 20,126,208 B) and modified DTB (`modified.dtb`, `e83a6437…`) to test whether the display path completes under the Route B kernel which carries `CONFIG_DEVMEM=y`, `CONFIG_FB=y`, `CONFIG_FB_SIMPLE=y`, and `CONFIG_DRM_FBDEV_EMULATION=y`.

1. **Gate Verification (Steps 0 & 1)**: Mac disk gate passed with 12 GiB available (>= 8 GiB threshold). All 10 staged inputs and markdown records were SHA-verified byte-for-byte; all guest-inputs were confirmed hardlinked. Reference records (`findings-t696.md`, `findings-t683.md`, `findings-t688.md`) were read in full.
2. **Supervisor Adaptation (Step 2)**: Adapted `scripts/supervisor_t696.py` into `scripts/supervisor_t697.py` with `TRACK=T697`, kernel `guest-inputs/routeB-Image`, drive id `t697system`, updated cmdline tag `androidboot.t697_kernel_sha256`, and GDB ports 1240/1241. No rebuild of kernel, initramfs, or binaries was performed.
3. **Flight on Route B Kernel (Step 3, `t697-flight-01`)**: Flown on Mac QEMU 11.1.2 for the full 1200 s guest target window (guest 1201.365 s, host 1202.85 s, exit code 0, 51,301 lines, UART SHA-256 `ae12556e2449bc558edb10da5540d8ae903dcaa6003db76f188363c3a31bac97`).
   - All T696 kernel regression markers held: `fb0: simplefb registered!` at guest 0.337292 s, `Freeing unused kernel memory: 960K` at guest 26.061646 s, `init first stage` at guest 29.070790 s, `init second stage` at guest 35.115832 s.
   - Zero hang, zero `udf #0x0`, zero Oops, zero panic.
   - Userspace graphics initialized: SurfaceFlinger (pid 21261) initialized with `SkiaGL Backend`, loaded SwiftShader (`/vendor/lib64/egl/libGLES_swiftshader.so`), connected to `SimpleFB CPU GLES Rasterizer`, and launched `bootanim` (pid 21327).
   - Display path failure: `HWComposer: getSupportedContentTypes failed for display ... Unsupported (8)`. SurfaceFlinger never issued a `presentOrValidate` or `presentDisplay` call; Composer 2.1 service received binder transactions but never dispatched a frame commit to fb0; no guest-origin fb0 write lines appeared.
   - Boot state: `system_server` never forked from Zygote (`system_server_pids=` empty across all 82 `T642_BOOT_STATE` watcher probes through guest uptime 1189.07 s); `sys.boot_completed` was not observed.
   - Frame commit: not observed (Verdict FAIL).
4. **A/B Control (Step 4, `t697-flight-ab`)**: Canonical kernel Image (`05c43917…`) booted cleanly on the same Mac QEMU 11.1.2 harness, reaching `Freeing unused kernel memory: 1024K` at guest 24.374130 s and `init first stage` at guest 27.269416 s, running to guest 242.40 s (host 245.53 s, exit code 0, 13,371 lines, UART SHA-256 `181518276f8676df52032b3c302dadfbf5221aaed8b62a2236bf7b59a269bf01`). The Mac QEMU harness is re-proven 100% sound.
5. **Adjudication**: Honest Verdict FAIL. The Route B kernel userspace base is rock solid, but Stage 5 frame commit is not achieved because `system_server` does not fork and SurfaceFlinger does not dispatch present calls to the Composer service.

---

## Detailed Step Records

### Step 0 — Mac Disk Gate: PASS

- `df -h /System/Volumes/Data` reading:
  `Filesystem Size Used Avail Capacity iused ifree %iused Mounted on`
  `/dev/disk3s5 228Gi 186Gi 12Gi 94% 3.2M 128M 2% /System/Volumes/Data`
- Available disk space: 12 GiB, strictly above the 8 GiB gate threshold.
- No WAIT-DISK. Recorded in `progress-t697.txt` at 2026-11-07T23:51:58Z.

### Step 1 — Input Verification & Reference Audit: PASS

All 10 staged inputs and markdown records were SHA-verified in the worktree:
- `guest-inputs/routeB-Image`: `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` (20,126,208 B, nlink=3)
- `guest-inputs/Image`: `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` (23,073,280 B, nlink=14)
- `guest-inputs/extracted_kernel.config`: `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` (133,733 B, nlink=5)
- `guest-inputs/modified.dtb`: `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` (1,920 B, nlink=6)
- `guest-inputs/base-t683-fix.cpio.gz`: `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70` (958,780,579 B, nlink=5)
- `guest-inputs/system.img`: `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` (2,059,816,960 B, nlink=13)
- `guest-inputs/android.hardware.graphics.composer@2.1-service`: `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` (19,941,040 B, nlink=5)
- `findings-t696.md`: `7184046210a2b439bf57c49305aa3558a119e4f1bdec1f8bb8beda58785c5102` (20,656 B)
- `findings-t683.md`: `fd06523c4f37e68b291afd729aa4e62934cef9d974f752cdb3532855da0e26b0` (9,842 B)
- `findings-t688.md`: `181a15807def6d9d2fd6f028b944342fc5578fc7043b5dcda9fc59bc599a1632` (23,120 B)

All files in `guest-inputs/` are confirmed hardlinks (`nlink > 1`) sharing inodes with `/Users/Shared/guest-inputs/`.
QEMU binary verified:
`/opt/homebrew/bin/qemu-system-aarch64` (31,498,864 B, SHA-256 `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869`).
Version string:
`QEMU emulator version 11.1.2`
`Copyright (c) 2003-2026 Fabrice Bellard and the QEMU Project developers`
`findings-t696.md`, `findings-t683.md`, and `findings-t688.md` read in full.
Recorded in `progress-t697.txt` at 2026-11-07T23:54:29Z.

### Step 2 — Flight Staging & Supervisor Adaptation: PASS

Adapted `scripts/supervisor_t696.py` into `scripts/supervisor_t697.py`.
Adaptations:
- `TRACK = "T697"`
- Kernel default: `guest-inputs/routeB-Image` (SHA-256 `3f45272f…`)
- Drive id: `t697system` (`-drive if=none,id=t697system,file={SYSTEM_IMG},format=raw,readonly=on -device virtio-blk-device,drive=t697system`)
- Append cmdline tag: `androidboot.t697_kernel_sha256={KERNEL_SHA}`
- Default flight name: `t697-flight-01`
- GDB port: 1240 for flight-01, 1241 for flight-ab
- Environment variable overrides prefixed with `T697_`
- `subprocess.Popen` explicitly given `stdin=subprocess.DEVNULL`, `stdout=subprocess.DEVNULL`, `stderr=stderr_f`
- Heartbeat step mapping: Step 3 for `t697-flight-01`, Step 4 for `t697-flight-ab`
- Preserved identical machine shape: `-machine virt,gic-version=2,virtio-mmio-transports=1 -cpu cortex-a57 -m 8192 -smp 4` with initramfs loader at `0x110000000`.
- No kernel rebuild, NO DTB/initramfs/system.img/ELF modification.
Recorded in `progress-t697.txt` at 2026-11-07T23:57:49Z.

### Step 3 — Flight on Route B Kernel (`t697-flight-01`): PASS (harness/kernel) / FAIL (frame commit)

Supervisor `scripts/supervisor_t697.py` executed flight `t697-flight-01`:
- Kernel: `guest-inputs/routeB-Image` (`3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3`)
- DTB: `guest-inputs/modified.dtb` (`e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389`)
- Initramfs: `guest-inputs/base-t683-fix.cpio.gz` (`8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70`)
- System: `guest-inputs/system.img` (`98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d`)
- Drive id: `t697system`
- GDB port: 1240
- Host elapsed: 1202.85 s
- Guest max time: 1201.365 s (target 1200 s reached cleanly)
- QEMU exit code: 0
- UART lines: 51,301 lines
- UART SHA-256: `ae12556e2449bc558edb10da5540d8ae903dcaa6003db76f188363c3a31bac97`
- Preserved at `flights/t697-flight-01/uart.log`, `flights/t697-flight-01/uart.log.numbered.txt`, and `flights/t697-flight-01/uart.log.gz`.

#### 1. Regression Markers Observed
1. `[    0.337292] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!` (guest 0.337292 s, UART line 97)
2. `[   26.061646] Freeing unused kernel memory: 960K` (guest 26.061646 s, UART line 186)
3. `[   29.070790] init: init first stage started!` (guest 29.070790 s, UART line 594)
4. `[   35.115832] init: init second stage started!` (guest 35.115832 s, UART line 721)
5. Zero hang at ~25 s, zero `udf #0x0`, zero Oops, zero panic. Specifically, zero 'Kernel panic - not syncing: Attempted to kill init!'.

#### 2. Display Path Evidence
1. `SurfaceFlinger` process started (pid 21261):
   - Line 44297: `[  683.165028] init: Control message: Processed ctl.start for 'bootanim' from pid: 21261 (/system/bin/surfaceflinger)`
   - Line 47461: `01-01 00:11:19.280 21261 21261 I SurfaceFlinger: Using HWComposer service: default`
   - Line 47462: `01-01 00:11:19.280 21261 21261 I SurfaceFlinger: SurfaceFlinger is starting`
   - Line 47464: `01-01 00:11:19.322 21261 21261 I SurfaceFlinger: Enabling backpressure for GPU composition`
   - Line 47465: `01-01 00:11:19.364 21261 21261 I SurfaceFlinger: SurfaceFlinger's main thread ready to run. Initializing graphics H/W...`
   - Line 47466: `01-01 00:11:19.373 21261 21261 D RenderEngine: RenderEngine with SkiaGL Backend`
   - Line 47468: `01-01 00:11:19.389 21261 21261 D libEGL  : loaded /vendor/lib64/egl/libGLES_swiftshader.so`
   - Line 47472: `01-01 00:11:19.615 21261 21261 I RenderEngine: vendor    : Android`
   - Line 47480: `01-01 00:11:19.855 21261 21261 I RenderEngine: vendor    : OpenAI`
   - Line 47481: `01-01 00:11:19.855 21261 21261 I RenderEngine: renderer  : SimpleFB CPU GLES Rasterizer`
   - Line 47482: `01-01 00:11:19.855 21261 21261 I RenderEngine: version   : OpenGL ES 3.0 CPU`
2. `BootAnimation` started (pid 21327):
   - Line 47786: `01-01 00:11:28.242 21327 21327 D BootAnimation: BootAnimationStartTiming start time: 688242ms`
   - Line 47790: `01-01 00:11:28.286 21261 21292 E HWComposer: getSupportedContentTypes: getSupportedContentTypes failed for display 4612927614820412928: Unsupported (8)`
   - Line 47794: `01-01 00:11:28.777 21327 21354 D BootAnimation: No animation file`
   - Line 47795: `01-01 00:11:28.777 21327 21354 D BootAnimation: BootAnimationShownTiming start time: 688777ms`
3. SurfaceFlinger present dispatch:
   - Search for `presentOrValidate`: 0 matches in `uart.log`.
   - Search for `presentDisplay`: 0 matches in `uart.log`.
   - Search for `executeCommands`: 0 matches in `uart.log`.
   - Search for `NoResources`: 0 matches in `uart.log`.
   - Present was NEVER dispatched by SurfaceFlinger.
4. Composer 2.1 service execution:
   - Process `android.hardware.graphics.composer@2.1-service` (pid 20484) executed and bound to binder interfaces (e.g. line 47664: `Data addr = android.hardware.graphics.composer@2.1::IComposerClient`).
   - However, no present commands or display commit commands were received or executed.
5. Guest fb0 write evidence:
   - Line 97: `[    0.337292] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!`
   - Line 1133: `[   40.786246] init: starting service 'exec 1 (/system/bin/toybox truncate -s 16M /dev/fb0)'...` (creates 16M file on tmpfs)
   - Line 42261: `avc: denied { read write } for comm="android.hardwar" name="fb0" dev="tmpfs" ino=8663 scontext=u:r:hal_graphics_allocator_default:s0` (gralloc access to tmpfs file)
   - Zero guest-origin writes to the hardware framebuffer device (`0x5f000000`) were observed.
   - Per standing adjudication (T673), snapshot byte differences are NOT frame-commit proof. No snapshot byte differences were claimed or used as evidence.
   - Frame commit: **NOT OBSERVED**.

#### 3. Userspace Boot & system_server State
- Zygote64 running (pid 20441).
- `system_server`: Did NOT fork. Across all 82 periodic `T642_BOOT_STATE` watcher reports (from uptime 143.41 s through 1189.07 s), `system_server_pids=` remained empty and `system_server_prop=` remained empty.
  - Final probe at line 51300 / 51301 (guest uptime 1189.07 s, guest max 1201.365 s):
    `[ 1201.365202] T642_BOOT_STATE user_de=missing mirror_de=missing mirror_ce=missing uptime=1189.07 system_server_prop= system_server_pids= zygote=running zypid=20441 sensors_hal=running netd=running netstats=ok wifi_apex=ok tethering_apex=ok boot_completed= ce_available= data_fs=unknown media_fs=unknown gatekeeperd=2525 gnss=2524 fingerprint=2526 swcodec=none`
- `sys.boot_completed`: **NOT OBSERVED** (`boot_completed=` empty).
Recorded in `progress-t697.txt` at 2026-11-08T00:22:23Z.

### Step 4 — A/B Control Flight on Canonical Image (`t697-flight-ab`): PASS

Supervisor `scripts/supervisor_t697.py` executed flight `t697-flight-ab`:
- Kernel: canonical `guest-inputs/Image` (`05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04`)
- DTB: `guest-inputs/modified.dtb` (`e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389`)
- Initramfs: `guest-inputs/base-t683-fix.cpio.gz` (`8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70`)
- System: `guest-inputs/system.img` (`98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d`)
- Drive id: `t697system`
- GDB port: 1241
- Host elapsed: 245.53 s
- Guest max time: 242.40 s (target 240 s reached cleanly)
- QEMU exit code: 0
- UART lines: 13,371 lines
- UART SHA-256: `181518276f8676df52032b3c302dadfbf5221aaed8b62a2236bf7b59a269bf01`
- Preserved at `flights/t697-flight-ab/uart.log`, `flights/t697-flight-ab/uart.log.numbered.txt`, and `flights/t697-flight-ab/uart.log.gz`.

Key markers observed:
1. `[   24.374130] Freeing unused kernel memory: 1024K` (UART line 186)
2. `[   27.269416] init: init first stage started!` (UART line 598)
3. `[   32.532178] init: init second stage started!` (UART line 718)
4. Zero panic, zero Oops, zero udf.
5. Harness is 100% sound.
Recorded in `progress-t697.txt` at 2026-11-08T00:27:18Z.

### Step 5 — Adjudication

- **Userspace markers**: All T696 userspace markers hold on the Route B kernel:
  - `fb0: simplefb registered!` at guest 0.337292 s
  - `Freeing unused kernel memory: 960K` at guest 26.061646 s
  - `init: init first stage started!` at guest 29.070790 s
  - `init: init second stage started!` at guest 35.115832 s
  - Zygote64 running (pid 20441)
  - SurfaceFlinger running (pid 21261)
  - bootanim running (pid 21327)
- **system_server**: Did NOT fork. Across 82 consecutive `T642_BOOT_STATE` watcher probes, `system_server_pids` and `system_server_prop` remained blank through uptime 1189.07 s.
- **SurfaceFlinger present**: Never dispatched. 0 `presentOrValidate` or `presentDisplay` lines appeared. 0 composer `executeCommands` writes to fb0 appeared.
- **Frame commit**: No guest-origin frame commit observed. First failing link: `system_server` never forks AND SurfaceFlinger never dispatches present.
- **sys.boot_completed**: Not observed (0 / empty).
- **Snapshot byte differences**: Not used as frame-commit proof.
- **TEN-PASSES**: NOT run in this lane (one Route B flight to 1200 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.
- **Regressions**: T664–T696 fixes all held. DTB, initramfs, system image, and Composer ELF were untouched and verified.

---

## Evidence Files

- Supervisor script: `scripts/supervisor_t697.py`
- Step heartbeat log: `progress-t697.txt`
- Heartbeat state: `heartbeat.json`
- Outcome file: `outcome-t697.txt`
- Death certificate: `DEATH_NOTE-T697.md`
- Flight 01 records (`flights/t697-flight-01/`):
  - `flight-end.json`
  - `uart.log` (51,301 lines, SHA-256 `ae12556e2449bc558edb10da5540d8ae903dcaa6003db76f188363c3a31bac97`)
  - `uart.log.numbered.txt`
  - `uart.log.gz`
  - `preflight.json`
  - `qemu-command.txt`
  - `qemu-stderr.log`
- Flight A/B control records (`flights/t697-flight-ab/`):
  - `flight-end.json`
  - `uart.log` (13,371 lines, SHA-256 `181518276f8676df52032b3c302dadfbf5221aaed8b62a2236bf7b59a269bf01`)
  - `uart.log.numbered.txt`
  - `uart.log.gz`
  - `preflight.json`
  - `qemu-command.txt`
  - `qemu-stderr.log`
